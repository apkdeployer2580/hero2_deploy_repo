//! Headless probe runner for the landscape (full-screen) track.
//!
//! Boots the game on the host with no window, no audio and an arbitrary
//! framebuffer size, then dumps frames as raw RGBA + the full WIPI draw-call
//! trace. This is the measurement tool `LANDSCAPE_FEASIBILITY.md` asks for:
//! "정적 disasm보다 런타임 WIPI draw-call 트레이스가 훨씬 빠름".
//!
//! usage:
//!   wie_probe --zip <archive.zip> --w 176 --h 220 --frames 600 \
//!             --out <dump_dir> --dump 60,120,300,600 \
//!             --keys 60:OK,90:OK,150:DOWN,200:OK
//!
//!   RUST_LOG=wie_wipi_c=debug wie_probe ... 2> trace.log
//!
//! ⚠ That draw-call trace needs a build with default features OFF. The default
//! `quiet_release` feature compiles every `debug!`/`trace!` in the whole graph out,
//! so that the probe measures the same code the APK runs (and so its
//! instructions/second number is comparable to the device's):
//!     cargo build --release -p wie_probe --no-default-features
//!
//! `--keys` is `frame:KEY[,frame:KEY...]`; a key is pressed on that frame and
//! released 6 frames later. Dumps land in `<out>/frame_<n>_<w>x<h>.rgba`
//! (convert with tools_re/probe_png.py).

use std::{
    collections::{BTreeMap, HashMap},
    fs,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
};

use wie_backend::{
    AudioSink, Database, DatabaseRepository, Emulator, Event, Filesystem, GuestCallResult, Instant, JavaArg, JavaReceiver, KeyCode, Options,
    Platform, RecordId, Screen, canvas::Image, extract_zip,
};
use wie_ktf::KtfEmulator;
use wie_lgt::LgtEmulator;

// ------------------------------------------------------- host heap gauge ----

/// ★ HOST-HEAP ALLOCATION GAUGE (2026-09-19, roadmap #3).
///
/// simpleperf on the device (S25, 2026-09-18) put the ALLOCATOR at 33 % of the emulator
/// thread — twice the ARM interpreter. The phase profiler cannot see that (it only says
/// which phase the thread is in), and `--profile` samples the GUEST pc. So this counts
/// every host heap allocation and bins it by the phase byte the allocating thread has
/// published — `svc:java` allocations are the boundary crossings, `gc` the collector,
/// `engine` the interpreter's own. Deterministic (a count, not a sample), which is what
/// an A/B needs on a machine with ±15 % wall-clock noise.
///
/// Cost: one relaxed load+store per allocation, no atomic RMW — a lost increment under a
/// race with the sampler thread is one count in a million and does not matter for a
/// gauge. Printed only with `--phases 1`, where the timing is already a profile.
struct CountingAlloc;

static HALLOC_BY_PHASE: [AtomicU64; 16] = [const { AtomicU64::new(0) }; 16];
static HALLOC_BYTES: AtomicU64 = AtomicU64::new(0);

unsafe impl std::alloc::GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: std::alloc::Layout) -> *mut u8 {
        let p = wie_util::stats::PHASE.load(Ordering::Relaxed) as usize & 15;
        let c = &HALLOC_BY_PHASE[p];
        c.store(c.load(Ordering::Relaxed) + 1, Ordering::Relaxed);
        HALLOC_BYTES.store(HALLOC_BYTES.load(Ordering::Relaxed) + layout.size() as u64, Ordering::Relaxed);
        unsafe { std::alloc::System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: std::alloc::Layout) {
        unsafe { std::alloc::System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static HOST_ALLOC: CountingAlloc = CountingAlloc;

/// The per-phase allocation counts since the last call, and RESET (an interval gauge,
/// like `[phases]`), plus the bytes.
fn halloc_take() -> ([u64; 16], u64) {
    let mut out = [0u64; 16];
    for (i, c) in HALLOC_BY_PHASE.iter().enumerate() {
        out[i] = c.swap(0, Ordering::Relaxed);
    }
    (out, HALLOC_BYTES.swap(0, Ordering::Relaxed))
}

// ---------------------------------------------------------------- screen ----

/// Captures the last painted frame instead of showing it.
struct ProbeScreen {
    w: u32,
    h: u32,
    last: Arc<Mutex<Vec<u32>>>,
    /// ★★ 2026-10-15 HD: the effects the last paint recorded (HD mode), composited into the `--h2-hd` dumps.
    hd: Arc<Mutex<Option<wie_backend::canvas::HdFrame>>>,
    redraw: Arc<Mutex<bool>>,
    /// ★ 페이싱 게이지 (2026-09-08, 순서표 #8 착수 게이트).
    ///
    /// `--profile` 은 **게스트 PC 샘플러**라 호스트가 무엇을 하는지 원리적으로 못 본다.
    /// 그래서 "96.35% 가 경계" 라는 예전 수치는 "호스트가 96% 를 쓰고 있다" 는 뜻이지
    /// 어디에 쓰는지는 말해주지 않았다. 여기 두 개가 그 공백을 메운다:
    ///
    /// * `paints` — `Display::handlePaintEvent` 가 실제로 화면을 present 한 횟수.
    ///   가상 1초당 이 값이 게임의 자체 목표(6/10/14/18 fps)에 붙으면 **게임이 스스로
    ///   페이싱하고 있는 것**이고, 그러면 블리터 최적화는 fps 를 못 올린다.
    ///   틱마다(= 50/가상초) 붙으면 페이싱이 없고 호스트가 병목이다.
    /// * `paint_ns` — present 한 번의 호스트 비용(픽셀 변환). GC 는 이 바깥이라
    ///   `paint_ns` 가 작은데 프레임이 느리면 비용은 present 가 아닌 곳에 있다.
    paints: Arc<AtomicU64>,
    paint_ns: Arc<AtomicU64>,
}

impl Screen for ProbeScreen {
    fn request_redraw(&self) -> wie_util::Result<()> {
        *self.redraw.lock().unwrap() = true;
        Ok(())
    }

    fn paint(&self, image: &dyn Image) {
        let t0 = std::time::Instant::now();
        // ★★ 2026-10-15 HD: the record this paint made (None when HD mode is off)
        {
            let raw = image.raw();
            let hd = wie_backend::canvas::hd_finish(&raw, image.width(), image.height(), image.bytes_per_pixel());
            if HD_AUDIT.load(Ordering::Relaxed) {
                let n = HD_AUDIT_PAINTS.fetch_add(1, Ordering::Relaxed);
                let body: Vec<String> = hd
                    .as_ref()
                    .map(|h| h.draws.iter().filter(|d| d.effect == 146).map(|d| format!("{}({},{})g{}", d.age, d.x, d.y, d.group)).collect())
                    .unwrap_or_default();
                let draws = hd.as_ref().map_or(0, |h| h.draws.len());
                eprintln!("[h2-hd-paint] n={n} draws={draws} body=[{}]", body.join(" "));
            }
            *self.hd.lock().unwrap() = hd;
        }
        let data = image
            .colors()
            .iter()
            .map(|c| ((c.a as u32) << 24) | ((c.r as u32) << 16) | ((c.g as u32) << 8) | (c.b as u32))
            .collect::<Vec<_>>();
        *self.last.lock().unwrap() = data;
        self.paints.fetch_add(1, Ordering::SeqCst);
        self.paint_ns.fetch_add(t0.elapsed().as_nanos() as u64, Ordering::SeqCst);
    }

    fn width(&self) -> u32 {
        self.w
    }

    fn height(&self) -> u32 {
        self.h
    }
}

// ------------------------------------------------------------ filesystem ----

#[derive(Default)]
struct MemFs {
    files: Arc<Mutex<HashMap<String, Vec<u8>>>>,
}

/// The live filesystem, shared with `main` so `--fs-out` can write it after the run.
/// An `Arc` clone rather than a second copy: what gets dumped has to be what the guest
/// actually ended up with, including anything it wrote in the last frame.
static FS_LIVE: std::sync::OnceLock<Arc<Mutex<HashMap<String, Vec<u8>>>>> = std::sync::OnceLock::new();

impl MemFs {
    /// ★ The leading slash is NOT part of the identity (2026-09-19). hero2 reads its
    /// save as `/n1` and writes it back as `n1`; on the device `AndroidFilesystem::
    /// path_for` throws the `RootDir` component away, so both land on the same file
    /// and nobody ever noticed. A raw-string key here would make them two files, and
    /// the save A/B would then compare the host's write against a record the game
    /// never touched — the most convincing kind of wrong answer.
    fn key(aid: &str, path: &str) -> String {
        format!("{aid}:{}", path.trim_start_matches('/'))
    }

    /// Snapshot one file — used by the save A/B to compare the host's save against
    /// the game's own, byte for byte, without going anywhere near the disk.
    fn snapshot(&self, aid: &str, path: &str) -> Option<Vec<u8>> {
        self.files.lock().unwrap().get(&Self::key(aid, path)).cloned()
    }

    /// Preload from a real tree laid out exactly like the device's app-private
    /// directory — `<root>/<aid>/fs/<path>` — the same shape `AndroidFilesystem`
    /// writes (`wie_android/src/filesystem.rs`), so an `adb run-as` pull drops in
    /// with no rearranging.
    ///
    /// ★ The copy is READ-ONLY by construction, and that is the whole point: the
    /// bytes land in this `HashMap`, the guest's writes land in the same map, and
    /// nothing ever goes back to disk. `--db` needed a `--db-rw` switch because it
    /// hands the guest a real directory; this one cannot lose a save even if the
    /// guest erases every record it owns. Reproducing a device save headlessly is
    /// therefore free of the one risk that makes save work dangerous.
    fn preload(root: &Path) -> std::io::Result<Vec<(String, usize)>> {
        fn walk(dir: &Path, rel: &mut Vec<String>, out: &mut Vec<(PathBuf, Vec<String>)>) -> std::io::Result<()> {
            for entry in fs::read_dir(dir)? {
                let entry = entry?;
                let name = entry.file_name().to_string_lossy().into_owned();
                let ty = entry.file_type()?;
                rel.push(name);
                if ty.is_dir() {
                    walk(&entry.path(), rel, out)?;
                } else if ty.is_file() {
                    out.push((entry.path(), rel.clone()));
                }
                rel.pop();
            }
            Ok(())
        }

        let mut loaded = Vec::new();
        let mut files = HashMap::new();
        for entry in fs::read_dir(root)? {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }
            let aid = entry.file_name().to_string_lossy().into_owned();
            let fs_dir = entry.path().join("fs");
            if !fs_dir.is_dir() {
                continue;
            }
            let mut found = Vec::new();
            walk(&fs_dir, &mut Vec::new(), &mut found)?;
            for (disk, rel) in found {
                let data = fs::read(&disk)?;
                // The guest asks with a leading slash (`/n1`); `AndroidFilesystem`
                // strips it on the way out, so put it back on the way in.
                let path = format!("/{}", rel.join("/"));
                loaded.push((format!("{aid}:{path}"), data.len()));
                files.insert(Self::key(&aid, &path), data);
            }
        }
        PRELOADED.set(files).ok();
        Ok(loaded)
    }
}

/// Filled by [`MemFs::preload`] before the system is built, read once when
/// `ProbePlatform` is constructed. A `OnceLock` rather than a parameter because that
/// platform is built in one literal and threading an `Option` through it buys nothing.
static PRELOADED: std::sync::OnceLock<HashMap<String, Vec<u8>>> = std::sync::OnceLock::new();

#[async_trait::async_trait]
impl Filesystem for MemFs {
    async fn exists(&self, aid: &str, path: &str) -> bool {
        raid_serve(path).is_some() || self.files.lock().unwrap().contains_key(&Self::key(aid, path))
    }

    async fn size(&self, aid: &str, path: &str) -> Option<usize> {
        if let Some(v) = raid_serve(path) {
            return Some(v.len());
        }
        self.files.lock().unwrap().get(&Self::key(aid, path)).map(|v| v.len())
    }

    async fn read(&self, aid: &str, path: &str, offset: usize, count: usize, buf: &mut [u8]) -> Option<usize> {
        if let Some(v) = raid_serve(path) {
            if offset >= v.len() {
                return Some(0);
            }
            let n = count.min(v.len() - offset);
            buf[..n].copy_from_slice(&v[offset..offset + n]);
            return Some(n);
        }
        let files = self.files.lock().unwrap();
        let data = files.get(&Self::key(aid, path))?;
        if offset == 0 {
            raid_on_save_read(path, data); // ★ 2026-10-16: a /g read decides the temple's raid gate, as on the phone
        }
        if offset >= data.len() {
            return Some(0);
        }
        let n = count.min(data.len() - offset);
        buf[..n].copy_from_slice(&data[offset..offset + n]);
        Some(n)
    }

    async fn write(&self, aid: &str, path: &str, offset: usize, data: &[u8]) -> usize {
        let mut files = self.files.lock().unwrap();
        if let Some((savevault::Half::Prog, slot)) = savevault::classify(path) {
            costume_g_edge(slot, files.get(&Self::key(aid, path)).map(Vec::as_slice));
        }
        let f = files.entry(Self::key(aid, path)).or_default();
        if f.len() < offset + data.len() {
            f.resize(offset + data.len(), 0);
        }
        f[offset..offset + data.len()].copy_from_slice(data);
        // ★ Every guest write is announced. This is the ground truth a host-driven
        // save has to reproduce: the bytes the game itself puts in `/n<slot>`, and
        // WHEN it puts them there. It is the only way to tell "the host called the
        // right routine" from "the host called something that ran without error".
        //
        // ⛔ … but only for the files that ARE a save (2026-09-20, review). Unfiltered,
        // a guest that writes in its loop — an LGT record store, say — makes one
        // `format!` + one stderr write per call, from the emulator thread, inside this
        // lock, and `CountingAlloc` books those allocations against whatever phase is
        // running. That moves `[halloc] /tick` and the `[phases]` split, which are the
        // two numbers this repo's performance baseline is stated in.
        if savevault::classify(path).is_some() {
            eprintln!("[fs] write {aid}:{path} off={offset} len={} (file now {} B)", data.len(), f.len());
        }
        data.len()
    }

    async fn truncate(&self, aid: &str, path: &str, len: usize) {
        let mut files = self.files.lock().unwrap();
        if let Some((savevault::Half::Prog, slot)) = savevault::classify(path) {
            costume_g_edge(slot, files.get(&Self::key(aid, path)).map(Vec::as_slice));
        }
        files.entry(Self::key(aid, path)).or_default().resize(len, 0);
    }
}

// -------------------------------------------------------------- database ----

type Store = HashMap<(String, String), HashMap<RecordId, Vec<u8>>>;

#[derive(Default)]
struct MemDbRepo {
    store: Arc<Mutex<Store>>,
}

struct MemDb {
    store: Arc<Mutex<Store>>,
    key: (String, String),
}

#[async_trait::async_trait]
impl DatabaseRepository for MemDbRepo {
    async fn open(&self, name: &str, app_id: &str) -> Box<dyn Database> {
        let key = (app_id.to_string(), name.to_string());
        self.store.lock().unwrap().entry(key.clone()).or_default();
        Box::new(MemDb {
            store: self.store.clone(),
            key,
        })
    }

    async fn exists(&self, name: &str, app_id: &str) -> bool {
        self.store.lock().unwrap().contains_key(&(app_id.to_string(), name.to_string()))
    }

    async fn delete(&self, name: &str, app_id: &str) -> bool {
        self.store.lock().unwrap().remove(&(app_id.to_string(), name.to_string())).is_some()
    }
}

#[async_trait::async_trait]
impl Database for MemDb {
    async fn next_id(&self) -> RecordId {
        let store = self.store.lock().unwrap();
        store[&self.key].keys().max().map(|m| m + 1).unwrap_or(1)
    }

    async fn add(&mut self, data: &[u8]) -> RecordId {
        let mut store = self.store.lock().unwrap();
        let recs = store.get_mut(&self.key).unwrap();
        let id = recs.keys().max().map(|m| m + 1).unwrap_or(1);
        recs.insert(id, data.to_vec());
        id
    }

    async fn get(&self, id: RecordId) -> Option<Vec<u8>> {
        self.store.lock().unwrap()[&self.key].get(&id).cloned()
    }

    async fn set(&mut self, id: RecordId, data: &[u8]) -> bool {
        let mut store = self.store.lock().unwrap();
        store.get_mut(&self.key).unwrap().insert(id, data.to_vec());
        true
    }

    async fn delete(&mut self, id: RecordId) -> bool {
        let mut store = self.store.lock().unwrap();
        store.get_mut(&self.key).unwrap().remove(&id).is_some()
    }

    async fn get_record_ids(&self) -> Vec<RecordId> {
        let mut ids: Vec<RecordId> = self.store.lock().unwrap()[&self.key].keys().copied().collect();
        ids.sort();
        ids
    }
}

// ------------------------------------------------------ filesystem-backed DB ----

/// `<root>/<app_id>/db/<name>/<record_id>` — byte-identical layout to
/// `wie_android::database`, so a device backup can be dropped in as-is.
///
/// ⚠ READ-ONLY BY DEFAULT (2026-09-04). The guest drives every method here, so a
/// scripted `--keys` run is enough to make the GAME rewrite or erase whatever tree
/// `--db` points at — and the tree one actually points at is a save pulled off the
/// device. `--db-rw 1` is the only way to arm the writes, and refusals are logged so
/// a run that needed them cannot be mistaken for a run that worked.
struct FsDbRepo {
    root: PathBuf,
    writable: bool,
}

struct FsDb {
    dir: PathBuf,
    writable: bool,
}

impl FsDbRepo {
    fn dir_for(&self, name: &str, app_id: &str) -> PathBuf {
        self.root.join(app_id).join("db").join(name.trim_start_matches('/'))
    }
}

#[async_trait::async_trait]
impl DatabaseRepository for FsDbRepo {
    async fn open(&self, name: &str, app_id: &str) -> Box<dyn Database> {
        let dir = self.dir_for(name, app_id);
        eprintln!("[probe] DB open name={name:?} app_id={app_id:?} -> {dir:?} (exists={})", dir.is_dir());
        if self.writable {
            let _ = fs::create_dir_all(&dir);
        }
        Box::new(FsDb {
            dir,
            writable: self.writable,
        })
    }

    async fn exists(&self, name: &str, app_id: &str) -> bool {
        let e = self.dir_for(name, app_id).is_dir();
        eprintln!("[probe] DB exists name={name:?} app_id={app_id:?} -> {e}");
        e
    }

    async fn delete(&self, name: &str, app_id: &str) -> bool {
        let dir = self.dir_for(name, app_id);
        if !self.writable {
            eprintln!("[probe] DB delete REFUSED (read-only, pass --db-rw 1): {dir:?}");
            return false;
        }
        fs::remove_dir_all(dir).is_ok()
    }
}

impl FsDb {
    fn ids(&self) -> Vec<RecordId> {
        let mut v: Vec<RecordId> = fs::read_dir(&self.dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|e| e.file_name().to_string_lossy().parse().ok())
            .collect();
        v.sort();
        v
    }
}

#[async_trait::async_trait]
impl Database for FsDb {
    async fn next_id(&self) -> RecordId {
        self.ids().last().map(|m| m + 1).unwrap_or(1)
    }

    async fn add(&mut self, data: &[u8]) -> RecordId {
        let id = self.ids().last().map(|m| m + 1).unwrap_or(1);
        if !self.writable {
            eprintln!("[probe] DB add REFUSED (read-only, pass --db-rw 1): {:?} id={id} {} bytes", self.dir, data.len());
            return id;
        }
        let _ = fs::write(self.dir.join(id.to_string()), data);
        id
    }

    async fn get(&self, id: RecordId) -> Option<Vec<u8>> {
        fs::read(self.dir.join(id.to_string())).ok()
    }

    async fn set(&mut self, id: RecordId, data: &[u8]) -> bool {
        if !self.writable {
            eprintln!("[probe] DB set REFUSED (read-only, pass --db-rw 1): {:?} id={id} {} bytes", self.dir, data.len());
            return false;
        }
        fs::write(self.dir.join(id.to_string()), data).is_ok()
    }

    async fn delete(&mut self, id: RecordId) -> bool {
        if !self.writable {
            eprintln!("[probe] DB delete REFUSED (read-only, pass --db-rw 1): {:?} id={id}", self.dir);
            return false;
        }
        fs::remove_file(self.dir.join(id.to_string())).is_ok()
    }

    async fn get_record_ids(&self) -> Vec<RecordId> {
        self.ids()
    }
}

// -------------------------------------------------------------- platform ----

/// `--audio-log <path>` (2026-10-01, 배경음 A): every sink event, one line each, stamped with
/// the virtual clock — `<ms> on|off|pc|cc|pb|sysex|aso|wave|musvol …`. Opened once when the
/// command line is parsed; `audio_sink()` is called more than once and every sink it hands
/// out writes to this one file. Nothing reaches the guest from here, so the run it logs is
/// the run it would have been.
static AUDIO_LOG: std::sync::OnceLock<Mutex<fs::File>> = std::sync::OnceLock::new();
/// ★★ 2026-10-16 `--h2-hd-audit 1`: every paint prints its HD record's body draw (`[h2-hd-paint]`) — the 「악의 기본공격시
/// 잠깐잠깐 투명화」 hunt: a dumped frame shows only the LAST paint's record, a paint between dumps that lost the body did not.
static HD_AUDIT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static HD_AUDIT_PAINTS: AtomicU64 = AtomicU64::new(0);

/// `--audio-fm 1` (2026-10-01, 배경음 B3): the sink claims an FM synth, so hero2's songs take the
/// app's FM path (`AudioSink::fm_supported`) — `fmv`/`fm`/`fmoff` lines in the audio log.
static AUDIO_FM: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// `--audio-wav <path>` (implies `--audio-fm 1`): what the FM path plays, rendered on the
/// virtual clock by the app's own synth and written when the run ends — the game's own
/// sequence of songs, effects and hand-overs, audible without a phone.
static AUDIO_WAV: std::sync::OnceLock<(PathBuf, Mutex<FmCapture>)> = std::sync::OnceLock::new();

/// The app's FM synth — the file the audio thread renders with.
#[allow(dead_code)]
mod ma2_fm {
    include!("../../wie_android/src/ma2_fm.rs");
}

/// The app's effect upsampler — the file the audio thread converts a clip with.
#[allow(dead_code)]
mod sfx_upsample {
    include!("../../wie_android/src/sfx_upsample.rs");
}

/// ★ 2026-10-09 the app's voice clips (the awakenings) — the file the worker decodes and queues them with.
#[allow(dead_code)]
mod voice {
    include!("../../wie_android/src/voice.rs");

    #[cfg(test)]
    mod tests {
        use super::*;

        /// Every clip the app ships decodes: mono or stereo at a sane rate, and as long as the user's files are
        /// (0.56 · 2.06 · 3.49 · 1.70 s at 44.1 kHz mono — the Ogg headers, read separately).
        #[test]
        fn every_voice_clip_decodes() {
            let want = [("angry_start", 91_028usize), ("angry_ending", 24_552), ("night_sword", 153_954), ("flower", 74_970)];
            for (i, (name, frames)) in want.iter().enumerate() {
                assert_eq!(VOICE_FILES[i].0, *name);
                let raw = decode_ogg(VOICE_FILES[i].1).unwrap_or_else(|| panic!("{name} decodes"));
                let clip = voice_clip(i).unwrap_or_else(|| panic!("{name} decodes"));
                assert_eq!((clip.channels, clip.rate), (1, 44_100), "{name}");
                assert_eq!((raw.samples.len(), clip.samples.len()), (*frames, *frames), "{name}: the Ogg's last granule position");
                let (raw_active, active) = (active_db(&raw.samples, 1, 44_100), active_db(&clip.samples, 1, 44_100));
                let ((rp, rr), (peak, rms)) = (raw.level_db(), clip.level_db());
                eprintln!(
                    "[voice] {name}: {:.2} s · file: loudest 300 ms {raw_active:.1} dBFS, peak {rp:.1}, rms {rr:.1} → played: {active:.1} dBFS, peak {peak:.1}, rms {rms:.1} (gain {:+.1} dB)",
                    clip.seconds(),
                    VOICE_TARGET_DB - raw_active
                );
                // ★ 2026-10-09 the balance: every clip at the target, within what the soft knee takes off a raised one
                assert!((active - VOICE_TARGET_DB).abs() <= 1.0, "{name}: {active:.2} dB is the target {VOICE_TARGET_DB}");
                assert!(peak <= 20.0 * VOICE_CEILING.log10() + 0.01, "{name}: under the ceiling");
            }
            assert!(voice_clip(VOICE_FILES.len()).is_none());
            for v in crate::guest_build::H2Voice::ALL {
                assert_eq!(VOICE_FILES[v.index()].0, v.file(), "{v:?}: the cue names its own clip");
            }
        }

        /// ★★ 2026-10-24 the thunder (`tools_re/thunder_sfx.py`): 1.00 s mono at 44.1 kHz, brought to its own loudness —
        /// 4 dB under the voices — under the ceiling; the clip the cue names.
        #[test]
        fn the_thunder_clip_decodes_at_its_own_level() {
            let i = crate::guest_build::H2Voice::Thunder.index();
            assert_eq!(VOICE_FILES[i].0, "thunder");
            let clip = voice_clip(i).expect("the thunder decodes");
            assert_eq!((clip.channels, clip.rate, clip.samples.len()), (1, 44_100, 44_100));
            let active = active_db(&clip.samples, 1, 44_100);
            let (peak, _) = clip.level_db();
            eprintln!("[voice] thunder: loudest 300 ms {active:.1} dBFS, peak {peak:.1}");
            assert!((active - THUNDER_TARGET_DB).abs() <= 1.0, "{active:.2} dB is the thunder's {THUNDER_TARGET_DB}");
            assert!(peak <= 20.0 * VOICE_CEILING.log10() + 0.01);
            assert_eq!((clip_target_db(i), clip_target_db(0)), (THUNDER_TARGET_DB, VOICE_TARGET_DB));
        }

        /// The game's sound level as its effects play it here: off at ≤ 0 (`br.a(Clip,Z)V` plays nothing), else full
        /// (`Clip.setVolume` is a stub in this emulator).
        #[test]
        fn the_voices_play_like_the_games_effects() {
            assert_eq!((voice_gain(0), voice_gain(-3), voice_gain(1), voice_gain(3), voice_gain(10)), (0.0, 0.0, 1.0, 1.0, 1.0));
            assert!(voice_pcm(0, 0).is_none(), "silent at level 0 — the game plays no effect there either");
            for lv in [1i8, 3, 10] {
                let (c, r, d) = voice_pcm(1, lv).unwrap();
                assert_eq!((c, r), (1, 44_100));
                assert_eq!(d, voice_clip(1).unwrap().samples, "level {lv}: the clip as decoded");
            }
        }
    }
}

struct FmCapture {
    fm: ma2_fm::Ma2Fm,
    pcm: Vec<f32>,
    /// ★ 2026-10-02: the effects (`play_wave`), each converted the way the app's audio thread
    /// converts it and laid down where the virtual clock stood — summed with `pcm` at the end.
    sfx: Vec<f32>,
}

impl FmCapture {
    const RATE: u64 = 44_100;
    /// The app's effect-bus trim at its default level (`audio_mix::SFX_TRIM`).
    const SFX_TRIM: f32 = 0.4;

    /// One effect clip, starting at virtual time `ms`.
    fn wave(&mut self, ms: u64, channels: u8, rate: u32, data: &[i16]) {
        if channels == 0 || rate == 0 {
            return;
        }
        let clip = sfx_upsample::sfx_clip(data, channels, rate, Self::RATE as u32, Self::SFX_TRIM);
        let start = (ms * Self::RATE / 1000) as usize * 2;
        if self.sfx.len() < start + clip.len() {
            self.sfx.resize(start + clip.len(), 0.0);
        }
        for (o, s) in self.sfx[start..].iter_mut().zip(&clip) {
            *o += s;
        }
    }

    /// Render up to virtual time `ms` (the WAV starts at virtual 0).
    fn advance(&mut self, ms: u64) {
        let want = (ms * Self::RATE / 1000) as usize * 2;
        if want > self.pcm.len() {
            let start = self.pcm.len();
            self.pcm.resize(want, 0.0);
            self.fm.render(&mut self.pcm[start..]);
        }
    }
}

struct NoAudio {
    clock: Arc<AtomicU64>,
}

impl NoAudio {
    fn log(&self, line: std::fmt::Arguments) {
        if let Some(file) = AUDIO_LOG.get() {
            use std::io::Write;
            let t = self.clock.load(Ordering::SeqCst);
            if let Ok(mut file) = file.lock() {
                let _ = writeln!(file, "{t} {line}");
            }
        }
    }

    fn capture(&self, f: impl FnOnce(&mut ma2_fm::Ma2Fm)) {
        if let Some((_, cap)) = AUDIO_WAV.get() {
            if let Ok(mut cap) = cap.lock() {
                cap.advance(self.clock.load(Ordering::SeqCst));
                f(&mut cap.fm);
            }
        }
    }
}

impl AudioSink for NoAudio {
    fn fm_supported(&self) -> bool {
        AUDIO_FM.load(Ordering::Relaxed)
    }
    fn fm_voices(&self, part: u8, voices: &[wie_backend::Ma2Voice]) {
        self.log(format_args!("fmv {part} {}", voices.len()));
        let table = voices.iter().map(|v| (v.bank, v.program, ma2_fm::FmVoice::decode(&v.data))).collect();
        self.capture(|fm| fm.set_voices(part as usize, table));
    }
    /// The same mapping as `wie_android/src/audio_sink.rs` `Bus::apply_fm` (which cannot be
    /// included here: it lives beside oxisynth and rodio).
    fn fm_event(&self, part: u8, e: wie_backend::Ma2Event) {
        use wie_backend::Ma2Event as E;
        self.log(format_args!("fm {part} {e:?}"));
        let p = part as usize;
        self.capture(|fm| match e {
            E::Program { channel, bank, program } => fm.program(p, channel, bank, program),
            E::Volume { channel, value } => fm.volume(p, channel, value),
            E::Expression { channel, value } => fm.expression(p, channel, value),
            E::Pan { channel, value } => fm.pan(p, channel, value),
            E::PitchBend { channel, value } => fm.pitch_bend(p, channel, value),
            E::Modulation { channel, value } => fm.modulation(p, channel, value),
            E::NoteOn { channel, note } => fm.note_on(p, channel, note),
            E::NoteOff { channel, note } => fm.note_off(p, channel, note),
            E::End => {}
        });
    }
    fn fm_all_off(&self, part: u8) {
        self.log(format_args!("fmoff {part}"));
        self.capture(|fm| fm.all_off(part as usize));
    }
    fn play_wave(&self, c: u8, r: u32, d: &[i16]) {
        // ★ 2026-10-02: with the clip's mean and last sample — a clip decoded in the wrong
        // nibble order sits tens of thousands off centre and ends there (`atr.rs`).
        let mean = d.iter().map(|s| *s as i64).sum::<i64>() / d.len().max(1) as i64;
        self.log(format_args!("wave {c} {r} {} mean {mean} last {}", d.len(), d.last().copied().unwrap_or(0)));
        if let Some((_, cap)) = AUDIO_WAV.get() {
            if let Ok(mut cap) = cap.lock() {
                cap.wave(self.clock.load(Ordering::SeqCst), c, r, d);
            }
        }
    }
    fn midi_note_on(&self, c: u8, n: u8, v: u8) {
        self.log(format_args!("on {c} {n} {v}"))
    }
    fn midi_note_off(&self, c: u8, n: u8, _v: u8) {
        self.log(format_args!("off {c} {n}"))
    }
    fn midi_program_change(&self, c: u8, p: u8) {
        self.log(format_args!("pc {c} {p}"))
    }
    fn midi_control_change(&self, c: u8, ctl: u8, v: u8) {
        self.log(format_args!("cc {c} {ctl} {v}"))
    }
    fn midi_pitch_bend(&self, c: u8, v: u16) {
        self.log(format_args!("pb {c} {v}"))
    }
    fn midi_sysex(&self, d: &[u8]) {
        self.log(format_args!("sysex {}", d.len()))
    }
    fn midi_all_sound_off(&self, c: u8) {
        self.log(format_args!("aso {c}"))
    }
    /// `true` like the app's sink (a real music bus): velocities in the log are the score's.
    fn set_music_volume(&self, v: u8) -> bool {
        self.log(format_args!("musvol {v}"));
        true
    }
}

struct ProbePlatform {
    screen: ProbeScreen,
    fs: MemFs,
    db: Box<dyn DatabaseRepository + Send + Sync>,
    /// Virtual clock (ms). The guest arms 50 ms WIPI timers and then idles, so a
    /// wall-clock `now()` would never fire them in a headless run that ticks as
    /// fast as it can. Driving time from the frame counter also makes runs
    /// deterministic.
    clock: Arc<AtomicU64>,
}

impl Platform for ProbePlatform {
    fn screen(&self) -> &dyn Screen {
        &self.screen
    }

    fn now(&self) -> Instant {
        Instant::from_epoch_millis(self.clock.load(Ordering::SeqCst))
    }

    fn database_repository(&self) -> &dyn DatabaseRepository {
        self.db.as_ref()
    }

    fn filesystem(&self) -> &dyn Filesystem {
        &self.fs
    }

    fn audio_sink(&self) -> Box<dyn AudioSink> {
        Box::new(NoAudio { clock: self.clock.clone() })
    }

    fn write_stdout(&self, buf: &[u8]) {
        print!("{}", String::from_utf8_lossy(buf));
    }

    fn write_stderr(&self, buf: &[u8]) {
        eprint!("{}", String::from_utf8_lossy(buf));
    }

    fn exit(&self) {
        eprintln!("[probe] guest called exit()");
    }

    fn vibrate(&self, _duration_ms: u64, _intensity: u8) {}
}

// ------------------------------------------------------ shipping minimap ----
//
// `--map-check` diffs the app's OWN minimap renderer against the framebuffer the
// guest paints, so the source is included rather than mirrored: a check that
// exercises a copy proves nothing about what ships.
include!("../../wie_android/src/minimap_render.rs");

// ------------------------------------------------------- shipping anchors ----
//
// Same reasoning, for the per-build guest anchors (CHAR base, state offset, the
// handle-vs-direct pointer convention). The probe used to hard-code KTF's
// `0x1b3084` / `0x1b7b6c` / `+0x7c` in five places, so every introspection flag
// read unrelated memory on an LGT boot (MIGRATION_LGT.md §3.4).
// `Skt` is never constructed here — the probe only boots KTF/LGT archives — but the
// table is shared source, so the variant has to exist.
#[allow(dead_code)]
mod guest_build {
    include!("../../wie_android/src/guest_anchors.rs");
    // hero2's Java-side walks and write producers — the same code the app runs, so the
    // headless A/B below (`--h2-relabel` / `--h2-commit`) tests the shipping logic.
    include!("../../wie_android/src/hero2java.rs");
}
use guest_build::{AppState, CharAnchor, GuestBuild};

/// hero2's hero5-style minimap VIEW (2026-09-29) — the file the app's overlay draws with,
/// so `--h2-minimap` renders the identical panel headless.
#[allow(dead_code)]
mod minimap_view {
    include!("../../wie_android/src/minimap_view.rs");
}

/// hero2's STATUS HUD (2026-09-29) — the portrait ring · HP/SP bars · level plate and the
/// EXP bar, the file the app's overlay draws with, so `--h2-status` renders them headless.
#[allow(dead_code)]
mod hud_view {
    include!("../../wie_android/src/hud_view.rs");
}

/// The app's icon atlas (its `icons.bin` is `include_bytes!`d relative to that file) — for
/// `--h2-status`'s portrait head.
#[allow(dead_code)]
#[path = "../../wie_android/src/atlas.rs"]
mod atlas;

/// What `--h2-reset` plans. All three produce a [`guest_build::H2Reset`], so the flag's
/// body is one path; only the planner differs.
#[derive(Clone, Copy, PartialEq, Eq)]
enum H2ResetWhat {
    Stat,
    Skill,
    /// ★ 2026-09-23: put 「기본공격」 stage 1 back after an early skill reset zeroed it.
    Repair,
    /// ★★ 2026-10-16: 엘릭서 — HP · SP to their maxima (`h2_elixir_plan`; `refund` is 0, `cleared` the vitals that move).
    Elixir,
}

impl H2ResetWhat {
    fn name(self) -> &'static str {
        match self {
            Self::Stat => "stat",
            Self::Skill => "skill",
            Self::Repair => "repair",
            Self::Elixir => "elixir",
        }
    }

    fn plan(self, rd: &impl Fn(u32, usize) -> Vec<u8>, h2: &guest_build::Hero2Java) -> Option<guest_build::H2Reset> {
        match self {
            Self::Stat => guest_build::h2_stat_reset(rd, h2),
            Self::Skill => guest_build::h2_skill_reset(rd, h2),
            Self::Repair => guest_build::h2_combo_repair(rd, h2),
            Self::Elixir => guest_build::h2_elixir_plan(rd, h2),
        }
    }
}

// The app's REAL save-chain step machine (`Job`), not a copy — same contract as the
// other `include!`s above. `--host-save` drives it through `call_guest_function_out`,
// which is the device-free end-to-end of MIGRATION_LGT.md §4-3: the probe issues the
// same four calls the app's quick-save issues, and the game itself is the judge (the
// record must land in `--db` and load on the next boot).
mod host_save {
    include!("../../wie_android/src/save.rs");
}

/// The app's REAL save-vault policy, for the one question `--fs` asks of it: is this
/// guest path one half of a character save? Same `include!` contract again — the probe
/// must call a file a save on exactly the rule the device does.
mod savevault {
    include!("../../wie_android/src/savevault.rs");
}

/// ★ 2026-10-02: the app's REAL plate window — here for the parked cooldowns
/// (`Window::park` / `unpark`), which `--h2-cd-flip` ages by the same clock the app reads.
#[allow(dead_code)]
mod quickpage {
    include!("../../wie_android/src/quickpage.rs");
}

/// ★ 의상 아바타 A2 (2026-09-30): the app's REAL costume state and its save binding — the
/// book the worker runs (`AvatarBook`), driven here over the in-memory filesystem by
/// `--h2-costume-dir` · `--h2-costume-grant` · `--h2-costume-wear`.
#[allow(dead_code)]
mod avatar_state {
    include!("../../wie_android/src/avatar_state.rs");
}

// ★ A3b: the costume book's part list IS the override's (as the app's emulator asserts).
const _: () = {
    let (a, b) = (avatar_state::COSTUME_PARTS, guest_build::AvatarPatch::PARTS);
    let mut i = 0;
    while i < a.len() {
        assert!(a[i] == b[i], "avatar_state::COSTUME_PARTS must equal AvatarPatch::PARTS");
        i += 1;
    }
    // ★ A3c: the avatar slot of each part — the book's and the equipment window's.
    let (s, t) = (avatar_state::SLOT_OF_PART, guest_build::H2_AV_PART_SLOT);
    let mut j = 0;
    while j < s.len() {
        assert!(s[j] == t[j], "avatar_state::SLOT_OF_PART must equal hero2java::H2_AV_PART_SLOT");
        j += 1;
    }
    assert!(avatar_state::SLOTS == guest_build::H2_AV_SLOTS);
    // ★ A5: the quest slots the book marks are the ones the reader reads.
    assert!(avatar_state::QUESTS == guest_build::H2_QUESTS);
};

/// `--h2-costume-dir`: where the records live, and the journal state the filesystem's save
/// hook reads (the app keeps the same three things in `vault.rs`).
static COSTUME_DIR: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();

/// ★ 2026-10-16 보스 레이드: the app's `raid_gate` in miniature — the gate-bearing 신전 (map 112's bytes, the archive's
/// `P/m/m/112.m.gz`) is served for `/m/m/15.m.gz` while RAID_ON. `--h2-raid-gate 1|0` forces it (RAID_FORCED); otherwise
/// the first chunk of a `/g<slot>` read decides it from the costume book's committed record (`--h2-costume-dir`), exactly
/// as the phone does on its `/g` read (`avatar_state::raid_gate_for_save`).
static RAID_VARIANT: std::sync::OnceLock<Vec<u8>> = std::sync::OnceLock::new();
static RAID_ON: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static RAID_FORCED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn raid_serve(path: &str) -> Option<&'static [u8]> {
    if RAID_ON.load(std::sync::atomic::Ordering::SeqCst) && guest_build::h2_raid_gate_path(path) { RAID_VARIANT.get().map(Vec::as_slice) } else { None }
}

fn raid_on_save_read(path: &str, g: &[u8]) {
    if RAID_FORCED.load(std::sync::atomic::Ordering::SeqCst) {
        return;
    }
    if let Some((savevault::Half::Prog, slot)) = savevault::classify(path) {
        let on = COSTUME_DIR.get().is_some_and(|d| avatar_state::raid_gate_for_save(&avatar_state::AvatarFiles::new(d.clone()), slot, g));
        if RAID_ON.swap(on, std::sync::atomic::Ordering::SeqCst) != on {
            eprintln!("[h2-raid] /g{slot} read — the temple's raid gate {}", if on { "ON (a GUARDIAN HARD receipt bound to this save)" } else { "off" });
        }
    }
}
static COSTUME_WORKING: Mutex<Option<avatar_state::AvatarState>> = Mutex::new(None);
static COSTUME_ARMED: Mutex<[bool; 6]> = Mutex::new([true; 6]);
static COSTUME_G_EDGES: [std::sync::atomic::AtomicU64; 6] = [const { std::sync::atomic::AtomicU64::new(0) }; 6];

/// The `/g<slot>` edge, as `vault::before_overwrite` runs it: the journal first (while `/g`
/// is still the old save), then the edge count.
fn costume_g_edge(slot: u8, current: Option<&[u8]>) {
    let Some(dir) = COSTUME_DIR.get() else { return };
    let armed = {
        let mut a = COSTUME_ARMED.lock().unwrap();
        std::mem::replace(&mut a[usize::from(slot)], false)
    };
    if armed {
        if let Some(w) = COSTUME_WORKING.lock().unwrap().clone() {
            match avatar_state::write_journal(&avatar_state::AvatarFiles::new(dir.clone()), slot, current, &w) {
                Ok(base) => eprintln!("[h2-costume] slot {slot} journal written (base {base:016x}, wearing {:?})", w.worn_avatars()),
                Err(e) => eprintln!("[h2-costume] slot {slot} journal NOT written: {e}"),
            }
        }
    }
    COSTUME_G_EDGES[usize::from(slot)].fetch_add(1, std::sync::atomic::Ordering::Relaxed);
}

/// The probe's side of `avatar_state::AvatarBook`: `/g` from the in-memory filesystem.
struct ProbeAvatarIo {
    files: Arc<Mutex<HashMap<String, Vec<u8>>>>,
    aid: String,
}

/// One sprite the drawer cannot draw: `(sequence, frame, direction 0..=3, part, image frame, mirrored, why)`.
type PoseHole = (u32, u32, u32, u8, u8, bool, &'static str);

/// `--h2-pose-audit 1` (2026-10-10): every pose of the player's live class animation `cf.j:[B` (all
/// sequences, all frames, the four directions — `cf.a:[I` gives each sequence's offset) against the
/// parts table the game loaded `cf.a:[Object`, looked up exactly as `n.a([BIBB[Object;ZBBII)B`
/// does: `(p | 0x80, f)` = `table[p + 8][f]` (the mirrored copy the part loader makes only for the
/// frames the part's flip list names), `(p, f)` = `table[p][f]`. A null image is skipped by the
/// drawer — that body part is see-through in that pose (vC 98: every mirrored leg pose); an index
/// past the array throws. Head, body, arms and legs (0..=3) only — weapon/off-hand arrays are
/// legitimately null when the hand is empty, effects are not the character. `(poses, sprites, holes)`.
fn h2_pose_audit_run(rd: &impl Fn(u32, usize) -> Vec<u8>) -> Option<(u32, u32, Vec<PoseHole>)> {
    let (_, blob) = guest_build::h2_class_blob(rd)?;
    let (ob, ol) = guest_build::h2_array(rd, guest_build::h2_static_handle(rd, guest_build::Hero2Java::SEQ_OFFSETS_SLOT)?)?;
    let (tb, tl) = guest_build::h2_array(rd, guest_build::h2_static_handle(rd, guest_build::Hero2Java::PARTS_TABLE_SLOT)?)?;
    let (mut poses, mut sprites, mut holes) = (0u32, 0u32, Vec::new());
    for seq in 0..ol {
        let mut p = guest_build::h2_rd_u32(rd, ob + 4 * seq)? as usize;
        let frames = u32::from(*blob.get(p)?);
        p += 1;
        for frame in 0..frames {
            for dir in 0..4u32 {
                let n = usize::from(*blob.get(p)?);
                p += 1;
                poses += 1;
                for k in 0..n {
                    let s = blob.get(p + 4 * k..p + 4 * k + 4)?;
                    let (part, f, flip) = (s[0] & 0x7f, s[1], s[0] & 0x80 != 0);
                    if part > 3 || f == 255 {
                        continue;
                    }
                    sprites += 1;
                    let idx = u32::from(part) + if flip { guest_build::Hero2Java::PARTS_IMAGES_BASE } else { 0 };
                    let arr = (idx < tl).then(|| guest_build::h2_rd_u32(rd, tb + 4 * idx)).flatten().filter(|h| guest_build::h2_in_java_heap(*h));
                    let why = match arr.and_then(|a| guest_build::h2_array(rd, a)) {
                        None => Some("the part has no image array"),
                        Some((_, al)) if u32::from(f) >= al => Some("past the image array (the drawer throws)"),
                        Some((ab, _)) => (!guest_build::h2_rd_u32(rd, ab + 4 * u32::from(f)).is_some_and(guest_build::h2_in_java_heap))
                            .then_some(if flip { "null mirrored copy — not in the flip list (skipped: see-through)" } else { "null image (skipped: see-through)" }),
                    };
                    if let Some(why) = why {
                        holes.push((seq, frame, dir, part, f, flip, why));
                    }
                }
                p += 4 * n;
            }
        }
    }
    Some((poses, sprites, holes))
}

/// `--h2-equip-ui` (의상 아바타 A3 · ★ A3c): the character's items and what each avatar slot
/// wears — the app's `EquipAvatar::wardrobe`, over the probe's book.
fn probe_wardrobe(book: Option<&avatar_state::AvatarBook>) -> guest_build::H2Wardrobe {
    let Some(s) = book.and_then(|b| b.state()) else { return guest_build::H2Wardrobe::default() };
    guest_build::H2Wardrobe { owned: s.items.iter().map(|i| guest_build::H2Owned { id: i.id, avatar: i.avatar, slot: i.slot, mode: i.mode }).collect(), worn: s.wear }
}

/// `--h2-equip-ui`: the cursor switch follows the model — the app's `EquipAvatar::sync_hide`.
fn probe_equip_hide(emu: &mut dyn Emulator, block: u32, model: &guest_build::H2EquipAvatar, hidden: &mut bool, view: Option<&guest_build::H2EquipView>, f: u64) {
    let want = model.hides_cursor();
    if want == *hidden {
        return;
    }
    let (a, bytes) = guest_build::h2_equip_cursor_hide_write(block, want);
    let ok = emu.write_memory(a, &bytes);
    *hidden = want;
    if let Some(v) = view {
        for (a, b) in guest_build::h2_repaint_writes(&v.chain) {
            emu.write_memory(a, &b);
        }
    }
    eprintln!("[h2-equip-ui] f={f} the game's cursor frame {} (ok={ok})", if want { "OFF" } else { "on" });
}

impl avatar_state::AvatarIo for ProbeAvatarIo {
    fn live_g(&self, slot: u8) -> Option<Vec<u8>> {
        self.files.lock().unwrap().get(&MemFs::key(&self.aid, &format!("g{slot}"))).cloned()
    }
    fn files(&self) -> Option<avatar_state::AvatarFiles> {
        COSTUME_DIR.get().map(|d| avatar_state::AvatarFiles::new(d.clone()))
    }
    fn g_edges(&self, slot: u8) -> u64 {
        COSTUME_G_EDGES[usize::from(slot)].load(std::sync::atomic::Ordering::Relaxed)
    }
    fn publish(&self, working: Option<&avatar_state::AvatarState>) {
        *COSTUME_WORKING.lock().unwrap() = working.cloned();
        *COSTUME_ARMED.lock().unwrap() = [true; 6];
    }
    fn rearm(&self, slot: u8) {
        COSTUME_ARMED.lock().unwrap()[usize::from(slot)] = true;
    }
    fn note(&self, _warn: bool, line: String) {
        eprintln!("[h2-costume] {line}");
    }
}

/// CHAR base for the build that booted, or 0 when this build has no known anchor.
///
/// Deliberately WITHOUT `items::resolve_char`'s "is a character alive yet" content
/// test: that test is built from CHAR-record offsets that live in `items.rs`, and a
/// diagnostic tool wants the raw base even when the record reads dead — that is the
/// state one is usually debugging.
fn char_base(build: GuestBuild, read: &impl Fn(u32, usize) -> Vec<u8>) -> u32 {
    let u32at = |a: u32| -> u32 {
        let b = read(a, 4);
        if b.len() == 4 { u32::from_le_bytes([b[0], b[1], b[2], b[3]]) } else { 0 }
    };
    match build.char_anchor() {
        Some(CharAnchor::Pointer { global, fallback }) => {
            let p = u32at(global);
            if (0x10_0000..0x8000_0000).contains(&p) { p } else { fallback }
        }
        Some(CharAnchor::Static(base)) => base,
        // ★ hero2: two dereferences, and the base is the object's FIELD BLOCK
        // (`ptr_fields + 4`). Same "raw, no liveness test" contract as the arms above —
        // 0 when either hop is not in the guest heap, which is what a dead/booting
        // process looks like and is exactly what a diagnostic wants to see.
        Some(CharAnchor::JavaObject { slot }) => {
            let obj = u32at(slot);
            if !(0x4000_0000..0x5000_0000).contains(&obj) {
                return 0;
            }
            let fields = u32at(obj);
            if !(0x4000_0000..0x5000_0000).contains(&fields) {
                return 0;
            }
            fields + 4
        }
        None => 0,
    }
}

/// The map chain for this build, assembled the same way `items::map_chain` does.
fn map_chain_of(build: GuestBuild) -> MapChain {
    MapChain {
        data_off: build.container_data_off(),
        noblend_global: build.minimap_noblend_global(),
    }
}

/// Where this build keeps its statics — the regions to sweep when hunting for a
/// global that points at something. KTF is RWPI (everything sits around
/// `sl = 0x1b2c40`); LGT is absolute, with `.data` and `.bss` at fixed addresses and
/// the whole CHAR record living inside `.bss`.
fn statics_ranges(build: GuestBuild) -> &'static [(u32, usize)] {
    match build {
        GuestBuild::Ktf => &[(0x1b_0000, 0x1_8000)],
        _ => &[(0x140_0000, 0x604), (0x150_0000, 0x1_2958)],
    }
}

// ------------------------------------------------------ build fingerprint ----
//
// ★ WHY A HASH GATE EXISTS (2026-09-04, 순서표 #4).
//
// Every guest address in `guest_anchors.rs` was reverse engineered from ONE compile
// of ONE game, and `--poke` / `--host-save` / `--host-levelup` fire real guest WRITES
// and real guest CALLS at them. A wrong address does not error — it silently corrupts
// the character record or the save, which is the one failure mode this repo cannot
// afford (CLAUDE.md, 「게스트 앵커 게이트」).
//
// The test the probe used to key on cannot tell those compiles apart:
// `KtfEmulator::loadable_archive` only asks "does the archive have an `__adf__`",
// which hero2 KTF, hero3 KTF and hero4 KTF all answer yes to. So the old
// `let build = if is_ktf { GuestBuild::Ktf } else { GuestBuild::Lgt }` handed HERO3's
// addresses to whatever KTF archive was passed in — hero2 included.
//
// The fix is to identify the BUILD, not the carrier, and to let the table below — not
// the archive shape — decide whether anchors exist at all. `assets/*.zip` is a repack
// whose own md5 moves every time `make_hero2_asset.py` runs; the guest binary inside
// the jar is byte-stable, so that is what gets hashed.

/// One known compile. `anchors` is `Some` only when guest addresses in this repo were
/// actually measured on this binary — that is the flag that opens the write gate.
struct KnownBuild {
    /// Full md5 of the guest binary, or a PREFIX when the archive is not on this disk
    /// and only the truncated hash from `guest_anchors.rs` is available.
    md5: &'static str,
    anchors: Option<GuestBuild>,
    name: &'static str,
}

/// Measured 2026-09-04 by hashing every `client.bin*` / `binary.mod` reachable from
/// the archives on this machine, plus the two truncated hashes `guest_anchors.rs`
/// carries for builds that are not here.
const KNOWN_BUILDS: &[KnownBuild] = &[
    KnownBuild {
        // dist/영웅서기2-1.zip and the asset built from it. 614,356 B.
        md5: "08a53cc43e57a1173df5d03121fa32ed",
        // ★★ 2026-09-09: hero2 now has its OWN anchors (`HERO2_GUEST_RE.md`), so the
        // read-only decoders are open. The WRITE/CALL flags stay refused below — hero2's
        // write side is genuinely unmeasured, and that is a different question from
        // whether the reads are valid. 순서표 #11/#12 was the item that closed this.
        anchors: Some(GuestBuild::Hero2Ktf),
        name: "hero2 KTF 1.0.7 (client.bin1120)",
    },
    KnownBuild {
        // hero3's own shipping asset: hero3_fixed.zip is an LGT archive, not KTF.
        // This completes the truncated `6a352944…` in guest_anchors.rs:29. 519,952 B.
        md5: "6a352944bb43c0789ca9f4b6f0702f4b",
        anchors: Some(GuestBuild::Lgt),
        name: "hero3 LGT 1.0.7 (binary.mod)",
    },
    KnownBuild {
        // guest_anchors.rs:29, prefix only — no 01.00.04 archive on this disk.
        md5: "e76fafdd",
        anchors: Some(GuestBuild::Lgt),
        name: "hero3 LGT 1.0.4 (binary.mod)",
    },
    KnownBuild {
        // guest_anchors.rs:26, prefix only. ⚠ Every KTF address in `wie_android` was
        // reversed from THIS binary and it is not on this machine any more — the old
        // `--zip` default `d:/tmp/hero3_fixed.zip` is gone too (verified absent
        // 2026-09-04), which is a second reason `--zip` is now required.
        md5: "3792a476",
        anchors: Some(GuestBuild::Ktf),
        name: "hero3 KTF 1.0.3 (client.bin64000)",
    },
    KnownBuild {
        md5: "7f7571af0abcb0eb87971ca6da6ef0f7",
        anchors: None,
        name: "hero4 KTF (client.bin387872)",
    },
    KnownBuild {
        md5: "5ee2b660cf3f76c13f5d98712ca7499a",
        anchors: None,
        name: "wie test_data helloworld KTF",
    },
    KnownBuild {
        md5: "ba5bf6039d64d818f44473dc1d8bbdb2",
        anchors: None,
        name: "wie test_data helloworld LGT",
    },
];

/// What the hash said. `anchors == None` is the closed gate: either the build is not
/// in the table at all, or it is in the table as a build nothing was measured on.
struct Fingerprint {
    /// `<jar>!<member>` so the log line says where the hash came from.
    member: String,
    md5: String,
    anchors: Option<GuestBuild>,
    name: String,
}

fn md5_hex(data: &[u8]) -> String {
    md5::compute(data).0.iter().map(|b| format!("{b:02x}")).collect()
}

/// The guest binary inside the archive — the only byte-stable identity an archive has.
/// Looks at the top level first (a hand-unpacked tree), then inside every `.jar`.
fn guest_binary(files: &BTreeMap<String, Vec<u8>>) -> Option<(String, Vec<u8>)> {
    let is_guest = |n: &str| n.starts_with("client.bin") || n == "binary.mod";
    for (name, data) in files {
        if is_guest(name) {
            return Some((name.clone(), data.clone()));
        }
    }
    for (name, data) in files {
        if !name.to_ascii_lowercase().ends_with(".jar") {
            continue;
        }
        let Ok(inner) = extract_zip(data) else { continue };
        for (member, d) in &inner {
            if is_guest(member) {
                return Some((format!("{name}!{member}"), d.clone()));
            }
        }
    }
    None
}

fn fingerprint(files: &BTreeMap<String, Vec<u8>>) -> Fingerprint {
    let Some((member, data)) = guest_binary(files) else {
        return Fingerprint {
            member: "<none>".to_string(),
            md5: "-".to_string(),
            anchors: None,
            name: "UNIDENTIFIED (no client.bin*/binary.mod in the archive)".to_string(),
        };
    };
    let md5 = md5_hex(&data);
    match KNOWN_BUILDS.iter().find(|k| md5.starts_with(k.md5)) {
        Some(k) => Fingerprint {
            member,
            md5,
            anchors: k.anchors,
            name: k.name.to_string(),
        },
        None => Fingerprint {
            member,
            md5,
            anchors: None,
            name: "UNKNOWN BUILD (not in KNOWN_BUILDS)".to_string(),
        },
    }
}

// ------------------------------------------------------- stack high-water ----
//
// ★ The one gauge that cannot miss the leak (2026-08-06). The device crash is a
// native stack overflow on `wie-emu` whose backtrace is 500+ frames of two functions
// alternating, and every in-process counter we tried measures the WRONG layer:
// `ArmCoreThreadWrapper` nesting is 1 by construction, `run_function` nesting tops
// out at 21, and the executor's task count stays under 8. So measure the thing that
// actually overflows — the stack itself — from outside the code that uses it.
//
// Windows commits stack pages lazily and NEVER decommits them, so the still-RESERVED
// region at the base of the stack allocation only ever shrinks, and its size is an
// exact high-water mark for the whole run. A healthy emulator settles; a leaking one
// eats the reserve at a steady rate, and the rate says how long the device had.
#[cfg(windows)]
mod stack_gauge {
    use core::ffi::c_void;

    #[repr(C)]
    #[derive(Default, Clone, Copy)]
    struct MemoryBasicInformation {
        base_address: *mut c_void,
        allocation_base: *mut c_void,
        allocation_protect: u32,
        partition_id: u16,
        _pad0: u16,
        region_size: usize,
        state: u32,
        protect: u32,
        type_: u32,
        _pad1: u32,
    }

    impl MemoryBasicInformation {
        fn zeroed() -> Self {
            unsafe { core::mem::zeroed() }
        }
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn VirtualQuery(address: *const c_void, buffer: *mut MemoryBasicInformation, length: usize) -> usize;
    }

    /// Bytes of this thread's stack still RESERVED (never touched). Monotonically
    /// non-increasing; `initial - current` is the deepest the stack has ever gone.
    pub fn reserved_bytes() -> Option<usize> {
        let probe = 0u8;
        let size = size_of::<MemoryBasicInformation>();
        let mut here = MemoryBasicInformation::zeroed();
        if unsafe { VirtualQuery(&probe as *const u8 as *const c_void, &mut here, size) } == 0 {
            return None;
        }
        let mut base = MemoryBasicInformation::zeroed();
        if unsafe { VirtualQuery(here.allocation_base as *const c_void, &mut base, size) } == 0 {
            return None;
        }
        // MEM_RESERVE = 0x2000. Anything else means the base region is already
        // committed, i.e. the whole stack has been touched.
        (base.state == 0x2000).then_some(base.region_size)
    }
}

#[cfg(not(windows))]
mod stack_gauge {
    pub fn reserved_bytes() -> Option<usize> {
        None
    }
}

// ------------------------------------------------------------------ main ----

fn key_of(name: &str) -> Option<KeyCode> {
    Some(match name.to_ascii_uppercase().as_str() {
        "UP" => KeyCode::UP,
        "DOWN" => KeyCode::DOWN,
        "LEFT" => KeyCode::LEFT,
        "RIGHT" => KeyCode::RIGHT,
        "OK" => KeyCode::OK,
        "LSK" => KeyCode::LEFT_SOFT_KEY,
        "RSK" => KeyCode::RIGHT_SOFT_KEY,
        "CLEAR" | "CLR" => KeyCode::CLEAR,
        "NUM0" | "0" => KeyCode::NUM0,
        "NUM1" | "1" => KeyCode::NUM1,
        "NUM2" | "2" => KeyCode::NUM2,
        "NUM3" | "3" => KeyCode::NUM3,
        "NUM4" | "4" => KeyCode::NUM4,
        "NUM5" | "5" => KeyCode::NUM5,
        "NUM6" | "6" => KeyCode::NUM6,
        "NUM7" | "7" => KeyCode::NUM7,
        "NUM8" | "8" => KeyCode::NUM8,
        "NUM9" | "9" => KeyCode::NUM9,
        "HASH" | "#" => KeyCode::HASH,
        "STAR" | "*" => KeyCode::STAR,
        _ => return None,
    })
}

const USAGE: &str = "wie_probe — headless WIPI/KTF/LGT runner (measurement tool: it refuses rather than guesses)

  --zip <archive.zip>     REQUIRED. The game archive. No default, on purpose.
  --out <dir>             where frame/memory dumps land (default <temp>/wie-probe)
  --w <n> --h <n>         framebuffer size (default 240x320 — hero3's; hero2 is 176x220)
  --frames <n>            how many ticks to run (default 400)
  --ms <n>                virtual ms per frame. Without it the screen stays BLACK.
  --dump <f[,f...]>       write <out>/frame_<f>_<w>x<h>.rgba on those frames
  --dump-passive 1        a dump writes the last PRESENTED frame and forces no paint (★ 2026-10-14: the forced paint
                          runs the HUD painter once more — an effect counted in paints, 번개의 호흡's lightning, ends early)
  --dump-mem <hex:hex,..> raw guest memory snapshots on every --dump frame
  --keys <f:KEY[:hold]>   scripted input. KEY = UP DOWN LEFT RIGHT OK LSK RSK CLR
                          0-9 HASH STAR
  --profile 1             sampled guest PC profile
  --h2-benchmark <s:e>    hero2: prepare full HP/SP through frame s, then record actor-identity
                          HP telemetry through e; suppress enemy attacks only during preparation
  --hle <0|1>             host sprite blitter (default 1)
  --h2-relabel <f>        hero2: rewrite the open skill popup to 퀵슬롯등록/해제 from frame f on,
                          every frame (as the app does); only frames that write are logged
  --h2-commit <f:g>       hero2: at frame f, commit the open popup into guest slot g (0..3)
  --h2-tap <f:x:y[,..]>   hero2: route a framebuffer tap through the touch router at frame f
  --h2-inv <f[b|c][,..]>  hero2: print the live inventory (ag.a: heal index + every record), the
                          recovery cycle (bq.j) and the inventory popup if one is open, at frame f;
                          a trailing b / c prints the 재료 / 카드 bag (ag.b / ag.c) instead
  --h2-iaido-unlock <f>   hero2 (2026-10-03 발도): at frame f, run the app's unlock plan — a KNIGHT whose
                          방어의지 (a.p[8]) is open gets a.p[9] (발도, 부활오라's slot) -1 -> 0 — and apply it.
  --h2-ilpung 0           hero2 (2026-10-05 일풍격): skip the rogue's seventh-active block the app installs after
                          the accessory block (guest_anchors::IlpungPatch — a.p grows to 16 slots for every class,
                          value 7 = kind 15, the SP / cooldown / max level, the SKILL window's cell 6, the plan
                          and the drive; [heap] +1 alloc with it). [h2-ilpung] prints the plan words.
  --h2-ilpung-unlock <f>  hero2 (2026-10-05): at frame f, run the app's unlock plan — a ROGUE whose 번개난타
                          (a.p[10]) is open gets a.p[15] (일풍격) -1 -> 0 — and apply it. ★ 2026-10-07: a BERSERKER
                          whose 사자후 (a.p[10]) is open gets a.p[15] (도발) the same way.
  --h2-awaken-unlock <f>  hero2 (2026-10-07 (2) 각성기, repeatable): at frame f, run the app's slot-unlock plan for EVERY
                          slot it opens (the sixteenth as --h2-ilpung-unlock, and the class's awakening slot — 버서커
                          격분 a.p[16] · 나이트 천공의검 a.p[15] · 로그 개화 a.p[16] — at LV 70 or more) and apply it.
  --h2-bodyslam <f>       hero2 (2026-10-07 (2), repeatable): the app's 몸통치기 retire at frame f — a BERSERKER's a.p[5]
                          locked (-1) with every point it held refunded ONCE (the receipt = the book's with
                          --h2-costume-dir, else a flag of this run); [h2-bodyslam] prints the writes.
  --h2-damage-cap 0       hero2 (2026-10-07 (2)): keep the monster HP delta's SHORT (bd.a(IB)V: a hit past HP + 32,768
                          wrapped the HP positive — the effective per-hit cap); default 1 = the two halfwords that
                          keep the word, with the other balance patches at frame 30.
  --h2-hp-int 0           hero2 (★★ 2026-10-16): keep the five OTHER monster HP stores' 16-bit wraps (spawn · shipped
                          poison tick · two boss heals · template x/2); default 1 = monster HP is an int (no 32,767).
  --h2-fury-gauge <f:g>   hero2 (2026-10-09): set the berserker block's 격분 gauge to g % (and its hits) at frame f.
  --h2-voice 0            hero2 (2026-10-09): no voices — the berserker block keeps the game's 기합 (FURY_VOICE 0) and
                          the cues are not watched. Default 1 = the app: [h2-voice] f=… <clip> at each cue, and with
                          --audio-wav the clip lands in the WAV at the game's sound level.
  --h2-berserker 0        hero2 (2026-10-07): skip the berserker block the app installs after SkillPower · accessory ·
                          combat · 일풍격 (guest_anchors::BerserkerPatch — 분노태세 ON/OFF with its dealt/taken
                          factors, the shipped ×2 attack · ×3/5 defence off, 「도발」 kind 15 / value 8; [heap] +1 alloc).
                          [h2-berserker] prints TAUNT · LAST · a.n (rage) · the character's buffs when they change.
  --h2-born-repair <f>    hero2 (2026-10-06, repeatable): the app's born-skill repair at frame f — a knight's 중장비
                          a.p[10] 0 → 1, a rogue's 심안 a.p[12] (the old 독 연마) → 1 with EVERY level it held back
                          as skill points, ONCE (slot first, points last; the receipt = the book's with
                          --h2-costume-dir, else a flag of this run), then only held at 1; [h2-born-repair] prints
                          the plan and a replan.
  --h2-skill-set <f:i:lv> hero2 (diagnostic, repeatable): at frame f, write a.p[i] = lv (the character's skill
                          level array, any length) — e.g. 일풍격 = 15 on a test rogue the quest never reached.
  --h2-cap <f:n>          hero2: at frame f, write the main bag's capacity byte (bq.b) to n.
                          The 5-page inventory experiment: nothing outside `bq` reads that
                          field, and the save serialises records, not capacity.
  --h2-give <f:T:I:N[,..]> hero2: from frame f, give N of item (type T, id I) — into the bag the game's
                          drop routes it to (★ 2026-10-09: 23 · 30 · 31 · 32 the 재료가방, the rest the inventory)
                          the way a drop does — `ch.a(BBZZ)` makes the record, `bq.a(Lch;B)I`
                          places it (JVM calls by name, see guest_anchors::JavaCall). One item
                          per tick pair; prints the count PLACED for each.
  --h2-move <f:S:D:IDX[:N]>
                          hero2: at frame f, MOVE record IDX of bag S to bag D (a = inventory,
                          b = 재료 가방, c = 카드, w = 창고) as the same object — `bq.a(Lch;I)Z`
                          into D, then `bq.b(B)V` out of S, and only if the insert said yes.
                          With :N (and N < the stack) it is the PARTIAL move instead — the
                          three calls the app runs once the game's own 개수 box has been
                          answered: `ch.a(BBZZ)` for a fresh record, `bq.a(Lch;B)I` into D
                          (which merges into its stacks and says how many it PLACED), then
                          `bq.a(BI)V` for exactly that many out of S. ★ 2026-10-13: a STACK
                          always moves that way (whole too — the app's rule), so it joins D's
                          stack of it; :0 = the old object move (a cell per move — how a
                          warehouse came to hold four 1-stacks).
  --h2-fold <f:S>         hero2 (★ 2026-10-13): from frame f, fold bag S (a b c w) the way the
                          warehouse load does — each record an earlier stack of its kind has room
                          for, one split at a time with S as both ends, until none is left.
  --h2-pages 1           hero2: install the inventory PAGING patch at frame 30 (the app's
                          own plan — guest_anchors::InventoryPagePatch: 7 `bl` retargets, 7
                          cells, 3 trampolines in a host block) and keep its context current.
                          ★ 2026-10-20 + the 사용하기 → a.g(B)V site (ItemUsePagePatch) on top of it.
  --h2-use-page 0         hero2: with --h2-pages, leave that 사용하기 site as shipped (A/B: a potion
                          on page 2..5 then uses page 1's record at the same cell).
  --h2-difficulty <f:d>   hero2 (2026-09-29): from frame f, hold 난이도 d (0 EASY · 1 NORMAL ·
                          2 HARD) — the app's own planner (H2Difficulty) every 5 frames, its
                          writes applied, each monster's hp/max/atk printed before → after.
                          Repeatable: a later frame switches the level (the live rescale).
  --h2-exp <n>            hero2: at frame 30, install the 경험치 배율 redirect with multiplier
                          n (the app's own plan — guest_anchors::ExpRatePatch: a.d(I)V's
                          definition word → a 16-byte host block that scales the argument).
                          The app only ever asks for 1/2/4/6; any 1..=64 is accepted here.
  --h2-drop-rate <f:d>    hero2 (2026-10-16): from frame f, hold 드랍률 d (0 낮음 · 1 보통 · 2 높음)
                          — the app's own plan (guest_anchors::DropRatePatch: bf.a(BBBB)V's
                          definition word → a host block that keeps 1/16 / ¼ / all of the EQUIPMENT
                          drops). Installed the first time d < 2; repeatable (a later frame moves
                          the KEEP word). `[h2-drop-rate]` prints the block's refused/passed
                          counters, and --h2-kill's report adds them.
  --h2-slots <3|5>        hero2 (2026-09-27): the save-slot count. 5 (the default, = the app) runs
                          the app's own install at the logo (emulator::SaveSlots — ag initialized
                          early, ag's /g and /n name tables five long, then ag.k()V's previews and
                          the slot screen's code); 3 leaves the game's own three. [h2-slots] lines
                          show the install and, while a slot screen is up, its mode/cursor/rows.
  --h2-slots-from <f>     start that install at frame f instead of 1 — the app's LATE path (a
                          title built with three previews first: the screen waits for the next).
  --h2-minimap <side>     hero2 (2026-09-29): on every --dump frame also write
                          minimap_<f>_<W>x<H>.rgba — the frame scaled to phone height (~1080) with
                          the app's hero5-style minimap panel (<side> px, 280 = S25/S9) in the
                          top-right corner, drawn by the app's own minimap_view.rs from the same
                          cells (floor · wall · exit · chest) and marker the worker sends. [h2-map]
                          marks the exits (E) and the still-closed treasure chests (C) either way,
                          and lists chests(group, flag, opened). The live monsters are its red
                          squares (h2_monster_marks, listed on the [h2-minimap] line).
  --h2-pose-audit 1       hero2 (2026-10-10): on every --dump frame, walk EVERY pose of the
                          player's live class animation (sequence x frame x 4 directions) and look
                          up each head/body/arm/leg sprite's image the way the drawer does
                          (cf.a[part (+8 mirrored)][f]): [h2-pose-audit] prints the poses, the
                          sprites and the ones whose image is null (the drawer skips them — that
                          part vanishes in that pose; vC 98's mirrored legs) or past the array
                          (the drawer throws). 0 missing = no pose draws a body part see-through.
  --h2-hd-audit 1         hero2 (2026-10-16): every paint prints its HD record's demon body draw ([h2-hd-paint]) —
                          a dump shows only the LAST paint's record; this counts the paints that lost the body.
  --h2-hd <k>             hero2 (2026-10-15): 악마화 HD — the canvas records the effect draws instead of drawing
                          them (the plain dumps then show no effect and no demon body), the demon block gets HD = 1
                          (the blank body sheet 301, the body as effect FX_BODY) and every --dump frame also writes
                          hd_<f>_<W>x<H>.rgba = the frame at k× with the record composited by the app's
                          compose_hd (the library's 3× twins). k = the window's scale (5 = S25). 0 = off.
  --h2-status 1           hero2 (2026-09-29): on every --dump frame also write
                          hud_<f>_<W>x<H>.rgba — the frame scaled to phone height (~1080) with the
                          app's status HUD (hud_view.rs: head ring · HP/SP · level plate at the
                          top-left, EXP at the bottom centre) from the live gauges and the helmet's
                          head sprite (h2_head_sprite); with --h2-minimap the panel too.
  --h2-exp-gain <f:n>     hero2: at frame f, call the game's own a.d(I)V with n (JVM call by
                          name, JavaCall::CharGainExp) and print the character's and the
                          beast's EXP before and after — the redirect's end-to-end check.
  --h2-shipped 1          hero2: do NOT apply the balance patches (#16 combo SP · #18 skill
                          points · #17 cooldown + skill power · card drop
                          off · honour penalty off · title menu without 네트워크창고 · ★ 2026-10-06 the
                          beast's 지력/SP and the rogue block) — the shipped rules, for an A/B
                          against the default (= the app).
  --h2-venom 0            hero2 (2026-09-29): skip ONLY the 맹독감염 block (the buff cast, the
                          기본공격 poison, the attack-share tick) — the rest as the app has it.
                          [h2-flags] shows the buff (id 33), [h2-mobs] the poisoned HP.
  --h2-boss-crit 0        hero2 (★ 2026-10-10): skip ONLY the boss crit floor (guest_anchors::BossCritPatch) — a
                          boss crit / 투지 hit back to the shipped ⌊crit % / 10⌋ × the hit (0 under 10 %), for an
                          A/B against the default (= the app: never below the weapon's crit multiplier).
  --h2-boss-pierce <pct>  hero2 (★ 2026-10-21): the share of a boss's hit that passes defence (guest_anchors::
                          BossPiercePatch) — default 20 (= the app); 0 = skip the block (the shipped hit, for an A/B).
                          [h2-boss-pierce] says the install. A boss's hit is max(R − DEF, R × pct / 100).
  --h2-grade-line 0       hero2 (★ 2026-10-13): skip ONLY the grade line (guest_anchors::GradeLinePatch) — a long
                          item name and its grade back on one line (overlapping), for an A/B against the default
                          (= the app: the grade on the empty line under a name wider than 71 px, right-aligned).
  --h2-shop-sell 0        hero2 (2026-09-30): skip ONLY the 야수진화제 sell-price block (the
                          sale's and the info painter's price) — the shipped price / 4, for an
                          A/B against the default (= the app: 백·흑 2,500, 적 4,000).
  --h2-hell-drop 0        hero2 (2026-10-02): skip ONLY the HELL bonus drop (guest_anchors::
                          HellDropPatch — bd.q()V's wrapper that, in HELL, rolls the enemy's
                          bonus slots 5.. once more): HELL drops exactly as HARDCORE, for an A/B.
                          Measure with --h2-kill on a NORMAL save + --poke <f>:193c4c:02000000.
  --h2-hell-gate <0|1>    hero2 (2026-10-02 저녁, 드랍 티어): hold the HELL block's 난이도 gate
                          word (the new six accessories' slot 8) closed or open regardless of
                          --h2-difficulty — the app opens it at 난이도 NORMAL and above (HARD only
                          until 2026-10-03), whose monster scaler can kill the test knight before
                          2,000 kills.
  --h2-gear 0             hero2 (2026-10-09, 가디언 장비의 개성 옵션 31): skip the gear block the app installs
                          after every other wrapper (SP ×3 · damage ×3 · 철괴 hook · buff add · a.m()V chained, the
                          crit read hooked; the guardian / venom blocks' GEAR_PTR words pointed at its effect
                          words). With it in, every 5 frames the seven equipment slots and the class set the
                          effect words as the app does; [h2-gear] prints the worn options when they change.
  --h2-gear-wear <f:list> hero2: at frame f write the effect words for the items in `list` as if worn
                          (`type.id` joined by `+`, e.g. 0.33+6.51+12.21; empty = nothing) and stop detecting
                          (forced, repeatable). Max HP / SP are NOT forced — the a.m()V piece reads the real
                          equipment.
  --h2-guardian 0         hero2 (2026-10-08, 「가디언」 모드): skip the fourth round's three pieces —
                          the mode tables (ag.i:[B · ag.m · co.b four long + the round clear's #2,
                          every frame from f=1 before the save-slot install), the template block
                          (cq.a()V's wrapper: HELL LV 60..80 → 70..105 · HP ×1.5 · ATK/DEF ×1.2 over
                          HELL, and the four bd.n hooks that show the formulas the HELL level) at f ≥ 30, and
                          the HELL block's guardian pass. [h2-guardian] prints the tables' state,
                          the block's seven words and, in mode 3, [h2-mobs]'s LV/ATK show the scale.
                          Load a mode-3 save with `h2save.py edit … --mode 3`.
  --h2-colour-init 0      hero2 (2026-10-17): don't initialise `co` (the slot colours co.b:[I) from the host
                          during the logo — the game then does it in the slot screen's first paint and
                          GUARDIAN draws black (blank) on the first 이어하기 after a cold boot. Default 1 = the app.
  --h2-guardian-word <i:v> hero2: at install, word i (0 HELL_BASE · 1 LEVEL_BASE · 2 LEVEL_MUL · 3 LEVEL_CAP ·
                          4 HP_MUL · 5 ATK_MUL · 6 DEF_MUL · 7 AGI_MUL · 8 SEEN_MUL · 9 CORRECT ·
                          10 BOSS_HP_MUL (drop gate 0) · 11 KILL_EXP_MUL (bd.q()V's a.d(I)V argument in
                          mode 3 — quest EXP untouched) · 12 GEAR_PTR (the host's) · 13 EVADE_TRUE ·
                          ★ 2026-10-16 14 RAID_MAP (★★ 10-17 the first stage map, 113; 255 = none) ·
                          15 RAID_LEVEL (110) · 16 RAID_MUL (HP · attack ×, over guardian, on those maps) ·
                          ★★ 17 RAID_COUNT (9 stage maps); the MULs are /256) of the block ← v —
                          the balance knobs, for an A/B without a rebuild. Repeatable.
  --h2-raid-gate <0|1>    hero2 (2026-10-16, 보스 레이드): 1 = the temple (map 15) loads as the gate-bearing copy
                          (the archive's P/m/m/112.m.gz: the upper-right pillar = a red-flamed 귀환문 whose OK
                          asks 「보스레이드를 시작할까요?」 and warps to stage map 113) · 0 = the shipped 신전. Without
                          the flag a /g read decides it as the phone does: the committed costume record
                          (--h2-costume-dir) bound to that save holding a GUARDIAN HARD receipt; [h2-raid] says.
  --h2-boss-raid <0|1>    hero2 (★★ 2026-10-17, 보스 레이드 아홉 단계): the app's raid driver (hero2java::H2BossRaid) —
                          on maps 113..121 a kill flag → 5..1 countdown notices (raidnotice_<f>_<w>x<h>.rgba +
                          [h2-raid]) → bb.a(B)V on the stage's advance group → the next stage (after the ninth the
                          first: the next lap, ×1.3 HP · ×1.05 ATK through the 난이도 poll, `N바퀴 시작!`); back on a
                          non-raid map the gold (ag.g(I)V) and two notices. Default 1 (it acts on raid maps only).
  --h2-raid-autokill <f>  hero2 (★★ 2026-10-17): from frame f, on a raid stage with its kill flag still down, the
                          first monster on the stage dies (bd.q()V) every 20 frames — so a probe run walks all
                          nine bosses; [h2-raid] says which death raised the flag (and lists a stage of
                          several monsters). --h2-raid-autokill-last 1 kills the LAST one first instead.
  --h2-raid-notice-check <f> hero2 (★★ 2026-10-17): from frame f (in the field), a few texts go through the game's own
                          notice n.a([BII)V and the picture it renders is compared, pixel for pixel, with the host's
                          h2_notice_picture of the same text ([h2-raid-check] · raidcheck_<i>_{game,host}.rgba).
  --h2-kill-any <0|1>     hero2 (2026-10-16): with --h2-kill, kill the FIRST monster on the stage instead of the
                          one faced — a `ca` boss (율리케 in the raid room) teleports and is never in front.
  --h2-acc <f:code>       hero2 (2026-10-02, 스킬을 바꾸는 장신구): at frame f write ACTIVE = code into the
                          app's AccessoryPatch block (0 none · 1 광룡의 외침 · 2 바바리안의 의지 · 3 야수조련 ·
                          4 크루세이더 · 5 벽력일섬 · 6 극사의 목걸이), regardless of the worn item (repeatable).
  --h2-acc-auto 1         the app's own detection every 5 frames: the record in equipment slot 2 and the
                          class -> ACTIVE (guest_anchors::AccessoryPatch::code_for); a map change ends a channel.
  --h2-acc-relabel <f:t:i> DIAGNOSTIC: at frame f the worn accessory record's ch.g/ch.h <- t/i (so a save
                          wearing any ring or necklace tests the detection, e.g. 12:17 = 광룡의 외침).
  --h2-acc-patch 0        skip installing the accessory block (the shipped game, for an A/B).
  --h2-acc-word <f:off:v> DIAGNOSTIC: at frame f the accessory block's data word at hex offset off
                          (< 0x40: ACTIVE 0 · CHANNEL 4 · … · TRAIL 24 · TRAIL_K 28 · TRAIL_C 2c) <- v.
  --h2-mob-place <f:x0:y0:x1:y1> DIAGNOSTIC: at frame f the monster standing on tile (x0,y0) is moved to
                          (x1,y1) by its own bd.a(SS)V (pixels = tiles x 16) — a target where a test needs one.
  --h2-mob-hurt <f:x:y:dmg> DIAGNOSTIC (repeatable): at frame f the monster on grid tile (x,y) takes dmg through
                          its own bd.a(IB)V(dmg, 0) — a kill falls and leaves the grid like a real one (e.g. 오딘's
                          two hands in map 109, so only the head is left).
  --h2-beast-refund <f:idx>  hero2 (2026-10-16): at frame f call the game's own bn.d()V on owned beast #idx — the
                          refund the app asks before the beast list's 「버리시겠습니까?」 예; [h2-beast-refund]
                          prints the [h2-reset] report before and a frame after.
  --h2-beast-skill <f:slot:type:level> DIAGNOSTIC: at frame f the beast's skill slot (0..=3) <- that
                          skill type (bn.j, 0..=12: 3 암살자 · 7..9 the rings · 10..12 the heavy ones) and
                          level (bn.k) — the quickbar's beast values 10..=13 are these slots.
  --h2-clear-hook 0       hero2 (2026-10-01, 의상 아바타 A4): skip the round-clear counter the app
                          installs from f=30 (guest_anchors::ClearHookPatch — ag.y()V's definition
                          word to a block that only counts; [heap] +1 alloc with it).
  --h2-gl 0               hero2 (2026-10-22, 가디언 레벨): skip the level-up's GL wrapper the app installs from f=30
                          (guest_anchors::GuardianLevelPatch — a.l()V's definition word; [heap] +1 alloc with it).
                          With --h2-costume-dir it is armed on the world entry from the record's GL, the record
                          follows it, and [h2-gl] says each step (a LV 99 save: tools_re/h2save.py edit --level 99;
                          EXP: --h2-exp-gain).
  --h2-flight <f[:town]>  hero2 (2026-10-21): at frame f, with the bag popup on a 비행서 (itm/28 id 1), do what the app's
                          AccountState::start_flight does — no town: write the town list over the popup's labels (the LIVE
                          save's visited towns, ag.q[31]); with a town index 0..5: consume one from the bag the popup's
                          window shows (the inventory OR the warehouse) and warp (ag.b(BBBB)V). Prints the popup's rows,
                          the move row's place (창고에넣기 / 가방에넣기 — stage 1 only) and the visited set.
  --h2-diff-gate 0        hero2 (★ 2026-10-22 상점주인의 혼 피해 급증): run --h2-difficulty with the app's gate AS IT WAS
                          before this day — a state word outside `AppState::in_game` (3 · 4 · 5 · 8; the SHOP is 7)
                          made the planner FORGET its bases, and the next in-game poll re-based from the words as they
                          stood (HARD's ×2 became ×4 on every shop). Default 1 = the app now: out of the world
                          (`h2_demon_in_world` — the title shell or no character) forgets; the bases otherwise live
                          with the STAGE OBJECT (a new map = a new `ag.a:Lbf;`). `[h2-diff]` says when it forgets.
  --h2-menu-release 0     hero2 (★ 2026-10-21): close the field menu for --h2-flight / --h2-shop-orb the OLD way (the
                          menu's child <- 0 and the state <- 3 only) instead of the app's release (state <- 3, then the
                          game's own bw.a(Z)V) — the A/B for 「the menu does not open after a 비행서」.
  --h2-shop-plate <f>     hero2 (★ 2026-10-21): at frame f press a 상점주인의 혼 PLATE the way the app's
                          AccountState::use_shop_plate does — the live inventory must hold (21, 8) (a warehouse orb is not
                          there), the stock loaded when null, the shop event (4, 0) posted; no menu to close.
  --h2-park <f:0|1>       hero2 (2026-10-21): at frame f park (1) or release (0) the guest's tasks the way the app's
                          world map does (Emulator::park_guest) — the game stands, the music player (a host task) runs;
                          read it with --audio-log and the [work] / [paint] lines.
  --h2-item-icon 0        hero2 (2026-10-16): skip the item icon block the app installs from f=30 (ItemIconPatch on
                          n.a(IIB)V — the elixir draws the strip cell the asset appends).
  --h2-cd-clock 0         hero2 (2026-10-02): skip the cooldown tick counter the app installs from
                          f=30 (guest_anchors::CooldownClockPatch — ag.i()V's definition word to a
                          block that only counts; [heap] +1 alloc with it). [quickbar] prints it
                          as ticks=.
  --h2-slot-focus 0       hero2 (2026-10-12): skip the slot-screen focus the app installs from f=30
                          (guest_anchors::SlotFocusPatch — be.a()V's definition word to a block that runs the
                          first-save finder and then puts the cursor on the slot last played; [heap] +1 alloc).
  --h2-slot-last <1..5>   hero2 (2026-10-12): the slot last played, as wie_save_last.txt seeds the app (default
                          none). Each field entry then notes ag.c like the app. [h2-slot] prints the slot
                          screen's cursor when it opens and every LAST change.
  --h2-rogue 0            hero2 (2026-10-06): skip the rogue block the app installs from f=30 (guest_anchors::
                          RoguePatch — 번개난타's slot is 살상연희: the nearest monsters of the 5×5, behind each in
                          turn, see-through and untouchable, 10 s; a rogue whose 심안 a.p[12] is ≥ 1 sees the ghost
                          monsters at half alpha; [heap] +1 alloc with it). [h2-slaughter] (--gauges) shows a run.
  --h2-teleport-gate 0    hero2 (2026-10-11): skip the gate the app installs after the rogue block (guest_anchors::
                          TeleportGatePatch — while 맵 18's three 도적 corpses lie there, event 0x10 on and 0x11 off,
                          the rogue's teleports 살상연희 · 일풍격 · 개화 (and 일섬 · 극사 while worn) are not cast).
                          [h2-tgate] prints the install and every MASK change.
  --h2-demon 0            hero2 (2026-10-13 「악마화」 D1 · ★ D2 the look): skip the 악의 block the app installs after every other block
                          (★ D2: the demon LOOK needs the avatar override — give --h2-costume-dir <dir>; the probe then runs the
                          app's `Demon::poll`: body sheet 300 while STATE 1·2, LOOK 1 once it is in, LOOK 0 before the human one)
                          (guest_anchors::DemonPatch — the beast's 대지분노 turns the character demon for 60 s: received
                          damage −15 %, the class's attack skills refused, the 기본공격 stages a claw combo on the six tiles
                          ahead / the 5×5 slam at a.a()I + 지력'×4.73; SP max/4, the slot's cooldown 90 s from the cast).
                          [h2-demon] prints the install and the block's words whenever they change (--gauges N: every N).
  --fx-lib <file>         ★ 2026-10-14: draw 악의's effects from this H2FX library instead of the built-in
                          wie_android/src/demon_fx.bin (tools_re/demon_fx.py build — iterate on the art without a rebuild)
  --h2-sky-draw 0         hero2 (2026-10-08): skip the 천공의검 / 개화 picture block the app installs after the rogue block
                          (guest_anchors::SkyDrawPatch — a wrapper on the character draw a.a(BBII)V, chained, that draws
                          the knight's three tears · cracks · flash with the game's Graphics, and one on the ground draw
                          bf.a(Graphics;II)V for 개화's floor; [heap] +1 alloc with it).
  --h2-buff-refresh 0     hero2 (2026-10-05): skip the buff refresh the app installs from f=30
                          (guest_anchors::BuffRefreshPatch — g.a(BI)V's definition word to a block
                          that, when the same buff is already up, restarts it in place: elapsed 0,
                          length = the new cast's; as shipped the recast is refused and the old buff
                          keeps only its remainder; [heap] +1 alloc with it). See it in [h2-flags]
                          (--gauges): buffs(id,elapsed,len).
  --h2-beast-recovery 0   hero2 (2026-10-05): leave the beast's 빛의회복 shipped (guest_anchors::
                          beast_recovery_buff — 4 halfwords in bn.f()V send type 4 through bn.c(4) like
                          얼음장막: the buff list g.b, the game's own type-4 regen tick a.a(Lav;)V, the
                          HUD timer). As shipped it is an effect-list animation only. [h2-flags] shows id 4.
  --h2-beast-tune 0       hero2 (2026-10-06): leave the beast's attack skills shipped — 지력 ×1 in the rings
                          (빛의파동·지옥불·대지분노) and the heavy three (심판번개·학살화염·암흑폭발) and their SP
                          (guest_anchors::{beast_int_weight, beast_attack_sp_cut}: 지력 ×1.5 · 암살자/rings/heavy SP
                          −20 %, the buffs as shipped). [h2-beast-cost] (--gauges) prints the slots' SP and cooldown.
  --h2-cd-flip <f1:f2:g[:old]> hero2 (2026-10-02, repeatable): a quickbar page flip's two halves on
                          guest slot g — at f1 its skill leaves the bar (the counter parked in the
                          app's quickpage::Window), at f2 it is seated again with what the parked
                          counter has left by the clock. The cd= of [quickbar] after f2 must equal
                          an undisturbed run's. `:old` = ready on every seating (the reported bug).
  --h2-clear <f>          hero2 (A4, repeatable): at frame f start the next round the game's own way
                          — the entry kind ag.e:B ← 2 and event 9 for the dispatcher ag.g()V, which
                          runs ag.y()V (the mode +1, HP/SP full). [h2-clear] prints the count, the
                          mode cleared and, with --h2-costume-dir, the rewards the book paid
                          (NORMAL → set 200 · HARDCORE → 201 · HELL → 203, + 204 for a round whose
                          done quests were 95 % at 난이도 NORMAL+ — ★ A5, for the character's class)
                          and the game's notices once the field is back.
  --h2-quest <f:i:s>      hero2 (★ 2026-10-01 의상 아바타 A5, repeatable): at frame f set quest i's
                          state to s (ag.r:[B, two bits a quest; 2 = done) — with --h2-costume-dir the
                          book marks it with the 난이도 held then (--h2-difficulty), as the app's
                          HUD poll does. [h2-quest] prints the write.
  --h2-tools 1            hero2 (2026-10-16): run the app's 정렬 / 일괄판매 driver (bag_tools_host.rs) — the
                          buttons are composited into every --dump frame, the scripted keys go to it
                          first (OK / CLR on its popups), [h2-tools] prints what it did. The inventory's
                          정렬 button sits at panel + (41, 15) 12x12, the sell window's 일괄판매 at
                          panel + (3, 109) 43x13 (both panels at (28, 51) on a 176x220 guest).
  --h2-tools-tap <f:x:y[,..]>  hero2: a tap at framebuffer (x, y) on frame f, to the tools first —
                          the app's InputEvent::MenuTap.
  --h2-shop <f>           hero2: at frame f open the shop the way its script event does
  --h2-shop-orb <f>       hero2 (2026-10-20): at frame f, with the bag popup on a 상점주인의 혼 (itm/21 id 8), do what the
                          app's 사용하기 does — load the stock (z.a()) into v.a when null, close the field menu, post the
                          shop event (4, 0); then print the state word until it reads 7 (the shop)
                          (ag.g()V: v.a()Lv; · v.a()V · ag.h(7)), then print [h2-gold] on
                          every change of the gold (ag.f:I). Drive it with --keys: DOWN, OK
                          = 아이템 판매; UP from the top = the bag's LAST record (--h2-give
                          appends); OK (판매하시겠습니까?) OK (예) [OK (the quantity box)].
  --h2-alchemist <f>      hero2 (★ 2026-10-09): at frame f open the ALCHEMIST's menu the way its script event
                          does (ag.g()V: ag.h(8) · aw.a()Law; · aw.a()V; state 8). Drive it with --keys:
                          DOWN, OK = 아이템 조합 → the recipe picker (the 재료가방's 조합서); OK on a recipe →
                          its materials; OK = 조합 (1,000 gold). `[h2-picker]` prints the picker on every change.
  --h2-craft-guard 0|1    hero2 (★ 2026-10-20, default 1 = the app): a scripted OK / 5 on the crafting window that
                          would craft a result into a FULL bag is taken — the game's own 「빈공간이 없습니다.」 box
                          (ba, itmtp 7) is hung on the window instead (`h2_craft_overflow`, emulator::craft_full_guard).
                          0 = the key goes to the game (the shipped behaviour: the result is made and lost).
                          `[h2-craft]` prints the decision either way.
  --h2-picker-page <f:p>  hero2 (★ 2026-10-09): at frame f put the item picker (k — the recipe list, EQUIPMENT's
                          candidates, 능력 부여's lists) on page p (0..4) the app's way: the list array rewritten
                          in place to that page (hero2java::h2_picker_page_writes). A list over 28 entries is put
                          on page 0 (its length word 28) the frame it is first seen, as the app does.
  --h2-bags127 1          hero2 (★ 2026-10-09): keep BOTH game bags (inventory ag.a + 재료가방 ag.b) at 127, the
                          app's standing write; with --h2-pages 1 the 재료가방 window pages too.
  --h2-avatar <f:spec>    hero2 (2026-09-30, 의상 아바타 A0): install the app's override on the
                          parts loader cf.a(I)V (from f=30, verified — guest_anchors::AvatarPatch),
                          at frame f write the host's (sprite, palette) per part from spec =
                          body:<n>[:<pal>],arm:<n>[:<pal>],leg:<n>[:<pal>] (a part not named =
                          the equipment's own), then ask the game's own cf.a(I)V to reload
                          parts 1, 2, 3 one call at a time — the cf.a[part+8] handles are
                          printed before and after (a new handle = the reload happened). Dump
                          a frame a few frames later to see the costume; with a far frame the
                          patch installs and nothing changes (the render-neutral A/B).
  --h2-equip-layout 0|1   hero2 (2026-09-30, 의상 아바타 A3): move the EQUIPMENT window's seven
                          cells left (guest_anchors::equip_cell_shift, from f=30, verified)
  --h2-equip-ui 0|1       hero2 (2026-09-30, 의상 아바타 A3): the app's EQUIPMENT avatar side — the
                          cells moved + the cursor switch (EquipCursorPatch; from f=30, all or
                          nothing), the scripted keys and --h2-tap through the app's model
                          (H2EquipAvatar), the picture burned into every --dump frame. Owned
                          costumes come from --h2-costume-dir's book
  --h2-costume-give <f:n[:mode[:slot]]>  (repeatable) give set n's items without wearing them (mode
                          0 NORMAL · 1 HARDCORE · 2 HELL; ★ A3c slot head|armor|shoes|weapon|
                          offhand = that one item, else every item of n the jar carries for the
                          character's class — ★ A4) — the wardrobe's test state
  --h2-costume-catalog <file>  an EUC-KR catalog (hero/costume.txt's format) instead of the jar's
                          — long names and many items without touching the asset
  --h2-costume-marker <slots>  ★ A3c: before booting, run the app's adb grant marker
                          (vault::grant_costumes_on_request → avatar_state::grant_into_record)
                          for these save slots (1, or 1,3) over the --fs files and the
                          --h2-costume-dir records: every catalog item the jar carries goes into
                          the slot's record, owned and not worn — ★ A5: numbers 200..999 in the
                          same text name sets, and then only those sets' items (1 2 202 203)
  --h2-equip-preview <seq:frame:dir[:noweapon]>  (A3) burn the host's character preview (the
                          app's h2_equip_preview: the live parts, pose from the class blob) into
                          every --dump frame while the EQUIPMENT window is on top
  --h2-costume-dir <dir>  hero2 (2026-09-30, 의상 아바타 A2): run the app's costume BOOK
                          (avatar_state::AvatarBook) with its records in <dir> (a real directory
                          — keep it between runs to test a restart): world entry from the shell
                          loads the slot's record against the in-memory /g (새로하기 = a fresh
                          character), a rewritten /g commits it (the journal is written at the
                          save's first /g edge), the worn costume becomes the parts override.
  --h2-costume-grant <f:n>  (repeatable) at frame f, give the character in the world set n (★ A3c
                          every item the catalog lists for it) and wear them — kept only if the
                          game then saves
  --h2-costume-wear <f:n|->  (repeatable) at frame f, wear the first owned item of set n in every
                          slot where one is owned, or nothing in any slot
  --h2-kill <f:n>        hero2 (#20/#21): from frame f, find the monster the character faces
                          (a.a()Lbd;) and run the game's own death routine bd.q()V on it n
                          times, tallying the ground list (bf.b) after each: gold, gear,
                          제련석, 물약, 스펠카드 per kill + every (type:id) seen. Test save
                          slot 2 (--fs dist/test_saves, keys 320:OK,420:OK,470:RIGHT,520:OK)
                          is a LV26 나이트 in a fight. (The asset ships NO saves since
                          2026-09-25 — make_hero2_asset.py unpacks the dump's to dist/test_saves.)
  --h2-skill <f>          hero2 (#17): at frame f, the class skill multiplier a(BILbd;)I(1000)
                          for every a.p index and the cooldown totals a(I)S, by name.
  --h2-drop-up <f[,f]>    hero2 (조이스틱 2026-09-24): the scripted key RELEASE due on frame f is
                          NOT delivered — the host-side shape of what the guest itself does
                          with a release outside state 3 (n.keyNotify ignores it). Shows the
                          walk going on with no key held; [h2-walk] in --gauges shows t=2.
  --h2-walk-guard 1       run the app's owed-release rule (emulator::WalkGuard) on the scripted
                          keys: a direction released >= 12 frames ago with the character still
                          walking on the field gets its release re-sent. Pair with --h2-drop-up.
  --h2-relatch 1          run the app's deferred re-press rule: an action key released while a
                          direction is held re-presses that direction once the character
                          stands again (the game refuses a press during the action states).
  --h2-notice 1           the app's notice feed: the game's HUD painter off (as the app has it — all
                          but the target-monster panel since 2026-09-25) and every pickup notice
                          copied out of n.b as notice_<f>_<w>x<h>.rgba.
  --h2-hud <shift>        the app's HUD painter plan — everything off EXCEPT the game's own
                          target-monster panel (name · LV · HP bar; tail 0x163a1e then head
                          0x1639a0) — with the panel dodged <shift> guest px left, the way the
                          app moves it beside the minimap (one site, 0x1640f2; 0..=189).
  --h2-weapon-type <f:t[:slot]> hero2 (2026-09-27): at frame f, relabel every equipped weapon's item type
                          ch.g as t (0..=4; diagnostic) — a sword knight can then cast 둔기강타.
                          :slot = only the weapon in that equip slot ([h2-honor]'s index; 2026-10-06).
  --h2-weapon-trait <f:t> hero2 (★ 2026-10-10): at frame f write every equipped weapon's 특성 bo.i (abs 0x58) and
                          the character's copy a.m:B (abs 0xd8) as t (1 일섬 · 2 집중 · 3 흡수 · 4 저항 · 5 투지 ·
                          6 기절 — orb id + 1; diagnostic). [h2-tuji] prints the 투지 flag a.h:Z on every change.
  --h2-sweep <f:kind[:face]> hero2 (2026-09-27): at frame f, call the knight's bx.h(B)Z by name with
                          that a.p kind (5 둔기강타 · 6 방패치기) — with the COMBAT block in, the
                          front sweep itself, independent of the cast animation and its keys.
                          [h2-sweep] prints the return (anything hit); [h2-mobs] shows who.
                          :face (1 up · 2 down · 3 left · 4 right) writes g.u first (diagnostic).
  --h2-honor <f[,f]>      the player's 명예 and every equipped weapon's 요구 명예: the full attack
                          pair next to what the shipped penalty would leave, and whether the
                          penalty patch (on by default; --h2-shipped 1 keeps it) is in.
  --h2-autoloot 1         the app's 오토루팅: the nearest drop within 2 tiles that FITS its bag is
                          moved under the player and picked up by the game's own ag.a()Z.
  --h2-drop-life 1        the app's ground-drop lifetime top-up (e[5] refilled 40 -> 120 until
                          ~1200 ticks); [h2-drop-life] lines per refill.
  --h2-turn 1             the app's stick-turn rule: a direction release due on the frame another
                          direction is pressed is not sent (on the field the new press alone
                          re-aims the walk; emulator::apply_turn).
  --h2-reverse 1          the app's reversal rule: a press straight back along a walk that is
                          between two tiles writes the facing at once (emulator::reverse_walk).
  --h2-revive 1           the app's 부활의 구 (emulator::Revive): when the character's fall reaches
                          its frame 7 of 0..=8 (the animation plays first) with an orb (21:5) in
                          the bag the box goes up and the guest is NOT ticked until
                          the scripted keys answer (OK = the cursor, LEFT/RIGHT flip it; it opens
                          on 예). 예 = HP/SP full + g.w/g.t words + one orb consumed + the
                          notice; 아니오 = the game's own GAME OVER. The box goes into dumps.
  --h2-die <f[,f..]>      hero2: at frame f, make the character fall with the game's own
                          a.b(I)V (−hp) — the killing blow, headless. Give the orb with
                          --h2-give <f:21:5:n> first.
  --h2-combo-watch <a:b>  hero2: every frame a..=b print the character's action state — g.t, g.w,
                          the chain a.o[] and its index a.i, the open stages a.h, the chain
                          offset a.o:I and the lock a.j — the fields a.d(B)Z judges OK/skill by
                          (the 부활의 구's stand-up clears that chain, 2026-10-26).
  --h2-confirm-check <f>  hero2: at frame f, with one of the game's own 예/아니오 boxes on top
                          (kind 2/10/11), draw it again with the host painter (its lines, its
                          cursor) and compare the 117 × h box pixel for pixel.
  --h2-page <f:k>         hero2: at frame f, put the inventory window on page k (0..4) —
                          context word, cursor to cell 0, repaint flags. Needs --h2-pages 1.
  --h2-account <f>        hero2: at frame f, with the field menu alone on its ACCOUNT row, do
                          what the app's OK does there — `new bq(127)` once, `new q(menu, bag,
                          false)`, hang it on the menu, swap the title row (the app's own
                          `AccountState` steps, through the same JavaCall table).
  --h2-moverow <f[,..]>   hero2: at frame f, put the MOVE row (창고에넣기 / 가방에넣기) on the item
                          popup that is up over the inventory or the warehouse — the app's own
                          plan: in the array's slack, or a fresh array from the game's table.
  --h2-movego <f[,..]>    hero2: at frame f, do what OK on that row does — the record under the
                          window's cursor into the other bag (insert, then remove), then the app's
                          notice: the popup closed as j.b(I)V closes it and the game's own message
                          box (new ba(window, gm 83/84)) hung on the window. A WORN record (aa.f >= 0,
                          the game's bq.a(Lch;)Z) is refused into the warehouse with gm 82 instead;
                          a full destination gets the game's 빈공간이 없습니다 (itmtp[7]). `--h2-inv`
                          marks worn records WORN(slot n).
  --h2-qty <f[,..]>       hero2: at frame f, with a bag-window popup up, hang the game's own 개수 box
                          (new aq(window, 0, count)) in the popup's place — the app's open_quantity.
  --h2-cover <f[,..]>     hero2: at frame f, where the app says the game's box over a paged bag window
                          is (h2_modal_over — the page strip's cover), checked against this frame's
                          pixels: the rect's border ring should be one colour (the box's outline) and
                          the ring one pixel outside it not; and whether the tab rail meets it.
  --h2-acct-roundtrip <f> hero2: at frame f, serialise the warehouse bag (`bq.a()[B`), make a
                          guest byte[] of the blob, deserialise it into a SECOND `new bq(127)`
                          (`bq.a([B)V`) and print both — the app's save/load of the warehouse.
                          `--h2-move` and `--h2-inv` accept `w` for the warehouse bag.
  --h2-wh-give <f:T:I:N[,..]>  --h2-wh-in <file>  --h2-wh-out <file>
                          hero2 (★ 2026-10-09): the 계정창고 FILE — at frame f make the app's warehouse
                          bag (`new bq(127)`), load --h2-wh-in into it (`bq.a([B)V`, an exported
                          `account/bag`; without it the bag starts empty), give each item the
                          --h2-give way (`ch.a(T,I,1,1)` + `bq.a(Lch;B)I`), serialise it (`bq.a()[B`)
                          and write --h2-wh-out. Nothing is written if any step fails. Prints the
                          bag before and after. How a release phone's warehouse gets items
                          (the file goes up through `save_put/01035D0B/account/bag`).
  --h2-unworn <f>         hero2: at frame f, take every worn flag (aa.f >= 0) off the warehouse bag's
                          records — the app's load-time AccountState::clear_worn_flags. Make one to
                          take off with the old bug's move: --h2-move <f:a:w:IDX> on a WORN record.
  --h2-wear <f:IDX:SLOT[,..]> hero2 (★ 2026-10-20 밸런스 실측): at frame f, the inventory (ag.a) record at
                          Vector index IDX gets its worn flag aa.f = SLOT (0 장갑 · 1 투구 · 2 장신구 · 3 무기 ·
                          4 갑옷 · 5 방패/보조단검 · 6 신발; -1 = take off), and whatever other record held
                          that SLOT is taken off first. Nothing else moves until the next LOAD, where the
                          game's own a.n()V rebuilds the seven from these flags (so --h2-save and boot the
                          exported fs). A LV 99 bench in the guardian gear, without the EQUIPMENT window.
  --h2-acct-front <f>     hero2: at frame f, make a 재료가방 record (ch.a(29,0,..)), APPEND it to
                          the warehouse bag with the game's bq.a(Lch;I)Z — behind whatever is
                          there, as an old blob would have it — then apply the app's cell-0
                          rule (h2_bag_front_writes: the Vector's slots rotated) and print the
                          bag's types before and after. Needs --h2-account first.
  --h2-quickbar <f:g:v>   hero2: at frame f, seat value v (1..254) in guest quickbar slot g
                          (0..3) with the app's own plate-seat writes (h2_bind_writes: ag.j[g],
                          duplicates cleared, cooldown reset) — how a class skill gets onto a
                          save that has none registered, so 1/3/7/9 can be measured.
  --h2-reset <f:stat|skill|repair|elixir[:apply]>
                          hero2 (#22): at frame f, PRINT what a 망각의 구 would do — the
                          class and its base abilities, the live pools, and every write
                          the plan makes. With `:apply` it also writes them. Read-only
                          without it, which is how the offsets were measured before any
                          character field was ever written.
                          `repair` plans the app's 기본공격 repair instead (§8.10).
  --h2-combo-break <f[:old]>
                          DIAGNOSTIC: at frame f, clear 기본공격 stage 1 (a.p[0] and a.h) —
                          the damage the pre-2026-09-23 skill reset did, so `repair` has
                          something to fix. `:old` = that reset's exact footprint instead:
                          every positive combo stage to 0, `a.h` untouched.
  --h2-heal <f:type>      hero2: at frame f, select item type `type` in the bag the way `#` would
                          (bq.c + bq.a, the app's HealSelect writes) and press `0` — S7's plate press
  --h2-use <f:type>       hero2: from frame f, use item type `type` through the bag's own 사용하기
                          (the app's item-use drive: CLR, INVENTORY+OK, record+OK, 사용하기+OK,
                          CLR, CLR — one verified key per frame; 귀환서 stops at the game's
                          question, answer it with --keys) — ⑧'s plate press for 치료제 / 귀환서;
                          type 37 = the 비행서 plate (2026-10-21): stops at the scroll's popup and
                          writes the town list (the app's start_flight(None)) — pick with
                          --h2-flight <f:town>, or CLR
  --phases 1              host PHASE profiler (#9): a 1 kHz sampler bins the emulator
                          thread's own time into engine / svc:<category> / gc / paint /
                          idle, printed with the other gauges. Answers what `--profile`
                          cannot — it samples the GUEST pc, so host time is invisible to
                          it. Costs one busy-waiting core; measurement only.
  --gc <N>                collect garbage every Nth PAINTED FRAME (default 1 =
                          upstream, every frame). ⛔ NOT a GC switch: the guest's
                          own System.gc() is not gated by this, and hero2 calls
                          it — a --gc 0 run still destroys thousands of instances.
                          See wie_util::stats::GC_EVERY.
  --h2-save <frame>       hero2 only: call the game's own ag.l()V (save) at <frame>
  --audio-log <path>      every audio-sink event, one line each with the virtual clock in ms
                          (`on ch note vel` · `off` · `pc` · `cc` · `aso` = hard stop · `wave` ·
                          `musvol`) — what the app's synth would have been told, headless
  --h2-sfx <f:n>[,…]      hero2: at frame f, the game's own effect call br.a(B)V(n) — the path
                          every effect takes (it stops the song and re-issues it 3 s later).
                          Slots the map has not loaded play nothing (town: 0..=3)
  --audio-fm 1            the sink claims an FM synth, so the songs take the app's FM path
                          (their own voices) — logged as `fmv`/`fm`/`fmoff` in --audio-log
  --audio-wav <path>      implies --audio-fm 1: render that path with the app's FM synth on the
                          virtual clock and write it as a 44.1 kHz WAV when the run ends —
                          with the effects (the PCM clips) over it, converted by the app's
                          own upsampler at the default effect level
  --fs <root>             preload a real <root>/<aid>/fs/** tree into memory
  --fs-out <dir>          dump the in-memory filesystem to <dir>/<aid>/fs/** at the end
                          (guest writes stay in memory; the disk copy is never touched)
  --db <root>             mount a real save tree (READ-ONLY unless --db-rw 1)
  --db-rw <0|1>           let the GUEST write/erase that tree. Off by default.

Anchor-dependent (refused unless the archive hashes to a build whose guest addresses
were actually reverse engineered — see KNOWN_BUILDS):
  --poke <f:addr:hex>     write guest memory
  --host-save <f>         run the app's save chain
  --host-levelup <f>      run the app's level-up grant
  --find-fb 1  --map-check 1  --map-hunt 1  --char-dump <dir>
  --trace-state 1  --verify-offsets 1
";

/// The probe's mirror of the app's `emulator::java_call` gate: the same [`guest_build::JavaCall`]
/// table, the same per-build question (`GuestBuild::java_call_ok`), the same spelling of the
/// receiver (a handle is checked against the spec's class by the backend). `None` = refused.
include!("refine_probe.rs");

// ★★ 2026-10-16 정렬 · 일괄판매: the app's own driver (`emulator::BagTools`), through the probe's call door.
include!("../../wie_android/src/bag_tools_host.rs");

/// The probe's doors for [`BagTools`]; carries the identified build's hero2 table (`build()` is `main`'s own).
struct ProbeBagIo(Option<guest_build::Hero2Java>);

impl BagToolsIo for ProbeBagIo {
    fn hero2(&self) -> Option<guest_build::Hero2Java> {
        self.0
    }

    fn call(&mut self, emu: &mut dyn Emulator, what: guest_build::JavaCall, this: BtThis, args: &[JavaArg]) -> Option<Arc<GuestCallResult>> {
        match this {
            BtThis::None => h2_call(emu, what, None, args),
            BtThis::Handle(h) => h2_call(emu, what, Some(h), args),
            BtThis::Static(class, field, descriptor) => {
                if !GuestBuild::Hero2Ktf.java_call_ok(what) {
                    return None;
                }
                let spec = what.spec();
                let out = Arc::new(GuestCallResult::new());
                emu.call_java_method(JavaReceiver::Static { class, field, descriptor }, spec.method, spec.descriptor, args, out.clone());
                eprintln!("[java-call] {what:?} {}.{}{} on {class}.{field} with {args:?}", spec.class, spec.method, spec.descriptor);
                Some(out)
            }
        }
    }

    fn write(&mut self, emu: &mut dyn Emulator, what: guest_build::JavaWrite, addr: u32, bytes: &[u8]) -> bool {
        GuestBuild::Hero2Ktf.java_write_ok(what) && emu.write_memory(addr, bytes)
    }
}

fn h2_call(emu: &mut dyn Emulator, what: guest_build::JavaCall, this: Option<u32>, args: &[JavaArg]) -> Option<Arc<GuestCallResult>> {
    if !GuestBuild::Hero2Ktf.java_call_ok(what) {
        eprintln!("[java-call] {what:?} REFUSED by the gate");
        return None;
    }
    let spec = what.spec();
    let out = Arc::new(GuestCallResult::new());
    let receiver = match (spec.kind, this, spec.singleton) {
        (guest_build::CallKind::Static, None, _) => JavaReceiver::Class(spec.class),
        (guest_build::CallKind::Virtual, Some(handle), _) => JavaReceiver::Handle { handle, class: spec.class },
        (guest_build::CallKind::Virtual, None, Some((class, field, descriptor))) => JavaReceiver::Static { class, field, descriptor },
        (guest_build::CallKind::Constructor, None, _) => {
            emu.new_java_object(spec.class, spec.descriptor, args, out.clone());
            eprintln!("[java-call] {what:?} new {}{} with {args:?}", spec.class, spec.descriptor);
            return Some(out);
        }
        _ => {
            eprintln!("[java-call] {what:?}: receiver shape does not fit (kind={:?}, this={this:?})", spec.kind);
            return None;
        }
    };
    emu.call_java_method(receiver, spec.method, spec.descriptor, args, out.clone());
    eprintln!("[java-call] {what:?} {}.{}{} on {receiver:?} with {args:?}", spec.class, spec.method, spec.descriptor);
    Some(out)
}

/// [`h2_call`] without the per-call log line — `--h2-kill` makes hundreds of calls.
/// ★★ 2026-10-18: the app's `emulator::sync_bag_full_row` — before an OK (and before 오토루팅's pickup call) the game's
/// pickup refusal is pointed at 「재료가방이 가득 참」 when every drop under the player is a 재료가방 type, row 7 otherwise.
fn h2_sync_bag_full_row(emu: &mut dyn Emulator, f: u64) {
    let (Some(h2), Some(site)) = (GuestBuild::Hero2Ktf.hero2_java(), GuestBuild::Hero2Ktf.bag_full_row_site()) else { return };
    let stage_slot = GuestBuild::Hero2Ktf.hero2_save().map(|s| s.area.stage_slot).unwrap_or(0);
    let write = {
        let rd = |a: u32, n: usize| emu.read_memory(a, n);
        let row = guest_build::h2_loot_near(&rd, &h2, stage_slot)
            .map_or(guest_build::Hero2Java::ITEM_TYPE_ROW_BAG_FULL, |(_, near)| guest_build::h2_bag_full_row(&near, guest_build::h2_material_full_text(&rd).is_some()));
        guest_build::h2_bag_full_row_write(&rd, site, row).map(|w| (w, row))
    };
    if let Some(((at, bytes), row)) = write {
        let ok = emu.write_memory(at, &bytes);
        eprintln!("[h2-loot] f={f} pickup refusal row -> {row} (ok={ok})");
    }
}

fn h2_call_quiet(emu: &mut dyn Emulator, what: guest_build::JavaCall, this: Option<u32>, args: &[JavaArg]) -> Option<Arc<GuestCallResult>> {
    if !GuestBuild::Hero2Ktf.java_call_ok(what) {
        eprintln!("[java-call] {what:?} REFUSED by the gate");
        return None;
    }
    let spec = what.spec();
    let out = Arc::new(GuestCallResult::new());
    let receiver = match (spec.kind, this, spec.singleton) {
        (guest_build::CallKind::Virtual, Some(handle), _) => JavaReceiver::Handle { handle, class: spec.class },
        (guest_build::CallKind::Virtual, None, Some((class, field, descriptor))) => JavaReceiver::Static { class, field, descriptor },
        (guest_build::CallKind::Static, None, _) => JavaReceiver::Class(spec.class),
        _ => return h2_call(emu, what, this, args),
    };
    emu.call_java_method(receiver, spec.method, spec.descriptor, args, out.clone());
    Some(out)
}

/// The character's level (`char_layout().lv`, +0x5c), for `--h2-kill`'s before/after.
fn char_level(read: &impl Fn(u32, usize) -> Vec<u8>) -> Option<u32> {
    let h2 = GuestBuild::Hero2Ktf.hero2_java()?;
    let obj = guest_build::h2_static_handle(read, h2.char_slot)?;
    let fields = guest_build::h2_fields(read, obj)?;
    read(fields + 4 + 0x5c, 1).first().map(|b| u32::from(*b))
}

/// `--h2-honor` (2026-09-25): the player's 명예 and every equipped WEAPON's 요구 명예, with
/// the full attack pair (`bo.a()S` / `bo.b()S` = base + refine×3/2 — the item-detail
/// screen's numbers) and what the shipped `bo.c()S` / `bo.d()S` would leave of it
/// (`v × (10 − k + h) / 10` when `h < k`), plus whether the two penalty `blt`s are
/// patched right now (`guest_anchors::honor_penalty_off`).
///
/// Absolute field offsets (`dump_hero2_java.py`; every field is a 4-byte slot, `B`/`S`
/// sign-extended): the character `a` starts after `ad` 0x34 + `g` 0x28 = 0x5c —
/// `a.g:B` 명예 @0x60, `a.a:[Laa;` the equipment @0x12c. An item runs `ch` 0x20 → `aa`
/// 0x1c → `bu` 0x1c → `bo`: `ch.g:B` type @0x0 · `ch.h:B` id @0x4 · `aa.d:B` refine
/// @0x2c · `bu.k:B` 요구 명예 @0x3c · `bu.b:S` @0x54 · `bo.c:S` @0x60. Weapons are item
/// types 0..=4 (`itm/00`..`04`).
///
/// Returns every equipped weapon as `(equipment index, handle, field block)`, so the
/// caller can put the game's own getters on the same objects.
fn h2_honor_report(read: &impl Fn(u32, usize) -> Vec<u8>, f: u64) -> Vec<(u32, u32, u32)> {
    let mut weapons = Vec::new();
    let word = |a: u32| -> Option<i32> {
        let b = read(a, 4);
        (b.len() == 4).then(|| i32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    };
    let Some(h2) = GuestBuild::Hero2Ktf.hero2_java() else { return weapons };
    let Some(ch) = guest_build::h2_static_handle(read, h2.char_slot).and_then(|o| guest_build::h2_fields(read, o)) else {
        eprintln!("[h2-honor] f={f} no character yet");
        return weapons;
    };
    let honour = word(ch + 4 + 0x60).unwrap_or(-1);
    let patched = GuestBuild::Hero2Ktf.honor_penalty_off().iter().all(|&(a, _, p)| {
        let b = read(a, 2);
        b.len() == 2 && u16::from_le_bytes([b[0], b[1]]) == p
    });
    eprintln!("[h2-honor] f={f} player 명예 = {honour} · penalty {}", if patched { "OFF (patched)" } else { "as shipped" });
    let Some((base, len)) = word(ch + 4 + 0x12c).and_then(|h| guest_build::h2_array(read, h as u32)) else {
        eprintln!("[h2-honor] f={f} no equipment array");
        return weapons;
    };
    for i in 0..len {
        let Some(handle) = word(base + 4 * i).filter(|h| *h != 0).map(|h| h as u32) else { continue };
        let Some(item) = guest_build::h2_fields(read, handle) else { continue };
        let at = |off: u32| word(item + 4 + off).unwrap_or(0);
        let (kind, id) = (at(0x0), at(0x4));
        if !(0..=4).contains(&kind) {
            eprintln!("[h2-honor]   [{i}] type={kind:2} id={id:3} (not a weapon)");
            continue;
        }
        let (refine, req) = (at(0x2c), at(0x3c));
        let full_a = at(0x60) + refine * 3 / 2;
        let full_b = at(0x54) + refine * 3 / 2;
        let cut = |v: i32| if honour >= req { v } else { v * (10 - req + honour) / 10 };
        eprintln!(
            "[h2-honor]   [{i}] type={kind:2} id={id:3} refine={refine} 요구명예={req} · full {full_a}/{full_b} · shipped rule {}/{}{}",
            cut(full_a),
            cut(full_b),
            if honour < req { format!("  (−{}%)", (req - honour) * 10) } else { String::new() }
        );
        weapons.push((i, handle, item));
    }
    weapons
}

/// `--h2-kill`'s report: per-kill rates by kind, then every (type, id) seen.
fn print_kill_tally(t: &KillTally, lv_before: Option<u32>, lv_after: Option<u32>, shipped: bool) {
    let k = f64::from(t.kills.max(1));
    let kind = |ty: i8| -> &'static str {
        match ty {
            0..=6 | 8..=12 => "gear",
            20 => "제련석",
            24..=26 => "물약",
            34 => "스펠카드",
            _ => "other",
        }
    };
    let mut by: std::collections::BTreeMap<&str, u32> = std::collections::BTreeMap::new();
    for ((ty, _), c) in &t.items {
        *by.entry(kind(*ty)).or_default() += c;
    }
    eprintln!(
        "[h2-kill] DONE {} kills ({}) failed={} · level {:?} -> {:?}",
        t.kills,
        if shipped { "SHIPPED rules" } else { "patched rules" },
        t.failed,
        lv_before,
        lv_after
    );
    eprintln!("[h2-kill]   gold: {} drop(s) = {:.1}% per kill, {} total", t.gold_drops, 100.0 * f64::from(t.gold_drops) / k, t.gold_total);
    for (name, c) in &by {
        eprintln!("[h2-kill]   {name:<6}: {c:5} = {:5.1}% per kill", 100.0 * f64::from(*c) / k);
    }
    let line: Vec<String> = t.items.iter().map(|((ty, id), c)| format!("{ty}:{id}x{c}")).collect();
    eprintln!("[h2-kill]   items: {}", line.join(" "));
}

/// `--h2-give` in flight: the factory call, then the placement.
enum GiveStep {
    New { t: i8, i: i8, n: i8, out: Arc<GuestCallResult>, since: u64 },
    Add { t: i8, i: i8, n: i8, rec: u32, out: Arc<GuestCallResult>, since: u64 },
}

/// `<frame>:<type>:<id>:<n>[,<type>:<id>:<n>..]` — the item list of `--h2-give` and `--h2-wh-give`.
fn parse_give_spec(v: &str) -> Option<(u64, Vec<(i8, i8, i8)>)> {
    let (f, spec) = v.split_once(':')?;
    let f = f.parse::<u64>().ok()?;
    let mut items = vec![];
    for one in spec.split(',') {
        let p: Vec<Option<i8>> = one.split(':').map(|x| x.parse::<i8>().ok()).collect();
        match p.as_slice() {
            [Some(t), Some(i), Some(n)] if *t >= 0 && *i >= 0 && *n > 0 => items.push((*t, *i, *n)),
            _ => return None,
        }
    }
    (!items.is_empty()).then_some((f, items))
}

/// ★ 2026-10-09: `--h2-wh-give` in flight — the 계정창고 FILE, the app's own steps: `new bq(127)`
/// (`AccountState::ensure_bags`), the blob on disk loaded into it (`bq.a([B)V`, `start_load`),
/// each item given the `--h2-give` way (`ch.a(T,I,1,1)` + `bq.a(Lch;B)I`), the bag serialised
/// (`bq.a()[B`, the save edge's `Serialize`) back to a file — how a release phone's warehouse
/// gets items (the file goes up through `save_put/…/account/bag`).
enum WhStep {
    NewBag { out: Arc<GuestCallResult> },
    Array { out: Arc<GuestCallResult>, bag: u32 },
    Load { out: Arc<GuestCallResult>, bag: u32, arr: u32 },
    Record { out: Arc<GuestCallResult>, bag: u32, t: i8, i: i8, n: i8 },
    Add { out: Arc<GuestCallResult>, bag: u32, t: i8, i: i8, n: i8, rec: u32 },
    Serialize { out: Arc<GuestCallResult>, bag: u32 },
}

/// The warehouse file's next step once `bag` holds what is on disk: the next item, or —
/// the queue empty — the serialisation.
fn wh_next(emu: &mut dyn Emulator, bag: u32, queue: &mut std::collections::VecDeque<(i8, i8, i8)>, f: u64) -> Option<WhStep> {
    match queue.pop_front() {
        Some((t, i, n)) => {
            let args = [JavaArg::Int(t as i32), JavaArg::Int(i as i32), JavaArg::Int(1), JavaArg::Int(1)];
            let step = h2_call(emu, guest_build::JavaCall::RecordNew, None, &args).map(|out| WhStep::Record { out, bag, t, i, n });
            if step.is_none() {
                eprintln!("[h2-wh] f={f} type {t} id {i}: RecordNew refused — nothing written");
            }
            step
        }
        None => h2_call(emu, guest_build::JavaCall::BagSerialize, Some(bag), &[]).map(|out| WhStep::Serialize { out, bag }),
    }
}

/// One line per record of a bag — `--h2-wh-give`'s before and after.
fn wh_print(tag: &str, f: u64, bag: Option<&guest_build::H2Inventory>) {
    match bag {
        None => eprintln!("[h2-wh] f={f} {tag}: the bag could not be read"),
        Some(b) => {
            eprintln!("[h2-wh] f={f} {tag}: {} record(s)", b.items.len());
            for (k, it) in b.items.iter().enumerate() {
                eprintln!("[h2-wh]   [{k:3}] type={:2} id={:3} count={:3} f={}", it.kind, it.id, it.count, it.f);
            }
        }
    }
}

/// `--h2-account` in flight: the bag once, then the window. `--h2-acct-front`: the
/// 재료가방 record from the factory, then its append.
enum AccountStep {
    NewBag { out: Arc<GuestCallResult>, menu: guest_build::H2Win },
    NewWindow { out: Arc<GuestCallResult>, menu: guest_build::H2Win },
    FrontNew { out: Arc<GuestCallResult> },
    FrontInsert { out: Arc<GuestCallResult>, rec: u32 },
}

/// `--h2-moverow` / `--h2-movego` in flight.
enum MrStep {
    Array { out: Arc<GuestCallResult>, popup: u32, len: u32 },
    Insert { out: Arc<GuestCallResult>, src: u32, index: u32, rec: u32, to_account: bool },
    Remove { out: Arc<GuestCallResult>, src: u32, rec: u32, to_account: bool },
    /// ★ 2026-09-26: the app's notice — `new ba(window, text)`, to be hung on `window` in
    /// place of `replaces` (`emulator::AccountState::notice`).
    Notice { out: Arc<GuestCallResult>, window: u32, replaces: Option<u32> },
    /// ★ 2026-09-29: the app's 개수 box — `new aq(window, 0, count)`, to be hung in place of
    /// the popup over `record` (`emulator::AccountState::{open_quantity, QtyNew}`).
    Qty { out: Arc<GuestCallResult>, window: guest_build::H2InvWin, record: u32 },
}

/// ★ 2026-09-26: the app's `AccountState::notice` — ask for the game's own message box with
/// `text` (a `byte[]` handle from one of the game's tables) over `window`.
fn h2_probe_notice(emu: &mut dyn Emulator, window: u32, replaces: Option<u32>, text: Option<u32>, f: u64) -> Option<MrStep> {
    let Some(text) = text else {
        eprintln!("[h2-movego] f={f} notice has no text row — was the asset regenerated (gm 82..=89)?");
        return None;
    };
    let out = h2_call(emu, guest_build::JavaCall::MessageBoxNew, None, &[JavaArg::Object(window), JavaArg::Object(text)])?;
    Some(MrStep::Notice { out, window, replaces })
}

/// `--h2-acct-roundtrip` in flight.
enum RtStep {
    Serialize { out: Arc<GuestCallResult> },
    Array { out: Arc<GuestCallResult>, blob: Vec<u8> },
    NewBag { out: Arc<GuestCallResult>, arr: u32, blob: Vec<u8> },
    Load { out: Arc<GuestCallResult>, arr: u32, bag2: u32, blob: Vec<u8> },
}

/// `--h2-kill` in flight (#20/#21): find the monster the character faces, then run the
/// game's own death routine on it again and again, tallying what each call drops.
enum KillStep {
    /// `a.a()Lbd;` on the character.
    Seek { out: Arc<GuestCallResult> },
    /// `bd.q()V` on `mon`; `before` = the ground list's element handles before the call.
    Kill { mon: u32, out: Arc<GuestCallResult>, before: Vec<u32> },
}

/// `--h2-kill`'s running tally.
#[derive(Default)]
struct KillTally {
    kills: u32,
    gold_drops: u32,
    gold_total: u64,
    /// (type, id) -> count, for everything that is not gold.
    items: std::collections::BTreeMap<(i8, i8), u32>,
    /// Calls that failed (the monster went away, an exception).
    failed: u32,
}

/// `--h2-skill` in flight (#17): one call at a time from a fixed queue.
struct SkillProbe {
    queue: std::collections::VecDeque<(guest_build::JavaCall, i32)>,
    cur: Option<(guest_build::JavaCall, i32, Arc<GuestCallResult>)>,
}

/// ★ 세이브 슬롯 5 (2026-09-27): the app's own install order (`emulator::SaveSlots`) —
/// `ag` initialized early, the two five-name tables, then `ag.k()V`'s halfwords, then the
/// slot screen's once its previews allow. One step per frame, from frame 1.
#[derive(Default)]
/// ★ 2026-10-08 가디언: the app's `emulator::ModeTables` order, every frame before the save-slot install —
/// the three `movs r1,#3` → `#4` (before `ag` / `co` initialise), the labels, the fourth cells once an array
/// exists four long, then the round clear's `#2` once all three read ready. A table that reads three long
/// (the initialiser beat the halfword) switches it off for the run, as the app does.
struct GuardianTablesInstall {
    tables: (bool, bool),
    labels: (bool, bool),
    open: (bool, bool),
    done: bool,
    off: bool,
    seen: Option<guest_build::H2ModeTables>,
    /// ★ 2026-10-17: the app's `co` initialisation (`emulator::ModeTables::init_colours`) — `--h2-colour-init 0` leaves
    /// `co` to the game (its first reader is the slot screen's own paint: GUARDIAN black on the first visit).
    colour_off: bool,
    colour_init: Option<Arc<GuestCallResult>>,
    colour_tries: u8,
    colour_retry_at: u64,
}

impl GuardianTablesInstall {
    /// One halfword list, once: `(done, said)` as the app's `HalfwordPatch`.
    fn halfwords(emu: &mut dyn Emulator, state: &mut (bool, bool), name: &str, sites: &[(u32, u16, u16)], f: u64) {
        if state.0 {
            return;
        }
        let (todo, already, total) = {
            let rd = |a: u32, n: usize| emu.read_memory(a, n);
            guest_build::h2_halfword_writes(&rd, sites)
        };
        if total == 0 {
            return;
        }
        if todo.is_empty() && already == 0 {
            if !state.1 {
                state.1 = true;
                eprintln!("[h2-guardian] f={f} {name}: no site reads as shipped — NOT applied");
            }
            return;
        }
        let mut landed = already;
        for (a, bytes) in &todo {
            if emu.write_memory(*a, bytes) {
                landed += 1;
            }
        }
        state.0 = landed == total;
        eprintln!("[h2-guardian] f={f} {name}: {landed}/{total} halfword(s){}", if state.0 { "" } else { "  ** NOT APPLIED **" });
    }

    /// The app's order: `co` from the host once the `#4`s are in and it is untouched — retried, eight tries.
    fn init_colours(&mut self, emu: &mut dyn Emulator, colour: guest_build::H2ModeTable, f: u64) {
        let class = guest_build::GuardianPatch::COLOUR_CLASS;
        if let Some(out) = self.colour_init.take() {
            match out.poll() {
                None => self.colour_init = Some(out),
                Some(r) => {
                    eprintln!("[h2-guardian] f={f} init_java_class({class}) -> {r:?}");
                    if r.is_err() {
                        self.colour_retry_at = f + 10;
                    }
                }
            }
            return;
        }
        if self.colour_off || !matches!(colour, guest_build::H2ModeTable::NotYet) || !self.tables.0 || f < self.colour_retry_at || self.colour_tries >= 8 {
            return;
        }
        self.colour_tries += 1;
        let out = Arc::new(GuestCallResult::new());
        emu.init_java_class(class, out.clone());
        self.colour_init = Some(out);
        eprintln!("[h2-guardian] f={f} {class} not initialized — init_java_class({class}) issued");
    }

    fn poll(&mut self, emu: &mut dyn Emulator, p: &guest_build::GuardianPatch, f: u64) {
        if self.done || self.off {
            return;
        }
        Self::halfwords(emu, &mut self.tables, "mode tables", guest_build::GuestBuild::Hero2Ktf.guardian_mode_tables(), f);
        Self::halfwords(emu, &mut self.labels, "labels", guest_build::GuestBuild::Hero2Ktf.guardian_labels(), f);
        let t = {
            let rd = |a: u32, n: usize| emu.read_memory(a, n);
            guest_build::h2_mode_tables(&rd, p)
        };
        if self.seen != Some(t) {
            eprintln!("[h2-guardian] f={f} tables {t:?}");
            self.seen = Some(t);
        }
        self.init_colours(emu, t.colour, f);
        if t.refused() {
            self.off = true;
            eprintln!("[h2-guardian] f={f} a mode table is not four long — the round clear stays at HELL this run");
            return;
        }
        for (at, bytes) in t.writes() {
            let ok = emu.write_memory(at, &bytes);
            eprintln!("[h2-guardian] f={f} fourth cell {at:#x} <- {bytes:02x?} ok={ok}");
        }
        if !t.ready() {
            return;
        }
        if !self.tables.0 {
            if self.tables.1 {
                self.off = true;
                eprintln!("[h2-guardian] f={f} the arrays read four long without this install's halfwords — not opening the round clear");
            }
            return;
        }
        Self::halfwords(emu, &mut self.open, "round clear", guest_build::GuestBuild::Hero2Ktf.guardian_mode_open(), f);
        if self.open.0 {
            self.done = true;
            eprintln!("[h2-guardian] f={f} mode 3 OPEN: the tables are four long and a HELL clear goes on to GUARDIAN");
        } else if self.open.1 {
            self.off = true;
        }
    }
}

#[derive(Default)]
struct SlotsInstall {
    init: Option<Arc<GuestCallResult>>,
    arrays: Option<(Arc<GuestCallResult>, Arc<GuestCallResult>)>,
    names_in: bool,
    table_in: bool,
    done: bool,
    off: bool,
    retry_at: u64,
    said_waiting: bool,
}

impl SlotsInstall {
    fn poll(&mut self, emu: &mut dyn Emulator, p: &guest_build::SaveSlotsPatch, h2: &guest_build::Hero2Java, f: u64) {
        use guest_build::{H2SaveNames, SaveSlotsPatch};
        if self.done || self.off || f < self.retry_at {
            return;
        }
        if let Some(out) = self.init.take() {
            match out.poll() {
                None => self.init = Some(out),
                Some(r) => {
                    eprintln!("[h2-slots] f={f} init_java_class({}) -> {r:?}", p.class);
                    if r.is_err() {
                        self.retry_at = f + 10;
                    }
                }
            }
            return;
        }
        if let Some((g, n)) = self.arrays.take() {
            match (g.poll(), n.poll()) {
                (Some(g), Some(n)) => {
                    let (Ok(g), Ok(n)) = (g, n) else {
                        eprintln!("[h2-slots] f={f} String[] creation failed: {g:?} {n:?}");
                        for h in [g, n].into_iter().flatten() {
                            emu.release_java_object(h);
                        }
                        self.retry_at = f + 10;
                        return;
                    };
                    let rd = |a: u32, l: usize| emu.read_memory(a, l);
                    let fresh = guest_build::h2_save_names_are(&rd, g, SaveSlotsPatch::PROG_PREFIX, SaveSlotsPatch::SLOTS)
                        && guest_build::h2_save_names_are(&rd, n, SaveSlotsPatch::CHAR_PREFIX, SaveSlotsPatch::SLOTS);
                    let state = guest_build::h2_save_names_state(&rd, p);
                    eprintln!("[h2-slots] f={f} made {g:#x} {:?} / {n:#x} {:?} fresh={fresh}, tables {state:?}", guest_build::h2_string_array(&rd, g), guest_build::h2_string_array(&rd, n));
                    if fresh && matches!(state, H2SaveNames::Shipped { .. }) {
                        let mut ok = true;
                        for (a, bytes) in guest_build::h2_save_names_writes(p, g, n) {
                            ok &= emu.write_memory(a, &bytes);
                        }
                        let rd = |a: u32, l: usize| emu.read_memory(a, l);
                        eprintln!("[h2-slots] f={f} tables written ok={ok} -> {:?}", guest_build::h2_save_names_state(&rd, p));
                    }
                    emu.release_java_object(g);
                    emu.release_java_object(n);
                }
                // Not both done yet (`poll` does not consume — a finished one keeps its value).
                _ => self.arrays = Some((g, n)),
            }
            return;
        }
        if self.names_in {
            self.code(emu, p, h2, f);
            return;
        }
        let state = {
            let rd = |a: u32, l: usize| emu.read_memory(a, l);
            guest_build::h2_save_names_state(&rd, p)
        };
        match state {
            H2SaveNames::Uninit => {
                let out = Arc::new(GuestCallResult::new());
                emu.init_java_class(p.class, out.clone());
                eprintln!("[h2-slots] f={f} ag not initialized — init_java_class({}) issued", p.class);
                self.init = Some(out);
            }
            H2SaveNames::Shipped { prog, chars } => {
                eprintln!("[h2-slots] f={f} shipped tables {prog:#x} / {chars:#x} — making the five-name ones");
                let names = |prefix| guest_build::h2_save_file_names(prefix, SaveSlotsPatch::SLOTS);
                let (g, n) = (names(SaveSlotsPatch::PROG_PREFIX), names(SaveSlotsPatch::CHAR_PREFIX));
                let (gout, nout) = (Arc::new(GuestCallResult::new()), Arc::new(GuestCallResult::new()));
                emu.new_java_string_array(&g.iter().map(String::as_str).collect::<Vec<_>>(), gout.clone());
                emu.new_java_string_array(&n.iter().map(String::as_str).collect::<Vec<_>>(), nout.clone());
                self.arrays = Some((gout, nout));
            }
            H2SaveNames::Extended => {
                self.names_in = true;
                eprintln!("[h2-slots] f={f} names extended");
                self.code(emu, p, h2, f);
            }
            H2SaveNames::NotYet => {}
            H2SaveNames::Foreign => {
                eprintln!("[h2-slots] f={f} tables read FOREIGN — left alone (three slots)");
                self.off = true;
            }
        }
    }

    /// Steps 3 and 4 (the app's `SaveSlots::patch_code`).
    fn code(&mut self, emu: &mut dyn Emulator, p: &guest_build::SaveSlotsPatch, h2: &guest_build::Hero2Java, f: u64) {
        let apply = |emu: &mut dyn Emulator, sites: &[(u32, u16, u16)], what: &str| -> bool {
            let (todo, already, total) = {
                let rd = |a: u32, n: usize| emu.read_memory(a, n);
                guest_build::h2_halfword_writes(&rd, sites)
            };
            let mut landed = already;
            for (a, bytes) in &todo {
                if emu.write_memory(*a, bytes) {
                    landed += 1;
                }
            }
            eprintln!("[h2-slots] f={f} {what}: {landed}/{total} halfword(s)");
            total > 0 && landed == total
        };
        if !self.table_in {
            self.table_in = apply(emu, p.table_sites, "ag.k previews");
            if !self.table_in {
                self.off = true;
                return;
            }
        }
        let ready = {
            let rd = |a: u32, l: usize| emu.read_memory(a, l);
            guest_build::h2_save_screen_ready(&rd, h2, p)
        };
        if !ready {
            if !self.said_waiting {
                self.said_waiting = true;
                let rd = |a: u32, l: usize| emu.read_memory(a, l);
                eprintln!("[h2-slots] f={f} slot screen NOT yet (previews {:?}) — waiting for the next title", guest_build::h2_save_previews(&rd, p));
            }
            self.retry_at = f + 10;
            return;
        }
        self.done = apply(emu, p.screen_sites, "slot screen");
        if !self.done {
            self.off = true;
        }
    }
}

/// `--h2-exp-gain` in flight: the game's own `a.d(I)V`, and what EXP read before it.
struct ExpGainStep {
    out: Arc<GuestCallResult>,
    before: u32,
    beast_before: Option<u32>,
    n: i32,
    since: u64,
}

/// `--h2-exp-gain`: the character's EXP (`a.c:I`, `char_layout().exp`) and its beast's
/// (`bn.a:I`: `bn` extends `ad` (0x34 B) and declares `a:I` at +0x24, so absolute +0x58 —
/// `dump_hero2_java.py --json`, 2026-09-23). `None` when there is no character yet. The
/// beast is read because `a.d(I)V` hands it the SAME amount (`bn.a(I)V` @0x14fa9a), so
/// a redirect that scales the argument must show on both numbers or it is not the
/// argument it scaled.
fn exp_pair(build: GuestBuild, read: &impl Fn(u32, usize) -> Vec<u8>) -> (Option<u32>, Option<u32>) {
    let rd_u = |a: u32| -> Option<u32> {
        let b = read(a, 4);
        (b.len() == 4).then(|| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    };
    let cp = char_base(build, read);
    let (Some(l), Some(h2)) = (build.char_layout(), build.hero2_java()) else {
        return (None, None);
    };
    if cp == 0 {
        return (None, None);
    }
    const BEAST_EXP_OFF: u32 = 0x58;
    let beast = rd_u(cp + h2.beast_off)
        .filter(|h| guest_build::h2_in_java_heap(*h))
        .and_then(|h| guest_build::h2_fields(read, h))
        .and_then(|fields| rd_u(fields + 4 + BEAST_EXP_OFF));
    (rd_u(cp + l.exp), beast)
}

/// `--h2-move` in flight: the insert, then — only after it said yes — the removal.
/// With a count (`--h2-move f:a:w:3:5`) it is the PARTIAL move instead: a fresh record of
/// the same `(type, id)`, merged into the destination for as many as it will take, and
/// exactly that many consumed from the source. The app's own three steps
/// (`emulator::AccountState`), so the protocol is settled here and not on the phone.
enum MoveStep {
    Insert { src: u32, idx: u8, rec: u32, out: Arc<GuestCallResult>, since: u64 },
    Remove { rec: u32, out: Arc<GuestCallResult>, since: u64 },
    SplitNew { src: u32, rec: u32, dst: u32, n: u8, out: Arc<GuestCallResult>, since: u64 },
    SplitAdd { src: u32, rec: u32, fresh: u32, n: u8, out: Arc<GuestCallResult>, since: u64 },
    SplitConsume { rec: u32, placed: i32, out: Arc<GuestCallResult>, since: u64 },
}

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .without_time()
        .init();

    // --- args ---
    // ⛔ NO DEFAULT for --zip. It used to be `d:/tmp/hero3_fixed.zip`, so forgetting
    // the flag did not fail — it silently booted a DIFFERENT GAME and reported its
    // numbers as this one's. (That file is gone from this disk as of 2026-09-04, but a
    // default that depends on a path not existing is not a safety property.)
    let mut zip: Option<PathBuf> = None;
    // Was `d:/tmp/probe` — one machine's path. The OS temp dir is the same idea
    // (scratch that nobody backs up) without the single-machine assumption.
    let mut out = std::env::temp_dir().join("wie-probe");
    let (mut w, mut h, mut frames) = (240u32, 320u32, 400u64);
    let mut ms_per_frame = 20u64;
    // --fs mounts a real `<root>/<aid>/fs/**` tree INTO MEMORY (see `MemFs::preload`).
    // Unlike --db it needs no write switch: the guest's writes never reach the disk.
    let mut fs_root: Option<PathBuf> = None;
    // `--fs-out <dir>` writes the in-memory filesystem out when the run ends, so a save
    // the guest produced during the run can be booted by the NEXT run. The round trip
    // (host writes it -> the game loads it -> the gauges match) is a stronger proof than
    // comparing bytes, because it is the game itself that judges the record.
    let mut fs_out: Option<PathBuf> = None;
    let mut db_root: Option<PathBuf> = None;
    // --db mounts a REAL directory tree, and the guest drives it: `FsDbRepo::delete`
    // is `fs::remove_dir_all` and `FsDb::{add,set,delete}` are `fs::write`/
    // `fs::remove_file`. A plain `--keys` playthrough is enough to overwrite or erase a
    // save tree pulled off the device, so --db is READ-ONLY unless --db-rw 1 says
    // otherwise. Refused writes are logged, not silently dropped.
    let mut db_rw = false;
    let mut find_fb = false;
    let mut map_check = false;
    // --map-hunt: locate the per-map object and its tile arrays on a build whose map
    // chain has not been reverse engineered yet. Structural, not address-based: it
    // walks every static/CHAR word, tries both pointer conventions, and reports the
    // ones that land on something map-SHAPED (plausible tile w/h at +0x48/+0x4c with
    // two readable w*h byte arrays at +0x60/+0x68). See MIGRATION_LGT.md §4 item 1.
    let mut map_hunt = false;
    let mut profile = false;
    let mut hle = true;
    let mut gc_every: u64 = 1;
    // `--phases`: run the host PHASE sampler (roadmap #9). See the block where it is
    // started for why it busy-waits rather than sleeping.
    let mut phases = false;
    // `--audio-log <path>` (2026-10-01, 배경음 A): see [`AUDIO_LOG`].
    let mut audio_log: Option<PathBuf> = None;
    // `--audio-fm 1` / `--audio-wav <path>` (배경음 B3): see [`AUDIO_FM`] / [`AUDIO_WAV`].
    let mut audio_fm = false;
    let mut audio_wav: Option<PathBuf> = None;
    // `--h2-sfx <f:n>[,…]` (2026-10-01, 배경음 A): at frame f, the game's own effect call
    // `br.a(B)V(n)` — the path every hero2 effect takes (`HERO2_GUEST_RE.md` §8.37). A slot
    // the map has not loaded plays nothing (slot 1 = town loads 0..=3).
    let mut h2_sfx: Vec<(u64, i32)> = vec![];
    let mut char_dump: Option<PathBuf> = None;
    let mut trace_state = false;
    // ★ `--gauges N`: print the player gauges every N frames, decoded through the
    // per-build `char_layout()` table instead of inline constants. Works on any build
    // that HAS a layout, which is what makes it the end-to-end check for a new anchor:
    // if the numbers do not match the game's own STATUS screen, the anchor is wrong.
    let mut gauges: u64 = 0;
    // --verify-offsets 1 — on every --dump frame, print the MIGRATION_LGT.md §2.11
    // matrix in one shot: the VERIFIED reference fields (gauges / window / state, so
    // the base can be confirmed against the on-screen status numbers) and then the five
    // 🔶 "silent-bug watch" offsets (quickbar cells, badges, secondary weapon, save
    // byte, skill-cast) decoded exactly as items.rs reads them. This is the whole point
    // of the audit made runnable: a local session opens the status screen and compares
    // these labelled values to the screen, closing all five 🔶 items in one command
    // instead of doing offset arithmetic by hand on a raw --char-dump (which is itself
    // a silent-bug risk). Reads are build-aware via the shared guest_build table.
    let mut verify_offsets = false;
    // --dump-mem base:len[,base:len...] — snapshot raw guest memory at every --dump
    // frame into <out>/mem_<frame>_<base>.bin. The RE workhorse: diff two frames'
    // snapshots to find the word that changed (HP dropped, cursor moved, …) with no
    // device. Ranges are hex; a good default for the LGT ELF is its statics:
    //   --dump-mem 0x1400000:0x604,0x1500000:0x12958
    let mut dump_mem: Vec<(u32, usize)> = vec![];
    let mut dumps: Vec<u64> = vec![];
    let mut dump_passive = false;
    // --poke frame:addr:hexbytes[,…] — write guest memory at the start of a frame.
    //
    // The bench for a candidate guest CODE patch before anything ships it: a probe run
    // is deterministic, so poking a patch in and diffing the frames (and the CHAR
    // record) against an unpoked run says exactly what the patch does and what else it
    // disturbs. Used to qualify the LGT HUD-skip branch (MIGRATION_LGT.md §4 item 2);
    // the app's own patch functions verify the ORIGINAL bytes before writing, and this
    // prints them so that check can be written down from measurement.
    let mut pokes: Vec<(u64, u32, Vec<u8>)> = vec![];
    // ★ hero2 popup A/B (2026-09-17): `--h2-relabel <frame>` applies the app's own
    // 퀵슬롯등록/해제 rewrite to the popup that is open at that frame; `--h2-commit
    // <frame>:<guest>` restores the native shape, points the popup's cursor at guest
    // slot `<guest>`'s `N번에 등록` row and presses OK — the worker's commit, replayed
    // with the shared `hero2java.rs` producers. Both walk the chain AT THE FRAME, since
    // heap addresses differ from run to run.
    let mut h2_relabel: Option<u64> = None;
    let mut h2_commit: Option<(u64, u8)> = None;
    // `--h2-tap <frame>:<fx>:<fy>[,…]`: route a framebuffer tap through the app's own
    // `h2_tap` at that frame and perform what it says (cursor writes / OK / tab key).
    let mut h2_taps: Vec<(u64, u32, u32)> = vec![];
    // ★ `--h2-save <frame>` — the host-driven save A/B (2026-09-19). At that frame the
    // probe calls hero2's own `ag.l()V` through the JVM, exactly as the app's SAVE
    // button will, and the `[fs] write` log then says whether the game wrote the same
    // two records it writes when the player walks SYSTEM -> 세이브 -> 예 by hand.
    let mut h2_save_frames: Vec<u64> = vec![];
    let mut h2_save_pending: Vec<(u64, std::sync::Arc<wie_backend::GuestCallResult>)> = vec![];
    // The attribute grid last printed, so a map change reprints and a static map does not.
    let mut last_map_sig: Option<u64> = None;
    // `--h2-inv <frame>[,…]`: the inventory walk (`h2_inventory`) printed at those frames.
    let mut h2_inv: Vec<(u64, u8)> = vec![];
    // `--h2-cap <frame>:<n>`: raise (or lower) `bq.b` on the main bag at that frame.
    let mut h2_cap: Option<(u64, i8)> = None;
    // 2026-10-03: `--h2-iaido-unlock <frame>` — the app's 발도 unlock plan (`h2_iaido_unlock`), applied.
    let mut h2_iaido_unlock: Option<u64> = None;
    // 2026-10-06: `--h2-born-repair <frame>` — the app's born-skill repair (`h2_born_skill_repair`: 나이트 중장비 ·
    //   로그 심안 = a.p[12] held at 1, what 독 연마 held refunded once — the receipt as the app keeps it), applied at that frame.
    let mut h2_born_repair: Vec<u64> = vec![];
    // 2026-10-05 일풍격: `--h2-ilpung 0` · `--h2-ilpung-unlock <frame>` · `--h2-skill-set <f:i:lv>`.
    let mut h2_ilpung = true;
    let mut h2_ilpung_unlock: Option<u64> = None;
    // 2026-10-07 버서커 분노태세 ON/OFF · 도발: `--h2-berserker 0`.
    let mut h2_berserker = true;
    // ★ 2026-10-09: `--h2-fury-gauge <f:g>` sets the berserker block's 격분 gauge (and the hits behind it) at frame f — the
    //   explosion's tests without forty scripted hits. Only the host block's own words.
    let mut h2_fury_gauge: Option<(u64, u32)> = None;
    // ★ 2026-10-09 the awakenings' voices: `--h2-voice 0` = the game's 기합, no cues.
    let mut h2_voice = true;
    // ★ 2026-10-08 밤 가디언 장비의 개성 옵션: `--h2-gear 0` skips the block, `--h2-gear-opts <f:mask>` forces OPTS.
    let mut h2_gear = true;
    let mut h2_gear_wear: Vec<(u64, Vec<(u8, u8)>)> = vec![];
    // 2026-10-07 (2) 각성기: `--h2-awaken-unlock <frame>` · `--h2-bodyslam <frame>` · `--h2-damage-cap 0`.
    let mut h2_awaken_unlock: Vec<u64> = vec![];
    let mut h2_bodyslam: Vec<u64> = vec![];
    let mut h2_damage_cap = true;
    // ★★ 2026-10-16 `--h2-hp-int 0`: keep the five other monster HP stores' 16-bit wraps (GuestBuild::monster_hp_int).
    let mut h2_hp_int = true;
    let mut h2_skill_set: Vec<(u64, u32, i8)> = vec![];
    // `--h2-give <frame>:<type>:<id>:<n>[,…]`: the game's own give-item path, driven.
    let mut h2_give: Option<(u64, Vec<(i8, i8, i8)>)> = None;
    // ★ 2026-10-09: `--h2-wh-give` / `--h2-wh-in` / `--h2-wh-out` — the 계정창고 file: load, give, serialise.
    let mut h2_wh_give: Option<(u64, Vec<(i8, i8, i8)>)> = None;
    let mut h2_wh_in: Option<Vec<u8>> = None;
    let mut h2_wh_out: Option<std::path::PathBuf> = None;
    // `--h2-move <frame>:<src>:<dst>:<idx>`: one identity-preserving record move.
    let mut h2_move: Vec<(u64, u8, u8, u8, Option<u8>)> = vec![];
    // ★ 2026-10-13 `--h2-fold <frame>:<bag>`: the warehouse load's fold (`AccountState::fold_next`) on a bag.
    let mut h2_fold: Vec<(u64, u8)> = vec![];
    // `--h2-pages 1`: install the paging patch; `--h2-page <frame>:<k>`: select a page.
    let mut h2_pages = false;
    // ★★ 2026-10-20 `--h2-use-page 0`: with --h2-pages, leave 사용하기's `a.g(B)V` site as shipped (the A/B of
    // `guest_anchors::ItemUsePagePatch` — a potion on page 2..5 then uses page 1's record at that cell).
    let mut h2_use_page = true;
    let mut h2_shift = false;
    let mut h2_shift_done = false;
    // `--h2-equip-layout 1` (2026-09-30, 의상 아바타 A3): the EQUIPMENT cells moved left.
    let mut h2_equip_layout = false;
    let mut h2_equip_layout_done = false;
    // `--h2-equip-preview <seq:frame:dir[:noweapon]>` (A3): the host's character preview in the
    // EQUIPMENT panel, burned into every --dump frame while that window is on top.
    let mut h2_equip_preview: Option<((u32, u32, u32), bool)> = None;
    let mut h2_curve_done = false;
    let mut h2_page: Vec<(u64, u8)> = vec![];
    // `--h2-exp <n>`: install the EXP redirect with that multiplier; `--h2-exp-gain <f:n>`:
    // call `a.d(I)V` with n at frame f and read the EXP back.
    let mut h2_exp: Option<u8> = None;
    // `--h2-difficulty <f:d>` (2026-09-29): the 난이도 level to hold from frame f, in order.
    let mut h2_difficulty: Vec<(u64, u8)> = vec![];
    let mut diff_state = guest_build::H2Difficulty::default();
    let mut diff_level: Option<u8> = None;
    // `--h2-drop-rate <f:d>` (2026-10-16): the 드랍률 level to hold from frame f, in order.
    let mut h2_drop_rate: Vec<(u64, u8)> = vec![];
    // `--h2-minimap <side>` (2026-09-29): on every --dump frame, also write the app's
    // hero5-style minimap panel over the frame scaled to phone size.
    let mut h2_minimap: u32 = 0;
    // `--h2-status 1` (2026-09-29): the app's status block + EXP bar over the same phone-size
    // frame (with the minimap too when `--h2-minimap` is given).
    let mut h2_status = false;
    // ★★ 2026-10-15 「악마화」 HD: `--h2-hd <k>` turns the canvas's HD mode on (the effect draws are recorded, not drawn — the
    // plain dumps show NO effect and no demon body), installs the demon block with HD = 1 (the blank body sheet 301, the body
    // as an effect) and, on every --dump frame, writes hd_<f>_<W>x<H>.rgba = the frame at k× with the recorded effects
    // composited by the app's own `compose_hd` (the library's 3× twins). k = the window's scale (5 = S25).
    let mut h2_hd: u32 = 0;
    // ★ 2026-10-16 보스 레이드: `--h2-raid-gate 1|0` forces the temple's raid gate; `--h2-kill-any 1` makes --h2-kill take the
    // first monster on the stage instead of the one faced (a `ca` boss teleports and is never "faced").
    let mut h2_raid_gate: Option<bool> = None;
    let mut h2_kill_any = false;
    // ★★ 2026-10-17 보스 레이드 아홉 단계: the app's driver (default on — it acts on raid maps only) and an auto-kill for the walk.
    let mut h2_boss_raid = true;
    let mut h2_raid_autokill: Option<u64> = None;
    let mut h2_raid_notice_check: Option<u64> = None;
    let mut h2_raid_autokill_last = false;
    // `--h2-pose-audit 1` (2026-10-10): on every --dump frame, every pose of the live class
    // animation against the images the game loaded (`h2_pose_audit`).
    let mut h2_pose_audit = false;
    // The worker's trigger cache (exits + chests), for `[h2-map]` and `--h2-minimap`
    // (`hero2java::H2TriggerCache`).
    let mut h2_trigger_cache = guest_build::H2TriggerCache::default();
    // `--h2-slots 3|5` (2026-09-27): the save-slot count; 5 = the app's install.
    let mut h2_slots: u8 = 5;
    // `--h2-slots-from <f>`: start that install at frame f instead of 1 (the late path).
    let mut h2_slots_from: u64 = 1;
    let mut h2_exp_gain: Vec<(u64, i32)> = vec![];
    // `--h2-shipped 1`: do NOT apply the 2026-09-23/24 balance patches (#16 combo SP · #18
    // skill points · #17 cooldown + skill power). The default applies
    // them exactly as the app does; this is the other half of an A/B.
    let mut h2_shipped = false;
    // `--h2-venom 0` (2026-09-29): skip only the 맹독감염 block — its own A/B.
    let mut h2_venom = true;
    // `--h2-shop-sell 0` (2026-09-30): skip only the 야수진화제 sell-price block — its own A/B.
    let mut h2_shop_sell = true;
    // ★ 2026-10-10: the boss crit floor (guest_anchors::BossCritPatch) — on by default, the app's; 0 = shipped.
    let mut h2_boss_crit = true;
    // ★ 2026-10-21: the boss defence pierce (guest_anchors::BossPiercePatch) — 20 % by default, the app's; 0 = shipped.
    let mut h2_boss_pierce: u32 = guest_build::BossPiercePatch::PIERCE_PCT;
    // ★ 2026-10-13: the grade line (guest_anchors::GradeLinePatch) — on by default, the app's; 0 = shipped.
    let mut h2_grade_line = true;
    // `--h2-hell-drop 0` (2026-10-02): skip only the HELL bonus drop — its own A/B.
    let mut h2_hell_drop = true;
    // `--h2-guardian 0` (2026-10-08): skip the fourth round's pieces (mode tables · round clear · template block).
    let mut h2_guardian = true;
    // `--h2-colour-init 0` (2026-10-17): leave `co` to the game (the first slot screen draws GUARDIAN black).
    let mut h2_colour_init = true;
    // `--h2-guardian-word <i:v>`: the block's tunable words at install.
    let mut h2_guardian_words: Vec<(usize, u32)> = vec![];
    // `--h2-hell-gate <0|1>` (2026-10-02 저녁): hold the block's 난이도 gate word (NORMAL+ since 2026-10-03; HARD only
    // before) open or closed regardless of `--h2-difficulty` — the gated slot's own A/B without the monster scaler
    // (whose doubled attack at HARD kills the test save's LV 26 knight before 2,000 kills).
    let mut h2_hell_gate: Option<bool> = None;
    // `--h2-shop <f>` (2026-09-30): open the shop at frame f through the game's own event calls.
    let mut h2_shop: Vec<u64> = vec![];
    // ★★ 2026-10-20: `--h2-shop-orb <f>` — the app's 상점주인의 혼 flow (AccountState::start_shop) at frame f: the popup
    // under the cursor must be the orb's; stock loaded when null, menu closed, shop event posted. `orb_step` = the call
    // in flight `(result, stage, menu_fields)`; `orb_watch` = print the state word's changes after the post.
    let mut h2_shop_orb: Vec<u64> = vec![];
    let mut orb_step: Option<(Arc<GuestCallResult>, u8, u32)> = None;
    // ★ 2026-10-21: `--h2-menu-release 0` = the old menu exit (child <- 0 · state <- 3); `--h2-shop-plate <f>` = the plate.
    let mut h2_menu_release = true;
    // ★ 2026-10-22: `--h2-diff-gate 0` = the difficulty mirror with the app's gate before this day (the state word's in-game set).
    let mut h2_diff_gate = true;
    let mut h2_shop_plate: Vec<u64> = vec![];
    let mut orb_watch: Option<(u64, Option<i32>)> = None;
    // ★★ 2026-10-21: `--h2-flight <f[:town]>` — the app's 비행서 flow (AccountState::start_flight); `flight_step` = the
    // call in flight `(result, stage, menu_fields, town, bag)`. `--h2-park <f:0|1>` — the world map's guest park.
    let mut h2_flight: Vec<(u64, Option<usize>)> = vec![];
    let mut flight_step: Option<(Arc<GuestCallResult>, u8, u32, usize, u32)> = None;
    let mut h2_park: Vec<(u64, bool)> = vec![];
    let mut h2_refine_test = None;
    // `--h2-tools 1` (2026-10-16): the app's 정렬 / 일괄판매 driver (`bag_tools_host.rs`) on the scripted keys, with taps
    // from `--h2-tools-tap <f:x:y[,..]>` (framebuffer px) — the app's `InputEvent::MenuTap`.
    let mut h2_tools = false;
    let mut h2_tools_tap: Vec<(u64, u32, u32)> = vec![];
    // ★ 2026-10-09 (재료가방 127 · 가디언 조합서): `--h2-alchemist <f>` opens the alchemist's menu the way its script event
    // does (ag.g()V: ag.h(8) · aw.a()Law; · aw.a()V); `--h2-picker-page <f:p>` pages the item picker (the app's
    // BagPaging::picker_go, same hero2java plan) and `[h2-picker]` prints the picker whenever it changes;
    // `--h2-bags127 1` = the app's standing capacity write on BOTH game bags (inventory + 재료가방, step 4d4).
    let mut h2_alchemist: Vec<u64> = vec![];
    let mut h2_picker_page: Vec<(u64, u8)> = vec![];
    let mut h2_bags127 = false;
    // ★★ 2026-10-20 `--h2-craft-guard 0|1`: the app's crafting-into-a-full-bag refusal (`craft_guard.rs`).
    let mut h2_craft_guard = true;
    // `--h2-clear-hook 0` (★ 2026-10-01 의상 아바타 A4): skip the round-clear counter (the app
    // installs it); `--h2-clear <f>` (repeatable): start the next round at frame f the game's own
    // way (the entry kind ag.e ← 2, event 9), so ag.y()V runs and the hook pays the reward.
    let mut h2_clear_hook = true;
    let mut h2_clear: Vec<u64> = vec![];
    // `--h2-gl 0` (★★ 2026-10-22 가디언 레벨): skip the level-up's GL wrapper (the app installs it).
    let mut h2_gl = true;
    // `--h2-cd-clock 0` (★ 2026-10-02): skip the cooldown tick counter (the app installs it);
    // `--h2-cd-flip <f1:f2:g[:old]>`: take guest slot g's skill off the bar at f1 and seat it
    // again at f2, the way the app's page flip does — its counter parked and aged by the clock
    // (`:old` = the shipped host behaviour, READY on every seating).
    let mut h2_cd_clock = true;
    // ★★ 2026-10-16 `--h2-item-icon 0`: skip the app's item icon block (the elixir's own strip cell, ItemIconPatch)
    let mut h2_item_icon = true;
    // `--h2-slot-focus 0` / `--h2-slot-last <1..5>` (★ 2026-10-12): the slot-screen focus the app installs, seeded.
    let mut h2_slot_focus = true;
    let mut h2_slot_last: Option<u8> = None;
    let mut h2_cd_flip: Vec<(u64, u64, u8, bool)> = vec![];
    // `--h2-buff-refresh 0` (★ 2026-10-05): skip the buff refresh (the app installs it).
    let mut h2_buff_refresh = true;
    // `--h2-rogue 0` (★ 2026-10-06): skip the rogue block (번개난타 three tiles · 심안's ghosts) the app installs.
    let mut h2_rogue = true;
    // `--h2-sky-draw 0` (★ 2026-10-08): skip the knight's 천공의검 picture block (prongs · cracks · flash) the app installs.
    let mut h2_sky_draw = true;
    // `--h2-teleport-gate 0` (★ 2026-10-11): skip the gate that refuses the rogue's teleports in 맵 18's corpse scene.
    let mut h2_teleport_gate = true;
    // `--h2-demon 0` (★ 2026-10-13 「악마화」 D1): skip the 악의 block the app installs after every other block.
    let mut h2_demon = true;
    // ★ 2026-10-05: the beast's 빛의회복 as a buff (`guest_anchors::beast_recovery_buff`) — 0 leaves only it shipped.
    let mut h2_beast_recovery = true;
    // ★ 2026-10-06: the beast's 지력 ×1.5 and attack-skill SP −20 % — 0 leaves both shipped.
    let mut h2_beast_tune = true;
    // `--h2-quest <f:i:s>` (★ 2026-10-01 의상 아바타 A5, repeatable): a quest's state at frame f.
    let mut h2_quest: Vec<(u64, u8, u8)> = vec![];
    // `--h2-avatar <f:spec>` (2026-09-30, repeatable): the 의상 아바타 override — installed from
    // f=30, the words written at each frame f, the parts reloaded by the game's own cf.a(I)V.
    // `(frame, spec)` as typed; parsed once the archive is open — the jar's member list decides
    // which costume sheets exist (`guest_build::AvatarSheets`).
    let mut h2_avatar_specs: Vec<(u64, String)> = vec![];
    // `--h2-costume-*` (2026-09-30, 의상 아바타 A2): the costume book, its records' directory,
    // and the grants / wears to drive it with.
    let mut costume_dir: Option<PathBuf> = None;
    // `--fx-lib <file>` (★ 2026-10-14): the effect library to register instead of the built-in demon_fx.bin.
    let mut fx_lib: Option<PathBuf> = None;
    let mut costume_grant: Vec<(u64, u16)> = vec![];
    let mut costume_wear: Vec<(u64, Option<u16>)> = vec![];
    // `--h2-costume-give <f:n[:mode[:slot]]>` (A3 · ★ A3c): a set's items (or one of them) given
    // WITHOUT wearing them — the wardrobe's test state (two sets, the same item twice).
    let mut costume_give: Vec<(u64, u16, u8, Option<u8>)> = vec![];
    // `--h2-costume-marker <slots>` (★ A3c): the app's adb grant marker, run before booting.
    let mut costume_marker: Option<String> = None;
    // `--h2-costume-catalog <file>` (A3): an EUC-KR catalog in `hero/costume.txt`'s format used
    // instead of the jar's — long names, many bags, without touching the asset.
    let mut costume_catalog_file: Option<PathBuf> = None;
    // `--h2-equip-ui 0|1` (A3): the app's EQUIPMENT avatar side — the cells moved and the cursor
    // switch (the app's install), the scripted keys and taps through the app's model, the
    // picture burned into every dump.
    let mut h2_equip_ui = false;
    // `--h2-notice 1` / `--h2-drop-life 1` / `--h2-autoloot 1` (2026-09-24): the app's
    // painter-off patch plus its notice feed (every notice copied out of `n.b`, dumped as
    // notice_<f>_<w>x<h>.rgba, `n.o` cleared), its ground-drop lifetime top-up
    // (emulator::DropLife, every 5 frames) and its 오토루팅 (emulator::AutoLoot).
    let mut h2_notice = false;
    let mut h2_notice_done = false;
    // `--h2-hud <shift>` (2026-09-25): the app's HUD painter plan (the target-monster panel
    // kept, everything else off — `GuestBuild::hud_bottom_writes`) plus its target-box
    // dodge, `shift` guest px left (`GuestBuild::target_box_x_writes`). `--h2-notice 1`
    // implies the painter plan with shift 0.
    let mut h2_hud: Option<u32> = None;
    let mut h2_hud_said = false;
    // `--h2-honor <f[:k][,..]>` (2026-09-25): the player's 명예 and every equipped weapon's
    // 요구 명예 with its full attack and what the shipped penalty would leave of it, then the
    // game's own `bo.c()S` / `bo.d()S` on the same weapons. `:k` first writes 요구 명예 = k on
    // each of them (DIAGNOSTIC — no test save wields an honour-gated weapon), so the getters
    // have a penalty to apply: the A/B against `--h2-shipped 1` is the patch's proof.
    let mut h2_honor: Vec<(u64, Option<i32>)> = vec![];
    // 2026-09-27: --h2-sweep <f:kind> — bx.h(B)Z by name (the front sweep), and its pending calls.
    let mut h2_sweep: Vec<(u64, i32, Option<i32>)> = vec![];
    let mut sweep_calls: Vec<(i32, Arc<GuestCallResult>)> = vec![];
    // 2026-09-27: --h2-weapon-type <f:t> — relabel every equipped weapon's ch.g (diagnostic).
    let mut h2_weapon_type: Vec<(u64, i32, Option<u32>)> = vec![];
    // ★ 2026-10-10: --h2-weapon-trait <f:t> — every equipped weapon's 특성 `bo.i` (abs 0x58, the byte an orb writes:
    // 1 일섬 … 5 투지 · 6 기절) and the character's copy `a.m:B` (abs 0xd8 — what `a.m()V` copies and every hit passes
    // on), diagnostic. [h2-tuji] then prints the 투지 flag `a.h:Z` (abs 0xfc) whenever it changes.
    let mut h2_weapon_trait: Vec<(u64, i32)> = vec![];
    let mut tuji_last: Option<i32> = None;
    // ★ 2026-10-02 스킬을 바꾸는 장신구: the app's AccessoryPatch block (installed from f=30 unless
    //   --h2-acc-patch 0), ACTIVE forced (--h2-acc) or detected from the worn slot 2 the app's way
    //   (--h2-acc-auto 1), the worn record relabelled (--h2-acc-relabel, diagnostic).
    let mut h2_acc: Vec<(u64, u32)> = vec![];
    let mut h2_acc_auto = false;
    let mut h2_acc_relabel: Vec<(u64, u8, u8)> = vec![];
    let mut h2_acc_patch = true;
    let mut h2_beast_skill: Vec<(u64, u8, i8, i8)> = vec![];
    // ★★ 2026-10-16 `--h2-beast-refund <f:idx>`: the game's own `bn.d()V` on owned beast #idx (the Vector `ag.a`) — what the
    // app asks on the 예 of the beast list's 「버리시겠습니까?」 (`h2_beast_discard_pending`); [h2-reset] report lines before and
    // one frame after show the skill points and every beast's `bn.k`.
    let mut h2_beast_refund: Vec<(u64, u32)> = vec![];
    let mut h2_beast_refund_report: Vec<u64> = vec![];
    let mut h2_mob_place: Vec<(u64, u32, u32, i32, i32)> = vec![];
    let mut h2_mob_hurt: Vec<(u64, u32, u32, i32)> = vec![];
    let mut h2_mob_hurt_calls: Vec<(u64, u32, Arc<GuestCallResult>)> = vec![];
    let mut h2_acc_word: Vec<(u64, u32, u32)> = vec![];
    let mut honor_calls: Vec<(u32, &'static str, Arc<GuestCallResult>)> = vec![];
    let mut h2_autoloot = false;
    // ★★ 2026-10-18: --h2-drop-as <f:t:i> — a TEST tool for the 재료가방 refusal notice: at frame f every ground drop
    //   becomes item (t, i) x1 and lies under the player (the game's own pickup then decides — an OK after it shows
    //   which notice row the host chose, `h2_bag_full_row`).
    //   `<f:t:i:stay>` retypes them where they lie (the 오토루팅 OK-near notice).
    let mut h2_drop_as: Vec<(u64, i8, i8, bool)> = Vec::new();
    let mut loot_call: Option<(Arc<GuestCallResult>, u32)> = None;
    let mut loot_log: u64 = 0;
    let mut h2_drop_life = false;
    // `--h2-font-check <f>`: with 환경설정 open, render its five labels and the focused
    // row's value with the host's replica of the game font (`hero2java::H2Font`) and
    // compare pixel-for-pixel with what the game drew.
    let mut h2_font_check: Option<u64> = None;
    // `--h2-revive 1` / `--h2-die <f>` / `--h2-confirm-check <f>` (부활의 구, 2026-09-25):
    // the app's revive (`emulator::Revive`) on the scripted keys — the box raised the
    // frame the character falls with an orb in the bag, NO TICKS while it is up, the keys
    // it takes swallowed, 예 = the four writes + one orb consumed + the notice, 아니오 =
    // the game's own death; the box composited into dumps. `--h2-die` makes the fall with
    // the game's own HP method (`a.b(I)V` with −hp). `--h2-confirm-check` renders the
    // game's OWN kind-2 box on screen at frame f (its lines, its cursor) with the host's
    // painter and compares pixel for pixel.
    let mut h2_revive = false;
    let mut h2_die: Vec<u64> = vec![];
    // 2026-09-26: `--h2-mob-hp <f:hp>` — every live monster's HP (`bd.d:S`, abs +0x74) set
    // to `hp` at frame f, so the next hit of the player's own attack is a real kill.
    let mut h2_mob_hp: Vec<(u64, i32)> = vec![];
    let mut h2_benchmark: Option<(u64, u64)> = None;
    let mut benchmark_attack = std::collections::HashMap::new();
    // 2026-09-26: `--h2-boss-refill 1` — the app's `BossRefill`, frame by frame: a boss (`instanceof ah`)
    // that starts to die refills the character and stands it up if it is falling.
    let mut h2_boss_refill = false;
    let mut boss_refill_answered: Vec<u32> = vec![];
    let mut boss_refill_until: Option<u64> = None;
    let mut h2_confirm_check: Option<u64> = None;
    // (the box, the orb, the lines, (ax.g, ax.i, ax.j)) while it is up.
    let mut revive_prompt: Option<(guest_build::H2Confirm, guest_build::H2ReviveOrb, Vec<Vec<u8>>, (i32, i32, i32))> = None;
    let mut revive_declined = false;
    let mut revive_consume: Option<(Arc<GuestCallResult>, u64)> = None;
    let mut revive_held_frames = 0u64;
    let mut revive_watch: Option<u64> = None;
    let mut revive_font: Option<guest_build::H2Font> = None;
    let mut h2_combo_watch: Option<(u64, u64)> = None;
    // `--h2-options 1`: the app's extended 환경설정 (8 rows, scrolled) on the scripted
    // keys, with the host's rows composited into every dumped frame.
    let mut h2_options = false;
    let mut opt_state = guest_build::H2Options::default();
    let mut opt_host: (bool, usize, usize) = (true, 0, 0);
    let mut opt_respawn = 0usize;
    let mut opt_drop = usize::from(guest_build::DROP_RATE_DEFAULT);
    let mut drop_ledger: Vec<guest_build::H2DropLife> = vec![];
    // `--h2-kill <f:n>`: from frame f, run the game's death routine `bd.q()V` n times on
    // the monster the character faces and tally the drops (#20/#21).
    let mut h2_kill: Option<(u64, u32)> = None;
    // `--h2-skill <f>`: the class skill multiplier (1000 in) for every a.p index, and the
    // cooldown totals, through the game's own methods (#17).
    let mut h2_skill: Option<u64> = None;
    // `--h2-drop-up <f[,f…]>`: a scripted key RELEASE due on frame f is not delivered — the
    // host-side shape of the guest ignoring it (n.keyNotify looks at a release only in
    // state 3). `--h2-walk-guard 1` / `--h2-relatch 1`: run the app's two walk rules
    // (`emulator::WalkGuard`) on the scripted keys, so the guest's response to them is
    // measured here through the same reader (`h2_field_walk`). 조이스틱, 2026-09-24.
    let mut h2_drop_up: Vec<u64> = vec![];
    let mut h2_walk_guard = false;
    let mut h2_relatch = false;
    // `--h2-turn 1` / `--h2-reverse 1` (조이스틱 2026-09-24): the app's two turn rules —
    // a direction release on the frame another direction is pressed is NOT sent (the
    // worker's `apply_turn`), and a press straight back along a walk that is between two
    // tiles flips the facing at once (`emulator::reverse_walk`).
    let mut h2_turn = false;
    let mut h2_reverse = false;
    // `--h2-account <frame>`: open the ACCOUNT window the way the app does.
    let mut h2_account: Vec<u64> = vec![];
    // `--h2-acct-roundtrip <frame>`: serialise → byte[] → second bag → deserialise.
    let mut h2_acct_rt: Vec<u64> = vec![];
    // `--h2-acct-front <frame>`: append a 재료가방 record, then the app's cell-0 rotation.
    let mut h2_acct_front: Vec<u64> = vec![];
    // `--h2-unworn <frame>`: the app's load-time rule — no worn flag in the warehouse.
    let mut h2_unworn: Vec<u64> = vec![];
    let mut h2_wear: Vec<(u64, u32, i32)> = vec![]; // ★ 2026-10-20 (frame, bag index, equip slot)
    // `--h2-quickbar <frame:slot:value>`: the app's plate-seat writes into `ag.j[slot]`.
    let mut h2_quickbar: Vec<(u64, u8, u8)> = vec![];
    // `--h2-moverow` / `--h2-movego`: the move row and the move, the app's own steps.
    let mut h2_moverow: Vec<u64> = vec![];
    let mut h2_movego: Vec<u64> = vec![];
    // `--h2-qty <frame>`: the app's 개수 box over the popup's record (`open_quantity`).
    // `--h2-cover <frame>`: where the app says the game's box is (the page strip's cover),
    // against the pixels the game painted.
    let mut h2_qty: Vec<u64> = vec![];
    let mut h2_cover: Vec<u64> = vec![];
    // `--h2-heal <frame>:<type>`: the potion plate's press, headless.
    let mut h2_heal: Option<(u64, u8)> = None;
    // `--h2-reset <frame>:<stat|skill|repair>[:apply]`: #22's reset, planned and
    // optionally run. `repair` is the 2026-09-23 기본공격 repair, which is the same shape
    // (a plan of writes) and so rides the same flag.
    let mut h2_reset: Option<(u64, H2ResetWhat, bool)> = None;
    // `--h2-combo-break <frame>`: DIAGNOSTIC. Reproduce the damage the pre-2026-09-23
    // skill reset did — 기본공격 stage 1 and the open count to 0 — so that
    // `--h2-reset <f>:repair:apply` can be proven on a LIVE character rather than only in
    // the selftest's fake heap. Nothing in the app writes this; it exists to break things.
    // `:old` is the old reset's EXACT footprint instead: every positive stage to 0 and
    // `a.h` left as it was (the reset never wrote it — the next `a.p()V` recomputes it).
    let mut h2_combo_break: Option<(u64, bool)> = None;
    // `--h2-use <frame>:<type>`: the 치료제 / 귀환서 plate's press — the item-use drive
    // (`h2_use_step`) stepped once per frame from that frame until it ends.
    let mut h2_use: Option<(u64, u8)> = None;
    // --host-save <frame> — at that frame, start the app's own four-step save chain
    // (serialise → write GameSave → summary → write SlotSave) with the per-build
    // `guest_anchors::save_chain()` addresses, driven exactly like `emulator.rs` does.
    // Run it from a FIELD frame (no window open) — the same precondition the app's
    // `save_gate` enforces.
    let mut host_save_frame: Option<u64> = None;
    // --host-levelup <frame> — at that frame, deliver one 천상의 영약 grant exactly the
    // way `emulator.rs` does: exp = expToNext + 1, then the game's own checkLevelUp
    // (`guest_anchors::level_up_fn`). Run it from a FIELD frame. The frames after it
    // watch the level word and print the before/after plus the stat/skill point
    // deltas, so one deterministic run verifies the whole mechanism headlessly.
    let mut host_levelup_frame: Option<u64> = None;
    let mut keys: Vec<(u64, KeyCode, u64)> = vec![];
    let argv: Vec<String> = std::env::args().collect();
    if argv.iter().any(|a| a == "--help" || a == "-h") {
        eprint!("{USAGE}");
        return;
    }
    // ── WHY THIS LOOP IS NOISY NOW ───────────────────────────────────────────────
    // The old parser failed four different ways in SILENCE, and this crate is a
    // measurement tool — a silently ignored flag makes it report a number for a run
    // that never happened:
    //   * `_ => {}`                     swallowed any misspelled flag
    //   * `while i + 1 < argv.len()`    dropped a trailing flag that had no value
    //   * `.parse().unwrap()`           panicked with no idea which flag was bad
    //   * `.parse().ok()`               turned a bad --host-save frame into "off"
    // So: collect every complaint, print them all, and exit NON-ZERO. Nothing runs
    // until the command line is exactly what the caller wrote.
    let mut errs: Vec<String> = vec![];
    // Declared HERE, not with the loop below: `macro_rules!` hygiene resolves the free
    // names in a macro body against the macro's DEFINITION site, so `i` has to already
    // be in scope for `num!` to name the flag it is complaining about.
    let mut i = 1;
    macro_rules! num {
        ($v:expr) => {
            match $v.parse() {
                Ok(n) => Some(n),
                Err(_) => {
                    errs.push(format!("{} wants a number, got {:?}", argv[i], $v));
                    None
                }
            }
        };
    }
    while i < argv.len() {
        let k = argv[i].clone();
        let k = k.as_str();
        if !k.starts_with("--") {
            errs.push(format!("stray argument {k:?} — every option is `--name value`"));
            i += 1;
            continue;
        }
        let Some(v) = argv.get(i + 1).cloned() else {
            errs.push(format!("{k} needs a value"));
            break;
        };
        let v = v.as_str();
        match k {
            "--zip" => zip = Some(PathBuf::from(v)),
            "--out" => out = PathBuf::from(v),
            "--w" => {
                if let Some(n) = num!(v) {
                    w = n
                }
            }
            "--h" => {
                if let Some(n) = num!(v) {
                    h = n
                }
            }
            "--frames" => {
                if let Some(n) = num!(v) {
                    frames = n
                }
            }
            "--ms" => {
                if let Some(n) = num!(v) {
                    ms_per_frame = n
                }
            }
            "--fs" => fs_root = Some(PathBuf::from(v)),
            "--fs-out" => fs_out = Some(PathBuf::from(v)),
            "--db" => db_root = Some(PathBuf::from(v)),
            "--db-rw" => db_rw = v == "1" || v == "true",
            "--find-fb" => find_fb = v == "1" || v == "true",
            "--map-check" => map_check = v == "1" || v == "true",
            "--map-hunt" => map_hunt = v == "1" || v == "true",
            "--profile" => profile = v == "1" || v == "true",
            "--hle" => hle = v == "1" || v == "true",
            "--phases" => phases = v == "1" || v == "true",
            "--gc" => {
                if let Some(n) = num!(v) {
                    gc_every = n
                }
            }
            "--char-dump" => char_dump = Some(PathBuf::from(v)),
            "--trace-state" => trace_state = v == "1",
            "--gauges" => match v.parse::<u64>() {
                Ok(n) if n > 0 => gauges = n,
                _ => errs.push(format!("--gauges wants a positive frame interval, got {v:?}")),
            },
            "--audio-log" => audio_log = Some(PathBuf::from(v)),
            "--audio-fm" => match v {
                "0" | "1" => audio_fm = v == "1",
                _ => errs.push(format!("--audio-fm wants 0 or 1, got {v:?}")),
            },
            "--audio-wav" => audio_wav = Some(PathBuf::from(v)),
            "--h2-sfx" => {
                for spec in v.split(',') {
                    match spec.split_once(':').map(|(f, n)| (f.parse::<u64>(), n.parse::<i32>())) {
                        Some((Ok(f), Ok(n))) if (0..=127).contains(&n) => h2_sfx.push((f, n)),
                        _ => errs.push(format!("bad --h2-sfx spec {spec:?} (want frame:slot, slot 0..=127)")),
                    }
                }
            }
            "--h2-relabel" => match v.parse::<u64>() {
                Ok(n) => h2_relabel = Some(n),
                Err(_) => errs.push(format!("--h2-relabel wants a frame number, got {v:?}")),
            },
            "--h2-save" => match v.parse::<u64>() {
                Ok(f) => h2_save_frames.push(f),
                _ => errs.push(format!("--h2-save wants a frame number, got {v:?}")),
            },
            "--h2-tap" => {
                for spec in v.split(',') {
                    let mut it = spec.split(':');
                    match (
                        it.next().and_then(|x| x.parse::<u64>().ok()),
                        it.next().and_then(|x| x.parse::<u32>().ok()),
                        it.next().and_then(|x| x.parse::<u32>().ok()),
                    ) {
                        (Some(f), Some(x), Some(y)) => h2_taps.push((f, x, y)),
                        _ => errs.push(format!("bad --h2-tap spec {spec:?} (want frame:fx:fy)")),
                    }
                }
            }
            "--h2-inv" => {
                for spec in v.split(',') {
                    // `700` = the inventory; `700b` / `700c` = the 재료 / 카드 bag at that frame.
                    let (num, bag) = match spec.as_bytes().last() {
                        Some(b'b') => (&spec[..spec.len() - 1], b'b'),
                        Some(b'c') => (&spec[..spec.len() - 1], b'c'),
                        Some(b'w') => (&spec[..spec.len() - 1], b'w'),
                        _ => (spec, b'a'),
                    };
                    match num.parse::<u64>() {
                        Ok(f) => h2_inv.push((f, bag)),
                        Err(_) => errs.push(format!("bad --h2-inv frame {spec:?} (want <frame>[b|c])")),
                    }
                }
            }
            "--h2-ilpung" => match v {
                "1" => h2_ilpung = true,
                "0" => h2_ilpung = false,
                _ => errs.push(format!("--h2-ilpung wants 0 or 1, got {v:?}")),
            },
            "--h2-fury-gauge" => match v.split_once(':').map(|(f, g)| (f.parse::<u64>(), g.parse::<u32>())) {
                Some((Ok(f), Ok(g))) if g <= 100 => h2_fury_gauge = Some((f, g)),
                _ => errs.push(format!("--h2-fury-gauge wants <frame>:<0..=100>, got {v:?}")),
            },
            "--h2-voice" => match v {
                "1" => h2_voice = true,
                "0" => h2_voice = false,
                _ => errs.push(format!("--h2-voice wants 0 or 1, got {v:?}")),
            },
            "--h2-berserker" => match v {
                "1" => h2_berserker = true,
                "0" => h2_berserker = false,
                _ => errs.push(format!("--h2-berserker wants 0 or 1, got {v:?}")),
            },
            "--h2-ilpung-unlock" => match v.parse::<u64>() {
                Ok(f) => h2_ilpung_unlock = Some(f),
                Err(_) => errs.push(format!("--h2-ilpung-unlock wants a frame, got {v:?}")),
            },
            "--h2-awaken-unlock" => match v.parse::<u64>() {
                Ok(f) => h2_awaken_unlock.push(f),
                Err(_) => errs.push(format!("--h2-awaken-unlock wants a frame, got {v:?}")),
            },
            "--h2-bodyslam" => match v.parse::<u64>() {
                Ok(f) => h2_bodyslam.push(f),
                Err(_) => errs.push(format!("--h2-bodyslam wants a frame, got {v:?}")),
            },
            "--h2-damage-cap" => match v {
                "1" => h2_damage_cap = true,
                "0" => h2_damage_cap = false,
                _ => errs.push(format!("--h2-damage-cap wants 0 or 1, got {v:?}")),
            },
            "--h2-hp-int" => match v {
                "1" => h2_hp_int = true,
                "0" => h2_hp_int = false,
                _ => errs.push(format!("--h2-hp-int wants 0 or 1, got {v:?}")),
            },
            "--h2-skill-set" => {
                let parts: Vec<&str> = v.split(':').collect();
                match (parts.first().and_then(|x| x.parse::<u64>().ok()), parts.get(1).and_then(|x| x.parse::<u32>().ok()), parts.get(2).and_then(|x| x.parse::<i8>().ok())) {
                    (Some(f), Some(i), Some(lv)) if parts.len() == 3 => h2_skill_set.push((f, i, lv)),
                    _ => errs.push(format!("--h2-skill-set wants <frame:index:level>, got {v:?}")),
                }
            }
            "--h2-born-repair" => match v.parse::<u64>() {
                Ok(f) => h2_born_repair.push(f),
                Err(_) => errs.push(format!("--h2-born-repair wants a frame, got {v:?}")),
            },
            "--h2-iaido-unlock" => match v.parse::<u64>() {
                Ok(f) => h2_iaido_unlock = Some(f),
                Err(_) => errs.push(format!("--h2-iaido-unlock wants a frame, got {v:?}")),
            },
            "--h2-cap" => {
                let mut it = v.split(':');
                match (
                    it.next().and_then(|x| x.parse::<u64>().ok()),
                    it.next().and_then(|x| x.parse::<i8>().ok()),
                ) {
                    (Some(f), Some(n)) if n > 0 => h2_cap = Some((f, n)),
                    _ => errs.push(format!("--h2-cap wants frame:1..127, got {v:?}")),
                }
            }
            "--h2-give" => match parse_give_spec(v) {
                Some(spec) => h2_give = Some(spec),
                None => errs.push(format!("--h2-give wants frame:type:id:n[,type:id:n..] (all 0..127, n >= 1), got {v:?}")),
            },
            "--h2-wh-give" => match parse_give_spec(v) {
                Some(spec) => h2_wh_give = Some(spec),
                None => errs.push(format!("--h2-wh-give wants frame:type:id:n[,type:id:n..] (all 0..127, n >= 1), got {v:?}")),
            },
            "--h2-wh-in" => match std::fs::read(v) {
                Ok(blob) => h2_wh_in = Some(blob),
                Err(e) => errs.push(format!("--h2-wh-in {v:?}: {e}")),
            },
            "--h2-wh-out" => h2_wh_out = Some(std::path::PathBuf::from(v)),
            "--h2-account" => match v.parse::<u64>() {
                Ok(f) => h2_account.push(f),
                Err(_) => errs.push(format!("--h2-account wants a frame, got {v:?}")),
            },
            "--h2-moverow" => {
                for spec in v.split(',') {
                    match spec.parse::<u64>() {
                        Ok(f) => h2_moverow.push(f),
                        Err(_) => errs.push(format!("bad --h2-moverow frame {spec:?}")),
                    }
                }
            }
            "--h2-movego" => {
                for spec in v.split(',') {
                    match spec.parse::<u64>() {
                        Ok(f) => h2_movego.push(f),
                        Err(_) => errs.push(format!("bad --h2-movego frame {spec:?}")),
                    }
                }
            }
            "--h2-qty" => {
                for spec in v.split(',') {
                    match spec.parse::<u64>() {
                        Ok(f) => h2_qty.push(f),
                        Err(_) => errs.push(format!("bad --h2-qty frame {spec:?}")),
                    }
                }
            }
            "--h2-cover" => {
                for spec in v.split(',') {
                    match spec.parse::<u64>() {
                        Ok(f) => h2_cover.push(f),
                        Err(_) => errs.push(format!("bad --h2-cover frame {spec:?}")),
                    }
                }
            }
            "--h2-acct-roundtrip" => match v.parse::<u64>() {
                Ok(f) => h2_acct_rt.push(f),
                Err(_) => errs.push(format!("--h2-acct-roundtrip wants a frame, got {v:?}")),
            },
            "--h2-acct-front" => match v.parse::<u64>() {
                Ok(f) => h2_acct_front.push(f),
                Err(_) => errs.push(format!("--h2-acct-front wants a frame, got {v:?}")),
            },
            "--h2-unworn" => match v.parse::<u64>() {
                Ok(f) => h2_unworn.push(f),
                Err(_) => errs.push(format!("--h2-unworn wants a frame, got {v:?}")),
            },
            "--h2-wear" => {
                // `f:IDX:SLOT[,IDX:SLOT..]` — at frame f the inventory record at Vector index IDX gets aa.f = SLOT.
                let (f, rest) = v.split_once(':').unwrap_or((v, ""));
                match f.parse::<u64>() {
                    Ok(f) => {
                        let mut any = false;
                        for one in rest.split(',') {
                            let p: Vec<&str> = one.split(':').collect();
                            match (p.first().and_then(|x| x.parse::<u32>().ok()), p.get(1).and_then(|x| x.parse::<i32>().ok())) {
                                (Some(i), Some(slot)) if p.len() == 2 && (-1..=6).contains(&slot) => {
                                    h2_wear.push((f, i, slot));
                                    any = true;
                                }
                                _ => errs.push(format!("--h2-wear wants f:IDX:SLOT[,IDX:SLOT..] (SLOT -1..6), got {one:?}")),
                            }
                        }
                        if !any {
                            errs.push(format!("--h2-wear wants f:IDX:SLOT, got {v:?}"));
                        }
                    }
                    Err(_) => errs.push(format!("--h2-wear wants a frame first, got {v:?}")),
                }
            }
            "--h2-quickbar" => {
                let p: Vec<&str> = v.split(':').collect();
                let f = p.first().and_then(|x| x.parse::<u64>().ok());
                let g = p.get(1).and_then(|x| x.parse::<u8>().ok());
                let val = p.get(2).and_then(|x| x.parse::<u8>().ok());
                match (f, g, val) {
                    (Some(f), Some(g), Some(val)) if p.len() == 3 && g < 4 && val >= 1 && val < 0xff => h2_quickbar.push((f, g, val)),
                    _ => errs.push(format!("--h2-quickbar wants frame:slot:value (slot 0..3, value 1..254), got {v:?}")),
                }
            }
            "--h2-pages" => match v.parse::<u8>() {
                Ok(x) => h2_pages = x != 0,
                Err(_) => errs.push(format!("--h2-pages wants 0|1, got {v:?}")),
            },
            "--h2-use-page" => match v.parse::<u8>() {
                Ok(x) => h2_use_page = x != 0,
                Err(_) => errs.push(format!("--h2-use-page wants 0|1, got {v:?}")),
            },
            "--h2-equip-preview" => {
                let parts: Vec<&str> = v.split(':').collect();
                let nums: Vec<Option<u32>> = parts.iter().take(3).map(|t| t.parse::<u32>().ok()).collect();
                match (nums.as_slice(), parts.get(3)) {
                    ([Some(a), Some(b), Some(c)], None) => h2_equip_preview = Some(((*a, *b, *c), false)),
                    ([Some(a), Some(b), Some(c)], Some(&"noweapon")) => h2_equip_preview = Some(((*a, *b, *c), true)),
                    _ => errs.push(format!("--h2-equip-preview wants seq:frame:dir[:noweapon], got {v:?}")),
                }
            }
            "--h2-equip-layout" => match v.parse::<u8>() {
                Ok(x) => h2_equip_layout = x != 0,
                Err(_) => errs.push(format!("--h2-equip-layout wants 0|1, got {v:?}")),
            },
            "--h2-shift" => match v.parse::<u8>() {
                Ok(x) => h2_shift = x != 0,
                Err(_) => errs.push(format!("--h2-shift wants 0|1, got {v:?}")),
            },
            "--h2-notice" => match v {
                "1" => h2_notice = true,
                "0" => h2_notice = false,
                _ => errs.push(format!("--h2-notice wants 0 or 1, got {v:?}")),
            },
            "--h2-hud" => match v.parse::<u32>() {
                Ok(n) if n <= 255 => h2_hud = Some(n),
                _ => errs.push(format!("--h2-hud wants a left shift in guest px (0..=255), got {v:?}")),
            },
            "--h2-weapon-type" => {
                // <f:t> relabels every equipped weapon; ★ 2026-10-06 <f:t:slot> only the one in that equip
                // slot (the [h2-honor] index — e.g. the rogue's main hand, to test 도검 + 단검).
                let mut it = v.split(':');
                match (it.next().map(str::parse::<u64>), it.next().map(str::parse::<i32>), it.next().map(str::parse::<u32>), it.next()) {
                    (Some(Ok(f)), Some(Ok(t)), None, None) if (0..=4).contains(&t) => h2_weapon_type.push((f, t, None)),
                    (Some(Ok(f)), Some(Ok(t)), Some(Ok(slot)), None) if (0..=4).contains(&t) => h2_weapon_type.push((f, t, Some(slot))),
                    _ => errs.push(format!("--h2-weapon-type wants <frame>:<type 0..=4>[:<equip slot>], got {v:?}")),
                }
            }
            "--h2-weapon-trait" => match v.split_once(':').map(|(a, b)| (a.parse::<u64>(), b.parse::<i32>())) {
                Some((Ok(f), Ok(t))) if (0..=6).contains(&t) => h2_weapon_trait.push((f, t)),
                _ => errs.push(format!("--h2-weapon-trait wants <frame>:<trait 0..=6>, got {v:?}")),
            },
            "--h2-acc" => match v.split_once(':').map(|(a, b)| (a.parse::<u64>(), b.parse::<u32>())) {
                Some((Ok(f), Ok(c))) if c <= 6 => h2_acc.push((f, c)),
                _ => errs.push(format!("--h2-acc wants <frame>:<code 0..=6>, got {v:?}")),
            },
            "--h2-acc-auto" => match v {
                "1" => h2_acc_auto = true,
                "0" => h2_acc_auto = false,
                _ => errs.push(format!("--h2-acc-auto wants 0 or 1, got {v:?}")),
            },
            "--h2-acc-patch" => match v {
                "1" => h2_acc_patch = true,
                "0" => h2_acc_patch = false,
                _ => errs.push(format!("--h2-acc-patch wants 0 or 1, got {v:?}")),
            },
            "--h2-beast-refund" => {
                let parts: Vec<&str> = v.split(':').collect();
                match parts.as_slice() {
                    [f, i] => match (f.parse::<u64>(), i.parse::<u32>()) {
                        (Ok(f), Ok(i)) => h2_beast_refund.push((f, i)),
                        _ => errs.push(format!("--h2-beast-refund wants <frame>:<beast index>, got {v:?}")),
                    },
                    _ => errs.push(format!("--h2-beast-refund wants <frame>:<beast index>, got {v:?}")),
                }
            }
            "--h2-beast-skill" => {
                let parts: Vec<&str> = v.split(':').collect();
                match parts.as_slice() {
                    [f, s, ty, lv] => match (f.parse::<u64>(), s.parse::<u8>(), ty.parse::<i8>(), lv.parse::<i8>()) {
                        (Ok(f), Ok(s), Ok(ty), Ok(lv)) if s < 4 && (0..=12).contains(&ty) && (0..=5).contains(&lv) => h2_beast_skill.push((f, s, ty, lv)),
                        _ => errs.push(format!("--h2-beast-skill wants <frame>:<slot 0..=3>:<type 0..=12>:<level 0..=5>, got {v:?}")),
                    },
                    _ => errs.push(format!("--h2-beast-skill wants <frame>:<slot>:<type>:<level>, got {v:?}")),
                }
            }
            "--h2-acc-word" => {
                let parts: Vec<&str> = v.split(':').collect();
                match parts.as_slice() {
                    [f, off, val] => match (f.parse::<u64>(), u32::from_str_radix(off.trim_start_matches("0x"), 16), val.parse::<u32>()) {
                        (Ok(f), Ok(off), Ok(val)) if off < guest_build::AccessoryPatch::BLOCK_LEN && off % 4 == 0 => h2_acc_word.push((f, off, val)),
                        _ => errs.push(format!("--h2-acc-word wants <frame>:<hex data offset in the block, word-aligned>:<value>, got {v:?}")),
                    },
                    _ => errs.push(format!("--h2-acc-word wants <frame>:<hex offset>:<value>, got {v:?}")),
                }
            }
            "--h2-mob-place" => {
                let parts: Vec<&str> = v.split(':').collect();
                match parts.as_slice() {
                    [f, x0, y0, x1, y1] => match (f.parse::<u64>(), x0.parse::<u32>(), y0.parse::<u32>(), x1.parse::<i32>(), y1.parse::<i32>()) {
                        (Ok(f), Ok(x0), Ok(y0), Ok(x1), Ok(y1)) => h2_mob_place.push((f, x0, y0, x1, y1)),
                        _ => errs.push(format!("--h2-mob-place wants <frame>:<x0>:<y0>:<x1>:<y1>, got {v:?}")),
                    },
                    _ => errs.push(format!("--h2-mob-place wants <frame>:<x0>:<y0>:<x1>:<y1>, got {v:?}")),
                }
            }
            "--h2-mob-hurt" => {
                let parts: Vec<&str> = v.split(':').collect();
                match parts.as_slice() {
                    [f, x, y, d] => match (f.parse::<u64>(), x.parse::<u32>(), y.parse::<u32>(), d.parse::<i32>()) {
                        (Ok(f), Ok(x), Ok(y), Ok(d)) if d > 0 => h2_mob_hurt.push((f, x, y, d)),
                        _ => errs.push(format!("--h2-mob-hurt wants <frame>:<x>:<y>:<damage > 0>, got {v:?}")),
                    },
                    _ => errs.push(format!("--h2-mob-hurt wants <frame>:<x>:<y>:<damage > 0>, got {v:?}")),
                }
            }
            "--h2-acc-relabel" => {
                let parts: Vec<&str> = v.split(':').collect();
                match parts.as_slice() {
                    [f, t, i] => match (f.parse::<u64>(), t.parse::<u8>(), i.parse::<u8>()) {
                        (Ok(f), Ok(t), Ok(i)) => h2_acc_relabel.push((f, t, i)),
                        _ => errs.push(format!("--h2-acc-relabel wants <frame>:<type>:<id>, got {v:?}")),
                    },
                    _ => errs.push(format!("--h2-acc-relabel wants <frame>:<type>:<id>, got {v:?}")),
                }
            }
            "--h2-sweep" => {
                for spec in v.split(',') {
                    let mut it = spec.trim().split(':');
                    let (f, k) = (it.next().and_then(|x| x.parse::<u64>().ok()), it.next().and_then(|x| x.parse::<i32>().ok()));
                    let face = it.next().map(|x| x.parse::<i32>().ok().filter(|d| (1..=4).contains(d)));
                    match (f, k, face) {
                        (Some(f), Some(k), None) if (0..15).contains(&k) => h2_sweep.push((f, k, None)),
                        (Some(f), Some(k), Some(Some(d))) if (0..15).contains(&k) => h2_sweep.push((f, k, Some(d))),
                        _ => errs.push(format!("--h2-sweep wants <frame>:<a.p kind 0..15>[:<facing 1..4>], got {spec:?}")),
                    }
                }
            }
            "--h2-honor" => {
                for spec in v.split(',') {
                    let mut it = spec.trim().split(':');
                    let f = it.next().and_then(|x| x.parse::<u64>().ok());
                    let k = it.next().map(|x| x.parse::<i32>().ok().filter(|k| (0..=17).contains(k)));
                    match (f, k) {
                        (Some(f), None) => h2_honor.push((f, None)),
                        (Some(f), Some(Some(k))) => h2_honor.push((f, Some(k))),
                        _ => errs.push(format!("--h2-honor wants <frame>[:<요구명예 0..=17>], got {spec:?}")),
                    }
                }
            }
            "--h2-options" => match v {
                "1" => h2_options = true,
                "0" => h2_options = false,
                _ => errs.push(format!("--h2-options wants 0 or 1, got {v:?}")),
            },
            "--h2-font-check" => match v.parse::<u64>() {
                Ok(n) => h2_font_check = Some(n),
                Err(_) => errs.push(format!("--h2-font-check wants a frame, got {v:?}")),
            },
            "--h2-revive" => match v {
                "1" => h2_revive = true,
                "0" => h2_revive = false,
                _ => errs.push(format!("--h2-revive wants 0 or 1, got {v:?}")),
            },
            "--h2-boss-refill" => match v {
                "0" => h2_boss_refill = false,
                "1" => h2_boss_refill = true,
                _ => errs.push(format!("--h2-boss-refill wants 0 or 1, got {v:?}")),
            },
            "--h2-benchmark" => match v.split_once(':').map(|(a, b)| (a.parse::<u64>(), b.parse::<u64>())) {
                Some((Ok(start), Ok(end))) if start > 548 && end > start => h2_benchmark = Some((start, end)),
                _ => errs.push(format!("--h2-benchmark wants start:end, 548 < start < end, got {v:?}")),
            },
            "--h2-mob-hp" => {
                let p: Vec<&str> = v.split(':').collect();
                match (p.first().and_then(|x| x.parse::<u64>().ok()), p.get(1).and_then(|x| x.parse::<i32>().ok())) {
                    (Some(f), Some(hp)) if p.len() == 2 && hp >= 0 => h2_mob_hp.push((f, hp)),
                    _ => errs.push(format!("--h2-mob-hp wants frame:hp (hp >= 0), got {v:?}")),
                }
            }
            "--h2-die" => {
                for spec in v.split(',') {
                    match spec.parse::<u64>() {
                        Ok(n) => h2_die.push(n),
                        Err(_) => errs.push(format!("--h2-die wants frame[,frame..], got {v:?}")),
                    }
                }
            }
            "--h2-combo-watch" => match v.split_once(':').map(|(a, b)| (a.parse::<u64>(), b.parse::<u64>())) {
                Some((Ok(a), Ok(b))) if b >= a => h2_combo_watch = Some((a, b)),
                _ => errs.push(format!("--h2-combo-watch wants from:to, got {v:?}")),
            },
            "--h2-confirm-check" => match v.parse::<u64>() {
                Ok(n) => h2_confirm_check = Some(n),
                Err(_) => errs.push(format!("--h2-confirm-check wants a frame, got {v:?}")),
            },
            "--h2-autoloot" => match v {
                "1" => h2_autoloot = true,
                "0" => h2_autoloot = false,
                _ => errs.push(format!("--h2-autoloot wants 0 or 1, got {v:?}")),
            },
            "--h2-drop-as" => {
                let parts: Vec<&str> = v.split(':').collect();
                match (parts.first().and_then(|x| x.parse::<u64>().ok()), parts.get(1).and_then(|x| x.parse::<i8>().ok()), parts.get(2).and_then(|x| x.parse::<i8>().ok())) {
                    (Some(f), Some(t), Some(i)) if parts.len() == 3 => h2_drop_as.push((f, t, i, true)),
                    (Some(f), Some(t), Some(i)) if parts.len() == 4 && parts[3] == "stay" => h2_drop_as.push((f, t, i, false)),
                    _ => errs.push(format!("--h2-drop-as wants <frame:type:id[:stay]>, got {v:?}")),
                }
            }
            "--h2-drop-life" => match v {
                "1" => h2_drop_life = true,
                "0" => h2_drop_life = false,
                _ => errs.push(format!("--h2-drop-life wants 0 or 1, got {v:?}")),
            },
            "--h2-shipped" => match v {
                "1" => h2_shipped = true,
                "0" => h2_shipped = false,
                _ => errs.push(format!("--h2-shipped wants 0 or 1, got {v:?}")),
            },
            "--h2-venom" => match v {
                "1" => h2_venom = true,
                "0" => h2_venom = false,
                _ => errs.push(format!("--h2-venom wants 0 or 1, got {v:?}")),
            },
            "--h2-shop-sell" => match v {
                "1" => h2_shop_sell = true,
                "0" => h2_shop_sell = false,
                _ => errs.push(format!("--h2-shop-sell wants 0 or 1, got {v:?}")),
            },
            "--h2-boss-crit" => match v {
                "1" => h2_boss_crit = true,
                "0" => h2_boss_crit = false,
                _ => errs.push(format!("--h2-boss-crit wants 0 or 1, got {v:?}")),
            },
            "--h2-boss-pierce" => match v.parse::<u32>() {
                Ok(n) if n <= guest_build::BossPiercePatch::PCT_MAX => h2_boss_pierce = n,
                _ => errs.push(format!("--h2-boss-pierce wants 0..={}, got {v:?}", guest_build::BossPiercePatch::PCT_MAX)),
            },
            "--h2-grade-line" => match v {
                "1" => h2_grade_line = true,
                "0" => h2_grade_line = false,
                _ => errs.push(format!("--h2-grade-line wants 0 or 1, got {v:?}")),
            },
            "--h2-hell-drop" => match v {
                "1" => h2_hell_drop = true,
                "0" => h2_hell_drop = false,
                _ => errs.push(format!("--h2-hell-drop wants 0 or 1, got {v:?}")),
            },
            "--h2-gear" => match v {
                "1" => h2_gear = true,
                "0" => h2_gear = false,
                _ => errs.push(format!("--h2-gear wants 0 or 1, got {v:?}")),
            },
            "--h2-gear-wear" => match v.split_once(':').map(|(f, list)| {
                let items: Option<Vec<(u8, u8)>> = list
                    .split('+')
                    .filter(|x| !x.is_empty())
                    .map(|x| x.split_once('.').and_then(|(t, i)| Some((t.parse::<u8>().ok()?, i.parse::<u8>().ok()?))))
                    .collect();
                (f.parse::<u64>(), items)
            }) {
                Some((Ok(f), Some(items))) => h2_gear_wear.push((f, items)),
                _ => errs.push(format!("--h2-gear-wear wants frame:type.id+type.id…, got {v:?}")),
            },
            "--h2-guardian" => match v {
                "1" => h2_guardian = true,
                "0" => h2_guardian = false,
                _ => errs.push(format!("--h2-guardian wants 0 or 1, got {v:?}")),
            },
            "--h2-colour-init" => match v {
                "1" => h2_colour_init = true,
                "0" => h2_colour_init = false,
                _ => errs.push(format!("--h2-colour-init wants 0 or 1, got {v:?}")),
            },
            "--h2-guardian-word" => match v.split_once(':').map(|(i, x)| (i.parse::<usize>(), x.parse::<u32>())) {
                Some((Ok(i), Ok(x))) if i < guest_build::GuardianPatch::WORDS => h2_guardian_words.push((i, x)),
                _ => errs.push(format!("--h2-guardian-word wants index:value (index 0..{}), got {v:?}", guest_build::GuardianPatch::WORDS - 1)),
            },
            "--h2-hell-gate" => match v {
                "1" => h2_hell_gate = Some(true),
                "0" => h2_hell_gate = Some(false),
                _ => errs.push(format!("--h2-hell-gate wants 0 or 1, got {v:?}")),
            },
            "--h2-refine-test" => {
                let parts: Vec<_> = v.split(':').collect();
                let parsed = (|| Some((parts.first()?.parse::<u64>().ok()?, parts.get(1)?.parse::<u32>().ok()?, *parts.get(2)? == "yes", parts.get(3)?.parse::<u8>().ok()?, parts.get(4).is_some_and(|v| *v == "warehouse"))))();
                if parsed.is_some() { h2_refine_test = parsed; } else { errs.push(format!("invalid refine test: {v}")); }
            },
            "--h2-shop" => match v.parse::<u64>() {
                Ok(f) => h2_shop.push(f),
                Err(_) => errs.push(format!("--h2-shop wants a frame, got {v:?}")),
            },
            "--h2-shop-orb" => match v.parse::<u64>() {
                Ok(f) => h2_shop_orb.push(f),
                Err(_) => errs.push(format!("--h2-shop-orb wants a frame, got {v:?}")),
            },
            "--h2-diff-gate" => match v {
                "1" => h2_diff_gate = true,
                "0" => h2_diff_gate = false,
                _ => errs.push(format!("--h2-diff-gate wants 0 or 1, got {v:?}")),
            },
            "--h2-menu-release" => match v {
                "1" => h2_menu_release = true,
                "0" => h2_menu_release = false,
                _ => errs.push(format!("--h2-menu-release wants 0 or 1, got {v:?}")),
            },
            "--h2-shop-plate" => match v.parse::<u64>() {
                Ok(f) => h2_shop_plate.push(f),
                Err(_) => errs.push(format!("--h2-shop-plate wants a frame, got {v:?}")),
            },
            "--h2-flight" => {
                let (f, town) = v.split_once(':').map_or((v, None), |(f, t)| (f, Some(t)));
                match (f.parse::<u64>(), town.map(|t| t.parse::<usize>())) {
                    (Ok(f), None) => h2_flight.push((f, None)),
                    (Ok(f), Some(Ok(t))) if t < guest_build::H2_FLIGHT_TOWNS.len() => h2_flight.push((f, Some(t))),
                    _ => errs.push(format!("--h2-flight wants <frame[:town 0..{}]>, got {v:?}", guest_build::H2_FLIGHT_TOWNS.len() - 1)),
                }
            }
            "--h2-park" => match v.split_once(':').map(|(f, on)| (f.parse::<u64>(), on)) {
                Some((Ok(f), "1")) => h2_park.push((f, true)),
                Some((Ok(f), "0")) => h2_park.push((f, false)),
                _ => errs.push(format!("--h2-park wants <frame:0|1>, got {v:?}")),
            },
            "--h2-tools" => match v {
                "1" => h2_tools = true,
                "0" => h2_tools = false,
                _ => errs.push(format!("--h2-tools wants 0 or 1, got {v:?}")),
            },
            "--h2-tools-tap" => {
                for spec in v.split(',') {
                    let p: Vec<_> = spec.split(':').collect();
                    match (p.len(), p.first().and_then(|s| s.parse::<u64>().ok()), p.get(1).and_then(|s| s.parse::<u32>().ok()), p.get(2).and_then(|s| s.parse::<u32>().ok())) {
                        (3, Some(f), Some(x), Some(y)) => h2_tools_tap.push((f, x, y)),
                        _ => errs.push(format!("--h2-tools-tap wants f:x:y[,..], got {spec:?}")),
                    }
                }
            }
            "--h2-alchemist" => match v.parse::<u64>() {
                Ok(f) => h2_alchemist.push(f),
                Err(_) => errs.push(format!("--h2-alchemist wants a frame, got {v:?}")),
            },
            "--h2-craft-guard" => match v {
                "0" => h2_craft_guard = false,
                "1" => h2_craft_guard = true,
                _ => errs.push(format!("--h2-craft-guard wants 0 or 1, got {v:?}")),
            },
            "--h2-picker-page" => {
                for spec in v.split(',') {
                    match spec.split_once(':').map(|(a, b)| (a.parse::<u64>(), b.parse::<u8>())) {
                        Some((Ok(f), Ok(p))) if p < 5 => h2_picker_page.push((f, p)),
                        _ => errs.push(format!("--h2-picker-page wants frame:page (page 0..4), got {spec:?}")),
                    }
                }
            }
            "--h2-bags127" => match v {
                "1" => h2_bags127 = true,
                "0" => h2_bags127 = false,
                _ => errs.push(format!("--h2-bags127 wants 0 or 1, got {v:?}")),
            },
            "--h2-clear-hook" => match v {
                "1" => h2_clear_hook = true,
                "0" => h2_clear_hook = false,
                _ => errs.push(format!("--h2-clear-hook wants 0 or 1, got {v:?}")),
            },
            "--h2-gl" => match v {
                "1" => h2_gl = true,
                "0" => h2_gl = false,
                _ => errs.push(format!("--h2-gl wants 0 or 1, got {v:?}")),
            },
            "--h2-slot-focus" => match v {
                "1" => h2_slot_focus = true,
                "0" => h2_slot_focus = false,
                _ => errs.push(format!("--h2-slot-focus wants 0 or 1, got {v:?}")),
            },
            "--h2-slot-last" => match v.parse::<u8>() {
                Ok(n) if (1..=guest_build::SaveSlotsPatch::SLOTS).contains(&n) => h2_slot_last = Some(n - 1),
                _ => errs.push(format!("--h2-slot-last wants a save slot 1..=5, got {v:?}")),
            },
            "--h2-cd-clock" => match v {
                "1" => h2_cd_clock = true,
                "0" => h2_cd_clock = false,
                _ => errs.push(format!("--h2-cd-clock wants 0 or 1, got {v:?}")),
            },
            "--h2-item-icon" => match v {
                "1" => h2_item_icon = true,
                "0" => h2_item_icon = false,
                _ => errs.push(format!("--h2-item-icon wants 0 or 1, got {v:?}")),
            },
            "--h2-beast-tune" => match v {
                "1" => h2_beast_tune = true,
                "0" => h2_beast_tune = false,
                _ => errs.push(format!("--h2-beast-tune wants 0 or 1, got {v:?}")),
            },
            "--h2-beast-recovery" => match v {
                "1" => h2_beast_recovery = true,
                "0" => h2_beast_recovery = false,
                _ => errs.push(format!("--h2-beast-recovery wants 0 or 1, got {v:?}")),
            },
            "--h2-rogue" => match v {
                "1" => h2_rogue = true,
                "0" => h2_rogue = false,
                _ => errs.push(format!("--h2-rogue wants 0 or 1, got {v:?}")),
            },
            "--h2-teleport-gate" => match v {
                "1" => h2_teleport_gate = true,
                "0" => h2_teleport_gate = false,
                _ => errs.push(format!("--h2-teleport-gate wants 0 or 1, got {v:?}")),
            },
            "--h2-demon" => match v {
                "1" => h2_demon = true,
                "0" => h2_demon = false,
                _ => errs.push(format!("--h2-demon wants 0 or 1, got {v:?}")),
            },
            "--h2-sky-draw" => match v {
                "1" => h2_sky_draw = true,
                "0" => h2_sky_draw = false,
                _ => errs.push(format!("--h2-sky-draw wants 0 or 1, got {v:?}")),
            },
            "--h2-buff-refresh" => match v {
                "1" => h2_buff_refresh = true,
                "0" => h2_buff_refresh = false,
                _ => errs.push(format!("--h2-buff-refresh wants 0 or 1, got {v:?}")),
            },
            "--h2-cd-flip" => {
                let p: Vec<&str> = v.split(':').collect();
                let f1 = p.first().and_then(|x| x.parse::<u64>().ok());
                let f2 = p.get(1).and_then(|x| x.parse::<u64>().ok());
                let g = p.get(2).and_then(|x| x.parse::<u8>().ok());
                let old = match p.get(3) {
                    None => Some(false),
                    Some(&"old") => Some(true),
                    Some(_) => None,
                };
                match (f1, f2, g, old) {
                    (Some(f1), Some(f2), Some(g), Some(old)) if p.len() <= 4 && g < 4 && f1 < f2 => h2_cd_flip.push((f1, f2, g, old)),
                    _ => errs.push(format!("--h2-cd-flip wants off:on:slot[:old] (two frames, off < on, slot 0..3), got {v:?}")),
                }
            }
            "--h2-clear" => match v.parse::<u64>() {
                Ok(f) => h2_clear.push(f),
                Err(_) => errs.push(format!("--h2-clear wants a frame, got {v:?}")),
            },
            "--h2-quest" => {
                let mut it = v.split(':');
                match (it.next().and_then(|x| x.parse::<u64>().ok()), it.next().and_then(|x| x.parse::<u8>().ok()), it.next().and_then(|x| x.parse::<u8>().ok()), it.next()) {
                    (Some(f), Some(i), Some(st), None) if usize::from(i) < guest_build::H2_QUESTS && st < 4 => h2_quest.push((f, i, st)),
                    _ => errs.push(format!("--h2-quest wants frame:quest:state (quest 0..{}, state 0..=3), got {v:?}", guest_build::H2_QUESTS - 1)),
                }
            }
            "--h2-costume-dir" => costume_dir = Some(PathBuf::from(v)),
            "--fx-lib" => fx_lib = Some(PathBuf::from(v)),
            "--h2-costume-grant" => match v.split_once(':').map(|(f, n)| (f.parse::<u64>(), n.parse::<u16>())) {
                Some((Ok(f), Ok(n))) => costume_grant.push((f, n)),
                _ => errs.push(format!("--h2-costume-grant wants frame:costume, got {v:?}")),
            },
            "--h2-costume-give" => {
                let p: Vec<&str> = v.split(':').collect();
                let slot = p.get(3).map(|k| guest_build::H2_AV_SLOT_KEYS.iter().position(|w| w == k).map(|s| s as u8));
                match (p.first().and_then(|t| t.parse::<u64>().ok()), p.get(1).and_then(|t| t.parse::<u16>().ok()), p.get(2).map(|t| t.parse::<u8>().ok()), slot) {
                    (Some(f), Some(n), None, None) => costume_give.push((f, n, 0, None)),
                    (Some(f), Some(n), Some(Some(m)), None) if m <= 2 => costume_give.push((f, n, m, None)),
                    (Some(f), Some(n), Some(Some(m)), Some(Some(s))) if m <= 2 && p.len() == 4 => costume_give.push((f, n, m, Some(s))),
                    _ => errs.push(format!("--h2-costume-give wants frame:set[:mode 0..2[:head|armor|shoes|weapon|offhand]], got {v:?}")),
                }
            }
            "--h2-costume-marker" => costume_marker = Some(v.to_string()),
            "--h2-equip-ui" => match v.parse::<u8>() {
                Ok(x) => h2_equip_ui = x != 0,
                Err(_) => errs.push(format!("--h2-equip-ui wants 0|1, got {v:?}")),
            },
            "--h2-costume-catalog" => costume_catalog_file = Some(PathBuf::from(v)),
            "--h2-costume-wear" => match v.split_once(':') {
                Some((f, "-")) if f.parse::<u64>().is_ok() => costume_wear.push((f.parse().unwrap(), None)),
                Some((f, n)) if f.parse::<u64>().is_ok() && n.parse::<u16>().is_ok() => costume_wear.push((f.parse().unwrap(), Some(n.parse().unwrap()))),
                _ => errs.push(format!("--h2-costume-wear wants frame:costume or frame:-, got {v:?}")),
            },
            "--h2-avatar" => match v.split_once(':') {
                Some((f, spec)) => match f.parse::<u64>() {
                    Ok(f) => h2_avatar_specs.push((f, spec.to_string())),
                    Err(_) => errs.push(format!("--h2-avatar wants frame:spec, got {v:?}")),
                },
                None => errs.push(format!("--h2-avatar wants frame:spec (spec = body:<n>[:<pal>],arm:…,leg:…), got {v:?}")),
            },
            "--h2-drop-up" => {
                for spec in v.split(',') {
                    match spec.trim().parse::<u64>() {
                        Ok(f) => h2_drop_up.push(f),
                        Err(_) => errs.push(format!("--h2-drop-up wants frame numbers, got {spec:?}")),
                    }
                }
            }
            "--h2-walk-guard" => match v {
                "1" => h2_walk_guard = true,
                "0" => h2_walk_guard = false,
                _ => errs.push(format!("--h2-walk-guard wants 0 or 1, got {v:?}")),
            },
            "--h2-turn" => match v {
                "1" => h2_turn = true,
                "0" => h2_turn = false,
                _ => errs.push(format!("--h2-turn wants 0 or 1, got {v:?}")),
            },
            "--h2-reverse" => match v {
                "1" => h2_reverse = true,
                "0" => h2_reverse = false,
                _ => errs.push(format!("--h2-reverse wants 0 or 1, got {v:?}")),
            },
            "--h2-relatch" => match v {
                "1" => h2_relatch = true,
                "0" => h2_relatch = false,
                _ => errs.push(format!("--h2-relatch wants 0 or 1, got {v:?}")),
            },
            "--h2-kill" => {
                let mut it = v.split(':');
                match (it.next().and_then(|x| x.parse::<u64>().ok()), it.next().and_then(|x| x.parse::<u32>().ok())) {
                    (Some(f), Some(n)) if (1..=100_000).contains(&n) => h2_kill = Some((f, n)),
                    _ => errs.push(format!("--h2-kill wants frame:count (count 1..=100000), got {v:?}")),
                }
            }
            "--h2-skill" => match v.parse::<u64>() {
                Ok(f) => h2_skill = Some(f),
                Err(_) => errs.push(format!("--h2-skill wants a frame number, got {v:?}")),
            },
            "--h2-slots-from" => match v.parse::<u64>() {
                Ok(x) => h2_slots_from = x,
                Err(_) => errs.push(format!("--h2-slots-from wants a frame, got {v:?}")),
            },
            "--h2-slots" => match v {
                "3" => h2_slots = 3,
                "5" => h2_slots = 5,
                _ => errs.push(format!("--h2-slots wants 3 or 5, got {v:?}")),
            },
            "--h2-minimap" => match v.parse::<u32>() {
                Ok(x) if (40..=1000).contains(&x) => h2_minimap = x,
                _ => errs.push(format!("--h2-minimap wants the panel side in phone px (40..=1000; 280 = S25/S9), got {v:?}")),
            },
            "--h2-hd-audit" => match v {
                "1" => HD_AUDIT.store(true, Ordering::Relaxed),
                "0" => HD_AUDIT.store(false, Ordering::Relaxed),
                _ => errs.push(format!("--h2-hd-audit wants 1 or 0, got {v:?}")),
            },
            "--h2-hd" => match v.parse::<u32>() {
                Ok(x) if x <= 8 => h2_hd = x,
                _ => errs.push(format!("--h2-hd wants the window scale 0..=8 (0 = off, 5 = S25), got {v:?}")),
            },
            "--h2-raid-gate" => match v {
                "1" => h2_raid_gate = Some(true),
                "0" => h2_raid_gate = Some(false),
                _ => errs.push(format!("--h2-raid-gate wants 1 (the gate-bearing 신전) or 0 (the shipped one), got {v:?}")),
            },
            "--h2-boss-raid" => match v {
                "1" => h2_boss_raid = true,
                "0" => h2_boss_raid = false,
                _ => errs.push(format!("--h2-boss-raid wants 1 or 0, got {v:?}")),
            },
            "--h2-raid-autokill" => match v.parse() {
                Ok(n) => h2_raid_autokill = Some(n),
                Err(_) => errs.push(format!("--h2-raid-autokill wants a frame, got {v:?}")),
            },
            "--h2-raid-autokill-last" => match v {
                "1" => h2_raid_autokill_last = true,
                "0" => h2_raid_autokill_last = false,
                _ => errs.push(format!("--h2-raid-autokill-last wants 1 or 0, got {v:?}")),
            },
            "--h2-raid-notice-check" => match v.parse() {
                Ok(n) => h2_raid_notice_check = Some(n),
                Err(_) => errs.push(format!("--h2-raid-notice-check wants a frame, got {v:?}")),
            },
            "--h2-kill-any" => match v {
                "1" => h2_kill_any = true,
                "0" => h2_kill_any = false,
                _ => errs.push(format!("--h2-kill-any wants 1 or 0, got {v:?}")),
            },
            "--h2-status" => match v {
                "1" => h2_status = true,
                "0" => h2_status = false,
                _ => errs.push(format!("--h2-status wants 1 or 0, got {v:?}")),
            },
            "--h2-pose-audit" => match v {
                "1" => h2_pose_audit = true,
                "0" => h2_pose_audit = false,
                _ => errs.push(format!("--h2-pose-audit wants 1 or 0, got {v:?}")),
            },
            "--h2-difficulty" => {
                let mut it = v.splitn(2, ':');
                match (it.next().and_then(|x| x.parse::<u64>().ok()), it.next().and_then(|x| x.parse::<u8>().ok())) {
                    (Some(f), Some(d)) if usize::from(d) < guest_build::DIFFICULTY_PCT.len() => h2_difficulty.push((f, d)),
                    _ => errs.push(format!("--h2-difficulty wants frame:level (level 0..=2), got {v:?}")),
                }
            }
            "--h2-drop-rate" => {
                let mut it = v.splitn(2, ':');
                match (it.next().and_then(|x| x.parse::<u64>().ok()), it.next().and_then(|x| x.parse::<u8>().ok())) {
                    (Some(f), Some(d)) if usize::from(d) < guest_build::DROP_RATE_KEEP.len() => h2_drop_rate.push((f, d)),
                    _ => errs.push(format!("--h2-drop-rate wants frame:level (level 0..=2), got {v:?}")),
                }
            }
            "--h2-exp" => match v.parse::<u8>() {
                Ok(x) if (1..=64).contains(&x) => h2_exp = Some(x),
                _ => errs.push(format!("--h2-exp wants a multiplier 1..=64, got {v:?}")),
            },
            "--h2-exp-gain" => {
                let mut it = v.split(':');
                match (it.next().and_then(|x| x.parse::<u64>().ok()), it.next().and_then(|x| x.parse::<i32>().ok())) {
                    (Some(f), Some(n)) if (1..=1_000_000).contains(&n) => h2_exp_gain.push((f, n)),
                    _ => errs.push(format!("--h2-exp-gain wants frame:amount (amount 1..=1000000), got {v:?}")),
                }
            }
            "--h2-page" => {
                let mut it = v.split(':');
                match (it.next().and_then(|x| x.parse::<u64>().ok()), it.next().and_then(|x| x.parse::<u8>().ok())) {
                    (Some(f), Some(k)) if (k as usize) < guest_build::Hero2Java::BAG_PAGES => h2_page.push((f, k)),
                    _ => errs.push(format!("--h2-page wants frame:0..{}, got {v:?}", guest_build::Hero2Java::BAG_PAGES - 1)),
                }
            }
            "--h2-move" => {
                let p: Vec<&str> = v.split(':').collect();
                let bag = |x: &str| match x {
                    "a" => Some(b'a'),
                    "b" => Some(b'b'),
                    "c" => Some(b'c'),
                    "w" => Some(b'w'),
                    _ => None,
                };
                match p.as_slice() {
                    // The count is optional: without it the whole record moves as the
                    // OBJECT it is; with it the move is the split the app performs after
                    // the game's own 개수 box has been answered.
                    [f, sb, db, idx] | [f, sb, db, idx, _] => {
                        // ★ 2026-10-13: count 0 = the OBJECT move even for a stack — the move
                        // before 2026-10-13, which left one cell per move (how a warehouse got
                        // the cells `--h2-fold` folds back).
                        let n = match p.as_slice() {
                            [_, _, _, _, n] => match n.parse::<u8>().ok() {
                                Some(n) => Some(Some(n)),
                                None => {
                                    errs.push(format!("--h2-move count must be 0.., got {v:?}"));
                                    None
                                }
                            },
                            _ => Some(None),
                        };
                        match (f.parse::<u64>().ok(), bag(sb), bag(db), idx.parse::<u8>().ok(), n) {
                            (Some(f), Some(sb), Some(db), Some(idx), Some(n)) if sb != db && idx < 128 => h2_move.push((f, sb, db, idx, n)),
                            (_, _, _, _, None) => {}
                            _ => errs.push(format!("--h2-move wants frame:a|b|c|w:a|b|c|w:idx[:count] (distinct bags, idx 0..127), got {v:?}")),
                        }
                    }
                    _ => errs.push(format!("--h2-move wants frame:src:dst:idx[:count], got {v:?}")),
                }
            }
            "--h2-fold" => match v.split_once(':').map(|(f, b)| (f.parse::<u64>().ok(), b)) {
                Some((Some(f), b @ ("a" | "b" | "c" | "w"))) => h2_fold.push((f, b.as_bytes()[0])),
                _ => errs.push(format!("--h2-fold wants frame:a|b|c|w, got {v:?}")),
            },
            "--h2-combo-break" => {
                let (f, mode) = v.split_once(':').map_or((&v[..], None), |(f, m)| (f, Some(m)));
                match (f.parse::<u64>(), mode) {
                    (Ok(f), None) => h2_combo_break = Some((f, false)),
                    (Ok(f), Some("old")) => h2_combo_break = Some((f, true)),
                    _ => errs.push(format!("--h2-combo-break wants frame[:old], got {v:?}")),
                }
            }
            "--h2-reset" => {
                let mut it = v.split(':');
                let f = it.next().and_then(|x| x.parse::<u64>().ok());
                let what = it.next();
                let apply = it.next() == Some("apply");
                match (f, what) {
                    (Some(f), Some("stat")) => h2_reset = Some((f, H2ResetWhat::Stat, apply)),
                    (Some(f), Some("skill")) => h2_reset = Some((f, H2ResetWhat::Skill, apply)),
                    (Some(f), Some("repair")) => h2_reset = Some((f, H2ResetWhat::Repair, apply)),
                    (Some(f), Some("elixir")) => h2_reset = Some((f, H2ResetWhat::Elixir, apply)),
                    _ => errs.push(format!("--h2-reset wants frame:stat|skill|repair|elixir[:apply], got {v:?}")),
                }
            }
            "--h2-heal" => {
                let mut it = v.split(':');
                match (
                    it.next().and_then(|x| x.parse::<u64>().ok()),
                    it.next().and_then(|x| x.parse::<u8>().ok()),
                ) {
                    (Some(f), Some(t)) => h2_heal = Some((f, t)),
                    _ => errs.push(format!("--h2-heal wants frame:type, got {v:?}")),
                }
            }
            "--h2-commit" => {
                let mut it = v.split(':');
                match (
                    it.next().and_then(|x| x.parse::<u64>().ok()),
                    it.next().and_then(|x| x.parse::<u8>().ok()),
                ) {
                    (Some(f), Some(g)) if g < 4 => h2_commit = Some((f, g)),
                    _ => errs.push(format!("--h2-commit wants frame:guest(0..3), got {v:?}")),
                }
            }
            "--h2-use" => {
                let mut it = v.split(':');
                match (
                    it.next().and_then(|x| x.parse::<u64>().ok()),
                    it.next().and_then(|x| x.parse::<u8>().ok()),
                ) {
                    (Some(f), Some(t)) => h2_use = Some((f, t)),
                    _ => errs.push(format!("--h2-use wants frame:type, got {v:?}")),
                }
            }
            "--verify-offsets" => verify_offsets = v == "1" || v == "true",
            "--host-save" => host_save_frame = num!(v),
            "--host-levelup" => host_levelup_frame = num!(v),
            "--dump-mem" => {
                for spec in v.split(',') {
                    let mut it = spec.split(':');
                    let (Some(b), Some(l)) = (it.next(), it.next()) else { continue };
                    let parse = |s: &str| -> Option<u64> {
                        let s = s.trim();
                        let s = s.strip_prefix("0x").unwrap_or(s);
                        u64::from_str_radix(s, 16).ok()
                    };
                    if let (Some(b), Some(l)) = (parse(b), parse(l)) {
                        dump_mem.push((b as u32, l as usize));
                    }
                }
            }
            "--dump-passive" => match v {
                "1" => dump_passive = true,
                "0" => dump_passive = false,
                _ => errs.push(format!("--dump-passive wants 0 or 1, got {v:?}")),
            },
            "--dump" => {
                dumps.clear();
                for spec in v.split(',') {
                    match spec.trim().parse() {
                        Ok(n) => dumps.push(n),
                        Err(_) => errs.push(format!("--dump wants frame numbers, got {spec:?}")),
                    }
                }
            }
            "--poke" => {
                for spec in v.split(',') {
                    let mut it = spec.split(':');
                    let (Some(f), Some(a), Some(hex)) = (it.next(), it.next(), it.next()) else {
                        continue;
                    };
                    let hex: String = hex.chars().filter(|c| !c.is_whitespace()).collect();
                    let bytes: Option<Vec<u8>> = (hex.len() % 2 == 0)
                        .then(|| (0..hex.len()).step_by(2).map(|i| u8::from_str_radix(&hex[i..i + 2], 16).ok()).collect())
                        .flatten();
                    let addr = u32::from_str_radix(a.trim().trim_start_matches("0x"), 16).ok();
                    if let (Ok(f), Some(addr), Some(bytes)) = (f.parse::<u64>(), addr, bytes) {
                        pokes.push((f, addr, bytes));
                    } else {
                        errs.push(format!("bad --poke spec {spec:?} (want frame:hexaddr:hexbytes)"));
                    }
                }
            }
            "--keys" => {
                for spec in v.split(',') {
                    let mut it = spec.split(':');
                    let (Some(f), Some(name)) = (it.next(), it.next()) else {
                        errs.push(format!("bad --keys spec {spec:?} (want frame:KEY[:hold])"));
                        continue;
                    };
                    let hold: u64 = match it.next() {
                        None => 6,
                        Some(h) => match h.trim().parse() {
                            Ok(n) => n,
                            Err(_) => {
                                errs.push(format!("--keys hold wants a number, got {h:?}"));
                                6
                            }
                        },
                    };
                    match (f.trim().parse::<u64>(), key_of(name)) {
                        (Ok(f), Some(kc)) => keys.push((f, kc, hold)),
                        (Err(_), _) => errs.push(format!("--keys frame wants a number, got {f:?}")),
                        (_, None) => errs.push(format!("--keys: {name:?} is not a key name (see --help)")),
                    }
                }
            }
            _ => errs.push(format!("unknown flag {k:?} — see --help")),
        }
        i += 2;
    }
    if zip.is_none() {
        errs.push("--zip <archive.zip> is required (there is no default: booting the wrong game silently is exactly what this guards)".to_string());
    }
    if !errs.is_empty() {
        eprintln!("[probe] refusing to run — {} problem(s) on the command line:", errs.len());
        for e in &errs {
            eprintln!("    {e}");
        }
        eprintln!("[probe] `wie_probe --help` lists every flag.");
        std::process::exit(2);
    }
    if let Some(path) = &audio_log {
        match fs::File::create(path) {
            Ok(file) => {
                let _ = AUDIO_LOG.set(Mutex::new(file));
                eprintln!("[probe] audio log -> {path:?}");
            }
            Err(e) => {
                eprintln!("[probe] --audio-log {path:?}: {e}");
                std::process::exit(2);
            }
        }
    }
    if let Some(path) = &audio_wav {
        audio_fm = true;
        let cap = FmCapture {
            fm: ma2_fm::Ma2Fm::new(FmCapture::RATE as u32),
            pcm: Vec::new(),
            sfx: Vec::new(),
        };
        let _ = AUDIO_WAV.set((path.clone(), Mutex::new(cap)));
    }
    if audio_fm {
        AUDIO_FM.store(true, Ordering::Relaxed);
        eprintln!("[probe] audio: hero2's songs take the FM path (the app's default)");
    }
    let zip = zip.unwrap();
    if let Some(on) = h2_raid_gate {
        RAID_FORCED.store(true, std::sync::atomic::Ordering::SeqCst);
        RAID_ON.store(on, std::sync::atomic::Ordering::SeqCst);
        eprintln!("[h2-raid] the temple's raid gate forced {} (--h2-raid-gate)", if on { "ON" } else { "off" });
    }
    fs::create_dir_all(&out).unwrap();
    eprintln!("[probe] zip={zip:?} screen={w}x{h} frames={frames} ms/frame={ms_per_frame} dumps={dumps:?} keys={keys:?}");

    // --- identify the build BEFORE anything is constructed ---
    //
    // The gate runs on the archive alone, so a refused command line costs no boot and
    // — more to the point — nothing has been written anywhere by the time it refuses.
    let bytes = fs::read(&zip).expect("game zip");
    let files = extract_zip(&bytes).expect("extract zip");
    // ★ 2026-10-16 보스 레이드: the gate-bearing 신전 the raid gate serves (tools_re/raid_gate.py's map 112).
    match files.get(guest_build::H2_RAID_GATE_VARIANT) {
        Some(v) => {
            let _ = RAID_VARIANT.set(v.clone());
        }
        None => eprintln!("[h2-raid] the asset has no {} — the temple's raid gate cannot show", guest_build::H2_RAID_GATE_VARIANT),
    }
    let is_ktf = KtfEmulator::loadable_archive(&files);
    let is_lgt = !is_ktf && LgtEmulator::loadable_archive(&files);
    if !is_ktf && !is_lgt {
        eprintln!("[probe] not a KTF/LGT archive");
        std::process::exit(2);
    }
    // ★ 의상 아바타: the costume sheets this jar carries — the only sprites `--h2-avatar` may name.
    let avatar_sheets = match files.iter().find(|(name, _)| name.ends_with(".jar") && !name.contains('/')) {
        Some((_, jar)) => guest_build::h2_avatar_sheets(jar),
        None => guest_build::AvatarSheets::shipped(),
    };
    let h2_avatar: Vec<(u64, [guest_build::AvatarPart; guest_build::AvatarPatch::N])> =
        h2_avatar_specs.iter().map(|(f, spec)| (*f, guest_build::h2_avatar_parse(spec, &avatar_sheets))).collect();
    // ★ A3: the jar's costume catalog (`hero/costume.txt`), as the app reads it at boot.
    let costume_catalog = match &costume_catalog_file {
        Some(path) => guest_build::h2_costume_catalog_parse(&fs::read(path).unwrap_or_else(|e| panic!("--h2-costume-catalog {path:?}: {e}"))),
        None => files.iter().find(|(name, _)| name.ends_with(".jar") && !name.contains('/')).map(|(_, jar)| guest_build::h2_costume_catalog(jar)).unwrap_or_default(),
    };
    if h2_equip_ui {
        eprintln!(
            "[h2-equip-ui] catalog: {} item(s) {:?}",
            costume_catalog.len(),
            costume_catalog.iter().map(|d| format!("{} {}", d.avatar, guest_build::H2_AV_SLOT_KEYS.get(usize::from(d.slot)).unwrap_or(&"?"))).collect::<Vec<_>>()
        );
    }
    if !h2_avatar.is_empty() {
        eprintln!(
            "[h2-avatar] jar costume sheets beyond the shipped: body {:?} · arms {:?} · legs {:?} · head {:?} · weapon {:?} · off-hand {:?}",
            avatar_sheets.added(1),
            avatar_sheets.added(2),
            avatar_sheets.added(3),
            avatar_sheets.added(0),
            avatar_sheets.added(6),
            avatar_sheets.added(7)
        );
        for (f, parts) in &h2_avatar {
            eprintln!(
                "[h2-avatar] f={f} wants body {:?} · arms {:?} · legs {:?} · head {:?} · weapon {:?} · off-hand {:?} (a sheet the jar lacks for the class passes through)",
                parts[0], parts[1], parts[2], parts[3], parts[4], parts[5]
            );
        }
    }
    // The archive's AID = the jar's name — the key the in-memory filesystem files saves under.
    let costume_aid = files.keys().find(|k| k.ends_with(".jar") && !k.contains('/')).map(|k| k.trim_end_matches(".jar").to_string()).unwrap_or_default();
    let fp = fingerprint(&files);
    eprintln!(
        "[probe] build = {} | {} md5={} | anchors {}",
        fp.name,
        fp.member,
        fp.md5,
        match fp.anchors {
            Some(b) => format!("{} (open)", b.name()),
            None => "NONE (closed)".to_string(),
        }
    );

    // Flags that need anchors. Two kinds, one gate: the writers fire guest writes and
    // guest CALLS at reversed addresses, and the readers DECODE reversed offsets — on
    // the wrong build the first kind corrupts and the second kind prints numbers that
    // look like measurements and are not. `--map-hunt` is in the list because its
    // sweep ranges (`statics_ranges`) are per-build too. `--dump-mem` is NOT: those
    // addresses come from the caller, not from this repo.
    // ★ SECOND LAYER (2026-09-09). "Anchors measured" and "guest CODE addresses
    // measured" stopped being the same question the moment hero2 got a data-side
    // anchor table: its reads are verified against the game's own STATUS screen, its
    // writes and call targets are not verified at all. So the write/call flags are
    // refused on a build whose code side is unported even though its anchors are open.
    // (On hero3 KTF/LGT both layers are open, so nothing changes there.)
    // ★ 2026-09-21: `--h2-give` / `--h2-move` are JVM calls BY NAME through the app's own
    // `JavaCall` table (guest_anchors.rs) — hero2 only, and refused everywhere else for the
    // opposite reason the hero3 call flags are refused on hero2: no other build has a JVM.
    if (h2_give.is_some() || !h2_move.is_empty() || h2_pages || h2_shift || !h2_page.is_empty() || !h2_account.is_empty() || !h2_moverow.is_empty() || !h2_movego.is_empty() || !h2_qty.is_empty() || h2_exp.is_some() || !h2_exp_gain.is_empty() || !h2_difficulty.is_empty() || !h2_drop_rate.is_empty() || h2_kill.is_some() || h2_skill.is_some() || !h2_acct_front.is_empty() || !h2_unworn.is_empty() || !h2_wear.is_empty() || !h2_quickbar.is_empty() || h2_revive || !h2_die.is_empty() || !h2_quest.is_empty() || !h2_acc.is_empty() || h2_acc_auto || !h2_acc_relabel.is_empty() || !h2_beast_skill.is_empty() || !h2_mob_place.is_empty() || !h2_mob_hurt.is_empty() || !h2_acc_word.is_empty() || !h2_gear_wear.is_empty() || !h2_alchemist.is_empty() || !h2_picker_page.is_empty() || h2_bags127 || h2_wh_give.is_some() || !h2_fold.is_empty() || !h2_shop_orb.is_empty() || !h2_flight.is_empty())
        && !matches!(fp.anchors, Some(GuestBuild::Hero2Ktf))
    {
        eprintln!("[probe] refusing to run — --h2-give / --h2-move / --h2-pages / --h2-page / --h2-exp / --h2-exp-gain / --h2-difficulty / --h2-revive / --h2-die are hero2-only (JVM calls, hero2 code patch) and this is {}", fp.name);
        std::process::exit(3);
    }
    if h2_wh_give.is_some() && h2_wh_out.is_none() {
        eprintln!("[probe] --h2-wh-give needs --h2-wh-out <file> (where the serialised warehouse goes)");
        std::process::exit(2);
    }
    let code_ok = !matches!(fp.anchors, Some(GuestBuild::Hero2Ktf));
    if !code_ok {
        let mut refused: Vec<&str> = vec![];
        // ★ 2026-09-17: `--poke` is OPEN on hero2. Its addresses come from the CALLER,
        // not from this repo's hero3 tables — the danger the gate was built against was
        // hero3 addresses being fired into hero2 memory, which `--poke` never does. And
        // hero2 now has host writes of its own (HERO2_QUICKSLOT_TOUCH_PLAN.md §4.5), so
        // the headless A/B that checks them needs exactly this. The two CALL flags stay
        // refused: no hero2 guest function has been measured for calling.
        if host_save_frame.is_some() {
            refused.push("--host-save (calls guest functions)");
        }
        if host_levelup_frame.is_some() {
            refused.push("--host-levelup (writes + calls guest functions)");
        }
        // These five DECODE hero3's CHAR offsets and map chain, spelled out inline
        // (0xe2 / 0xe4 / 0x9c6c / 0xa0c0 / `statics_ranges`). On a build with a
        // different record layout they do not fail — they print plausible-looking
        // numbers that are not measurements, which is the failure this tool exists to
        // avoid. Use `--gauges` instead: it reads through `char_layout()`.
        if find_fb {
            refused.push("--find-fb (hero3 CHAR offsets)");
        }
        if map_check {
            refused.push("--map-check (hero3 map chain)");
        }
        if map_hunt {
            refused.push("--map-hunt (hero3 sweep ranges)");
        }
        if trace_state {
            refused.push("--trace-state (hero3 CHAR offsets — use --gauges)");
        }
        if verify_offsets {
            refused.push("--verify-offsets (hero3 CHAR offsets — use --gauges)");
        }
        if !refused.is_empty() {
            eprintln!("[probe] refusing to run — this build's anchors are READ-ONLY (no guest code addresses measured):");
            for r in &refused {
                eprintln!("    {r}");
            }
            eprintln!("[probe] read-only flags that DO work here: --dump --dump-mem --profile --keys --hle --char-dump --gauges");
            std::process::exit(3);
        }
    }
    if fp.anchors.is_none() {
        let mut refused: Vec<&str> = vec![];
        if !pokes.is_empty() {
            refused.push("--poke (writes guest memory)");
        }
        if host_save_frame.is_some() {
            refused.push("--host-save (calls guest functions)");
        }
        if host_levelup_frame.is_some() {
            refused.push("--host-levelup (writes + calls guest functions)");
        }
        if find_fb {
            refused.push("--find-fb (decodes CHAR offsets)");
        }
        if map_check {
            refused.push("--map-check (decodes the map chain)");
        }
        if map_hunt {
            refused.push("--map-hunt (per-build sweep ranges)");
        }
        if char_dump.is_some() {
            refused.push("--char-dump (needs the CHAR anchor)");
        }
        if trace_state {
            refused.push("--trace-state (decodes CHAR offsets)");
        }
        if verify_offsets {
            refused.push("--verify-offsets (decodes CHAR offsets)");
        }
        if !refused.is_empty() {
            eprintln!("[probe] refusing to run — no guest anchors were measured on this build:");
            for r in &refused {
                eprintln!("    {r}");
            }
            eprintln!("[probe] read-only flags that DO work here: --dump --dump-mem --profile --keys --hle");
            eprintln!("[probe] to open the gate, reverse this build's anchors into guest_anchors.rs first.");
            std::process::exit(3);
        }
    }

    // --- boot ---
    let last = Arc::new(Mutex::new(vec![0u32; (w * h) as usize]));
    let hd_last: Arc<Mutex<Option<wie_backend::canvas::HdFrame>>> = Arc::new(Mutex::new(None));
    if h2_hd > 0 {
        wie_backend::canvas::set_hd_mode(true);
        eprintln!("[probe] HD effect mode: the effect draws are recorded and composited at {h2_hd}x into hd_<f>_*.rgba");
    }
    let redraw = Arc::new(Mutex::new(false));
    let paints = Arc::new(AtomicU64::new(0));
    let paint_ns = Arc::new(AtomicU64::new(0));
    // ★ Before the platform literal: `MemFs` reads `PRELOADED` when it is built, so
    // the tree has to be in the OnceLock by now. A missing/unreadable root is fatal
    // rather than silent — a save experiment that quietly ran against an EMPTY
    // filesystem would look like "the game started a new game", which is exactly the
    // wrong conclusion to draw from a slot screen.
    if let Some(root) = &fs_root {
        match MemFs::preload(root) {
            Ok(loaded) if loaded.is_empty() => {
                eprintln!("[probe] --fs {root:?}: no <aid>/fs/** files found — is the layout right?");
                std::process::exit(2);
            }
            Ok(loaded) => {
                eprintln!("[probe] FS root {root:?} preloaded into memory ({} file(s)):", loaded.len());
                for (key, len) in &loaded {
                    eprintln!("[probe]   {key} ({len} B)");
                }
            }
            Err(err) => {
                eprintln!("[probe] --fs {root:?}: {err}");
                std::process::exit(2);
            }
        }
    }

    let clock = Arc::new(AtomicU64::new(1_000));
    let platform = Box::new(ProbePlatform {
        screen: ProbeScreen {
            w,
            h,
            last: last.clone(),
            hd: hd_last.clone(),
            redraw: redraw.clone(),
            paints: paints.clone(),
            paint_ns: paint_ns.clone(),
        },
        fs: {
            let files = Arc::new(Mutex::new(PRELOADED.get().cloned().unwrap_or_default()));
            FS_LIVE.set(files.clone()).ok();
            MemFs { files }
        },
        db: match &db_root {
            Some(root) => Box::new(FsDbRepo {
                root: root.clone(),
                writable: db_rw,
            }),
            None => Box::new(MemDbRepo::default()),
        },
        clock: clock.clone(),
    });

    // Host sprite blitter on/off. Same binary, same deterministic input, one bool -
    // that is the whole A/B frame-hash check (see binary_patches/blit565.rs).
    wie_util::stats::set_blit_hle(hle);
    eprintln!("[probe] host blitter (HLE) {}", if hle { "ON" } else { "OFF" });
    // ★ 순서표 #8 의 A/B. 같은 바이너리·같은 결정적 입력, 다른 것은 이 불리언 하나뿐 —
    // 두 런의 벽시계 차이가 곧 프레임당 GC 의 몫이다.
    wie_util::stats::set_gc_every(gc_every);
    match gc_every {
        1 => {}
        0 => eprintln!("[probe] GC NEVER — 측정 모드. 게스트 힙이 차서 f300 부근에 죽는 것이 정상이다."),
        n => eprintln!("[probe] GC every {n} painted frames (upstream = 1)"),
    }

    if let Some(root) = &db_root {
        eprintln!("[probe] DB root {root:?} mounted {}", if db_rw { "READ-WRITE (--db-rw)" } else { "read-only" });
    }

    // TEMPORARY (2026-08-06): sampled guest profile. `ArmCore` already samples the
    // guest PC + R7 frame chain once per 1000-instruction batch; nothing ever turned
    // it on. Leaf PCs tell us WHICH guest routine burns the 2.5 M instructions a
    // frame costs, which is the only way to aim a host-side reimplementation.
    let prof_leaf: Arc<Mutex<HashMap<u32, u64>>> = Arc::new(Mutex::new(HashMap::new()));
    let prof_caller: Arc<Mutex<HashMap<(u32, u32), u64>>> = Arc::new(Mutex::new(HashMap::new()));
    let options = Options {
        enable_gdbserver: false,
        profile: if profile {
            let leaf = prof_leaf.clone();
            let caller = prof_caller.clone();
            Some(Box::new(move |batch: Vec<wie_backend::ProfileSample>| {
                let mut leaf = leaf.lock().unwrap();
                let mut caller = caller.lock().unwrap();
                for s in batch {
                    if let Some(&pc) = s.stack.first() {
                        *leaf.entry(pc).or_insert(0) += s.count;
                        if let Some(&lr) = s.stack.get(1) {
                            *caller.entry((pc, lr & !1)).or_insert(0) += s.count;
                        }
                    }
                }
            }))
        } else {
            None
        },
    };
    // ⛔ NOT `if is_ktf { GuestBuild::Ktf }` any more — that is the line that handed
    // hero3's addresses to any KTF archive. The build comes from the HASH, and it is
    // an `Option`: `None` means no address in this repo belongs to the binary that is
    // about to boot. Every caller below is behind the gate above, so reaching one of
    // these without anchors is a bug in the gate, not bad input — say so and abort
    // rather than substituting another build's numbers.
    let build = || fp.anchors.expect("anchor-dependent path reached on an unidentified build");
    let chain = || map_chain_of(build());
    let mut emu: Box<dyn Emulator> = if is_ktf {
        Box::new(KtfEmulator::from_archive(platform, files, options).expect("KTF load"))
    } else {
        Box::new(LgtEmulator::from_archive(platform, files, options).expect("LGT load"))
    };
    // ★ 2026-10-14: the effect library the demon block's FX pieces draw from — the app registers the same bytes at boot
    //   (`emulator::announce_hero2_ktf`). Nothing draws from it unless a host block arms `FX_TAG`.
    let fx_bytes: std::borrow::Cow<'static, [u8]> = match &fx_lib {
        Some(p) => std::borrow::Cow::Owned(std::fs::read(p).unwrap_or_else(|e| panic!("--fx-lib {}: {e}", p.display()))),
        None => std::borrow::Cow::Borrowed(include_bytes!("../../wie_android/src/demon_fx.bin")),
    };
    match wie_backend::canvas::set_fx_library(&fx_bytes) {
        Ok((s, e)) => eprintln!("[probe] fx library{}: {s} sprites · {e} effects", fx_lib.as_ref().map(|p| format!(" {}", p.display())).unwrap_or_default()),
        Err(e) => eprintln!("[probe] fx library refused: {e}"),
    }
    match fp.anchors {
        None => eprintln!(
            "[probe] loaded ({}) — no anchors, so char_anchor / map chain are not read at all",
            if is_ktf { "KTF carrier" } else { "LGT carrier" }
        ),
        Some(b) => eprintln!(
            "[probe] loaded ({}) char_anchor={} container_data_off={:?}",
            b.name(),
            match b.char_anchor() {
                Some(CharAnchor::Pointer { global, fallback }) => format!("*{global:#x} (fallback {fallback:#x})"),
                Some(CharAnchor::Static(b)) => format!("static {b:#x}"),
                Some(CharAnchor::JavaObject { slot }) => format!("java **{slot:#x} + 4"),
                None => "none".to_string(),
            },
            chain().data_off,
        ),
    }

    // ★ THE HOST PHASE SAMPLER (#9). It BUSY-WAITS between samples on purpose: this is
    // Windows, where `thread::sleep(1ms)` is quantised to the system timer (15.6 ms
    // unless something has raised it), which would give ~60 samples a second instead of
    // 1,000 — too few to split an 8 ms frame. A spinning sampler costs one core of a
    // machine that has eight, and it perturbs the measured thread only through the memory
    // system. On Android the app can sleep properly; this is the measurement tool.
    if phases {
        std::thread::spawn(|| {
            let mut next = std::time::Instant::now();
            loop {
                while std::time::Instant::now() < next {
                    std::hint::spin_loop();
                }
                next += std::time::Duration::from_micros(1000);
                wie_util::stats::phase_sample();
            }
        });
        eprintln!("[phases] host phase sampler on (1 kHz)");
    }

    // --- tick loop (deterministic: 1 tick per frame, no wall-clock budget) ---
    let wall_start = std::time::Instant::now();
    let stack_reserve_start = stack_gauge::reserved_bytes().unwrap_or(0);
    let mut stack_reserve_min = stack_reserve_start;
    eprintln!("[stack] initial reserve {} KB", stack_reserve_start / 1024);
    let mut released: Vec<(u64, KeyCode)> = vec![];
    let is_dir = |k: KeyCode| matches!(k, KeyCode::UP | KeyCode::DOWN | KeyCode::LEFT | KeyCode::RIGHT);
    // `--h2-walk-guard` / `--h2-relatch` state, mirroring `emulator::WalkGuard`: the
    // direction release last sent (frame, key) while the character may still be walking,
    // and the frame an action's release armed a walk re-press.
    let mut walk_owed: Option<(u64, KeyCode)> = None;
    let mut dir_pressed_at: Option<u64> = None;
    let mut walk_relatch: Option<u64> = None;
    // `--h2-use`: the drive in flight.
    let mut use_drive: Option<guest_build::H2UseDrive> = None;
    // `--h2-give`: the queue of (type, id, n) still to give, and the call in flight.
    let mut give_queue: std::collections::VecDeque<(i8, i8, i8)> = std::collections::VecDeque::new();
    let mut give_step: Option<GiveStep> = None;
    // ★ 2026-10-09: `--h2-wh-give` — the queue still to give into the warehouse file's bag, and the step in flight.
    let mut wh_queue: std::collections::VecDeque<(i8, i8, i8)> = std::collections::VecDeque::new();
    let mut wh_step: Option<WhStep> = None;
    // `--h2-move`: the two-call move in flight.
    let mut move_step: Option<MoveStep> = None;
    // `--h2-fold`: the bag being folded and the rounds so far.
    let mut fold_on: Option<(u8, u32)> = None;
    // `--h2-pages`: the host block once the patch is installed, and the page shown.
    let mut page_block: Option<u32> = None;
    let mut page_now: u8 = 0;
    // ★ 2026-10-09: whose window page_now belongs to (the inventory's or the 재료가방's).
    let mut page_bag: Option<u32> = None;
    // ★ 2026-10-09: the item picker being paged — (window, list, the list as the game built it, page, what the array
    //   holds now) — and the alchemist opener's calls in flight (one at a time), the menu handle to release.
    let mut picker_state: Option<(u32, u32, Vec<u8>, u8, Vec<u8>)> = None;
    let mut picker_seen = String::new();
    let mut alch_step: Option<(Arc<GuestCallResult>, u8, u64)> = None;
    let mut alch_aw: Option<u32> = None;
    // `--h2-exp`: the host block once the redirect is installed; `--h2-exp-gain`: the call
    // in flight.
    let mut exp_block: Option<u32> = None;
    // `--h2-drop-rate`: the filter's (block, data words) once installed.
    let mut drop_block: Option<(u32, u32)> = None;
    let mut exp_gain_step: Option<ExpGainStep> = None;
    // 세이브 슬롯 5: the install in flight, and the slot screen last printed.
    let mut slots_install = SlotsInstall::default();
    let mut slots_seen: Option<(i32, i32, i32, Option<u32>)> = None;
    let mut shell_seen: Option<bool> = None;
    // #17: the skill-power block once installed (the app's plan, from f=30 unless shipped).
    let mut skill_block: Option<u32> = None;
    let mut skill_power_tried = false;
    // 2026-09-27: the COMBAT block (weapons · front sweep · three-way 검기 · 민첩태세 guards).
    let mut combat_block: Option<u32> = None;
    let mut combat_tried = false;
    // 2026-09-29: the 맹독감염 block (the buff cast · 기본공격 poison · the attack-share tick).
    let mut venom_block: Option<u32> = None;
    let mut venom_tried = false;
    // 2026-09-30: the 야수진화제 sell-price block; `--h2-shop`'s calls in flight (one at a time:
    // `v.a()Lv;` → `v.a()V` → `ag.h(7)`), the shop handle to release, and the gold last printed.
    let mut refine_probe = RefineProbe::default();
    // ★★ 2026-10-16: the app's 정렬 / 일괄판매 driver, and the buttons it last wanted on screen (`--h2-tools`).
    let mut bag_tools = BagTools::default();
    let mut tools_patches: Vec<guest_build::H2Patch> = Vec::new();
    let mut tools_seen: (u32, Option<(u8, usize, i64)>) = (0, None);
    let mut shop_sell_block: Option<u32> = None;
    let mut shop_sell_tried = false;
    let mut boss_crit_tried = false;
    let mut boss_pierce_tried = false;
    let mut grade_line_tried = false;
    // 2026-10-02: the HELL bonus drop's install, once; its block and the gate word last written
    // (★ 저녁: the new six accessories' slot rolls only with the gate open — 난이도 NORMAL+ since 2026-10-03,
    //  HARD only before; `--h2-difficulty` drives it).
    let mut hell_drop_tried = false;
    let mut hell_drop_block: Option<u32> = None;
    let mut hell_drop_gate: Option<bool> = None;
    // ★ 2026-10-08 가디언: the mode tables' install (every frame, before the save slots'), the template block (once,
    //   f ≥ 30), and the last `[h2-guardian]` line printed (the tables' state · the mode word), to print changes only.
    let mut guardian_tables = GuardianTablesInstall { colour_off: !h2_colour_init, ..Default::default() };
    let mut guardian_tried = false;
    let mut guardian_block: Option<u32> = None;
    // ★ 2026-10-08 밤 가디언 장비의 개성 옵션: the block, the OPTS last written, whether a `--h2-gear-opts` forced it.
    let mut gear_tried = false;
    let mut gear_block: Option<u32> = None;
    let mut gear_seen: Vec<u8> = guest_build::GearPatch::effects_none().to_vec();
    let mut gear_forced = false;
    let mut gear_procs: (Option<u32>, Option<u32>, Option<u32>) = (None, Some(0), Some(0));
    // ★★ 2026-10-16 악의 무기: the host's live side (the app's `emulator::Gear` runs the same `H2GearLiveState`) and the items a
    //    `--h2-gear-wear` forced (the live poll keeps their words current — M1's bank, M2 · M3's moments).
    let mut gear_live = guest_build::H2GearLiveState::default();
    let mut gear_forced_items: Option<Vec<(u8, u8)>> = None;
    let mut gear_live_seen = guest_build::GearLive::default();
    let mut gear_beat_seen: (Option<u32>, Option<u32>) = (Some(0), Some(0));
    let mut guardian_seen: Option<(String, u32)> = None;
    // ★ 2026-10-02: the accessory block — (block, chains) once installed, the last data words seen,
    //   the last stage handle (a map change ends a channel, as the app does).
    let mut acc_tried = false;
    let mut acc_block: Option<(u32, u32)> = None;
    let mut ilpung_block: Option<u32> = None;
    let mut ilpung_tried = false;
    let mut ilpung_words = [0i32; 6];
    let mut berserker_block: Option<u32> = None;
    let mut berserker_tried = false;
    let mut berserker_seen = String::new();
    // ★ 2026-10-09 the awakenings' voices: the app's own edge watch (`hero2java::H2VoiceWatch`), every frame.
    let mut voice_watch = guest_build::H2VoiceWatch::default();
    // ★★ 2026-10-24 the thunder (어둠심판 · 벽력일섬) — the app's own watch, every frame
    let mut thunder_watch = guest_build::H2ThunderWatch::default();
    let h2_mode = matches!(fp.anchors, Some(GuestBuild::Hero2Ktf));
    let mut acc_seen: Vec<u32> = vec![];
    // ★★ 2026-10-14 번개의 호흡: the lightning's words as last printed (`[h2-bolt]`)
    let mut acc_bolt_seen: Vec<u32> = vec![];
    let mut acc_stage: u32 = 0;
    let mut acc_beast_seen = String::new();
    // the app's skill-name swap, mirrored: (table, code, [(slot, original)])
    let mut acc_names: Option<(u32, u32, Vec<(u32, u32)>)> = None;
    let mut shop_step: Option<(Arc<GuestCallResult>, u8, u64)> = None;
    let mut shop_v: Option<u32> = None;
    let mut shop_gold: Option<i32> = None;
    // ★ 2026-10-01 의상 아바타 A4: the round-clear counter, the count paid up to, the notices
    // waiting for the field.
    // ★ 2026-10-02: the cooldown tick counter and the app's plate window (its parked cooldowns).
    let mut cd_clock_block: Option<u32> = None;
    let mut slot_focus_block: Option<u32> = None;
    let mut slot_focus_tried = false;
    let mut slot_screen_seen: Option<u32> = None;
    let mut cd_clock_tried = false;
    let mut item_icon_tried = false;
    let mut buff_refresh_tried = false;
    let mut rogue_tried = false;
    let mut sky_draw_tried = false;
    // ★ 2026-10-06 살상연희: the block, for `[h2-slaughter]` (--gauges).
    let mut rogue_block: Option<u32> = None;
    // ★★ 2026-10-14 신규 아이템 2차 D: the 살상연희 gear words as last written, and whether `--h2-gear-wear` forced them.
    let mut rogue_gear_seen = [0u32; 6];
    let mut rogue_gear_forced = false;
    // ★ 2026-10-11: the teleport gate's block and the MASK last written.
    let mut tgate_tried = false;
    let mut demon_tried = false;
    let mut demon_block: Option<u32> = None;
    let mut demon_seen = String::new();
    let mut tgate: Option<(u32, u32)> = None;
    let mut cd_window = quickpage::Window::new();
    let mut cd_flip_off: Vec<Option<u8>> = vec![None; h2_cd_flip.len()];
    let mut clear_block: Option<u32> = None;
    let mut clear_tried = false;
    let mut clear_seen: u32 = 0;
    // ★★ 2026-10-22 가디언 레벨: the app's `GuardianLevel` — the block, the arm owed since the world entry, the GL told.
    let mut gl_block: Option<u32> = None;
    let mut gl_tried = false;
    let mut gl_want_arm = false;
    let mut gl_told: Option<u16> = None;
    let mut gl_saves_seen: u32 = 0;
    let mut clear_notices: Vec<u32> = Vec::new();
    // 2026-09-30: the 의상 아바타 override (the words, the reloads one call at a time).
    let mut avatar_block: Option<u32> = None;
    let mut avatar_tried = false;
    let mut avatar_queue: Vec<guest_build::H2PartOp> = Vec::new();
    let mut avatar_step: Option<(Arc<GuestCallResult>, guest_build::H2PartOp, u64)> = None;
    // ★ 2026-10-01: the EQUIPMENT window's character follows the equipment (the app's `Avatar::look`).
    let mut equip_look = guest_build::H2EquipLook::default();
    // ★ A3b: parts whose words changed before the world (the app's `Avatar::pre_world`).
    let mut avatar_pre_world: Vec<u8> = Vec::new();
    // 2026-09-30: 의상 아바타 A3 — the EQUIPMENT avatar side (`--h2-equip-ui`).
    let mut equip_ui_block: Option<u32> = None;
    let mut equip_ui_tried = false;
    let mut equip_model = guest_build::H2EquipAvatar::default();
    let mut equip_hidden = false;
    // 2026-09-30: 의상 아바타 A2 — the costume book over the in-memory filesystem.
    let costume_io: Option<ProbeAvatarIo> = costume_dir.as_ref().map(|dir| {
        COSTUME_DIR.set(dir.clone()).ok();
        ProbeAvatarIo { files: FS_LIVE.get().cloned().expect("the in-memory filesystem exists"), aid: costume_aid.clone() }
    });
    // ★ A3c: `--h2-costume-marker` — the app's adb grant marker (`vault::grant_costumes_on_request`),
    // before the game runs: every item the asset makes into the named slots' records.
    if let Some(slots) = costume_marker.as_ref() {
        match costume_io.as_ref().and_then(|io| avatar_state::AvatarIo::files(io).map(|f| (io, f))) {
            Some((io, files)) => {
                // ★ A4: per slot, the items for the class its save holds.
                let items_for = |class: Option<u8>| guest_build::h2_av_catalog_items(&costume_catalog, &avatar_sheets, class);
                let any = items_for(None);
                eprintln!("[h2-costume] marker {slots:?}: the asset makes {} item(s) {any:?} (each slot gets its class's)", any.len());
                // The app's run itself (`avatar_state::grant_marker_run`) — the probe only stands
                // in for the marker file and the vault's `read_live_g`.
                let live_g = |slot: u8| avatar_state::AvatarIo::live_g(io, slot);
                for line in avatar_state::grant_marker_run(&files, slots.as_bytes(), guest_build::SaveSlotsPatch::SLOTS, &items_for, &live_g, 0x5eed_0a3c) {
                    eprintln!("[h2-costume] marker: {line}");
                }
            }
            None => eprintln!("[h2-costume] --h2-costume-marker needs --h2-costume-dir (where the records live) — not run"),
        }
    }
    let mut costume_book = costume_io.as_ref().map(|io| avatar_state::AvatarBook::new(io));
    // ★ 2026-10-06 「심안」: the one-time 독 연마 refund's receipt — the book's (`--h2-costume-dir`), else this flag
    //   (a run without records = a character whose record holds no receipt yet).
    let mut probe_sight_paid = false;
    let mut probe_bodyslam_paid = false;
    let mut costume_last_shell = false;
    // `quickpage::NewGameWatch`, as the app samples it: (class pick seen, bare title frames).
    let mut costume_ng = (false, 0u8);
    let mut costume_applied: Option<[guest_build::AvatarWord; guest_build::AvatarPatch::N]> = None;
    // ★★ 2026-10-16 THE SKIN: the app's `Demon::poll_skin` — the beast table / icon array exchanged into the 악의 versions
    //   while STATE 2 (the container and the handle placed in its first exchanged slot)
    let mut skin_state: [Option<(u32, u32)>; 2] = [None, None];
    // ★★ 2026-10-17 보스 레이드: the app's raid driver (`emulator::BossRaid`) — the model, the game's font for the notices, and
    //   --h2-raid-autokill's death in flight `(monster, its result, the map it died on)` with the next frame it may strike.
    let mut raid_model = guest_build::H2BossRaid::default();
    let mut raid_pause_written: Option<bool> = None; // ★★ 2026-10-17 the demon block's PAUSE as last written
    let mut raid_font: Option<guest_build::H2Font> = None;
    let mut raid_kill: Option<(u32, Arc<GuestCallResult>, Option<u32>)> = None;
    let mut raid_kill_at = 0u64;
    let mut raid_seen: Option<(Option<u32>, bool, bool)> = None;
    // --h2-raid-notice-check: the texts the game renders through its own n.a([BII)V, compared with h2_notice_picture.
    let mut raid_check_idx = 0usize;
    let mut raid_check_step: Option<(Arc<GuestCallResult>, Option<u64>)> = None;
    let mut raid_gold_check: Option<(u64, Option<i32>)> = None;
    // #20/#21: the kill census.
    let mut kill_step: Option<KillStep> = None;
    let mut kill_tally = KillTally::default();
    let mut kill_done = false;
    let mut kill_level_before: Option<u32> = None;
    // #17: the skill readout.
    let mut skill_probe: Option<SkillProbe> = None;
    // `--h2-account`: the warehouse bag (pinned) and the step in flight.
    let mut account_bag: Option<u32> = None;
    let mut rt_step: Option<RtStep> = None;
    let mut mr_step: Option<MrStep> = None;
    // ★★ 2026-10-20: the craft guard's box in flight (`--h2-craft-guard`), the app's `AccountState::notice` shape.
    let mut craft_step: Option<MrStep> = None;
    let mut account_step: Option<AccountStep> = None;
    let mut account_title: Option<(u32, u32)> = None;
    // One host save in flight at most, same as the app.
    let mut hs_job: Option<(host_save::Job, Arc<wie_backend::GuestCallResult>)> = None;
    // --host-levelup watch: (level, stat points, skill points) before the call.
    let mut lu_watch: Option<(u16, u16, u8)> = None;
    for f in 0..=frames {
        clock.fetch_add(ms_per_frame, Ordering::SeqCst);
        for (pf, addr, bytes) in &pokes {
            if *pf != f {
                continue;
            }
            let before = emu.read_memory(*addr, bytes.len());
            let ok = emu.write_memory(*addr, bytes);
            let hex = |b: &[u8]| b.iter().map(|x| format!("{x:02x}")).collect::<String>();
            eprintln!("[poke] f={f} {addr:#x}: {} -> {} ({})", hex(&before), hex(bytes), if ok { "ok" } else { "REFUSED" });
        }
        // `--h2-sfx` (배경음 A): the game's own effect call, as a hit or a menu makes it.
        if build().hero2_java().is_some() {
            for (_, n) in h2_sfx.iter().filter(|(sf, _)| *sf == f) {
                eprintln!("[h2-sfx] f={f} t={} ms br.a(B)V({n})", clock.load(Ordering::SeqCst));
                emu.call_java_method(JavaReceiver::Class("br"), "a", "(B)V", &[JavaArg::Int(*n)], Arc::new(GuestCallResult::new()));
            }
        }
        if let Some(h2) = build().hero2_java() {
            let hex = |b: &[u8]| b.iter().map(|x| format!("{x:02x}")).collect::<String>();
            // ── 가디언 (2026-10-08): the mode tables — every frame from the first, BEFORE the save-slot install
            //    asks for `ag`'s initialisation (the app's order; `ag.<clinit>` sizes the arrays) ──
            if !h2_shipped
                && h2_guardian
                && let Some(p) = GuestBuild::Hero2Ktf.guardian_patch()
            {
                guardian_tables.poll(&mut *emu, &p, f);
            }
            // ── 세이브 슬롯 5 (2026-09-27): the app's own install order, every frame until done ──
            if h2_slots == 5
                && let Some(p) = GuestBuild::Hero2Ktf.save_slots_patch()
            {
                if f >= h2_slots_from {
                    slots_install.poll(&mut *emu, &p, &h2, f);
                }
                let rd = |a: u32, n: usize| emu.read_memory(a, n);
                let screen = guest_build::h2_slot_screen(&rd, &h2);
                let now = screen.map(|s| (s.mode, s.cursor, s.rows, guest_build::h2_save_previews(&rd, &p).flatten()));
                if now != slots_seen {
                    if let Some((mode, cursor, rows, previews)) = now {
                        let count = rd(0x0019_3a8c, 4);
                        eprintln!("[h2-slots] f={f} slot screen mode={mode} cursor={cursor} (SLOT {}) rows={rows} previews={previews:?} saves={:?}", cursor + 1, count.first());
                    } else {
                        eprintln!("[h2-slots] f={f} slot screen gone (shell up = {})", guest_build::h2_shell_up(&rd));
                    }
                    slots_seen = now;
                }
                let shell_now = guest_build::h2_shell_up(&rd);
                if shell_seen.is_some_and(|s| s != shell_now) {
                    eprintln!("[h2-slots] f={f} shell up {shell_now}");
                }
                shell_seen = Some(shell_now);
            }
            // From frame f on, EVERY frame — the app re-applies the rewrite each frame
            // (`emulator.rs` 4b), and a one-shot here hid the commit race of 2026-09-24:
            // the rewrite that runs between a commit and the game's handling of its OK.
            // Only frames that write something are logged.
            if h2_relabel.is_some_and(|rf| f >= rf) {
                let rd = |a: u32, n: usize| emu.read_memory(a, n);
                let first = h2_relabel == Some(f);
                match guest_build::h2_popup_walk(&rd, &h2) {
                    Some(p) => {
                        let writes = guest_build::h2_popup_relabel_writes(&rd, &p);
                        if first || !writes.is_empty() {
                            eprintln!("[h2-relabel] f={f} popup={:#x} sel={} shape={:?} value={:?} -> {} write(s)", p.handle, p.sel, p.shape(), p.value(), writes.len());
                        }
                        for (a, bytes) in &writes {
                            let ok = emu.write_memory(*a, bytes);
                            eprintln!("[h2-relabel]   {a:#x} <- {} ({})", hex(bytes), if ok { "ok" } else { "REFUSED" });
                        }
                    }
                    None => match guest_build::h2_item_popup_walk(&rd, &h2, (0, 0)) {
                        Some(p) => {
                            let cycle = guest_build::h2_heal_cycle(&rd, &h2).unwrap_or_default();
                            let writes = guest_build::h2_item_popup_relabel_writes(&rd, &p);
                            if first || !writes.is_empty() {
                                eprintln!(
                                    "[h2-relabel] f={f} item popup={:#x} record [{}] type={} id={} x{} shape={:?} bindable={} -> {} write(s)",
                                    p.handle, p.item_index, p.item_kind, p.item_id, p.item_count, p.shape(), p.bindable(&cycle), writes.len()
                                );
                            }
                            for (a, bytes) in &writes {
                                let ok = emu.write_memory(*a, bytes);
                                eprintln!("[h2-relabel]   {a:#x} <- {} ({})", hex(bytes), if ok { "ok" } else { "REFUSED" });
                            }
                        }
                        None if first => eprintln!("[h2-relabel] f={f} no register popup on screen"),
                        None => {}
                    },
                }
            }
            // ── --h2-iaido-unlock: the app's plan for 발도's slot (a knight whose 방어의지 is open) ──
            // ── --h2-ilpung-unlock: the app's plan for 일풍격's slot (a rogue whose 번개난타 is open) ──
            if h2_ilpung_unlock == Some(f) {
                let plan = {
                    let rd = |a: u32, n: usize| emu.read_memory(a, n);
                    guest_build::h2_ilpung_unlock(&rd, &h2)
                };
                match plan {
                    Some((at, v)) => {
                        let ok = emu.write_memory(at, &[v]);
                        eprintln!("[h2-ilpung-unlock] f={f} a.p[15] {at:#x} <- {v} ({})", if ok { "ok" } else { "REFUSED" });
                    }
                    None => eprintln!("[h2-ilpung-unlock] f={f} nothing to open (not a rogue, 번개난타 locked, 15 slots, or already open)"),
                }
            }
            // ── --h2-awaken-unlock: every slot the app opens (the sixteenth · the awakening at LV 70) ──
            if h2_awaken_unlock.contains(&f) {
                let plan = {
                    let rd = |a: u32, n: usize| emu.read_memory(a, n);
                    guest_build::h2_extra_slot_unlocks(&rd, &h2)
                };
                if plan.is_empty() {
                    eprintln!("[h2-awaken-unlock] f={f} nothing to open (a short a.p, the conditions unmet, or already open)");
                }
                for (at, v) in plan {
                    let ok = emu.write_memory(at, &[v]);
                    eprintln!("[h2-awaken-unlock] f={f} a.p byte {at:#x} <- {v} ({})", if ok { "ok" } else { "REFUSED" });
                }
            }
            // ── --h2-bodyslam: the app's 몸통치기 retire (the receipt as the app keeps it) ──
            if h2_bodyslam.contains(&f) {
                let retired = match costume_book.as_ref() {
                    Some(b) => b.bodyslam_refunded(),
                    None => Some(probe_bodyslam_paid),
                };
                let plan = {
                    let rd = |a: u32, n: usize| emu.read_memory(a, n);
                    guest_build::h2_bodyslam_retire(&rd, &h2, retired)
                };
                match plan {
                    Some((plan, receipt)) => {
                        let ok = plan.writes.iter().all(|(a, b)| emu.write_memory(*a, b));
                        if ok && receipt {
                            match (costume_book.as_mut(), costume_io.as_ref()) {
                                (Some(b), Some(io)) => {
                                    b.mark_bodyslam_refunded(io, plan.refund);
                                }
                                _ => probe_bodyslam_paid = true,
                            }
                        }
                        eprintln!(
                            "[h2-bodyslam] f={f} {} write(s) {:x?} refund {}{} ({})",
                            plan.writes.len(),
                            plan.writes,
                            plan.refund,
                            if receipt { " — the one-time refund, receipt recorded" } else { "" },
                            if ok { "ok" } else { "REFUSED" }
                        );
                    }
                    None => eprintln!("[h2-bodyslam] f={f} nothing to retire (not a berserker, a.p[5] locked already, or no record yet: {retired:?})"),
                }
            }
            // ── --h2-skill-set: a.p[i] = lv (diagnostic) ──
            for &(_, i, lv) in h2_skill_set.iter().filter(|(at, ..)| *at == f) {
                let at = {
                    let rd = |a: u32, n: usize| emu.read_memory(a, n);
                    guest_build::h2_static_handle(&rd, h2.char_slot)
                        .and_then(|ch| guest_build::h2_fields(&rd, ch))
                        .and_then(|fb| guest_build::h2_rd_u32(&rd, fb + 4 + h2.char_skill_levels_off))
                        .and_then(|arr| guest_build::h2_array(&rd, arr))
                };
                match at {
                    Some((base, len)) if i < len => {
                        let ok = emu.write_memory(base + i, &[lv as u8]);
                        eprintln!("[h2-skill-set] f={f} a.p[{i}] <- {lv} ({}, a.p is {len} long)", if ok { "ok" } else { "REFUSED" });
                    }
                    Some((_, len)) => eprintln!("[h2-skill-set] f={f} a.p[{i}]: a.p is only {len} long"),
                    None => eprintln!("[h2-skill-set] f={f} no character / a.p"),
                }
            }
            if h2_born_repair.contains(&f) {
                // ★ 2026-10-06: as the app — the receipt from the character's record (the book), the refund once.
                let refunded = match costume_book.as_ref() {
                    Some(b) => b.sight_refunded(),
                    None => Some(probe_sight_paid),
                };
                let plan = {
                    let rd = |a: u32, n: usize| emu.read_memory(a, n);
                    guest_build::h2_born_skill_repair(&rd, &h2, refunded)
                };
                match plan {
                    Some((plan, receipt)) => {
                        let ok = plan.writes.iter().all(|(a, b)| emu.write_memory(*a, b));
                        if ok && receipt {
                            match (costume_book.as_mut(), costume_io.as_ref()) {
                                (Some(b), Some(io)) => {
                                    b.mark_sight_refunded(io, plan.refund);
                                }
                                _ => probe_sight_paid = true,
                            }
                        }
                        let refunded = match costume_book.as_ref() {
                            Some(b) => b.sight_refunded(),
                            None => Some(probe_sight_paid),
                        };
                        let again = {
                            let rd = |a: u32, n: usize| emu.read_memory(a, n);
                            guest_build::h2_born_skill_repair(&rd, &h2, refunded)
                        };
                        eprintln!(
                            "[h2-born-repair] f={f} {} write(s) {:x?} refund {}{} ({}) · replan {again:?}",
                            plan.writes.len(),
                            plan.writes,
                            plan.refund,
                            if receipt { " — the one-time 독 연마 refund, receipt recorded" } else { "" },
                            if ok { "ok" } else { "REFUSED" }
                        );
                    }
                    None => eprintln!("[h2-born-repair] f={f} nothing to repair (held at 1, a class/slot without a born skill, or a rogue with no record yet: {refunded:?})"),
                }
            }
            if h2_iaido_unlock == Some(f) {
                let plan = {
                    let rd = |a: u32, n: usize| emu.read_memory(a, n);
                    guest_build::h2_iaido_unlock(&rd, &h2)
                };
                match plan {
                    Some((at, v)) => {
                        let ok = emu.write_memory(at, &[v]);
                        let again = {
                            let rd = |a: u32, n: usize| emu.read_memory(a, n);
                            guest_build::h2_iaido_unlock(&rd, &h2)
                        };
                        eprintln!("[h2-iaido-unlock] f={f} a.p[9] {at:#x} <- {v} ({}) · replan {again:?}", if ok { "ok" } else { "REFUSED" });
                    }
                    None => eprintln!("[h2-iaido-unlock] f={f} nothing to open (not a knight, 방어의지 locked, or the slot is already open)"),
                }
            }
            if let Some((cf, n)) = h2_cap
                && cf == f
            {
                let rd = |a: u32, n: usize| emu.read_memory(a, n);
                match guest_build::h2_inventory(&rd, &h2) {
                    Some(inv) => {
                        let addr = inv.fields + 4 + guest_build::Hero2Java::INV_B_OFF;
                        let ok = emu.write_memory(addr, &[n as u8]);
                        eprintln!("[h2-cap] f={f} bag={:#x} b {} -> {n} at {addr:#x} ({}) items={}", inv.handle, inv.b, if ok { "ok" } else { "REFUSED" }, inv.items.len());
                    }
                    None => eprintln!("[h2-cap] f={f} no inventory yet"),
                }
            }
            // ── --h2-shift: move the `q` grid right, through the APP's own applier ──
            //    (`items::inventory_grid_shift`), so what this dumps is what the app draws.
            if h2_shift && !h2_shift_done && f >= 30 {
                let (todo, already, total) = {
                    let rd = |a: u32, n: usize| emu.read_memory(a, n);
                    guest_build::h2_halfword_writes(&rd, guest_build::GuestBuild::Hero2Ktf.inventory_grid_shift())
                };
                if total > 0 {
                    let mut landed = already;
                    for (a, bytes) in &todo {
                        if emu.write_memory(*a, bytes) {
                            landed += 1;
                        }
                    }
                    h2_shift_done = landed == total;
                    eprintln!("[h2-shift] f={f} {landed}/{total} site(s) shifted by {} px", guest_build::Hero2Java::INV_GRID_SHIFT);
                }
            }
            // ── --h2-equip-layout (의상 아바타 A3): the EQUIPMENT cells moved left ──
            if h2_equip_layout && !h2_equip_layout_done && f >= 30 {
                let (todo, already, total) = {
                    let rd = |a: u32, n: usize| emu.read_memory(a, n);
                    guest_build::h2_halfword_writes(&rd, guest_build::GuestBuild::Hero2Ktf.equip_cell_shift())
                };
                if total > 0 {
                    let mut landed = already;
                    for (a, bytes) in &todo {
                        if emu.write_memory(*a, bytes) {
                            landed += 1;
                        }
                    }
                    h2_equip_layout_done = landed == total;
                    eprintln!("[h2-equip-layout] f={f} {landed}/{total} site(s) — the cells moved {} px left", guest_build::Hero2Java::EQUIP_CELL_SHIFT);
                }
            }
            // ── --h2-equip-ui (의상 아바타 A3): the app's install — the cells and the cursor switch,
            //    all or nothing, the def word last — then the switch following the model every frame ──
            if h2_equip_ui && !equip_ui_tried && f >= 30 {
                equip_ui_tried = true;
                let sites = guest_build::GuestBuild::Hero2Ktf.equip_cell_shift();
                let patch = guest_build::GuestBuild::Hero2Ktf.equip_cursor_patch().expect("hero2 has the cursor switch");
                match emu.alloc_guest(guest_build::EquipCursorPatch::BLOCK_LEN) {
                    Some(block) => {
                        let (hook, (shift, already, total)) = {
                            let rd = |a: u32, n: usize| emu.read_memory(a, n);
                            (guest_build::h2_equip_cursor_install_writes(&rd, &patch, block), guest_build::h2_halfword_writes(&rd, sites))
                        };
                        match hook.filter(|_| shift.len() + already == total) {
                            Some(hook) => {
                                let mut ok = emu.write_memory(hook[0].0, &hook[0].1);
                                for (a, b) in &shift {
                                    ok &= emu.write_memory(*a, b);
                                }
                                ok &= emu.write_memory(hook[1].0, &hook[1].1);
                                let verified = ok && {
                                    let rd = |a: u32, n: usize| emu.read_memory(a, n);
                                    let (todo, done, n) = guest_build::h2_halfword_writes(&rd, sites);
                                    guest_build::h2_equip_cursor_installed(&rd, &patch, block) && todo.is_empty() && done == n
                                };
                                eprintln!("[h2-equip-ui] f={f} cells {}/{total} moved, cursor switch at {block:#x} — verified={verified}", shift.len() + already);
                                if verified {
                                    equip_ui_block = Some(block);
                                }
                            }
                            None => eprintln!("[h2-equip-ui] f={f} REFUSED — a cell site or cc.b(II)V's def does not read as shipped"),
                        }
                    }
                    None => eprintln!("[h2-equip-ui] f={f} no guest block"),
                }
            }
            if let (Some(block), Some(h2)) = (equip_ui_block, build().hero2_java()) {
                let view = {
                    let rd = |a: u32, n: usize| emu.read_memory(a, n);
                    guest_build::h2_equip_view(&rd, &h2).filter(|v| v.shifted)
                };
                let before = equip_model.handle;
                equip_model.sync(view.as_ref().map(|v| v.win.handle));
                if equip_model.handle != before {
                    eprintln!("[h2-equip-ui] f={f} EQUIPMENT window {}", if equip_model.handle != 0 { format!("up ({:#x})", equip_model.handle) } else { "gone".to_string() });
                }
                probe_equip_hide(&mut *emu, block, &equip_model, &mut equip_hidden, view.as_ref(), f);
            }
            // ── #18: the SKILL-POINT curve, through the APP's own applier ──────────
            //    Always on (the app applies it unconditionally too), and the point here
            //    is the VERIFY: `h2_halfword_writes` answers `(empty, 0, total)` unless
            //    every one of the five halfwords still reads exactly as the measured
            //    image ships them, so a probe run is a standing check that the site did
            //    not move. It cannot check the BEHAVIOUR — that needs a level-up, which
            //    needs combat, which the probe cannot drive yet.
            if !h2_curve_done && f >= 30 {
                let mut all = true;
                // ★ 2026-09-24: #17's cooldown halving rides the same applier (#20's drop
                //   window was removed 2026-09-25 — it broke the level rule). `--h2-shipped 1`
                //   skips them all: the shipped rules, for an A/B.
                let lists: Vec<(&str, &[(u32, u16, u16)])> = if h2_shipped {
                    eprintln!("[h2-curve] f={f} --h2-shipped: the balance patches are NOT applied");
                    vec![]
                } else {
                    vec![
                        ("skill points", guest_build::GuestBuild::Hero2Ktf.skill_point_curve()),
                        ("combo SP", guest_build::GuestBuild::Hero2Ktf.combo_sp_gain()),
                        // 2026-09-26: 버서커 근력단련 no longer takes 7 SP a level off the
                        // basic hit (its new effect, +3 % skill damage a level, rides the
                        // skill-power block). `[h2-combo]`'s cost shows the A/B.
                        ("근력단련 SP", guest_build::GuestBuild::Hero2Ktf.strength_training_sp_off()),
                        // 2026-09-26: 반격태세's counter = the character's attack a.a()I × the
                        // level factor (100 % + 10 % a level), not the flat lv·14+20 —
                        // `--h2-skill` prints a(BILbd;)I(8) both ways — and its stance lasts
                        // 70·lv + 50 frames (6 s .. 20 s) instead of 120 — ★ 2026-10-06 250·lv + 250 (25 s .. 75 s).
                        ("반격태세", guest_build::GuestBuild::Hero2Ktf.counter_stance_attack()),
                        ("반격태세 지속", guest_build::GuestBuild::Hero2Ktf.counter_stance_duration()),
                        // ★ 2026-10-06: the buff cooldowns at one minute (분노태세 · 반격태세 · 빛의수호 · 빛의회복; the
                        // rest are their own patches'). `[quickbar]`'s cd and `[h2-beast-cost]` show them.
                        ("버프 쿨타임", guest_build::GuestBuild::Hero2Ktf.buff_cooldown_minute()),
                        ("skill cooldown", guest_build::GuestBuild::Hero2Ktf.skill_cooldown_half()),
                        // 2026-09-24: the 11 % spell-card roll off (the card window is
                        // ACCOUNT's since #13) and the title carousel without its
                        // 네트워크창고 stop — `--h2-shipped 1` keeps both as shipped,
                        // so a kill tally A/B still shows the original card rate.
                        ("card drop", guest_build::GuestBuild::Hero2Ktf.card_drop_off()),
                        // 2026-09-26: 사자후 scans the whole map — the shipped screen-sized
                        // square ran off every map smaller than a landscape screen (AIOOBE,
                        // worker dead); `--h2-shipped 1` keeps it, which is how the probe
                        // shows the crash is the game's own arithmetic.
                        ("roar whole map", guest_build::GuestBuild::Hero2Ktf.roar_whole_map()),
                        // 2026-09-27 전투 밸런스: rogue skill SP −40 % · 치명/회피 from 민첩 · 검기 5
                        // tiles · 신의가호 (length, cooldown, the old boost off) · 민첩태세 (the cast,
                        // the old bonus off, cooldown). `--h2-skill` prints the costs and stats.
                        ("rogue skill SP", guest_build::GuestBuild::Hero2Ktf.rogue_skill_sp_cut()),
                        ("agility", guest_build::GuestBuild::Hero2Ktf.agility_coefficients()),
                        ("검기 range", guest_build::GuestBuild::Hero2Ktf.sword_wave_range()),
                        // ★ 2026-10-20 밸런스 전수 조사 ②: the knight's attack from 힘 × 0.85 (bx.c()V's 7/10 → 17/20).
                        ("knight strength", guest_build::GuestBuild::Hero2Ktf.knight_strength_coefficient()),
                        // 2026-10-03 발도: the SKILL window shows the knight's seventh active (부활오라's
                        // slot, 발도 now) — the cast / SP / cooldown are the combat block's.
                        ("knight actives", guest_build::GuestBuild::Hero2Ktf.knight_active_seven()),
                        ("사자후 cooldown", guest_build::GuestBuild::Hero2Ktf.roar_cooldown_up()),
                        ("신의가호", guest_build::GuestBuild::Hero2Ktf.blessing_aura_rework()),
                        ("민첩태세", guest_build::GuestBuild::Hero2Ktf.agility_stance_invuln()),
                        // 2026-10-05: 방어의지 → 철괴 (length, cooldown, no break on an attack, the plain
                        // walk); the cut is the combat block's. `[h2-flags]` shows buff 35's length.
                        ("철괴", guest_build::GuestBuild::Hero2Ktf.iron_wall_rework()),
                        // 2026-10-05: the beast's 빛의회복 → the buff list (bn.c(4), like 얼음장막) — its regen
                        // tick runs and `[h2-status]` / `[h2-flags]` show it. `--h2-shipped 1` keeps it an effect.
                        ("빛의회복", guest_build::GuestBuild::Hero2Ktf.beast_recovery_buff()),
                        // 2026-10-06: the beast's 지력 ×1.5 (rings · heavy) and attack-skill SP −20 %.
                        // `--h2-beast-tune 0` keeps both shipped for an A/B.
                        ("야수 지력", guest_build::GuestBuild::Hero2Ktf.beast_int_weight()),
                        ("야수 SP", guest_build::GuestBuild::Hero2Ktf.beast_attack_sp_cut()),
                        // ★ 2026-10-12: the beast's attack cooldown ×0.8 and every beast hit ×1.5 (also `--h2-beast-tune 0`),
                        // the seals' 상급 (id 3) 15..20 and 20,000.
                        ("야수 쿨타임", guest_build::GuestBuild::Hero2Ktf.beast_attack_cooldown_cut()),
                        ("야수 데미지", guest_build::GuestBuild::Hero2Ktf.beast_attack_power()),
                        ("인장 상급", guest_build::GuestBuild::Hero2Ktf.seal_top_tier()),
                        // 2026-09-25: weapons keep their full attack at any 명예 — the
                        // penalty only a network 대전 could lift. `--h2-honor` prints both.
                        ("honour penalty", guest_build::GuestBuild::Hero2Ktf.honor_penalty_off()),
                        // ★★ 2026-10-17: owned items load with the current table's name, price and 요구레벨
                        // (`--h2-shipped 1` keeps the saved copies — the A/B).
                        ("item table refresh", guest_build::GuestBuild::Hero2Ktf.item_table_refresh()),
                        ("title menu", guest_build::GuestBuild::Hero2Ktf.title_menu_no_network()),
                        // The shipped SKILL-window cancel crash — `--h2-shipped 1` leaves it
                        // in, which is how the probe shows the bug is the game's own.
                        ("skill popup cancel", guest_build::GuestBuild::Hero2Ktf.skill_popup_cancel()),
                        // ★★ 2026-10-16: the 개수 box wraps both ways (LEFT / DOWN on 1 → its ceiling: 99 buying, the stack
                        // selling · UP / RIGHT on the ceiling → 1).
                        ("qty box wrap", guest_build::GuestBuild::Hero2Ktf.qty_box_wrap()),
                        // ★ 2026-10-09 아이템 조합 on 127-cell bags: the material mark without the 재료가방 index's +50, and the
                        // picker's grid 7 px right for its page tabs (the worn-skip CELLS follow the list, below).
                        ("craft material mark", guest_build::GuestBuild::Hero2Ktf.craft_material_mark()),
                        ("picker grid", guest_build::GuestBuild::Hero2Ktf.picker_grid_shift()),
                        // ★ 2026-10-07 (2): 몸통치기 never opens from the level table (its slot is retired) and the monster
                        // HP delta keeps the word (the SHORT was the per-hit damage cap). `--h2-damage-cap 0` keeps the short.
                        ("몸통치기 lock", guest_build::GuestBuild::Hero2Ktf.bodyslam_locked()),
                        ("damage cap", guest_build::GuestBuild::Hero2Ktf.damage_cap_off()),
                        // ★★ 2026-10-16: the other five monster HP stores keep the word — HP is an int. `--h2-hp-int 0` keeps them.
                        ("monster HP int", guest_build::GuestBuild::Hero2Ktf.monster_hp_int()),
                    ]
                };
                for (name, sites) in lists {
                    if (name == "빛의회복" && !h2_beast_recovery) || ((name == "야수 지력" || name == "야수 SP" || name == "야수 쿨타임" || name == "야수 데미지") && !h2_beast_tune) || (name == "damage cap" && !h2_damage_cap) || (name == "monster HP int" && !h2_hp_int) {
                        continue;
                    }
                    let (todo, already, total) = {
                        let rd = |a: u32, n: usize| emu.read_memory(a, n);
                        guest_build::h2_halfword_writes(&rd, sites)
                    };
                    if total == 0 {
                        continue;
                    }
                    let mut landed = already;
                    for (a, bytes) in &todo {
                        if emu.write_memory(*a, bytes) {
                            landed += 1;
                        }
                    }
                    all &= landed == total;
                    eprintln!("[h2-curve] f={f} {name}: {landed}/{total} halfword(s){}", if landed == total { "" } else { "  ** NOT APPLIED **" });
                }
                // ★ 2026-10-09: the craft that never eats a worn piece — two CELL words (bq.a(BBB)V's finder → bq.b(BB)B).
                if !h2_shipped {
                    let (todo, already, total) = {
                        let rd = |a: u32, n: usize| emu.read_memory(a, n);
                        guest_build::h2_cell_redirect_writes(&rd, guest_build::GuestBuild::Hero2Ktf.craft_worn_skip())
                    };
                    let mut landed = already;
                    for (a, bytes) in &todo {
                        if emu.write_memory(*a, bytes) {
                            landed += 1;
                        }
                    }
                    all &= landed == total;
                    eprintln!("[h2-curve] f={f} craft worn skip: {landed}/{total} cell(s){}", if landed == total { "" } else { "  ** NOT APPLIED **" });
                }
                h2_curve_done = all;
            }
            // ── --h2-notice / --h2-hud: the app's painter plan (`items::hud_bottom_patch` —
            //    the target-monster panel kept, the rest off) + the notice feed ──
            //    ★ 2026-09-25: two halfwords now, TAIL first — the same ordered plan the app
            //    writes, stopping at the first failed write.
            if (h2_notice || h2_autoloot || h2_hud.is_some()) && !h2_notice_done && f >= 10 {
                let plan = {
                    let rd = |a: u32, n: usize| emu.read_memory(a, n);
                    GuestBuild::Hero2Ktf.hud_bottom_writes(&rd)
                };
                let mut ok = true;
                for (a, bytes) in &plan {
                    ok = emu.write_memory(*a, bytes);
                    eprintln!("[h2-hud] f={f} painter {a:#x} <- {:#06x} ok={ok}", u16::from_le_bytes(*bytes));
                    if !ok {
                        break;
                    }
                }
                // Done once the HEAD reads as patched — the plan only ever writes it behind
                // the tail, so that one halfword says both are in. (An empty plan is not
                // "done": it is also what an image that is not readable yet answers.)
                h2_notice_done = ok
                    && GuestBuild::Hero2Ktf.hud_bottom_patch().is_some_and(|(site, _, head)| {
                        let b = emu.read_memory(site, 2);
                        b.len() == 2 && u16::from_le_bytes([b[0], b[1]]) == head
                    });
            }
            // `--h2-hud <shift>`: the target box dodged `shift` px left, re-asserted every
            // frame like the app's poke pass (idempotent — prints only when it writes).
            if let Some(shift) = h2_hud
                && f >= 10
            {
                let writes = {
                    let rd = |a: u32, n: usize| emu.read_memory(a, n);
                    GuestBuild::Hero2Ktf.target_box_x_writes(&rd, shift)
                };
                for (a, bytes) in &writes {
                    let ok = emu.write_memory(*a, bytes);
                    eprintln!("[h2-hud] f={f} target box x {a:#x} <- {:#06x} (shift {shift}) ok={ok}", u16::from_le_bytes(*bytes));
                }
                if writes.is_empty() && !h2_hud_said && shift > GuestBuild::Hero2Ktf.target_box_shift_max() {
                    h2_hud_said = true;
                    eprintln!("[h2-hud] shift {shift} is past the ceiling {} — nothing written", GuestBuild::Hero2Ktf.target_box_shift_max());
                }
            }
            if h2_notice || h2_autoloot {
                let got = {
                    let rd = |a: u32, n: usize| emu.read_memory(a, n);
                    guest_build::h2_notice_pending(&rd)
                };
                if let Some((timer_at, n)) = got {
                    let name = out.join(format!("notice_{f:05}_{}x{}.rgba", n.w, n.h));
                    let bytes: Vec<u8> = n.argb.iter().flat_map(|p| {
                        let [b, g, r, a] = p.to_le_bytes();
                        [r, g, b, a]
                    }).collect();
                    let _ = std::fs::write(&name, bytes);
                    let ok = emu.write_memory(timer_at, &0u32.to_le_bytes());
                    eprintln!("[h2-notice] f={f} notice {}x{} taken -> {} (n.o cleared ok={ok})", n.w, n.h, name.display());
                }
            }
            // ── ★★ 2026-10-17 보스 레이드 아홉 단계: the app's driver, step for step (`emulator::BossRaid`) ──
            if h2_boss_raid
                && let (Some(h2), Some(sv), Some(AppState::Absolute { addr, field, .. })) =
                    (build().hero2_java(), build().hero2_save(), build().app_state())
            {
                let view = {
                    let rd = |a: u32, n: usize| emu.read_memory(a, n);
                    let map = guest_build::h2_live_map(&rd, &sv.area);
                    let raid = map.and_then(guest_build::GuardianPatch::raid_stage).is_some();
                    guest_build::H2RaidView {
                        in_world: guest_build::h2_demon_in_world(&rd, &h2),
                        map,
                        field: guest_build::h2_rd_u32(&rd, addr) == Some(field),
                        flag: raid && guest_build::h2_event_flags(&rd, &sv.area).is_some_and(|fl| guest_build::h2_flag_on(&fl, guest_build::H2_RAID_FLAG)),
                        difficulty: diff_level.unwrap_or(guest_build::DIFFICULTY_DEFAULT),
                        // ★★ (3) the app's rule: the fall played out to the ask frame and no 부활의 구 box is up or due (`Revive::waits`)
                        fallen: raid
                            && guest_build::h2_fall_frame(&rd, &h2, addr, field).is_some_and(|fr| fr >= guest_build::H2_FALL_ASK_FRAME)
                            && !(revive_prompt.is_some()
                                || revive_consume.is_some()
                                || (h2_revive && !revive_declined && guest_build::h2_revive_orb(&rd, &h2).is_some())),
                    }
                };
                let seen = (view.map, view.field, view.flag);
                let raid_now = view.map.and_then(guest_build::GuardianPatch::raid_stage).is_some();
                let raid_was = raid_seen.and_then(|s| s.0).and_then(guest_build::GuardianPatch::raid_stage).is_some();
                if raid_seen != Some(seen) && (raid_now || raid_was) {
                    let state = guest_build::h2_rd_u32(&|a: u32, n: usize| emu.read_memory(a, n), addr);
                    eprintln!("[h2-raid] f={f} map {:?} field {} (state {state:?}) flag {} world {} — tally {:?}", view.map, view.field, view.flag, view.in_world, raid_model.tally());
                }
                raid_seen = Some(seen);
                for action in raid_model.step(f * ms_per_frame, view) {
                    match action {
                        guest_build::H2RaidAction::Countdown(text) => {
                            if raid_font.is_none() {
                                let rd = |a: u32, n: usize| emu.read_memory(a, n);
                                raid_font = guest_build::h2_font_from_guest(&rd);
                            }
                            if let Some(font) = &raid_font {
                                let n = guest_build::h2_notice_picture(font, &text);
                                let name = out.join(format!("raidnotice_{f:05}_{}x{}.rgba", n.w, n.h));
                                let bytes: Vec<u8> = n.argb.iter().flat_map(|p| {
                                    let [b, g, r, a] = p.to_le_bytes();
                                    [r, g, b, a]
                                }).collect();
                                let _ = std::fs::write(&name, bytes);
                            }
                            eprintln!("[h2-raid] f={f} map {:?} countdown {} (font {})", view.map, text.last().map_or('?', |&c| c as char), raid_font.is_some());
                        }
                        guest_build::H2RaidAction::Notice(text) => {
                            if let Some(font) = &raid_font {
                                let n = guest_build::h2_notice_picture(font, &text);
                                let name = out.join(format!("raidnotice_{f:05}_lap_{}x{}.rgba", n.w, n.h));
                                let bytes: Vec<u8> = n.argb.iter().flat_map(|p| {
                                    let [b, g, r, a] = p.to_le_bytes();
                                    [r, g, b, a]
                                }).collect();
                                let _ = std::fs::write(&name, bytes);
                            }
                            eprintln!("[h2-raid] f={f} map {:?} lap {} begins — tally {:?}", view.map, raid_model.lap() + 1, raid_model.tally());
                        }
                        guest_build::H2RaidAction::Advance => {
                            let group = i32::from(guest_build::H2_RAID_ADVANCE_GROUP) + 1;
                            match h2_call_quiet(&mut *emu, guest_build::JavaCall::ScriptGroup, None, &[JavaArg::Int(group)]) {
                                Some(_) => eprintln!("[h2-raid] f={f} map {:?} advance — bb.a(B)V({group})", view.map),
                                None => {
                                    eprintln!("[h2-raid] f={f} the advance call was refused");
                                    raid_model.advance_refused();
                                }
                            }
                        }
                        guest_build::H2RaidAction::Escape => {
                            let group = i32::from(guest_build::H2_RAID_HOME_GROUP) + 1;
                            match h2_call_quiet(&mut *emu, guest_build::JavaCall::ScriptGroup, None, &[JavaArg::Int(group)]) {
                                Some(_) => {
                                    let writes = {
                                        let rd = |a: u32, n: usize| emu.read_memory(a, n);
                                        guest_build::h2_revive_writes(&rd, &h2)
                                    };
                                    let mut done = 0usize;
                                    for (a, bytes) in writes.iter().flatten() {
                                        if !emu.write_memory(*a, bytes) {
                                            break;
                                        }
                                        done += 1;
                                    }
                                    eprintln!("[h2-raid] f={f} map {:?} fell — home bb.a(B)V({group}) · stood up {done}/{} · tally {:?}", view.map, writes.as_ref().map_or(0, Vec::len), raid_model.tally());
                                }
                                None => {
                                    eprintln!("[h2-raid] f={f} the home call was refused");
                                    raid_model.escape_refused();
                                }
                            }
                        }
                        guest_build::H2RaidAction::ClearStage => {
                            let standing: Vec<u32> = {
                                let rd = |a: u32, n: usize| emu.read_memory(a, n);
                                guest_build::h2_monsters(&rd, sv.area.stage_slot).iter().filter(|m| m.hp > 0).map(|m| m.handle).collect()
                            };
                            for &m in &standing {
                                let _ = h2_call_quiet(&mut *emu, guest_build::JavaCall::MonsterDeath, Some(m), &[]);
                            }
                            eprintln!("[h2-raid] f={f} map {:?} the boss is down — {} monster(s) left on the stage go with it", view.map, standing.len());
                        }
                        guest_build::H2RaidAction::Pay { bosses, gold } => {
                            let before: Option<i32> = emu.read_memory(guest_build::ShopSellPatch::GOLD_SLOT, 4).try_into().map(i32::from_le_bytes).ok();
                            let paid = h2_call_quiet(&mut *emu, guest_build::JavaCall::GoldAdd, None, &[JavaArg::Int(gold as i32)]).is_some();
                            eprintln!("[h2-raid] f={f} out of the raid on map {:?} — {bosses} boss(es) · {gold} gold (issued {paid}, gold before {before:?})", view.map);
                            raid_gold_check = Some((f + 5, before));
                            if let Some(font) = &raid_font {
                                for (i, text) in guest_build::h2_raid_reward_texts(bosses, gold).iter().enumerate() {
                                    let n = guest_build::h2_notice_picture(font, text);
                                    let name = out.join(format!("raidnotice_{f:05}_{i}_{}x{}.rgba", n.w, n.h));
                                    let bytes: Vec<u8> = n.argb.iter().flat_map(|p| {
                                        let [b, g, r, a] = p.to_le_bytes();
                                        [r, g, b, a]
                                    }).collect();
                                    let _ = std::fs::write(&name, bytes);
                                }
                            }
                        }
                    }
                }
                // ★★ 2026-10-17 the app's rule (`Demon::set_pause`): 악의's clock stands between a raid kill and the next stage
                if let Some(block) = demon_block {
                    let on = raid_model.paused();
                    if raid_pause_written != Some(on) && emu.write_memory(block + guest_build::DemonPatch::PAUSE, &u32::from(on).to_le_bytes()) {
                        if raid_pause_written.is_some() || on {
                            eprintln!("[h2-raid] f={f} the demon's clock {}", if on { "stands" } else { "runs again" });
                        }
                        raid_pause_written = Some(on);
                    }
                }
                if let Some((at, before)) = raid_gold_check
                    && f >= at
                {
                    let now: Option<i32> = emu.read_memory(guest_build::ShopSellPatch::GOLD_SLOT, 4).try_into().map(i32::from_le_bytes).ok();
                    eprintln!("[h2-raid] f={f} gold {before:?} -> {now:?}");
                    raid_gold_check = None;
                }
                // --h2-raid-notice-check: the game's own notice of a text vs the host's picture of it, one text at a time.
                const CHECK_TEXTS: [&[u8]; 4] = [
                    b"\xc0\xcc\xb5\xbf \xc1\xdf...5", // 이동 중...5 — Hangul, spaces, dots and a digit
                    b"\xba\xb8\xbd\xba 9\xb8\xb6\xb8\xae \xc3\xb3\xc4\xa1!", // 보스 9마리 처치!
                    b"1000G",
                    b"\xbd\xc5\xc0\xfc\xc0\xb8\xb7\xce", // 신전으로
                ];
                if let Some(cf) = h2_raid_notice_check
                    && f >= cf
                    && view.field
                    && raid_check_idx < CHECK_TEXTS.len()
                {
                    let text = CHECK_TEXTS[raid_check_idx];
                    match raid_check_step.take() {
                        None => {
                            let out = Arc::new(GuestCallResult::new());
                            emu.new_java_byte_array(text, out.clone());
                            raid_check_step = Some((out, None));
                        }
                        Some((out, None)) => match out.poll() {
                            None => raid_check_step = Some((out, None)),
                            Some(Ok(arr)) if arr != 0 => {
                                let _ = h2_call_quiet(&mut *emu, guest_build::JavaCall::Notice, None, &[JavaArg::Object(arr), JavaArg::Int(16), JavaArg::Int(0)]);
                                raid_check_step = Some((out, Some(f)));
                            }
                            Some(r) => {
                                eprintln!("[h2-raid-check] text {raid_check_idx}: no byte[] ({r:?})");
                                raid_check_idx += 1;
                            }
                        },
                        Some((out, Some(at))) if f < at + 2 => raid_check_step = Some((out, Some(at))),
                        Some((_, Some(_))) => {
                            let got = {
                                let rd = |a: u32, n: usize| emu.read_memory(a, n);
                                (guest_build::h2_notice_pending(&rd), raid_font.clone().or_else(|| guest_build::h2_font_from_guest(&rd)))
                            };
                            match got {
                                (Some((timer_at, game)), Some(font)) => {
                                    let host = guest_build::h2_notice_picture(&font, text);
                                    let ink = |n: &guest_build::H2Notice, c: u32| -> Option<(u32, u32, u32, u32)> {
                                        let pts: Vec<(u32, u32)> = (0..n.h).flat_map(|y| (0..n.w).map(move |x| (x, y))).filter(|&(x, y)| n.argb[(y * n.w + x) as usize] == c).collect();
                                        Some((pts.iter().map(|p| p.0).min()?, pts.iter().map(|p| p.1).min()?, pts.iter().map(|p| p.0).max()?, pts.iter().map(|p| p.1).max()?))
                                    };
                                    let diff = if (game.w, game.h) == (host.w, host.h) { game.argb.iter().zip(&host.argb).filter(|(a, b)| a != b).count() } else { usize::MAX };
                                    eprintln!(
                                        "[h2-raid-check] text {raid_check_idx}: game {}x{} ink {:?} frame {:?} · host {}x{} ink {:?} frame {:?} · {} pixel(s) differ",
                                        game.w, game.h, ink(&game, guest_build::H2_NOTICE_INK), ink(&game, guest_build::H2_NOTICE_FRAME),
                                        host.w, host.h, ink(&host, guest_build::H2_NOTICE_INK), ink(&host, guest_build::H2_NOTICE_FRAME),
                                        if diff == usize::MAX { "the sizes".to_string() } else { diff.to_string() }
                                    );
                                    for (tag, n) in [("game", &game), ("host", &host)] {
                                        let bytes: Vec<u8> = n.argb.iter().flat_map(|p| {
                                            let [b, g, r, a] = p.to_le_bytes();
                                            [r, g, b, a]
                                        }).collect();
                                        let _ = std::fs::write(out.join(format!("raidcheck_{raid_check_idx}_{tag}_{}x{}.rgba", n.w, n.h)), bytes);
                                    }
                                    let _ = emu.write_memory(timer_at, &0u32.to_le_bytes());
                                }
                                (n, font) => eprintln!("[h2-raid-check] text {raid_check_idx}: notice {} · font {}", n.is_some(), font.is_some()),
                            }
                            raid_check_idx += 1;
                        }
                    }
                }
                // --h2-raid-autokill: on a stage with its flag still down, the first monster on the stage dies every 20 frames.
                if let Some((mon, res, map)) = raid_kill.take() {
                    match res.poll() {
                        None => raid_kill = Some((mon, res, map)),
                        Some(r) => {
                            let rd = |a: u32, n: usize| emu.read_memory(a, n);
                            let flag = guest_build::h2_event_flags(&rd, &sv.area).is_some_and(|fl| guest_build::h2_flag_on(&fl, guest_build::H2_RAID_FLAG));
                            eprintln!("[h2-raid] f={f} map {map:?} bd.q()V on {mon:#x} -> {r:?} — kill flag {}", if flag { "UP" } else { "still down" });
                        }
                    }
                }
                if let Some(af) = h2_raid_autokill
                    && f >= af
                    && f >= raid_kill_at
                    && raid_kill.is_none()
                    && view.field
                    && !view.flag
                    && view.map.and_then(guest_build::GuardianPatch::raid_stage).is_some()
                {
                    let mon = {
                        let rd = |a: u32, n: usize| emu.read_memory(a, n);
                        let all: Vec<_> = guest_build::h2_monsters(&rd, sv.area.stage_slot)
                            .iter()
                            .filter(|m| m.hp > 0)
                            .map(|m| {
                                let level = build().guardian_patch().and_then(|p| guest_build::h2_rd_u32(&rd, m.block + 4 + p.monster_level_off)).map_or(-1, |v| v as i32);
                                (m.handle, level, m.max_hp, m.hp, m.atk, m.ca)
                            })
                            .collect();
                        if all.len() > 1 {
                            eprintln!("[h2-raid] f={f} map {:?} on the stage: {}", view.map, all.iter().map(|(h, l, mx, _, a, ca)| format!("{h:#x} LV {l} HP {mx} ATK {a}{}", if *ca { " ca" } else { "" })).collect::<Vec<_>>().join(" · "));
                        }
                        if h2_raid_autokill_last { all.last().copied() } else { all.first().copied() }
                    };
                    if let Some((mon, level, max_hp, hp, atk, ca)) = mon {
                        eprintln!("[h2-raid] f={f} map {:?} autokill {mon:#x} (LV {level} · HP {hp}/{max_hp} · ATK {atk} · ca {ca})", view.map);
                        if let Some(res) = h2_call_quiet(&mut *emu, guest_build::JavaCall::MonsterDeath, Some(mon), &[]) {
                            raid_kill = Some((mon, res, view.map));
                        }
                    }
                    raid_kill_at = f + 20;
                }
            }
            // ── --h2-autoloot: the app's plan, one pickup call at a time ──
            if h2_autoloot && let Some(h2) = GuestBuild::Hero2Ktf.hero2_java() {
                if let Some((out, handle)) = loot_call.take() {
                    match out.poll() {
                        None => loot_call = Some((out, handle)),
                        Some(r) => eprintln!("[h2-loot] f={f} ag.a()Z on drop {handle:#x} -> {r:?}"),
                    }
                }
                let on_field = matches!(build().app_state(), Some(AppState::Absolute { addr, field, .. })
                    if emu.read_memory(addr, 4).len() == 4 && u32::from_le_bytes(emu.read_memory(addr, 4).try_into().unwrap()) == field);
                if loot_call.is_none() && on_field {
                    let stage_slot = GuestBuild::Hero2Ktf.hero2_save().map(|s| s.area.stage_slot).unwrap_or(0);
                    let plan = {
                        let rd = |a: u32, n: usize| emu.read_memory(a, n);
                        let walk = guest_build::h2_walk(&rd, &h2);
                        let standing_or_walking = walk.is_some_and(|w| w.state == guest_build::H2_WALK_STANDING || w.state == guest_build::H2_WALK_WALKING);
                        guest_build::h2_loot_near(&rd, &h2, stage_slot).filter(|_| standing_or_walking).and_then(|(tile, near)| {
                            if f >= loot_log + 100 && !near.is_empty() {
                                eprintln!("[h2-loot] f={f} player tile {tile:?}, near: {near:?}");
                            }
                            guest_build::h2_auto_loot_pick(&near, |_| false).map(|l| (tile, l, guest_build::h2_drop_tile_at(&rd, l.handle)))
                        })
                    };
                    if let Some((tile, l, Some(at))) = plan {
                        loot_log = f;
                        let moved = if (l.entry[0], l.entry[1]) != tile { emu.write_memory(at, &[tile.0 as u8, tile.1 as u8]) } else { true };
                        eprintln!("[h2-loot] f={f} taking {:?} at dist {} (moved to {tile:?} ok={moved})", l.entry, l.dist);
                        h2_sync_bag_full_row(&mut *emu, f);
                        if let Some(out) = h2_call_quiet(&mut *emu, guest_build::JavaCall::FieldPickup, None, &[]) {
                            loot_call = Some((out, l.handle));
                        }
                    }
                }
            }
            // ── ★★ 2026-10-18 --h2-drop-as: every ground drop -> (t, i) x1, under the player ──
            for &(af, t, i, under) in h2_drop_as.iter().filter(|(af, _, _, _)| *af == f) {
                let Some(h2) = GuestBuild::Hero2Ktf.hero2_java() else { break };
                let stage_slot = GuestBuild::Hero2Ktf.hero2_save().map(|s| s.area.stage_slot).unwrap_or(0);
                let writes = {
                    let rd = |a: u32, n: usize| emu.read_memory(a, n);
                    guest_build::h2_player_tile(&rd, &h2).zip(guest_build::h2_ground_drops(&rd, stage_slot)).map(|(tile, drops)| {
                        drops
                            .iter()
                            .filter_map(|(h, e)| guest_build::h2_drop_tile_at(&rd, *h).map(|at| (at, e)))
                            .map(|(at, e)| {
                                let (x, y) = if under { (tile.0, tile.1) } else { (e[0], e[1]) };
                                (at, [x as u8, y as u8, t as u8, i as u8, 1u8])
                            })
                            .collect::<Vec<_>>()
                    })
                };
                match writes {
                    Some(w) => {
                        let ok = w.iter().all(|(at, b)| emu.write_memory(*at, b));
                        eprintln!("[h2-drop-as] f={af} {} ground drop(s) -> ({t}, {i}) x1 {} (ok={ok})", w.len(), if under { "under the player" } else { "where they lie" });
                    }
                    None => eprintln!("[h2-drop-as] f={af} no stage / no character"),
                }
            }
            // ── --h2-drop-life: the app's ground-drop top-up, every 5 frames ──
            if h2_drop_life && f % 5 == 2 {
                let stage_slot = GuestBuild::Hero2Ktf.hero2_save().map(|s| s.area.stage_slot).unwrap_or(0);
                let writes = {
                    let rd = |a: u32, n: usize| emu.read_memory(a, n);
                    guest_build::h2_ground_drops(&rd, stage_slot).map(|drops| {
                        guest_build::h2_drop_life_plan(&drops, &mut drop_ledger)
                            .into_iter()
                            .filter_map(|(h, v)| guest_build::h2_drop_life_at(&rd, h).map(|a| (h, a, v)))
                            .collect::<Vec<_>>()
                    })
                };
                for (h, a, v) in writes.unwrap_or_default() {
                    let ok = emu.write_memory(a, &[v as u8]);
                    let given = drop_ledger.iter().find(|l| l.handle == h).map_or(0, |l| l.given);
                    eprintln!("[h2-drop-life] f={f} drop {h:#x} e[5] <- {v} (given {given} tick(s) so far, ok={ok})");
                }
            }
            // ── --h2-difficulty <f:d>: the app's own monster scaler (2026-09-29) ────────
            if let Some((_, d)) = h2_difficulty.iter().filter(|(at, _)| f >= *at).last() {
                if diff_level != Some(*d) {
                    eprintln!("[h2-diff] f={f} level -> {d} ({}%)", guest_build::difficulty_pct(*d));
                }
                diff_level = Some(*d);
            }
            // ★ 2026-10-02 저녁: the HELL bonus roll's gate word follows the 난이도, as the app's poll does.
            if let Some(block) = hell_drop_block {
                let open = h2_hell_gate.unwrap_or_else(|| guest_build::HellDropPatch::gate_open(diff_level.unwrap_or(guest_build::DIFFICULTY_DEFAULT)));
                if hell_drop_gate != Some(open)
                    && let Some(patch) = guest_build::GuestBuild::Hero2Ktf.hell_drop_patch()
                    && let Some((at, word)) = guest_build::h2_hell_drop_gate_write(&patch, block, open)
                {
                    let ok = emu.write_memory(at, &word);
                    eprintln!("[h2-hell-drop] f={f} 난이도 gate (NORMAL+) {} (word {at:#x} <- {}, ok={ok})", if open { "open" } else { "closed" }, u8::from(open));
                    hell_drop_gate = Some(open);
                }
            }
            // ── 가디언 (2026-10-08): the mode word and the block's words, when either changes ──
            if f % 5 == 3
                && let Some(p) = GuestBuild::Hero2Ktf.guardian_patch()
            {
                let rd = |a: u32, n: usize| emu.read_memory(a, n);
                let mode = guest_build::h2_rd_u32(&rd, p.mode_slot).unwrap_or(u32::MAX);
                let words = guardian_block.and_then(|b| guest_build::h2_guardian_words(&rd, &p, b));
                let line = format!("block={} words={words:?} tables={}", guardian_block.map_or("-".to_string(), |b| format!("{b:#x}")), if guardian_tables.done { "OPEN" } else if guardian_tables.off { "OFF" } else { "pending" });
                if guardian_seen.as_ref() != Some(&(line.clone(), mode)) {
                    eprintln!("[h2-guardian] f={f} ag.i={mode} {line}");
                    guardian_seen = Some((line, mode));
                }
            }
            // ★ 2026-10-16 보스 레이드: the raid's maps hold their own percent — ★★ 2026-10-17 ⅓ above the setting (133 · 200 ·
            //   267 %; `h2_difficulty_want`, the app's `Difficulty::poll` rule), the setting being the flag's or EASY without it.
            //   ⛔ The live stage's number (`h2_live_map`) — `ag.h` is −1 once the map is up.
            let live_area = GuestBuild::Hero2Ktf.hero2_save().map(|s| s.area);
            let live_map = |emu: &mut dyn Emulator| {
                let rd = |a: u32, n: usize| emu.read_memory(a, n);
                live_area.and_then(|area| guest_build::h2_live_map(&rd, &area))
            };
            let raid_map = f % 5 == 2 && live_map(&mut *emu).and_then(guest_build::GuardianPatch::raid_stage).is_some();
            if let Some(d) = if raid_map { Some(diff_level.unwrap_or(guest_build::DIFFICULTY_DEFAULT)) } else { diff_level }
                && f % 5 == 2
                && let Some(stage_slot) = GuestBuild::Hero2Ktf.hero2_save().map(|s| s.area.stage_slot)
            {
                // ★★ 2026-10-22 (상점주인의 혼 뒤 받는 피해 급증): the app's `Difficulty::poll` gate, missing here until this day.
                //   `--h2-diff-gate 0` = the gate as it was: `items::in_game_state` false (a state word outside
                //   `AppState::in_game` = 3 · 4 · 5 · 8 — the SHOP is 7) made the planner FORGET its bases, and the next in-game
                //   poll took the scaled words as fresh bases: ×m again. 1 (the app now) = out of the WORLD (the title shell or
                //   no character, `h2_demon_in_world`) forgets; a new map is a new stage object, which the planner keys on.
                let keep = {
                    let rd = |a: u32, n: usize| emu.read_memory(a, n);
                    if h2_diff_gate {
                        build().hero2_java().is_some_and(|h2| guest_build::h2_demon_in_world(&rd, &h2))
                    } else {
                        match build().app_state() {
                            Some(st @ AppState::Absolute { addr, .. }) => {
                                let b = rd(addr, 4);
                                b.len() == 4 && st.in_game(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
                            }
                            _ => false,
                        }
                    }
                };
                if !keep {
                    if !diff_state.is_empty() {
                        eprintln!("[h2-diff] f={f} out of the {} — the planner forgets its bases", if h2_diff_gate { "world" } else { "state word's in-game set (the gate before 2026-10-22)" });
                    }
                    diff_state.forget();
                } else {
                    let mode = {
                        let rd = |a: u32, n: usize| emu.read_memory(a, n);
                        GuestBuild::Hero2Ktf.guardian_patch().and_then(|p| guest_build::h2_rd_u32(&rd, p.mode_slot)).map(|w| w & 0xff).unwrap_or(0)
                    };
                    let (want_hp, want_atk) = guest_build::h2_monster_want(live_map(&mut *emu), d, raid_model.lap(), mode);
                    // ★ 2026-10-21: the raid's 오딘 stage holds his hands at half (`h2_raid_hand_hp_pct`, the app's rule).
                    let hand_pct = guest_build::h2_raid_hand_hp_pct(live_map(&mut *emu));
                    let (before, writes) = {
                        let rd = |a: u32, n: usize| emu.read_memory(a, n);
                        (guest_build::h2_monsters(&rd, stage_slot), diff_state.plan_hands(&rd, stage_slot, want_hp, want_atk, hand_pct))
                    };
                    if !writes.is_empty() {
                        let mut ok = true;
                        for (a, bytes) in &writes {
                            ok &= emu.write_memory(*a, bytes);
                        }
                        let rd = |a: u32, n: usize| emu.read_memory(a, n);
                        let after = guest_build::h2_monsters(&rd, stage_slot);
                        eprintln!("[h2-diff] f={f} level={d} mode={mode} (HP {want_hp}% · ATK {want_atk}%, raid lap {}) {} write(s) ok={ok} monsters={}", raid_model.lap(), writes.len(), after.len());
                        for (b, a) in before.iter().zip(after.iter()).take(6) {
                            eprintln!(
                                "[h2-diff]   {:#x} hp {}/{} atk {} -> hp {}/{} atk {}{}",
                                b.handle, b.hp, b.max_hp, b.atk, a.hp, a.max_hp, a.atk,
                                if a.ca { format!(" (ca, template atk {} -> {})", b.template_atk, a.template_atk) } else if a.hand { format!(" (hand, {hand_pct}% of the stage's HP)") } else { String::new() }
                            );
                        }
                    }
                }
            }
            // ── --h2-drop-rate: the 드랍률 filter, the app's own plan (install once below 높음, then the KEEP word) ──
            if let Some(&(_, d)) = h2_drop_rate.iter().find(|(df, _)| *df == f)
                && let Some(patch) = guest_build::GuestBuild::Hero2Ktf.drop_rate_patch()
            {
                let keep = guest_build::drop_rate_keep(d);
                match drop_block {
                    Some((_, words)) => {
                        let ok = emu.write_memory(words + guest_build::DropRatePatch::KEEP, &keep.to_le_bytes());
                        let rd = |a: u32, n: usize| emu.read_memory(a, n);
                        eprintln!("[h2-drop-rate] f={f} level={d} keep={keep:#x} ok={ok} words={:?}", guest_build::h2_drop_rate_words(&rd, words));
                    }
                    None if keep >= 0x10000 => eprintln!("[h2-drop-rate] f={f} level={d} = 높음: nothing to install"),
                    None => match emu.alloc_guest(guest_build::DropRatePatch::BLOCK_LEN) {
                        Some(block) => {
                            let plan = {
                                let rd = |a: u32, n: usize| emu.read_memory(a, n);
                                guest_build::h2_drop_rate_install_writes(&rd, &patch, block, keep, 0x2545_f491)
                            };
                            match plan {
                                Some((writes, words)) => {
                                    let mut ok = true;
                                    for (a, bytes) in &writes {
                                        ok &= emu.write_memory(*a, bytes);
                                    }
                                    let rd = |a: u32, n: usize| emu.read_memory(a, n);
                                    let installed = guest_build::h2_drop_rate_installed(&rd, &patch, block);
                                    eprintln!(
                                        "[h2-drop-rate] f={f} level={d} block={block:#x} def={:#x} {} write(s) ok={ok} installed={installed} words={:?}",
                                        patch.def,
                                        writes.len(),
                                        guest_build::h2_drop_rate_words(&rd, words)
                                    );
                                    drop_block = Some((block, words));
                                }
                                None => eprintln!("[h2-drop-rate] f={f} REFUSED — bf.a(BBBB)V's definition at {:#x} does not read as shipped", patch.def),
                            }
                        }
                        None => eprintln!("[h2-drop-rate] f={f} no guest block"),
                    },
                }
            }
            // ── --h2-exp: the 경험치 배율 redirect, the app's own plan (once, from f=30) ──
            if let (Some(mult), None) = (h2_exp, exp_block)
                && f >= 30
                && let Some(patch) = guest_build::GuestBuild::Hero2Ktf.exp_rate_patch()
            {
                match emu.alloc_guest(guest_build::ExpRatePatch::BLOCK_LEN) {
                    Some(block) => {
                        let plan = {
                            let rd = |a: u32, n: usize| emu.read_memory(a, n);
                            guest_build::h2_exp_rate_install_writes(&rd, &patch, block, u32::from(mult))
                        };
                        match plan {
                            Some(writes) => {
                                let mut ok = true;
                                for (a, bytes) in &writes {
                                    ok &= emu.write_memory(*a, bytes);
                                }
                                let rd = |a: u32, n: usize| emu.read_memory(a, n);
                                let installed = guest_build::h2_exp_rate_installed(&rd, &patch, block);
                                let now = guest_build::h2_exp_rate_mult(&rd, block);
                                eprintln!("[h2-exp] f={f} block={block:#x} def={:#x} {} write(s) ok={ok} installed={installed} mult={now:?}", patch.def, writes.len());
                                exp_block = Some(block);
                            }
                            None => eprintln!("[h2-exp] f={f} REFUSED — a.d(I)V's definition at {:#x} does not read as shipped", patch.def),
                        }
                    }
                    None => eprintln!("[h2-exp] f={f} no guest block"),
                }
            }
            // ── #17: the SKILL POWER redirect, the app's own plan (once, from f=30) ──────
            if !h2_shipped && !skill_power_tried && f >= 30 {
                skill_power_tried = true;
                if let Some(patch) = guest_build::GuestBuild::Hero2Ktf.skill_power_patch() {
                    match emu.alloc_guest(guest_build::SkillPowerPatch::BLOCK_LEN) {
                        Some(block) => {
                            let plan = {
                                let rd = |a: u32, n: usize| emu.read_memory(a, n);
                                guest_build::h2_skill_power_install_writes(&rd, &patch, block)
                            };
                            match plan {
                                Some(writes) => {
                                    let mut ok = true;
                                    for (a, bytes) in &writes {
                                        ok &= emu.write_memory(*a, bytes);
                                    }
                                    let rd = |a: u32, n: usize| emu.read_memory(a, n);
                                    let installed = guest_build::h2_skill_power_installed(&rd, &patch, block);
                                    eprintln!("[h2-skillpower] f={f} block={block:#x} {} write(s) ok={ok} installed={installed}", writes.len());
                                    if installed {
                                        skill_block = Some(block);
                                    }
                                }
                                None => eprintln!("[h2-skillpower] f={f} REFUSED — the three defs do not read as shipped"),
                            }
                        }
                        None => eprintln!("[h2-skillpower] f={f} no guest block"),
                    }
                }
            }
            // ── 2026-09-27: the COMBAT block, the app's own plan (once, from f=30) ──────────
            if !h2_shipped && !combat_tried && f >= 30 {
                combat_tried = true;
                if let Some(patch) = guest_build::GuestBuild::Hero2Ktf.combat_patch() {
                    match emu.alloc_guest(guest_build::CombatPatch::BLOCK_LEN) {
                        Some(block) => {
                            let plan = {
                                let rd = |a: u32, n: usize| emu.read_memory(a, n);
                                guest_build::h2_combat_install_writes(&rd, &patch, block)
                            };
                            match plan {
                                Some(writes) => {
                                    let mut ok = true;
                                    for (a, bytes) in &writes {
                                        ok &= emu.write_memory(*a, bytes);
                                    }
                                    let rd = |a: u32, n: usize| emu.read_memory(a, n);
                                    let installed = guest_build::h2_combat_installed(&rd, &patch, block);
                                    let used: usize = guest_build::h2_combat_blocks(&patch, block).map_or(0, |b| b.iter().map(|(_, v)| v.len()).sum());
                                    eprintln!("[h2-combat] f={f} block={block:#x} {} write(s) ok={ok} installed={installed} ({used} B of code)", writes.len());
                                    if installed {
                                        combat_block = Some(block);
                                    }
                                }
                                None => eprintln!("[h2-combat] f={f} REFUSED — a def, pool word or the guard site does not read as shipped"),
                            }
                        }
                        None => eprintln!("[h2-combat] f={f} no guest block"),
                    }
                }
            }
            // ── 2026-09-29: the 맹독감염 block, the app's own plan (once, from f=30) ─────────────
            //    `[h2-flags]` shows the buff (id 33); `[h2-mobs]` the poisoned monster's HP.
            if !h2_shipped && h2_venom && !venom_tried && f >= 30 {
                venom_tried = true;
                if let Some(patch) = guest_build::GuestBuild::Hero2Ktf.venom_patch() {
                    match emu.alloc_guest(guest_build::VenomPatch::BLOCK_LEN) {
                        Some(block) => {
                            let plan = {
                                let rd = |a: u32, n: usize| emu.read_memory(a, n);
                                guest_build::h2_venom_install_writes(&rd, &patch, block)
                            };
                            match plan {
                                Some(writes) => {
                                    let mut ok = true;
                                    for (a, bytes) in &writes {
                                        ok &= emu.write_memory(*a, bytes);
                                    }
                                    let rd = |a: u32, n: usize| emu.read_memory(a, n);
                                    let installed = guest_build::h2_venom_installed(&rd, &patch, block);
                                    let used: usize = guest_build::h2_venom_blocks(&patch).map_or(0, |b| b.iter().map(|(_, v)| v.len()).sum());
                                    eprintln!("[h2-venom] f={f} block={block:#x} {} write(s) ok={ok} installed={installed} ({used} B of code)", writes.len());
                                    if installed {
                                        venom_block = Some(block);
                                    }
                                }
                                None => eprintln!("[h2-venom] f={f} REFUSED — the cast, cooldown, strike def or tick site does not read as shipped"),
                            }
                        }
                        None => eprintln!("[h2-venom] f={f} no guest block"),
                    }
                }
            }
            // ── 2026-09-30: the 야수진화제 sell-price block, the app's own plan (once, from f=30) ──
            if !h2_shipped && h2_shop_sell && !shop_sell_tried && f >= 30 {
                shop_sell_tried = true;
                if let Some(patch) = guest_build::GuestBuild::Hero2Ktf.shop_sell_patch() {
                    match emu.alloc_guest(guest_build::ShopSellPatch::BLOCK_LEN) {
                        Some(block) => {
                            let plan = {
                                let rd = |a: u32, n: usize| emu.read_memory(a, n);
                                guest_build::h2_shop_sell_install_writes(&rd, &patch, block)
                            };
                            match plan {
                                Some(writes) => {
                                    let mut ok = true;
                                    for (a, bytes) in &writes {
                                        ok &= emu.write_memory(*a, bytes);
                                    }
                                    let rd = |a: u32, n: usize| emu.read_memory(a, n);
                                    let installed = guest_build::h2_shop_sell_installed(&rd, &patch, block);
                                    let used: usize = guest_build::h2_shop_sell_blocks(&patch).map_or(0, |b| b.iter().map(|(_, v)| v.len()).sum());
                                    eprintln!("[h2-shop-sell] f={f} block={block:#x} {} write(s) ok={ok} installed={installed} ({used} B of code)", writes.len());
                                    if installed {
                                        shop_sell_block = Some(block);
                                    }
                                }
                                None => eprintln!("[h2-shop-sell] f={f} REFUSED — the sale's or the painter's ch.d read does not read as shipped"),
                            }
                        }
                        None => eprintln!("[h2-shop-sell] f={f} no guest block"),
                    }
                }
            }
            // ── ★ 2026-10-10: the boss crit floor, the app's own plan (once, from f=30) ──
            if !h2_shipped && h2_boss_crit && !boss_crit_tried && f >= 30 {
                boss_crit_tried = true;
                if let Some(patch) = guest_build::GuestBuild::Hero2Ktf.boss_crit_patch() {
                    match emu.alloc_guest(guest_build::BossCritPatch::BLOCK_LEN) {
                        Some(block) => {
                            let plan = {
                                let rd = |a: u32, n: usize| emu.read_memory(a, n);
                                guest_build::h2_boss_crit_install_writes(&rd, &patch, block)
                            };
                            match plan {
                                Some(writes) => {
                                    let mut ok = true;
                                    for (a, bytes) in &writes {
                                        ok &= emu.write_memory(*a, bytes);
                                    }
                                    let rd = |a: u32, n: usize| emu.read_memory(a, n);
                                    let installed = guest_build::h2_boss_crit_installed(&rd, &patch, block);
                                    let used = guest_build::h2_boss_crit_block(&patch).map_or(0, |b| b.len());
                                    eprintln!("[h2-boss-crit] f={f} block={block:#x} {} write(s) ok={ok} installed={installed} ({used} B of code)", writes.len());
                                }
                                None => eprintln!("[h2-boss-crit] f={f} REFUSED — the boss hit routine's crit read does not read as shipped"),
                            }
                        }
                        None => eprintln!("[h2-boss-crit] f={f} no guest block"),
                    }
                }
            }
            // ── ★ 2026-10-21: the boss defence pierce, the app's own plan (once, from f=30) ──
            if !h2_shipped && h2_boss_pierce > 0 && !boss_pierce_tried && f >= 30 {
                boss_pierce_tried = true;
                if let Some(patch) = guest_build::GuestBuild::Hero2Ktf.boss_pierce_patch() {
                    match emu.alloc_guest(guest_build::BossPiercePatch::BLOCK_LEN) {
                        Some(block) => {
                            let plan = {
                                let rd = |a: u32, n: usize| emu.read_memory(a, n);
                                guest_build::h2_boss_pierce_install_writes(&rd, &patch, block, h2_boss_pierce)
                            };
                            match plan {
                                Some(writes) => {
                                    let mut ok = true;
                                    for (a, bytes) in &writes {
                                        ok &= emu.write_memory(*a, bytes);
                                    }
                                    let rd = |a: u32, n: usize| emu.read_memory(a, n);
                                    let installed = guest_build::h2_boss_pierce_installed(&rd, &patch, block);
                                    let used = guest_build::h2_boss_pierce_block(&patch, block).map_or(0, |b| b.0.len());
                                    eprintln!("[h2-boss-pierce] f={f} block={block:#x} {h2_boss_pierce}% {} write(s) ok={ok} installed={installed} ({used} B of code)", writes.len());
                                }
                                None => eprintln!("[h2-boss-pierce] f={f} REFUSED — the hit routine's defence / shield reads do not read as shipped"),
                            }
                        }
                        None => eprintln!("[h2-boss-pierce] f={f} no guest block"),
                    }
                }
            }
            // ── ★ 2026-10-13: the grade line, the app's own plan (once, from f=30) ──
            if !h2_shipped && h2_grade_line && !grade_line_tried && f >= 30 {
                grade_line_tried = true;
                if let Some(patch) = guest_build::GuestBuild::Hero2Ktf.grade_line_patch() {
                    match emu.alloc_guest(guest_build::GradeLinePatch::BLOCK_LEN) {
                        Some(block) => {
                            let plan = {
                                let rd = |a: u32, n: usize| emu.read_memory(a, n);
                                guest_build::h2_grade_line_install_writes(&rd, &patch, block)
                            };
                            match plan {
                                Some(writes) => {
                                    let mut ok = true;
                                    for (a, bytes) in &writes {
                                        ok &= emu.write_memory(*a, bytes);
                                    }
                                    let rd = |a: u32, n: usize| emu.read_memory(a, n);
                                    let installed = guest_build::h2_grade_line_installed(&rd, &patch, block);
                                    let used = guest_build::h2_grade_line_block(&patch).map_or(0, |b| b.len());
                                    eprintln!("[h2-grade-line] f={f} block={block:#x} {} write(s) ok={ok} installed={installed} ({used} B of code)", writes.len());
                                }
                                None => eprintln!("[h2-grade-line] f={f} REFUSED — the info painter's grade draw does not read as shipped"),
                            }
                        }
                        None => eprintln!("[h2-grade-line] f={f} no guest block"),
                    }
                }
            }
            // ── 2026-10-02: the HELL bonus drop, the app's own plan (once, from f=30) ──
            if !h2_shipped && h2_hell_drop && !hell_drop_tried && f >= 30 {
                hell_drop_tried = true;
                if let Some(patch) = guest_build::GuestBuild::Hero2Ktf.hell_drop_patch() {
                    match emu.alloc_guest(guest_build::HellDropPatch::BLOCK_LEN) {
                        Some(block) => {
                            let plan = {
                                let rd = |a: u32, n: usize| emu.read_memory(a, n);
                                guest_build::h2_hell_drop_install_writes(&rd, &patch, block)
                            };
                            match plan {
                                Some(writes) => {
                                    let mut ok = true;
                                    for (a, bytes) in &writes {
                                        ok &= emu.write_memory(*a, bytes);
                                    }
                                    let rd = |a: u32, n: usize| emu.read_memory(a, n);
                                    let installed = guest_build::h2_hell_drop_installed(&rd, &patch, block);
                                    let used = guest_build::h2_hell_drop_block(&patch).map_or(0, |b| b.len());
                                    eprintln!("[h2-hell-drop] f={f} block={block:#x} {} write(s) ok={ok} installed={installed} ({used} B of code)", writes.len());
                                    if installed {
                                        hell_drop_block = Some(block);
                                    }
                                }
                                None => eprintln!("[h2-hell-drop] f={f} REFUSED — bd.q()V's definition word does not read as shipped"),
                            }
                        }
                        None => eprintln!("[h2-hell-drop] f={f} no guest block"),
                    }
                }
            }
            // ── 2026-10-08 가디언: the template block + the four bd.n hooks, the app's own plan (once, from f=30) ──
            if !h2_shipped && h2_guardian && !guardian_tried && f >= 30 {
                guardian_tried = true;
                if let Some(patch) = GuestBuild::Hero2Ktf.guardian_patch() {
                    match emu.alloc_guest(guest_build::GuardianPatch::BLOCK_LEN) {
                        Some(block) => {
                            let plan = {
                                let rd = |a: u32, n: usize| emu.read_memory(a, n);
                                guest_build::h2_guardian_install_writes(&rd, &patch, block)
                            };
                            match plan {
                                Some(writes) => {
                                    let mut ok = true;
                                    for (a, bytes) in &writes {
                                        ok &= emu.write_memory(*a, bytes);
                                    }
                                    let installed = {
                                        let rd = |a: u32, n: usize| emu.read_memory(a, n);
                                        guest_build::h2_guardian_installed(&rd, &patch, block)
                                    };
                                    let used = guest_build::h2_guardian_block(&patch).map_or(0, |b| b.len());
                                    eprintln!("[h2-guardian] f={f} block={block:#x} {} write(s) ok={ok} installed={installed} ({used} B)", writes.len());
                                    if installed {
                                        guardian_block = Some(block);
                                        for &(i, v) in &h2_guardian_words {
                                            if let Some((at, bytes)) = guest_build::h2_guardian_word_write(&patch, block, i, v) {
                                                let ok = emu.write_memory(at, &bytes);
                                                eprintln!("[h2-guardian] f={f} word {i} <- {v} ({at:#x}, ok={ok})");
                                            }
                                        }
                                    }
                                }
                                None => eprintln!("[h2-guardian] f={f} REFUSED — cq.a()V's definition word or a bd.n read site does not read as shipped"),
                            }
                        }
                        None => eprintln!("[h2-guardian] f={f} no guest block"),
                    }
                }
            }
            // ── 2026-10-02: the skill-changing accessories, the app's own plan (once, from f=30, after
            //    the SkillPower block — two of its pieces chain to what that left in the definitions) ──
            if !h2_shipped && h2_acc_patch && !acc_tried && f >= 30 {
                acc_tried = true;
                if let Some(patch) = guest_build::GuestBuild::Hero2Ktf.accessory_patch() {
                    match emu.alloc_guest(guest_build::AccessoryPatch::BLOCK_LEN) {
                        Some(block) => {
                            let plan = {
                                let rd = |a: u32, n: usize| emu.read_memory(a, n);
                                guest_build::h2_accessory_chain(&rd, &patch, block, None)
                                    .and_then(|chain| guest_build::h2_accessory_install_writes(&rd, &patch, block, chain).map(|w| (chain, w)))
                            };
                            match plan {
                                Some((chain, writes)) => {
                                    let mut ok = true;
                                    for (a, bytes) in &writes {
                                        ok &= emu.write_memory(*a, bytes);
                                    }
                                    let rd = |a: u32, n: usize| emu.read_memory(a, n);
                                    let installed = guest_build::h2_accessory_installed(&rd, &patch, block, chain);
                                    eprintln!("[h2-acc] f={f} block={block:#x} {} write(s) ok={ok} installed={installed} chain={chain:#x}", writes.len());
                                    if installed {
                                        acc_block = Some((block, chain));
                                    }
                                }
                                None => eprintln!("[h2-acc] f={f} REFUSED - an arm does not read as shipped"),
                            }
                        }
                        None => eprintln!("[h2-acc] f={f} no guest block"),
                    }
                }
            }
            // ── 2026-10-05 일풍격: the app's plan, right after the accessory block (the usable / drive words chain) ──
            if !h2_shipped && h2_ilpung && !ilpung_tried && f >= 30 && (acc_tried || !h2_acc_patch) {
                ilpung_tried = true;
                if let (Some(patch), Some(acc)) = (guest_build::GuestBuild::Hero2Ktf.ilpung_patch(), guest_build::GuestBuild::Hero2Ktf.accessory_patch()) {
                    match emu.alloc_guest(guest_build::IlpungPatch::BLOCK_LEN) {
                        Some(block) => {
                            let plan = {
                                let rd = |a: u32, n: usize| emu.read_memory(a, n);
                                guest_build::h2_ilpung_install_writes(&rd, &patch, &acc, block)
                            };
                            match plan {
                                Some(writes) => {
                                    let mut ok = true;
                                    for (a, bytes) in &writes {
                                        ok &= emu.write_memory(*a, bytes);
                                    }
                                    let rd = |a: u32, n: usize| emu.read_memory(a, n);
                                    let installed = guest_build::h2_ilpung_installed(&rd, &patch, block);
                                    eprintln!("[h2-ilpung] f={f} block={block:#x} {} write(s) ok={ok} installed={installed}", writes.len());
                                    if installed {
                                        ilpung_block = Some(block);
                                    }
                                }
                                None => eprintln!("[h2-ilpung] f={f} REFUSED - an arm does not read as shipped"),
                            }
                        }
                        None => eprintln!("[h2-ilpung] f={f} no guest block"),
                    }
                }
            }
            // ── 2026-10-07 버서커: the app's plan, after SkillPower · accessory · combat · 일풍격 (its words chain onto theirs) ──
            if !h2_shipped && h2_berserker && !berserker_tried && f >= 30 && (ilpung_tried || !h2_ilpung) && (acc_tried || !h2_acc_patch) && combat_tried {
                berserker_tried = true;
                if let (Some(patch), Some(cp)) = (guest_build::GuestBuild::Hero2Ktf.berserker_patch(), guest_build::GuestBuild::Hero2Ktf.combat_patch()) {
                    let rage_off = guest_build::GuestBuild::Hero2Ktf.rage_shipped_off();
                    let hook = combat_block.and_then(|c| guest_build::h2_iron_hook_at(&cp).map(|h| c + h));
                    match emu.alloc_guest(guest_build::BerserkerPatch::BLOCK_LEN) {
                        Some(block) => {
                            let plan = {
                                let rd = |a: u32, n: usize| emu.read_memory(a, n);
                                guest_build::h2_berserker_chain(&rd, &patch, block)
                                    .and_then(|chain| {
                                        let acc_pulse = acc_block.map(|(b, _)| b + guest_build::AccessoryPatch::STORM_PULSE_W);
                                        guest_build::h2_berserker_install_writes(&rd, &patch, block, rage_off, hook, acc_pulse, h2_voice).map(|w| (chain, w))
                                    })
                            };
                            match plan {
                                Some((chain, writes)) => {
                                    let mut ok = true;
                                    for (a, bytes) in &writes {
                                        ok &= emu.write_memory(*a, bytes);
                                    }
                                    let rd = |a: u32, n: usize| emu.read_memory(a, n);
                                    let installed = guest_build::h2_berserker_installed(&rd, &patch, block, &chain, rage_off, hook);
                                    eprintln!("[h2-berserker] f={f} block={block:#x} {} write(s) ok={ok} installed={installed} hook={hook:?} chain={chain:x?}", writes.len());
                                    if installed {
                                        berserker_block = Some(block);
                                    }
                                }
                                None => eprintln!("[h2-berserker] f={f} REFUSED - a definition / rage halfword / hook word does not read as expected"),
                            }
                        }
                        None => eprintln!("[h2-berserker] f={f} no guest block"),
                    }
                }
            }
            if let (Some(block), Some((at, g))) = (berserker_block, h2_fury_gauge) {
                if f == at {
                    let hits = g * u32::from(guest_build::BerserkerPatch::FURY_HITS_FULL) / 100;
                    let ok = emu.write_memory(block + guest_build::BerserkerPatch::FURY_GAUGE, &g.to_le_bytes())
                        && emu.write_memory(block + guest_build::BerserkerPatch::FURY_HITS, &hits.to_le_bytes());
                    eprintln!("[h2-fury-gauge] f={f} GAUGE <- {g} · HITS <- {hits} ({})", if ok { "ok" } else { "FAILED" });
                }
            }
            if let Some(block) = berserker_block {
                let rd = |a: u32, n: usize| emu.read_memory(a, n);
                let h2 = GuestBuild::Hero2Ktf.hero2_java().unwrap();
                let (taunt, last) = guest_build::h2_berserker_words(&rd, block).unwrap_or((-1, -1));
                let rage = guest_build::h2_static_handle(&rd, h2.char_slot)
                    .and_then(|ch| guest_build::h2_fields(&rd, ch))
                    .and_then(|fb| guest_build::h2_rd_u32(&rd, fb + 4 + guest_build::GuestBuild::Hero2Ktf.berserker_patch().unwrap().rage_off))
                    .map_or(-1, |v| v as i32);
                let buffs: Vec<String> = guest_build::h2_char_buffs(&rd, h2.char_slot)
                    .unwrap_or_default()
                    .iter()
                    .map(|b| format!("{}:{}/{}{}", b.id, b.elapsed, b.len, if b.ended { "E" } else { "" }))
                    .collect();
                let fury = guest_build::h2_berserker_fury(&rd, block)
                    .map_or_else(|| "?".to_string(), |w| format!("{}/{}h/{}% auto={} expl={} mul={}", w.state, w.hits, w.gauge, w.auto, w.exploding, w.last_mul));
                // ★ 2026-10-07 (2): the action as the driver sees it — g.t · g.w · the queued kind a.o[a.i] · a.n[15..=16]
                let bp = guest_build::GuestBuild::Hero2Ktf.berserker_patch().unwrap();
                let action = guest_build::h2_static_handle(&rd, h2.char_slot)
                    .and_then(|ch| guest_build::h2_fields(&rd, ch))
                    .map(|fb| {
                        let w = |o: u32| guest_build::h2_rd_u32(&rd, fb + 4 + o).map_or(-1, |v| v as i32);
                        let queue = guest_build::h2_rd_u32(&rd, fb + 4 + bp.queue_off).and_then(|h| guest_build::h2_byte_array(&rd, h)).unwrap_or_default();
                        let at = w(bp.queue_at_off);
                        let kind = usize::try_from(at).ok().and_then(|i| queue.get(i)).map_or(-2, |k| *k as i8 as i32);
                        let frames = guest_build::h2_static_handle(&rd, bp.frames_slot).and_then(|h| guest_build::h2_byte_array(&rd, h)).unwrap_or_default();
                        format!("g.t={} g.w={} a.i={at} kind={kind} a.n[15..]={:?}", w(bp.state_off), w(bp.frame_off), frames.get(15..).unwrap_or(&[]))
                    })
                    .unwrap_or_else(|| "no character".to_string());
                let line = format!("TAUNT={taunt} LAST={last} a.n={rage} FURY={fury} {action} buffs {buffs:?}");
                if line != berserker_seen || (gauges > 0 && f % gauges == 0) {
                    eprintln!("[h2-berserker] f={f} {line}");
                    berserker_seen = line;
                }
            }
            // ★ 2026-10-09 the voices — what the app's worker does each frame (emulator.rs 4c4), the clip into --audio-wav
            if h2_voice && h2_mode {
                let h2 = GuestBuild::Hero2Ktf.hero2_java().unwrap();
                let (class, action, fury, level) = {
                    let rd = |a: u32, n: usize| emu.read_memory(a, n);
                    (
                        rd(h2.class_slot, 1).first().copied(),
                        guest_build::h2_action_kind(&rd, h2.char_slot),
                        berserker_block.and_then(|b| guest_build::h2_berserker_fury(&rd, b)).map(|w| (w.state, w.exploding)),
                        rd(guest_build::Hero2Java::SOUND_LEVEL_SLOT, 1).first().map(|b| *b as i8),
                    )
                };
                let demon_state = {
                    let rd = |a: u32, n: usize| emu.read_memory(a, n);
                    demon_block.and_then(|b| guest_build::h2_demon_words(&rd, b)).map(|w| w.state)
                };
                if let Some(cue) = voice_watch.step(class, action, fury, demon_state) {
                    let pcm = level.and_then(|lv| voice::voice_pcm(cue.index(), lv));
                    let ms = clock.load(Ordering::SeqCst);
                    eprintln!(
                        "[h2-voice] f={f} t={ms}ms {} ({cue:?}) class={class:?} level={level:?} {}",
                        cue.file(),
                        pcm.as_ref().map_or("silent".to_string(), |(c, r, d)| format!("{c}ch {r} Hz {} samples", d.len()))
                    );
                    if let (Some((c, r, d)), Some((_, cap))) = (pcm, AUDIO_WAV.get()) {
                        if let Ok(mut cap) = cap.lock() {
                            cap.wave(ms, c, r, &d);
                        }
                    }
                }
                let (judge, bolt) = {
                    let rd = |a: u32, n: usize| emu.read_memory(a, n);
                    guest_build::h2_thunder_words(&rd, demon_block, acc_block.map(|(b, _)| b))
                };
                if thunder_watch.step(judge, bolt) {
                    let pcm = level.and_then(|lv| voice::voice_pcm(guest_build::H2Voice::Thunder.index(), lv));
                    let ms = clock.load(Ordering::SeqCst);
                    eprintln!(
                        "[h2-thunder] f={f} t={ms}ms judge={judge:?} bolt={bolt:?} level={level:?} {}",
                        pcm.as_ref().map_or("silent".to_string(), |(c, r, d)| format!("{c}ch {r} Hz {} samples", d.len()))
                    );
                    if let (Some((c, r, d)), Some((_, cap))) = (pcm, AUDIO_WAV.get()) {
                        if let Ok(mut cap) = cap.lock() {
                            cap.wave(ms, c, r, &d);
                        }
                    }
                }
            }
            if let (Some(block), true) = (ilpung_block, gauges > 0) {
                let words = {
                    let rd = |a: u32, n: usize| emu.read_memory(a, n);
                    let w = |o: u32| guest_build::h2_rd_u32(&rd, block + o).map_or(-1, |v| v as i32);
                    type IP = guest_build::IlpungPatch;
                    // ★ 2026-10-09 (11) + 「암살」's bonus count and the last bonus (a first blow prints at once)
                    [w(IP::TARGET), w(IP::X), w(IP::Y), w(IP::LAST), w(IP::AMSAL_COUNT), w(IP::AMSAL_LAST)]
                };
                if f % gauges == 0 || words != ilpung_words {
                    ilpung_words = words;
                    let rd = |a: u32, n: usize| emu.read_memory(a, n);
                    let w = |o: u32| guest_build::h2_rd_u32(&rd, block + o).map_or(-1, |v| v as i32);
                    type IP = guest_build::IlpungPatch;
                    let h2 = GuestBuild::Hero2Ktf.hero2_java().unwrap();
                    let me = guest_build::h2_static_handle(&rd, h2.char_slot).and_then(|ch| guest_build::h2_fields(&rd, ch)).map(|fb| {
                        let r = |o: u32| guest_build::h2_rd_u32(&rd, fb + 4 + o).map_or(-1, |v| v as i32);
                        (r(0x10), r(0x14), r(0x40), r(0x3c), r(0x48), r(0x70))
                    });
                    let levels = guest_build::h2_static_handle(&rd, h2.char_slot)
                        .and_then(|ch| guest_build::h2_fields(&rd, ch))
                        .and_then(|fb| guest_build::h2_rd_u32(&rd, fb + 4 + h2.char_skill_levels_off))
                        .and_then(|arr| guest_build::h2_byte_array(&rd, arr))
                        .map(|v| v.iter().map(|b| *b as i8).collect::<Vec<i8>>());
                    eprintln!(
                        "[h2-ilpung] f={f} TARGET={:#x} at ({},{}) LAST={} | player {me:?} (tile x,y . facing . t . w . SP) | a.p {levels:?} | 암살 bonus x{} last {}",
                        w(IP::TARGET), w(IP::X), w(IP::Y), w(IP::LAST), w(IP::AMSAL_COUNT), w(IP::AMSAL_LAST)
                    );
                }
            }
            if let Some((block, _)) = acc_block {
                type AP = guest_build::AccessoryPatch;
                let h2 = GuestBuild::Hero2Ktf.hero2_java().unwrap();
                for &(_, t_, i_) in h2_acc_relabel.iter().filter(|(at, ..)| *at == f) {
                    let rd = |a: u32, n: usize| emu.read_memory(a, n);
                    let rec = guest_build::h2_static_handle(&rd, h2.char_slot)
                        .and_then(|ch| guest_build::h2_fields(&rd, ch))
                        .and_then(|fb| guest_build::h2_rd_u32(&rd, fb + 4 + guest_build::Hero2Java::CHAR_EQUIP_OFF))
                        .and_then(|eq| guest_build::h2_rd_u32(&rd, eq))
                        .and_then(|data| guest_build::h2_rd_u32(&rd, data + 8 + 4 * AP::SLOT as u32))
                        .and_then(|rec| guest_build::h2_fields(&rd, rec));
                    match rec {
                        Some(rfb) => {
                            // DIAGNOSTIC writes (never an app write): ch.g / ch.h of the worn accessory.
                            let ok = emu.write_memory(rfb + 4, &u32::from(t_).to_le_bytes()) && emu.write_memory(rfb + 8, &u32::from(i_).to_le_bytes());
                            eprintln!("[h2-acc] f={f} worn slot 2 relabelled {t_}:{i_} (diagnostic) ok={ok}");
                        }
                        None => eprintln!("[h2-acc] f={f} relabel: no record in equipment slot 2"),
                    }
                }
                for &(_, x0, y0, x1, y1) in h2_mob_place.iter().filter(|(at, ..)| *at == f) {
                    let rd = |a: u32, n: usize| emu.read_memory(a, n);
                    let stage_slot = GuestBuild::Hero2Ktf.accessory_patch().map(|p_| p_.stage_slot).unwrap_or(0);
                    let mob = guest_build::h2_static_handle(&rd, stage_slot)
                        .and_then(|s| guest_build::h2_fields(&rd, s))
                        .and_then(|sfb| guest_build::h2_rd_u32(&rd, sfb + 4 + AP::STAGE_GRID_OFF))
                        .and_then(|g| guest_build::h2_array(&rd, g))
                        .filter(|(_, rows)| y0 < *rows)
                        .and_then(|(base, _)| guest_build::h2_rd_u32(&rd, base + 4 * y0))
                        .and_then(|row| guest_build::h2_array(&rd, row))
                        .filter(|(_, cols)| x0 < *cols)
                        .and_then(|(base, _)| guest_build::h2_rd_u32(&rd, base + 4 * x0))
                        .filter(|&m| m != 0);
                    match mob {
                        Some(handle) => {
                            // DIAGNOSTIC call (never an app call): the monster's own relocation.
                            let out = Arc::new(GuestCallResult::new());
                            emu.call_java_method(JavaReceiver::Handle { handle, class: "bd" }, "a", "(SS)V", &[JavaArg::Int(x1 * 16), JavaArg::Int(y1 * 16)], out.clone());
                            eprintln!("[h2-mob-place] f={f} ({x0},{y0}) {handle:#x} -> ({x1},{y1}) (diagnostic)");
                        }
                        None => eprintln!("[h2-mob-place] f={f} no entity on ({x0},{y0})"),
                    }
                }
                h2_mob_hurt_calls.retain(|(at, handle, out)| match out.poll() {
                    None => true,
                    Some(r) => {
                        eprintln!("[h2-mob-hurt] f={f} (asked at {at}) {handle:#x} -> {r:?}");
                        false
                    }
                });
                // ★ 2026-10-06: the monster on grid tile (x,y) takes `dmg` through its own `bd.a(IB)V(dmg, 0)` — the
                //   path every hit and the poison tick take, so a kill falls, pays and leaves the grid like a real one
                //   (a raw HP write would not). DIAGNOSTIC: a boss fight with some of its parts already gone.
                for &(_, x, y, dmg) in h2_mob_hurt.iter().filter(|(at, ..)| *at == f) {
                    let rd = |a: u32, n: usize| emu.read_memory(a, n);
                    let stage_slot = GuestBuild::Hero2Ktf.accessory_patch().map(|p_| p_.stage_slot).unwrap_or(0);
                    let mob = guest_build::h2_static_handle(&rd, stage_slot)
                        .and_then(|s| guest_build::h2_fields(&rd, s))
                        .and_then(|sfb| guest_build::h2_rd_u32(&rd, sfb + 4 + AP::STAGE_GRID_OFF))
                        .and_then(|g| guest_build::h2_array(&rd, g))
                        .filter(|(_, rows)| y < *rows)
                        .and_then(|(base, _)| guest_build::h2_rd_u32(&rd, base + 4 * y))
                        .and_then(|row| guest_build::h2_array(&rd, row))
                        .filter(|(_, cols)| x < *cols)
                        .and_then(|(base, _)| guest_build::h2_rd_u32(&rd, base + 4 * x))
                        .filter(|&m| m != 0);
                    match mob {
                        Some(handle) => {
                            // the receiver must be named by its LIVE class (the call refuses a mismatch): a boss part is
                            // `al` / `ca` / `ci` / `m`, not `bd`
                            let class = match guest_build::h2_class_desc(&rd, handle) {
                                Some(0x0019_23e8) => "al",
                                Some(0x0019_29ac) => "ca",
                                Some(0x0019_2acc) => "ci",
                                Some(0x0019_3090) => "m",
                                _ => "bd",
                            };
                            let out = Arc::new(GuestCallResult::new());
                            emu.call_java_method(JavaReceiver::Handle { handle, class }, "a", "(IB)V", &[JavaArg::Int(dmg), JavaArg::Int(0)], out.clone());
                            eprintln!("[h2-mob-hurt] f={f} ({x},{y}) {handle:#x} takes {dmg} (diagnostic)");
                            h2_mob_hurt_calls.push((f, handle, out));
                        }
                        None => eprintln!("[h2-mob-hurt] f={f} no entity on ({x},{y})"),
                    }
                }
                // ★★ 2026-10-16: the refund the app asks before a discard — report, call, report a frame later
                for &(_, idx) in h2_beast_refund.iter().filter(|(at, _)| *at == f) {
                    let rd = |a: u32, n: usize| emu.read_memory(a, n);
                    if let Some(lines) = guest_build::h2_reset_report(&rd, &h2) {
                        for l in &lines {
                            eprintln!("[h2-beast-refund] f={f} before: {l}");
                        }
                    }
                    match guest_build::h2_beast_list(&rd, &h2).and_then(|l| l.get(idx as usize).copied()) {
                        Some(handle) => {
                            let spec = guest_build::JavaCall::BeastRefund.spec();
                            emu.call_java_method(JavaReceiver::Handle { handle, class: spec.class }, spec.method, spec.descriptor, &[], Arc::new(GuestCallResult::new()));
                            eprintln!("[h2-beast-refund] f={f} beast #{idx} {handle:#x}: {}.{}{} asked", spec.class, spec.method, spec.descriptor);
                            h2_beast_refund_report.push(f + 1);
                        }
                        None => eprintln!("[h2-beast-refund] f={f} no owned beast #{idx}"),
                    }
                }
                if h2_beast_refund_report.iter().any(|at| *at == f) {
                    let rd = |a: u32, n: usize| emu.read_memory(a, n);
                    if let Some(lines) = guest_build::h2_reset_report(&rd, &h2) {
                        for l in &lines {
                            eprintln!("[h2-beast-refund] f={f} after: {l}");
                        }
                    }
                }
                for &(_, slot, ty, lv) in h2_beast_skill.iter().filter(|(at, ..)| *at == f) {
                    let rd = |a: u32, n: usize| emu.read_memory(a, n);
                    let arrays = guest_build::h2_static_handle(&rd, h2.char_slot)
                        .and_then(|ch| guest_build::h2_fields(&rd, ch))
                        .and_then(|fb| guest_build::h2_rd_u32(&rd, fb + 4 + h2.beast_off))
                        .and_then(|b| guest_build::h2_fields(&rd, b))
                        .and_then(|bfb| {
                            let j = guest_build::h2_rd_u32(&rd, bfb + 4 + h2.beast_skill_ids_off).and_then(|h| guest_build::h2_array(&rd, h))?;
                            let k = guest_build::h2_rd_u32(&rd, bfb + 4 + AP::BEAST_LEVELS_OFF).and_then(|h| guest_build::h2_array(&rd, h))?;
                            (j.1 > u32::from(slot) && k.1 > u32::from(slot)).then_some((j.0, k.0))
                        });
                    let beast = guest_build::h2_static_handle(&rd, h2.char_slot)
                        .and_then(|ch| guest_build::h2_fields(&rd, ch))
                        .and_then(|fb| guest_build::h2_rd_u32(&rd, fb + 4 + h2.beast_off));
                    match arrays {
                        Some((j, k)) => {
                            // DIAGNOSTIC writes (never an app write): bn.j[slot] / bn.k[slot].
                            let ok = emu.write_memory(j + u32::from(slot), &[ty as u8]) && emu.write_memory(k + u32::from(slot), &[lv as u8]);
                            // ★ 2026-10-06: and the game's own refresh of the slots' cooldowns and SP costs (bn.h()V → bn.a:[S ·
                            //   bn.i()V → bn.b:[S, what bn.b(Z)V does on a load) — so [h2-beast-cost] shows the new kind's numbers.
                            if let (true, Some(handle)) = (ok, beast) {
                                for m in ["h", "i"] {
                                    emu.call_java_method(JavaReceiver::Handle { handle, class: "bn" }, m, "()V", &[], Arc::new(GuestCallResult::new()));
                                }
                            }
                            eprintln!("[h2-beast] f={f} slot {slot} <- type {ty} level {lv} (diagnostic) ok={ok} — bn.h()V · bn.i()V asked");
                        }
                        None => eprintln!("[h2-beast] f={f} no beast skill arrays"),
                    }
                }
                for &(_, off, val) in h2_acc_word.iter().filter(|(at, ..)| *at == f) {
                    let ok = emu.write_memory(block + off, &val.to_le_bytes());
                    eprintln!("[h2-acc] f={f} data +{off:#x} <- {val} (diagnostic) ok={ok}");
                }
                for &(_, code) in h2_acc.iter().filter(|(at, _)| *at == f) {
                    let ok = emu.write_memory(block + AP::ACTIVE, &code.to_le_bytes());
                    eprintln!("[h2-acc] f={f} ACTIVE <- {code} (forced) ok={ok}");
                }
                if h2_acc_auto && f % 5 == 0 {
                    let rd = |a: u32, n: usize| emu.read_memory(a, n);
                    let class = guest_build::h2_rd_u32(&rd, h2.class_slot).unwrap_or(0) as u8;
                    let code = guest_build::h2_worn_accessory(&rd, h2.char_slot).map_or(AP::NONE, |(t_, i_)| AP::code_for(t_, i_, class));
                    let stage = guest_build::h2_rd_u32(&rd, GuestBuild::Hero2Ktf.accessory_patch().unwrap().stage_slot).unwrap_or(0);
                    let now = guest_build::h2_rd_u32(&rd, block + AP::ACTIVE).unwrap_or(0);
                    // ★★ 2026-10-14 번개의 호흡 (the app's `Accessory::poll`): 벽력일섬 + one in the weapon or the off-hand slot
                    let blades: Vec<(usize, u8, u8)> = AP::BOLT_SLOTS
                        .iter()
                        .filter_map(|&s_| guest_build::h2_worn_slot(&rd, h2.char_slot, s_).map(|(t_, i_)| (s_, t_, i_)))
                        .collect();
                    let bolt = u32::from(AP::bolt_set(code, &blades));
                    let bolt_now = guest_build::h2_rd_u32(&rd, block + AP::BOLT_SET).unwrap_or(0);
                    // ★★ 2026-10-15 피의 연쇄 (the app's `Accessory::poll`): 극사의 목걸이 + one in either weapon slot
                    let chain_sp = AP::chain_sp(code, &blades);
                    let chain_now = guest_build::h2_rd_u32(&rd, block + AP::GK_SP).unwrap_or(0);
                    if now != code {
                        emu.write_memory(block + AP::ACTIVE, &code.to_le_bytes());
                        eprintln!("[h2-acc] f={f} ACTIVE <- {code} (detected, class {class})");
                    }
                    if bolt_now != bolt {
                        emu.write_memory(block + AP::BOLT_SET, &bolt.to_le_bytes());
                        eprintln!("[h2-acc] f={f} BOLT_SET <- {bolt} (번개의 호흡)");
                    }
                    if chain_now != chain_sp {
                        emu.write_memory(block + AP::GK_SP, &chain_sp.to_le_bytes());
                        eprintln!("[h2-acc] f={f} GK_SP <- {chain_sp} (피의 연쇄)");
                    }
                    if stage != acc_stage {
                        if acc_stage != 0 {
                            emu.write_memory(block + AP::CHANNEL, &0u32.to_le_bytes());
                        }
                        acc_stage = stage;
                    }
                }
                // ── the app's skill-name swap (`Accessory::poll_names`), every 5 frames: 일격 → 일섬 ·
                //    급소가격 → 극사 while ACTIVE says so and the menu's `bw.c` table is up ──
                if f % 5 == 0 {
                    let patch = guest_build::GuestBuild::Hero2Ktf.accessory_patch().unwrap();
                    let (active, table) = {
                        let rd = |a: u32, n: usize| emu.read_memory(a, n);
                        // ★★ 2026-10-17 벽력일섬 + 번개의 호흡 → 「벽력일섬」's rows (the app's `AccessoryPatch::name_key`)
                        let bolt = guest_build::h2_rd_u32(&rd, block + AP::BOLT_SET).unwrap_or(0) != 0;
                        (AP::name_key(guest_build::h2_rd_u32(&rd, block + AP::ACTIVE).unwrap_or(0), bolt), guest_build::h2_skill_table(&rd, &patch).unwrap_or(0))
                    };
                    let wanted = AP::SKILL_NAME_SWAPS.iter().any(|s| s.0 == active);
                    if let Some((t, code, slots)) = acc_names.take() {
                        if t == table && code == active {
                            acc_names = Some((t, code, slots));
                        } else {
                            for (addr, original) in &slots {
                                if t == table {
                                    emu.write_memory(*addr, &original.to_le_bytes());
                                }
                                emu.release_java_object(*original);
                            }
                            if t == table {
                                let rd = |a: u32, n: usize| emu.read_memory(a, n);
                                for (addr, bytes) in guest_build::h2_skill_name_repaint_writes(&rd, &h2) {
                                    emu.write_memory(addr, &bytes);
                                }
                            }
                            eprintln!("[h2-acc] f={f} skill names: {} (code {code})", if t == table { "put back" } else { "the table is gone — pins released" });
                        }
                    }
                    if acc_names.is_none() && table != 0 && wanted {
                        let plan = {
                            let rd = |a: u32, n: usize| emu.read_memory(a, n);
                            guest_build::h2_skill_name_swaps(&rd, &patch, table, active)
                        };
                        if let Some(plan) = plan {
                            let mut done = vec![];
                            for (addr, original, replacement) in plan {
                                if emu.pin_java_object(original) && emu.write_memory(addr, &replacement.to_le_bytes()) {
                                    done.push((addr, original));
                                }
                            }
                            eprintln!("[h2-acc] f={f} skill names: {} row(s) swapped (code {active}, table {table:#x})", done.len());
                            if !done.is_empty() {
                                let rd = |a: u32, n: usize| emu.read_memory(a, n);
                                for (addr, bytes) in guest_build::h2_skill_name_repaint_writes(&rd, &h2) {
                                    emu.write_memory(addr, &bytes);
                                }
                                acc_names = Some((table, active, done));
                            }
                        }
                    }
                }
                // the gauge: the data words and the player, on every change
                let rd = |a: u32, n: usize| emu.read_memory(a, n);
                let words: Vec<u32> = (0..15).map(|k| guest_build::h2_rd_u32(&rd, block + 4 * k).unwrap_or(0)).collect();
                let pl = guest_build::h2_static_handle(&rd, h2.char_slot).and_then(|ch| guest_build::h2_fields(&rd, ch)).map(|fb| {
                    let w = |o: u32| guest_build::h2_rd_u32(&rd, fb + 4 + o).unwrap_or(0) as i32;
                    (w(AP::TILE_X_OFF), w(AP::TILE_Y_OFF), w(AP::FACING_OFF), w(AP::STATE_OFF), w(AP::FRAME_OFF), w(AP::SP_OFF), w(AP::X_OFF), w(AP::Y_OFF))
                });
                let mut seen: Vec<u32> = words.clone();
                if let Some(p_) = pl {
                    seen.extend([p_.0 as u32, p_.1 as u32, p_.2 as u32, p_.3 as u32]);
                }
                if seen != acc_seen {
                    eprintln!(
                        "[h2-acc] f={f} ACTIVE={} CHANNEL={} CTICK={} PULSE={} MFRAME={} ILSEOM_HIT={} GK={:#x}@({},{}) TRAIL={}x{} col {} dir {} at ({},{}) | player {:?} (tile x,y . facing . t . w . SP . px x,y)",
                        words[0], words[1], words[2], words[3], words[4], words[5], words[6], words[7] as i32, words[8] as i32, words[9], words[10], words[11] as i32, words[12], words[13] as i32, words[14] as i32, pl
                    );
                    acc_seen = seen;
                }
                // ★★ 2026-10-14 번개의 호흡: the draws left · the hits recorded · the camera the player's draw caught, on every change
                let bolt: Vec<u32> = [AP::BOLT_SET, AP::BOLT_LEFT, AP::BOLT_N, AP::BOLT_CAMX, AP::BOLT_CAMY]
                    .iter()
                    .map(|&o| guest_build::h2_rd_u32(&rd, block + o).unwrap_or(0))
                    .collect();
                if bolt != acc_bolt_seen {
                    eprintln!("[h2-bolt] f={f} SET={} LEFT={} N={} CAM=({},{})", bolt[0], bolt[1], bolt[2], bolt[3] as i32, bolt[4] as i32);
                    acc_bolt_seen = bolt;
                }
                // ★ 야수조련's range: the beast (tile · casting slot · its four skill types · levels)
                //   and every effect `ce` on the stage list as (x,y)tier, on every change
                let beast = guest_build::h2_static_handle(&rd, h2.char_slot)
                    .and_then(|ch| guest_build::h2_fields(&rd, ch))
                    .and_then(|fb| guest_build::h2_rd_u32(&rd, fb + 4 + h2.beast_off))
                    .and_then(|b| guest_build::h2_fields(&rd, b))
                    .map(|bfb| {
                        let w = |o: u32| guest_build::h2_rd_u32(&rd, bfb + 4 + o).unwrap_or(0) as i32;
                        let arr = |o: u32| {
                            guest_build::h2_rd_u32(&rd, bfb + 4 + o)
                                .and_then(|h| guest_build::h2_byte_array(&rd, h))
                                .map(|v| v.iter().map(|&b| b as i8).collect::<Vec<i8>>())
                                .unwrap_or_default()
                        };
                        (w(AP::TILE_X_OFF), w(AP::TILE_Y_OFF), w(AP::BEAST_SKILL_OFF), arr(h2.beast_skill_ids_off), arr(AP::BEAST_LEVELS_OFF))
                    });
                const CE_DESC: u32 = 0x0019_2a3c;
                let stage_slot = GuestBuild::Hero2Ktf.accessory_patch().map(|p_| p_.stage_slot).unwrap_or(0);
                let mut effects: Vec<(i32, i32, i32)> = vec![];
                let mut e = guest_build::h2_static_handle(&rd, stage_slot)
                    .and_then(|s| guest_build::h2_fields(&rd, s))
                    .and_then(|sfb| guest_build::h2_rd_u32(&rd, sfb + 4 + AP::STAGE_HEAD_OFF))
                    .unwrap_or(0);
                for _ in 0..400 {
                    let Some(efb) = guest_build::h2_fields(&rd, e) else { break };
                    if guest_build::h2_class_desc(&rd, e) == Some(CE_DESC) {
                        let w = |o: u32| guest_build::h2_rd_u32(&rd, efb + 4 + o).unwrap_or(0) as i32;
                        effects.push((w(AP::TILE_X_OFF), w(AP::TILE_Y_OFF), w(0x34)));
                    }
                    e = guest_build::h2_rd_u32(&rd, efb + 4 + AP::NEXT_OFF).unwrap_or(0);
                }
                effects.sort();
                let line = format!(
                    "beast {:?} (tile x,y . slot . types . levels) | {} effect(s) {}",
                    beast,
                    effects.len(),
                    effects.iter().map(|(x, y, t_)| format!("({x},{y}){t_}")).collect::<Vec<_>>().join(" ")
                );
                if line != acc_beast_seen {
                    eprintln!("[h2-beast] f={f} {line}");
                    acc_beast_seen = line;
                }
            }
            // ── 2026-09-30: --h2-shop <f> — the shop, opened by the calls its script event makes
            //    (ag.g()V case 7: ag.h(7) · v.a()Lv; · v.a()V; the init goes first here so no frame
            //    paints state 7 before the menu exists), then the gold on every change ──────────
            if h2_refine_test.is_some() { refine_probe.poll(&mut *emu, f, h2_refine_test, &out); }
            // ── ★★ 2026-10-16: --h2-tools — the app's 정렬 / 일괄판매 driver, its taps, what it did ──────────────────────
            if h2_tools {
                for (_, x, y) in h2_tools_tap.iter().filter(|(tf, _, _)| *tf == f) {
                    // What a bulk sale of each grade would take right now — read-only, before the tap acts.
                    {
                        let rd = |a: u32, n: usize| emu.read_memory(a, n);
                        if let (Some(h2), Some(s)) = (build().hero2_java(), guest_build::h2_sell_window(&rd)) {
                            for g in 0..guest_build::BulkSell::GRADES {
                                if let Some(p) = guest_build::h2_bulk_sell_plan(&rd, &h2, &s, g) {
                                    let items: Vec<String> = p.sold.iter().map(|r| format!("[{}] {}/{} x{} @{}", r.index, r.kind, r.id, r.count, r.unit)).collect();
                                    eprintln!("[h2-tools] f={f} plan grade {g}: {} record(s), gold {} — {}", p.sold.len(), p.gold, items.join(" · "));
                                }
                            }
                        }
                    }
                    let took = bag_tools.tap(&mut *emu, &mut ProbeBagIo(build().hero2_java()), *x, *y);
                    eprintln!("[h2-tools] f={f} tap ({x}, {y}) {}", if took { "taken by the tools" } else { "not the tools'" });
                }
                if let Some(p) = bag_tools.poll(&mut *emu, &mut ProbeBagIo(build().hero2_java())) {
                    let rects: Vec<_> = p.iter().map(|q| (q.x, q.y, q.w, q.h)).collect();
                    eprintln!("[h2-tools] f={f} buttons {rects:?}");
                    tools_patches = p;
                }
                if (bag_tools.sorts, bag_tools.last_sale) != tools_seen {
                    tools_seen = (bag_tools.sorts, bag_tools.last_sale);
                    let recs = {
                        let rd = |a: u32, n: usize| emu.read_memory(a, n);
                        build().hero2_java().and_then(|h2| guest_build::h2_static_handle(&rd, h2.inventory_slot)).and_then(|bag| guest_build::h2_bag_sort_recs(&rd, bag))
                    };
                    let gold: Option<i32> = emu.read_memory(guest_build::ShopSellPatch::GOLD_SLOT, 4).try_into().map(i32::from_le_bytes).ok();
                    eprintln!("[h2-tools] f={f} sorts {} · last sale {:?} · gold {gold:?}", bag_tools.sorts, bag_tools.last_sale);
                    for (i, r) in recs.unwrap_or_default().iter().enumerate() {
                        eprintln!("[h2-tools]   [{i:3}] type {:2} id {:3} x{:<3} LV {:2} grade {} refine {} serial {}{}", r.kind, r.id, r.count, r.level, r.grade, r.refine, r.serial, if r.worn { " WORN" } else { "" });
                    }
                }
            }
            // ── ★★ 2026-10-21: --h2-park <f:0|1> — the app's world-map hold: the guest's tasks parked, the host's running ──
            for (_, on) in h2_park.iter().filter(|(pf, _)| *pf == f) {
                emu.park_guest(*on);
                eprintln!("[h2-park] f={f} guest tasks {}", if *on { "PARKED — the game stands, the music player runs on" } else { "released" });
            }
            // ── ★★ 2026-10-21: --h2-flight <f[:town]> — the app's 비행서 flow on the bag popup under the cursor ──────────
            if let Some((_, town)) = h2_flight.iter().find(|(ff, _)| *ff == f)
                && flight_step.is_none()
            {
                let rd = |a: u32, n: usize| emu.read_memory(a, n);
                let ctx = page_block.and_then(|b| guest_build::h2_page_ctx(&rd, b)).unwrap_or((0, 0));
                let bags: Vec<u32> = [guest_build::h2_static_handle(&rd, h2.inventory_slot), guest_build::h2_static_handle(&rd, h2.material_bag_slot), account_bag].into_iter().flatten().collect();
                match guest_build::h2_bag_popup_walk(&rd, &h2, &bags, ctx) {
                    Some(p) if guest_build::h2_is_flight(&p) => {
                        let allowed = guest_build::h2_flight_allowed(&rd);
                        let towns = guest_build::h2_flight_destinations(&rd);
                        let picker = guest_build::h2_flight_picker(&rd, &h2, &p);
                        let move_row = guest_build::h2_gm_row_handle(&rd, &h2, guest_build::h2_move_row_for(&p, &[account_bag, None]))
                            .and_then(|h| p.labels.iter().position(|&l| l == h));
                        let in_warehouse = Some(p.window.bag) == account_bag;
                        eprintln!(
                            "[h2-flight] f={f} popup {:#x} on 비행서 [{}] ×{} of bag {:#x}{} — rows {} labels {} · allowed {allowed} · visited {towns:?} · picker {picker:?} · move row {move_row:?}",
                            p.handle, p.index, p.record.count, p.window.bag, if in_warehouse { " (the WAREHOUSE)" } else { "" }, p.rows, p.labels_len
                        );
                        match town {
                            None => match guest_build::h2_flight_picker_writes(&rd, &h2, &p) {
                                Some(writes) => {
                                    let mut ok = true;
                                    for (a, b) in &writes {
                                        ok &= emu.write_memory(*a, b);
                                    }
                                    eprintln!("[h2-flight] f={f} town list written: {} row(s) ({} write(s) ok={ok})", towns.len(), writes.len());
                                }
                                None => eprintln!(
                                    "[h2-flight] f={f} no town list — {}",
                                    if towns.is_empty() { "nothing visited (the app says 방문한 마을이 없습니다.)" } else if picker.is_some() { "already there" } else { "no room in the label array" }
                                ),
                            },
                            Some(t) => {
                                if !allowed || !towns.contains(t) {
                                    eprintln!("[h2-flight] f={f} town {t} REFUSED (allowed {allowed}, visited {towns:?})");
                                } else if let Some(out) = h2_call(&mut *emu, guest_build::JavaCall::BagConsumeAt, Some(p.window.bag), &[JavaArg::Int(p.index as i32), JavaArg::Int(1)]) {
                                    eprintln!("[h2-flight] f={f} consuming record [{}] of bag {:#x} — then the warp to town {t} {:?}", p.index, p.window.bag, guest_build::H2_FLIGHT_TOWNS[*t]);
                                    flight_step = Some((out, 0, p.menu_fields, *t, p.window.bag));
                                }
                            }
                        }
                    }
                    other => eprintln!("[h2-flight] f={f} no bag popup on a 비행서 ({:?})", other.map(|p| (p.record.kind, p.record.id))),
                }
            }
            if let Some((out, stage, menu_fields, town, bag)) = flight_step.take() {
                match out.poll() {
                    None => flight_step = Some((out, stage, menu_fields, town, bag)),
                    Some(Err(())) => eprintln!("[h2-flight] f={f} stage {stage} FAILED"),
                    Some(Ok(_)) if stage == 0 => {
                        let left = {
                            let rd = |a: u32, n: usize| emu.read_memory(a, n);
                            guest_build::h2_bag_head_of(&rd, bag).map(|h| h.count)
                        };
                        // ★★ 2026-10-21: the app's exit — state <- 3, then the game's own menu release bw.a(Z)V, THEN the warp
                        //   (stage 2); `--h2-menu-release 0` = the old exit (child <- 0 · state <- 3) and the warp at once.
                        let state = emu.write_memory(0x0019_3aec, &3u32.to_le_bytes());
                        let release = if h2_menu_release { h2_call(&mut *emu, guest_build::JavaCall::MenuRelease, None, &[JavaArg::Int(1)]) } else { None };
                        match release {
                            Some(o) => {
                                eprintln!("[h2-flight] f={f} consumed (bag {bag:#x} now {left:?} record(s)) · state ← 3 ({state}) · bw.a(Z)V issued");
                                flight_step = Some((o, 2, menu_fields, town, bag));
                            }
                            None => {
                                let child = emu.write_memory(menu_fields + 4 + guest_build::Hero2Java::WIN_CHILD_OFF, &0u32.to_le_bytes());
                                let (map, x, y) = guest_build::H2_FLIGHT_TOWNS[town];
                                match h2_call(&mut *emu, guest_build::JavaCall::FlightWarp, None, &[map, x, y, 2].map(|v| JavaArg::Int(v as i32))) {
                                    Some(o) => {
                                        eprintln!("[h2-flight] f={f} consumed (bag {bag:#x} now {left:?} record(s)) · OLD exit: menu child cleared ({child}) · state ← 3 ({state}) · ag.b({map},{x},{y},2) issued");
                                        flight_step = Some((o, 1, menu_fields, town, bag));
                                    }
                                    None => eprintln!("[h2-flight] f={f} the warp call was REFUSED"),
                                }
                            }
                        }
                    }
                    Some(Ok(_)) if stage == 2 => {
                        let single = {
                            let rd = |a: u32, n: usize| emu.read_memory(a, n);
                            guest_build::h2_static_handle(&rd, 0x0019_422c)
                        };
                        let (map, x, y) = guest_build::H2_FLIGHT_TOWNS[town];
                        match h2_call(&mut *emu, guest_build::JavaCall::FlightWarp, None, &[map, x, y, 2].map(|v| JavaArg::Int(v as i32))) {
                            Some(o) => {
                                eprintln!("[h2-flight] f={f} menu released (bw.a:Lbw; now {single:x?}) · ag.b({map},{x},{y},2) issued");
                                flight_step = Some((o, 1, menu_fields, town, bag));
                            }
                            None => eprintln!("[h2-flight] f={f} the warp call was REFUSED"),
                        }
                    }
                    Some(Ok(_)) => {
                        let rd = |a: u32, n: usize| emu.read_memory(a, n);
                        let map = GuestBuild::Hero2Ktf.hero2_save().and_then(|sv| guest_build::h2_stage_map_no(&rd, &sv.area));
                        eprintln!("[h2-flight] f={f} arrived — stage map {map:?} · state {:?}", guest_build::h2_rd_u32(&rd, 0x0019_3aec));
                    }
                }
            }
            // ── ★★ 2026-10-20: --h2-shop-orb <f> — the app's 상점주인의 혼 flow on the bag popup under the cursor ──────
            if h2_shop_orb.contains(&f) && orb_step.is_none() {
                let rd = |a: u32, n: usize| emu.read_memory(a, n);
                // The same bag list and page context `--h2-moverow` walks with (they are built further down the loop).
                let orb_ctx = page_block.and_then(|b| guest_build::h2_page_ctx(&rd, b)).unwrap_or((0, 0));
                let orb_bags: Vec<u32> = [guest_build::h2_static_handle(&rd, h2.inventory_slot), guest_build::h2_static_handle(&rd, h2.material_bag_slot), account_bag].into_iter().flatten().collect();
                match guest_build::h2_bag_popup_walk(&rd, &h2, &orb_bags, orb_ctx) {
                    Some(p) if guest_build::h2_oblivion_of(p.record.kind, p.record.id) == Some(guest_build::H2Oblivion::Shop) => {
                        let stock = guest_build::h2_static_handle(&rd, guest_build::Hero2Java::SHOP_STOCK_SLOT);
                        let row = guest_build::h2_oblivion_row(&rd, &h2, &p);
                        let in_inventory = guest_build::h2_static_handle(&rd, h2.inventory_slot) == Some(p.window.bag);
                        eprintln!("[h2-shop-orb] f={f} popup {:#x} on 상점주인의 혼 [{}] — host row {row:?}, stock {stock:x?}, in the inventory {in_inventory}", p.handle, p.index);
                        // The app's gate (user: 「계정창고에서 사용은 안되어야해」): the INVENTORY's popup only.
                        if !in_inventory {
                            eprintln!("[h2-shop-orb] f={f} REFUSED — the popup is on a warehouse bag, not the inventory");
                        } else if stock.is_some() {
                            orb_step = Some((Arc::new(GuestCallResult::new()), 1, p.menu_fields));
                        } else if let Some(out) = h2_call(&mut *emu, guest_build::JavaCall::ShopStockLoad, None, &[]) {
                            eprintln!("[h2-shop-orb] f={f} z.a() issued (the stock loader)");
                            orb_step = Some((out, 0, p.menu_fields));
                        }
                    }
                    other => eprintln!("[h2-shop-orb] f={f} no bag popup on the shop orb ({:?})", other.map(|p| (p.record.kind, p.record.id))),
                }
            }
            // ── ★★ 2026-10-21: --h2-shop-plate <f> — the app's use_shop_plate: the LIVE inventory's (21, 8), no menu ──
            if h2_shop_plate.contains(&f) && orb_step.is_none() {
                let rd = |a: u32, n: usize| emu.read_memory(a, n);
                let field = guest_build::h2_rd_u32(&rd, 0x0019_3aec) == Some(3);
                let have = guest_build::h2_inventory(&rd, &h2).and_then(|inv| guest_build::h2_item_stock(&inv, guest_build::H2_PLATE_SHOP_ORB));
                let stock = guest_build::h2_static_handle(&rd, guest_build::Hero2Java::SHOP_STOCK_SLOT);
                eprintln!("[h2-shop-plate] f={f} field {field} · inventory stock {have:?} · shop stock {stock:x?}");
                if !field {
                    eprintln!("[h2-shop-plate] f={f} REFUSED — not on the field");
                } else if !have.is_some_and(|(_, n)| n > 0) {
                    eprintln!("[h2-shop-plate] f={f} REFUSED — no 상점주인의 혼 in the inventory (the app says 아이템없음)");
                } else if stock.is_some() {
                    orb_step = Some((Arc::new(GuestCallResult::new()), 1, 0));
                } else if let Some(out) = h2_call(&mut *emu, guest_build::JavaCall::ShopStockLoad, None, &[]) {
                    eprintln!("[h2-shop-plate] f={f} z.a() issued (the stock loader)");
                    orb_step = Some((out, 0, 0));
                }
            }
            if let Some((out, stage, menu_fields)) = orb_step.take() {
                let post = |emu: &mut dyn Emulator, f: u64| -> Option<Arc<GuestCallResult>> {
                    let (d, e) = guest_build::Hero2Java::SHOP_EVENT;
                    let out = h2_call(emu, guest_build::JavaCall::EventPost, None, &[JavaArg::Int(d), JavaArg::Int(e)]);
                    eprintln!("[h2-shop-orb] f={f} event ({d}, {e}) {}", if out.is_some() { "posted" } else { "REFUSED" });
                    out
                };
                // ★★ 2026-10-21: the app's open_shop — menu_fields 0 = a PLATE (no menu); otherwise state <- 3 and the game's own
                //   release bw.a(Z)V first (stage 3 → the post), or with --h2-menu-release 0 the old child <- 0 · state <- 3.
                //   Returns `(call, stage)` to wait on.
                let open = |emu: &mut dyn Emulator, f: u64, menu_fields: u32| -> Option<(Arc<GuestCallResult>, u8)> {
                    if menu_fields == 0 {
                        return post(emu, f).map(|o| (o, 2));
                    }
                    let state = emu.write_memory(0x0019_3aec, &3u32.to_le_bytes());
                    if h2_menu_release && let Some(o) = h2_call(emu, guest_build::JavaCall::MenuRelease, None, &[JavaArg::Int(1)]) {
                        eprintln!("[h2-shop-orb] f={f} state ← 3 ({state}) · bw.a(Z)V issued");
                        return Some((o, 3));
                    }
                    let child = emu.write_memory(menu_fields + 4 + guest_build::Hero2Java::WIN_CHILD_OFF, &0u32.to_le_bytes());
                    eprintln!("[h2-shop-orb] f={f} OLD exit: menu child cleared ({child}) · state ← 3 ({state})");
                    post(emu, f).map(|o| (o, 2))
                };
                match stage {
                    // stage 1 = the stock was already there: open now, then wait for the post to land.
                    1 => {
                        if let Some((o, s)) = open(&mut *emu, f, menu_fields) {
                            orb_step = Some((o, s, menu_fields));
                            orb_watch = Some((f, None));
                        }
                    }
                    _ => match out.poll() {
                        None => orb_step = Some((out, stage, menu_fields)),
                        Some(Err(())) => eprintln!("[h2-shop-orb] f={f} stage {stage} FAILED"),
                        Some(Ok(v)) if stage == 0 => {
                            if v == 0 {
                                eprintln!("[h2-shop-orb] f={f} z.a() returned null — no stock");
                            } else {
                                let ok = emu.write_memory(guest_build::Hero2Java::SHOP_STOCK_SLOT, &v.to_le_bytes());
                                emu.release_java_object(v);
                                eprintln!("[h2-shop-orb] f={f} stock {v:#x} → v.a (ok={ok})");
                                if ok {
                                    if let Some((o, s)) = open(&mut *emu, f, menu_fields) {
                                        orb_step = Some((o, s, menu_fields));
                                        orb_watch = Some((f, None));
                                    }
                                }
                            }
                        }
                        Some(Ok(_)) if stage == 3 => {
                            let single = {
                                let rd = |a: u32, n: usize| emu.read_memory(a, n);
                                guest_build::h2_static_handle(&rd, 0x0019_422c)
                            };
                            eprintln!("[h2-shop-orb] f={f} menu released (bw.a:Lbw; now {single:x?})");
                            if let Some(o) = post(&mut *emu, f) {
                                orb_step = Some((o, 2, menu_fields));
                            }
                        }
                        Some(Ok(_)) => eprintln!("[h2-shop-orb] f={f} event post landed"),
                    },
                }
            }
            if let Some((since, last)) = orb_watch {
                let state = match build().app_state() {
                    Some(AppState::Absolute { addr, .. }) => emu.read_memory(addr, 4).try_into().map(i32::from_le_bytes).ok(),
                    _ => None,
                };
                if state != last {
                    eprintln!("[h2-shop-orb] f={f} state {last:?} -> {state:?}{}", if state == Some(7) { " — the shop is open" } else { "" });
                }
                orb_watch = if f >= since + 400 { None } else { Some((since, state)) };
            }
            if h2_shop.contains(&f) && shop_step.is_none() {
                let out = Arc::new(GuestCallResult::new());
                emu.call_java_method(JavaReceiver::Class("v"), "a", "()Lv;", &[], out.clone());
                eprintln!("[h2-shop] f={f} v.a()Lv; issued (the sell block {})", shop_sell_block.map_or("NOT installed — the shipped price / 4".to_string(), |b| format!("at {b:#x}")));
                shop_step = Some((out, 0, f));
            }
            if let Some((out, stage, since)) = shop_step.take() {
                match out.poll() {
                    None => shop_step = Some((out, stage, since)),
                    Some(Err(())) => eprintln!("[h2-shop] f={f} step {stage} FAILED (issued f={since})"),
                    Some(Ok(0)) if stage == 0 => eprintln!("[h2-shop] f={f} v.a()Lv; returned null"),
                    Some(Ok(v)) => match stage {
                        0 => {
                            shop_v = Some(v);
                            let next = Arc::new(GuestCallResult::new());
                            emu.call_java_method(JavaReceiver::Handle { handle: v, class: "v" }, "a", "()V", &[], next.clone());
                            shop_step = Some((next, 1, f));
                        }
                        1 => {
                            if let Some(h) = shop_v.take() {
                                emu.release_java_object(h);
                            }
                            let next = Arc::new(GuestCallResult::new());
                            emu.call_java_method(JavaReceiver::Class("ag"), "h", "(I)V", &[JavaArg::Int(7)], next.clone());
                            shop_step = Some((next, 2, f));
                        }
                        _ => {
                            let state = match build().app_state() {
                                Some(AppState::Absolute { addr, .. }) => emu.read_memory(addr, 4).try_into().map(i32::from_le_bytes).ok(),
                                _ => None,
                            };
                            eprintln!("[h2-shop] f={f} open — state {state:?} (7 = the shop)");
                        }
                    },
                }
            }
            if h2_shop.iter().any(|s| f >= *s) {
                let gold: Option<i32> = emu.read_memory(guest_build::ShopSellPatch::GOLD_SLOT, 4).try_into().map(i32::from_le_bytes).ok();
                if gold != shop_gold {
                    match (shop_gold, gold) {
                        (Some(a), Some(b)) => eprintln!("[h2-gold] f={f} {a} -> {b} ({:+})", b - a),
                        (_, g) => eprintln!("[h2-gold] f={f} gold {g:?}"),
                    }
                    shop_gold = gold;
                }
            }
            // ── ★ 2026-10-09: --h2-alchemist <f> — the alchemist's menu, opened by the calls its script event makes
            //    (ag.g()V: ag.h(8) · aw.a()Law; · aw.a()V; the menu first here, as --h2-shop does) ─────────────────
            if h2_alchemist.contains(&f) && alch_step.is_none() {
                let out = Arc::new(GuestCallResult::new());
                emu.call_java_method(JavaReceiver::Class("aw"), "a", "()Law;", &[], out.clone());
                eprintln!("[h2-alchemist] f={f} aw.a()Law; issued");
                alch_step = Some((out, 0, f));
            }
            if let Some((out, stage, since)) = alch_step.take() {
                match out.poll() {
                    None => alch_step = Some((out, stage, since)),
                    Some(Err(())) => eprintln!("[h2-alchemist] f={f} step {stage} FAILED (issued f={since})"),
                    Some(Ok(0)) if stage == 0 => eprintln!("[h2-alchemist] f={f} aw.a()Law; returned null"),
                    Some(Ok(v)) => match stage {
                        0 => {
                            alch_aw = Some(v);
                            let next = Arc::new(GuestCallResult::new());
                            emu.call_java_method(JavaReceiver::Handle { handle: v, class: "aw" }, "a", "()V", &[], next.clone());
                            alch_step = Some((next, 1, f));
                        }
                        1 => {
                            if let Some(h) = alch_aw.take() {
                                emu.release_java_object(h);
                            }
                            let next = Arc::new(GuestCallResult::new());
                            emu.call_java_method(JavaReceiver::Class("ag"), "h", "(I)V", &[JavaArg::Int(8)], next.clone());
                            alch_step = Some((next, 2, f));
                        }
                        _ => {
                            let state = match build().app_state() {
                                Some(AppState::Absolute { addr, .. }) => emu.read_memory(addr, 4).try_into().map(i32::from_le_bytes).ok(),
                                _ => None,
                            };
                            eprintln!("[h2-alchemist] f={f} open — state {state:?} (8 = an NPC's service window)");
                        }
                    },
                }
            }
            // ── ★ 2026-10-09: --h2-bags127 — the app's standing capacity write (4d4) on both game bags ──
            if h2_bags127 && f % 30 == 3 {
                for slot in [h2.inventory_slot, h2.material_bag_slot] {
                    let head = {
                        let rd = |a: u32, n: usize| emu.read_memory(a, n);
                        guest_build::h2_bag_head(&rd, slot)
                    };
                    if let Some(head) = head
                        && let Some((addr, cap)) = guest_build::h2_bag_capacity_write(&head)
                    {
                        let ok = emu.write_memory(addr, &[cap]);
                        eprintln!("[h2-bags127] f={f} bag {:#x} (slot {slot:#x}) b {} -> {cap} ok={ok} ({} record(s))", head.handle, head.b, head.count);
                    }
                }
            }
            // ── ★ 2026-10-09: the item picker, paged the app's way (BagPaging::poll_picker / picker_go) ──
            {
                let (view, data) = {
                    let rd = |a: u32, n: usize| emu.read_memory(a, n);
                    let view = guest_build::h2_picker_window(&rd, &h2);
                    let data = view.as_ref().map(|v| rd(v.base, v.len as usize));
                    (view, data)
                };
                match (view, data) {
                    (Some(view), Some(data)) if data.len() == view.len as usize => {
                        let same = picker_state.as_ref().is_some_and(|(w, l, _, _, shown)| *w == view.win.handle && *l == view.list && *shown == data);
                        if !same {
                            let pages = guest_build::h2_picker_pages(data.len());
                            let mut st = (view.win.handle, view.list, data.clone(), 0u8, data.clone());
                            if pages > 1 {
                                let first = guest_build::h2_picker_page(&st.2, 0).to_vec();
                                let ok = emu.write_memory(view.base - 4, &(first.len() as u32).to_le_bytes());
                                eprintln!("[h2-picker] f={f} a list of {} entries on {pages} pages — page 1 (length word 28, ok={ok})", st.2.len());
                                st.4 = first;
                            }
                            picker_state = Some(st);
                        }
                        for &(_, page) in h2_picker_page.iter().filter(|p| p.0 == f) {
                            let Some(st) = picker_state.as_mut() else { continue };
                            let now_view = {
                                let rd = |a: u32, n: usize| emu.read_memory(a, n);
                                guest_build::h2_picker_window(&rd, &h2)
                            };
                            match now_view.and_then(|v| guest_build::h2_picker_page_writes(&v, &st.2, page, 0)) {
                                Some(writes) => {
                                    let mut ok = true;
                                    for (_, a, bytes) in &writes {
                                        ok &= emu.write_memory(*a, bytes);
                                    }
                                    st.3 = page;
                                    st.4 = guest_build::h2_picker_page(&st.2, page).to_vec();
                                    eprintln!("[h2-picker] f={f} page {} ({} write(s) ok={ok})", page + 1, writes.len());
                                }
                                None => eprintln!("[h2-picker] f={f} page {} refused (no such page, or no picker)", page + 1),
                            }
                        }
                        let line = {
                            let rd = |a: u32, n: usize| emu.read_memory(a, n);
                            let (st_full, st_page) = picker_state.as_ref().map(|s| (s.2.len(), s.3)).unwrap_or((0, 0));
                            let cursor = rd(view.win.fields + 4 + guest_build::Hero2Java::WIN_CURSOR_OFF, 1).first().copied();
                            let len_now = rd(view.base - 4, 4).try_into().map(u32::from_le_bytes).unwrap_or(0);
                            let now = rd(view.base, len_now as usize);
                            format!(
                                "window {:#x} list {:#x} ({} entries, page {}/{}) shows {:?} cursor {cursor:?} panel {:?} on_top {}",
                                view.win.handle,
                                view.list,
                                st_full,
                                st_page + 1,
                                guest_build::h2_picker_pages(st_full),
                                now,
                                view.panel,
                                view.on_top
                            )
                        };
                        if line != picker_seen {
                            eprintln!("[h2-picker] f={f} {line}");
                            picker_seen = line;
                        }
                    }
                    _ => {
                        if picker_state.take().is_some() {
                            eprintln!("[h2-picker] f={f} the picker is gone");
                            picker_seen.clear();
                        }
                    }
                }
            }
            // ── 2026-09-30: the 의상 아바타 override, the app's own plan (once, from f=30, when asked) ──
            if (!h2_avatar.is_empty() || costume_book.is_some()) && !avatar_tried && f >= 30 {
                avatar_tried = true;
                if let Some(patch) = guest_build::GuestBuild::Hero2Ktf.avatar_patch() {
                    match emu.alloc_guest(guest_build::AvatarPatch::BLOCK_LEN) {
                        Some(block) => {
                            let plan = {
                                let rd = |a: u32, n: usize| emu.read_memory(a, n);
                                guest_build::h2_avatar_install_writes(&rd, &patch, block)
                            };
                            match plan {
                                Some(writes) => {
                                    let mut ok = true;
                                    for (a, bytes) in &writes {
                                        ok &= emu.write_memory(*a, bytes);
                                    }
                                    let rd = |a: u32, n: usize| emu.read_memory(a, n);
                                    let installed = guest_build::h2_avatar_installed(&rd, &patch, block);
                                    eprintln!("[h2-avatar] f={f} block={block:#x} {} write(s) ok={ok} installed={installed}", writes.len());
                                    if installed {
                                        avatar_block = Some(block);
                                        costume_applied = Some(guest_build::AVATAR_PASS_WORDS);
                                    }
                                }
                                None => eprintln!("[h2-avatar] f={f} REFUSED — a region or branch of cf.a(I)V does not read as shipped"),
                            }
                        }
                        None => eprintln!("[h2-avatar] f={f} no guest block"),
                    }
                }
            }
            // ── #20/#21 --h2-kill: the game's own death routine, n times, drops tallied ──
            if let Some((kf, n)) = h2_kill
                && f >= kf
                && !kill_done
            {
                if kill_level_before.is_none() {
                    let rd = |a: u32, n: usize| emu.read_memory(a, n);
                    kill_level_before = char_level(&rd);
                }
                let stage_slot = GuestBuild::Hero2Ktf.hero2_save().map(|s| s.area.stage_slot).unwrap_or(0);
                let ground = |emu: &mut dyn Emulator| {
                    let rd = |a: u32, n: usize| emu.read_memory(a, n);
                    guest_build::h2_ground_drops(&rd, stage_slot)
                };
                match kill_step.take() {
                    None if h2_kill_any => {
                        // ★ 2026-10-16 --h2-kill-any: the first monster on the stage, wherever it stands (a `ca` boss
                        //   teleports; the "faced" lookup and the grid never have it).
                        let mon = {
                            let rd = |a: u32, n: usize| emu.read_memory(a, n);
                            guest_build::h2_monsters(&rd, stage_slot).first().map(|m| {
                                // Its stats as they stand (the raid's LV 110 · HP · attack are what this flag exists to show).
                                let level = GuestBuild::Hero2Ktf.guardian_patch().and_then(|p| guest_build::h2_rd_u32(&rd, m.block + 4 + p.monster_level_off)).map_or(-1, |v| v as i32);
                                (m.handle, level, m.max_hp, m.hp, m.atk, m.template_atk, m.ca)
                            })
                        };
                        match mon {
                            Some((mon, level, max_hp, hp, atk, tatk, ca)) => {
                                eprintln!("[h2-kill] f={f} first monster on the stage {mon:#x} (LV {level} · HP {hp}/{max_hp} · ATK {atk} · template ATK {tatk} · ca {ca}) — {n} x bd.q()V");
                                let before = ground(&mut *emu).map(|g| g.iter().map(|(h, _)| *h).collect()).unwrap_or_default();
                                match h2_call_quiet(&mut *emu, guest_build::JavaCall::MonsterDeath, Some(mon), &[]) {
                                    Some(out) => kill_step = Some(KillStep::Kill { mon, out, before }),
                                    None => kill_done = true,
                                }
                            }
                            None if f < kf + 500 => {}
                            None => {
                                eprintln!("[h2-kill] f={f} no monster on the stage in 500 frames — gave up");
                                kill_done = true;
                            }
                        }
                    }
                    None => match h2_call_quiet(&mut *emu, guest_build::JavaCall::CharFacingMonster, None, &[]) {
                        Some(out) => kill_step = Some(KillStep::Seek { out }),
                        None => kill_done = true,
                    },
                    Some(KillStep::Seek { out }) => match out.poll() {
                        None => kill_step = Some(KillStep::Seek { out }),
                        Some(Ok(mon)) if mon != 0 => {
                            eprintln!("[h2-kill] f={f} facing monster {mon:#x} — {n} x bd.q()V");
                            let before = ground(&mut *emu).map(|g| g.iter().map(|(h, _)| *h).collect()).unwrap_or_default();
                            match h2_call_quiet(&mut *emu, guest_build::JavaCall::MonsterDeath, Some(mon), &[]) {
                                Some(out) => kill_step = Some(KillStep::Kill { mon, out, before }),
                                None => kill_done = true,
                            }
                        }
                        // Nothing in front yet: ask again next frame, for ten seconds.
                        Some(_) if f < kf + 500 => {}
                        Some(_) => {
                            eprintln!("[h2-kill] f={f} no monster faced the character in 500 frames — gave up");
                            kill_done = true;
                        }
                    },
                    Some(KillStep::Kill { mon, out, before }) => match out.poll() {
                        None => kill_step = Some(KillStep::Kill { mon, out, before }),
                        Some(res) => {
                            if res.is_err() {
                                kill_tally.failed += 1;
                            }
                            kill_tally.kills += 1;
                            if let Some(g) = ground(&mut *emu) {
                                for (h, e) in g {
                                    if before.contains(&h) {
                                        continue;
                                    }
                                    if e[2] == guest_build::Hero2Java::GROUND_GOLD {
                                        kill_tally.gold_drops += 1;
                                        kill_tally.gold_total += u64::from(e[3] as u8) * 100 + u64::from(e[4] as u8);
                                    } else {
                                        *kill_tally.items.entry((e[2], e[3])).or_default() += 1;
                                    }
                                }
                            }
                            if kill_tally.kills < n {
                                let before = ground(&mut *emu).map(|g| g.iter().map(|(h, _)| *h).collect()).unwrap_or_default();
                                match h2_call_quiet(&mut *emu, guest_build::JavaCall::MonsterDeath, Some(mon), &[]) {
                                    Some(out) => kill_step = Some(KillStep::Kill { mon, out, before }),
                                    None => kill_done = true,
                                }
                            } else {
                                kill_done = true;
                                emu.release_java_object(mon);
                                let rd = |a: u32, n: usize| emu.read_memory(a, n);
                                print_kill_tally(&kill_tally, kill_level_before, char_level(&rd), h2_shipped);
                                if let Some((_, words)) = drop_block {
                                    match guest_build::h2_drop_rate_words(&rd, words) {
                                        Some((keep, refused, passed)) => eprintln!(
                                            "[h2-kill]   드랍률 filter: keep {keep:#x}/0x10000 · equipment refused {refused} · passed {passed} ({:.1}% kept)",
                                            100.0 * f64::from(passed) / f64::from((refused + passed).max(1))
                                        ),
                                        None => eprintln!("[h2-kill]   드랍률 filter: words unreadable"),
                                    }
                                }
                            }
                        }
                    },
                }
            }
            // ── #17 --h2-skill: the class multiplier and cooldowns, by the game's methods ──
            if h2_skill == Some(f) && skill_probe.is_none() {
                let mut queue = std::collections::VecDeque::new();
                for idx in 0..15 {
                    queue.push_back((guest_build::JavaCall::CharSkillPower, idx));
                }
                for i in 0..8 {
                    queue.push_back((guest_build::JavaCall::CharCooldownTotal, i));
                }
                for idx in 0..15 {
                    queue.push_back((guest_build::JavaCall::CharSkillSpCost, idx));
                }
                eprintln!("[h2-skill] f={f} {} — skill power {}", if h2_shipped { "SHIPPED" } else { "patched" }, skill_block.map_or("not installed".to_string(), |b| format!("via {b:#x}")));
                // 2026-09-27: the derived stats a.m()V left in the character (absolute offsets:
                // a.l:I 치명 0xc0 · a.m:I 회피 0xc8 · a.k:I 0xbc · a.e:I 최대SP 0x7c · a.g:I 공격력 0x9c
                // · a.b:[S 0x98 (힘 체력 민첩 의지 지력) · a.g:S 장비 민첩 0x8c · a.q/a.r/a.s 0x114/0x118/0x120).
                {
                    let rd = |a: u32, n: usize| emu.read_memory(a, n);
                    let rd_u = |a: u32, _n: usize| -> Option<u32> {
                        let b = rd(a, 4);
                        (b.len() == 4).then(|| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
                    };
                    let h2 = GuestBuild::Hero2Ktf.hero2_java().unwrap();
                    if let Some(ch) = guest_build::h2_static_handle(&rd, h2.char_slot).and_then(|o| guest_build::h2_fields(&rd, o)) {
                        let w = |off: u32| rd_u(ch + 4 + off, 4).map_or(-1, |v| v as i32);
                        let stats: Vec<i32> = rd_u(ch + 4 + 0x98, 4)
                            .and_then(|h| guest_build::h2_array(&rd, h))
                            .map(|(base, len)| (0..len.min(5)).map(|i| rd(base + 2 * i, 2)).map(|b| if b.len() == 2 { i32::from(i16::from_le_bytes([b[0], b[1]])) } else { -1 }).collect())
                            .unwrap_or_default();
                        eprintln!(
                            "[h2-skill]   장비 스탯 a.b {stats:?} · 기본 민첩 a.g:S {} · 치명 a.l={} · 회피 a.m={} · a.k={} · 공격력 a.g={} · 최대SP {} · 신의가호 a.q={} 철괴 a.r={} 민첩태세 a.s={}",
                            w(0x8c), w(0xc0), w(0xc8), w(0xbc), w(0x9c), w(0x7c), w(0x114), w(0x118), w(0x120)
                        );
                    }
                    eprintln!("[h2-skill]   combat block {}", combat_block.map_or("not installed".to_string(), |b| format!("via {b:#x}")));
                    eprintln!("[h2-skill]   venom block {}", venom_block.map_or("not installed".to_string(), |b| format!("via {b:#x}")));
                }
                skill_probe = Some(SkillProbe { queue, cur: None });
            }
            if let Some(sp) = skill_probe.as_mut() {
                if let Some((what, arg, out)) = sp.cur.take() {
                    match out.poll() {
                        None => sp.cur = Some((what, arg, out)),
                        Some(r) => {
                            let v = r.map(|x| x as i32);
                            match what {
                                guest_build::JavaCall::CharSkillPower => eprintln!("[h2-skill]   a.p[{arg:2}] a(BILbd;)I(1000) = {v:?}"),
                                guest_build::JavaCall::CharSkillSpCost => eprintln!("[h2-skill]   a.p[{arg:2}] SP cost a(I)I = {v:?}"),
                                _ => eprintln!("[h2-skill]   cooldown a(I)S({arg}) = {:?}", v.map(|x| x as i16)),
                            }
                        }
                    }
                }
                let mut finished = false;
                if sp.cur.is_none() {
                    match sp.queue.pop_front() {
                        Some((what, arg)) => {
                            let args: Vec<JavaArg> = match what {
                                guest_build::JavaCall::CharSkillPower => vec![JavaArg::Int(arg), JavaArg::Int(1000), JavaArg::Object(0)],
                                _ => vec![JavaArg::Int(arg)],
                            };
                            match h2_call_quiet(&mut *emu, what, None, &args) {
                                Some(out) => sp.cur = Some((what, arg, out)),
                                None => eprintln!("[h2-skill]   {what:?}({arg}) refused"),
                            }
                        }
                        None => finished = true,
                    }
                }
                if finished {
                    skill_probe = None;
                }
            }
            // ── --h2-avatar: the words at frame f, then the game's own cf.a(I)V per part ──
            // ── 2026-09-30 의상 아바타 A2: the costume book, as the app's worker runs it ──
            // ── ★ 2026-10-01 의상 아바타 A4: the round-clear counter, the app's plan (once, from
            //    f=30), and --h2-clear <f> — the next round started the game's own way: the entry
            //    kind ag.e:B ← 2 and event 9 raised (ag.d:B) for the dispatcher ag.g()V, which then
            //    runs ag.y()V through the redirected definition word ─────────────────────────────
            if h2_clear_hook && !clear_tried && f >= 30 {
                clear_tried = true;
                if let Some(patch) = guest_build::GuestBuild::Hero2Ktf.clear_hook_patch() {
                    match emu.alloc_guest(guest_build::ClearHookPatch::BLOCK_LEN) {
                        Some(block) => {
                            let plan = {
                                let rd = |a: u32, n: usize| emu.read_memory(a, n);
                                guest_build::h2_clear_hook_install_writes(&rd, &patch, block)
                            };
                            match plan {
                                Some(writes) => {
                                    let mut ok = true;
                                    for (a, bytes) in &writes {
                                        ok &= emu.write_memory(*a, bytes);
                                    }
                                    let rd = |a: u32, n: usize| emu.read_memory(a, n);
                                    let installed = guest_build::h2_clear_hook_installed(&rd, &patch, block);
                                    eprintln!("[h2-clear] f={f} block={block:#x} {} write(s) ok={ok} installed={installed}", writes.len());
                                    if installed {
                                        clear_block = Some(block);
                                    }
                                }
                                None => eprintln!("[h2-clear] f={f} REFUSED — ag.y()V's definition does not read as shipped"),
                            }
                        }
                        None => eprintln!("[h2-clear] f={f} no guest block"),
                    }
                }
            }
            // ── ★★ 2026-10-22 가디언 레벨: the app's install (once, from f=30) ──
            if h2_gl && !gl_tried && f >= 30 {
                gl_tried = true;
                if let Some(patch) = guest_build::GuestBuild::Hero2Ktf.guardian_level_patch() {
                    match emu.alloc_guest(guest_build::GuardianLevelPatch::BLOCK_LEN) {
                        Some(block) => {
                            let plan = {
                                let rd = |a: u32, n: usize| emu.read_memory(a, n);
                                guest_build::h2_guardian_level_install_writes(&rd, &patch, block)
                            };
                            match plan {
                                Some(writes) => {
                                    let mut ok = true;
                                    for (a, bytes) in &writes {
                                        ok &= emu.write_memory(*a, bytes);
                                    }
                                    let rd = |a: u32, n: usize| emu.read_memory(a, n);
                                    let installed = guest_build::h2_guardian_level_installed(&rd, &patch, block);
                                    eprintln!("[h2-gl] f={f} block={block:#x} {} write(s) ok={ok} installed={installed}", writes.len());
                                    if installed {
                                        gl_block = Some(block);
                                    }
                                }
                                None => eprintln!("[h2-gl] f={f} REFUSED — a.l()V's definition does not read as shipped"),
                            }
                        }
                        None => eprintln!("[h2-gl] f={f} no guest block"),
                    }
                }
            }
            for &(_, qi, qs) in h2_quest.iter().filter(|(qf, _, _)| *qf == f) {
                // ag.r:[B — the array's element base, then the quest's two bits (`ag.a(I)B`'s reading).
                let area = build().hero2_save().map(|s| s.area);
                let base = {
                    let rd = |a: u32, n: usize| emu.read_memory(a, n);
                    area.and_then(|a| guest_build::h2_static_handle(&rd, a.quest_states_slot)).and_then(|h| guest_build::h2_array(&rd, h)).map(|(b, _)| b)
                };
                let ok = match base {
                    Some(b) => {
                        let at = b + u32::from(qi >> 2);
                        let old = emu.read_memory(at, 1).first().copied().unwrap_or(0);
                        let sh = (qi & 3) << 1;
                        emu.write_memory(at, &[(old & !(3 << sh)) | (qs << sh)])
                    }
                    None => false,
                };
                let done = {
                    let rd = |a: u32, n: usize| emu.read_memory(a, n);
                    area.and_then(|a| guest_build::h2_quests_done(&rd, &a)).map(|d| d.iter().filter(|x| **x).count())
                };
                eprintln!("[h2-quest] f={f} quest {qi} ← state {qs} ok={ok} — {done:?} done now");
            }
            if h2_clear.contains(&f) {
                // ag.e:B @0x193abc = the entry kind (2 = the next round), ag.d:B @0x193aac = the event.
                let ok = emu.write_memory(0x0019_3abc, &2u32.to_le_bytes()) && emu.write_memory(0x0019_3aac, &9u32.to_le_bytes());
                eprintln!("[h2-clear] f={f} the next round raised (ag.e ← 2, event 9) ok={ok} — the hook {}", if clear_block.is_some() { "counts it" } else { "is NOT installed" });
            }
            let mut clear_event: Option<u8> = None;
            if let Some(block) = clear_block.filter(|_| f % 5 == 2) {
                let got = {
                    let rd = |a: u32, n: usize| emu.read_memory(a, n);
                    guest_build::h2_clear_hook_read(&rd, block)
                };
                if let Some((count, mode)) = got.filter(|(c, _)| *c != clear_seen) {
                    eprintln!("[h2-clear] f={f} ag.y()V ran (count {clear_seen} → {count}, mode cleared {mode})");
                    clear_seen = count;
                    clear_event = u8::try_from(mode).ok();
                }
            }
            if !clear_notices.is_empty() {
                let (field, text) = {
                    let rd = |a: u32, n: usize| emu.read_memory(a, n);
                    let field = match build().app_state() {
                        Some(st @ AppState::Absolute { addr, .. }) => {
                            let b = rd(addr, 4);
                            b.len() == 4 && u32::from_le_bytes([b[0], b[1], b[2], b[3]]) == st.field()
                        }
                        _ => false,
                    };
                    (field, guest_build::h2_text_row(&rd, guest_build::Hero2Java::ITEM_TYPE_TABLE_SLOT, clear_notices[0]))
                };
                if field {
                    let row = clear_notices.remove(0);
                    let raised = match text {
                        Some(t) => h2_call(&mut *emu, guest_build::JavaCall::Notice, None, &[JavaArg::Object(t), JavaArg::Int(16), JavaArg::Int(0)]).is_some(),
                        None => false,
                    };
                    eprintln!("[h2-clear] f={f} notice /t/itmtp[{row}] raised={raised}");
                }
            }
            if let (Some(book), Some(io)) = (costume_book.as_mut(), costume_io.as_ref()) {
                let (shell, slot_mode, save_slot, in_world) = {
                    let rd = |a: u32, n: usize| emu.read_memory(a, n);
                    let word = |a: u32| {
                        let b = rd(a, 4);
                        (b.len() == 4).then(|| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
                    };
                    let h2 = build().hero2_java();
                    // `items::shell_state`: a shell state word AND the title singleton alive.
                    let shell = match (build().app_state(), h2) {
                        (Some(st @ AppState::Absolute { addr, .. }), Some(h2)) => word(addr).is_some_and(|v| st.is_shell(v)) && word(h2.title_slot).is_some_and(|p| p != 0),
                        _ => false,
                    };
                    let mode = h2.and_then(|h2| guest_build::h2_slot_screen(&rd, &h2)).map(|s| s.mode);
                    let slot = h2.and_then(|h2| rd(h2.save_slot_slot, 1).first().copied());
                    (shell, mode, slot, !shell && guest_build::h2_parts_images(&rd, 1).is_some())
                };
                if shell {
                    match slot_mode {
                        Some(m) => costume_ng = (m == i32::from(guest_build::Hero2Java::SLOT_MODE_CLASS_PICK), 0),
                        None => {
                            costume_ng.1 = costume_ng.1.saturating_add(1);
                            if costume_ng.1 >= 5 {
                                costume_ng.0 = false;
                            }
                        }
                    }
                }
                if costume_last_shell && !shell {
                    let new_game = std::mem::take(&mut costume_ng).0;
                    match save_slot.filter(|s| *s < guest_build::SaveSlotsPatch::SLOTS) {
                        Some(s) => {
                            eprintln!("[h2-costume] f={f} entered the world — slot {} ({})", s + 1, if new_game { "새로하기" } else { "이어하기" });
                            book.enter_world(io, s + 1, new_game, f ^ 0x5eed_1234_abcd);
                            // ★★ 2026-10-22 가디언 레벨: armed from this record's GL once the field shows the character.
                            gl_want_arm = gl_block.is_some();
                            gl_told = None;
                            // ★ A5: the starter set — owned from the start (the app does the same).
                            let class = {
                                let rd = |a: u32, n: usize| emu.read_memory(a, n);
                                build().hero2_java().and_then(|h2| guest_build::h2_char_class(&rd, &h2))
                            };
                            let starter: Vec<(u16, u8)> = avatar_state::STARTER_SETS
                                .iter()
                                .flat_map(|&set| guest_build::h2_av_set_slots_for(&costume_catalog, &avatar_sheets, set, class).into_iter().map(move |slot| (set, slot)))
                                .collect();
                            book.grant_starter(io, &starter);
                            // ★ 2026-10-02: the clears the loaded save had made already, paid as EASY
                            //   clears with their notices (the app's world entry does the same).
                            if !new_game {
                                let g = avatar_state::AvatarIo::live_g(io, s + 1);
                                match g.as_deref().and_then(avatar_state::g_progress) {
                                    Some(progress) => {
                                        eprintln!(
                                            "[h2-costume] f={f} slot {} save: mode {} 달성도 {} → cleared before {:?}",
                                            s + 1,
                                            avatar_state::mode_label(progress.mode),
                                            progress.achievement,
                                            avatar_state::cleared_before(progress).into_iter().map(avatar_state::mode_label).collect::<Vec<_>>()
                                        );
                                        let class = class.or_else(|| g.as_deref().and_then(avatar_state::g_class));
                                        let slots_for = |set: u16| guest_build::h2_av_set_slots_for(&costume_catalog, &avatar_sheets, set, class);
                                        for reward in book.grant_earned(io, progress, &slots_for) {
                                            clear_notices.push(reward.notice_row);
                                        }
                                    }
                                    None => eprintln!("[h2-costume] f={f} slot {}: the /g does not read as a whole save — no earlier clears paid", s + 1),
                                }
                            }
                        }
                        None => eprintln!("[h2-costume] f={f} entered the world, save slot unreadable"),
                    }
                }
                if shell && !costume_last_shell {
                    book.leave_world(io);
                    // ★★ 2026-10-22 가디언 레벨: the block waits for the next character.
                    gl_want_arm = false;
                    gl_told = None;
                    if let Some(block) = gl_block {
                        let (a, bytes) = guest_build::h2_guardian_level_disarm_write(block);
                        emu.write_memory(a, &bytes);
                    }
                }
                costume_last_shell = shell;
                // ★★ 2026-10-22 가디언 레벨: the app's per-frame step (`emulator::GuardianLevel::poll`), said as [h2-gl].
                if let Some(block) = gl_block {
                    let (field, live, words) = {
                        let rd = |a: u32, n: usize| emu.read_memory(a, n);
                        let field = match build().app_state() {
                            Some(st @ AppState::Absolute { addr, .. }) => {
                                let b = rd(addr, 4);
                                b.len() == 4 && u32::from_le_bytes([b[0], b[1], b[2], b[3]]) == st.field()
                            }
                            _ => false,
                        };
                        let live = build().hero2_java().and_then(|h2| guest_build::h2_guardian_level_char(&rd, h2.char_slot));
                        (field, live, guest_build::h2_guardian_level_read(&rd, block))
                    };
                    match guest_build::h2_guardian_level_step(gl_want_arm, gl_told, field, live, words, book.guardian_level()) {
                        guest_build::H2GlStep::Idle => {}
                        guest_build::H2GlStep::Arm { owner, gl, fresh, level, record } => {
                            let mut ok = true;
                            for (a, bytes) in guest_build::h2_guardian_level_arm_writes(block, owner, gl, fresh) {
                                ok &= emu.write_memory(a, &bytes);
                            }
                            if ok {
                                gl_want_arm = false;
                                gl_told = Some(gl);
                                book.set_guardian_level(io, gl);
                            }
                            eprintln!("[h2-gl] f={f} ARM ok={ok} LV {level} record {record} → GL {gl} fresh={fresh} owner={owner:#x}");
                        }
                        guest_build::H2GlStep::ReArm { owner, gl, was } => {
                            for (a, bytes) in guest_build::h2_guardian_level_arm_writes(block, owner, gl, false) {
                                emu.write_memory(a, &bytes);
                            }
                            eprintln!("[h2-gl] f={f} RE-ARM {was:#x} → {owner:#x} at GL {gl}");
                        }
                        guest_build::H2GlStep::Follow { gl, was } => {
                            gl_told = Some(gl);
                            book.set_guardian_level(io, gl);
                            let w = words.unwrap_or(guest_build::H2GuardianWords { armed: true, owner: 0, gl: u32::from(gl), next: 0, ups: 0 });
                            eprintln!(
                                "[h2-gl] f={f} GL {was} → {gl} (next {}, ups {}){}",
                                w.next,
                                w.ups,
                                if gl > was { format!(" — notice {:?}", guest_build::h2_guardian_level_notice(u32::from(gl))) } else { String::new() }
                            );
                        }
                    }
                    if f % 100 == 0 && gl_told.is_some() {
                        let rd = |a: u32, n: usize| emu.read_memory(a, n);
                        let ch = build().hero2_java().and_then(|h2| guest_build::h2_static_handle(&rd, h2.char_slot)).and_then(|h| guest_build::h2_fields(&rd, h));
                        let at = |off: u32| ch.and_then(|fb| guest_build::h2_rd_u32(&rd, fb + 4 + off)).map(|v| v as i32);
                        if let Some(w) = guest_build::h2_guardian_level_read(&rd, block) {
                            eprintln!(
                                "[h2-gl] f={f} armed={} GL {} next {} ups {} | LV {:?} EXP {:?} a.f {:?} stat points {:?}",
                                w.armed,
                                w.gl,
                                w.next,
                                w.ups,
                                at(0x5c),
                                at(0x74),
                                at(0x80),
                                at(0x64)
                            );
                        }
                    }
                }
                let class = {
                    let rd = |a: u32, n: usize| emu.read_memory(a, n);
                    build().hero2_java().and_then(|h2| guest_build::h2_char_class(&rd, &h2))
                };
                // ★ A4: a set's items for THIS character's class (the app's grants do the same).
                for &(_, n) in costume_grant.iter().filter(|(gf, _)| *gf == f) {
                    if book.grant_and_wear(io, n, &guest_build::h2_av_set_slots_for(&costume_catalog, &avatar_sheets, n, class), 0).is_none() {
                        eprintln!("[h2-costume] f={f} grant {n}: nobody in the world");
                    }
                }
                for &(_, n, mode, slot) in costume_give.iter().filter(|(gf, _, _, _)| *gf == f) {
                    let slots = match slot {
                        Some(s) => vec![s],
                        None => guest_build::h2_av_set_slots_for(&costume_catalog, &avatar_sheets, n, class),
                    };
                    if book.give(io, n, &slots, mode).is_none() {
                        eprintln!("[h2-costume] f={f} give {n}: nobody in the world");
                    }
                }
                for &(_, n) in costume_wear.iter().filter(|(wf, _)| *wf == f) {
                    if !book.wear_avatar(io, n) {
                        eprintln!("[h2-costume] f={f} wear {n:?}: not owned / nobody in the world");
                    }
                }
                if f % 5 == 2 {
                    // ★★ 2026-10-22 가디언 레벨 (review P1): the GL the save was made at, before the commit (the app's order).
                    if let Some(block) = gl_block {
                        let saved = {
                            let rd = |a: u32, n: usize| emu.read_memory(a, n);
                            guest_build::h2_guardian_level_saved(&rd, block)
                        };
                        if let Some((saves, gl)) = saved.filter(|(s, _)| *s != gl_saves_seen) {
                            gl_saves_seen = saves;
                            eprintln!("[h2-gl] f={f} save #{saves} snapshotted at GL {gl}");
                            book.note_save_gl(io, u16::try_from(gl.min(guest_build::GuardianLevelPatch::MAX_GL)).unwrap_or(0));
                        }
                    }
                    book.poll(io);
                }
                // ★ A4: a round cleared since the last poll pays its mode's set (the app's
                // `ClearHook::poll` → `AvatarBook::clear_reward`).
                if let Some(m) = clear_event {
                    let slots_for = |set: u16| guest_build::h2_av_set_slots_for(&costume_catalog, &avatar_sheets, set, class);
                    for out in book.clear_rewards(io, m, &slots_for) {
                        eprintln!("[h2-clear] f={f} {} cleared ({}) → {out:?}", avatar_state::mode_label(m), avatar_state::class_label(class));
                        if let avatar_state::ClearOutcome::Paid { reward, .. } = out {
                            clear_notices.push(reward.notice_row);
                        }
                    }
                }
                // ★ A5: the quest marks, after the clear (the app's HUD-poll order) — at the 난이도
                // --h2-difficulty holds now.
                if f % 5 == 2 && in_world {
                    let done = {
                        let rd = |a: u32, n: usize| emu.read_memory(a, n);
                        build().hero2_save().and_then(|save| guest_build::h2_quests_done(&rd, &save.area))
                    };
                    if let Some(done) = done {
                        let d = h2_difficulty.iter().filter(|(at, _)| f >= *at).last().map_or(0, |(_, d)| *d);
                        book.observe_quests(io, &done, d);
                    }
                }
                let mut wanted = avatar_state::costume_parts(book.worn(), |p, n| avatar_sheets.has(p, n, class));
                // ★★ 2026-10-13 「악마화」 D2: the app's `Demon::poll` — the demon body wanted while the block's STATE is 1 · 2,
                //   LOOK up once the loader holds it, down before the human body comes back (the writes after the avatar's).
                let demon_plan = demon_block.and_then(|b| {
                    let rd = |a: u32, n: usize| emu.read_memory(a, n);
                    guest_build::h2_demon_words(&rd, b).map(|w| {
                        let demon_world = build().hero2_java().is_some_and(|h2| guest_build::h2_demon_in_world(&rd, &h2));
                        (w, guest_build::h2_demon_wing_plan(w.state, w.look, class, demon_world))
                    })
                });
                if let Some((_, plan)) = demon_plan
                    && let Some(n) = plan.body
                {
                    wanted[0] = (n, guest_build::AvatarPatch::PASS);
                }
                let words = guest_build::h2_avatar_resolve(&wanted, class, &avatar_sheets);
                if let Some(block) = avatar_block.filter(|_| costume_applied != Some(words)) {
                    let changed: Vec<u8> = guest_build::AvatarPatch::PARTS
                        .iter()
                        .enumerate()
                        .filter(|(i, _)| costume_applied.is_none_or(|a| a[*i] != words[*i]))
                        .map(|(_, p)| *p)
                        .collect();
                    let mut ok = true;
                    for (a, bytes) in guest_build::h2_avatar_word_writes(block, &words) {
                        ok &= emu.write_memory(a, &bytes);
                    }
                    costume_applied = Some(words);
                    eprintln!(
                        "[h2-costume] f={f} override body {:?} arms {:?} legs {:?} head {:?} weapon {:?} off-hand {:?} (class {class:?}) ok={ok}{}",
                        words[0], words[1], words[2], words[3], words[4], words[5],
                        if in_world { " — reloading" } else { " — reloaded on entering the world" }
                    );
                    if ok {
                        for p in changed {
                            if in_world {
                                if !avatar_queue.contains(&guest_build::H2PartOp::Reload(p)) {
                                    avatar_queue.push(guest_build::H2PartOp::Reload(p));
                                }
                            } else if !avatar_pre_world.contains(&p) {
                                avatar_pre_world.push(p);
                            }
                        }
                    }
                }
                if in_world && !avatar_pre_world.is_empty() {
                    for p in std::mem::take(&mut avatar_pre_world) {
                        if !avatar_queue.contains(&guest_build::H2PartOp::Reload(p)) {
                            avatar_queue.push(guest_build::H2PartOp::Reload(p));
                        }
                    }
                }
                // ★ D2: the demon block's look words (the table and the transform sequence before LOOK, STATE last)
                if let (Some((w, plan)), Some(b)) = (demon_plan, demon_block) {
                    let writes = guest_build::h2_demon_look_writes(b, &plan, class);
                    if !writes.is_empty() {
                        let mut ok = true;
                        for (a, bytes) in &writes {
                            ok &= emu.write_memory(*a, bytes);
                        }
                        eprintln!("[h2-demon] f={f} look: state {} look {} → body {:?} look {:?} state {:?} table {} (class {class:?}) ok={ok}", w.state, w.look, plan.body, plan.look, plan.state, plan.table);
                    }
                }
                // ★★ 2026-10-16 THE SKIN — as `Demon::poll_skin`: each container exchanged while STATE 2, back when it ends
                if demon_block.is_some() {
                    let want = demon_plan.is_some_and(|(w, _)| w.state == guest_build::DemonPatch::STATE_ON as i32);
                    for (k, rows) in [(0usize, true), (1usize, false)] {
                        let (container, pairs, standing, writes) = {
                            let rd = |a: u32, n: usize| emu.read_memory(a, n);
                            let rd32 = |a: u32| -> Option<u32> { rd(a, 4).try_into().ok().map(u32::from_le_bytes) };
                            let container = if rows { guest_build::h2_beast_text_table(&rd) } else { guest_build::h2_beast_icon_array(&rd) };
                            let pairs = container.and_then(|c| if rows { guest_build::h2_demon_skin_row_pairs(&rd, c) } else { guest_build::h2_demon_skin_icon_pairs(&rd, c) });
                            let standing = match (skin_state[k], container, pairs.as_ref()) {
                                (Some((c, placed)), Some(now), Some(p)) if c == now && !p.is_empty() => rd32(p[0].0) == Some(placed),
                                _ => false,
                            };
                            let writes = pairs.as_ref().and_then(|p| guest_build::h2_exchange_writes(&rd, p));
                            (container, pairs, standing, writes)
                        };
                        if !standing {
                            skin_state[k] = None;
                        }
                        if want == standing {
                            continue;
                        }
                        let (Some(c), Some(_p), Some(writes)) = (container, pairs, writes) else { continue };
                        let mut ok = true;
                        for (a, h) in &writes {
                            ok &= emu.write_memory(*a, &h.to_le_bytes());
                        }
                        skin_state[k] = want.then_some((c, writes[0].1));
                        // the window chain repaints (as `Accessory::repaint_chain` after the app's exchange)
                        if let Some(h2) = build().hero2_java() {
                            let rp = {
                                let rd = |a: u32, n: usize| emu.read_memory(a, n);
                                guest_build::h2_skill_name_repaint_writes(&rd, &h2)
                            };
                            for (a, bytes) in rp {
                                ok &= emu.write_memory(a, &bytes);
                            }
                        }
                        eprintln!("[h2-skin] f={f} the beast {} at {c:#x} {} ({} slot(s)) ok={ok}", if rows { "table" } else { "icons" }, if want { "in the 악의 versions" } else { "back in the shipped order" }, writes.len());
                    }
                }
            }
            if let Some(block) = avatar_block.filter(|_| !h2_avatar.is_empty() || costume_book.is_some()) {
                if let Some(&(_, parts)) = h2_avatar.iter().find(|(af, _)| *af == f) {
                    if avatar_step.is_some() || !avatar_queue.is_empty() {
                        eprintln!("[h2-avatar] f={f} a reload is still in flight — this spec is skipped");
                    }
                    let rd = |a: u32, n: usize| emu.read_memory(a, n);
                    let class = build().hero2_java().and_then(|h2| guest_build::h2_char_class(&rd, &h2));
                    let words = guest_build::h2_avatar_resolve(&parts, class, &avatar_sheets);
                    let before: Vec<Option<u32>> = guest_build::AvatarPatch::PARTS.iter().map(|p| guest_build::h2_parts_images(&rd, *p)).collect();
                    let changed: Vec<u8> = guest_build::AvatarPatch::PARTS
                        .iter()
                        .enumerate()
                        .filter(|(i, _)| costume_applied.is_none_or(|a| a[*i] != words[*i]))
                        .map(|(_, p)| *p)
                        .collect();
                    let mut ok = true;
                    for (a, bytes) in guest_build::h2_avatar_word_writes(block, &words) {
                        ok &= emu.write_memory(a, &bytes);
                    }
                    costume_applied = Some(words);
                    eprintln!(
                        "[h2-avatar] f={f} words body {:?} arms {:?} legs {:?} head {:?} weapon {:?} off-hand {:?} (class {class:?}) ok={ok} — parts {changed:?} change; images before (PARTS order) {before:?}",
                        words[0], words[1], words[2], words[3], words[4], words[5]
                    );
                    if avatar_step.is_none() && avatar_queue.is_empty() {
                        avatar_queue = changed.into_iter().map(guest_build::H2PartOp::Reload).collect();
                    }
                }
                if let Some((out, op, since)) = avatar_step.take() {
                    match out.poll() {
                        None => avatar_step = Some((out, op, since)),
                        Some(Err(())) => {
                            eprintln!("[h2-avatar] f={f} {op:?} FAILED (issued f={since})");
                            avatar_queue.clear();
                        }
                        Some(Ok(_)) => {
                            let rd = |a: u32, n: usize| emu.read_memory(a, n);
                            let part = op.part().unwrap_or(5);
                            eprintln!("[h2-avatar] f={f} {op:?} done (issued f={since}) — cf.a[{}] now {:?}", u32::from(part) + guest_build::Hero2Java::PARTS_IMAGES_BASE, guest_build::h2_parts_images(&rd, part));
                        }
                    }
                }
                // ★ 2026-10-01: as the app's `Avatar::poll` — every 5 frames, the EQUIPMENT window's
                // changes get the game's own per-slot refresh (and one look after it closes).
                if f % 5 == 0 {
                    let rd = |a: u32, n: usize| emu.read_memory(a, n);
                    let h2 = build().hero2_java();
                    // `items::in_game_state`: the state word says field or window, and the parts exist.
                    let in_game = match build().app_state() {
                        Some(st @ AppState::Absolute { addr, .. }) => {
                            let b = rd(addr, 4);
                            b.len() == 4 && st.in_game(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
                        }
                        _ => false,
                    };
                    let in_world = h2.is_some() && in_game && guest_build::h2_parts_images(&rd, 1).is_some();
                    let window = in_world && equip_ui_block.is_some() && h2.is_some_and(|h2| guest_build::h2_equip_view(&rd, &h2).is_some_and(|v| v.shifted));
                    let records = if in_world { h2.and_then(|h2| guest_build::h2_equip_records(&rd, &h2)) } else { None };
                    let slots = equip_look.look(window, records);
                    let class = h2.and_then(|h2| guest_build::h2_char_class(&rd, &h2));
                    if let (false, Some(now), Some(c)) = (slots.is_empty(), records, class) {
                        let second = guest_build::h2_weapon_trait(&rd, now[guest_build::Hero2Java::EQUIP_OFFHAND_SLOT]);
                        let ops = guest_build::h2_equip_refresh(&slots, &now, c, second);
                        eprintln!("[h2-equip-look] f={f} the EQUIPMENT window changed slot(s) {slots:?} (window up: {window}) — {ops:?}");
                        for op in ops {
                            if !avatar_queue.contains(&op) {
                                avatar_queue.push(op);
                            }
                        }
                    }
                }
                if avatar_step.is_none() {
                    // ⛔ As the app: never a part this class does not draw (the berserker has no part 7).
                    let class = {
                        let rd = |a: u32, n: usize| emu.read_memory(a, n);
                        build().hero2_java().and_then(|h2| guest_build::h2_char_class(&rd, &h2))
                    };
                    if let Some(c) = class {
                        avatar_queue.retain(|op| op.part().is_none_or(|p| !guest_build::AvatarPatch::typed(p) || guest_build::AvatarPatch::path_type(p, c).is_some()));
                    }
                    if let Some(op) = avatar_queue.first().copied() {
                        let (what, arg) = op.call();
                        match h2_call(&mut *emu, what, None, &[JavaArg::Int(arg)]) {
                            Some(out) => {
                                avatar_queue.remove(0);
                                avatar_step = Some((out, op, f));
                            }
                            None => {
                                eprintln!("[h2-avatar] f={f} {what:?} refused");
                                avatar_queue.clear();
                            }
                        }
                    }
                }
            }
            // ── --h2-exp-gain: the game's own a.d(I)V by name, the EXP read back after ──
            if let Some(&(_, n)) = h2_exp_gain.iter().find(|(gf, _)| *gf == f) {
                if exp_gain_step.is_some() {
                    eprintln!("[h2-exp-gain] f={f} a gain is still in flight — skipped");
                } else {
                    let (before, beast_before) = {
                        let rd = |a: u32, n: usize| emu.read_memory(a, n);
                        exp_pair(build(), &rd)
                    };
                    match before {
                        None => eprintln!("[h2-exp-gain] f={f} no character yet — skipped"),
                        Some(before) => match h2_call(&mut *emu, guest_build::JavaCall::CharGainExp, None, &[JavaArg::Int(n)]) {
                            Some(out) => exp_gain_step = Some(ExpGainStep { out, before, beast_before, n, since: f }),
                            None => eprintln!("[h2-exp-gain] f={f} CharGainExp refused"),
                        },
                    }
                }
            }
            if let Some(step) = exp_gain_step.take() {
                match step.out.poll() {
                    None => exp_gain_step = Some(step),
                    Some(Err(())) => eprintln!("[h2-exp-gain] f={f} a.d(I)V FAILED (issued f={})", step.since),
                    Some(Ok(_)) => {
                        let (after, beast_after) = {
                            let rd = |a: u32, n: usize| emu.read_memory(a, n);
                            exp_pair(build(), &rd)
                        };
                        // The multiplier in force: the block's word if the redirect is in, else 1.
                        let mult = exp_block
                            .and_then(|b| {
                                let rd = |a: u32, n: usize| emu.read_memory(a, n);
                                guest_build::h2_exp_rate_mult(&rd, b)
                            })
                            .unwrap_or(1);
                        let want = i64::from(step.n) * i64::from(mult);
                        let delta = after.map(|a| i64::from(a) - i64::from(step.before));
                        let beast = match (step.beast_before, beast_after) {
                            (Some(b), Some(a)) => format!(" beast {b} -> {a} (+{})", i64::from(a) - i64::from(b)),
                            _ => " beast -".to_string(),
                        };
                        eprintln!(
                            "[h2-exp-gain] f={f} a.d({}) at x{mult}: exp {} -> {:?} ({}, expected +{want}){beast}{}",
                            step.n,
                            step.before,
                            after,
                            delta.map_or("?".to_string(), |d| format!("{d:+}")),
                            if delta == Some(want) { "  OK" } else { "  ** MISMATCH **" }
                        );
                    }
                }
            }
            // ── --h2-pages / --h2-page: the inventory paging patch, the app's own plan ──
            if h2_pages && page_block.is_none() && f >= 30 {
                if let Some(patch) = guest_build::GuestBuild::Hero2Ktf.inventory_page_patch() {
                    match emu.alloc_guest(guest_build::InventoryPagePatch::BLOCK_LEN) {
                        Some(block) => {
                            let plan = {
                                let rd = |a: u32, n: usize| emu.read_memory(a, n);
                                guest_build::h2_page_install_writes(&rd, &patch, block)
                            };
                            match plan {
                                Some(writes) => {
                                    let mut ok = true;
                                    for (a, bytes) in &writes {
                                        ok &= emu.write_memory(*a, bytes);
                                    }
                                    let rd = |a: u32, n: usize| emu.read_memory(a, n);
                                    let installed = guest_build::h2_page_installed(&rd, &patch, block);
                                    eprintln!("[h2-pages] f={f} block={block:#x} {} write(s) ok={ok} installed={installed}", writes.len());
                                    page_block = Some(block);
                                    // ★★ 2026-10-20 the eighth site (사용하기 → a.g(B)V), the app's own plan.
                                    if let Some(up) = guest_build::GuestBuild::Hero2Ktf.item_use_page_patch().filter(|_| h2_use_page) {
                                        match emu.alloc_guest(guest_build::ItemUsePagePatch::BLOCK_LEN) {
                                            Some(ub) => {
                                                let plan = {
                                                    let rd = |a: u32, n: usize| emu.read_memory(a, n);
                                                    guest_build::h2_use_page_install_writes(&rd, &up, block, ub)
                                                };
                                                match plan {
                                                    Some(writes) => {
                                                        let mut ok = true;
                                                        for (a, bytes) in &writes {
                                                            ok &= emu.write_memory(*a, bytes);
                                                        }
                                                        let rd = |a: u32, n: usize| emu.read_memory(a, n);
                                                        let installed = guest_build::h2_use_page_installed(&rd, &up, ub);
                                                        eprintln!("[h2-pages] f={f} 사용하기 site block={ub:#x} {} write(s) ok={ok} installed={installed}", writes.len());
                                                    }
                                                    None => eprintln!("[h2-pages] f={f} 사용하기 site REFUSED — the site, cell or def does not read as expected"),
                                                }
                                            }
                                            None => eprintln!("[h2-pages] f={f} 사용하기 site: no guest block"),
                                        }
                                    }
                                }
                                None => eprintln!("[h2-pages] f={f} REFUSED — a site or a def does not read as shipped"),
                            }
                        }
                        None => eprintln!("[h2-pages] f={f} no guest block"),
                    }
                }
            }
            if let Some(block) = page_block {
                // Keep the context current the way the app does: the inventory handle as
                // it is NOW (a load makes a new bag), page 0 whenever the window is not up.
                // ★ 2026-10-09: with --h2-bags127 the 재료가방's window pages too (the app's BagPaging::bags).
                let (view, bag, cur) = {
                    let rd = |a: u32, n: usize| emu.read_memory(a, n);
                    let inv = guest_build::h2_static_handle(&rd, h2.inventory_slot).unwrap_or(0);
                    let mut bags = vec![inv];
                    if h2_bags127 {
                        bags.extend(guest_build::h2_static_handle(&rd, h2.material_bag_slot));
                    }
                    let view = guest_build::h2_bag_window(&rd, &h2, &bags);
                    (view, view.map_or(inv, |v| v.bag), guest_build::h2_page_ctx(&rd, block))
                };
                if view.is_none() || page_bag != Some(bag) {
                    page_now = 0;
                }
                page_bag = view.map(|v| v.bag);
                for &(pf, k) in h2_page.iter().filter(|p| p.0 == f) {
                    match view {
                        Some(v) if v.on_top => {
                            let writes = {
                                let rd = |a: u32, n: usize| emu.read_memory(a, n);
                                guest_build::h2_page_select_writes(&rd, &h2, block, &v, k, 0)
                            };
                            let mut ok = true;
                            for (a, bytes) in &writes {
                                ok &= emu.write_memory(*a, bytes);
                            }
                            page_now = k;
                            let cells = emu.read_memory(guest_build::h2_page_cells_write(&v, k).0, 1).first().copied();
                            eprintln!(
                                "[h2-page] f={pf} page {k}: {} write(s) ok={ok} (window {:#x} over bag {:#x}, panel {:?}, j.i = {cells:?}, hidden cells {:?})",
                                writes.len(),
                                v.win.handle,
                                v.bag,
                                v.panel,
                                guest_build::h2_page_hidden_cells(v.panel, guest_build::h2_page_cells(k), guest_build::Hero2Java::INV_GRID_SHIFT).len()
                            );
                        }
                        _ => eprintln!("[h2-page] f={pf} page {k}: the inventory window is not on top — refused"),
                    }
                }
                let want = guest_build::h2_page_ctx_writes(block, bag, page_now);
                let want_ctx = (bag, u32::from_le_bytes([want[1].1[0], want[1].1[1], want[1].1[2], want[1].1[3]]));
                if cur != Some(want_ctx) {
                    for (a, bytes) in &want {
                        emu.write_memory(*a, bytes);
                    }
                    eprintln!("[h2-pages] f={f} ctx {cur:x?} -> {want_ctx:x?}");
                }
            }
            // ── ★ 2026-10-02: the cooldown tick counter, the app's plan (once, from f=30) ──
            if h2_cd_clock && !cd_clock_tried && f >= 30 {
                cd_clock_tried = true;
                if let Some(patch) = guest_build::GuestBuild::Hero2Ktf.cooldown_clock_patch() {
                    match emu.alloc_guest(guest_build::CooldownClockPatch::BLOCK_LEN) {
                        Some(block) => {
                            let plan = {
                                let rd = |a: u32, n: usize| emu.read_memory(a, n);
                                guest_build::h2_cd_clock_install_writes(&rd, &patch, block)
                            };
                            match plan {
                                Some(writes) => {
                                    let mut ok = true;
                                    for (a, bytes) in &writes {
                                        ok &= emu.write_memory(*a, bytes);
                                    }
                                    let rd = |a: u32, n: usize| emu.read_memory(a, n);
                                    let installed = guest_build::h2_cd_clock_installed(&rd, &patch, block);
                                    eprintln!("[h2-cd] f={f} clock block={block:#x} {} write(s) ok={ok} installed={installed}", writes.len());
                                    if installed {
                                        cd_clock_block = Some(block);
                                    }
                                }
                                None => eprintln!("[h2-cd] f={f} REFUSED — ag.i()V's definition does not read as shipped"),
                            }
                        }
                        None => eprintln!("[h2-cd] f={f} no guest block"),
                    }
                }
            }
            // ── ★★ 2026-10-16: the item icon block, the app's plan (once, from f=30) ──
            if h2_item_icon && !item_icon_tried && f >= 30 {
                item_icon_tried = true;
                if let Some(patch) = guest_build::GuestBuild::Hero2Ktf.item_icon_patch() {
                    match emu.alloc_guest(guest_build::ItemIconPatch::BLOCK_LEN) {
                        Some(block) => {
                            let plan = {
                                let rd = |a: u32, n: usize| emu.read_memory(a, n);
                                guest_build::h2_item_icon_install_writes(&rd, &patch, block)
                            };
                            match plan {
                                Some(writes) => {
                                    let mut ok = true;
                                    for (a, bytes) in &writes {
                                        ok &= emu.write_memory(*a, bytes);
                                    }
                                    let rd = |a: u32, n: usize| emu.read_memory(a, n);
                                    let installed = guest_build::h2_item_icon_installed(&rd, &patch, block);
                                    eprintln!("[h2-icon] f={f} item icon block={block:#x} {} write(s) ok={ok} installed={installed}", writes.len());
                                }
                                None => eprintln!("[h2-icon] f={f} REFUSED — n.a(IIB)V's definition does not read as shipped"),
                            }
                        }
                        None => eprintln!("[h2-icon] f={f} no guest block"),
                    }
                }
            }
            // ── ★ 2026-10-12: the slot-screen focus, the app's plan (once, from f=30), then the app's note on a field ──
            if h2_slot_focus && !slot_focus_tried && f >= 30 {
                slot_focus_tried = true;
                if let Some(patch) = guest_build::GuestBuild::Hero2Ktf.slot_focus_patch() {
                    match emu.alloc_guest(guest_build::SlotFocusPatch::BLOCK_LEN) {
                        Some(block) => {
                            let plan = {
                                let rd = |a: u32, n: usize| emu.read_memory(a, n);
                                guest_build::h2_slot_focus_install_writes(&rd, &patch, block, guest_build::h2_slot_focus_last(h2_slot_last))
                            };
                            match plan {
                                Some(writes) => {
                                    let mut ok = true;
                                    for (a, bytes) in &writes {
                                        ok &= emu.write_memory(*a, bytes);
                                    }
                                    let rd = |a: u32, n: usize| emu.read_memory(a, n);
                                    let installed = guest_build::h2_slot_focus_installed(&rd, &patch, block);
                                    eprintln!("[h2-slot] f={f} focus block={block:#x} {} write(s) ok={ok} installed={installed} last={:?}", writes.len(), h2_slot_last.map(|s| s + 1));
                                    if installed {
                                        slot_focus_block = Some(block);
                                    }
                                }
                                None => eprintln!("[h2-slot] f={f} REFUSED — be.a()V's definition does not read as shipped"),
                            }
                        }
                        None => eprintln!("[h2-slot] f={f} no guest block"),
                    }
                }
            }
            if let (Some(block), Some(h2)) = (slot_focus_block, GuestBuild::Hero2Ktf.hero2_java()) {
                let (on_field, slot, screen) = {
                    let rd = |a: u32, n: usize| emu.read_memory(a, n);
                    let on_field = matches!(build().app_state(), Some(AppState::Absolute { addr, field, .. })
                        if rd(addr, 4).len() == 4 && u32::from_le_bytes(rd(addr, 4).try_into().unwrap()) == field);
                    (on_field, rd(h2.save_slot_slot, 1).first().copied(), guest_build::h2_slot_screen(&rd, &h2))
                };
                // the app's SlotFocus::note: the slot the world was entered from is the last played
                if on_field && let Some(s) = slot.filter(|s| *s < guest_build::SaveSlotsPatch::SLOTS) && h2_slot_last != Some(s) {
                    h2_slot_last = Some(s);
                    emu.write_memory(block + guest_build::SlotFocusPatch::LAST, &u32::from(s).to_le_bytes());
                    eprintln!("[h2-slot] f={f} SLOT {} played — LAST", s + 1);
                }
                let now = screen.as_ref().map(|w| w.handle);
                if now != slot_screen_seen {
                    slot_screen_seen = now;
                    if let Some(w) = screen {
                        eprintln!("[h2-slot] f={f} slot screen opens: mode {} cursor {} (SLOT {}) rows {}", w.mode, w.cursor, w.cursor + 1, w.rows);
                    }
                }
            }
            // ── ★ 2026-10-05: the buff refresh, the app's plan (once, from f=30) ─────────────
            if h2_buff_refresh && !buff_refresh_tried && f >= 30 {
                buff_refresh_tried = true;
                if let Some(patch) = guest_build::GuestBuild::Hero2Ktf.buff_refresh_patch() {
                    match emu.alloc_guest(guest_build::BuffRefreshPatch::BLOCK_LEN) {
                        Some(block) => {
                            let plan = {
                                let rd = |a: u32, n: usize| emu.read_memory(a, n);
                                guest_build::h2_buff_refresh_install_writes(&rd, &patch, block)
                            };
                            match plan {
                                Some(writes) => {
                                    let mut ok = true;
                                    for (a, bytes) in &writes {
                                        ok &= emu.write_memory(*a, bytes);
                                    }
                                    let rd = |a: u32, n: usize| emu.read_memory(a, n);
                                    let installed = guest_build::h2_buff_refresh_installed(&rd, &patch, block);
                                    eprintln!("[h2-buff] f={f} refresh block={block:#x} {} write(s) ok={ok} installed={installed}", writes.len());
                                }
                                None => eprintln!("[h2-buff] f={f} REFUSED — g.a(BI)V / g.b(BBI)V definitions do not read as shipped"),
                            }
                        }
                        None => eprintln!("[h2-buff] f={f} no guest block"),
                    }
                }
            }
            // ── ★ 2026-10-08 밤 가디언 장비의 개성 옵션 13 (guest_anchors::GearPatch) — after every block it chains onto
            //    (SkillPower · combat · accessory · 일풍격 · berserker · buff-refresh) and the guardian / venom blocks it
            //    points at; the app's own plan (`emulator::Gear`). `--h2-shipped 1` skips it with the other balance patches. ──
            // ★★ 2026-10-14 and after the rogue block: its cooldown-table wrappers (2차 B4 · 번개의 호흡) are the outermost on `ay.a(I)S`.
            if !h2_shipped && h2_gear && !gear_tried && f >= 30 && buff_refresh_tried && (berserker_tried || !h2_berserker) && (guardian_tried || !h2_guardian) && (hell_drop_tried || !h2_hell_drop) && (cd_clock_tried || !h2_cd_clock) && (rogue_tried || !h2_rogue) {
                gear_tried = true;
                if let (Some(patch), Some(cp), Some(gp)) = (GuestBuild::Hero2Ktf.gear_patch(), GuestBuild::Hero2Ktf.combat_patch(), GuestBuild::Hero2Ktf.guardian_patch()) {
                    let hook_word = combat_block.and_then(|c| guest_build::h2_iron_hook_at(&cp).map(|h| c + h));
                    let guardian_ptr = guardian_block.and_then(|g| guest_build::h2_guardian_word_write(&gp, g, guest_build::GuardianPatch::W_GEAR_PTR, 0).map(|(at, _)| at));
                    let venom_ptr = venom_block.map(|v| v + guest_build::VenomPatch::GEAR_PTR);
                    // ★ 2026-10-09 (3): 용의 심장's minute runs on the cooldown clock (none: never on).
                    let clock = cd_clock_block.map(|b| b + guest_build::CooldownClockPatch::COUNT);
                    // ★★ 2026-10-15 2차 E: the combat / ilpung blocks' range pointer words
                    let range_ptrs: Vec<u32> = [combat_block.map(|c| c + guest_build::CombatPatch::GEAR_RANGE_PTR), ilpung_block.map(|i| i + guest_build::IlpungPatch::GEAR_RANGE_PTR)].into_iter().flatten().collect();
                    match emu.alloc_guest(guest_build::GearPatch::BLOCK_LEN) {
                        Some(block) => {
                            let plan = {
                                let rd = |a: u32, n: usize| emu.read_memory(a, n);
                                guest_build::h2_gear_chain(&rd, &patch, block, hook_word, None)
                                    .and_then(|chain| guest_build::h2_gear_install_writes(&rd, &patch, block, &chain, hook_word, guardian_ptr, venom_ptr, clock, berserker_block, &range_ptrs).map(|w| (chain, w)))
                            };
                            match plan {
                                Some((chain, writes)) => {
                                    let mut ok = true;
                                    for (a, bytes) in &writes {
                                        ok &= emu.write_memory(*a, bytes);
                                    }
                                    let rd = |a: u32, n: usize| emu.read_memory(a, n);
                                    let installed = guest_build::h2_gear_installed(&rd, &patch, block, &chain, hook_word, guardian_ptr, venom_ptr, clock, berserker_block, &range_ptrs);
                                    eprintln!("[h2-gear] f={f} block={block:#x} {} write(s) ok={ok} installed={installed} hook={hook_word:?} guardian_ptr={guardian_ptr:?} venom_ptr={venom_ptr:?} clock={clock:x?} chain={:x?}", writes.len(), chain.words());
                                    if installed {
                                        gear_block = Some(block);
                                    }
                                }
                                None => eprintln!("[h2-gear] f={f} REFUSED — a wrapped word is not a Thumb address or reads as another patch's"),
                            }
                        }
                        None => eprintln!("[h2-gear] f={f} no guest block"),
                    }
                }
            }
            if let Some(block) = gear_block {
                type GP = guest_build::GearPatch;
                let h2 = GuestBuild::Hero2Ktf.hero2_java().unwrap();
                for (_, items) in h2_gear_wear.iter().filter(|(at, _)| *at == f) {
                    let class = {
                        let rd = |a: u32, n: usize| emu.read_memory(a, n);
                        guest_build::h2_rd_u32(&rd, h2.class_slot).unwrap_or(0) as u8
                    };
                    // ★★ 2026-10-15 2차 F1: the bag's equipment counted at the forcing frame (when a forced item wants it)
                    let counted = if GP::wants_count(items) {
                        let rd = |a: u32, n: usize| emu.read_memory(a, n);
                        guest_build::h2_bag_equipment_count(&rd, &h2).unwrap_or(0)
                    } else {
                        0
                    };
                    let words = GP::effects_for_counted(items, class, counted);
                    let ok = emu.write_memory(block, &words);
                    eprintln!("[h2-gear] f={f} effect words <- {items:?} (forced, class {class}, bag equipment {counted}) ok={ok}");
                    gear_forced = true;
                    gear_seen = words.to_vec();
                    gear_forced_items = Some(items.clone());
                }
                // ★★ 2026-10-16 악의 무기: forced items stay live too (the moment-bound options), every fifth frame
                if let (true, Some(items)) = (f % 5 == 0, gear_forced_items.clone()) {
                    let (live, owed, words, plan) = {
                        let rd = |a: u32, n: usize| emu.read_memory(a, n);
                        let class = guest_build::h2_rd_u32(&rd, h2.class_slot).unwrap_or(0) as u8;
                        let clock = cd_clock_block.map(|b| b + guest_build::CooldownClockPatch::COUNT);
                        let (live, owed) = gear_live.step(&rd, h2.char_slot, &items, class, Some(block), clock, demon_block, berserker_block);
                        let counted = if GP::wants_count(&items) { guest_build::h2_bag_equipment_count(&rd, &h2).unwrap_or(0) } else { 0 };
                        let words = GP::effects_for_live(&items, class, counted, live).to_vec();
                        let plan = (owed > 0).then(|| guest_build::h2_malice_value(&rd, &h2).map(|v| (v, guest_build::h2_cooldown_cut_write(&rd, &h2, v, owed)))).flatten();
                        (live, owed, words, plan)
                    };
                    if live != gear_live_seen {
                        eprintln!("[h2-gear] f={f} live {live:?}");
                        gear_live_seen = live;
                    }
                    if words != gear_seen {
                        emu.write_memory(block, &words);
                        gear_seen = words;
                    }
                    if owed > 0 {
                        match plan {
                            Some((v, Some((at, new)))) => {
                                emu.write_memory(at, &new.to_le_bytes());
                                eprintln!("[h2-gear] f={f} 광마의 대검: 악의 (value {v}) cooldown − {owed} → {new}");
                            }
                            Some((v, None)) => eprintln!("[h2-gear] f={f} 광마의 대검: 악의 (value {v}) − {owed} — not seated or not running"),
                            None => eprintln!("[h2-gear] f={f} 광마의 대검: − {owed} but no 악의 on the beast"),
                        }
                    }
                }
                if !gear_forced && f % 5 == 0 {
                    let rd = |a: u32, n: usize| emu.read_memory(a, n);
                    let in_game = match build().app_state() {
                        Some(AppState::Absolute { addr, .. }) => guest_build::h2_rd_u32(&rd, addr).is_some_and(|s| s == 3 || s == 5),
                        _ => false,
                    };
                    let class = guest_build::h2_rd_u32(&rd, h2.class_slot).unwrap_or(0) as u8;
                    let worn = if in_game { guest_build::h2_worn_all(&rd, h2.char_slot) } else { Vec::new() };
                    let counted = if GP::wants_count(&worn) { guest_build::h2_bag_equipment_count(&rd, &h2).unwrap_or(0) } else { 0 };
                    let mut malice_write = None;
                    let live = if in_game {
                        let clock = cd_clock_block.map(|b| b + guest_build::CooldownClockPatch::COUNT);
                        let (live, owed) = gear_live.step(&rd, h2.char_slot, &worn, class, Some(block), clock, demon_block, berserker_block);
                        if owed > 0 {
                            malice_write = guest_build::h2_malice_value(&rd, &h2).and_then(|v| guest_build::h2_cooldown_cut_write(&rd, &h2, v, owed).map(|w| (v, owed, w)));
                        }
                        live
                    } else {
                        gear_live.reset();
                        Default::default()
                    };
                    let words = GP::effects_for_live(&worn, if in_game { class } else { 0xff }, counted, live).to_vec();
                    if words != gear_seen {
                        let ok = emu.write_memory(block, &words);
                        let ours: Vec<(u8, u8)> = worn.into_iter().filter(|(t, i)| GP::item(*t, *i).is_some()).collect();
                        eprintln!("[h2-gear] f={f} effect words rewritten — worn options {ours:?} (detected, class {class}) ok={ok}");
                        gear_seen = words;
                    }
                    if let Some((v, owed, (at, new))) = malice_write {
                        emu.write_memory(at, &new.to_le_bytes());
                        eprintln!("[h2-gear] f={f} 광마의 대검: 악의 (value {v}) cooldown − {owed} → {new}");
                    }
                }
                // ★ 2026-10-09 (3): 용의 심장's buff and the seal's resets, when they move.
                if f % 5 == 0 {
                    let rd = |a: u32, n: usize| emu.read_memory(a, n);
                    let now = (guest_build::h2_gear_dragon_left(&rd, block), guest_build::h2_gear_resets(&rd, block), guest_build::h2_gear_drunk_left(&rd, block));
                    if now != gear_procs {
                        eprintln!("[h2-gear] f={f} 용의 심장 frames left {:?} · 현자의 인장 resets {:?} · ★★ 술꾼 updates left {:?}", now.0, now.1, now.2);
                        gear_procs = now;
                    }
                    // ★★ 2026-10-16 악의 무기 M5: 짐승의 박자's window · its 둔기강타 on · M1's hit count
                    type GP = guest_build::GearPatch;
                    let beat = (guest_build::h2_rd_u32(&rd, block + GP::BEAT_UNTIL), guest_build::h2_rd_u32(&rd, block + GP::BEAT_ON));
                    if beat != gear_beat_seen {
                        let clock = cd_clock_block.and_then(|b| guest_build::h2_rd_u32(&rd, b + guest_build::CooldownClockPatch::COUNT));
                        eprintln!("[h2-gear] f={f} 짐승의 박자 until {:?} (clock {clock:?}) · on {:?} · hits {:?}", beat.0, beat.1, guest_build::h2_rd_u32(&rd, block + GP::HIT_COUNT));
                        gear_beat_seen = beat;
                    }
                }
            }
            // ── ★ 2026-10-06: the rogue block (guest_anchors::RoguePatch) — 번개난타's slot is 살상연희 (the 5×5's nearest,
            //    behind each in turn, see-through and untouchable, 10 s); a rogue with 심안 (a.p[12] ≥ 1) sees the ghosts at
            //    half alpha. Installed as the app does: after 일풍격 (it chains onto its words), `--h2-shipped 1` skips it with
            //    the other balance patches. ──
            if !h2_shipped && h2_rogue && !rogue_tried && f >= 30 && (ilpung_tried || !h2_ilpung) {
                rogue_tried = true;
                if let (Some(patch), Some(acc)) = (guest_build::GuestBuild::Hero2Ktf.rogue_patch(), guest_build::GuestBuild::Hero2Ktf.accessory_patch()) {
                    match emu.alloc_guest(guest_build::RoguePatch::BLOCK_LEN) {
                        Some(block) => {
                            let plan = {
                                let rd = |a: u32, n: usize| emu.read_memory(a, n);
                                guest_build::h2_rogue_install_writes(&rd, &patch, &acc, block)
                            };
                            match plan {
                                Some(writes) => {
                                    let mut ok = true;
                                    for (a, bytes) in &writes {
                                        ok &= emu.write_memory(*a, bytes);
                                    }
                                    let rd = |a: u32, n: usize| emu.read_memory(a, n);
                                    let installed = guest_build::h2_rogue_installed(&rd, &patch, &acc, block);
                                    if installed {
                                        rogue_block = Some(block);
                                    }
                                    eprintln!("[h2-rogue] f={f} block={block:#x} {} write(s) ok={ok} installed={installed}", writes.len());
                                }
                                None => eprintln!("[h2-rogue] f={f} REFUSED — bd.a(II)V is not as shipped, or a chained definition is not a Thumb entry"),
                            }
                        }
                        None => eprintln!("[h2-rogue] f={f} no guest block"),
                    }
                }
            }
            // ★★ 2026-10-14 신규 아이템 2차 D: the worn 살상연희 weapons / ring → the rogue block's gear words (the app's
            //    `Rogue::poll`); `--h2-gear-wear` forces them as it does the gear block's.
            if let Some(block) = rogue_block {
                type RP = guest_build::RoguePatch;
                let h2 = GuestBuild::Hero2Ktf.hero2_java().unwrap();
                let rd = |a: u32, n: usize| emu.read_memory(a, n);
                let class = guest_build::h2_rd_u32(&rd, h2.class_slot).unwrap_or(0) as u8;
                let mut words = None;
                for (_, items) in h2_gear_wear.iter().filter(|(at, _)| *at == f) {
                    words = Some(RP::gear_words(items, class));
                    rogue_gear_forced = true;
                }
                if words.is_none() && !rogue_gear_forced && f % 5 == 0 {
                    let in_game = match build().app_state() {
                        Some(AppState::Absolute { addr, .. }) => guest_build::h2_rd_u32(&rd, addr).is_some_and(|s| s == 3 || s == 5),
                        _ => false,
                    };
                    let worn = if in_game { guest_build::h2_worn_all(&rd, h2.char_slot) } else { Vec::new() };
                    words = Some(RP::gear_words(&worn, if in_game { class } else { 0xff }));
                }
                if let Some(w) = words.filter(|w| *w != rogue_gear_seen) {
                    let bytes: Vec<u8> = w.iter().flat_map(|v| v.to_le_bytes()).collect();
                    let ok = emu.write_memory(block + RP::GEAR_HITS, &bytes);
                    eprintln!("[h2-rogue] f={f} 살상연희 gear words {w:?} (class {class}) ok={ok}");
                    rogue_gear_seen = w;
                }
            }
            // ── ★ 2026-10-11: the teleport gate (guest_anchors::TeleportGatePatch) — after the rogue block, as the app; MASK
            //    from the scene (맵 18 · event 0x10 on · 0x11 off) and the accessory block's ACTIVE every 5 frames. ──
            if !h2_shipped && h2_teleport_gate && !tgate_tried && f >= 30 && (rogue_tried || !h2_rogue) {
                tgate_tried = true;
                if let Some(patch) = guest_build::GuestBuild::Hero2Ktf.teleport_gate_patch() {
                    match emu.alloc_guest(guest_build::TeleportGatePatch::BLOCK_LEN) {
                        Some(block) => {
                            let plan = {
                                let rd = |a: u32, n: usize| emu.read_memory(a, n);
                                guest_build::h2_teleport_gate_install_writes(&rd, &patch, block)
                            };
                            match plan {
                                Some(writes) => {
                                    let mut ok = true;
                                    for (a, bytes) in &writes {
                                        ok &= emu.write_memory(*a, bytes);
                                    }
                                    let rd = |a: u32, n: usize| emu.read_memory(a, n);
                                    let installed = guest_build::h2_teleport_gate_installed(&rd, &patch, block);
                                    if installed {
                                        tgate = Some((block, 0));
                                    }
                                    eprintln!("[h2-tgate] f={f} block={block:#x} {} write(s) ok={ok} installed={installed}", writes.len());
                                }
                                None => eprintln!("[h2-tgate] f={f} REFUSED — ay.b(B)Z's word 0 is not a Thumb entry"),
                            }
                        }
                        None => eprintln!("[h2-tgate] f={f} no guest block"),
                    }
                }
            }
            if let (Some((block, last)), true, Some(sv)) = (tgate, f % 5 == 0, GuestBuild::Hero2Ktf.hero2_save()) {
                type TG = guest_build::TeleportGatePatch;
                let rd = |a: u32, n: usize| emu.read_memory(a, n);
                let scene = guest_build::h2_stage_map_no(&rd, &sv.area).zip(guest_build::h2_event_flags(&rd, &sv.area));
                let active = acc_block.and_then(|(b, _)| guest_build::h2_rd_u32(&rd, b + guest_build::AccessoryPatch::ACTIVE)).unwrap_or(0);
                // ★ 2026-10-15: the app's rule — the corpses stand until the next map load, 0x11 or not.
                let corpses = guest_build::h2_npc_rec_shown(&rd, sv.area.stage_slot, TG::CORPSE_REC);
                let mask = match &scene {
                    Some((map, flags)) if TG::fenced(*map, flags, || corpses) => TG::kinds_mask(active),
                    _ => 0,
                };
                if mask != last {
                    let ok = emu.write_memory(block + TG::MASK, &mask.to_le_bytes());
                    let searched = scene.as_ref().map(|(_, fl)| guest_build::h2_flag_on(fl, TG::EVENT_OFF));
                    eprintln!(
                        "[h2-tgate] f={f} MASK {last:#x} -> {mask:#x} (map {:?}, 0x11 {searched:?}, corpses shown {corpses:?}) ok={ok}",
                        scene.as_ref().map(|s| s.0)
                    );
                    tgate = Some((block, mask));
                }
            }
            // ── ★ 2026-10-08: 천공의검's picture block (guest_anchors::SkyDrawPatch) — after the rogue block, as the app. ──
            if !h2_shipped && h2_sky_draw && !sky_draw_tried && f >= 30 && (rogue_tried || !h2_rogue) && (berserker_tried || !h2_berserker) {
                sky_draw_tried = true;
                if let Some(patch) = guest_build::GuestBuild::Hero2Ktf.sky_draw_patch() {
                    match emu.alloc_guest(guest_build::SkyDrawPatch::BLOCK_LEN) {
                        Some(block) => {
                            let plan = {
                                let rd = |a: u32, n: usize| emu.read_memory(a, n);
                                let flower = rogue_block.map_or(0, |b| b + guest_build::RoguePatch::FLOWER);
                                guest_build::h2_sky_draw_install_writes(&rd, &patch, block, flower, berserker_block.unwrap_or(0))
                            };
                            match plan {
                                Some(writes) => {
                                    let mut ok = true;
                                    for (a, bytes) in &writes {
                                        ok &= emu.write_memory(*a, bytes);
                                    }
                                    let rd = |a: u32, n: usize| emu.read_memory(a, n);
                                    let installed = guest_build::h2_sky_draw_installed(&rd, &patch, block);
                                    eprintln!("[h2-sky-draw] f={f} block={block:#x} {} write(s) ok={ok} installed={installed}", writes.len());
                                }
                                None => eprintln!("[h2-sky-draw] f={f} REFUSED — a.a(BBII)V's or bf.a(Graphics;II)V's definition is not a Thumb entry outside the block"),
                            }
                        }
                        None => eprintln!("[h2-sky-draw] f={f} no guest block"),
                    }
                }
            }
            // ── ★★ 2026-10-13 「악마화」 D1: the 악의 block — the app's plan, after every block it chains onto (the gear block,
            //    the sky draw, the teleport gate, the berserker, the combat block) ──
            if !h2_shipped && h2_demon && !demon_tried && f >= 30 && (gear_tried || !h2_gear) && (sky_draw_tried || !h2_sky_draw) && (tgate_tried || !h2_teleport_gate) && (berserker_tried || !h2_berserker) && combat_tried {
                demon_tried = true;
                if let (Some(patch), Some(cp)) = (guest_build::GuestBuild::Hero2Ktf.demon_patch(), guest_build::GuestBuild::Hero2Ktf.combat_patch()) {
                    let hook = combat_block.and_then(|c| guest_build::h2_iron_hook_at(&cp).map(|h| c + h));
                    match emu.alloc_guest(guest_build::DemonPatch::BLOCK_LEN) {
                        Some(block) => {
                            let plan = {
                                let rd = |a: u32, n: usize| emu.read_memory(a, n);
                                guest_build::h2_demon_chain(&rd, &patch, block, hook)
                                    .and_then(|chain| guest_build::h2_demon_install_writes(&rd, &patch, block, hook, h2_voice, gear_block, h2_hd > 0, combat_block.map(|c| c + guest_build::CombatPatch::WIND_AT), berserker_block, acc_block.map(|(b, _)| b)).map(|w| (chain, w)))
                            };
                            match plan {
                                Some((chain, writes)) => {
                                    let mut ok = true;
                                    for (a, bytes) in &writes {
                                        ok &= emu.write_memory(*a, bytes);
                                    }
                                    let rd = |a: u32, n: usize| emu.read_memory(a, n);
                                    let installed = guest_build::h2_demon_installed(&rd, &patch, block, &chain, hook);
                                    eprintln!("[h2-demon] f={f} block={block:#x} {} write(s) ok={ok} installed={installed} hook={hook:?} chain={chain:x?}", writes.len());
                                    if installed {
                                        demon_block = Some(block);
                                    }
                                }
                                None => eprintln!("[h2-demon] f={f} REFUSED - a definition / the hook word does not read as expected"),
                            }
                        }
                        None => eprintln!("[h2-demon] f={f} no guest block"),
                    }
                }
            }
            if let Some(block) = demon_block {
                let rd = |a: u32, n: usize| emu.read_memory(a, n);
                let h2 = GuestBuild::Hero2Ktf.hero2_java().unwrap();
                let dp = guest_build::GuestBuild::Hero2Ktf.demon_patch().unwrap();
                let frames_slot = guest_build::GuestBuild::Hero2Ktf.berserker_patch().unwrap().frames_slot;
                let cooldown_slot = guest_build::GuestBuild::Hero2Ktf.gear_patch().unwrap().cooldown_slot;
                let w = guest_build::h2_demon_words(&rd, block).unwrap_or_default();
                // the character: SP · the action (g.t · g.w · a.i · the queued kind) · a.n[0..3] · the beast's slots and the
                // quickbar counters (the slot's 1800 from the cast)
                let character = guest_build::h2_static_handle(&rd, h2.char_slot)
                    .and_then(|ch| guest_build::h2_fields(&rd, ch))
                    .map(|fb| {
                        let rw = |o: u32| guest_build::h2_rd_u32(&rd, fb + 4 + o).map_or(-1, |v| v as i32);
                        let queue = guest_build::h2_rd_u32(&rd, fb + 4 + dp.queue_off).and_then(|h| guest_build::h2_byte_array(&rd, h)).unwrap_or_default();
                        let at = rw(dp.queue_at_off);
                        let kind = usize::try_from(at).ok().and_then(|i| queue.get(i)).map_or(-2, |k| *k as i8 as i32);
                        let frames = guest_build::h2_static_handle(&rd, frames_slot).and_then(|h| guest_build::h2_byte_array(&rd, h)).unwrap_or_default();
                        let beast = guest_build::h2_rd_u32(&rd, fb + 4 + dp.beast_off)
                            .and_then(|b| guest_build::h2_fields(&rd, b))
                            .map(|bfb| {
                                let arr = |o: u32| guest_build::h2_rd_u32(&rd, bfb + 4 + o).and_then(|h| guest_build::h2_byte_array(&rd, h)).unwrap_or_default();
                                format!("j={:?} k={:?} state={}", arr(dp.beast_ids_off), arr(dp.beast_levels_off), guest_build::h2_rd_u32(&rd, bfb + 4 + dp.beast_state_off).map_or(-1, |v| v as i32))
                            })
                            .unwrap_or_else(|| "no beast".to_string());
                        format!("sp={}/{} g.t={} g.w={} a.i={at} kind={kind} a.n[0..3]={:?} beast {beast}", rw(dp.sp_off), rw(dp.max_sp_off), rw(dp.state_off), rw(dp.frame_off), frames.get(..3).unwrap_or(&[]))
                    })
                    .unwrap_or_else(|| "no character".to_string());
                let counters = guest_build::h2_static_handle(&rd, cooldown_slot).and_then(|h| guest_build::h2_array(&rd, h)).map(|(base, len)| (0..len.min(4)).map(|i| guest_build::h2_rd_u32(&rd, base + 4 * i).map_or(-1, |v| v as i32)).collect::<Vec<_>>()).unwrap_or_default();
                // printed when a block word other than the clock moves (a cast, a hit, an end, the recovery) or on the
                // --gauges cadence — not for every tick / animation frame
                // ★ D3: the toggles — BLADE / JUDGE (0 · 1), their hits and last tick damages, the presses, the judgement's wind-up end
                // ★ D4: 추종 (FOLLOW · FUNTIL · FHITS · FGAUGE · BLASTS · BLDMG · LEECH · FOLLOWS) · 돌진 (CHARGE · CUNTIL · CHITS · CDMG ·
                //   CHARGES · PUSHES) · the extra steps and swings
                // ★ D5: 용암불 (LAVAS · LHITS · LDMG · LLIVE) · 멸화 (FIRE · FDUE · FSLOT · FIRE_HITS · FDMG · FIRES · FSEQ · FN · FTICK)
                let key = format!(
                    "STATE={} OWNER={:#x} UNTIL={} LAST={} RECOVER={} HITS={} CASTS={} ENDS={} LOOK={} T1={} T3={} N0={} TSEQ={} BLADE={} JUDGE={} BHITS={} JHITS={} BDMG={} JDMG={} TOG={} JFROM={} FOLLOW={} FUNTIL={} FHITS={} FGAUGE={} FTAKEN={} FDEALT={} BLASTS={} BLDMG={} LEECH={} FOLLOWS={} CHARGE={} CUNTIL={} CHITS={} CDMG={} CHARGES={} PUSHES={} STEPS={} SWINGS={} LAVAS={} LHITS={} LDMG={} LLIVE={} FIRE={} FDUE={} FSLOT={} FIRE_HITS={} FDMG={} FIRES={} FSEQ={} FN={} FTICK={} POSE={} PSTART={} POSES={}",
                    w.state, w.owner, w.until, w.last, w.recover_until, w.hits, w.casts, w.ends, w.look, w.t1_until, w.t3_until, w.human_n0, w.transform_seq, w.blade, w.judge, w.blade_hits, w.judge_hits, w.blade_dmg, w.judge_dmg, w.toggles, w.judge_from,
                    w.follow, w.follow_until, w.follow_hits, w.follow_gauge, w.follow_taken, w.follow_dealt, w.blasts, w.blast_dmg, w.leeched, w.follows, w.charge, w.charge_until, w.charge_hits, w.charge_dmg, w.charges, w.pushes, w.steps, w.swings,
                    w.lavas, w.lava_hits, w.lava_dmg, w.lava_live, w.fire, w.fire_due, w.fire_slot, w.fire_hits, w.fire_dmg, w.fires, w.fire_seq, w.fire_n, w.fire_tick, w.pose_kind, w.pose_start, w.poses_started
                );
                if key != demon_seen || (gauges > 0 && f % gauges == 0) {
                    eprintln!("[h2-demon] f={f} {key} TICKS={} cd={counters:?} {character}", w.ticks);
                    demon_seen = key;
                }
            }
            // ── --h2-cd-flip: a page flip's two halves on one guest slot — the skill leaves the
            //    bar at f1 (its counter parked, as `emulator::seat_cooldown` does) and is seated
            //    again at f2 with what the parked counter has left by the clock ──────────────
            for (i, (f1, f2, g, old)) in h2_cd_flip.iter().copied().enumerate() {
                if f != f1 && f != f2 {
                    continue;
                }
                let now = {
                    let rd = |a: u32, n: usize| emu.read_memory(a, n);
                    cd_clock_block.and_then(|b| guest_build::h2_cd_clock_read(&rd, b)).unwrap_or(0)
                };
                let value = if f == f1 { None } else { cd_flip_off[i] };
                if f == f2 && value.is_none() {
                    eprintln!("[h2-cd] f={f} slot {g}: nothing was taken off at f={f1} — not seated");
                    continue;
                }
                let seat = {
                    let rd = |a: u32, n: usize| emu.read_memory(a, n);
                    guest_build::h2_seat_cooldown(&rd, &h2, g, value)
                };
                let Some(seat) = seat else {
                    eprintln!("[h2-cd] f={f} slot {g}: the bar does not read — skipped");
                    continue;
                };
                if let Some((v, left)) = seat.evicted {
                    cd_window.park(v, left, left, now);
                    cd_flip_off[i] = Some(v);
                    eprintln!("[h2-cd] f={f} slot {g}: skill {v} leaves the bar with cd={left} (clock {now})");
                }
                let left = match value {
                    None => -1,
                    Some(_) if old => -1,
                    Some(v) => seat.moved.unwrap_or_else(|| cd_window.unpark(v, now).map_or(-1, |(left, _)| left)),
                };
                let writes = {
                    let rd = |a: u32, n: usize| emu.read_memory(a, n);
                    guest_build::h2_bind_writes(&rd, &h2, g, value, left)
                };
                let mut ok = true;
                for (a, bytes) in &writes {
                    ok &= emu.write_memory(*a, bytes);
                }
                match value {
                    Some(v) => eprintln!("[h2-cd] f={f} slot {g}: skill {v} seated with cd={left} (clock {now}{}) — {} write(s) ok={ok}", if old { ", the OLD host: ready on every seating" } else { "" }, writes.len()),
                    None => eprintln!("[h2-cd] f={f} slot {g}: cleared — {} write(s) ok={ok}", writes.len()),
                }
            }
            // ── --h2-quickbar: the app's plate-seat writes into a guest quickbar slot ──
            for (qf, g, val) in h2_quickbar.iter().copied() {
                if qf != f {
                    continue;
                }
                let writes = {
                    let rd = |a: u32, n: usize| emu.read_memory(a, n);
                    guest_build::h2_bind_writes(&rd, &h2, g, Some(val), -1)
                };
                if writes.is_empty() {
                    eprintln!("[h2-quickbar] f={f} slot {g} <- {val}: nothing to write (quickbar not resolvable yet, or already {val})");
                }
                for (a, bytes) in &writes {
                    let w = emu.write_memory(*a, bytes);
                    eprintln!("[h2-quickbar] f={f} slot {g} <- {val}: {a:#x} <- {} ({})", hex(bytes), if w { "ok" } else { "REFUSED" });
                }
            }
            // ── --h2-wear: aa.f on an inventory record (★ 2026-10-20) ──
            for (_, idx, slot) in h2_wear.iter().filter(|(at, _, _)| *at == f) {
                // plan with the read closure, then write with the emulator (one borrow at a time)
                let plan: Option<(Vec<u32>, Option<(u32, i8, i8)>)> = {
                    let rd = |a: u32, n: usize| emu.read_memory(a, n);
                    guest_build::h2_bag(&rd, h2.inventory_slot).map(|inv| {
                        let flag_at = |handle: u32| guest_build::h2_fields(&rd, handle).map(|fl| fl + 4 + guest_build::Hero2Java::ITEM_EQUIP_SLOT_OFF);
                        let mut clears = vec![];
                        if *slot >= 0 {
                            for it in &inv.items {
                                if let Some(a) = flag_at(it.handle) {
                                    let cur = i32::from_le_bytes(rd(a, 4).try_into().unwrap_or([0xff; 4]));
                                    if cur == *slot && guest_build::h2_bag_index_of(&rd, inv.handle, it.handle) != Some(*idx) {
                                        clears.push(a);
                                    }
                                }
                            }
                        }
                        let target = guest_build::h2_bag_element(&rd, inv.handle, *idx).and_then(|h| {
                            let it = guest_build::h2_bag_record_at(&rd, inv.handle, *idx);
                            flag_at(h).map(|a| (a, it.as_ref().map(|i| i.kind).unwrap_or(-1), it.as_ref().map(|i| i.id).unwrap_or(-1)))
                        });
                        (clears, target)
                    })
                };
                match plan {
                    None => eprintln!("[h2-wear] f={f} no inventory yet"),
                    Some((clears, target)) => {
                        let mut cleared = 0usize;
                        for a in &clears {
                            cleared += usize::from(emu.write_memory(*a, &(-1i32).to_le_bytes()));
                        }
                        match target {
                            None => eprintln!("[h2-wear] f={f} [{idx}] no record at that index"),
                            Some((a, kind, id)) => {
                                let ok = emu.write_memory(a, &slot.to_le_bytes());
                                eprintln!("[h2-wear] f={f} [{idx}] type={kind} id={id} aa.f <- {slot} ({}) · {cleared} other record(s) taken off slot {slot}", if ok { "ok" } else { "REFUSED" });
                            }
                        }
                    }
                }
            }
            // ── --h2-unworn: the app's `AccountState::clear_worn_flags` on the probe's warehouse ──
            if h2_unworn.contains(&f) {
                match account_bag {
                    None => eprintln!("[h2-unworn] f={f} no warehouse bag yet (needs --h2-account first)"),
                    Some(bag) => {
                        let writes = {
                            let rd = |a: u32, n: usize| emu.read_memory(a, n);
                            guest_build::h2_bag_worn_flag_clears(&rd, bag)
                        };
                        match writes {
                            None => eprintln!("[h2-unworn] f={f} the warehouse bag could not be read"),
                            Some(writes) => {
                                let mut ok = 0usize;
                                for (a, bytes) in &writes {
                                    ok += usize::from(emu.write_memory(*a, bytes));
                                }
                                eprintln!("[h2-unworn] f={f} {ok}/{} worn flag(s) taken off in the warehouse", writes.len());
                            }
                        }
                    }
                }
            }
            // ── --h2-acct-front: the app's 재료가방-at-cell-0 rule on the probe's warehouse ──
            if h2_acct_front.contains(&f) {
                match (account_bag, account_step.is_some()) {
                    (None, _) => eprintln!("[h2-acct-front] f={f} no warehouse bag yet (needs --h2-account first)"),
                    (_, true) => eprintln!("[h2-acct-front] f={f} a step is in flight"),
                    (Some(bag), false) => {
                        let kinds = {
                            let rd = |a: u32, n: usize| emu.read_memory(a, n);
                            guest_build::h2_bag_of(&rd, bag).map(|b| b.items.iter().map(|i| i.kind).collect::<Vec<_>>())
                        };
                        eprintln!("[h2-acct-front] f={f} bag {bag:#x} before: {kinds:?}");
                        let t = guest_build::Hero2Java::ITEM_TYPE_MATERIAL_BAG as i32;
                        let args = [JavaArg::Int(t), JavaArg::Int(0), JavaArg::Int(1), JavaArg::Int(0)];
                        match h2_call(&mut *emu, guest_build::JavaCall::RecordNew, None, &args) {
                            Some(out) => account_step = Some(AccountStep::FrontNew { out }),
                            None => eprintln!("[h2-acct-front] f={f} RecordNew refused"),
                        }
                    }
                }
            }
            // ── --h2-account: the app's warehouse-window steps ─────────────────────────
            if h2_account.contains(&f) {
                let menu = {
                    let rd = |a: u32, n: usize| emu.read_memory(a, n);
                    guest_build::h2_menu_on_account_row(&rd, &h2)
                };
                match (menu, account_bag, account_step.is_some()) {
                    (None, _, _) => eprintln!("[h2-account] f={f} the menu is not alone on the ACCOUNT row — nothing to open"),
                    (_, _, true) => eprintln!("[h2-account] f={f} a step is in flight"),
                    (Some(menu), None, false) => {
                        let cap = guest_build::Hero2Java::BAG_CAPACITY as i32;
                        if let Some(out) = h2_call(&mut *emu, guest_build::JavaCall::BagNew, None, &[JavaArg::Int(cap)]) {
                            account_step = Some(AccountStep::NewBag { out, menu });
                        }
                    }
                    (Some(menu), Some(bag), false) => {
                        let args = [JavaArg::Object(menu.handle), JavaArg::Object(bag), JavaArg::Int(0)];
                        if let Some(out) = h2_call(&mut *emu, guest_build::JavaCall::InventoryWindowNew, None, &args) {
                            account_step = Some(AccountStep::NewWindow { out, menu });
                        }
                    }
                }
            }
            match account_step.take() {
                None => {}
                Some(AccountStep::NewBag { out, menu }) => match out.poll() {
                    None => account_step = Some(AccountStep::NewBag { out, menu }),
                    Some(Err(())) => eprintln!("[h2-account] f={f} bq(127) FAILED"),
                    Some(Ok(bag)) => {
                        eprintln!("[h2-account] f={f} warehouse bag {bag:#x} (pinned)");
                        account_bag = Some(bag);
                        let args = [JavaArg::Object(menu.handle), JavaArg::Object(bag), JavaArg::Int(0)];
                        if let Some(out) = h2_call(&mut *emu, guest_build::JavaCall::InventoryWindowNew, None, &args) {
                            account_step = Some(AccountStep::NewWindow { out, menu });
                        }
                    }
                },
                Some(AccountStep::FrontNew { out }) => match out.poll() {
                    None => account_step = Some(AccountStep::FrontNew { out }),
                    Some(Err(())) | Some(Ok(0)) => eprintln!("[h2-acct-front] f={f} ch.a(BBZZ) FAILED"),
                    Some(Ok(rec)) => match account_bag.and_then(|bag| h2_call(&mut *emu, guest_build::JavaCall::BagInsertRecord, Some(bag), &[JavaArg::Object(rec), JavaArg::Int(1)])) {
                        Some(out) => account_step = Some(AccountStep::FrontInsert { out, rec }),
                        None => {
                            emu.release_java_object(rec);
                            eprintln!("[h2-acct-front] f={f} BagInsertRecord refused");
                        }
                    },
                },
                Some(AccountStep::FrontInsert { out, rec }) => match out.poll() {
                    None => account_step = Some(AccountStep::FrontInsert { out, rec }),
                    Some(done) => {
                        emu.release_java_object(rec);
                        eprintln!("[h2-acct-front] f={f} bq.a(Lch;I)Z(재료가방, 1) -> {done:?} (appended)");
                        if let Some(bag) = account_bag {
                            let (at, writes, kinds) = {
                                let rd = |a: u32, n: usize| emu.read_memory(a, n);
                                let at = guest_build::h2_bag_index_of_kind(&rd, bag, guest_build::Hero2Java::ITEM_TYPE_MATERIAL_BAG);
                                let writes = at.filter(|k| *k > 0).and_then(|k| guest_build::h2_bag_front_writes(&rd, bag, k));
                                let kinds = guest_build::h2_bag_of(&rd, bag).map(|b| b.items.iter().map(|i| i.kind).collect::<Vec<_>>());
                                (at, writes, kinds)
                            };
                            eprintln!("[h2-acct-front] f={f} appended: {kinds:?} — 재료가방 at index {at:?}, {} slot write(s) to rotate", writes.as_ref().map_or(0, Vec::len));
                            if let Some(w) = writes {
                                for (a, bytes) in &w {
                                    let ok = emu.write_memory(*a, bytes);
                                    eprintln!("[h2-acct-front]   BagOrder {a:#x} <- {} ({})", hex(bytes), if ok { "ok" } else { "REFUSED" });
                                }
                            }
                            let (at2, kinds2) = {
                                let rd = |a: u32, n: usize| emu.read_memory(a, n);
                                (
                                    guest_build::h2_bag_index_of_kind(&rd, bag, guest_build::Hero2Java::ITEM_TYPE_MATERIAL_BAG),
                                    guest_build::h2_bag_of(&rd, bag).map(|b| b.items.iter().map(|i| i.kind).collect::<Vec<_>>()),
                                )
                            };
                            eprintln!("[h2-acct-front] f={f} after: {kinds2:?} — 재료가방 at index {at2:?}");
                        }
                    }
                },
                Some(AccountStep::NewWindow { out, menu }) => match out.poll() {
                    None => account_step = Some(AccountStep::NewWindow { out, menu }),
                    Some(Err(())) => eprintln!("[h2-account] f={f} new q FAILED"),
                    Some(Ok(win)) => {
                        let mut ok = true;
                        for (kind, a, bytes) in guest_build::h2_account_open_writes(&menu, win, &h2, true) {
                            let w = emu.write_memory(a, &bytes);
                            eprintln!("[h2-account]   {kind:?} {a:#x} <- {} ({})", hex(&bytes), if w { "ok" } else { "REFUSED" });
                            ok &= w;
                        }
                        let _ = h2_call(&mut *emu, guest_build::JavaCall::HudDirty, None, &[]);
                        emu.release_java_object(win);
                        let swap = {
                            let rd = |a: u32, n: usize| emu.read_memory(a, n);
                            guest_build::h2_account_title_swap(&rd, &h2)
                        };
                        if let Some((addr, original, replacement)) = swap {
                            let w = emu.write_memory(addr, &replacement.to_le_bytes());
                            eprintln!("[h2-account]   title row 57 {original:#x} -> {replacement:#x} at {addr:#x} ({})", if w { "ok" } else { "REFUSED" });
                            account_title = Some((addr, original));
                        }
                        eprintln!("[h2-account] f={f} window {win:#x} hung on menu {:#x} (ok={ok})", menu.handle);
                    }
                },
            }
            if let (Some((addr, original)), Some(bag)) = (account_title, account_bag) {
                let (up, same) = {
                    let rd = |a: u32, n: usize| emu.read_memory(a, n);
                    (
                        guest_build::h2_bag_window(&rd, &h2, &[bag]).is_some(),
                        guest_build::h2_gm_row_addr(&rd, &h2, guest_build::Hero2Java::GM_ROW_MATERIAL_BAG_TITLE) == Some(addr),
                    )
                };
                if !up {
                    if same {
                        let w = emu.write_memory(addr, &original.to_le_bytes());
                        eprintln!("[h2-account] f={f} window gone — title row put back ({})", if w { "ok" } else { "REFUSED" });
                    } else {
                        eprintln!("[h2-account] f={f} window gone with the menu — nothing to put back");
                    }
                    account_title = None;
                }
            }
            // ── --h2-moverow / --h2-movego: the move row and the move itself ─────────────
            let page_ctx = page_block
                .and_then(|b| {
                    let rd = |a: u32, n: usize| emu.read_memory(a, n);
                    guest_build::h2_page_ctx(&rd, b)
                })
                .unwrap_or((0, 0));
            let mr_bags: Vec<u32> = {
                let rd = |a: u32, n: usize| emu.read_memory(a, n);
                [guest_build::h2_static_handle(&rd, h2.inventory_slot), guest_build::h2_static_handle(&rd, h2.material_bag_slot), account_bag].into_iter().flatten().collect()
            };
            if h2_moverow.contains(&f) && mr_step.is_none() {
                let mut mr_labels: Vec<(u32, Vec<u8>)> = Vec::new();
                let (popup, plan) = {
                    let rd = |a: u32, n: usize| emu.read_memory(a, n);
                    match guest_build::h2_bag_popup_walk(&rd, &h2, &mr_bags, page_ctx) {
                        Some(p) => {
                            // The APP's own order (#22): a 망각의 구 gets its reset row
                            // first, then the move row — one per call, exactly as
                            // `AccountState::poll_popup` does it. ★★ 2026-10-17 an elixir in the
                            // inventory: 퀵슬롯등록 / 해제 between them, their labels relabelled first.
                            let mut wanted = guest_build::h2_popup_wanted_rows(&rd, &h2, &p, &[account_bag, None]);
                            let plate_rows = [guest_build::Hero2Java::GM_ROW_BIND, guest_build::Hero2Java::GM_ROW_UNBIND];
                            if wanted.contains(&plate_rows[0]) {
                                match guest_build::h2_register_label_writes(&rd, plate_rows.map(|r| guest_build::h2_gm_row_handle(&rd, &h2, r))) {
                                    Some(w) => mr_labels = w,
                                    None => wanted.retain(|r| !plate_rows.contains(r)),
                                }
                            }
                            let mut plan = None;
                            for row in wanted {
                                match guest_build::h2_move_row_plan(&rd, &h2, &p, row) {
                                    guest_build::H2MoveRowPlan::Present(_) => continue,
                                    other => {
                                        plan = Some(other);
                                        break;
                                    }
                                }
                            }
                            let fallback = guest_build::H2MoveRowPlan::Present(p.labels_len as u8 - 1);
                            (Some(p), Some(plan.unwrap_or(fallback)))
                        }
                        None => (None, None),
                    }
                };
                for (a, b) in &mr_labels {
                    let ok = emu.write_memory(*a, b);
                    eprintln!("[h2-moverow] f={f} plate-row label at {a:#x} relabelled (ok={ok})");
                }
                match (popup, plan) {
                    (None, _) | (_, None) => eprintln!("[h2-moverow] f={f} no bag-window popup is up"),
                    (Some(p), Some(plan)) => match plan {
                        guest_build::H2MoveRowPlan::Present(row) => eprintln!("[h2-moverow] f={f} present at row {row} (popup {:#x} kind {})", p.handle, p.kind),
                        guest_build::H2MoveRowPlan::Refused => eprintln!("[h2-moverow] f={f} refused (popup {:#x} kind {} labels {:x?})", p.handle, p.kind, p.labels),
                        guest_build::H2MoveRowPlan::Slack { row, writes } => {
                            let mut ok = true;
                            for (a, bytes) in &writes {
                                ok &= emu.write_memory(*a, bytes);
                            }
                            eprintln!("[h2-moverow] f={f} appended in slack at row {row} ({} write(s) ok={ok}) on popup {:#x} kind {}", writes.len(), p.handle, p.kind);
                        }
                        guest_build::H2MoveRowPlan::NeedArray { gm_rows } => {
                            let args: Vec<JavaArg> = gm_rows.iter().map(|r| JavaArg::Int(*r as i32)).collect();
                            let call = if gm_rows.len() == 3 { guest_build::JavaCall::TableRows3 } else { guest_build::JavaCall::TableRows2 };
                            if let Some(out) = h2_call(&mut *emu, call, None, &args) {
                                eprintln!("[h2-moverow] f={f} no slack — the table makes {:?}", gm_rows);
                                mr_step = Some(MrStep::Array { out, popup: p.handle, len: gm_rows.len() as u32 });
                            }
                        }
                    },
                }
            }
            if h2_movego.contains(&f) && mr_step.is_none() {
                let rd = |a: u32, n: usize| emu.read_memory(a, n);
                match guest_build::h2_bag_popup_walk(&rd, &h2, &mr_bags, page_ctx) {
                    None => eprintln!("[h2-movego] f={f} no bag-window popup is up"),
                    Some(p) => {
                        let game = [guest_build::h2_static_handle(&rd, h2.inventory_slot), guest_build::h2_static_handle(&rd, h2.material_bag_slot)];
                        let dest = guest_build::h2_move_destination(&p, &game, &[account_bag, None]);
                        let to_account = dest.is_some_and(|(_, t)| t);
                        let worn = guest_build::h2_record_worn(&rd, p.record.handle);
                        // ★ 2026-09-26: the app's refusal — a WORN record stays out of the
                        //   warehouse, and the game's own box says so over the popup.
                        if to_account && worn != Some(false) {
                            eprintln!("[h2-movego] f={f} record {:#x} (type {} id {}) is WORN ({worn:?}) — refused, notice gm {}", p.record.handle, p.record.kind, p.record.id, guest_build::Hero2Java::GM_ROW_NOTE_EQUIPPED);
                            let text = guest_build::h2_gm_row_handle(&rd, &h2, guest_build::Hero2Java::GM_ROW_NOTE_EQUIPPED);
                            mr_step = h2_probe_notice(&mut *emu, p.window.win.handle, Some(p.handle), text, f);
                        } else {
                            match dest.and_then(|(d, _)| guest_build::h2_bag_head_of(&rd, d).map(|hd| (d, hd))) {
                                None => eprintln!("[h2-movego] f={f} no destination bag"),
                                // ★ 2026-10-13: a stack goes through the game's merge (the app's `MoveStart::Merge`):
                                //   `--h2-move`'s split machine, the room question `bq.a(Lch;B)I`'s. Only the data move is
                                //   mirrored here — the popup stays up (the app closes it and says 「…넣었습니다」).
                                Some((dst, _)) if guest_build::h2_move_merges(p.record.kind) && move_step.is_none() => {
                                    let args = [JavaArg::Int(i32::from(p.record.kind)), JavaArg::Int(i32::from(p.record.id)), JavaArg::Int(1), JavaArg::Int(0)];
                                    if let Some(out) = h2_call(&mut *emu, guest_build::JavaCall::RecordNew, None, &args) {
                                        eprintln!("[h2-movego] f={f} stack {:#x} (type {} id {} x{}) -> {dst:#x} through the merge", p.record.handle, p.record.kind, p.record.id, p.record.count);
                                        move_step = Some(MoveStep::SplitNew { src: p.window.bag, rec: p.record.handle, dst, n: p.record.count.clamp(0, 99) as u8, out, since: f });
                                    }
                                }
                                Some((_, hd)) if !guest_build::h2_bag_has_room(&hd) => {
                                    eprintln!("[h2-movego] f={f} destination full ({} of {}) — notice 빈공간이 없습니다", hd.count, hd.b);
                                    let text = guest_build::h2_bag_full_text(&rd);
                                    mr_step = h2_probe_notice(&mut *emu, p.window.win.handle, Some(p.handle), text, f);
                                }
                                Some((dst, _)) => {
                                    let clear = if to_account { None } else { guest_build::h2_worn_flag_clear(&rd, p.record.handle) };
                                    if let Some((a, bytes)) = clear {
                                        let ok = emu.write_memory(a, &bytes);
                                        eprintln!("[h2-movego] f={f} record {:#x} lost a stale worn flag on its way out ({})", p.record.handle, if ok { "ok" } else { "REFUSED" });
                                    }
                                    let args = [JavaArg::Object(p.record.handle), JavaArg::Int(p.record.count)];
                                    if let Some(out) = h2_call(&mut *emu, guest_build::JavaCall::BagInsertRecord, Some(dst), &args) {
                                        eprintln!("[h2-movego] f={f} record {:#x} (type {} id {} x{}) [{}] -> {:#x} ({})", p.record.handle, p.record.kind, p.record.id, p.record.count, p.index, dst, if to_account { "warehouse" } else { "inventory" });
                                        mr_step = Some(MrStep::Insert { out, src: p.window.bag, index: p.index, rec: p.record.handle, to_account });
                                    }
                                }
                            }
                        }
                    }
                }
            }
            // ★ 2026-09-29: the app's `AccountState::open_quantity` — the game's 개수 box over
            //   the popup's record, to be hung where the popup is.
            if h2_qty.contains(&f) && mr_step.is_none() {
                let rd = |a: u32, n: usize| emu.read_memory(a, n);
                match guest_build::h2_bag_popup_walk(&rd, &h2, &mr_bags, page_ctx) {
                    None => eprintln!("[h2-qty] f={f} no bag-window popup is up"),
                    Some(p) => {
                        let max = guest_build::h2_qty_ceiling(p.record.count);
                        eprintln!("[h2-qty] f={f} 개수 box for record {:#x} (type {} id {} x{}), max {max}", p.record.handle, p.record.kind, p.record.id, p.record.count);
                        let args = [JavaArg::Object(p.window.win.handle), JavaArg::Int(0), JavaArg::Int(max)];
                        if let Some(out) = h2_call(&mut *emu, guest_build::JavaCall::QuantityPopupNew, None, &args) {
                            mr_step = Some(MrStep::Qty { out, window: p.window, record: p.record.handle });
                        }
                    }
                }
            }
            // ★★ 2026-10-20: the craft guard's 「빈공간이 없습니다.」 box — hung on the crafting window once built.
            if let Some(MrStep::Notice { out, window, replaces }) = craft_step.take() {
                match out.poll() {
                    None => craft_step = Some(MrStep::Notice { out, window, replaces }),
                    Some(Err(())) | Some(Ok(0)) => eprintln!("[h2-craft] f={f} new ba(..) FAILED"),
                    Some(Ok(bx)) => {
                        let writes = {
                            let rd = |a: u32, n: usize| emu.read_memory(a, n);
                            build().hero2_java().and_then(|h2| guest_build::h2_notice_target(&rd, &h2, window, replaces).map(|(win, chain)| guest_build::h2_child_open_writes(&win, bx, &chain)))
                        };
                        match writes {
                            Some(writes) => {
                                let mut ok = true;
                                for (_, a, bytes) in &writes {
                                    ok &= emu.write_memory(*a, bytes);
                                }
                                eprintln!("[h2-craft] f={f} the game's box {bx:#x} hung on the crafting window {window:#x} ({} write(s) ok={ok})", writes.len());
                            }
                            None => eprintln!("[h2-craft] f={f} the crafting window {window:#x} changed — box dropped"),
                        }
                        emu.release_java_object(bx);
                    }
                }
            }
            match mr_step.take() {
                None => {}
                Some(MrStep::Qty { out, window, record }) => match out.poll() {
                    None => mr_step = Some(MrStep::Qty { out, window, record }),
                    Some(Err(())) | Some(Ok(0)) => eprintln!("[h2-qty] f={f} new aq(..) FAILED"),
                    Some(Ok(popup)) => {
                        let writes = {
                            let rd = |a: u32, n: usize| emu.read_memory(a, n);
                            guest_build::h2_bag_popup_walk(&rd, &h2, &mr_bags, page_ctx)
                                .filter(|p| p.window.win.handle == window.win.handle && p.record.handle == record)
                                .and_then(|_| guest_build::h2_window_chain(&rd, &h2))
                                .map(|chain| guest_build::h2_qty_open_writes(&window, popup, &chain))
                        };
                        match writes {
                            Some(writes) => {
                                let mut ok = true;
                                for (_, a, bytes) in &writes {
                                    ok &= emu.write_memory(*a, bytes);
                                }
                                eprintln!("[h2-qty] f={f} 개수 box {popup:#x} hung on window {:#x} ({} write(s) ok={ok})", window.win.handle, writes.len());
                            }
                            None => eprintln!("[h2-qty] f={f} the popup went away — box dropped"),
                        }
                        emu.release_java_object(popup);
                    }
                },
                Some(MrStep::Array { out, popup, len }) => match out.poll() {
                    None => mr_step = Some(MrStep::Array { out, popup, len }),
                    Some(Err(())) | Some(Ok(0)) => eprintln!("[h2-moverow] f={f} the table did not make the array"),
                    Some(Ok(arr)) => {
                        let attach = {
                            let rd = |a: u32, n: usize| emu.read_memory(a, n);
                            guest_build::h2_bag_popup_walk(&rd, &h2, &mr_bags, page_ctx)
                                .filter(|p| p.handle == popup && p.labels_len + 1 == len)
                                .map(|p| guest_build::h2_move_row_attach_writes(&p, arr, len))
                        };
                        match attach {
                            Some(writes) => {
                                let mut ok = true;
                                for (a, bytes) in &writes {
                                    ok &= emu.write_memory(*a, bytes);
                                }
                                eprintln!("[h2-moverow] f={f} attached array {arr:#x} of {len} rows on popup {popup:#x} ({} write(s) ok={ok})", writes.len());
                            }
                            None => eprintln!("[h2-moverow] f={f} popup changed — array dropped"),
                        }
                        emu.release_java_object(arr);
                    }
                },
                Some(MrStep::Insert { out, src, index, rec, to_account }) => match out.poll() {
                    None => mr_step = Some(MrStep::Insert { out, src, index, rec, to_account }),
                    Some(Err(())) => eprintln!("[h2-movego] f={f} insert FAILED"),
                    Some(Ok(0)) => eprintln!("[h2-movego] f={f} destination refused"),
                    Some(Ok(_)) => match h2_call(&mut *emu, guest_build::JavaCall::BagRemoveAt, Some(src), &[JavaArg::Int(index as i32)]) {
                        Some(out) => mr_step = Some(MrStep::Remove { out, src, rec, to_account }),
                        None => eprintln!("[h2-movego] f={f} remove refused — record {rec:#x} in BOTH bags"),
                    },
                },
                // ★ 2026-09-26: the app's `AccountState::notice_after` — the popup is stale
                //   now, so it is closed at once the way `j.b(I)V` closes a child, and the
                //   game's box goes up over the bare window when it is built.
                Some(MrStep::Remove { out, src, rec, to_account }) => match out.poll() {
                    None => mr_step = Some(MrStep::Remove { out, src, rec, to_account }),
                    Some(Err(())) => eprintln!("[h2-movego] f={f} remove FAILED — record {rec:#x} in BOTH bags"),
                    Some(Ok(_)) => {
                        let close = {
                            let rd = |a: u32, n: usize| emu.read_memory(a, n);
                            guest_build::h2_bag_window(&rd, &h2, &[src]).and_then(|v| {
                                let chain = guest_build::h2_window_chain(&rd, &h2)?;
                                Some((v.win.handle, guest_build::h2_child_open_writes(&v.win, 0, &chain)))
                            })
                        };
                        match close {
                            Some((window, writes)) => {
                                for (_, a, bytes) in &writes {
                                    emu.write_memory(*a, bytes);
                                }
                                let _ = h2_call(&mut *emu, guest_build::JavaCall::HudDirty, None, &[]);
                                let row = if to_account { guest_build::Hero2Java::GM_ROW_NOTE_TO_ACCOUNT } else { guest_build::Hero2Java::GM_ROW_NOTE_TO_BAG };
                                let text = {
                                    let rd = |a: u32, n: usize| emu.read_memory(a, n);
                                    guest_build::h2_gm_row_handle(&rd, &h2, row)
                                };
                                eprintln!("[h2-movego] f={f} moved record {rec:#x}; popup closed ({} write(s)), notice gm {row}", writes.len());
                                mr_step = h2_probe_notice(&mut *emu, window, None, text, f);
                            }
                            None => eprintln!("[h2-movego] f={f} moved record {rec:#x}; no window shows the source any more"),
                        }
                    }
                },
                Some(MrStep::Notice { out, window, replaces }) => match out.poll() {
                    None => mr_step = Some(MrStep::Notice { out, window, replaces }),
                    Some(Err(())) | Some(Ok(0)) => eprintln!("[h2-movego] f={f} new ba(..) FAILED"),
                    Some(Ok(bx)) => {
                        let writes = {
                            let rd = |a: u32, n: usize| emu.read_memory(a, n);
                            guest_build::h2_notice_target(&rd, &h2, window, replaces).map(|(win, chain)| guest_build::h2_child_open_writes(&win, bx, &chain))
                        };
                        match writes {
                            Some(writes) => {
                                let mut ok = true;
                                for (_, a, bytes) in &writes {
                                    ok &= emu.write_memory(*a, bytes);
                                }
                                eprintln!("[h2-movego] f={f} the game's box {bx:#x} hung on window {window:#x} (replacing {replaces:x?}, {} write(s) ok={ok})", writes.len());
                            }
                            None => eprintln!("[h2-movego] f={f} window {window:#x} changed — box dropped"),
                        }
                        emu.release_java_object(bx);
                    }
                },
            }
            // ── --h2-acct-roundtrip: serialise the warehouse, load it into a second bag ─
            if h2_acct_rt.contains(&f) {
                match (account_bag, rt_step.is_some()) {
                    (None, _) => eprintln!("[h2-acct-rt] f={f} no warehouse bag yet (needs --h2-account first)"),
                    (_, true) => eprintln!("[h2-acct-rt] f={f} a step is in flight"),
                    (Some(bag), false) => {
                        if let Some(out) = h2_call(&mut *emu, guest_build::JavaCall::BagSerialize, Some(bag), &[]) {
                            rt_step = Some(RtStep::Serialize { out });
                        }
                    }
                }
            }
            match rt_step.take() {
                None => {}
                Some(RtStep::Serialize { out }) => match out.poll() {
                    None => rt_step = Some(RtStep::Serialize { out }),
                    Some(Err(())) | Some(Ok(0)) => eprintln!("[h2-acct-rt] f={f} bq.a()[B FAILED"),
                    Some(Ok(arr)) => {
                        let blob = {
                            let rd = |a: u32, n: usize| emu.read_memory(a, n);
                            guest_build::h2_blob_array(&rd, arr)
                        };
                        emu.release_java_object(arr);
                        match blob {
                            Some(blob) => {
                                eprintln!("[h2-acct-rt] f={f} blob {} bytes: {}…", blob.len(), hex(&blob[..blob.len().min(24)]));
                                let out = std::sync::Arc::new(GuestCallResult::new());
                                emu.new_java_byte_array(&blob, out.clone());
                                rt_step = Some(RtStep::Array { out, blob });
                            }
                            None => eprintln!("[h2-acct-rt] f={f} the blob array could not be read"),
                        }
                    }
                },
                Some(RtStep::Array { out, blob }) => match out.poll() {
                    None => rt_step = Some(RtStep::Array { out, blob }),
                    Some(Err(())) | Some(Ok(0)) => eprintln!("[h2-acct-rt] f={f} byte[] FAILED"),
                    Some(Ok(arr)) => {
                        let cap = guest_build::Hero2Java::BAG_CAPACITY as i32;
                        match h2_call(&mut *emu, guest_build::JavaCall::BagNew, None, &[JavaArg::Int(cap)]) {
                            Some(out) => rt_step = Some(RtStep::NewBag { out, arr, blob }),
                            None => emu.release_java_object(arr),
                        }
                    }
                },
                Some(RtStep::NewBag { out, arr, blob }) => match out.poll() {
                    None => rt_step = Some(RtStep::NewBag { out, arr, blob }),
                    Some(Err(())) | Some(Ok(0)) => {
                        eprintln!("[h2-acct-rt] f={f} second bq FAILED");
                        emu.release_java_object(arr);
                    }
                    Some(Ok(bag2)) => match h2_call(&mut *emu, guest_build::JavaCall::BagDeserialize, Some(bag2), &[JavaArg::Object(arr)]) {
                        Some(out) => rt_step = Some(RtStep::Load { out, arr, bag2, blob }),
                        None => emu.release_java_object(arr),
                    },
                },
                Some(RtStep::Load { out, arr, bag2, blob }) => match out.poll() {
                    None => rt_step = Some(RtStep::Load { out, arr, bag2, blob }),
                    Some(done) => {
                        emu.release_java_object(arr);
                        let rd = |a: u32, n: usize| emu.read_memory(a, n);
                        let a = account_bag.and_then(|h| guest_build::h2_bag_of(&rd, h));
                        let b = guest_build::h2_bag_of(&rd, bag2);
                        let same = match (&a, &b) {
                            (Some(a), Some(b)) => a.items.len() == b.items.len() && a.items.iter().zip(&b.items).all(|(x, y)| (x.kind, x.id, x.count, x.d, x.e, x.f, &x.name) == (y.kind, y.id, y.count, y.d, y.e, y.f, &y.name)),
                            _ => false,
                        };
                        eprintln!(
                            "[h2-acct-rt] f={f} deserialise {:?}: warehouse {} record(s) -> second bag {:?} record(s), blob {} B, records identical = {same}",
                            done.map(|_| ()),
                            a.map_or(0, |v| v.items.len()),
                            b.as_ref().map(|v| v.items.len()),
                            blob.len()
                        );
                        if let Some(b) = b {
                            for (i, it) in b.items.iter().enumerate() {
                                eprintln!("[h2-acct-rt]   [{i:2}] type={:2} id={:3} count={:3} f={}", it.kind, it.id, it.count, it.f);
                            }
                        }
                        emu.release_java_object(bag2);
                    }
                },
            }
            // ── --h2-give: the game's own give-item path, one item per two calls ──────
            if let Some((gf, items)) = &h2_give
                && *gf == f
            {
                give_queue.extend(items.iter().copied());
                eprintln!("[h2-give] f={f} queued {} item(s)", give_queue.len());
            }
            match give_step.take() {
                None => {
                    if let Some((t, i, n)) = give_queue.pop_front() {
                        // `ch.a(type, id, 1, 1)` — what the PICKUP passes (`ag.a()Z` @0x1122a6: r3 = 1,
                        // [sp] = 1). ⛔ 2026-10-03: this was `(…, 1, 0)` (`bq.a(Lch;B)I`'s stack-merge
                        // arguments) and the last flag gates the gear's OPTIONS — the accessory parser
                        // `z.a([BIZZ)I` skips its stat block when it is 0 — so every piece of gear this
                        // flag gave (the phones' new six on 10-03) came with no stats at all.
                        let args = [JavaArg::Int(t as i32), JavaArg::Int(i as i32), JavaArg::Int(1), JavaArg::Int(1)];
                        match h2_call(&mut *emu, guest_build::JavaCall::RecordNew, None, &args) {
                            Some(out) => give_step = Some(GiveStep::New { t, i, n, out, since: f }),
                            None => eprintln!("[h2-give] f={f} type {t} id {i}: RecordNew refused"),
                        }
                    }
                }
                Some(GiveStep::New { t, i, n, out, since }) => match out.poll() {
                    None => give_step = Some(GiveStep::New { t, i, n, out, since }),
                    Some(Err(())) => eprintln!("[h2-give] f={f} type {t} id {i}: ch.a(BBZZ) FAILED (issued f={since})"),
                    Some(Ok(0)) => eprintln!("[h2-give] f={f} type {t} id {i}: ch.a(BBZZ) returned null"),
                    Some(Ok(rec)) => {
                        let rd = |a: u32, n: usize| emu.read_memory(a, n);
                        // ★ 2026-10-09: into the bag the game's own drop routes the type to (`bf.a(BB)[B` — the 재료가방
                        //   for 23 · 30 · 31 · 32), so a recipe and its ores land where the alchemist looks for them.
                        let slot = guest_build::h2_drop_bag_slot(&h2, t as i8).unwrap_or(h2.inventory_slot);
                        match guest_build::h2_bag(&rd, slot) {
                            Some(inv) => match h2_call(&mut *emu, guest_build::JavaCall::BagAddCount, Some(inv.handle), &[JavaArg::Object(rec), JavaArg::Int(n as i32)]) {
                                Some(out) => give_step = Some(GiveStep::Add { t, i, n, rec, out, since: f }),
                                None => {
                                    eprintln!("[h2-give] f={f} type {t} id {i}: BagAddCount refused");
                                    emu.release_java_object(rec);
                                }
                            },
                            None => {
                                eprintln!("[h2-give] f={f} type {t} id {i}: no inventory to give to");
                                emu.release_java_object(rec);
                            }
                        }
                    }
                },
                Some(GiveStep::Add { t, i, n, rec, out, since }) => match out.poll() {
                    None => give_step = Some(GiveStep::Add { t, i, n, rec, out, since }),
                    Some(done) => {
                        // The factory's record was PINNED when `RecordNew` returned it (the
                        // backend's contract — it had no other reference). Placed or not, the
                        // host is done with it now: in the bag it lives on the bag's Vector,
                        // out of it it is garbage.
                        emu.release_java_object(rec);
                        match done {
                            Err(()) => eprintln!("[h2-give] f={f} type {t} id {i} x{n}: bq.a(Lch;B)I FAILED (issued f={since})"),
                            Ok(placed) => {
                                let rd = |a: u32, n: usize| emu.read_memory(a, n);
                                let size = guest_build::h2_bag(&rd, guest_build::h2_drop_bag_slot(&h2, t as i8).unwrap_or(h2.inventory_slot)).map(|v| v.items.len());
                                eprintln!("[h2-give] f={f} type {t} id {i} x{n}: record {rec:#x} -> placed {} (bag now {:?} records, {} left in queue)", placed as i32, size, give_queue.len());
                            }
                        }
                    }
                },
            }
            // ── ★ 2026-10-09: --h2-wh-give — the 계정창고 file: load it, give into it, serialise it ──
            if let Some((wf, items)) = &h2_wh_give
                && *wf == f
            {
                wh_queue.extend(items.iter().copied());
                let cap = guest_build::Hero2Java::BAG_CAPACITY as i32;
                match h2_call(&mut *emu, guest_build::JavaCall::BagNew, None, &[JavaArg::Int(cap)]) {
                    Some(out) => {
                        eprintln!("[h2-wh] f={f} queued {} item(s); making the warehouse bag (bq({cap}))", wh_queue.len());
                        wh_step = Some(WhStep::NewBag { out });
                    }
                    None => eprintln!("[h2-wh] f={f} BagNew refused"),
                }
            }
            match wh_step.take() {
                None => {}
                Some(WhStep::NewBag { out }) => match out.poll() {
                    None => wh_step = Some(WhStep::NewBag { out }),
                    Some(Err(())) | Some(Ok(0)) => eprintln!("[h2-wh] f={f} bq(127) FAILED"),
                    Some(Ok(bag)) => match h2_wh_in.as_deref().filter(|b| !b.is_empty()) {
                        Some(blob) => {
                            eprintln!("[h2-wh] f={f} bag {bag:#x} (pinned); loading the {} byte blob", blob.len());
                            let out = Arc::new(GuestCallResult::new());
                            emu.new_java_byte_array(blob, out.clone());
                            wh_step = Some(WhStep::Array { out, bag });
                        }
                        None => {
                            eprintln!("[h2-wh] f={f} bag {bag:#x} (pinned); no blob — starting empty");
                            wh_step = wh_next(&mut *emu, bag, &mut wh_queue, f);
                        }
                    },
                },
                Some(WhStep::Array { out, bag }) => match out.poll() {
                    None => wh_step = Some(WhStep::Array { out, bag }),
                    Some(Err(())) | Some(Ok(0)) => eprintln!("[h2-wh] f={f} byte[] FAILED"),
                    Some(Ok(arr)) => match h2_call(&mut *emu, guest_build::JavaCall::BagDeserialize, Some(bag), &[JavaArg::Object(arr)]) {
                        Some(out) => wh_step = Some(WhStep::Load { out, bag, arr }),
                        None => emu.release_java_object(arr),
                    },
                },
                Some(WhStep::Load { out, bag, arr }) => match out.poll() {
                    None => wh_step = Some(WhStep::Load { out, bag, arr }),
                    Some(done) => {
                        emu.release_java_object(arr);
                        let before = {
                            let rd = |a: u32, n: usize| emu.read_memory(a, n);
                            guest_build::h2_bag_of(&rd, bag)
                        };
                        wh_print(&format!("loaded ({:?})", done.map(|_| ())), f, before.as_ref());
                        if done.is_ok() && before.is_some() {
                            wh_step = wh_next(&mut *emu, bag, &mut wh_queue, f);
                        } else {
                            eprintln!("[h2-wh] f={f} the load failed — nothing given, nothing written");
                        }
                    }
                },
                Some(WhStep::Record { out, bag, t, i, n }) => match out.poll() {
                    None => wh_step = Some(WhStep::Record { out, bag, t, i, n }),
                    Some(Err(())) | Some(Ok(0)) => eprintln!("[h2-wh] f={f} type {t} id {i}: ch.a(BBZZ) FAILED — nothing written"),
                    Some(Ok(rec)) => match h2_call(&mut *emu, guest_build::JavaCall::BagAddCount, Some(bag), &[JavaArg::Object(rec), JavaArg::Int(n as i32)]) {
                        Some(out) => wh_step = Some(WhStep::Add { out, bag, t, i, n, rec }),
                        None => {
                            eprintln!("[h2-wh] f={f} type {t} id {i}: BagAddCount refused — nothing written");
                            emu.release_java_object(rec);
                        }
                    },
                },
                Some(WhStep::Add { out, bag, t, i, n, rec }) => match out.poll() {
                    None => wh_step = Some(WhStep::Add { out, bag, t, i, n, rec }),
                    Some(done) => {
                        emu.release_java_object(rec);
                        match done {
                            Ok(placed) if placed as i32 == n as i32 => {
                                eprintln!("[h2-wh] f={f} type {t} id {i} x{n}: placed ({} left)", wh_queue.len());
                                wh_step = wh_next(&mut *emu, bag, &mut wh_queue, f);
                            }
                            other => eprintln!("[h2-wh] f={f} type {t} id {i} x{n}: bq.a(Lch;B)I -> {other:?} — the bag is full? nothing written"),
                        }
                    }
                },
                Some(WhStep::Serialize { out, bag }) => match out.poll() {
                    None => wh_step = Some(WhStep::Serialize { out, bag }),
                    Some(Err(())) | Some(Ok(0)) => eprintln!("[h2-wh] f={f} bq.a()[B FAILED — nothing written"),
                    Some(Ok(arr)) => {
                        let (blob, after) = {
                            let rd = |a: u32, n: usize| emu.read_memory(a, n);
                            (guest_build::h2_blob_array(&rd, arr), guest_build::h2_bag_of(&rd, bag))
                        };
                        emu.release_java_object(arr);
                        wh_print("after", f, after.as_ref());
                        match (blob, h2_wh_out.as_ref()) {
                            (Some(blob), Some(path)) => match std::fs::write(path, &blob) {
                                Ok(()) => eprintln!("[h2-wh] f={f} wrote {} bytes to {}", blob.len(), path.display()),
                                Err(e) => eprintln!("[h2-wh] f={f} writing {}: {e}", path.display()),
                            },
                            (None, _) => eprintln!("[h2-wh] f={f} the blob array could not be read — nothing written"),
                            (_, None) => {}
                        }
                    }
                },
            }
            // ── --h2-move: insert THE SAME OBJECT into the destination, then remove ────
            for &(_mf, sb, db, idx, want) in h2_move.iter().filter(|m| m.0 == f) {
                let rd = |a: u32, n: usize| emu.read_memory(a, n);
                let walk = |b: u8| match b {
                    b'w' => account_bag.and_then(|h| guest_build::h2_bag_of(&rd, h)),
                    b'a' => guest_build::h2_bag(&rd, h2.inventory_slot),
                    b'b' => guest_build::h2_bag(&rd, h2.material_bag_slot),
                    _ => guest_build::h2_bag(&rd, h2.card_bag_slot),
                };
                match (walk(sb), walk(db)) {
                    (Some(src), Some(dst)) => match src.items.get(idx as usize) {
                        // ★ A STACK (★ 2026-10-13: whole or a part of it — `h2_move_merges`, the
                        // app's rule): the game's own split, and NO room pre-check —
                        // `bq.a(Lch;B)I` merges into existing stacks, so it has room a
                        // record count alone would deny, and it answers with how many it
                        // actually placed. `:0` is the old object move instead.
                        Some(it) if want != Some(0) && guest_build::h2_move_merges(it.kind) && it.count > 0 => {
                            let n = want.map_or(it.count, |n| i32::from(n).min(it.count)).min(guest_build::Hero2Java::STACK_MAX) as u8;
                            eprintln!(
                                "[h2-move] f={f} SPLIT {n} of {} from bag {} [{idx}] (type={} id={}) -> bag {}",
                                it.count, sb as char, it.kind, it.id, db as char
                            );
                            let args = [JavaArg::Int(i32::from(it.kind)), JavaArg::Int(i32::from(it.id)), JavaArg::Int(1), JavaArg::Int(0)];
                            match h2_call(&mut *emu, guest_build::JavaCall::RecordNew, None, &args) {
                                Some(out) => move_step = Some(MoveStep::SplitNew { src: src.handle, rec: it.handle, dst: dst.handle, n, out, since: f }),
                                None => eprintln!("[h2-move] f={f} RecordNew refused"),
                            }
                        }
                        // ★ The game's own room check, applied by the host: `bq.a(BBI)Z` →
                        // `bq.a(I)B` says FULL at `size + 1 > b`, i.e. at `size == b`. The raw
                        // insert `bq.a(Lch;I)Z` alone only refuses at `size > b` — measured
                        // 2026-09-21: it took a 29th record into a 28-bag and refused the 30th.
                        // ⛔ The Vector's own elementCount, not `items.len()` (2026-09-21,
                        // review): `h2_bag_of` `continue`s past a record it cannot read, so
                        // the walked list UNDER-counts and this mirror would bless an
                        // insert the device refuses. `h2_bag_has_room` is what the app asks.
                        Some(_) if guest_build::h2_bag_head_of(&rd, dst.handle).is_none_or(|hd| !guest_build::h2_bag_has_room(&hd)) => {
                            eprintln!("[h2-move] f={f} bag {} is full or unreadable ({} of {}) — refused before the call, as the game's room check would", db as char, dst.items.len(), dst.b);
                        }
                        Some(it) => {
                            eprintln!(
                                "[h2-move] f={f} bag {} [{idx}] type={} id={} count={} f={} ({:#x}) -> bag {} ({} records, cap {})",
                                sb as char, it.kind, it.id, it.count, it.f, it.handle, db as char, dst.items.len(), dst.b
                            );
                            match h2_call(&mut *emu, guest_build::JavaCall::BagInsertRecord, Some(dst.handle), &[JavaArg::Object(it.handle), JavaArg::Int(it.count)]) {
                                Some(out) => move_step = Some(MoveStep::Insert { src: src.handle, idx, rec: it.handle, out, since: f }),
                                None => eprintln!("[h2-move] f={f} BagInsertRecord refused"),
                            }
                        }
                        None => eprintln!("[h2-move] f={f} bag {} has no record {idx} ({} records)", sb as char, src.items.len()),
                    },
                    _ => eprintln!("[h2-move] f={f} a bag is not readable yet"),
                }
            }
            match move_step.take() {
                None => {}
                Some(MoveStep::Insert { src, idx, rec, out, since }) => match out.poll() {
                    None => move_step = Some(MoveStep::Insert { src, idx, rec, out, since }),
                    Some(Err(())) => eprintln!("[h2-move] f={f} bq.a(Lch;I)Z FAILED (issued f={since}) — nothing moved"),
                    Some(Ok(0)) => eprintln!("[h2-move] f={f} destination refused the record (full) — nothing moved"),
                    // ⛔ Remove by where the record IS (2026-09-21, review) — the same rule
                    // the app now follows. `idx` was read before the insert was queued and
                    // the guest has run since; removing by a stale index destroys the
                    // neighbour and leaves the moved record in both Vectors. This mirror is
                    // how the move protocol is settled headless, so it has to agree.
                    Some(Ok(_)) => {
                        let rd = |a: u32, n: usize| emu.read_memory(a, n);
                        match guest_build::h2_bag_index_of(&rd, src, rec) {
                            None => eprintln!("[h2-move] f={f} record {rec:#x} is not in the source bag any more — removing nothing"),
                            Some(at) => {
                                if at != idx as u32 {
                                    eprintln!("[h2-move] f={f} record {rec:#x} slid from index {idx} to {at} while the insert was in flight");
                                }
                                match h2_call(&mut *emu, guest_build::JavaCall::BagRemoveAt, Some(src), &[JavaArg::Int(at as i32)]) {
                                    Some(out) => move_step = Some(MoveStep::Remove { rec, out, since: f }),
                                    None => eprintln!("[h2-move] f={f} BagRemoveAt refused — record {rec:#x} is now in BOTH bags"),
                                }
                            }
                        }
                    }
                },
                Some(MoveStep::SplitNew { src, rec, dst, n, out, since }) => match out.poll() {
                    None => move_step = Some(MoveStep::SplitNew { src, rec, dst, n, out, since }),
                    Some(Err(())) => eprintln!("[h2-move] f={f} ch.a(BBZZ) FAILED (issued f={since}) — nothing moved"),
                    Some(Ok(0)) => eprintln!("[h2-move] f={f} ch.a(BBZZ) returned null — nothing moved"),
                    Some(Ok(fresh)) => match h2_call(&mut *emu, guest_build::JavaCall::BagAddCount, Some(dst), &[JavaArg::Object(fresh), JavaArg::Int(i32::from(n))]) {
                        Some(out) => move_step = Some(MoveStep::SplitAdd { src, rec, fresh, n, out, since: f }),
                        None => {
                            eprintln!("[h2-move] f={f} BagAddCount refused — nothing moved");
                            emu.release_java_object(fresh);
                        }
                    },
                },
                Some(MoveStep::SplitAdd { src, rec, fresh, n, out, since }) => match out.poll() {
                    None => move_step = Some(MoveStep::SplitAdd { src, rec, fresh, n, out, since }),
                    Some(done) => {
                        emu.release_java_object(fresh);
                        match done {
                            Err(()) => eprintln!("[h2-move] f={f} bq.a(Lch;B)I FAILED (issued f={since}) — nothing moved"),
                            // ⛔ What the DESTINATION took is what the source gives up.
                            Ok(placed) => {
                                let placed = placed as i32;
                                if placed <= 0 {
                                    eprintln!("[h2-move] f={f} the destination took none of the {n} — nothing moved");
                                } else {
                                    if placed < i32::from(n) {
                                        eprintln!("[h2-move] f={f} the destination took {placed} of the {n} asked for");
                                    }
                                    let rd = |a: u32, k: usize| emu.read_memory(a, k);
                                    match guest_build::h2_bag_index_of(&rd, src, rec) {
                                        None => eprintln!("[h2-move] f={f} record {rec:#x} is not in the source bag any more — consuming nothing"),
                                        Some(at) => match h2_call(&mut *emu, guest_build::JavaCall::BagConsumeAt, Some(src), &[JavaArg::Int(at as i32), JavaArg::Int(placed)]) {
                                            Some(out) => move_step = Some(MoveStep::SplitConsume { rec, placed, out, since: f }),
                                            None => eprintln!("[h2-move] f={f} BagConsumeAt refused — {placed} of record {rec:#x} are now in BOTH bags"),
                                        },
                                    }
                                }
                            }
                        }
                    }
                },
                Some(MoveStep::SplitConsume { rec, placed, out, since }) => match out.poll() {
                    None => move_step = Some(MoveStep::SplitConsume { rec, placed, out, since }),
                    Some(Err(())) => eprintln!("[h2-move] f={f} bq.a(BI)V FAILED (issued f={since}) — {placed} of record {rec:#x} are now in BOTH bags"),
                    Some(Ok(_)) => {
                        let writes = {
                            let rd = |a: u32, k: usize| emu.read_memory(a, k);
                            guest_build::h2_window_chain(&rd, &h2).map(|c| guest_build::h2_repaint_writes(&c)).unwrap_or_default()
                        };
                        for (a, bytes) in &writes {
                            emu.write_memory(*a, bytes);
                        }
                        eprintln!("[h2-move] f={f} split {placed} out of record {rec:#x}; {} repaint flag(s)", writes.len());
                    }
                },
                Some(MoveStep::Remove { rec, out, since }) => match out.poll() {
                    None => move_step = Some(MoveStep::Remove { rec, out, since }),
                    Some(Err(())) => eprintln!("[h2-move] f={f} bq.b(B)V FAILED (issued f={since}) — record {rec:#x} is now in BOTH bags"),
                    Some(Ok(_)) => {
                        // A window over either bag paints only when its `e` is set.
                        let writes = {
                            let rd = |a: u32, n: usize| emu.read_memory(a, n);
                            guest_build::h2_window_chain(&rd, &h2).map(|c| guest_build::h2_repaint_writes(&c)).unwrap_or_default()
                        };
                        for (a, bytes) in &writes {
                            emu.write_memory(*a, bytes);
                        }
                        eprintln!("[h2-move] f={f} moved record {rec:#x} (same object, same serial); {} repaint flag(s)", writes.len());
                    }
                },
            }
            // ── ★ 2026-10-13 --h2-fold: the warehouse load's fold, one split at a time (the app's rule) ──
            if let Some(&(_, b)) = h2_fold.iter().find(|x| x.0 == f) {
                fold_on = Some((b, 0));
            }
            if let Some((b, rounds)) = fold_on
                && move_step.is_none()
            {
                let next = {
                    let rd = |a: u32, n: usize| emu.read_memory(a, n);
                    let handle = match b {
                        b'w' => account_bag,
                        b'a' => guest_build::h2_static_handle(&rd, h2.inventory_slot),
                        b'b' => guest_build::h2_static_handle(&rd, h2.material_bag_slot),
                        _ => guest_build::h2_static_handle(&rd, h2.card_bag_slot),
                    };
                    handle.and_then(|h| guest_build::h2_bag_merge_candidate(&rd, h).map(|it| (h, it)))
                };
                match next {
                    Some((bag, it)) if rounds < 256 => {
                        eprintln!("[h2-fold] f={f} bag {} round {}: record {:#x} (type {} id {} x{}) into an earlier stack", b as char, rounds + 1, it.handle, it.kind, it.id, it.count);
                        let args = [JavaArg::Int(i32::from(it.kind)), JavaArg::Int(i32::from(it.id)), JavaArg::Int(1), JavaArg::Int(0)];
                        match h2_call(&mut *emu, guest_build::JavaCall::RecordNew, None, &args) {
                            Some(out) => {
                                move_step = Some(MoveStep::SplitNew { src: bag, rec: it.handle, dst: bag, n: it.count.clamp(0, 99) as u8, out, since: f });
                                fold_on = Some((b, rounds + 1));
                            }
                            None => {
                                eprintln!("[h2-fold] f={f} RecordNew refused — stopped");
                                fold_on = None;
                            }
                        }
                    }
                    _ => {
                        eprintln!("[h2-fold] f={f} bag {} folded after {rounds} round(s)", b as char);
                        fold_on = None;
                    }
                }
            }
            // #22's reset, planned from the live character. Prints first and writes
            // only when asked: these are the fields the save carries, so the flag that
            // measures them must not be the flag that changes them.
            if let Some((rf, which, apply)) = h2_reset
                && rf == f
            {
                let rd = |a: u32, n: usize| emu.read_memory(a, n);
                let what = which.name();
                match guest_build::h2_reset_report(&rd, &h2) {
                    Some(lines) => {
                        for l in &lines {
                            eprintln!("[h2-reset] f={f} {l}");
                        }
                    }
                    None => eprintln!("[h2-reset] f={f} no character yet"),
                }
                let plan = which.plan(&rd, &h2);
                match plan {
                    Some(r) => {
                        eprintln!("[h2-reset] f={f} {what}: refund {} over {} slot(s), {} write(s){}",
                                  r.refund, r.cleared, r.writes.len(), if apply { "" } else { "  (dry run)" });
                        for (a, bytes) in &r.writes {
                            if apply {
                                let ok = emu.write_memory(*a, bytes);
                                eprintln!("[h2-reset]   {a:#x} <- {} ({})", hex(bytes), if ok { "ok" } else { "REFUSED" });
                            } else {
                                eprintln!("[h2-reset]   {a:#x} <- {}", hex(bytes));
                            }
                        }
                    }
                    // ⛔ For the repair, no plan is the PASS: it is idempotent by condition
                    // and answers `None` on a character whose stage 1 is learned. The
                    // `combo stages` line above says whether the split was readable at all.
                    None if which == H2ResetWhat::Repair => eprintln!("[h2-reset] f={f} repair: nothing to do — 기본공격 is intact"),
                    None => eprintln!("[h2-reset] f={f} {what}: REFUSED (a read did not make sense — nothing written)"),
                }
                if apply {
                    // Read it all back: the point of applying here is to prove the write
                    // landed where the plan said, not that the plan looked right.
                    let rd2 = |a: u32, n: usize| emu.read_memory(a, n);
                    if let Some(lines) = guest_build::h2_reset_report(&rd2, &h2) {
                        for l in &lines {
                            eprintln!("[h2-reset] f={f} after: {l}");
                        }
                    }
                    match (which, which.plan(&rd2, &h2)) {
                        // ⛔ The repair is idempotent by CONDITION: once stage 1 reads as
                        // learned it plans NOTHING, so `None` is the pass here and a plan
                        // that survives is the failure. The two resets are the other way
                        // round — they stay plannable and answer "0 left spent".
                        (H2ResetWhat::Repair, None) => eprintln!("[h2-reset] f={f} after: the repair no longer fires — 기본공격 is back"),
                        (H2ResetWhat::Repair, Some(r)) => eprintln!("[h2-reset] f={f} after: the repair STILL fires ({} write(s)) — the write did not land", r.writes.len()),
                        (_, Some(r)) => eprintln!("[h2-reset] f={f} after: a second reset would refund {} ({} write(s)) — 0 means nothing is left spent", r.refund, r.writes.len()),
                        (_, None) => eprintln!("[h2-reset] f={f} after: the plan now REFUSES — the write left something inconsistent"),
                    }
                }
            }
            if let Some((bf, old)) = h2_combo_break
                && bf == f
            {
                let rd = |a: u32, n: usize| emu.read_memory(a, n);
                let where_ = (|| {
                    let obj = guest_build::h2_static_handle(&rd, h2.char_slot)?;
                    let fields = guest_build::h2_fields(&rd, obj)?;
                    let w: [u8; 4] = rd(fields + 4 + h2.char_skill_levels_off, 4).try_into().ok()?;
                    let (base, _) = guest_build::h2_array(&rd, u32::from_le_bytes(w))?;
                    Some((base, fields + 4 + h2.char_combo_open_off))
                })();
                let stages = guest_build::h2_combo_limit(&rd, &h2);
                match (where_, stages) {
                    (Some((base, _)), Some(stages)) if old => {
                        let levels = rd(base, stages as usize);
                        let mut n = 0;
                        for (i, lv) in levels.iter().enumerate() {
                            if (*lv as i8) > 0 {
                                n += usize::from(emu.write_memory(base + i as u32, &[0]));
                            }
                        }
                        eprintln!("[h2-reset] f={f} combo-break:old: {n} positive stage(s) of {stages} <- 0, a.h untouched — the old reset's footprint");
                    }
                    (None, _) | (_, None) if old => eprintln!("[h2-reset] f={f} combo-break:old: no character / no split yet"),
                    (Some((stage1, open)), _) => {
                        let a = emu.write_memory(stage1, &[0]);
                        let b = emu.write_memory(open, &[0]);
                        eprintln!("[h2-reset] f={f} combo-break: a.p[0]@{stage1:#x} <- 0 ({a}), a.h@{open:#x} <- 0 ({b}) — the pre-fix damage, reproduced");
                    }
                    (None, _) => eprintln!("[h2-reset] f={f} combo-break: no character yet"),
                }
            }
            if let Some((hf, kind)) = h2_heal
                && hf == f
            {
                let rd = |a: u32, n: usize| emu.read_memory(a, n);
                match guest_build::h2_heal_select_writes(&rd, &h2, kind) {
                    Some((writes, index, total)) => {
                        eprintln!("[h2-heal] f={f} type {kind} -> bag index {index} (x{total}), {} write(s) + `0`", writes.len());
                        for (a, bytes) in &writes {
                            let ok = emu.write_memory(*a, bytes);
                            eprintln!("[h2-heal]   {a:#x} <- {} ({})", hex(bytes), if ok { "ok" } else { "REFUSED" });
                        }
                        emu.handle_event(Event::Keydown(KeyCode::NUM0));
                        released.push((f + 2, KeyCode::NUM0));
                    }
                    None => eprintln!("[h2-heal] f={f} type {kind}: not in the cycle or none in the bag"),
                }
            }
            // The item-use drive: started at its frame, then one look per frame — the
            // same `h2_use_step` the app's worker runs, so the log here IS the app's
            // sequence. Keys go down now and up two frames later like every other key
            // the probe presses.
            if let Some((uf, kind)) = h2_use
                && uf == f
            {
                let rd = |a: u32, n: usize| emu.read_memory(a, n);
                match guest_build::h2_use_start(&rd, &h2, kind) {
                    Ok(d) => {
                        eprintln!("[h2-use] f={f} type {kind}: drive started");
                        use_drive = Some(d);
                    }
                    Err(why) => eprintln!("[h2-use] f={f} type {kind}: refused — {why}"),
                }
            }
            if let Some(d) = use_drive.as_mut() {
                let rd = |a: u32, n: usize| emu.read_memory(a, n);
                match guest_build::h2_use_step(&rd, &h2, d) {
                    guest_build::H2UseAction::Act { writes, key, what } => {
                        for (a, bytes) in &writes {
                            let ok = emu.write_memory(*a, bytes);
                            eprintln!("[h2-use]   {a:#x} <- {} ({})", hex(bytes), if ok { "ok" } else { "REFUSED" });
                        }
                        let code = match key {
                            guest_build::H2Key::Ok => KeyCode::OK,
                            guest_build::H2Key::Clr => KeyCode::CLEAR,
                        };
                        eprintln!("[h2-use] f={f} {what} ({} write(s)) -> {code:?}, next {:?}", writes.len(), d.step);
                        emu.handle_event(Event::Keydown(code));
                        released.push((f + 2, code));
                    }
                    guest_build::H2UseAction::Wait => {}
                    guest_build::H2UseAction::Handoff { why } => eprintln!("[h2-use] f={f} HANDOFF — {why} (answer it with --keys)"),
                    // ★★ 2026-10-21 비행서 plate: what the app's `AccountState::start_flight(None)` does on the scroll's popup —
                    //   the town list over its labels (refused = the app's gm 77 / gm 98 box), then the screen is the player's.
                    guest_build::H2UseAction::FlightPicker { popup } => {
                        let ctx = page_block.and_then(|b| guest_build::h2_page_ctx(&rd, b)).unwrap_or((0, 0));
                        let bags: Vec<u32> = [guest_build::h2_static_handle(&rd, h2.inventory_slot), guest_build::h2_static_handle(&rd, h2.material_bag_slot), account_bag].into_iter().flatten().collect();
                        let p = guest_build::h2_bag_popup_walk(&rd, &h2, &bags, ctx).filter(|p| p.handle == popup && guest_build::h2_is_flight(p));
                        let (allowed, towns) = (guest_build::h2_flight_allowed(&rd), guest_build::h2_flight_destinations(&rd));
                        let writes = p.as_ref().filter(|_| allowed).and_then(|p| guest_build::h2_flight_picker_writes(&rd, &h2, p));
                        match writes {
                            Some(writes) => {
                                let mut ok = true;
                                for (a, b) in &writes {
                                    ok &= emu.write_memory(*a, b);
                                }
                                eprintln!("[h2-use] f={f} 비행서 popup {popup:#x}: town list written {towns:?} ({} write(s) ok={ok}) — HANDOFF, pick with --h2-flight <f:town>", writes.len());
                            }
                            None => eprintln!("[h2-use] f={f} 비행서 popup {popup:#x}: no town list (popup found {}, allowed {allowed}, visited {towns:?}) — HANDOFF", p.is_some()),
                        }
                    }
                    guest_build::H2UseAction::Resume => eprintln!("[h2-use] f={f} resume — closing the rest"),
                    guest_build::H2UseAction::Done => {
                        eprintln!("[h2-use] f={f} DONE ({} poll(s))", d.polls);
                        use_drive = None;
                    }
                    guest_build::H2UseAction::Abort { why } => {
                        eprintln!("[h2-use] f={f} ABORT at {:?} — {why} ({} poll(s))", d.step, d.polls);
                        use_drive = None;
                    }
                }
            }
            for sf in &h2_save_frames {
                if *sf != f {
                    continue;
                }
                // ★ 2026-09-25: the app's `save_begin` refuses a character that is DOWN
                //   (dying, dead, 0 HP) — mirrored, through the same reader.
                let down = {
                    let rd = |a: u32, n: usize| emu.read_memory(a, n);
                    guest_build::h2_char_down(&rd, &h2)
                };
                if down {
                    eprintln!("[h2-save] f={f} REFUSED — the character is down (the app's save_begin gate)");
                    continue;
                }
                let out = std::sync::Arc::new(wie_backend::GuestCallResult::new());
                eprintln!("[h2-save] f={f} calling ag.l()V through the JVM");
                emu.call_java_singleton_void("ag", "a", "Lag;", "l", out.clone());
                h2_save_pending.push((f, out));
            }
            h2_save_pending.retain(|(at, out)| match out.poll() {
                None => true,
                Some(Ok(_)) => {
                    eprintln!("[h2-save] f={f} ag.l()V returned (issued at f={at})");
                    false
                }
                Some(Err(())) => {
                    eprintln!("[h2-save] f={f} ag.l()V FAILED (issued at f={at})");
                    false
                }
            });
            for (tf, tx, ty) in &h2_taps {
                if *tf != f {
                    continue;
                }
                // ★ 부활의 구: with the box up a tap is the box's (the app's `Revive::tap`) — a
                //   choice becomes this frame's OK, which the key loop answers through the
                //   same arm a scripted OK takes.
                if let Some(p) = revive_prompt.as_mut() {
                    let (sw, sh, cx) = p.3;
                    let hit = guest_build::h2_confirm_hit(p.2.len(), sw, sh, cx, *tx as i32, *ty as i32);
                    match p.0.tap(hit) {
                        guest_build::H2ConfirmAct::Chose(yes) => {
                            eprintln!("[h2-tap] f={f} ({tx},{ty}) -> the 부활의 구 box chooses {}", if yes { "예" } else { "아니오" });
                            keys.push((f, KeyCode::OK, 1));
                        }
                        guest_build::H2ConfirmAct::Moved => eprintln!("[h2-tap] f={f} ({tx},{ty}) -> the box's cursor to {}", if p.0.yes { "예" } else { "아니오" }),
                        guest_build::H2ConfirmAct::Swallowed => eprintln!("[h2-tap] f={f} ({tx},{ty}) -> swallowed by the box"),
                    }
                    continue;
                }
                if let Some(block) = equip_ui_block {
                    let view = {
                        let rd = |a: u32, n: usize| emu.read_memory(a, n);
                        guest_build::h2_equip_view(&rd, &h2).filter(|v| v.shifted && v.over.is_none())
                    };
                    if let Some(v) = view {
                        equip_model.sync(Some(v.win.handle));
                        let wardrobe = probe_wardrobe(costume_book.as_ref());
                        let class = {
                            let rd = |a: u32, n: usize| emu.read_memory(a, n);
                            guest_build::h2_char_class(&rd, &h2)
                        };
                        let wearable = |a: u16, slot: u8| guest_build::h2_av_wearable(&avatar_sheets, a, slot, class);
                        let had_focus = equip_model.focus.is_some();
                        let r = equip_model.tap(*tx as i32, *ty as i32, v.panel, v.cursor, &wardrobe, &wearable, class);
                        // The app's rule: the first tap after an avatar cell had the focus only shows
                        // the game's cursor again, even on the cell it rests on (no OK).
                        if had_focus && r == guest_build::H2EquipKey::Pass {
                            let same_cell = {
                                let rd = |a: u32, n: usize| emu.read_memory(a, n);
                                guest_build::h2_tap(&rd, &h2, *tx, *ty, 0) == guest_build::H2Tap::Activate
                            };
                            if same_cell {
                                probe_equip_hide(&mut *emu, block, &equip_model, &mut equip_hidden, Some(&v), f);
                                eprintln!("[h2-tap] f={f} ({tx},{ty}) -> the game's cursor shown again on its own cell (no OK)");
                                continue;
                            }
                        }
                        if let guest_build::H2EquipKey::Take { wear: Some((slot, id)) } = r {
                            let ok = match (costume_book.as_mut(), costume_io.as_ref()) {
                                (Some(b), Some(io)) => b.wear_instance(io, slot, id),
                                _ => false,
                            };
                            eprintln!("[h2-equip-ui] f={f} avatar slot {slot} wears instance {id:?} ok={ok}");
                        }
                        probe_equip_hide(&mut *emu, block, &equip_model, &mut equip_hidden, Some(&v), f);
                        if let guest_build::H2EquipKey::Take { .. } = r {
                            eprintln!("[h2-tap] f={f} ({tx},{ty}) -> the avatar side: focus {:?} list {:?}", equip_model.focus, equip_model.list);
                            continue;
                        }
                    }
                }
                let rd = |a: u32, n: usize| emu.read_memory(a, n);
                // The probe does not run the app's patch pass, so the grid is unshifted.
                let tap = guest_build::h2_tap(&rd, &h2, *tx, *ty, 0);
                match &tap {
                    guest_build::H2Tap::Focus { writes } | guest_build::H2Tap::Tab { writes, .. } => {
                        for (a, bytes) in writes {
                            let ok = emu.write_memory(*a, bytes);
                            eprintln!("[h2-tap]   {a:#x} <- {} ({})", hex(bytes), if ok { "ok" } else { "REFUSED" });
                        }
                    }
                    _ => {}
                }
                match &tap {
                    guest_build::H2Tap::Focus { .. } => eprintln!("[h2-tap] f={f} ({tx},{ty}) -> focus"),
                    guest_build::H2Tap::Activate => {
                        eprintln!("[h2-tap] f={f} ({tx},{ty}) -> OK");
                        emu.handle_event(Event::Keydown(KeyCode::OK));
                        released.push((f + 2, KeyCode::OK));
                    }
                    guest_build::H2Tap::Tab { right, .. } => {
                        let key = if *right { KeyCode::RIGHT } else { KeyCode::LEFT };
                        eprintln!("[h2-tap] f={f} ({tx},{ty}) -> tab {key:?}");
                        emu.handle_event(Event::Keydown(key));
                        released.push((f + 2, key));
                    }
                    guest_build::H2Tap::Page { dir } => {
                        let key = match dir {
                            0 => KeyCode::UP,
                            1 => KeyCode::DOWN,
                            2 => KeyCode::LEFT,
                            _ => KeyCode::RIGHT,
                        };
                        eprintln!("[h2-tap] f={f} ({tx},{ty}) -> page {key:?}");
                        emu.handle_event(Event::Keydown(key));
                        released.push((f + 2, key));
                    }
                    guest_build::H2Tap::Swallow => eprintln!("[h2-tap] f={f} ({tx},{ty}) -> swallowed"),
                    guest_build::H2Tap::None => eprintln!("[h2-tap] f={f} ({tx},{ty}) -> nothing"),
                }
            }
            for &(_, req) in h2_honor.iter().filter(|(at, _)| *at == f) {
                if let Some(k) = req {
                    let weapons = {
                        let rd = |a: u32, n: usize| emu.read_memory(a, n);
                        h2_honor_report(&rd, f)
                    };
                    // DIAGNOSTIC write (never an app write): `bu.k:B` is a sign-extended word.
                    for (i, _, item) in &weapons {
                        let ok = emu.write_memory(item + 4 + 0x3c, &k.to_le_bytes());
                        eprintln!("[h2-honor] f={f} [{i}] 요구명예 <- {k} (diagnostic) ok={ok}");
                    }
                }
                let weapons = {
                    let rd = |a: u32, n: usize| emu.read_memory(a, n);
                    h2_honor_report(&rd, f)
                };
                // The game's own getters — the two the patch rewrites. A read-only call the
                // app never makes, so it goes to the backend directly rather than through the
                // app's JavaCall table.
                for (i, handle, _) in weapons {
                    for m in ["c", "d"] {
                        let out = Arc::new(GuestCallResult::new());
                        emu.call_java_method(JavaReceiver::Handle { handle, class: "bo" }, m, "()S", &[], out.clone());
                        honor_calls.push((i, m, out));
                    }
                }
            }
            for &(_, t, only) in h2_weapon_type.iter().filter(|(at, ..)| *at == f) {
                let weapons = {
                    let rd = |a: u32, n: usize| emu.read_memory(a, n);
                    h2_honor_report(&rd, f)
                };
                for (i, _, item) in weapons.iter().filter(|(i, ..)| only.map_or(true, |s| s == *i)) {
                    // DIAGNOSTIC write (never an app write): ch.g, abs 0x0 — e.g. 2 도검 -> 4 둔기, so the
                    // game lets a sword knight cast 둔기강타 through its own skill path.
                    let ok = emu.write_memory(item + 4, &t.to_le_bytes());
                    eprintln!("[h2-weapon-type] f={f} [{i}] type <- {t} (diagnostic) ok={ok}");
                }
            }
            for &(_, t) in h2_weapon_trait.iter().filter(|(at, _)| *at == f) {
                let (weapons, ch) = {
                    let rd = |a: u32, n: usize| emu.read_memory(a, n);
                    let h2 = GuestBuild::Hero2Ktf.hero2_java().unwrap();
                    (h2_honor_report(&rd, f), guest_build::h2_static_handle(&rd, h2.char_slot).and_then(|o| guest_build::h2_fields(&rd, o)))
                };
                for (i, _, item) in weapons.iter() {
                    // DIAGNOSTIC write (never an app write): bo.i, abs 0x58 — a sign-extended word like every B field.
                    let ok = emu.write_memory(item + 4 + 0x58, &t.to_le_bytes());
                    eprintln!("[h2-weapon-trait] f={f} [{i}] bo.i <- {t} (diagnostic) ok={ok}");
                }
                if let Some(ch) = ch {
                    let ok = emu.write_memory(ch + 4 + 0xd8, &t.to_le_bytes());
                    eprintln!("[h2-weapon-trait] f={f} character a.m:B <- {t} (diagnostic) ok={ok}");
                }
            }
            if !h2_weapon_trait.is_empty() {
                let rd = |a: u32, n: usize| emu.read_memory(a, n);
                let h2 = GuestBuild::Hero2Ktf.hero2_java().unwrap();
                if let Some(ch) = guest_build::h2_static_handle(&rd, h2.char_slot).and_then(|o| guest_build::h2_fields(&rd, o)) {
                    let w = |off: u32| {
                        let b = rd(ch + 4 + off, 4);
                        (b.len() == 4).then(|| i32::from_le_bytes([b[0], b[1], b[2], b[3]])).unwrap_or(-99)
                    };
                    let tuji = w(0xfc);
                    if tuji_last != Some(tuji) {
                        eprintln!("[h2-tuji] f={f} a.h:Z(투지) = {tuji} · a.m:B(특성) = {} · a.l:I(치명 %) = {} · a.k:B(치명 배율 ×/10) = {}", w(0xd8), w(0xc0), w(0xc4));
                        tuji_last = Some(tuji);
                    }
                }
            }
            for &(_, kind, face) in h2_sweep.iter().filter(|(at, ..)| *at == f) {
                let rd = |a: u32, n: usize| emu.read_memory(a, n);
                let h2 = GuestBuild::Hero2Ktf.hero2_java().unwrap();
                match guest_build::h2_static_handle(&rd, h2.char_slot) {
                    Some(handle) => {
                        if let (Some(d), Some(fb)) = (face, guest_build::h2_fields(&rd, handle)) {
                            // DIAGNOSTIC write (never an app write): g.u, abs 0x40, a sign-extended word.
                            let ok = emu.write_memory(fb + 4 + 0x40, &d.to_le_bytes());
                            eprintln!("[h2-sweep] f={f} facing <- {d} (diagnostic) ok={ok}");
                        }
                        let out = Arc::new(GuestCallResult::new());
                        emu.call_java_method(JavaReceiver::Handle { handle, class: "bx" }, "h", "(B)Z", &[JavaArg::Int(kind)], out.clone());
                        eprintln!("[h2-sweep] f={f} bx.h(B)Z({kind}) issued");
                        sweep_calls.push((kind, out));
                    }
                    None => eprintln!("[h2-sweep] f={f} no character"),
                }
            }
            sweep_calls.retain(|(kind, out)| match out.poll() {
                None => true,
                Some(r) => {
                    eprintln!("[h2-sweep] f={f} bx.h(B)Z({kind}) = {:?}", r.map(|v| v as i32));
                    false
                }
            });
            honor_calls.retain(|(i, m, out)| match out.poll() {
                None => true,
                Some(Ok(v)) => {
                    eprintln!("[h2-honor] f={f} [{i}] the game's bo.{m}()S = {}", v as i32);
                    false
                }
                Some(Err(())) => {
                    eprintln!("[h2-honor] f={f} [{i}] bo.{m}()S FAILED");
                    false
                }
            });
            for (_, bag) in h2_inv.iter().filter(|(at, _)| *at == f) {
                let rd = |a: u32, n: usize| emu.read_memory(a, n);
                let walked = match bag {
                    b'w' => account_bag.and_then(|h| guest_build::h2_bag_of(&rd, h)),
                    b'b' => guest_build::h2_bag(&rd, h2.material_bag_slot),
                    b'c' => guest_build::h2_bag(&rd, h2.card_bag_slot),
                    _ => guest_build::h2_bag(&rd, h2.inventory_slot),
                };
                match walked {
                    Some(inv) => {
                        eprintln!(
                            "[h2-inv] f={f} bag {} bq={:#x} b={} heal={} c={} items={} cycle={:?}",
                            *bag as char,
                            inv.handle,
                            inv.b,
                            inv.heal,
                            inv.c,
                            inv.items.len(),
                            guest_build::h2_heal_cycle(&rd, &h2)
                        );
                        for (i, it) in inv.items.iter().enumerate() {
                            let name: String = it.name.iter().map(|x| format!("{x:02x}")).collect();
                            let mark = if i as i8 == inv.heal { " <== heal" } else { "" };
                            // ★ 2026-09-26: the worn flag (`aa.f`) — the game's `bq.a(Lch;)Z`, read.
                            let worn = match guest_build::h2_record_worn(&rd, it.handle) {
                                Some(true) => format!(" WORN(slot {})", guest_build::h2_worn_flag_clear(&rd, it.handle).map_or(-1, |(a, _)| rd(a, 4).first().copied().map_or(-1, |b| b as i8 as i32))),
                                Some(false) => String::new(),
                                None => " worn=?".to_string(),
                            };
                            // ★★ 2026-10-17: equipment's 요구레벨 `aa.a:B` (absolute +0x20 — `ch` is 0x20) — the item table refresh
                            let lv = if (0..=12).contains(&it.kind) {
                                guest_build::h2_fields(&rd, it.handle).and_then(|fb| rd(fb + 4 + 0x20, 1).first().copied()).map_or(" lv=?".to_string(), |b| format!(" lv={}", b as i8))
                            } else {
                                String::new()
                            };
                            eprintln!("[h2-inv]   [{i:2}] type={:2} id={:3} count={:3} d={} e={} f={}{lv} {name}{mark}{worn}", it.kind, it.id, it.count, it.d, it.e, it.f);
                        }
                    }
                    None => eprintln!("[h2-inv] f={f} no inventory yet"),
                }
                if let Some(p) = guest_build::h2_item_popup_walk(&rd, &h2, (0, 0)) {
                    eprintln!(
                        "[h2-inv] f={f} item popup={:#x} kind6 over [{}] type={} id={} x{} sel={} rows={} h={} len={} labels={:x?} gm={:x?} shape={:?}",
                        p.handle, p.item_index, p.item_kind, p.item_id, p.item_count, p.sel, p.rows, p.height, p.labels_len, p.labels, p.gm, p.shape()
                    );
                }
            }
            if let Some((cf, g)) = h2_commit
                && cf == f
            {
                let rd = |a: u32, n: usize| emu.read_memory(a, n);
                match guest_build::h2_popup_walk(&rd, &h2) {
                    Some(p) => match p.shape() {
                        Some(shape) => {
                            let restore = guest_build::h2_popup_restore_writes(&p);
                            for (a, bytes) in &restore {
                                let ok = emu.write_memory(*a, bytes);
                                eprintln!("[h2-commit]   restore {a:#x} <- {} ({})", hex(bytes), if ok { "ok" } else { "REFUSED" });
                            }
                            let row = shape.bind_row + g;
                            let ok = emu.write_memory(p.cursor_addr(), &[row]);
                            eprintln!("[h2-commit] f={f} popup={:#x} value={:?} guest {g} -> cursor {row} at {:#x} ({}) + OK", p.handle, p.value(), p.cursor_addr(), if ok { "ok" } else { "REFUSED" });
                            emu.handle_event(Event::Keydown(KeyCode::OK));
                            released.push((f + 2, KeyCode::OK));
                        }
                        None => eprintln!("[h2-commit] f={f} popup {:#x} is not a register popup (labels {:x?})", p.handle, p.labels),
                    },
                    None => eprintln!("[h2-commit] f={f} no register popup on screen"),
                }
            }
        }
        // ── --h2-boss-refill (2026-09-26): the app's `BossRefill` with the same shared calls ──
        if h2_boss_refill
            && let (Some(h2), Some(sv), Some(st @ AppState::Absolute { addr, field, .. })) = (build().hero2_java(), GuestBuild::Hero2Ktf.hero2_save(), build().app_state())
        {
            let (fresh, writes) = {
                let rd = |a: u32, n: usize| emu.read_memory(a, n);
                let b = rd(addr, 4);
                let state = (b.len() == 4).then(|| u32::from_le_bytes([b[0], b[1], b[2], b[3]]));
                if state.is_some_and(|v| v == field || st.is_script(v) == Some(true)) {
                    let dying = guest_build::h2_dying_bosses(&rd, sv.area.stage_slot);
                    boss_refill_answered.retain(|x| dying.contains(x));
                    let fresh: Vec<u32> = dying.iter().copied().filter(|x| !boss_refill_answered.contains(x)).collect();
                    // The app's window is 3 s of wall clock; the probe's is the same span of ticks.
                    let open = !fresh.is_empty() || boss_refill_until.is_some_and(|u| f < u);
                    let w = if open { guest_build::h2_refill_writes(&rd, &h2) } else { None };
                    (fresh.clone(), w.filter(|w| !fresh.is_empty() || guest_build::h2_plan_stands_up(w)))
                } else {
                    (vec![], None)
                }
            };
            if !fresh.is_empty() {
                boss_refill_answered.extend(&fresh);
                boss_refill_until = Some(f + 150);
            }
            if let Some(w) = writes {
                let mut ok = true;
                for (a, bytes) in &w {
                    ok = emu.write_memory(*a, bytes);
                    if !ok {
                        break;
                    }
                }
                eprintln!("[h2-boss-refill] f={f} {} — {} write(s){} ok={ok}", if fresh.is_empty() { "fall inside the window".to_string() } else { format!("boss {fresh:#x?} dying") }, w.len(), if guest_build::h2_plan_stands_up(&w) { " (stand-up)" } else { "" });
            } else if !fresh.is_empty() {
                eprintln!("[h2-boss-refill] f={f} boss {fresh:#x?} dying — character already full");
            }
        }
        if let Some((start, _)) = h2_benchmark
            && (548..=start).contains(&f)
            && let (Some(sv), Some(h2)) = (build().hero2_save(), build().hero2_java())
        {
            let monsters = guest_build::h2_monsters(&|a, n| emu.read_memory(a, n), sv.area.stage_slot);
            for mon in monsters {
                let original = *benchmark_attack.entry(mon.handle).or_insert((mon.atk, mon.template_atk));
                let atk = if f == start { i32::from(original.0) } else { 0 };
                emu.write_memory(mon.block + 4 + guest_build::Hero2Java::MONSTER_ATK_OFF, &atk.to_le_bytes());
                if mon.template_fields != 0 {
                    let atk = if f == start { i32::from(original.1) } else { 0 };
                    emu.write_memory(mon.template_fields + 4 + guest_build::Hero2Java::TEMPLATE_ATK_OFF, &atk.to_le_bytes());
                }
                let hp = if f == start { mon.max_hp } else { 100_000_000 };
                emu.write_memory(mon.block + 4 + guest_build::Hero2Java::MONSTER_HP_OFF, &hp.to_le_bytes());
            }
            if let Some(writes) = guest_build::h2_refill_writes(&|a, n| emu.read_memory(a, n), &h2) {
                for (a, bytes) in writes { emu.write_memory(a, &bytes); }
            }
        }
        // ── --h2-mob-hp (2026-09-26): every live monster's HP ← n, raw (probe only), so the
        //    player's next real hit is a kill — the boss-and-player-die-together repro.
        if let Some(&(_, hp)) = h2_mob_hp.iter().find(|(mf, _)| *mf == f)
            && let Some(sv) = GuestBuild::Hero2Ktf.hero2_save()
        {
            let monster_descs = [0x0019_2670u32, 0x0019_23e8, 0x0019_29ac, 0x0019_2acc, 0x0019_3090]; // bd · al · ca · ci · m
            let targets: Vec<u32> = {
                let rd = |a: u32, n: usize| emu.read_memory(a, n);
                let rd_u = |a: u32| {
                    let b = rd(a, 4);
                    (b.len() == 4).then(|| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
                };
                let mut out = Vec::new();
                let grid = guest_build::h2_static_handle(&rd, sv.area.stage_slot)
                    .and_then(|s| guest_build::h2_fields(&rd, s))
                    .and_then(|b| rd_u(b + 4 + 0x30))
                    .and_then(|g| guest_build::h2_array(&rd, g));
                if let Some((rows_base, rows)) = grid {
                    for y in 0..rows {
                        let Some((row_base, len)) = rd_u(rows_base + y * 4).and_then(|r| guest_build::h2_array(&rd, r)) else { continue };
                        for x in 0..len {
                            let Some(e) = rd_u(row_base + x * 4).filter(|e| *e != 0) else { continue };
                            if guest_build::h2_class_desc(&rd, e).is_some_and(|d| monster_descs.contains(&d))
                                && let Some(fb) = guest_build::h2_fields(&rd, e)
                                && rd_u(fb + 4 + 0x3c).is_some_and(|t| t != 6 && t != 7)
                                && !out.contains(&fb)
                            {
                                out.push(fb);
                            }
                        }
                    }
                }
                out
            };
            for fb in &targets {
                let ok = emu.write_memory(fb + 4 + 0x74, &hp.to_le_bytes());
                if h2_benchmark.is_some() {
                    emu.write_memory(fb + 4 + 0x70, &hp.to_le_bytes());
                }
                eprintln!("[h2-mob-hp] f={f} monster fields {fb:#x}: HP (bd.d:S) <- {hp} ok={ok}");
            }
            if targets.is_empty() {
                eprintln!("[h2-mob-hp] f={f} no live monster on the stage");
            }
        }
        if let Some((start, end)) = h2_benchmark
            && (start..=end).contains(&f)
            && let (Some(sv), Some(h2)) = (build().hero2_save(), build().hero2_java())
        {
            let rd = |a, n| emu.read_memory(a, n);
            let word = |a| guest_build::h2_rd_u32(&rd, a).unwrap_or(0) as i32;
            if let Some(cf) = guest_build::h2_static_handle(&rd, h2.char_slot).and_then(|h| guest_build::h2_fields(&rd, h)) {
                eprintln!("[h2-bench] f={f} hp={} hpmax={} sp={} spmax={}", word(cf + 4 + 0x6c), word(cf + 4 + 0x78), word(cf + 4 + 0x70), word(cf + 4 + 0x7c));
            }
            for mon in guest_build::h2_monsters(&rd, sv.area.stage_slot) {
                eprintln!("[h2-bench-mob] f={f} id={} template={} state={} hp={} max={}", mon.handle, mon.template, word(mon.block + 4 + 0x3c), mon.hp, mon.max_hp);
            }
        }
        // ── 부활의 구 (2026-09-25): --h2-die / --h2-revive — the app's `Revive` ────────
        if let (Some(h2), Some(AppState::Absolute { addr, field, .. })) = (build().hero2_java(), build().app_state()) {
            if h2_die.contains(&f) {
                let v = {
                    let rd = |a: u32, n: usize| emu.read_memory(a, n);
                    guest_build::h2_vitals(&rd, &h2)
                };
                match v {
                    Some(v) if v.hp > 0 => {
                        let ok = h2_call(&mut *emu, guest_build::JavaCall::CharHpDelta, None, &[JavaArg::Int(-v.hp)]).is_some();
                        eprintln!("[h2-die] f={f} a.b({}) — HP {}/{} SP {}/{} g.t={} (call issued={ok})", -v.hp, v.hp, v.hp_max, v.sp, v.sp_max, v.motion);
                    }
                    Some(v) => eprintln!("[h2-die] f={f} HP already {} — nothing to do", v.hp),
                    None => eprintln!("[h2-die] f={f} no character yet"),
                }
            }
            if let Some((out, since)) = revive_consume.take() {
                match out.poll() {
                    None if f < since + 150 => revive_consume = Some((out, since)),
                    None => eprintln!("[h2-revive] f={f} bq.a(BI)V never returned (issued f={since})"),
                    Some(Err(())) => eprintln!("[h2-revive] f={f} bq.a(BI)V FAILED — a free revive"),
                    Some(Ok(_)) => {
                        let (text, left) = {
                            let rd = |a: u32, n: usize| emu.read_memory(a, n);
                            (guest_build::h2_revive_text(&rd), guest_build::h2_revive_orb(&rd, &h2).map_or(0, |o| o.total))
                        };
                        let noticed = match text {
                            Some(t) => h2_call(&mut *emu, guest_build::JavaCall::Notice, None, &[JavaArg::Object(t), JavaArg::Int(16), JavaArg::Int(0)]).is_some(),
                            None => false,
                        };
                        eprintln!("[h2-revive] f={f} one 부활의 구 consumed — {left} left; notice (itmtp[64]) raised={noticed}");
                    }
                }
            }
            if h2_revive && revive_prompt.is_none() {
                let (fall, down) = {
                    let rd = |a: u32, n: usize| emu.read_memory(a, n);
                    (guest_build::h2_fall_frame(&rd, &h2, addr, field), guest_build::h2_char_down(&rd, &h2))
                };
                if let Some(fr) = fall {
                    eprintln!("[h2-revive] f={f} falling — frame {fr} of 0..={}", guest_build::H2_FALL_LAST_FRAME);
                }
                if fall.is_none() {
                    if revive_declined && !down {
                        revive_declined = false;
                        eprintln!("[h2-revive] f={f} the character is up again — the 아니오 latch clears");
                    }
                } else if !revive_declined && revive_consume.is_none() && fall.is_some_and(|fr| fr >= guest_build::H2_FALL_ASK_FRAME) {
                    let (orb, screen) = {
                        let rd = |a: u32, n: usize| emu.read_memory(a, n);
                        if revive_font.is_none() {
                            revive_font = guest_build::h2_font_from_guest(&rd);
                        }
                        let word = |slot: u32| {
                            let b = rd(slot, 4);
                            (b.len() == 4).then(|| i32::from_le_bytes([b[0], b[1], b[2], b[3]]))
                        };
                        (guest_build::h2_revive_orb(&rd, &h2), (word(h2.screen.0), word(h2.screen.1), word(h2.screen.2)))
                    };
                    match (orb, screen, revive_font.is_some()) {
                        (Some(orb), (Some(sw), Some(sh), Some(cx)), true) => {
                            let lines = guest_build::h2_revive_lines(orb.total);
                            eprintln!(
                                "[h2-revive] f={f} FELL with {} 부활의 구 (first stack at index {} rec {:#x}) — the box is up on 예, the guest is HELD",
                                orb.total, orb.index, orb.record
                            );
                            revive_prompt = Some((guest_build::H2Confirm::new(), orb, lines, (sw, sh, cx)));
                        }
                        (None, _, _) => {
                            revive_declined = true;
                            eprintln!("[h2-revive] f={f} fell with no 부활의 구 — the game's own death runs");
                        }
                        _ => eprintln!("[h2-revive] f={f} fell, but the font / screen is not readable — cannot ask"),
                    }
                }
            }
            if h2_combo_watch.is_some_and(|(a, b)| (a..=b).contains(&f)) {
                let line = {
                    let rd = |a: u32, n: usize| emu.read_memory(a, n);
                    guest_build::h2_static_handle(&rd, h2.char_slot).and_then(|h| guest_build::h2_fields(&rd, h)).map(|cf| {
                        let b = cf + 4;
                        let w = |o: u32| guest_build::h2_rd_u32(&rd, b + o).map_or(i32::MIN, |v| v as i32);
                        let chain: Vec<i8> = guest_build::h2_rd_u32(&rd, b + guest_build::Hero2Java::CHAR_ACTION_QUEUE_OFF)
                            .and_then(|a| guest_build::h2_array(&rd, a))
                            .map(|(base, len)| rd(base, len.min(16) as usize).into_iter().map(|x| x as i8).collect())
                            .unwrap_or_default();
                        format!(
                            "t={} w={} a.i={} a.o={chain:?} a.h={} a.o:I={} a.j={} hp={}",
                            w(guest_build::Hero2Java::CHAR_WALK_STATE_OFF),
                            w(guest_build::Hero2Java::CHAR_MOTION_FRAME_OFF),
                            w(guest_build::Hero2Java::CHAR_ACTION_AT_OFF),
                            w(0xa8),
                            w(0xf4),
                            w(0xb4),
                            w(0x6c),
                        )
                    })
                };
                eprintln!("[h2-combo] f={f} {}", line.as_deref().unwrap_or("no character"));
            }
            if let Some(wf) = revive_watch {
                if f > wf + 30 {
                    revive_watch = None;
                } else if (f - wf) % 5 == 0 {
                    let (v, st) = {
                        let rd = |a: u32, n: usize| emu.read_memory(a, n);
                        let b = rd(addr, 4);
                        (guest_build::h2_vitals(&rd, &h2), (b.len() == 4).then(|| u32::from_le_bytes([b[0], b[1], b[2], b[3]])))
                    };
                    match v {
                        Some(v) => eprintln!("[h2-revive] f={f} +{} HP {}/{} SP {}/{} g.t={} state={st:?}", f - wf, v.hp, v.hp_max, v.sp, v.sp_max, v.motion),
                        None => eprintln!("[h2-revive] f={f} +{} no character state={st:?}", f - wf),
                    }
                }
            }
        }
        // ⛔ Held under the box: the guest does not tick, so a redraw request waits.
        if revive_prompt.is_none() && *redraw.lock().unwrap() {
            *redraw.lock().unwrap() = false;
            emu.handle_event(Event::Redraw);
        }
        for (kf, kc, hold) in &keys {
            // ★★ 2026-10-16 정렬 · 일괄판매: a scripted key the app's `BagTools` would take (OK / CLR on its popup, any key
            //   while a call is in flight) is taken here too; its release is the tools' at once (no key-up is scheduled).
            if *kf == f && h2_tools && bag_tools.key(&mut *emu, &mut ProbeBagIo(build().hero2_java()), *kc, true) {
                bag_tools.key(&mut *emu, &mut ProbeBagIo(build().hero2_java()), *kc, false);
                eprintln!("[h2-tools] f={f} {kc:?} taken by the tools");
                continue;
            }
            // ★★ 2026-10-20: the app's craft guard — an OK that would craft into a full bag is taken here too.
            if *kf == f
                && matches!(kc, KeyCode::OK | KeyCode::NUM5)
                && let Some(h2) = build().hero2_java()
            {
                let over = {
                    let rd = |a: u32, n: usize| emu.read_memory(a, n);
                    guest_build::h2_craft_overflow(&rd, &h2)
                };
                if let Some(o) = over {
                    if h2_craft_guard {
                        let text = {
                            let rd = |a: u32, n: usize| emu.read_memory(a, n);
                            guest_build::h2_bag_full_text(&rd)
                        };
                        eprintln!(
                            "[h2-craft] f={f} {kc:?} REFUSED — the result (type {}, id {}) would go to bag {:#x} ({} of {} cells, {} freed by the craft, no stack to merge into); asking for the game's box",
                            o.result.0, o.result.1, o.bag, o.count, o.capacity, o.freed
                        );
                        craft_step = h2_probe_notice(&mut *emu, o.window, None, text, f);
                        continue;
                    }
                    eprintln!(
                        "[h2-craft] f={f} {kc:?} would craft the result (type {}, id {}) into the full bag {:#x} ({} of {} cells) — guard off, the key goes to the game",
                        o.result.0, o.result.1, o.bag, o.count, o.capacity
                    );
                }
            }
            if *kf == f {
                // ★ 부활의 구: while the box is up every scripted key is the box's — the
                //   app's `Revive::key`. Its release is never scheduled (the `continue`
                //   below skips that), which is the probe's shape of "the UP is swallowed".
                if revive_prompt.is_some() {
                    let dir = match *kc {
                        KeyCode::UP | KeyCode::NUM2 => Some(guest_build::H2OptDir::Up),
                        KeyCode::DOWN | KeyCode::NUM8 => Some(guest_build::H2OptDir::Down),
                        KeyCode::LEFT | KeyCode::NUM4 => Some(guest_build::H2OptDir::Left),
                        KeyCode::RIGHT | KeyCode::NUM6 => Some(guest_build::H2OptDir::Right),
                        KeyCode::OK | KeyCode::NUM5 => Some(guest_build::H2OptDir::Ok),
                        _ => None,
                    };
                    let act = revive_prompt.as_mut().map(|p| p.0.key(dir));
                    match act {
                        Some(guest_build::H2ConfirmAct::Chose(yes)) => {
                            let (_c, orb, _lines, _screen) = revive_prompt.take().unwrap();
                            if !yes {
                                revive_declined = true;
                                eprintln!("[h2-revive] f={f} {kc:?} = 아니오 — the guest runs on, the game's own death follows ({revive_held_frames} frame(s) held)");
                                revive_watch = Some(f);
                            } else if let Some(h2) = build().hero2_java() {
                                let (plan, at) = {
                                    let rd = |a: u32, n: usize| emu.read_memory(a, n);
                                    (guest_build::h2_revive_writes(&rd, &h2), guest_build::h2_bag_index_of(&rd, orb.bag, orb.record))
                                };
                                match (plan, at) {
                                    (Some(writes), Some(at)) => {
                                        let mut done = 0usize;
                                        for (a, b) in &writes {
                                            if !emu.write_memory(*a, b) {
                                                break;
                                            }
                                            done += 1;
                                        }
                                        if done == writes.len() {
                                            revive_consume = h2_call(&mut *emu, guest_build::JavaCall::BagConsumeAt, Some(orb.bag), &[JavaArg::Int(at as i32), JavaArg::Int(1)]).map(|o| (o, f));
                                            eprintln!("[h2-revive] f={f} {kc:?} = 예 — {done} write(s) landed after {revive_held_frames} held frame(s); consuming 1 of {} at index {at}", orb.total);
                                        } else {
                                            revive_declined = true;
                                            eprintln!("[h2-revive] f={f} 예, but only {done}/{} write(s) landed — the fall runs on", writes.len());
                                        }
                                        revive_watch = Some(f);
                                    }
                                    _ => {
                                        revive_declined = true;
                                        eprintln!("[h2-revive] f={f} 예, but the fall / the orb no longer reads as it did — nothing written");
                                    }
                                }
                            }
                            revive_held_frames = 0;
                        }
                        Some(guest_build::H2ConfirmAct::Moved) => {
                            let yes = revive_prompt.as_ref().is_some_and(|p| p.0.yes);
                            eprintln!("[h2-revive] f={f} {kc:?} -> cursor on {}", if yes { "예" } else { "아니오" });
                        }
                        _ => eprintln!("[h2-revive] f={f} {kc:?} swallowed by the box"),
                    }
                    continue;
                }
                // ★ With the paging patch installed, LEFT/RIGHT at the grid's two ends is
                //   a page crossing the app performs instead of the key (`BagPaging::arrow`)
                //   — mirrored here so `--keys 940:LEFT` exercises the same rule headless.
                if let (Some(block), KeyCode::LEFT | KeyCode::RIGHT, Some(h2)) = (page_block, *kc, build().hero2_java()) {
                    let crossing = {
                        let rd = |a: u32, n: usize| emu.read_memory(a, n);
                        guest_build::h2_inventory_window(&rd, &h2).filter(|v| v.on_top).and_then(|v| {
                            let cursor = rd(v.win.fields + 4 + guest_build::Hero2Java::WIN_CURSOR_OFF, 1).first().map(|b| *b as i8)?;
                            guest_build::h2_page_arrow(page_now, cursor, *kc == KeyCode::LEFT).map(|(page, cell)| (v, cursor, page, cell))
                        })
                    };
                    if let Some((v, cursor, page, cell)) = crossing {
                        let writes = {
                            let rd = |a: u32, n: usize| emu.read_memory(a, n);
                            guest_build::h2_page_select_writes(&rd, &h2, block, &v, page, cell)
                        };
                        let mut ok = true;
                        for (a, bytes) in &writes {
                            ok &= emu.write_memory(*a, bytes);
                        }
                        page_now = page;
                        let cells = emu.read_memory(guest_build::h2_page_cells_write(&v, page).0, 1).first().copied();
                        eprintln!("[h2-page] f={f} {kc:?} on cell {cursor} -> page {page} cell {cell} ({} write(s) ok={ok}, window j.i = {cells:?}) — key swallowed", writes.len());
                        continue;
                    }
                }
                // ★ 2026-10-02: UP/DOWN on the short fifth page keep the column (`BagPaging::vertical`).
                if let (Some(_), KeyCode::UP | KeyCode::DOWN, Some(h2)) = (page_block, *kc, build().hero2_java()) {
                    let column = {
                        let rd = |a: u32, n: usize| emu.read_memory(a, n);
                        guest_build::h2_inventory_window(&rd, &h2).filter(|v| v.on_top).and_then(|v| {
                            let cursor = rd(v.win.fields + 4 + guest_build::Hero2Java::WIN_CURSOR_OFF, 1).first().map(|b| *b as i8)?;
                            guest_build::h2_page_vertical(page_now, cursor, *kc == KeyCode::UP).map(|cell| (v, cursor, cell))
                        })
                    };
                    if let Some((v, cursor, cell)) = column {
                        let mut writes = vec![(v.win.fields + 4 + guest_build::Hero2Java::WIN_CURSOR_OFF, vec![cell])];
                        {
                            let rd = |a: u32, n: usize| emu.read_memory(a, n);
                            if let Some(chain) = guest_build::h2_window_chain(&rd, &h2) {
                                writes.extend(guest_build::h2_repaint_writes(&chain));
                            }
                        }
                        let mut ok = true;
                        for (a, bytes) in &writes {
                            ok &= emu.write_memory(*a, bytes);
                        }
                        eprintln!("[h2-page] f={f} {kc:?} on page {page_now} cell {cursor} -> cell {cell} (the column, ok={ok}) — key swallowed");
                        continue;
                    }
                }
                if let (Some(block), Some(h2)) = (equip_ui_block, build().hero2_java()) {
                    let view = {
                        let rd = |a: u32, n: usize| emu.read_memory(a, n);
                        guest_build::h2_equip_view(&rd, &h2).filter(|v| v.shifted && v.over.is_none())
                    };
                    if let Some(v) = view {
                        equip_model.sync(Some(v.win.handle));
                        use guest_build::H2Nav;
                        // The app's map (EquipAvatar::key): the game's digit arrows and 5 = OK too.
                        let key = match *kc {
                            KeyCode::UP | KeyCode::NUM2 => Some(H2Nav::Up),
                            KeyCode::DOWN | KeyCode::NUM8 => Some(H2Nav::Down),
                            KeyCode::LEFT | KeyCode::NUM4 => Some(H2Nav::Left),
                            KeyCode::RIGHT | KeyCode::NUM6 => Some(H2Nav::Right),
                            KeyCode::OK | KeyCode::NUM5 => Some(H2Nav::Ok),
                            KeyCode::CLEAR => Some(H2Nav::Clr),
                            _ => None,
                        };
                        let wardrobe = probe_wardrobe(costume_book.as_ref());
                        let class = {
                            let rd = |a: u32, n: usize| emu.read_memory(a, n);
                            guest_build::h2_char_class(&rd, &h2)
                        };
                        let wearable = |a: u16, slot: u8| guest_build::h2_av_wearable(&avatar_sheets, a, slot, class);
                        let r = match key {
                            Some(k) => equip_model.key(k, v.cursor, &wardrobe, &wearable, class),
                            None if equip_model.list.is_some() => guest_build::H2EquipKey::Take { wear: None },
                            None => guest_build::H2EquipKey::Pass,
                        };
                        if let guest_build::H2EquipKey::Take { wear: Some((slot, id)) } = r {
                            let ok = match (costume_book.as_mut(), costume_io.as_ref()) {
                                (Some(b), Some(io)) => b.wear_instance(io, slot, id),
                                _ => false,
                            };
                            eprintln!("[h2-equip-ui] f={f} avatar slot {slot} wears instance {id:?} ok={ok}");
                        }
                        probe_equip_hide(&mut *emu, block, &equip_model, &mut equip_hidden, Some(&v), f);
                        match r {
                            guest_build::H2EquipKey::Take { .. } => {
                                eprintln!("[h2-equip-ui] f={f} {kc:?} taken — focus {:?} list {:?}", equip_model.focus, equip_model.list);
                                continue;
                            }
                            guest_build::H2EquipKey::Pass => eprintln!("[h2-equip-ui] f={f} {kc:?} is the game's (cursor {})", v.cursor),
                        }
                    }
                }
                if h2_options && let Some(h2) = build().hero2_java() {
                    let dir = match *kc {
                        KeyCode::UP => Some(guest_build::H2OptDir::Up),
                        KeyCode::DOWN => Some(guest_build::H2OptDir::Down),
                        KeyCode::LEFT => Some(guest_build::H2OptDir::Left),
                        KeyCode::RIGHT => Some(guest_build::H2OptDir::Right),
                        KeyCode::OK => Some(guest_build::H2OptDir::Ok),
                        _ => None,
                    };
                    let win = {
                        let rd = |a: u32, n: usize| emu.read_memory(a, n);
                        guest_build::h2_options_window(&rd, &h2).map(|(w, chain, _, _)| {
                            let cur = rd(w.fields + 4 + guest_build::Hero2Java::WIN_CURSOR_OFF, 1).first().map_or(0, |b| *b as i8);
                            (w, chain, cur)
                        })
                    };
                    if let (Some(dir), Some((w, chain, cur))) = (dir, win) {
                        opt_state.sync(&w, cur);
                        match opt_state.key(dir, &w, &chain) {
                            guest_build::H2OptKey::Pass => {}
                            guest_build::H2OptKey::Take { writes, change } => {
                                for (a, b) in &writes {
                                    emu.write_memory(*a, b);
                                }
                                if let Some((row, d)) = change {
                                    match row {
                                        5 => opt_drop = (opt_drop as i32 + i32::from(d)).rem_euclid(3) as usize,
                                        6 => diff_level = Some((i32::from(diff_level.unwrap_or(0)) + i32::from(d)).rem_euclid(3) as u8),
                                        7 => opt_respawn = (opt_respawn as i32 + i32::from(d)).rem_euclid(6) as usize,
                                        8 => opt_host.0 = !opt_host.0,
                                        9 => opt_host.1 = (opt_host.1 as i32 + i32::from(d)).rem_euclid(6) as usize,
                                        10 => opt_host.2 = (opt_host.2 as i32 + i32::from(d)).rem_euclid(3) as usize,
                                        // ★ 2026-10-28: the save actions (the app's `SaveXfer`; the probe only reports the press).
                                        r => eprintln!("[h2-opt] f={f} save action row {r} pressed"),
                                    }
                                }
                                eprintln!("[h2-opt] f={f} {kc:?} -> cursor {} top {} host {:?}", opt_state.cursor, opt_state.top, opt_host);
                                continue;
                            }
                        }
                    }
                }
                // ★★ 2026-10-18: the app's `sync_bag_full_row` — the refusal's notice row, chosen before the OK lands.
                if *kc == KeyCode::OK && build().hero2_java().is_some() {
                    h2_sync_bag_full_row(&mut *emu, f);
                }
                if h2_autoloot
                    && *kc == KeyCode::OK
                    && let Some(h2) = build().hero2_java()
                {
                    let stage_slot = GuestBuild::Hero2Ktf.hero2_save().map(|s| s.area.stage_slot).unwrap_or(0);
                    let text = {
                        let rd = |a: u32, n: usize| emu.read_memory(a, n);
                        guest_build::h2_loot_near(&rd, &h2, stage_slot)
                            .filter(|(_, near)| !near.is_empty() && near.iter().all(|l| !l.fits && l.dist > 0))
                            .and_then(|(_, near)| guest_build::h2_near_full_text(&rd, &near))
                    };
                    if let Some(text) = text {
                        let args = [JavaArg::Object(text), JavaArg::Int(16), JavaArg::Int(0)];
                        let ok = h2_call_quiet(&mut *emu, guest_build::JavaCall::Notice, None, &args).is_some();
                        eprintln!("[h2-loot] f={f} OK near a drop no bag has room for -> the bag-full notice {text:#x} (call issued={ok})");
                    }
                }
                if h2_reverse
                    && is_dir(*kc)
                    && let (Some(h2), Some(AppState::Absolute { addr, field, .. })) = (build().hero2_java(), build().app_state())
                {
                    let dir = match *kc {
                        KeyCode::UP => 1,
                        KeyCode::DOWN => 2,
                        KeyCode::LEFT => 3,
                        _ => 4,
                    };
                    let at = {
                        let rd = |a: u32, n: usize| emu.read_memory(a, n);
                        guest_build::h2_field_walk(&rd, &h2, addr, field).and_then(|_| guest_build::h2_reverse_write(&rd, &h2, dir))
                    };
                    if let Some(at) = at {
                        let ok = emu.write_memory(at, &[dir]);
                        eprintln!("[h2-walk] f={f} {kc:?} REVERSES mid-tile — facing {at:#x} <- {dir} (ok={ok})");
                    }
                }
                emu.handle_event(Event::Keydown(*kc));
                released.push((f + *hold, *kc));
                if is_dir(*kc) {
                    walk_owed = None; // a new press owns the next release
                    dir_pressed_at = Some(f);
                }
            } else if revive_prompt.is_none() && *kf < f && f < *kf + *hold && (f - *kf) % 6 == 0 {
                emu.handle_event(Event::Keyrepeat(*kc)); // hold => auto-repeat
            }
        }
        let due: Vec<KeyCode> = released.iter().filter(|(rf, _)| *rf == f).map(|(_, kc)| *kc).collect();
        released.retain(|(rf, _)| *rf != f);
        for kc in due {
            if h2_turn && is_dir(kc) && dir_pressed_at == Some(f) {
                eprintln!("[h2-walk] f={f} release of {kc:?} folded into the turn — not sent");
                continue;
            }
            if h2_drop_up.contains(&f) {
                eprintln!("[h2-walk] f={f} release of {kc:?} DROPPED on purpose — what the guest itself does with a release outside state 3");
            } else {
                emu.handle_event(Event::Keyup(kc));
            }
            if is_dir(kc) {
                walk_owed = Some((f, kc));
            } else if released.iter().any(|(_, k)| is_dir(*k)) {
                // The app's `relatch_walk`: an action's release while a direction is held.
                walk_relatch = Some(f);
            }
        }
        // The app's two walk rules (`emulator::WalkGuard`), on the scripted keys.
        if (h2_walk_guard || h2_relatch)
            && let (Some(h2), Some(AppState::Absolute { addr, field, .. })) = (build().hero2_java(), build().app_state())
        {
            let walk = {
                let rd = |a: u32, n: usize| emu.read_memory(a, n);
                guest_build::h2_field_walk(&rd, &h2, addr, field)
            };
            let held_dir = released.iter().rev().map(|(_, k)| *k).find(|k| is_dir(*k));
            if h2_relatch && let Some(armed) = walk_relatch {
                match (held_dir, walk) {
                    (None, _) => walk_relatch = None,
                    _ if f > armed + 75 => {
                        walk_relatch = None;
                        eprintln!("[h2-walk] f={f} deferred re-press dropped — the character never stood again");
                    }
                    (Some(dir), Some(w)) if w.state == guest_build::H2_WALK_STANDING => {
                        emu.handle_event(Event::Keydown(dir));
                        walk_relatch = None;
                        eprintln!("[h2-walk] f={f} {dir:?} re-pressed — the character stands again, {} frame(s) after the action", f - armed);
                    }
                    _ => {}
                }
            }
            if h2_walk_guard && let Some((since, dir)) = walk_owed {
                if held_dir.is_some() {
                    walk_owed = None;
                } else {
                    match walk {
                        None => {}
                        Some(w) if w.state != guest_build::H2_WALK_WALKING => walk_owed = None,
                        Some(_) if f >= since + 12 => {
                            emu.handle_event(Event::Keyup(dir));
                            walk_owed = Some((f, dir));
                            eprintln!("[h2-walk] f={f} {dir:?} release RE-SENT — still walking {} frame(s) after the release", f - since);
                        }
                        Some(_) => {}
                    }
                }
            }
        }
        // ★ 부활의 구: the app does not tick the guest while the box is up — nor does this.
        if revive_prompt.is_some() {
            revive_held_frames += 1;
            if revive_held_frames % 100 == 1 {
                eprintln!("[h2-revive] f={f} the guest is held under the box ({revive_held_frames} frame(s))");
            }
        } else if let Err(e) = emu.tick() {
            eprintln!("[probe] tick error at frame {f}: {e:?}");
            break;
        }
        if host_levelup_frame == Some(f) {
            // CHAR offsets mirrored from items.rs (LEVEL/EXP/EXP_NEXT/points) — items.rs
            // is not include!d here, same convention as the --verify-offsets block.
            const LV: u32 = 0xe2;
            const EXP: u32 = 0xe4;
            const NEXT: u32 = 0xe8;
            match build().level_up_fn() {
                None => eprintln!("[levelup] f={f} refused: checkLevelUp not located on {}", build().name()),
                Some(fun) => {
                    let c = {
                        let read = |a: u32, l: usize| emu.read_memory(a, l);
                        char_base(build(), &read)
                    };
                    let b = emu.read_memory(c + LV, 2);
                    let level = if b.len() == 2 { u16::from_le_bytes([b[0], b[1]]) } else { 0 };
                    let b = emu.read_memory(c + EXP, 4);
                    let exp = if b.len() == 4 { u32::from_le_bytes([b[0], b[1], b[2], b[3]]) } else { 0 };
                    let b = emu.read_memory(c + NEXT, 4);
                    let next = if b.len() == 4 { u32::from_le_bytes([b[0], b[1], b[2], b[3]]) } else { 0 };
                    let b = emu.read_memory(c + 0x18a, 2);
                    let sp0 = if b.len() == 2 { u16::from_le_bytes([b[0], b[1]]) } else { 0 };
                    let kp0 = emu.read_memory(c + 0x18c, 1).first().copied().unwrap_or(0);
                    let wrote = emu.write_memory(c + EXP, &(next.saturating_add(1)).to_le_bytes());
                    emu.call_guest_function(fun, &[]);
                    eprintln!(
                        "[levelup] f={f} issued: level={level} exp={exp}/{next} -> exp={} (write ok={wrote}), checkLevelUp={fun:#x}",
                        next.saturating_add(1)
                    );
                    lu_watch = Some((level, sp0, kp0));
                }
            }
        }
        if let Some((lv0, sp0, kp0)) = lu_watch {
            let read = |a: u32, l: usize| emu.read_memory(a, l);
            let c = char_base(build(), &read);
            let b = emu.read_memory(c + 0xe2, 2);
            let lv = if b.len() == 2 { u16::from_le_bytes([b[0], b[1]]) } else { 0 };
            if lv != lv0 {
                let sp = {
                    let b = emu.read_memory(c + 0x18a, 2);
                    if b.len() == 2 { u16::from_le_bytes([b[0], b[1]]) } else { 0 }
                };
                let kp = emu.read_memory(c + 0x18c, 1).first().copied().unwrap_or(0);
                let (hp, hpmax, spv) = {
                    let h = emu.read_memory(c + 0xf2, 2);
                    let hm = emu.read_memory(c + 0xec, 2);
                    let s = emu.read_memory(c + 0xf8, 4);
                    (
                        if h.len() == 2 { u16::from_le_bytes([h[0], h[1]]) } else { 0 },
                        if hm.len() == 2 { u16::from_le_bytes([hm[0], hm[1]]) } else { 0 },
                        if s.len() == 4 { u32::from_le_bytes([s[0], s[1], s[2], s[3]]) } else { 0 },
                    )
                };
                eprintln!(
                    "[levelup] f={f} LEVELLED: {lv0} -> {lv}, stat points {sp0} -> {sp} (+{}), skill points {kp0} -> {kp} (+{}), hp={hp}/{hpmax} sp={spv}",
                    sp.wrapping_sub(sp0),
                    kp.wrapping_sub(kp0)
                );
                lu_watch = None;
            }
        }
        if host_save_frame == Some(f) {
            match build().save_chain() {
                None => eprintln!("[hostsave] f={f} refused: no save chain on {}", build().name()),
                Some(sc) => {
                    let u32at = |a: u32| {
                        let b = emu.read_memory(a, 4);
                        (b.len() == 4).then(|| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
                    };
                    let names = match sc.names {
                        guest_build::SaveNames::SlPtrs { game, slot } => u32at(game).zip(u32at(slot)),
                        guest_build::SaveNames::Direct { game, slot } => Some((game, slot)),
                    };
                    let chain = host_save::Chain {
                        fn_serialise: sc.fn_serialise,
                        fn_slot_summary: sc.fn_slot_summary,
                        fn_write_record: sc.fn_write_record,
                        scratch: sc.scratch,
                    };
                    // Same order as `emulator.rs::save_begin`: clear the out-param word
                    // FIRST so a stale length cannot be mistaken for this save's.
                    let cleared = emu.write_memory(sc.scratch, &0u32.to_le_bytes());
                    let job = names.and_then(|(ng, ns)| host_save::Job::new(host_save::Reason::Manual, chain, ng, ns));
                    match (cleared, job) {
                        (true, Some(job)) => {
                            if let host_save::Next::Call { addr, args, argc, .. } = job.first_call() {
                                let res = Arc::new(wie_backend::GuestCallResult::new());
                                emu.call_guest_function_out(addr, &args[..argc], res.clone());
                                eprintln!(
                                    "[hostsave] f={f} serialise issued: fn={addr:#x} scratch={:#x} names=({:#x},{:#x})",
                                    sc.scratch, job.name_game, job.name_slot
                                );
                                hs_job = Some((job, res));
                            }
                        }
                        (false, _) => eprintln!("[hostsave] f={f} refused: scratch {:#x} not writable", sc.scratch),
                        (_, None) => eprintln!("[hostsave] f={f} refused: record names did not resolve"),
                    }
                }
            }
        }
        let mut hs_finished = false;
        if let Some((job, res)) = hs_job.as_mut() {
            let scratch_val = {
                let b = emu.read_memory(job.chain.scratch, 4);
                if b.len() == 4 { u32::from_le_bytes([b[0], b[1], b[2], b[3]]) } else { 0 }
            };
            match job.advance(res.poll(), scratch_val) {
                host_save::Next::Wait => {}
                host_save::Next::Call { addr, args, argc, step } => {
                    *res = Arc::new(wie_backend::GuestCallResult::new());
                    emu.call_guest_function_out(addr, &args[..argc], res.clone());
                    eprintln!("[hostsave] f={f} step {step:?} issued: fn={addr:#x} buf={:#x} len={}", job.buf, job.len);
                }
                host_save::Next::Done => {
                    eprintln!("[hostsave] f={f} DONE: payload {} B + slot summary written", job.len);
                    hs_finished = true;
                }
                host_save::Next::Failed(why) => {
                    eprintln!("[hostsave] f={f} FAILED: {why}");
                    hs_finished = true;
                }
            }
        }
        if hs_finished {
            hs_job = None;
        }
        // TEMPORARY (2026-08-06): the per-frame guest workload. The game rasterises
        // its world from guest code (the WIPI trace shows no DrawImage at all), so
        // this is the number that decides whether the lag is the interpreter or our
        // graphics layer.
        if f % 100 == 0 {
            let (b, fmt, tf, refused) = wie_util::stats::blit_snapshot();
            eprintln!(
                "[blit] f={f} calls={} hle={} px={} px_hle={} | fmt 4bpp={} 8bpp={} other={} | t0={} t1={} t2={} t3={} t4+={} | refused fmt={} tf={} geom={} mem={} null={}",
                b[0], b[1], b[2], b[3],
                fmt[0], fmt[1], fmt[2],
                tf[0], tf[1], tf[2], tf[3], tf[4] + tf[5] + tf[6] + tf[7] + tf[8] + tf[9],
                refused[0], refused[1], refused[2], refused[3], refused[4],
            );
            let (a, fr, walk, bh) = wie_util::stats::heap_snapshot();
            eprintln!(
                "[heap] f={f} allocs={a} frees={fr} LIVE={} list_walk_steps={walk} bucket_header_bytes={:.1} MB",
                a as i64 - fr as i64,
                bh as f64 / 1e6
            );
            // roadmap #12: the conservative root scan's own size. `words` is what it
            // EXAMINED — the cost — as against the 13~16 candidates it hands the
            // collector, and `stack` is the part that grows with `run_function` nesting.
            // The device prints the same numbers as `gc scan:`; this is its baseline.
            let (cs_n, cs_words, cs_stack, cs_kept, cs_max) = wie_util::stats::cons_scan_take();
            if cs_n > 0 {
                eprintln!(
                    "[gcscan] f={f} scans={cs_n} words/scan={} max={cs_max} stack/scan={} B candidates/scan={}",
                    cs_words / cs_n,
                    cs_stack / cs_n,
                    cs_kept / cs_n,
                );
            }
            // The guest SOUND path, which runs on THIS thread inside the guest's
            // `MC_mdaPlay`. `parses` is the only expensive part; with the parse cache it
            // should settle at the number of distinct sound files (Hero3 has 33) while
            // `plays` keeps climbing. `parses` tracking `plays` means the cache is not
            // being hit and every sound effect is paying for a full SMAF parse + sort
            // inside a frame. `handles` is the bookkeeping that used to grow forever.
            let au = wie_util::stats::audio_snapshot();
            eprintln!(
                "[snd]  f={f} loads={} plays={} parses={} ({:.0} KB) cache={} handles={} catchup={}",
                au[0], au[1], au[2], au[3] as f64 / 1024.0, au[4], au[5], au[6],
            );
            // ★★ THE KTF JAVA INTERFACE (2026-09-09). hero2's whole host/guest traffic
            // is name-resolved Java calls, and until this line existed there was no way
            // to see the cost: `--profile` samples the GUEST pc, so host time spent
            // walking method tables is invisible to it. `scan` and `name` are the work;
            // `hit%` is whether the resolution cache is doing its job.
            let cr = wie_util::stats::canvas_rows_snapshot();
            eprintln!("[rows] f={f} raw-copied={} composed={} fell-back={} calls", cr[0], cr[1], cr[2]);
            let (jbm, jbf, mem, scan, nr, nb) = wie_util::stats::jvm_snapshot();
            eprintln!(
                "[jvm]  f={f} jb_method={jbm} jb_field={jbf} | by-name members={mem} ({:.0}/frame) compared={scan} ({:.1}/lookup) | name parses={nr} bytes={nb} ({:.0}/frame)",
                mem as f64 / f.max(1) as f64,
                scan as f64 / mem.max(1) as f64,
                nb as f64 / f.max(1) as f64,
            );
            if phases {
                let h = wie_util::stats::phase_take();
                let total: u64 = h.iter().sum();
                let busy: u64 = total - h[wie_util::stats::PHASE_IDLE as usize];
                let mut parts: Vec<(usize, u64)> = h.iter().copied().enumerate().filter(|(i, n)| *n > 0 && *i != wie_util::stats::PHASE_IDLE as usize).collect();
                parts.sort_unstable_by_key(|(_, n)| core::cmp::Reverse(*n));
                let body = parts
                    .iter()
                    .map(|(i, n)| format!("{} {:.1}%", wie_util::stats::PHASE_NAMES[*i], *n as f64 * 100.0 / busy.max(1) as f64))
                    .collect::<Vec<_>>()
                    .join(" | ");
                eprintln!(
                    "[phases] f={f} busy {busy}/{total} samples ({:.0}% of wall) — {body}",
                    busy as f64 * 100.0 / total.max(1) as f64
                );
                // Host heap traffic over the same interval, by phase. `/frame` is per
                // TICK (100 of them between reports); divide by paints/100 for
                // per-painted-frame.
                let (ha, hb) = halloc_take();
                let total: u64 = ha.iter().sum();
                let mut parts: Vec<(usize, u64)> = ha.iter().copied().enumerate().filter(|(_, n)| *n > 0).collect();
                parts.sort_unstable_by_key(|(_, n)| core::cmp::Reverse(*n));
                let body = parts
                    .iter()
                    .map(|(i, n)| format!("{} {}", wie_util::stats::PHASE_NAMES[*i], n))
                    .collect::<Vec<_>>()
                    .join(" | ");
                eprintln!(
                    "[halloc] f={f} host allocs={total} ({:.0}/tick, {:.0} KB) — {body}",
                    total as f64 / 100.0,
                    hb as f64 / 1024.0
                );
            }
            {
                // The SVC mix, same gauge the app prints: which ids the guest is actually
                // spending its boundary crossings on. Ids in the JAVA category are the
                // method stub's address (`JavaMethod::register`), so `RUST_LOG=wie_ktf=debug`
                // names them.
                let (cats, hot) = wie_util::stats::svc_mix_take(8);
                let hot = hot.iter().map(|(id, n)| format!("{id:#x}={n}")).collect::<Vec<_>>().join(" ");
                eprintln!(
                    "[svc]  f={f} by category [hook {} init {} jif {} wipic {} java {} stdlib {}] hot: {hot}",
                    cats[0], cats[1], cats[2], cats[3], cats[4], cats[5]
                );
            }
            let (instr, svc, batches) = wie_util::stats::snapshot();
            let secs = wall_start.elapsed().as_secs_f64().max(1e-9);
            eprintln!(
                "[work] f={f} instr={instr} ({:.1} M/frame) svc={svc} batches={batches} | {:.1} M instr/s wall, virt {:.2}x",
                instr as f64 / (f.max(1) as f64) / 1e6,
                instr as f64 / secs / 1e6,
                (f as f64 * ms_per_frame as f64 / 1000.0) / secs,
            );
            // ★ 착수 게이트 (순서표 #8). `fps` 는 **가상초당 present 횟수**다 —
            // 게임의 자체 목표(6/10/14/18)에 붙으면 게임이 스스로 페이싱하는 것이고,
            // 틱률(= 1000/--ms)에 붙으면 페이싱이 없다. `host` 는 present 한 번의
            // 호스트 비용이고, `share` 는 그게 벽시계에서 차지하는 몫이다 —
            // 작으면 비용은 present 바깥(GC·게스트 실행)에 있다.
            let (_, gc_runs) = wie_util::stats::gc_snapshot();
            let np = paints.load(Ordering::SeqCst);
            let pns = paint_ns.load(Ordering::SeqCst);
            let vsecs = (f as f64 * ms_per_frame as f64 / 1000.0).max(1e-9);
            eprintln!(
                "[paint] f={f} paints={np} = {:.1}/virt-s (tick {:.0}/s) | present {:.2} ms ({:.1}% wall) | gc={} every {} | {:.1} ms host/painted-frame",
                np as f64 / vsecs,
                1000.0 / ms_per_frame as f64,
                if np > 0 { pns as f64 / np as f64 / 1e6 } else { 0.0 },
                pns as f64 / 1e9 / secs * 100.0,
                gc_runs,
                gc_every,
                // 벽시계를 그려진 프레임 수로 나눈 것 = 이 호스트가 한 프레임에 실제로
                // 쓴 시간. 게스트 주기(50 ms)보다 크면 그 기기는 목표를 못 맞춘다.
                if np > 0 { secs * 1000.0 / np as f64 } else { 0.0 },
            );
            // ★ 순서표 #8 의 본론. hero2 는 MIDP Graphics -> ImageBufferCanvas 로 그린다
            // (WIPI-C 블리터는 `[blit] calls=0`). fill 은 JavaImageBuffer 가 `fill_span` 을
            // 구현하지 않아 픽셀당 기본 경로로 떨어지고, draw 는 raw-row 경로가 src·dst
            // 둘 다 2 B/px 일 때만 열리는데 hero2 는 4/4 라 **도달 불가**다.
            // 그려진 프레임당 픽셀 수를 화면(176x220=38,720)과 비교하면 각 경로가 실제로
            // 몇 화면분인지 바로 읽힌다.
            let cv = wie_util::stats::canvas_snapshot();
            let per = |v: u64| if np > 0 { v as f64 / np as f64 } else { 0.0 };
            eprintln!(
                "[canvas] f={f} fill {} calls {:.0} px/frame ({:.2} screens) | draw {} calls {:.0} px/frame ({:.2} screens) | copy {} calls {:.0} px/frame",
                cv[0][0],
                per(cv[0][1]),
                per(cv[0][1]) / (w as f64 * h as f64),
                cv[1][0],
                per(cv[1][1]),
                per(cv[1][1]) / (w as f64 * h as f64),
                cv[2][0],
                per(cv[2][1]),
            );
        }
        // Stack high-water, every 500 frames (= 10 s of virtual time at --ms 20).
        // Flat after warm-up = healthy; a steady decline = the leak that overflows
        // the 8 MB `wie-emu` stack on the device.
        if f % 500 == 0 {
            if let Some(reserved) = stack_gauge::reserved_bytes() {
                if stack_reserve_min > reserved {
                    stack_reserve_min = reserved;
                    eprintln!("[stack] f={f} reserve left {} KB (used {} KB)", reserved / 1024, (stack_reserve_start - reserved) / 1024);
                }
            }
        }
        // --find-fb: after the scripted keys have run, hunt for the offscreen
        // framebuffer the game paints its MINIMAP into (measured: created once via
        // MC_grpCreateOffScreenFrameBuffer, painted pixel-by-pixel, then
        // CopyFrameBuffer'd to the screen). Report which CHAR field holds it.
        if find_fb && (f == frames || dumps.contains(&f)) {
            let rd = |a: u32, n: usize| emu.read_memory(a, n);
            let u32at = |a: u32| -> u32 {
                let b = rd(a, 4);
                if b.len() == 4 { u32::from_le_bytes([b[0], b[1], b[2], b[3]]) } else { 0 }
            };
            let char_ptr = char_base(build(), &rd);
            eprintln!("[probe] CHAR = {char_ptr:#x}");
            let mut found = 0;
            for off in (0..0xa800u32).step_by(4) {
                let cand = u32at(char_ptr + off);
                if !(0x4000_0000..0x5000_0000).contains(&cand) {
                    continue;
                }
                // WIPICFramebuffer { w, h, bpl, bpp, buf } - plausible small image?
                let (w, h, bpl, bpp) = (u32at(cand), u32at(cand + 4), u32at(cand + 8), u32at(cand + 12));
                if (8..=512).contains(&w) && (8..=512).contains(&h) && bpl == w * 2 && bpp == 16 {
                    eprintln!("[probe] CHAR+{off:#x} -> fb {cand:#x} {w}x{h} bpl={bpl} bpp={bpp} buf={:#x}", u32at(cand + 16));
                    found += 1;
                }
            }
            eprintln!("[probe] framebuffer-like CHAR fields: {found}");
            // Wider hunt: scan the guest heap for a WIPICFramebuffer-shaped record,
            // then find every static/CHAR word that points at it.
            let mut fbs: Vec<(u32, u32, u32)> = Vec::new();
            for base in (0x4900_0000u32..0x4930_0000).step_by(0x10000) {
                let blk = rd(base, 0x10000);
                if blk.len() < 0x10000 {
                    continue;
                }
                for i in (0..blk.len() - 20).step_by(4) {
                    let g = |k: usize| u32::from_le_bytes([blk[i + k], blk[i + k + 1], blk[i + k + 2], blk[i + k + 3]]);
                    let (w, h, bpl, bpp) = (g(0), g(4), g(8), g(12));
                    if (8..=512).contains(&w) && (8..=512).contains(&h) && bpl == w * 2 && bpp == 16 {
                        fbs.push((base + i as u32, w, h));
                    }
                }
            }
            eprintln!("[probe] heap framebuffer records: {}", fbs.len());
            for (a, w, h) in fbs.iter().take(8) {
                eprintln!("    fb {a:#x} {w}x{h}");
            }
            for (a, w, h) in fbs.iter().take(4) {
                let mut refs = 0;
                for (rbase, rlen) in statics_ranges(build()).iter().copied().chain([(char_ptr, 0xb000usize)]) {
                    let blk = rd(rbase, rlen);
                    for i in (0..blk.len().saturating_sub(4)).step_by(4) {
                        let v = u32::from_le_bytes([blk[i], blk[i + 1], blk[i + 2], blk[i + 3]]);
                        if v == *a {
                            let addr = rbase + i as u32;
                            eprintln!(
                                "    -> fb {a:#x} ({w}x{h}) referenced at {addr:#x} (CHAR+{:#x})",
                                addr.wrapping_sub(char_ptr),
                            );
                            refs += 1;
                        }
                    }
                }
                if refs == 0 {
                    eprintln!("    -> fb {a:#x} ({w}x{h}) no static/CHAR reference found");
                }
            }
        }
        // --map-check: prove that the host can render the game's minimap ITSELF, from
        // the map arrays in RAM, without ever opening the guest's map window.
        //
        // Reimplements guest 0x17ba44 (the map window's ENTER handler) exactly:
        //   MAP  = *(*(CHAR+0x9c90)) + 8      (KTF indirect handle, +8 data offset)
        //   w,h  = i32 @MAP+0x48 / +0x4c      (tiles); the buffer is 2w x 2h
        //   attr = *(*(MAP+0x60)) + 8         u8[w*h], bit 7 = blocked   (guest 0x17a6f8)
        //   evt  = *(*(MAP+0x68)) + 8         i8[w*h], >0 = event tile   (guest 0x17a6b4)
        // per tile (guest 0x17b874): blocked ? 0x5b5b5b : 0xd9d4be, then event -> 0x225acc
        // (except at tile 0,0); per pixel (guest 0x17b8e4) a corner blend with the two
        // neighbours in that quadrant, averaged per channel, skipped when the base
        // matches either neighbour.
        // The run then compares that against the buffer the guest itself painted, so a
        // mismatch is a bug in the reimplementation and not a matter of opinion.
        if map_check && dumps.contains(&f) {
            let rd = |a: u32, n: usize| emu.read_memory(a, n);
            let u32at = |a: u32| -> Option<u32> {
                let b = rd(a, 4);
                (b.len() == 4).then(|| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
            };
            let cp = char_base(build(), &rd);
            // ★ The SHIPPING renderer, included from wie_android — not a copy of it.
            let render = render_minimap_rgb(&rd, cp, chain()).map(|(w, h, px)| (u32::from(w), u32::from(h), px));
            if let Some((w, h, _)) = &render {
                // The marker is drawn to the SCREEN, not into the buffer, so it is
                // reported rather than stamped: the diff below must stay like-for-like.
                match player_tile(&rd, cp, (*w / 2) as i32, (*h / 2) as i32, chain()) {
                    Some((tx, ty)) => {
                        // The attribute byte under the marker, for context. NOT an
                        // assertion: the tile grid is a coarse quantisation of a
                        // pixel-space position, so standing against a wall can round
                        // onto it — and the game's own marker lands there too, which
                        // is the only standard that matters here.
                        let mapw = (*w / 2) as i32;
                        // Same hop count as the renderer, whichever build this is.
                        let hop = |field: u32| -> u32 {
                            let first = u32at(field).unwrap_or(0);
                            match chain().data_off {
                                Some(off) => u32at(first).unwrap_or(0).wrapping_add(off),
                                None => first,
                            }
                        };
                        let attr_p = hop(hop(cp + MAP_HANDLE_OFF) + 0x60);
                        let a = rd(attr_p + (ty * mapw + tx) as u32, 1).first().copied().unwrap_or(0xff);
                        eprintln!("[map] f={f}: player tile ({tx},{ty}) of {}x{}  attr={a:#04x}", w / 2, h / 2);
                    }
                    None => eprintln!("[map] f={f}: player tile UNRESOLVED"),
                }
            }
            let Some((fw, fh, ours)) = render else {
                eprintln!("[map] f={f}: could not resolve the map arrays");
                continue;
            };
            // The guest's own buffer, for comparison (only valid once its map window
            // has painted at least once).
            let theirs = (|| -> Option<(u32, u32, Vec<u32>)> {
                let (handle, gw, gh) = wie_backend::last_offscreen_copy()?;
                // `WIPICIndirectPtr` -> payload, per carrier runtime: `*(h)+8` on KTF,
                // the identity on LGT. Both the framebuffer record and its pixel
                // buffer are reached that way.
                let payload = |p: u32| -> Option<u32> {
                    match chain().data_off {
                        Some(off) => u32at(p).map(|v| v.wrapping_add(off)),
                        None => Some(p),
                    }
                };
                let sp = payload(handle)?;
                let hdr = rd(sp, 20);
                if hdr.len() < 20 {
                    return None;
                }
                let g = |i: usize| u32::from_le_bytes([hdr[i], hdr[i + 1], hdr[i + 2], hdr[i + 3]]);
                if g(0) != gw || g(4) != gh || g(12) != 16 {
                    return None;
                }
                let data = payload(g(16))?;
                let raw = rd(data, (gw * gh * 2) as usize);
                if raw.len() < (gw * gh * 2) as usize {
                    return None;
                }
                let px = (0..(gw * gh) as usize)
                    .map(|i| {
                        let v = u16::from_le_bytes([raw[i * 2], raw[i * 2 + 1]]);
                        let (r, gg, b) = ((v >> 11) & 0x1f, (v >> 5) & 0x3f, v & 0x1f);
                        ((u32::from(r) * 255 / 31) << 16) | ((u32::from(gg) * 255 / 63) << 8) | (u32::from(b) * 255 / 31)
                    })
                    .collect();
                Some((gw, gh, px))
            })();
            match theirs {
                None => eprintln!("[map] f={f}: ours {fw}x{fh} (guest buffer not available yet)"),
                Some((gw, gh, _)) if (gw, gh) != (fw, fh) => {
                    eprintln!("[map] f={f}: SIZE MISMATCH ours {fw}x{fh} vs guest {gw}x{gh}");
                }
                Some((gw, gh, theirs)) => {
                    // RGB565 is lossy, so compare after quantising ours the same way.
                    let q = |c: u32| {
                        let (r, g, b) = ((c >> 16) & 0xff, (c >> 8) & 0xff, c & 0xff);
                        let v = ((r >> 3) << 11) | ((g >> 2) << 5) | (b >> 3);
                        let (r, g, b) = ((v >> 11) & 0x1f, (v >> 5) & 0x3f, v & 0x1f);
                        ((r * 255 / 31) << 16) | ((g * 255 / 63) << 8) | (b * 255 / 31)
                    };
                    let mut bad = 0usize;
                    let mut samples = Vec::new();
                    for i in 0..ours.len() {
                        if q(ours[i]) != theirs[i] {
                            bad += 1;
                            if samples.len() < 8 {
                                samples.push((i as u32 % gw, i as u32 / gw, q(ours[i]), theirs[i]));
                            }
                        }
                    }
                    eprintln!(
                        "[map] f={f}: {gw}x{gh}  mismatch {bad}/{} ({:.3}%)",
                        ours.len(),
                        bad as f64 * 100.0 / ours.len() as f64
                    );
                    for (x, y, a, b) in samples {
                        eprintln!("        ({x},{y}) ours {a:#08x} guest {b:#08x}");
                    }
                }
            }
        }
        // --map-hunt: find the per-map object on a build whose chain is not known yet.
        //
        // Structural rather than address-based, because a KTF address means nothing in
        // the LGT compile (MIGRATION_LGT.md §1) while the STRUCT LAYOUT does carry
        // over. So: sweep every word of the statics and of the CHAR record, try both
        // pointer conventions on it, and keep the ones that land on something
        // map-shaped — plausible tile dimensions at +0x48/+0x4c with two readable
        // `w*h` byte arrays at +0x60/+0x68. Reported with the blocked/event tile
        // counts, which is what identifies the right candidate against the picture the
        // game's own map window paints.
        if map_hunt && dumps.contains(&f) {
            let rd = |a: u32, n: usize| emu.read_memory(a, n);
            let u32at = |a: u32| -> Option<u32> {
                let b = rd(a, 4);
                (b.len() == 4).then(|| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
            };
            let cp = char_base(build(), &rd);
            let heap = |v: u32| (0x4000_0000..0x5000_0000).contains(&v);
            // Both conventions, tried side by side: `Some(8)` = KTF handle, `None` =
            // LGT direct. The hunt does not assume the build's own answer is right,
            // because "which convention" is exactly one of the things being confirmed.
            let convs: [(&str, Option<u32>); 2] = [("direct", None), ("handle+8", Some(8))];
            let mut hits = 0usize;
            // Windows to sweep: the statics (which on LGT contain the CHAR record) and
            // the CHAR record itself (which on KTF is on the heap).
            let mut windows: Vec<(u32, usize)> = statics_ranges(build()).to_vec();
            if heap(cp) {
                windows.push((cp, 0xb000));
            }
            for (wbase, wlen) in windows {
                let blk = rd(wbase, wlen);
                for i in (0..blk.len().saturating_sub(4)).step_by(4) {
                    let field = wbase + i as u32;
                    let raw = u32::from_le_bytes([blk[i], blk[i + 1], blk[i + 2], blk[i + 3]]);
                    if !heap(raw) {
                        continue;
                    }
                    for (mname, madj) in convs {
                        let Some(map) = (match madj {
                            None => Some(raw),
                            Some(off) => u32at(raw).map(|v| v.wrapping_add(off)).filter(|v| heap(*v)),
                        }) else {
                            continue;
                        };
                        let (Some(mw), Some(mh)) = (u32at(map + 0x48), u32at(map + 0x4c)) else {
                            continue;
                        };
                        let (mw, mh) = (mw as i32, mh as i32);
                        if !(1..=200).contains(&mw) || !(1..=200).contains(&mh) {
                            continue;
                        }
                        let n = (mw * mh) as usize;
                        for (aname, aadj) in convs {
                            let arr = |off: u32| -> Option<Vec<u8>> {
                                let h = u32at(map + off).filter(|v| heap(*v))?;
                                let p = match aadj {
                                    None => h,
                                    Some(a) => u32at(h).map(|v| v.wrapping_add(a)).filter(|v| heap(*v))?,
                                };
                                let b = rd(p, n);
                                (b.len() == n).then_some(b)
                            };
                            let (Some(attr), Some(event)) = (arr(0x60), arr(0x68)) else { continue };
                            let blocked = attr.iter().filter(|b| **b & 0x80 != 0).count();
                            let events = event.iter().filter(|b| (**b as i8) > 0).count();
                            // An all-zero or all-blocked "attribute" array is what a
                            // random heap block looks like; a real map has some of both.
                            let plausible = blocked > 0 && blocked < n;
                            eprintln!(
                                "[hunt] f={f} field {field:#x} (CHAR+{:#x}) {mname} -> map {map:#x} {mw}x{mh} arrays {aname}: blocked {blocked}/{n} events {events}{}",
                                field.wrapping_sub(cp),
                                if plausible { "  <= PLAUSIBLE" } else { "" }
                            );
                            // The event tiles, listed: these are the map's warps, so
                            // they are the coordinates to walk a probe run TOWARDS when
                            // the next thing to verify is a second map.
                            //
                            // ⚠ Printed per CANDIDATE, and the `{aname}` convention is
                            // part of the line, because the wrong convention can still
                            // produce a "plausible" array: on KTF the `direct` reading
                            // of the same map object yields 818 blocked / 21 events and
                            // the `handle+8` one 828 / 12, and only the latter is real
                            // (it is the one whose render matches the guest's own
                            // buffer). The pixel diff is the arbiter, not this list.
                            if plausible {
                                let evs: Vec<String> = (0..n)
                                    .filter(|i| (event[*i] as i8) > 0)
                                    .take(32)
                                    .map(|i| format!("({},{})={}", i as i32 % mw, i as i32 / mw, event[i] as i8))
                                    .collect();
                                eprintln!(
                                    "[hunt]   arrays {aname}: player {:?} event tiles: {}",
                                    player_tile(&rd, cp, mw, mh, chain()),
                                    evs.join(" ")
                                );
                            }
                            hits += 1;
                        }
                    }
                }
            }
            eprintln!("[hunt] f={f}: {hits} map-shaped candidates (CHAR {cp:#x})");
        }
        if trace_state && dumps.contains(&f) {
            // Map signature (CHAR+0x9bcc, walk-invariant / map-sensitive) + the
            // framebuffer the guest last copied to screen (the HUD's minimap source).
            let rd = |a: u32, n: usize| emu.read_memory(a, n);
            let cp = char_base(build(), &rd);
            let sig = rd(cp + 0x9bcc, 8);
            eprintln!(
                "[mm] f={f} sig={} copy={:x?}",
                sig.iter().map(|b| format!("{b:02x}")).collect::<String>(),
                wie_backend::last_offscreen_copy()
            );
        }
        if gauges > 0 && f % gauges == 0 {
            let rd = |a: u32, n: usize| emu.read_memory(a, n);
            let cp = char_base(build(), &rd);
            let rd_u = |a: u32, n: usize| -> Option<u32> {
                let b = rd(a, n);
                (b.len() == n).then(|| b.iter().rev().fold(0u32, |acc, x| (acc << 8) | u32::from(*x)))
            };
            // The state word first: on hero2 it is an ABSOLUTE static and readable even
            // when no character exists, which is exactly the boot case one wants to see.
            let state = match build().app_state() {
                Some(st @ AppState::Absolute { addr, field, window, .. }) => rd_u(addr, 4).map(|v| {
                    // ★ hero2 (2026-09-18): the script box's mode rides along — `4` is
                    // the script screen and `bb.b:B` says narration / OK-wait / CHOICE.
                    let script = match build().hero2_java() {
                        Some(h2) if st.is_script(v) == Some(true) => {
                            let m = rd(h2.script_mode_slot, 1).first().copied().unwrap_or(0);
                            let sel = rd(h2.script_choice_sel_slot, 1).first().copied().unwrap_or(0);
                            format!(" script mode={m}{} sel={sel}", if m == 3 { " CHOICE" } else { "" })
                        }
                        _ => String::new(),
                    };
                    format!("{v} (field={field} window={window}) @{addr:#x}{script}")
                }),
                Some(AppState::CharRelative { off, field, window, .. }) if cp != 0 => {
                    rd_u(cp + off, 4).map(|v| format!("{v} (field={field} window={window}) @CHAR+{off:#x}"))
                }
                _ => None,
            };
            match (build().char_layout(), cp) {
                (Some(l), c) if c != 0 => {
                    let sp_max = match l.sp_max {
                        Some(off) => rd_u(c + off, 4),
                        None => Some(u32::from(l.sp_max_const)),
                    };
                    eprintln!(
                        "[gauges] f={f} CHAR={c:#x} lv={:?} hp={:?}/{:?} sp={:?}/{:?} exp={:?}/{:?} state={}",
                        rd_u(c + l.level, 2),
                        rd_u(c + l.hp, 4),
                        rd_u(c + l.hp_max, 4),
                        rd_u(c + l.sp, 4),
                        sp_max,
                        rd_u(c + l.exp, 4),
                        rd_u(c + l.exp_next, 4),
                        state.unwrap_or_else(|| "-".to_string()),
                    );
                    // hero2's combo, raw (§8.8): stage a.i:B +0xb0 · hit cost a.j:I +0xb8 ·
                    // chain a.o:[B +0xac · open stages a.h:B +0xa8 · skill levels a.p:[B +0x138.
                    if build().hero2_java().is_some() {
                        let arr = |off: u32| rd_u(c + off, 4).and_then(|h| guest_build::h2_byte_array(&rd, h)).map(|v| v.iter().map(|&b| b as i8).collect::<Vec<_>>());
                        // a.c:[S +0xdc / a.d:[S +0xe0 — the weapon values a.m()V copies in (a.c[a.n] = bo.d()S).
                        let shorts = |off: u32| rd_u(c + off, 4).and_then(|h| guest_build::h2_array(&rd, h)).map(|(base, len)| {
                            let b = rd(base, len as usize * 2);
                            b.chunks_exact(2).map(|p| i16::from_le_bytes([p[0], p[1]])).collect::<Vec<_>>()
                        });
                        eprintln!(
                            "[h2-combo] f={f} stage={:?} cost={:?} open={:?} chain={:?} levels={:?} a.c={:?} a.d={:?} a.n/a.o={:?}/{:?}",
                            rd_u(c + 0xb0, 4).map(|v| v as i32),
                            rd_u(c + 0xb8, 4).map(|v| v as i32),
                            rd_u(c + 0xa8, 4).map(|v| v as i32),
                            arr(0xac),
                            arr(0x138),
                            shorts(0xdc),
                            shorts(0xe0),
                            rd_u(0x0019_45cc, 4).map(|v| v as i32),
                            rd_u(0x0019_45dc, 4).map(|v| v as i32),
                        );
                        // 2026-09-27: facing g.u +0x40 · tile ad.d/ad.e +0x10/+0x14 · the three
                        // stance flags a.q 신의가호 +0x114 · a.r 철괴(방어의지) +0x118 · a.s 민첩태세 +0x120.
                        // … and the buff list g.b:Vector (+0x38): per `av` (id c:B +0, elapsed b:S +8, length a:I +0xc).
                        let buffs: Vec<(i32, i32, i32, i32, i32)> = rd_u(c + 0x38, 4)
                            .and_then(|v| guest_build::h2_fields(&rd, v))
                            .and_then(|vf| Some((rd_u(vf + 4, 4)?, rd_u(vf + 8, 4)?)))
                            .and_then(|(data, n)| guest_build::h2_array(&rd, data).map(|(base, len)| (base, n.min(len).min(16))))
                            .map(|(base, n)| {
                                (0..n)
                                    .filter_map(|i| rd_u(base + 4 * i, 4))
                                    .filter_map(|e| guest_build::h2_fields(&rd, e))
                                    .map(|b| {
                                        let w = |o: u32| rd_u(b + 4 + o, 4).map_or(-1, |v| v as i32);
                                        (w(0x0), w(0x4), w(0x8), w(0xc), w(0x10))
                                    })
                                    .collect()
                            })
                            .unwrap_or_default();
                        eprintln!(
                            "[h2-flags] f={f} buffs(id,unit,elapsed,len,ended)={buffs:?} facing={:?} tile=({:?},{:?}) 신의가호={:?} 철괴={:?} 민첩태세={:?}",
                            rd_u(c + 0x40, 4).map(|v| v as i32),
                            rd_u(c + 0x10, 4).map(|v| v as i32),
                            rd_u(c + 0x14, 4).map(|v| v as i32),
                            rd_u(c + 0x114, 4).map(|v| v as i32),
                            rd_u(c + 0x118, 4).map(|v| v as i32),
                            rd_u(c + 0x120, 4).map(|v| v as i32),
                        );
                        // ★ 2026-10-08 가디언 밸런스: the derived numbers a.m()V keeps — ATK a.g:I 0x9c · DEF a.i:I 0xa4 · the miss
                        //   base a.k:I 0xbc · CRIT a.l:I 0xc0 · the evade base a.m:I 0xc8 · the resist a.n:I 0xcc · the STR term
                        //   a.h:I 0xa0 · the basic hit's SP a.j:I 0xb8 · HP/SP max 0x78/0x7c · the five base stats 0x84..0x94.
                        let w = |o: u32| rd_u(c + o, 4).map_or(-1, |v| v as i32);
                        eprintln!(
                            "[h2-stats] f={f} lv={} atk={} def={} crit={} miss_base={} evade_base={} resist={} str_term={} hit_sp={} hpmax={} spmax={} stats=({},{},{},{},{}) bonus=({},{},{},{},{})",
                            w(0x5c), w(0x9c), w(0xa4), w(0xc0), w(0xbc), w(0xc8), w(0xcc), w(0xa0), w(0xb8), w(0x78), w(0x7c), w(0x84), w(0x88), w(0x8c), w(0x90), w(0x94),
                            rd_u(c + 0x98, 4).and_then(|h| guest_build::h2_array(&rd, h)).and_then(|(b, _)| rd_u(b, 2)).map_or(-1, |v| v as i16 as i32),
                            rd_u(c + 0x98, 4).and_then(|h| guest_build::h2_array(&rd, h)).and_then(|(b, _)| rd_u(b + 2, 2)).map_or(-1, |v| v as i16 as i32),
                            rd_u(c + 0x98, 4).and_then(|h| guest_build::h2_array(&rd, h)).and_then(|(b, _)| rd_u(b + 4, 2)).map_or(-1, |v| v as i16 as i32),
                            rd_u(c + 0x98, 4).and_then(|h| guest_build::h2_array(&rd, h)).and_then(|(b, _)| rd_u(b + 6, 2)).map_or(-1, |v| v as i16 as i32),
                            rd_u(c + 0x98, 4).and_then(|h| guest_build::h2_array(&rd, h)).and_then(|(b, _)| rd_u(b + 8, 2)).map_or(-1, |v| v as i16 as i32),
                        );
                    }
                }
                (Some(_), _) => eprintln!("[gauges] f={f} CHAR unresolved (anchor chain not in the guest heap yet) state={}", state.unwrap_or_else(|| "-".to_string())),
                (None, _) => eprintln!("[gauges] f={f} no char_layout for {}", build().name()),
            }
            // ★ hero2's Java-side quickbar (2026-09-17, HERO2_QUICKSLOT_TOUCH_PLAN.md S1):
            // `ag.j` bytes + `ag.a` cooldowns + class byte + the popup chain root, decoded
            // through the shared `Hero2Java` table — the same reads `items::h2_scan_quickbar`
            // does, so a wrong value here is wrong on the device too.
            if let Some(h2) = build().hero2_java() {
                let arr = |slot: u32, esz: usize| -> Option<Vec<i32>> {
                    let h = rd_u(slot, 4).filter(|p| (0x4000_0000..0x5000_0000).contains(p))?;
                    let pf = rd_u(h, 4).filter(|p| (0x4000_0000..0x5000_0000).contains(p))?;
                    let len = rd_u(pf + 4, 4)?;
                    (len <= 64).then_some(())?;
                    let raw = rd(pf + 8, len as usize * esz);
                    (raw.len() == len as usize * esz).then(|| {
                        raw.chunks_exact(esz)
                            .map(|c| match esz {
                                1 => i32::from(c[0] as i8),
                                2 => i32::from(i16::from_le_bytes([c[0], c[1]])),
                                _ => i32::from_le_bytes([c[0], c[1], c[2], c[3]]),
                            })
                            .collect()
                    })
                };
                let class = rd(h2.class_slot, 1).first().copied();
                let menu = rd_u(h2.menu_slot, 4).unwrap_or(0);
                let (ox, oy) = (rd_u(h2.menu_origin.0, 4), rd_u(h2.menu_origin.1, 4));
                let scr = (rd_u(h2.screen.0, 4), rd_u(h2.screen.1, 4), rd_u(h2.screen.2, 4), rd_u(h2.screen.3, 4));
                // ★ 2026-10-02: `ticks` = the cooldown clock (runs of ag.i()V since the install).
                let ticks = cd_clock_block.and_then(|b| guest_build::h2_cd_clock_read(&rd, b));
                eprintln!(
                    "[quickbar] f={f} ag.j={:?} cd={:?} ticks={ticks:?} class={class:?} menu={menu:#x} origin=({ox:?},{oy:?}) screen={scr:?}",
                    arr(h2.quickbar_slot, 1),
                    arr(h2.cooldown_slot, 4),
                );
                // The walk machine (조이스틱 2026-09-24): `g.t` motion state · `g.u` facing ·
                // `ag.k`/`ag.l` the pending request. `t=2` with no key held is a walk the
                // guest was never told to stop.
                if let Some(w) = guest_build::h2_walk(&rd, &h2) {
                    let pos = guest_build::h2_player_px(&rd, h2.char_slot);
                    eprintln!("[h2-walk] f={f} t={} u={} k={} l={} pos={pos:?}", w.state, w.facing, w.request, w.request_dir);
                }
                // hero2's live stage, the minimap's source. The grid is printed IN FULL
                // the first time it resolves and on every map change — it is the only
                // way to check the row/column orientation without a device, and a
                // transposed map looks perfectly plausible until you compare it.
                if let Some(sv) = GuestBuild::Hero2Ktf.hero2_save()
                    && let Some(m) = guest_build::h2_map(&rd, &sv.area, h2.char_slot)
                {
                    let sig: u64 = m.attr.iter().fold(1469598103934665603u64, |h, b| (h ^ u64::from(*b)).wrapping_mul(1099511628211));
                    // ★ 2026-09-29: the minimap's cells — `E` = a way to another map (the
                    // blue bar), `C` = a treasure chest still closed (the green box), from
                    // the map's own trigger tables (`h2_trigger_groups`) + the event flags.
                    let triggers = h2_trigger_cache.triggers(&rd, &sv.area, &m).cloned();
                    let flags = guest_build::h2_event_flags(&rd, &sv.area);
                    let cells = guest_build::h2_minimap_cells(&m, triggers.as_ref(), flags.as_deref());
                    eprintln!(
                        "[h2-map] f={f} map#{:?} {}x{} player={:?} marker={:?} px={:?} sig={sig:#018x} can_save={} exit_groups={:?} chests(group,flag,opened)={:?}",
                        guest_build::h2_stage_map_no(&rd, &sv.area),
                        m.w,
                        m.h,
                        m.player,
                        guest_build::h2_player_marker(&rd, &sv.area, h2.char_slot),
                        guest_build::h2_player_px(&rd, h2.char_slot),
                        guest_build::h2_can_save_here(&rd, &sv.area),
                        triggers.as_ref().map(|t| t.exits.iter().enumerate().filter(|(_, x)| **x).map(|(g, _)| g).collect::<Vec<_>>()),
                        triggers.as_ref().map(|t| t
                            .chests
                            .iter()
                            .enumerate()
                            .filter_map(|(g, c)| c.map(|fl| (g, fl, flags.as_deref().map(|fs| guest_build::h2_flag_on(fs, fl)))))
                            .collect::<Vec<_>>()),
                    );
                    if last_map_sig != Some(sig) {
                        last_map_sig = Some(sig);
                        for y in 0..m.h {
                            let row: String = (0..m.w)
                                .map(|x| {
                                    if m.player == Some((x as i32, y as i32)) {
                                        '@'
                                    } else {
                                        match cells[(y * m.w + x) as usize] {
                                            guest_build::H2_MM_EXIT => 'E',
                                            guest_build::H2_MM_CHEST => 'C',
                                            guest_build::H2_MM_FLOOR => '.',
                                            _ => '#',
                                        }
                                    }
                                })
                                .collect();
                            eprintln!("[h2-map]   {row}");
                        }
                    }
                    // ★ 2026-09-26 (사자후): the stage's ENTITY grid `bf.a:[[Lad;` (+0x30), the
                    // one `a.e(B)V` scans, counted the way its `instanceof bd` counts — `bd` and
                    // the four concrete subclasses of the abstract `ah` — and split by the
                    // monsters' motion state `g.t` (abs +0x3c: 6 dying · 7 dead) and by the
                    // visible screen around the player. With the whole-map roar in, the
                    // off-screen ones must die too; with the shipped scan they cannot.
                    let monster_descs = [0x0019_2670u32, 0x0019_23e8, 0x0019_29ac, 0x0019_2acc, 0x0019_3090]; // bd · al · ca · ci · m
                    let grid = guest_build::h2_static_handle(&rd, sv.area.stage_slot)
                        .and_then(|s| guest_build::h2_fields(&rd, s))
                        .and_then(|b| rd_u(b + 4 + 0x30, 4))
                        .and_then(|g| guest_build::h2_array(&rd, g));
                    if let (Some((rows_base, rows)), Some((px, py))) = (grid, m.player) {
                        let (half_w, half_h) = (scr.0.unwrap_or(176) as i32 / 32, scr.1.unwrap_or(220) as i32 / 32);
                        let mut seen = std::collections::HashSet::new();
                        let (mut alive, mut off, mut dying, mut dead, mut cols) = (0, 0, 0, 0, 0u32);
                        // HP / max = `bd.d:S` / `bd.c:S` (abs +0x74 / +0x70: `ad` 0x34 + `g` 0x28 + 0x18/0x14 —
                        // the pair the target panel's bar reads, HERO2_GUEST_RE.md §8.20).
                        let (mut hp_sum, mut hurt) = (0i64, 0);
                        let mut detail: Vec<String> = Vec::new();
                        for y in 0..rows {
                            let Some((row_base, len)) = rd_u(rows_base + y * 4, 4).and_then(|r| guest_build::h2_array(&rd, r)) else { continue };
                            cols = cols.max(len);
                            for x in 0..len {
                                let Some(e) = rd_u(row_base + x * 4, 4).filter(|e| *e != 0) else { continue };
                                if !guest_build::h2_class_desc(&rd, e).is_some_and(|d| monster_descs.contains(&d)) || !seen.insert(e) {
                                    continue;
                                }
                                {
                                    // (tile, g.t, g.w abs +0x48, HP bd.d:S +0x74 / max bd.c:S +0x70)
                                    let fb = guest_build::h2_fields(&rd, e).unwrap_or(0);
                                    let w = |off: u32| rd_u(fb + 4 + off, 4).map_or(-1, |v| v as i32);
                                    if detail.len() < 16 {
                                        let cls = match guest_build::h2_class_desc(&rd, e) {
                                            Some(0x0019_2670) => "bd",
                                            Some(0x0019_23e8) => "al",
                                            Some(0x0019_29ac) => "ca",
                                            Some(0x0019_2acc) => "ci",
                                            Some(0x0019_3090) => "m",
                                            _ => "?",
                                        };
                                        // 2026-09-27: + the stun flag g.k:Z (abs 0x54) the game's bd.a(I)V sets.
                                        // ★ 2026-10-06: (x,y) is the first grid tile it covers (a boss covers several —
                                        //   오딘 `al.t()V` 3×5), @ its own tile `ad.d/ad.e` (abs 0x10/0x14).
                                        // ★ 2026-10-06: a boss (`ah`'s four) shows its guard `ah.e:Z` (abs 0xc4 = bd's 0x5c + 0x48 + 0x20) —
                                        //   while it is set the boss's own `bd.a(La;BIZBBBB)Z` (0x115fb4) turns every hit away.
                                        let guard = if cls != "bd" && w(0xc4) > 0 { " GUARD" } else { "" };
                                        // ★ 2026-10-08 가디언: + the level bd.n (abs 0x6c) and the attack bd.e (0x78) the template gave it.
                                        detail.push(format!("{cls}({x},{y})@({},{}) t{} w{} {}/{}{}{guard} LV{} atk{}", w(0x10), w(0x14), w(0x3c), w(0x48), w(0x74), w(0x70), if w(0x54) > 0 { " STUN" } else { "" }, w(0x6c), w(0x78)));
                                    }
                                }
                                match guest_build::h2_fields(&rd, e).and_then(|b| rd(b + 4 + 0x3c, 1).first().copied()) {
                                    Some(6) => dying += 1,
                                    Some(7) => dead += 1,
                                    _ => {
                                        alive += 1;
                                        if (x as i32 - px).abs() > half_w || (y as i32 - py).abs() > half_h {
                                            off += 1;
                                        }
                                        let fb = guest_build::h2_fields(&rd, e).unwrap_or(0);
                                        let (hp, max) = (rd_u(fb + 4 + 0x74, 4).unwrap_or(0) as i32, rd_u(fb + 4 + 0x70, 4).unwrap_or(0) as i32);
                                        hp_sum += i64::from(hp);
                                        if hp < max {
                                            hurt += 1;
                                        }
                                    }
                                }
                            }
                        }
                        eprintln!("[h2-mobs] f={f} grid {rows}x{cols} monsters={} alive={alive} (off-screen {off}, hurt {hurt}, hp sum {hp_sum}) dying={dying} dead={dead}  {}", seen.len(), detail.join(" · "));
                    }
                    // ★ 2026-10-06: the beast's four slots as the cast sees them — type (bn.j abs 0x3c) · level (bn.k 0x40) ·
                    //   SP cost (bn.b:[S 0x4c, what bn.a(BBBB)V takes) · cooldown (bn.a:[S 0x48). `--h2-beast-tune 0` = shipped.
                    let beast_cost = guest_build::h2_static_handle(&rd, h2.char_slot)
                        .and_then(|ch| guest_build::h2_fields(&rd, ch))
                        .and_then(|fb| rd_u(fb + 4 + h2.beast_off, 4))
                        .and_then(|b| guest_build::h2_fields(&rd, b))
                        .map(|bfb| {
                            let bytes = |o: u32| rd_u(bfb + 4 + o, 4).and_then(|h| guest_build::h2_byte_array(&rd, h)).unwrap_or_default();
                            let shorts = |o: u32| -> Vec<i16> {
                                rd_u(bfb + 4 + o, 4)
                                    .and_then(|h| guest_build::h2_array(&rd, h))
                                    .map(|(base, len)| (0..len.min(8)).map(|i| rd(base + 2 * i, 2)).map(|v| if v.len() == 2 { i16::from_le_bytes([v[0], v[1]]) } else { -1 }).collect())
                                    .unwrap_or_default()
                            };
                            let (ty, lv, cd, sp) = (bytes(0x3c), bytes(0x40), shorts(0x48), shorts(0x4c));
                            (0..ty.len().min(4)).map(|k| format!("[{k}] type {} lv {} sp {} cd {}", ty[k] as i8, lv.get(k).map_or(-1, |v| *v as i8), sp.get(k).copied().unwrap_or(-1), cd.get(k).copied().unwrap_or(-1))).collect::<Vec<_>>().join(" · ")
                        });
                    if let Some(line) = beast_cost {
                        eprintln!("[h2-beast-cost] f={f} {line}");
                    }
                    // ★ 2026-10-06 살상연희: the run (targets · hits on each · made / total · cap · the current hop's index · a.s
                    //   and whether the run set it) and the player's frame and tile.
                    if let (Some(b), Some(rp)) = (rogue_block, guest_build::GuestBuild::Hero2Ktf.rogue_patch()) {
                        type RP = guest_build::RoguePatch;
                        let w = |off: u32| rd_u(b + off, 4).unwrap_or(0) as i32;
                        // ★ 2026-10-08 (5): one target at a time (CUR_T), the anchor the next search goes from, the landings
                        //   by it as the last hop enumerated them (`score + 64 << 16 | y << 8 | x`), the blow's turn (ROT).
                        let n = w(RP::LAND_N).clamp(0, i32::from(RP::LANDS));
                        let lands: Vec<String> = (0..n as u32).map(|i| w(RP::LAND + 4 * i)).map(|v| format!("({},{})s{}", v & 0xff, (v >> 8) & 0xff, (v >> 16) - 64)).collect();
                        let me = guest_build::h2_static_handle(&rd, rp.char_slot).and_then(|ch| guest_build::h2_fields(&rd, ch));
                        let (gw, gt, tile, s) = me.map_or((-1, -1, (-1, -1), -1), |fb| {
                            let r = |off: u32| rd_u(fb + 4 + off, 4).unwrap_or(0) as i32;
                            (r(rp.frame_off), r(0x3c), (r(0x10), r(0x14)), r(rp.invuln_off))
                        });
                        // the afterimages: draws left on each (the draw wrapper takes one per paint) and the pose last drawn
                        let trail: Vec<i32> = (0..u32::from(RP::TRAILS)).map(|e| w(RP::TRAIL + 16 * e + 12)).collect();
                        eprintln!(
                            "[h2-slaughter] f={f} kind {} made {}/{} target {:#x} anchor ({},{}) rot {} lands {n} {} · plan {} · a.s {s} own {} · g.t {gt} g.w {gw} tile {tile:?} · trail {trail:?} pose {}/{}/{}",
                            w(RP::RUN_KIND),
                            w(RP::HOPS),
                            w(RP::TOTAL),
                            w(RP::CUR_T) as u32,
                            w(RP::ANCHOR_X),
                            w(RP::ANCHOR_Y),
                            w(RP::ROT),
                            lands.join(" "),
                            w(RP::PLAN_N),
                            w(RP::OWN_S),
                            w(RP::LAST_SEQ),
                            w(RP::LAST_FRAME),
                            w(RP::LAST_DIR)
                        );
                    }
                }
                // The live WINDOW CHAIN, named. The touch router picks its geometry from
                // the top window's class (and, on the slot screen, from that window's own
                // mode byte), so this is the only way to see headlessly which arm of
                // `h2_tap` a screen will take — the class pointers are heap addresses and
                // say nothing on their own. 1e's class-pick box (`be` mode 11) is the one
                // arm that had never been seen on a real screen.
                if let Some(chain) = guest_build::h2_window_chain(&rd, &h2) {
                    let cls = &h2.classes;
                    let name = |c: u32| -> String {
                        for (n, a) in [
                            ("title", cls.title_window),
                            ("slot", cls.slot_window),
                            ("option", cls.option_window),
                            ("help", cls.help_window),
                            ("menu", cls.field_menu),
                            ("popup", cls.popup),
                            ("message", cls.message_window),
                            ("status", cls.status_window),
                            ("inventory", cls.inventory_window),
                            ("equip", cls.equip_window),
                            ("skill", cls.skill_window),
                            ("beast", cls.beast_window),
                            ("beastlist", cls.beast_list_window),
                            ("quest", cls.quest_window),
                            ("card", cls.card_window),
                            ("detail", cls.detail_window),
                            ("list", cls.list_windows[0]),
                        ] {
                            if c == a {
                                return n.to_string();
                            }
                        }
                        format!("{c:#x}")
                    };
                    let mut parts: Vec<String> = vec![];
                    for w in &chain {
                        let mut s = name(w.class);
                        if w.class == cls.slot_window {
                            let m = rd(w.fields + 4 + guest_build::Hero2Java::SLOT_MODE_OFF, 1).first().copied().unwrap_or(0);
                            let dy = if m == guest_build::Hero2Java::SLOT_MODE_CLASS_PICK { guest_build::Hero2Java::SLOT_CLASS_BOX_DY } else { guest_build::Hero2Java::SLOT_BOX_DY };
                            let top_y = rd_u(h2.screen.3, 4).unwrap_or(0).saturating_sub(dy);
                            s.push_str(&format!("(mode={m} box_top={top_y})"));
                        }
                        parts.push(s);
                    }
                    eprintln!("[h2-win] f={f} {}", parts.join(" > "));
                }
                if let Some(p) = guest_build::h2_popup_walk(&rd, &h2) {
                    eprintln!(
                        "[popup] f={f} handle={:#x} beast={} parent_cursor={} sel={} rows={} len={} height={} shape={:?} value={:?} box={:?}",
                        p.handle, p.beast, p.parent_cursor, p.sel, p.rows, p.labels_len, p.height, p.shape(), p.value(),
                        p.shape().and_then(|s| guest_build::h2_popup_box(&rd, &h2, if s.native_shape { s.native_rows } else { s.our_rows }))
                    );
                }
            }
        }
        if trace_state && f % 20 == 0 {
            let rd = |a: u32, n: usize| emu.read_memory(a, n);
            let u32at = |a: u32| -> u32 {
                let b = rd(a, 4);
                if b.len() == 4 { u32::from_le_bytes([b[0], b[1], b[2], b[3]]) } else { 0xffff_ffff }
            };
            let cp = char_base(build(), &rd);
            // state byte moved between builds (KTF +0x7c / LGT +0x78); the window id
            // did not. `window` is the one that says WHICH of the game's own windows
            // owns the screen (3 = none, 4 = script, 8 = map), so it is what tells us
            // whether a key press opened the map at all.
            let st = u32at(cp + build().state_off());
            let sel = rd(cp + 0x9c6c, 1).first().copied().unwrap_or(0xff);
            let win = rd(cp + 0xa0c0, 1).first().copied().unwrap_or(0xff);
            eprintln!("[state] f={f} state={st} selflag={sel} window={win}");
        }
        if let Some(dir) = &char_dump {
            if dumps.contains(&f) {
                let rd = |a: u32, n: usize| emu.read_memory(a, n);
                let cp = char_base(build(), &rd);
                let blob = rd(cp, 0xb000);
                let _ = fs::create_dir_all(dir);
                let path = dir.join(format!("char_{f:05}.bin"));
                fs::write(&path, &blob).unwrap();
                // The static/global data area too: a current-map id is far more likely
                // to be a global than a CHAR field. (KTF keeps its RWPI statics around
                // sl = 0x1b2c40; LGT has absolute `.data` + `.bss`.)
                for (gbase, glen) in statics_ranges(build()).iter().copied() {
                    let g = rd(gbase, glen);
                    fs::write(dir.join(format!("glob_{f:05}_{gbase:#010x}.bin")), &g).unwrap();
                }
                eprintln!("[probe] CHAR dump {path:?} base={cp:#x} len={}", blob.len());
            }
        }
        if verify_offsets && dumps.contains(&f) {
            // MIGRATION_LGT.md §2.11 in one line. Offsets MIRROR items.rs (that file is
            // not include!d here — only guest_anchors is), so if items.rs ever moves one
            // this must follow; the reference block exists precisely so a wrong mirror
            // shows up as garbage against the on-screen status numbers.
            let rd = |a: u32, n: usize| emu.read_memory(a, n);
            let cp = char_base(build(), &rd);
            let u8at = |a: u32| rd(a, 1).first().copied();
            let i8at = |a: u32| u8at(a).map(|b| b as i8);
            let u16at = |a: u32| { let b = rd(a, 2); (b.len() == 2).then(|| u16::from_le_bytes([b[0], b[1]])) };
            let u32at = |a: u32| { let b = rd(a, 4); (b.len() == 4).then(|| u32::from_le_bytes([b[0], b[1], b[2], b[3]])) };
            // VERIFIED reference (§2.7): if these don't match the status screen the base
            // is wrong and nothing below means anything.
            eprintln!(
                "[verify] f={f} CHAR={cp:#x} | REF lv={:?} exp={:?}/{:?} hp={:?}/{:?} sp={:?} win={:?} state={:?} sel={:?}",
                u16at(cp + 0xe2), u32at(cp + 0xe4), u32at(cp + 0xe8),
                u16at(cp + 0xf2), u32at(cp + 0xec), u32at(cp + 0xf8),
                u8at(cp + 0xa0c0), u32at(cp + build().state_off()), u8at(cp + 0x9c6c),
            );
            // 🔶 #6 badges (0x18a u16 read as i16 / 0x18c u8), #18 skill-cast (0x9bb5,0x2c9)
            eprintln!(
                "[verify]   #6 badge stat={:?} skill={:?} | #18 cast mode={:?} cat={:?}",
                u16at(cp + 0x18a).map(|v| v as i16), u8at(cp + 0x18c),
                u8at(cp + 0x9bb5), u8at(cp + 0x2c9),
            );
            // 🔶 #5 quickbar: type/itemref/skillref/skillidx per slot (0x252/0x257/0x25c/0x26c)
            for s in 0..5u32 {
                eprintln!(
                    "[verify]   #5 qb[{s}] type={:?} itemref={:?} skillref={:?} cell={:?}",
                    u8at(cp + 0x252 + s), u8at(cp + 0x257 + s), u8at(cp + 0x25c + s), i8at(cp + 0x26c + s),
                );
            }
            // 🔶 #5b on-field skill sheet ids (0xd1+i) — the byte `items::skill_icon`
            // gates the atlas on (valid 4..=10, atlas index = value-4). §2.18-(2)
            // suspected the offset itself on LGT, but static RE closed that: the LGT
            // sheet loader (fn 0x4957c, template "res/skill/s4_bm" at 0x5e2e0) reads
            // (CHAR+0x28)+0x8c+0x1d+i = CHAR+0xd1+i — the SAME offset as KTF. What is
            // left to verify live is the VALUE (populated on party load, 0 before).
            eprintln!(
                "[verify]   #5b sheet[0]={:?} sheet[1]={:?} (want 4..=10; atlas=v-4)",
                u8at(cp + 0xd1), u8at(cp + 0xd2),
            );
            // 🔶 #7 secondary weapon (0x524 present / 0x513 tab / 0x514 slot, all i8)
            eprintln!(
                "[verify]   #7 equip present={:?} tab={:?} slot={:?}",
                i8at(cp + 0x524), i8at(cp + 0x513), i8at(cp + 0x514),
            );
            // 🔶 #8 save byte: area = deref(CHAR+0x9c90) [+0x11d]. Build-aware hop.
            let area = u32at(cp + 0x9c90).and_then(|first| match build().container_data_off() {
                None => Some(first),
                Some(off) => u32at(first).map(|blk| blk + off),
            });
            eprintln!("[verify]   #8 area={area:#x?} can_save={:?} (0=refuse)", area.and_then(|a| i8at(a + 0x11d)));
        }
        if h2_font_check == Some(f) && let Some(h2) = build().hero2_java() {
            let px = last.lock().unwrap().clone();
            let rd = |a: u32, n: usize| emu.read_memory(a, n);
            let font = guest_build::h2_font_from_guest(&rd);
            let (cx, cy) = (rd(h2.screen.2, 4), rd(h2.screen.3, 4));
            let cx = i32::from_le_bytes([cx[0], cx[1], cx[2], cx[3]]);
            let cy = i32::from_le_bytes([cy[0], cy[1], cy[2], cy[3]]);
            let text = |idx: u32| guest_build::h2_text_row(&rd, guest_build::Hero2Java::COM_TABLE_SLOT, idx).and_then(|h| guest_build::h2_byte_array(&rd, h));
            match font {
                None => eprintln!("[h2-font] f={f} font not loaded from the guest"),
                Some(font) => {
                    let mut report = |what: String, s: &[u8], x: i32, y: i32, align: i32, colour: u32| {
                        let mut mine = std::collections::HashSet::new();
                        font.draw_aligned(s, x, y, align, |px_, py_| {
                            mine.insert((px_, py_));
                        });
                        let (x0, x1) = (x - 60, x + 70);
                        let (mut ok, mut extra, mut missing) = (0, 0, 0);
                        for yy in y - 2..y + 16 {
                            for xx in x0..x1 {
                                if xx < 0 || yy < 0 || xx >= w as i32 || yy >= h as i32 {
                                    continue;
                                }
                                let game = px[(yy as u32 * w + xx as u32) as usize] & 0x00ff_ffff == colour;
                                match (game, mine.contains(&(xx, yy))) {
                                    (true, true) => ok += 1,
                                    (false, true) => extra += 1,
                                    (true, false) => missing += 1,
                                    _ => {}
                                }
                            }
                        }
                        eprintln!("[h2-font] f={f} {what}: {ok} px match, {extra} host-only, {missing} game-only (colour {colour:#08x})");
                    };
                    for i in 0..5u32 {
                        if let Some(s) = text(26 + i) {
                            report(format!("label {i}"), &s, cx - 45, cy - 36 + 18 * i as i32, 4, 0xd7ab92);
                        }
                    }
                    // Row 1's value (배경음, ON): grey unless focused.
                    if let Some(s) = text(24) {
                        report("row 1 value ON".to_string(), &s, cx + 24, cy - 36 + 18 - 2, 1, 0x7f7f7f);
                    }
                    if let Some(s) = text(24) {
                        report("row 2 value ON".to_string(), &s, cx + 24, cy - 36 + 36 - 2, 1, 0x7f7f7f);
                    }
                }
            }
        }
        if h2_cover.contains(&f) && let Some(h2) = build().hero2_java() {
            // ★ 2026-09-29: the page strip's cover — where the app says the game's box over a
            //   paged bag window is (`h2_modal_over`) — against the pixels the game painted. A
            //   true rect has a one-colour border ring (the box's outline) and a ring one pixel
            //   outside it that is not that colour: a pixel off either way breaks one of the two.
            let px = last.lock().unwrap().clone();
            let rd = |a: u32, n: usize| emu.read_memory(a, n);
            let bags: Vec<u32> = [guest_build::h2_static_handle(&rd, h2.inventory_slot), account_bag].into_iter().flatten().collect();
            match guest_build::h2_bag_window(&rd, &h2, &bags) {
                None => eprintln!("[h2-cover] f={f} no paged bag window in the chain"),
                Some(v) => {
                    let c = h2.classes;
                    let what = match guest_build::h2_window_chain(&rd, &h2).and_then(|ch| ch.last().copied()) {
                        None => "nothing".to_string(),
                        Some(t) if t.class == c.popup => format!("popup kind {}", rd(t.fields + 4 + guest_build::Hero2Java::POPUP_KIND_OFF, 1).first().copied().unwrap_or(0)),
                        Some(t) if t.class == c.quantity_popup => "개수 box aq".to_string(),
                        Some(t) if t.class == c.message_window => "message box ba".to_string(),
                        Some(t) if t.class == c.inventory_window => "bag window q".to_string(),
                        Some(t) => format!("class {:#x}", t.class),
                    };
                    // The inside rail (#14): five tabs down the panel's left margin.
                    let pitch = guest_build::Hero2Java::INV_CELL_PITCH * 4 / 5;
                    let rail = (v.panel.0 + guest_build::Hero2Java::BAG_RAIL_X, v.panel.1 + guest_build::Hero2Java::INV_CELL_Y0, guest_build::Hero2Java::BAG_RAIL_W, 4 * pitch + pitch - 1);
                    match guest_build::h2_modal_over(&rd, &h2, &v) {
                        None => eprintln!("[h2-cover] f={f} top {what}, on_top={} — no cover ({})", v.on_top, if v.on_top { "strip drawn whole" } else { "strip hidden" }),
                        Some((x, y, cw, ch)) => {
                            let at = |xx: i64, yy: i64| -> Option<u32> {
                                (xx >= 0 && yy >= 0 && (xx as u32) < w && (yy as u32) < h).then(|| px[(yy as u32 * w + xx as u32) as usize] & 0x00ff_ffff)
                            };
                            let ring = |x0: i64, y0: i64, x1: i64, y1: i64| -> Vec<u32> {
                                let mut out = vec![];
                                for xx in x0..=x1 {
                                    out.extend(at(xx, y0));
                                    out.extend(at(xx, y1));
                                }
                                for yy in y0 + 1..y1 {
                                    out.extend(at(x0, yy));
                                    out.extend(at(x1, yy));
                                }
                                out
                            };
                            let (xi, yi, wi, hi) = (i64::from(x), i64::from(y), i64::from(cw), i64::from(ch));
                            let border = ring(xi, yi, xi + wi - 1, yi + hi - 1);
                            let outside = ring(xi - 1, yi - 1, xi + wi, yi + hi);
                            let mut counts: std::collections::HashMap<u32, usize> = std::collections::HashMap::new();
                            for col in &border {
                                *counts.entry(*col).or_insert(0) += 1;
                            }
                            let (colour, n) = counts.into_iter().max_by_key(|(_, n)| *n).unwrap_or((0, 0));
                            let same_out = outside.iter().filter(|col| **col == colour).count();
                            let meets = rail.0 < x + cw && x < rail.0 + rail.2 && rail.1 < y + ch && y < rail.1 + rail.3;
                            eprintln!(
                                "[h2-cover] f={f} top {what}: cover ({x},{y}) {cw}x{ch} — border {n}/{} px #{colour:06x}, the ring outside {same_out}/{} px that colour; rail ({},{}) {}x{} {}",
                                border.len(),
                                outside.len(),
                                rail.0,
                                rail.1,
                                rail.2,
                                rail.3,
                                if meets { "MEETS it — drawn under the box" } else { "clear of it" }
                            );
                        }
                    }
                }
            }
        }
        if h2_confirm_check == Some(f) && let Some(h2) = build().hero2_java() {
            // The game's own 예/아니오 box on top of the live chain, drawn again by the host.
            let px = last.lock().unwrap().clone();
            let rd = |a: u32, n: usize| emu.read_memory(a, n);
            let word = |a: u32| {
                let b = rd(a, 4);
                (b.len() == 4).then(|| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
            };
            let top = guest_build::h2_window_chain(&rd, &h2).and_then(|c| c.last().copied()).filter(|t| t.class == h2.classes.popup);
            let font = guest_build::h2_font_from_guest(&rd);
            match (top, font) {
                (Some(t), Some(font)) => {
                    let kind = rd(t.fields + 4 + guest_build::Hero2Java::POPUP_KIND_OFF, 1).first().copied().unwrap_or(0);
                    let cursor = rd(t.fields + 4 + guest_build::Hero2Java::WIN_CURSOR_OFF, 1).first().map_or(-1, |b| *b as i8);
                    let lines: Vec<Vec<u8>> = word(t.fields + 4 + guest_build::Hero2Java::POPUP_LABELS_OFF)
                        .and_then(|arr| guest_build::h2_array(&rd, arr))
                        .map(|(base, n)| (0..n).filter_map(|i| word(base + i * 4).and_then(|h| guest_build::h2_byte_array(&rd, h))).collect())
                        .unwrap_or_default();
                    let (sw, sh, cx) = (word(h2.screen.0).unwrap_or(0) as i32, word(h2.screen.1).unwrap_or(0) as i32, word(h2.screen.2).unwrap_or(0) as i32);
                    let p = guest_build::h2_confirm_patch(&font, &lines, cursor == 0, sw, sh, cx);
                    let (mut same, mut differ) = (0u32, 0u32);
                    let mut first = vec![];
                    for yy in 0..p.h as i32 {
                        for xx in 0..p.w as i32 {
                            let (x, y) = (p.x + xx, p.y + yy);
                            if x < 0 || y < 0 || x >= w as i32 || y >= h as i32 {
                                continue;
                            }
                            let game = px[(y as u32 * w + x as u32) as usize] & 0x00ff_ffff;
                            let mine = p.argb[(yy as u32 * p.w + xx as u32) as usize] & 0x00ff_ffff;
                            if game == mine {
                                same += 1;
                            } else {
                                differ += 1;
                                if first.len() < 8 {
                                    first.push(format!("({x},{y}) game {game:06x} host {mine:06x}"));
                                }
                            }
                        }
                    }
                    eprintln!(
                        "[h2-confirm] f={f} kind {kind} · {} line(s) · cursor {cursor} · box ({},{}) {}x{} — {same} px match, {differ} differ {}",
                        lines.len(),
                        p.x,
                        p.y,
                        p.w,
                        p.h,
                        first.join(" ")
                    );
                }
                (None, _) => eprintln!("[h2-confirm] f={f} no popup on top of the chain"),
                (_, None) => eprintln!("[h2-confirm] f={f} font not loaded from the guest"),
            }
        }
        if dumps.contains(&f) {
            // force a paint of the current guest framebuffer — ⛔ not while the 부활의 구 box
            // holds the guest (the extra tick would run the fall on under it)
            if revive_prompt.is_none() && !dump_passive {
                emu.handle_event(Event::Redraw);
                let _ = emu.tick();
            }
            let mut px = last.lock().unwrap().clone();
            if let (Some((c, _orb, lines, (sw, sh, cx))), Some(font)) = (revive_prompt.as_ref(), revive_font.as_ref()) {
                let p = guest_build::h2_confirm_patch(font, lines, c.yes, *sw, *sh, *cx);
                for yy in 0..p.h as i32 {
                    for xx in 0..p.w as i32 {
                        let (x, y) = (p.x + xx, p.y + yy);
                        if x >= 0 && y >= 0 && (x as u32) < w && (y as u32) < h {
                            px[(y as u32 * w + x as u32) as usize] = p.argb[(yy as u32 * p.w + xx as u32) as usize];
                        }
                    }
                }
            }
            if let (Some((pose, noweapon)), Some(h2)) = (h2_equip_preview, build().hero2_java()) {
                let rd = |a: u32, n: usize| emu.read_memory(a, n);
                let skip: &[u8] = if noweapon { &[4, 5, 6, 7] } else { &[4, 5] };
                match guest_build::h2_equip_preview(&rd, &h2, pose, skip) {
                    Some(p) => {
                        let opaque = p.argb.iter().filter(|c| *c >> 24 != 0).count();
                        eprintln!("[h2-equip-preview] f={f} pose {pose:?} at ({}, {}) {}x{} — {opaque} px", p.x, p.y, p.w, p.h);
                        for yy in 0..p.h as i32 {
                            for xx in 0..p.w as i32 {
                                let c = p.argb[(yy as u32 * p.w + xx as u32) as usize];
                                let (x, y) = (p.x + xx, p.y + yy);
                                if c >> 24 != 0 && x >= 0 && y >= 0 && (x as u32) < w && (y as u32) < h {
                                    px[(y as u32 * w + x as u32) as usize] = c;
                                }
                            }
                        }
                    }
                    None => eprintln!("[h2-equip-preview] f={f} no EQUIPMENT window on top (or the pose is unreadable)"),
                }
            }
            if let (Some(_), Some(h2)) = (equip_ui_block, build().hero2_java()) {
                let rd = |a: u32, n: usize| emu.read_memory(a, n);
                match (guest_build::h2_equip_view(&rd, &h2).filter(|v| v.shifted && v.over != Some(None)), guest_build::h2_font_from_guest(&rd)) {
                    (Some(v), Some(font)) => {
                        let icons = guest_build::h2_item_icons(&rd);
                        let (seq, frame, dir) = guest_build::H2_EQUIP_POSE;
                        let pose = guest_build::h2_hero_pose(&rd, seq, frame, dir, guest_build::H2_EQUIP_POSE_SKIP).unwrap_or_default();
                        let wardrobe = probe_wardrobe(costume_book.as_ref());
                        let class = guest_build::h2_char_class(&rd, &h2);
                        let wearable = |a: u16, slot: u8| guest_build::h2_av_wearable(&avatar_sheets, a, slot, class);
                        let scene = guest_build::H2EquipScene {
                            font: &font,
                            panel: v.panel,
                            cover: v.over.flatten(),
                            model: &equip_model,
                            catalog: &costume_catalog,
                            wardrobe: &wardrobe,
                            wearable: &wearable,
                            class,
                            pose: &pose,
                            icons: icons.as_ref().map(|i| &i.1),
                        };
                        let p = guest_build::h2_equip_avatar_patch(&scene);
                        let mut n = 0usize;
                        for yy in 0..p.h as i32 {
                            for xx in 0..p.w as i32 {
                                let c = p.argb[(yy as u32 * p.w + xx as u32) as usize];
                                let (x, y) = (p.x + xx, p.y + yy);
                                if c >> 24 != 0 && x >= 0 && y >= 0 && (x as u32) < w && (y as u32) < h {
                                    px[(y as u32 * w + x as u32) as usize] = c;
                                    n += 1;
                                }
                            }
                        }
                        eprintln!(
                            "[h2-equip-ui] f={f} drew the avatar side ({n} px; pose {} sprite(s), icons {}; focus {:?} list {:?}; {} item(s), worn {:?})",
                            pose.len(),
                            icons.is_some(),
                            equip_model.focus,
                            equip_model.list,
                            wardrobe.owned.len(),
                            wardrobe.worn
                        );
                    }
                    (None, _) => {}
                    (Some(_), None) => eprintln!("[h2-equip-ui] f={f} font not loaded from the guest"),
                }
            }
            if h2_options && let Some(h2) = build().hero2_java() {
                let rd = |a: u32, n: usize| emu.read_memory(a, n);
                if let (Some((win, chain, cx, cy)), Some(font)) = (guest_build::h2_options_window(&rd, &h2), guest_build::h2_font_from_guest(&rd))
                    && let Some(ground) = guest_build::h2_options_ground(&rd, &h2, &chain)
                {
                    let cur = rd(win.fields + 4 + guest_build::Hero2Java::WIN_CURSOR_OFF, 1).first().map_or(0, |b| *b as i8);
                    opt_state.sync(&win, cur);
                    let exp = guest_build::H2_OPT_EXP_VALUES;
                    let ctl = guest_build::H2_OPT_CONTROL_VALUES;
                    let onoff: &[u8] = if opt_host.0 { b"ON" } else { b"OFF" };
                    let diff = guest_build::H2_OPT_DIFFICULTY_VALUES[usize::from(diff_level.unwrap_or(0))];
                    if let Some(rows) = guest_build::h2_options_rows(&rd, [guest_build::H2_OPT_DROP_RATE_VALUES[opt_drop], diff, guest_build::H2_OPT_RESPAWN_VALUES[opt_respawn], onoff, exp[opt_host.1], ctl[opt_host.2]]) {
                        for p in guest_build::h2_options_patch(&font, &rows, &opt_state, cx, cy, ground) {
                            for yy in 0..p.h as i32 {
                                for xx in 0..p.w as i32 {
                                    let c = p.argb[(yy as u32 * p.w + xx as u32) as usize];
                                    let (X, Y) = (p.x + xx, p.y + yy);
                                    if c >> 24 != 0 && X >= 0 && Y >= 0 && (X as u32) < w && (Y as u32) < h {
                                        px[(Y as u32 * w + X as u32) as usize] = c;
                                    }
                                }
                            }
                        }
                    }
                }
            }
            // ★★ 2026-10-16 --h2-tools: the 정렬 / 일괄판매 buttons the app lays over the frame (`draw_guest_patches`).
            if h2_tools {
                for p in &tools_patches {
                    for yy in 0..p.h as i32 {
                        for xx in 0..p.w as i32 {
                            let c = p.argb[(yy as u32 * p.w + xx as u32) as usize];
                            let (X, Y) = (p.x + xx, p.y + yy);
                            if c >> 24 != 0 && X >= 0 && Y >= 0 && (X as u32) < w && (Y as u32) < h {
                                px[(Y as u32 * w + X as u32) as usize] = c;
                            }
                        }
                    }
                }
            }
            let path = out.join(format!("frame_{f:05}_{w}x{h}.rgba"));
            let mut raw = Vec::with_capacity(px.len() * 4);
            for p in &px {
                raw.extend_from_slice(&[(p >> 16) as u8, (p >> 8) as u8, *p as u8, (p >> 24) as u8]);
            }
            fs::write(&path, &raw).unwrap();
            let opaque = px.iter().filter(|p| (*p >> 24) != 0).count();
            let nonblack = px.iter().filter(|p| (*p & 0x00ff_ffff) != 0).count();
            eprintln!("[probe] frame {f}: wrote {path:?} ({} px, opaque {opaque}, non-black {nonblack})", px.len());
            if h2_pose_audit {
                let rd = |a: u32, n: usize| emu.read_memory(a, n);
                match h2_pose_audit_run(&rd) {
                    Some((poses, sprites, missing)) => {
                        // ★ 2026-10-13 「악마화」 D2: the demon copies (sequences from DemonPatch::SHIPPED_SEQS[class] on) name body
                        // frames only the demon sheet hero/1/300 has — holes there are EXPECTED while the human body sheet is in
                        // (the block maps nothing to them then) and a defect once 악의 has the look (LOOK 1).
                        let demon_from = GuestBuild::Hero2Ktf.hero2_java().and_then(|h2| guest_build::h2_char_class(&rd, &h2)).map(|c| u32::from(guest_build::DemonPatch::SHIPPED_SEQS[usize::from(c)]));
                        let (demon, human): (Vec<_>, Vec<_>) = missing.iter().partition(|m| demon_from.is_some_and(|d| m.0 >= d));
                        eprintln!(
                            "[h2-pose-audit] f={f} {poses} poses (all sequences x 4 directions), {sprites} head/body/arm/leg sprites, {} without an image ({} in the demon sequences — expected while the human body sheet is in)",
                            human.len(),
                            demon.len()
                        );
                        for (seq, frame, dir, part, img, flip, why) in human.iter().chain(demon.iter().take(2)).take(16) {
                            eprintln!("[h2-pose-audit]   seq {seq} frame {frame} {} part {part} frame {}{img} — {why}", ["back", "front", "left", "right"][*dir as usize], if *flip { "~" } else { "" });
                        }
                    }
                    None => eprintln!("[h2-pose-audit] f={f} the class animation / parts table is not readable yet"),
                }
            }
            // `--h2-minimap <side>` (2026-09-29): the app's hero5-style panel over this frame
            // at phone size — the painter is the app's own `minimap_view.rs`, fed the same
            // cells and marker the worker sends.
            if (h2_minimap > 0 || h2_status)
                && let (Some(sv), Some(h2)) = (GuestBuild::Hero2Ktf.hero2_save(), GuestBuild::Hero2Ktf.hero2_java())
            {
                let rd = |a: u32, n: usize| emu.read_memory(a, n);
                let k = (1080 / h.max(1)).clamp(1, 8);
                let (ww, wh) = (w * k, h * k);
                let mut win = vec![0u32; (ww * wh) as usize];
                for y in 0..wh {
                    for x in 0..ww {
                        win[(y * ww + x) as usize] = 0xff00_0000 | px[((y / k) * w + x / k) as usize];
                    }
                }
                if h2_minimap > 0 {
                    // The built-in corner: `overlay::minimap_panel_builtin` with its 16 px pad.
                    let side = h2_minimap;
                    let (px0, py0) = (ww.saturating_sub(32 + side), 32);
                    let map = guest_build::h2_map(&rd, &sv.area, h2.char_slot);
                    let marker = guest_build::h2_player_marker(&rd, &sv.area, h2.char_slot);
                    let mobs = guest_build::h2_monster_marks(&rd, sv.area.stage_slot, guest_build::h2_sees_ghosts(&rd, &GuestBuild::Hero2Ktf.hero2_java().unwrap()));
                    let drawn = map.as_ref().is_some_and(|m| {
                        let cells = guest_build::h2_minimap_cells_live(&rd, &sv.area, m, &mut h2_trigger_cache);
                        minimap_view::mm_draw_panel(&mut win, ww, wh, (px0, py0, side, side), m.w, m.h, &cells, marker, &mobs)
                    });
                    eprintln!(
                        "[h2-minimap] f={f} map={:?} marker={marker:?} tile_px={} drawn={drawn} mobs={} {:?}",
                        map.as_ref().map(|m| (m.w, m.h)),
                        minimap_view::mm_tile_px(side),
                        mobs.len(),
                        mobs.iter().take(8).collect::<Vec<_>>(),
                    );
                }
                if h2_status {
                    // The live gauges the worker's `items::read_stats` reads, through the same
                    // `char_layout()` table, and the helmet's head sprite.
                    let word = |a: u32| {
                        let b = rd(a, 4);
                        (b.len() == 4).then(|| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
                    };
                    let c = guest_build::h2_static_handle(&rd, h2.char_slot).and_then(|o| guest_build::h2_fields(&rd, o)).map(|b| b + 4);
                    let l = GuestBuild::Hero2Ktf.char_layout();
                    let gauges = c.zip(l).and_then(|(c, l)| {
                        Some((
                            hud_view::HvStats {
                                hp: word(c + l.hp)?,
                                hp_max: word(c + l.hp_max)?,
                                sp: word(c + l.sp)?,
                                sp_max: l.sp_max.map_or(Some(u32::from(l.sp_max_const)), |o| word(c + o))?,
                                level: word(c + l.level)? & 0xffff,
                            },
                            word(c + l.exp)?,
                            word(c + l.exp_next)?,
                        ))
                    });
                    let head = guest_build::h2_head_sprite(&rd, &h2);
                    // ★ A3b: as the app — the head the game loaded (a costume's too), the atlas head
                    // for the helmet's sprite only when that cannot be read.
                    let live = guest_build::h2_class_blob(&rd).and_then(|(_, b)| guest_build::h2_live_head(&rd, &b));
                    let icon = match &live {
                        Some(p) => Some(hud_view::HvIcon { w: p.w, h: p.h, px: &p.argb }),
                        None => head.and_then(|n| atlas::atlas().head(n)).map(|ic| hud_view::HvIcon { w: u32::from(ic.w), h: u32::from(ic.h), px: &ic.px }),
                    };
                    let g = hud_view::HvGeom::new((0, 0, ww, wh), w, h);
                    let e = hud_view::HvExp::new((0, 0, ww, wh), w, h);
                    // ★ 2026-10-05: the skill buffs right of the bars, as the app draws them.
                    let buffs = guest_build::h2_buff_hud(&rd, h2.char_slot, h2.class_slot);
                    // ★ 2026-10-07 (2): the rage gauge under 격분's cell, as the app draws it
                    let fury_gauge = berserker_block.and_then(|b| guest_build::h2_berserker_fury(&rd, b)).filter(|w| w.state != 0).map(|w| w.gauge.clamp(0, 100) as u32);
                    // ★★ 2026-10-22 가디언 레벨: as the app's `GuardianLevel::overlay` — an armed LV 99 character's bar is the
                    //   GL bar (the block's NEXT) and its GL sits above it.
                    let gl = gl_block.and_then(|b| guest_build::h2_guardian_level_read(&rd, b)).filter(|w| {
                        w.armed && w.gl >= 1 && w.next > 0 && guest_build::h2_guardian_level_char(&rd, h2.char_slot).is_some_and(|(o, lv)| o == w.owner && lv >= 99)
                    });
                    if let Some((s, exp, next)) = gauges {
                        let next = gl.map_or(next, |w| w.next);
                        hud_view::hv_draw_status(&mut win, ww, wh, &g, &s, icon);
                        hud_view::hv_draw_exp(&mut win, ww, wh, &e, exp, next);
                        if let Some(w) = gl {
                            hud_view::hv_draw_gl(&mut win, ww, wh, &e, w.gl);
                        }
                        let cells: Vec<hud_view::HvBuff> = buffs
                            .iter()
                            .map(|b| hud_view::HvBuff {
                                icon: if b.is_beast() { atlas::atlas().beast(b.cell) } else { atlas::atlas().skill(b.sheet, b.cell) }.map(|ic| hud_view::HvIcon { w: u32::from(ic.w), h: u32::from(ic.h), px: &ic.px }),
                                secs: u32::from(b.secs),
                                gauge: (b.id == guest_build::BerserkerPatch::FURY_ID && b.sheet == guest_build::BerserkerPatch::CLASS).then_some(fury_gauge).flatten(),
                            })
                            .collect();
                        hud_view::hv_draw_buffs(&mut win, ww, wh, &g, &cells);
                    }
                    eprintln!("[h2-status] f={f} buffs={buffs:?}");
                    eprintln!(
                        "[h2-status] f={f} gauges={gauges:?} head={head:?} portrait={} unit={} block={:?} exp={:?}",
                        live.as_ref().map_or("atlas".to_string(), |p| format!("live {}x{}", p.w, p.h)),
                        g.u,
                        g.bbox(),
                        e.rect()
                    );
                }
                let path = out.join(format!("{}_{f:05}_{ww}x{wh}.rgba", if h2_status { "hud" } else { "minimap" }));
                let mut raw = Vec::with_capacity(win.len() * 4);
                for p in &win {
                    raw.extend_from_slice(&[(p >> 16) as u8, (p >> 8) as u8, *p as u8, 0xff]);
                }
                fs::write(&path, &raw).unwrap();
                eprintln!("[h2-minimap] f={f} -> {path:?}");
            }
            // ★★ 2026-10-15 「악마화」 HD: the frame at k× with the recorded effects composited as the app does it
            if h2_hd > 0 {
                let k = h2_hd;
                let (ww, wh) = (w * k, h * k);
                let mut win = vec![0u32; (ww * wh) as usize];
                for y in 0..wh {
                    for x in 0..ww {
                        win[(y * ww + x) as usize] = 0xff00_0000 | px[((y / k) * w + x / k) as usize];
                    }
                }
                let hd = hd_last.lock().unwrap().clone();
                let (draws, groups) = hd.as_ref().map_or((0, 0), |h| (h.draws.len(), h.draws.iter().map(|d| u32::from(d.group) + 1).max().unwrap_or(0)));
                if let (Some(hd), Some(lib)) = (hd.as_ref(), wie_backend::canvas::fx_library()) {
                    let view = wie_backend::canvas::HdView { rect: (0, 0, ww, wh), src_w: w, src_y0: 0, src_h: h };
                    wie_backend::canvas::compose_hd(&lib, hd, &mut win, ww, wh, &view);
                }
                let path = out.join(format!("hd_{f:05}_{ww}x{wh}.rgba"));
                let mut raw = Vec::with_capacity(win.len() * 4);
                for p in &win {
                    raw.extend_from_slice(&[(p >> 16) as u8, (p >> 8) as u8, *p as u8, 0xff]);
                }
                fs::write(&path, &raw).unwrap();
                // ★ 2026-10-16: the record's LAYER MAP at 1× (`layer_<f>_<w>x<h>.rgba`, grey = group × 40, 255 = painted
                //   after the last recorded draw) — what the composite is clipped by, cell for cell
                if let Some(hd) = hd.as_ref() {
                    let lpath = out.join(format!("layer_{f:05}_{}x{}.rgba", hd.w, hd.h));
                    let mut lraw = Vec::with_capacity(hd.layer.len() * 4);
                    for &l in &hd.layer {
                        let g = if l == 255 { 255 } else { (u32::from(l) * 40).min(240) as u8 };
                        lraw.extend_from_slice(&[g, g, g, 0xff]);
                    }
                    fs::write(&lpath, &lraw).unwrap();
                }
                let effects: Vec<String> = hd.as_ref().map(|h| h.draws.iter().take(12).map(|d| format!("{}@{}({},{})g{}", d.effect, d.age, d.x, d.y, d.group)).collect()).unwrap_or_default();
                eprintln!("[h2-hd] f={f} draws={draws} groups={groups} -> {path:?} {}", effects.join(" "));
            }
        }
        if dumps.contains(&f) {
            for (base, len) in &dump_mem {
                let bytes = emu.read_memory(*base, *len);
                let path = out.join(format!("mem_{f:05}_{base:#010x}.bin"));
                fs::write(&path, &bytes).unwrap();
                let nz = bytes.iter().filter(|b| **b != 0).count();
                eprintln!("[probe] frame {f}: mem {base:#x} len {} -> {path:?} ({} read, {nz} non-zero)", len, bytes.len());
            }
        }
    }
    if profile {
        let leaf = prof_leaf.lock().unwrap();
        let total: u64 = leaf.values().sum();
        let mut v: Vec<(u32, u64)> = leaf.iter().map(|(a, c)| (*a, *c)).collect();
        v.sort_by_key(|(_, c)| core::cmp::Reverse(*c));
        eprintln!("[prof] {total} samples, {} distinct leaf PCs", v.len());
        for (pc, c) in v.iter().take(30) {
            // Guest address -> client.bin file offset (see CLAUDE.md: guest = fo + 0x100000).
            eprintln!("[prof] {:6.2}%  pc={pc:#010x}  fo={:#x}  ({c})", *c as f64 * 100.0 / total as f64, pc.wrapping_sub(0x100000));
        }
        let caller = prof_caller.lock().unwrap();
        let mut cv: Vec<((u32, u32), u64)> = caller.iter().map(|(k, c)| (*k, *c)).collect();
        cv.sort_by_key(|(_, c)| core::cmp::Reverse(*c));
        eprintln!("[prof] top pc<-caller pairs:");
        for ((pc, lr), c) in cv.iter().take(15) {
            eprintln!("[prof]   {:6.2}%  pc={pc:#010x} <- lr={lr:#010x}", *c as f64 * 100.0 / total as f64);
        }
    }
    if let Some(dir) = &fs_out {
        let live = FS_LIVE.get().expect("filesystem not built").lock().unwrap();
        let mut written = 0usize;
        for (key, data) in live.iter() {
            // key = `<aid>:<path>` with the leading slash already normalised away.
            let Some((aid, path)) = key.split_once(':') else { continue };
            let target = dir.join(aid).join("fs").join(path);
            if let Some(parent) = target.parent() {
                let _ = fs::create_dir_all(parent);
            }
            match fs::write(&target, data) {
                Ok(()) => {
                    written += 1;
                    eprintln!("[probe] fs-out {target:?} ({} B)", data.len());
                }
                Err(e) => eprintln!("[probe] fs-out {target:?}: {e}"),
            }
        }
        eprintln!("[probe] fs-out wrote {written} file(s) to {dir:?}");
    }
    if let Some((path, cap)) = AUDIO_WAV.get() {
        let mut cap = cap.lock().unwrap();
        cap.advance(clock.load(Ordering::SeqCst));
        // The effects over the music, as the app's bus sums them.
        let cap = &mut *cap;
        if cap.pcm.len() < cap.sfx.len() {
            cap.pcm.resize(cap.sfx.len(), 0.0);
        }
        for (o, s) in cap.pcm.iter_mut().zip(&cap.sfx) {
            *o += s;
        }
        let mut wav = Vec::with_capacity(44 + cap.pcm.len() * 2);
        let data = (cap.pcm.len() * 2) as u32;
        let rate = FmCapture::RATE as u32;
        for chunk in [&b"RIFF"[..], &(36 + data).to_le_bytes(), b"WAVEfmt ", &16u32.to_le_bytes(), &1u16.to_le_bytes(), &2u16.to_le_bytes()] {
            wav.extend_from_slice(chunk);
        }
        for chunk in [&rate.to_le_bytes()[..], &(rate * 4).to_le_bytes(), &4u16.to_le_bytes(), &16u16.to_le_bytes(), b"data", &data.to_le_bytes()] {
            wav.extend_from_slice(chunk);
        }
        let mut peak = 0.0f32;
        for s in &cap.pcm {
            peak = peak.max(s.abs());
            wav.extend_from_slice(&((s.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes());
        }
        match fs::write(path, &wav) {
            Ok(()) => eprintln!("[probe] audio wav {path:?}: {:.1} s, peak {peak:.3}", cap.pcm.len() as f64 / 2.0 / rate as f64),
            Err(e) => eprintln!("[probe] audio wav {path:?}: {e}"),
        }
    }
    eprintln!("[probe] done");
}
