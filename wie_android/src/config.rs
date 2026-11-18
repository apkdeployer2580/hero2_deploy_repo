//! Tiny persisted settings (currently just the orientation preference), stored
//! as a one-line file in the app-private data dir so it survives restarts.

use std::path::{Path, PathBuf};

fn config_path(data_dir: &Path) -> PathBuf {
    data_dir.join("wie_config.txt")
}

/// The adb-reachable escape-hatch directory for THIS app.
///
/// ★ 2026-09-04. All seven switches below used to spell out `com.hero3remake.app` —
/// the SIBLING project's package, inherited verbatim by the fork. On a phone with both
/// apps installed that names hero3's directory, so the escape hatch was pointed at
/// another app's settings. It never actually read them (§2⑤ of
/// `HERO2_NEXT_SESSION.md`: `AndroidManifest.xml` declares no storage permission, so
/// the read always fails and the `data_dir` fallback is what answers), which is why
/// nobody noticed for four weeks. One constant now, so it cannot drift again.
///
/// ⚠ Keep in step with `applicationId` in `android_app/app/build.gradle.kts`.
///
/// ★ `pub(crate)` since 2026-09-19: the save vault dumps its backups here too, and
/// that path has to be the SAME one — a second spelling is how the hero3 address
/// above survived four weeks.
pub(crate) const SDCARD_FILES: &str = "/sdcard/Android/data/com.hero2remake.app/files";

/// Returns the saved orientation preference.
///
/// ★ Default changed to LANDSCAPE 2026-07-31, when the orientation row was removed
/// from the settings modal (player request). Defaulting to portrait with no way to
/// change it would strand a fresh install: the layout editor is landscape-only, so
/// the one screen that could fix anything would be unreachable. Only an explicit
/// "portrait" in the file selects portrait now.
pub fn read_landscape(data_dir: &Path) -> bool {
    match std::fs::read_to_string(config_path(data_dir)) {
        Ok(s) => s.trim() != "portrait",
        Err(_) => true,
    }
}

pub fn write_landscape(data_dir: &Path, landscape: bool) {
    let value = if landscape { "landscape" } else { "portrait" };
    if let Err(e) = std::fs::write(config_path(data_dir), value) {
        log::warn!("failed to persist orientation pref: {e}");
    }
}

fn quickpage_path(data_dir: &Path) -> PathBuf {
    data_dir.join("wie_quickpages.txt")
}

/// Load the host-owned quickbar pages. A missing or unreadable file gives an empty
/// store on page 0, which is the guest's own bar — i.e. exactly the pre-feature
/// behaviour, so a lost file costs the player their page-2 arrangement and nothing else.
pub fn read_quickpages(data_dir: &Path) -> crate::quickpage::Pages {
    match std::fs::read_to_string(quickpage_path(data_dir)) {
        Ok(s) => crate::quickpage::Pages::parse(&s),
        Err(_) => crate::quickpage::Pages::new(),
    }
}

/// The hidden-plate mask alone (hero3 82차) — what `App::new` seeds `crate::quick_hidden`
/// from before the first layout is built. The same file the worker owns; only the
/// `hidden` line is of interest here, and `Pages::parse` already sanitises it.
pub fn read_quick_hidden(data_dir: &Path) -> u8 {
    read_quickpages(data_dir).hidden()
}

/// Did the last write of the plate store reach the disk? (★ 2026-10-28, code review P2: 세이브
/// 내보내기 reads this file beside the save it belongs to, and must know when it is not that one.)
static QUICKPAGES_WRITE_OK: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);

pub fn quickpages_write_ok() -> bool {
    QUICKPAGES_WRITE_OK.load(std::sync::atomic::Ordering::Relaxed)
}

/// Persist the pages. Called on every flip: the store is the only record of what is on
/// an inactive page, and the app can be killed at any moment from the background.
///
/// ★ 2026-10-28 (code review P2): temp + rename, and the outcome is RETURNED and recorded
/// ([`quickpages_write_ok`]). `fs::write` straight over the file left it torn when the disk ran
/// out half way, and nobody heard of it — 세이브내보내기 then sent the torn store as part of a save.
/// Now a failed write leaves the previous store whole, the worker writes again until one lands,
/// and the export refuses while the last one failed. Logged on a change of state, not per retry.
pub fn write_quickpages(data_dir: &Path, pages: &crate::quickpage::Pages) -> bool {
    let path = quickpage_path(data_dir);
    let tmp = data_dir.join(".wie_quickpages.txt.tmp");
    let written = std::fs::write(&tmp, pages.serialize()).and_then(|()| std::fs::rename(&tmp, &path));
    let ok = written.is_ok();
    let was = QUICKPAGES_WRITE_OK.swap(ok, std::sync::atomic::Ordering::Relaxed);
    match written {
        Err(e) => {
            let _ = std::fs::remove_file(&tmp);
            if was {
                log::warn!("failed to persist quickbar pages: {e} — the previous store stands; retrying");
            }
        }
        Ok(()) if !was => log::info!("quickbar pages persisted again"),
        Ok(()) => {}
    }
    ok
}

fn control_style_path(data_dir: &Path) -> PathBuf {
    data_dir.join("wie_control_style.txt")
}

/// Whether the left control is a floating JOYSTICK (default) vs the classic
/// D-pad. Persisted so the choice survives restarts.
pub fn read_joystick(data_dir: &Path) -> bool {
    match std::fs::read_to_string(control_style_path(data_dir)) {
        Ok(s) => s.trim() != "dpad", // default (missing/other) = joystick
        Err(_) => true,
    }
}

pub fn write_joystick(data_dir: &Path, joystick: bool) {
    let value = if joystick { "joystick" } else { "dpad" };
    if let Err(e) = std::fs::write(control_style_path(data_dir), value) {
        log::warn!("failed to persist control style: {e}");
    }
}

fn touch_type_path(data_dir: &Path) -> PathBuf {
    data_dir.join("wie_touch_type.txt")
}

/// TOUCH TYPE (player request 2026-08-16). `false` = **A**, the shipped floating
/// stick (touch anywhere, the base spawns under the finger); `true` = **B**, the
/// four-arrow D-pad pinned bottom-left with the stick hidden outright (second round
/// of the same day — B began as a pinned stick and the player asked for arrows
/// instead). See [`crate::stick_fixed`].
///
/// Its own file rather than a second value in `wie_control_style.txt`: that one
/// selects D-pad vs joystick for the NON-immersive layouts, which is a different
/// question, and folding two settings into one line would make a partial write lose
/// both.
pub fn read_touch_type(data_dir: &Path) -> u8 {
    match std::fs::read_to_string(touch_type_path(data_dir)) {
        // "fixed" is touch type B (the arrows) since 2026-08-16; "pinned" is fixed_A
        // (2026-09-24); missing / anything else = free, the floating stick.
        Ok(s) => match s.trim() {
            "fixed" => crate::TOUCH_FIXED_B,
            "pinned" => crate::TOUCH_FIXED_A,
            _ => crate::TOUCH_FREE,
        },
        Err(_) => crate::TOUCH_FREE,
    }
}

pub fn write_touch_type(data_dir: &Path, t: u8) {
    let value = match t {
        crate::TOUCH_FIXED_B => "fixed",
        crate::TOUCH_FIXED_A => "pinned",
        _ => "float",
    };
    if let Err(e) = std::fs::write(touch_type_path(data_dir), value) {
        log::warn!("failed to persist touch type: {e}");
    }
}

fn buffs_path(data_dir: &Path) -> PathBuf {
    data_dir.join("wie_buffs.txt")
}

/// Host-owned cash-shop buff expiries, one absolute UNIX-ms stamp per line:
/// `exp_x2 <ms>` / `exp_x4 <ms>` / `relief <ms>`.
///
/// ★ These are OURS, not the game's — the three items they come from have no
/// client-side effect at all (see [`crate::buffs`]), and the game's save file is
/// encrypted, so there is nowhere in it to put them. Losing this file loses the
/// remaining time; that is the accepted trade.
///
/// A damaged or partly written file degrades to "no buff" per line rather than
/// failing the read: an unparsable stamp must not hand out a free week, and it must
/// not stop the app either.
pub fn read_buffs(data_dir: &Path) -> crate::buffs::Buffs {
    let mut b = crate::buffs::Buffs::default();
    let Ok(text) = std::fs::read_to_string(buffs_path(data_dir)) else {
        return b;
    };
    for line in text.lines() {
        let mut it = line.split_whitespace();
        let (Some(key), Some(val)) = (it.next(), it.next()) else {
            continue;
        };
        let Ok(ms) = val.parse::<u64>() else {
            continue;
        };
        match key {
            "exp_x2" => b.exp_x2_until = ms,
            "exp_x4" => b.exp_x4_until = ms,
            "relief" => b.relief_until = ms,
            _ => {}
        }
    }
    b
}

pub fn write_buffs(data_dir: &Path, b: &crate::buffs::Buffs) {
    let text = format!(
        "exp_x2 {}\nexp_x4 {}\nrelief {}\n",
        b.exp_x2_until, b.exp_x4_until, b.relief_until
    );
    if let Err(e) = std::fs::write(buffs_path(data_dir), text) {
        log::warn!("failed to persist cash buffs: {e}");
    }
}

fn levelups_path(data_dir: &Path) -> PathBuf {
    data_dir.join("wie_levelups.txt")
}

/// Pending 천상의 영약 level-ups, a bare decimal count. Persisted for the same
/// reason the buffs are: the game consumes the item on OK, but the grant fires
/// only once the player is back on the field — an app death in between must not
/// eat a purchase. A damaged file degrades to 0 (no free level-ups), same policy
/// as [`read_buffs`].
pub fn read_pending_levelups(data_dir: &Path) -> u32 {
    std::fs::read_to_string(levelups_path(data_dir))
        .ok()
        .and_then(|t| t.trim().parse::<u32>().ok())
        .unwrap_or(0)
}

pub fn write_pending_levelups(data_dir: &Path, n: u32) {
    if let Err(e) = std::fs::write(levelups_path(data_dir), format!("{n}\n")) {
        log::warn!("failed to persist pending level-ups: {e}");
    }
}

fn cash_baseline_path(data_dir: &Path) -> PathBuf {
    data_dir.join("wie_cash_uncommitted.txt")
}

/// Note down the host state that the last completed save already accounts for, so
/// an app death before the NEXT one can be undone. See [`crate::buffs::CashBaseline`]
/// for why this exists at all.
///
/// A note that is already there is left alone: the first uncommitted use is the one
/// that defines the baseline, and overwriting it on the second use would silently
/// commit the first.
pub fn arm_cash_baseline(data_dir: &Path, buffs: &crate::buffs::Buffs, levelups: u32) {
    let path = cash_baseline_path(data_dir);
    if path.exists() {
        return;
    }
    let text = crate::buffs::serialise_baseline(&crate::buffs::CashBaseline {
        buffs: *buffs,
        levelups,
    });
    if let Err(e) = std::fs::write(&path, text) {
        // Not fatal: the effect and the retry still happen, we just lose the ability
        // to undo them. Worth a warning because it re-opens the duplication window.
        log::warn!("failed to arm the cash rollback note: {e}");
    }
}

/// Read the rollback note without consuming it. `None` = nothing pending, or a note
/// too damaged to trust (see [`crate::buffs::parse_baseline`] for that policy).
pub fn read_cash_baseline(data_dir: &Path) -> Option<crate::buffs::CashBaseline> {
    let text = std::fs::read_to_string(cash_baseline_path(data_dir)).ok()?;
    let parsed = crate::buffs::parse_baseline(&text);
    if parsed.is_none() {
        log::warn!("cash rollback note is unreadable — leaving host state as it is");
    }
    parsed
}

/// Drop the note: the guest save on disk now includes the consumption, so there is
/// nothing left to roll back to.
pub fn clear_cash_baseline(data_dir: &Path) {
    match std::fs::remove_file(cash_baseline_path(data_dir)) {
        Ok(()) => log::info!("cash rollback note cleared — the save committed the consumption"),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => log::warn!("failed to clear the cash rollback note: {e}"),
    }
}

fn cooldowns_path(data_dir: &Path) -> PathBuf {
    data_dir.join("wie_cooldowns.txt")
}

/// Per-skill cooldown overrides, one skill per line:
///
/// ```text
/// # sheet cell ms        ("#" starts a comment)
/// s4 0 0                 # sheet as the game's own id (s4..s10), cell 0..8
/// 2  3 8000              # or sheet as the atlas index (0..6 = s4..s10)
/// ```
///
/// `ms` = that skill's cooldown; 0 = none. **Skills not listed have NO cooldown**
/// (2026-08-07, player: "미기재 스킬은 스킬쿨타임이 없는거야") — the game's own
/// cast delay is the only gate, exactly as the original plays. Keyed the same way
/// the HUD keys skill icons — `(atlas sheet 0..6, strip cell 0..8)` — because
/// that pair is the one identity the host can always resolve for a quickbar
/// skill (see `items::skill_icon`).
///
/// A hand-maintained file rather than guest data, because the original game HAS
/// no per-skill cooldown table to read (skills are gated by SP alone) — the
/// numbers are the player's own tuning. A damaged line is skipped, never fatal.
pub fn read_skill_cooldowns(data_dir: &Path) -> std::collections::HashMap<(u8, u8), u64> {
    let mut out = std::collections::HashMap::new();
    let Ok(text) = std::fs::read_to_string(cooldowns_path(data_dir)) else {
        return out;
    };
    for line in text.lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        let mut it = line.split_whitespace();
        let (Some(sheet), Some(cell), Some(ms)) = (it.next(), it.next(), it.next()) else {
            continue;
        };
        // `s4`..`s10` (the game's sheet id) or a bare atlas index 0..6.
        let sheet: u8 = match sheet.strip_prefix(['s', 'S']) {
            Some(id) => match id.parse::<u8>() {
                Ok(id @ 4..=10) => id - 4,
                _ => continue,
            },
            None => match sheet.parse::<u8>() {
                Ok(a @ 0..=6) => a,
                _ => continue,
            },
        };
        let (Ok(cell), Ok(ms)) = (cell.parse::<u8>(), ms.parse::<u64>()) else {
            continue;
        };
        if cell > 8 {
            continue;
        }
        out.insert((sheet, cell), ms);
    }
    out
}

fn immersive_path(data_dir: &Path) -> PathBuf {
    data_dir.join("wie_immersive.txt")
}

/// Landscape **immersive** mode (default ON): give the guest an aspect-matched
/// wide framebuffer and let the game frame fill the whole screen, with the
/// controls as translucent overlays instead of side columns.
///
/// Off = the classic layout (portrait-shaped game centred, control columns at
/// the sides). Only takes effect at app start — the WIPI screen framebuffer is
/// allocated once and its pointer is cached by the guest.
pub fn read_immersive(data_dir: &Path) -> bool {
    match std::fs::read_to_string(immersive_path(data_dir)) {
        Ok(s) => s.trim() != "off", // default (missing/other) = on
        Err(_) => true,
    }
}

pub fn write_immersive(data_dir: &Path, on: bool) {
    let value = if on { "on" } else { "off" };
    if let Err(e) = std::fs::write(immersive_path(data_dir), value) {
        log::warn!("failed to persist immersive pref: {e}");
    }
}

/// Cheap-synth switch (default OFF = today's sound, unchanged).
///
/// ★ 2026-08-09. The MIDI synth's fixed per-sample cost is paid on the AAudio callback
/// thread whether or not a note is playing: `oxisynth`'s `Core::one_block` runs reverb
/// and chorus with **no voice-count guard**, so a silent menu still renders 16 comb +
/// 8 allpass filters and three 5-tap chorus blocks at 44,100 Hz. Measured on the
/// developer's own S21 at the title screen: the `AudioTrack` thread sits at 13.3 % of a
/// core. On a slower device that is a callback that can miss its deadline, and a missed
/// deadline is the crackle the player hears.
///
/// This is NOT on by default, and that is the point: turning reverb and chorus off
/// CHANGES HOW THE GAME SOUNDS, and the standing rule for this round is that nothing
/// regresses on the phone that already works. So the default build is bit-identical to
/// today, and this file is the one-command escape hatch for a device that cannot keep
/// up — flip it, restart, and compare `AudioTrack` in `top -H`.
///
/// Read ONCE, when the synth is built (`audio_sink::build_synth`), so a change needs an
/// app restart.
pub fn read_audio_lite(data_dir: &Path) -> bool {
    // Same two-location search as `read_blit_hle`, and for the same reason: a RELEASE
    // build's internal data dir is unreachable over adb, and a debug build would be
    // measuring a different binary.
    for path in [
        PathBuf::from(format!("{SDCARD_FILES}/wie_audio_lite.txt")),
        data_dir.join("wie_audio_lite.txt"),
    ] {
        if let Ok(s) = std::fs::read_to_string(&path) {
            let on = s.trim() == "on";
            log::warn!("audio lite switch read from {path:?}: {}", if on { "on" } else { "off" });
            return on;
        }
    }
    false
}

/// Which synth plays hero2's songs: `true` = their own FM voices (the default), `false` =
/// the General MIDI SoundFont the app used before 2026-10-01.
///
/// ★ 2026-10-01 (`HERO2_BGM_FM_PLAN.md` §3 B3). `wie_bgm_synth.txt` = `gm` goes back to the
/// old sound — the one-file escape hatch for an A/B on the device, or for a player who
/// prefers it, until the question of a settings row is settled. Anything else, or no file,
/// is FM. Same two-location search as [`read_audio_lite`], and read ONCE at start-up
/// (`AndroidPlatform::new`), so a change needs an app restart.
/// ★★ 2026-10-15 「악마화」 HD. `wie_hd_fx.txt` = `0` draws 악의's effects and body at the guest's resolution as before;
/// anything else, or no file, is HD (the host composites the library's 3× twins at the window's resolution). Same two
/// locations as [`read_bgm_synth`], read once at start-up.
pub fn read_hd_fx(data_dir: &Path) -> bool {
    for path in [PathBuf::from(format!("{SDCARD_FILES}/wie_hd_fx.txt")), data_dir.join("wie_hd_fx.txt")] {
        if let Ok(s) = std::fs::read_to_string(&path) {
            let on = s.trim() != "0";
            log::warn!("hd fx switch read from {path:?}: {}", if on { "on" } else { "off" });
            return on;
        }
    }
    true
}

pub fn read_bgm_synth(data_dir: &Path) -> bool {
    for path in [
        PathBuf::from(format!("{SDCARD_FILES}/wie_bgm_synth.txt")),
        data_dir.join("wie_bgm_synth.txt"),
    ] {
        if let Ok(s) = std::fs::read_to_string(&path) {
            let fm = s.trim() != "gm";
            log::warn!("bgm synth switch read from {path:?}: {}", if fm { "fm" } else { "gm" });
            return fm;
        }
    }
    true
}

/// Guest PC profiler (default OFF).
///
/// ★ 2026-08-09. The one lever that actually moves the framerate on a slow device is
/// removing guest INSTRUCTIONS, and 26차 proved the method: find the hottest guest
/// function, reimplement it on the host, gate it, verify pixel-identical output. That
/// single patch (`blit565`, guest 0x110fe4) took 36 % of all guest instructions away.
///
/// This turns the sampler on so the NEXT such function can be named instead of guessed.
/// It costs a stack walk per sample, so it is not something to ship enabled.
pub fn read_guest_profile(data_dir: &Path) -> bool {
    for path in [
        PathBuf::from(format!("{SDCARD_FILES}/wie_profile.txt")),
        data_dir.join("wie_profile.txt"),
    ] {
        if let Ok(s) = std::fs::read_to_string(&path) {
            let on = s.trim() == "on";
            log::warn!("guest profiler switch read from {path:?}: {}", if on { "on" } else { "off" });
            return on;
        }
    }
    false
}

fn heat_saver_path(data_dir: &Path) -> PathBuf {
    data_dir.join("wie_heat_saver.txt")
}

fn pad_hud_path(data_dir: &Path) -> PathBuf {
    data_dir.join("wie_pad_hud.txt")
}

/// 게임패드 전용 HUD (59차, default AUTO). `auto` = the last input source decides,
/// `on` = always, `off` = never. Same two-location search as the switches above so a
/// release build can be flipped over adb.
///
/// Unlike HEAT SAVER this applies AT ONCE: the HUD is rebuilt from
/// `crate::pad_hud_active()` every relayout, so there is nothing to restart.
pub fn read_pad_hud(data_dir: &Path) -> crate::padhud::PadHud {
    for path in [
        PathBuf::from(format!("{SDCARD_FILES}/wie_pad_hud.txt")),
        pad_hud_path(data_dir),
    ] {
        if let Ok(s) = std::fs::read_to_string(&path) {
            return crate::padhud::PadHud::parse(&s);
        }
    }
    crate::padhud::PadHud::default()
}

pub fn write_pad_hud(data_dir: &Path, p: crate::padhud::PadHud) {
    if let Err(e) = std::fs::write(pad_hud_path(data_dir), p.as_token()) {
        log::warn!("failed to persist pad hud pref: {e}");
    }
}

/// Effect-bus level, as an index into [`crate::audio_mix::SFX_LEVELS`].
///
/// One location only, unlike [`read_heat_saver`]: this one is set from the settings
/// modal and has no diagnostic value over adb.
///
/// ★ Missing / unreadable / out of range all mean the shipped default, so a fresh
/// install and a corrupt file sound the same as every build before this setting existed.
pub fn read_sfx_level(data_dir: &Path) -> u8 {
    match std::fs::read_to_string(data_dir.join("wie_sfx_level.txt")) {
        Ok(s) => s.trim().parse::<u8>().ok().filter(|v| usize::from(*v) < crate::audio_mix::SFX_LEVELS.len()),
        Err(_) => None,
    }
    .unwrap_or(crate::audio_mix::SFX_LEVEL_DEFAULT)
}

pub fn write_sfx_level(data_dir: &Path, level: u8) {
    let path = data_dir.join("wie_sfx_level.txt");
    if let Err(e) = std::fs::write(&path, level.to_string()) {
        log::warn!("config: cannot write {path:?}: {e}");
    }
}

/// 경험치 배율 (2026-09-23) — the multiplier itself, one of
/// [`crate::guest_build::EXP_RATES`], as decimal text. The value rather than an index so
/// the file says what it does over `adb` (`4` = x4).
///
/// ★ Missing / unreadable / off the table all mean 기존 (x1): a fresh install and a
/// corrupt file both play the shipped game, and no file can ask for a multiplier the
/// row cannot show.
pub fn read_exp_rate(data_dir: &Path) -> u8 {
    match std::fs::read_to_string(data_dir.join("wie_exp_rate.txt")) {
        Ok(s) => s.trim().parse::<u8>().ok(),
        Err(_) => None,
    }
    .map_or(crate::guest_build::EXP_RATE_DEFAULT, crate::guest_build::exp_rate_sanitise)
}

pub fn write_exp_rate(data_dir: &Path, mult: u8) {
    let path = data_dir.join("wie_exp_rate.txt");
    if let Err(e) = std::fs::write(&path, mult.to_string()) {
        log::warn!("config: cannot write {path:?}: {e}");
    }
}

/// ★ 2026-10-12 마지막으로 실행한 세이브 슬롯 — the 0-based slot the player last entered the world from, so the slot
/// screen opens there after a restart too (`guest_anchors::SlotFocusPatch`). Missing, unreadable or past the five
/// slots = none (the screen opens as shipped, on the first save).
pub fn read_save_last(data_dir: &Path) -> Option<u8> {
    let s = std::fs::read_to_string(data_dir.join("wie_save_last.txt")).ok()?;
    s.trim().parse::<u8>().ok().filter(|v| *v < crate::guest_build::SaveSlotsPatch::SLOTS)
}

pub fn write_save_last(data_dir: &Path, slot: u8) {
    let path = data_dir.join("wie_save_last.txt");
    if let Err(e) = std::fs::write(&path, slot.to_string()) {
        log::warn!("config: cannot write {path:?}: {e}");
    }
}

/// 오토루팅 (2026-09-24) — `1` on, `0` off. ★ Missing / unreadable means ON: the
/// player asked for the feature, and a fresh install should have it.
pub fn read_auto_loot(data_dir: &Path) -> bool {
    match std::fs::read_to_string(data_dir.join("wie_auto_loot.txt")) {
        Ok(s) => s.trim() != "0",
        Err(_) => true,
    }
}

pub fn write_auto_loot(data_dir: &Path, on: bool) {
    let path = data_dir.join("wie_auto_loot.txt");
    if let Err(e) = std::fs::write(&path, if on { "1" } else { "0" }) {
        log::warn!("config: cannot write {path:?}: {e}");
    }
}

/// 난이도 (2026-09-29) — `0` EASY · `1` NORMAL · `2` HARD. ★ Missing / unreadable / off the
/// table all mean EASY: a fresh install and a corrupt file both play the shipped game.
pub fn read_difficulty(data_dir: &Path) -> u8 {
    match std::fs::read_to_string(data_dir.join("wie_difficulty.txt")) {
        Ok(s) => s.trim().parse::<u8>().ok(),
        Err(_) => None,
    }
    .map_or(crate::guest_build::DIFFICULTY_DEFAULT, crate::guest_build::difficulty_sanitise)
}

pub fn write_difficulty(data_dir: &Path, d: u8) {
    let path = data_dir.join("wie_difficulty.txt");
    if let Err(e) = std::fs::write(&path, d.to_string()) {
        log::warn!("config: cannot write {path:?}: {e}");
    }
}

/// 드랍률 (2026-10-16) — `0` 낮음 · `1` 보통 · `2` 높음. ★ Missing / unreadable / off the table all mean 높음:
/// a fresh install and a corrupt file both play today's game.
pub fn read_drop_rate(data_dir: &Path) -> u8 {
    match std::fs::read_to_string(data_dir.join("wie_drop_rate.txt")) {
        Ok(s) => s.trim().parse::<u8>().ok(),
        Err(_) => None,
    }
    .map_or(crate::guest_build::DROP_RATE_DEFAULT, crate::guest_build::drop_rate_sanitise)
}

pub fn write_drop_rate(data_dir: &Path, d: u8) {
    let path = data_dir.join("wie_drop_rate.txt");
    if let Err(e) = std::fs::write(&path, d.to_string()) {
        log::warn!("config: cannot write {path:?}: {e}");
    }
}

/// 발열 절감 모드 (57차, default OFF). One switch, three effects, all fixed at
/// process start:
///
///  * the UI renders at HALF resolution and `ANativeWindow_setBuffersGeometry`
///    has the compositor upscale in hardware — every pixel-proportional cost on
///    the UI thread (clear, game blit, overlay draws, the present copy) drops 4x;
///  * the MIDI synth runs in its lite preset (see [`read_audio_lite`] — reverb/
///    chorus off), which is the documented escape hatch for exactly this device
///    class and worth ~13 % of a core even in silence;
///  * (nothing changes in the emulator worker: game speed is timer-driven and a
///    slower render never slows the game).
///
/// Default OFF because the half-resolution upscale visibly softens the overlay a
/// little and the standing rule is that nothing regresses on the phone that
/// already works — the player flips it per device from the settings modal
/// (HEAT SAVER row). Same two-location search as [`read_blit_hle`] so a release
/// build can be flipped over adb too.
pub fn read_heat_saver(data_dir: &Path) -> bool {
    for path in [
        PathBuf::from(format!("{SDCARD_FILES}/wie_heat_saver.txt")),
        heat_saver_path(data_dir),
    ] {
        if let Ok(s) = std::fs::read_to_string(&path) {
            return s.trim() == "on";
        }
    }
    false
}

pub fn write_heat_saver(data_dir: &Path, on: bool) {
    let value = if on { "on" } else { "off" };
    if let Err(e) = std::fs::write(heat_saver_path(data_dir), value) {
        log::warn!("failed to persist heat saver pref: {e}");
    }
}

/// Gamepad button overrides (58차, `wie_gamepad.txt`; default = empty, i.e. the
/// built-in layout in `gamepad::default_button_action`). One line per button:
/// `<android keycode>=<TOKEN>` where TOKEN is `UP DOWN LEFT RIGHT OK CLR 0`..`9 #
/// * PAGE NONE` — e.g. `96=OK`, `107=PAGE`, `97=NONE`. Read once at App start;
/// garbage lines are skipped (a typo must never take the pad down with it). Same
/// two-location search as [`read_blit_hle`] so a release build can be remapped
/// over adb without a rebuild.
pub fn read_gamepad_map(
    data_dir: &Path,
) -> std::collections::HashMap<i32, Option<crate::gamepad::PadAction>> {
    let mut map = std::collections::HashMap::new();
    for path in [
        PathBuf::from(format!("{SDCARD_FILES}/wie_gamepad.txt")),
        data_dir.join("wie_gamepad.txt"),
    ] {
        if let Ok(s) = std::fs::read_to_string(&path) {
            for line in s.lines() {
                if let Some((code, action)) = crate::gamepad::parse_map_line(line) {
                    map.insert(code, action);
                }
            }
            log::info!("gamepad map read from {path:?}: {} override(s)", map.len());
            break;
        }
    }
    map
}

/// Persist the pad screen's table (2026-10-09, with hero3) — the first WRITER
/// `wie_gamepad.txt` has.
///
/// ★ It writes the file [`read_gamepad_map`] will READ: the external copy when one exists
/// (an adb-pushed remap), the internal one otherwise. Writing the internal file while an
/// external one exists would be shadowed by it at the next start, and the player's change
/// would silently come undone.
///
/// Atomic (temp + rename): a crash mid-write leaves the old file, never half a table — half
/// a table is a pad with buttons that quietly went back to their defaults.
pub fn write_gamepad_map(data_dir: &Path, text: &str) -> bool {
    let external = PathBuf::from(format!("{SDCARD_FILES}/wie_gamepad.txt"));
    let path = if external.exists() { external } else { data_dir.join("wie_gamepad.txt") };
    let tmp = path.with_file_name(".wie_gamepad.txt.tmp");
    let result = std::fs::write(&tmp, text.as_bytes()).and_then(|()| std::fs::rename(&tmp, &path));
    match result {
        Ok(()) => {
            log::info!("gamepad map written to {path:?} ({} line(s))", text.lines().filter(|l| !l.starts_with('#')).count());
            true
        }
        Err(e) => {
            let _ = std::fs::remove_file(&tmp);
            log::warn!("failed to persist the gamepad map to {path:?}: {e}");
            false
        }
    }
}

fn blit_hle_path(data_dir: &Path) -> PathBuf {
    data_dir.join("wie_blit_hle.txt")
}

/// Host sprite blitter (default ON). Writing `off` here makes the emulator hand every
/// blit back to the interpreted guest code — the exact behaviour before 26차.
///
/// ★ This exists for MEASUREMENT, not for players. A device A/B of the biggest
/// optimisation in the emulator is otherwise impossible: the only alternative is
/// installing the previous APK, and that is a downgrade, which means uninstall, which
/// means the save is gone. With this file the comparison is the same phone, the same
/// save, the same map, one restart apart. It is read once at worker start because the
/// binary patch is installed during boot.
pub fn read_blit_hle(data_dir: &Path) -> bool {
    // The internal data dir is unreachable over adb for a RELEASE build (`run-as` refuses a
    // non-debuggable package, and a debug build would measure a different binary), so the
    // app's own external files dir is checked first. `adb shell` is exempt from scoped
    // storage for `/sdcard/Android/data`, which makes this the only way to flip the switch
    // on the build that actually ships.
    for path in [
        PathBuf::from(format!("{SDCARD_FILES}/wie_blit_hle.txt")),
        blit_hle_path(data_dir),
    ] {
        if let Ok(s) = std::fs::read_to_string(&path) {
            let on = s.trim() != "off"; // default (missing/other) = on
            log::warn!("blit HLE switch read from {path:?}: {}", if on { "on" } else { "off" });

            return on;
        }
    }

    true
}

fn frame_timer_cap_path(data_dir: &Path) -> PathBuf {
    data_dir.join("wie_frame_timer_cap.txt")
}

/// The default ceiling on the guest's own frame timer, in milliseconds.
///
/// 50 is not a tuning knob picked by feel — it is the interval Hero3 itself asks for on
/// the field (`emu frame timing: min 50 ms`). Capping at the game's own fastest request
/// means the modal screens are woken no more often than the game already wakes itself
/// elsewhere, so nothing runs at a rate this build has never run at.
pub const FRAME_TIMER_CAP_DEFAULT_MS: u64 = 50;

/// Ceiling on `MC_knlSetTimer` (default 50 ms; `off`/`0` = honour the guest exactly).
///
/// ★ Why this switch exists. 37차 removed the host's two 16 ms roundings and the game
/// went 22 → 69 fps on the title. What survived was the game's OWN number: in the
/// modal/SLOT states — network warehouse, cash shop — it asks to be woken every
/// **244 ms**, which is 4 fps by design. 2026-08-14 device measurement showed nothing
/// else is left to blame there (page turn = one round trip, 24~49 ms; `recv polls/s` =
/// 0), so the only way further is to wake the guest earlier than it asked.
///
/// That is a behaviour change, not an optimisation, which is why it is a runtime switch
/// with the same external-files-dir escape hatch as [`read_blit_hle`]: a guest loop that
/// counts frame ticks instead of wall-clock now counts them ~5x faster in those screens.
/// Flip it off and restart to compare on the same phone and the same save:
/// `adb shell "echo off > /sdcard/Android/data/com.hero2remake.app/files/wie_frame_timer_cap.txt"`
pub fn read_frame_timer_cap(data_dir: &Path) -> u64 {
    for path in [
        PathBuf::from(format!("{SDCARD_FILES}/wie_frame_timer_cap.txt")),
        frame_timer_cap_path(data_dir),
    ] {
        if let Ok(s) = std::fs::read_to_string(&path) {
            let s = s.trim();
            let ms = if s == "off" { 0 } else { s.parse::<u64>().unwrap_or(FRAME_TIMER_CAP_DEFAULT_MS) };
            log::warn!("frame timer cap read from {path:?}: {ms} ms (0 = off)");

            return ms;
        }
    }

    FRAME_TIMER_CAP_DEFAULT_MS
}

fn base_h_path(data_dir: &Path) -> PathBuf {
    data_dir.join("wie_base_h.txt")
}

/// Immersive-mode logical content height. Smaller = more of the screen is
/// guaranteed to be filled by the map (Hero3 maps are 176..1120 px wide, median
/// 512) but chunkier pixels; larger = finer pixels but black side margins on
/// narrow maps. Cycled from the settings menu; applies at next start.
///
/// 영웅서기2 는 세로 220 px 게임이라 220 이 네이티브(1:1) 높이다. 그보다 작은 두 값은
/// 발열/저사양용(픽셀이 굵어지는 대신 부하가 준다). A previously persisted value outside
/// the list falls back to [`crate::LANDSCAPE_BASE_H`].
pub const BASE_H_CHOICES: [u32; 3] = [160, 190, 220];

pub fn read_base_h(data_dir: &Path) -> u32 {
    let v = std::fs::read_to_string(base_h_path(data_dir))
        .ok()
        .and_then(|s| s.trim().parse::<u32>().ok())
        .unwrap_or(crate::LANDSCAPE_BASE_H);
    if BASE_H_CHOICES.contains(&v) { v } else { crate::LANDSCAPE_BASE_H }
}

pub fn write_base_h(data_dir: &Path, h: u32) {
    if let Err(e) = std::fs::write(base_h_path(data_dir), h.to_string()) {
        log::warn!("failed to persist base height: {e}");
    }
}

fn layout_path(data_dir: &Path) -> PathBuf {
    data_dir.join("wie_layout.txt")
}

/// The player's custom control layout, one control per line:
///
/// ```text
/// # mode id centre-x centre-y size-percent
/// field ok 0.9120 0.8083 130
/// menu key1 0.4000 0.6000 100
/// ```
///
/// Position is a FRACTION of the window and size a PERCENT of the built-in size —
/// never pixels (a layout authored on one device would be nonsense on another, and
/// never guest px, which are frozen at boot). Parsing is per-line defensive: a line
/// that does not parse is dropped and everything else still loads, the same
/// discipline [`read_base_h`] uses for a single value.
pub fn read_layout(data_dir: &Path) -> crate::placement::Table {
    let mut table = crate::placement::Table::default();
    let Ok(text) = std::fs::read_to_string(layout_path(data_dir)) else {
        return table;
    };
    let (mut ok, mut bad, mut stale, mut folded, mut tabs) = (0u32, 0u32, 0u32, 0u32, 0u32);
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        // ★ A `menu ...` line is SUPERSEDED, not corrupt: it belongs to the second seat
        // table, removed 2026-09-18 (`placement::Table`). Counted apart so the log says
        // "your old menu arrangement was dropped on purpose" instead of accusing the file
        // of being damaged - the menu controls follow the field arrangement now, and the
        // next write simply leaves these lines out.
        if line.starts_with(crate::placement::LEGACY_MENU_PREFIX) {
            stale += 1;
            continue;
        }
        // ★ hero3 91차: a `dpad …` line is the D-pad control that was folded into the
        // stick. It still parses (as the stick's seat) - counted so the log says the old
        // arrows' seat was ADOPTED, and so that "where did my dpad line go" has an answer.
        // It is written back as a `stick` line, so this fires once per migration.
        if crate::placement::is_legacy_dpad_line(line) {
            folded += 1;
        }
        // ★ 2026-10-01: a `qsave` / `mapt` line placed the ROUND SAVE / minimap-toggle
        // buttons. Those are fixed TABS on the top edge now (`placement::CtrlId::
        // is_top_tab`), so the seat is dropped on purpose - counted apart so the log
        // does not call the file damaged. The next layout save leaves the line out.
        if crate::placement::is_top_tab_line(line) {
            tabs += 1;
            continue;
        }
        match crate::placement::parse_line(line) {
            Some((id, place)) => {
                table.set(id, Some(place));
                ok += 1;
            }
            None => bad += 1,
        }
    }
    if bad > 0 {
        log::warn!("layout: {bad} unreadable line(s) dropped, {ok} loaded (a retired `k<n>` menu-key line counts here)");
    }
    if stale > 0 {
        log::warn!("layout: {stale} superseded menu-mode line(s) dropped - one layout now, {ok} loaded");
    }
    if folded > 0 {
        log::warn!("layout: {folded} dpad line(s) adopted as the stick's seat - the stick and the arrows are one control now");
    }
    if tabs > 0 {
        log::warn!("layout: {tabs} SAVE/MAP seat line(s) dropped - those two are fixed tabs on the top edge now, {ok} loaded");
    }
    table
}

pub fn write_layout(data_dir: &Path, table: &crate::placement::Table) {
    let mut out = String::from("# WIE custom control layout — field id cx cy pct\n");
    out.push_str(&table.to_text());
    if let Err(e) = std::fs::write(layout_path(data_dir), out) {
        log::warn!("failed to persist the control layout: {e}");
    }
}

fn server_path(data_dir: &Path) -> PathBuf {
    data_dir.join("wie_server.txt")
}

// The endpoint LINE PARSER lives in `endpoint.rs` so a standalone selftest can
// include the real code; this module owns the file/env layering around it.
pub use crate::endpoint::{DEFAULT_SERVER_PORT, Endpoint, parse_endpoint, parse_pin};

/// The private-server endpoint the game's (dead) hardcoded address is rewritten
/// to — see `network::AndroidNetwork::connect`.
///
/// Stored as one line, `host` or `host:port`, in the app-private dir. Resolution
/// order (first hit wins), so one build works for a tester, a self-hoster and the
/// protocol-learning loop:
///
///   1. this file — set from the in-app settings, so the address can change
///      (server moved, tunnel re-issued its port) WITHOUT a rebuild+reinstall;
///   2. the `HERO3_SERVER_HOST` / `HERO3_SERVER_PORT` build-time env, so a build
///      handed to testers already points at the public server and they configure
///      nothing;
///   3. `127.0.0.1` — the `adb reverse tcp:8091 tcp:8091` dev loop.
///
/// ★ Deliberately NOT gated on `debug_assertions`. It used to be, which pinned
/// every debug build to localhost — and since the release build has no signing
/// config, that made "an APK someone else can actually install AND that reaches
/// a public server" impossible. The file/env layers now apply to both profiles;
/// localhost is only the last-resort default.
///
/// A host is returned verbatim (an IP literal or a name — `connect_resolved`
/// does the DNS). A `tls://` prefix selects an encrypted transport; see
/// [`Endpoint::tls`]. Garbage in the file falls back to the next layer rather
/// than failing the connect, the same discipline [`read_base_h`] uses.
pub fn read_server_endpoint(data_dir: &Path) -> Endpoint {
    if let Some(ep) = std::fs::read_to_string(server_path(data_dir))
        .ok()
        .and_then(|s| parse_endpoint(&s))
    {
        return ep;
    }
    build_time_endpoint()
}

/// The endpoint baked in at build time (env), or the localhost dev default.
/// Exposed so the settings UI can offer "back to the built-in default".
///
/// `HERO3_SERVER_HOST` may itself carry the `tls://` prefix, so one env var still
/// decides both destination and transport for a tester build.
pub fn build_time_endpoint() -> Endpoint {
    let host = match option_env!("HERO3_SERVER_HOST") {
        Some(h) if !h.is_empty() => h,
        // No public host was baked in: fall back to the adb-reverse dev loop
        // rather than a bogus name, so a plain `build_android.ps1` keeps working.
        _ => "127.0.0.1",
    };
    let port: u16 = match option_env!("HERO3_SERVER_PORT") {
        Some(p) => p.parse().unwrap_or(DEFAULT_SERVER_PORT),
        None => DEFAULT_SERVER_PORT,
    };
    // Route it through the same parser so `tls://` works here too, and fall back
    // to a plain endpoint if the env held something unusable.
    parse_endpoint(&format!("{host}:{port}")).unwrap_or(Endpoint {
        host: host.to_string(),
        port,
        tls: false,
    })
}

// ★ There is deliberately no `write_server_endpoint` here. The endpoint is WRITTEN
// by `MainActivity.promptServerAddress` (Java), because the value comes from the
// platform text dialog — the overlay font is a 5x7 ASCII bitmap with no text input,
// so the keyboard has to be the OS one. Rust owns READING and PARSING (this module
// plus `endpoint.rs`, covered by `endpoint_selftest.rs`); Java writes one line and
// validates its shape before doing so. Adding a Rust writer would give the file two
// authors and two notions of its format.

fn pin_path(data_dir: &Path) -> PathBuf {
    data_dir.join("wie_server_pin.txt")
}

/// The pinned server-certificate fingerprint (SHA-256 of its DER), if one is set.
///
/// Resolution order mirrors the endpoint's: the on-device file first, then the
/// `HERO3_SERVER_PIN` build-time env.
///
/// Only needed for a SELF-SIGNED server. With a normal certificate (Let's Encrypt)
/// validation goes through the bundled Mozilla root store and neither layer is set.
/// Pinning is the stronger of the two — it trusts exactly one certificate rather
/// than every public CA, and skips expiry/name checks so a long-lived certificate
/// needs no renewal — at the cost of having to change the value when the
/// certificate is replaced.
///
/// ★ The BUILD-TIME layer is what makes a domain optional. A self-hoster can point
/// the app at a bare VPS address with a self-signed certificate, bake the pin into
/// the APK, and hand that APK to players who configure nothing — no domain, no CA,
/// no certificate renewal, and no `adb` to push a pin file (which is impossible for
/// an ordinary user anyway, since the data dir is app-private).
pub fn read_server_pin(data_dir: &Path) -> Option<[u8; 32]> {
    if let Ok(raw) = std::fs::read_to_string(pin_path(data_dir)) {
        match parse_pin(&raw) {
            Some(pin) => return Some(pin),
            // Loud, because the consequence of silently ignoring it is falling
            // back to CA validation on a self-signed server = every connect fails,
            // which looks like a network problem rather than a config typo.
            None => log::warn!("wie_server_pin.txt is not a 32-byte SHA-256 hex digest — ignoring it"),
        }
    }
    match option_env!("HERO3_SERVER_PIN") {
        Some(s) if !s.is_empty() => match parse_pin(s) {
            Some(pin) => Some(pin),
            None => {
                log::error!("HERO3_SERVER_PIN was set at build time but is not a SHA-256 hex digest");
                None
            }
        },
        _ => None,
    }
}

fn account_id_path(data_dir: &Path) -> PathBuf {
    data_dir.join("wie_account_id")
}

/// The 16-byte per-device account identity injected into the game's `01 3d`
/// login packet (see `network::AndroidNetwork::send`).
///
/// The vanilla Hero3 binary hardcodes this field to the constant `"0123456789"`
/// (its `getProperty("PHONENUMBER")` is a stub), so without this every player
/// would share ONE server account. We give each install a stable, unique id so
/// the private server keys each user's warehouse / recovery backups separately.
///
/// ★ ACCOUNT MODEL (user decision, PROTOCOL.md §6): this is the **device-auto-id**
/// model — the id is generated once and persisted in the app-private dir, so it
/// is stable across restarts but NOT across reinstall / device change. That is
/// enough to separate concurrent users, but it does NOT give cross-device
/// "복구" (recover my character on a new phone). For that, swap this for a
/// **user login (ID/PW)** model: derive the 16 bytes from a user-entered account
/// token (needs a small login UI). Left as device-auto until the user chooses.
pub fn read_or_create_account_id(data_dir: &Path) -> [u8; 16] {
    let path = account_id_path(data_dir);
    if let Ok(bytes) = std::fs::read(&path) {
        if bytes.len() == 16 {
            let mut id = [0u8; 16];
            id.copy_from_slice(&bytes);
            return id;
        }
    }
    // First run: 16 bytes from the OS CSPRNG.
    //
    // ★ 2026-07-31: this used to expand ONE time-based u64 seed with splitmix64,
    // which looks like 128 bits but carries at most 64 bits of entropy — and far
    // less in practice, because the seed is "nanoseconds since the epoch at first
    // launch", a value an attacker can bracket to a narrow window. While the
    // server was only reachable over a USB tunnel that did not matter. On a public
    // server the id IS the credential, so a guessable id is a guessable password.
    let id = random_id().unwrap_or_else(|| {
        // Never fail to produce an id — a missing identity would mean no online
        // features at all. splitmix64 over the clock is the old behaviour, kept
        // only as a last resort and logged loudly.
        log::error!("net: OS randomness unavailable; falling back to a time-seeded account id");
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0x9E37_79B9_7F4A_7C15);
        let mut id = [0u8; 16];
        let mut x = seed ^ 0x9E37_79B9_7F4A_7C15;
        for chunk in id.chunks_mut(8) {
            x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
            let mut z = x;
            z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
            z ^= z >> 31;
            let bytes = z.to_le_bytes();
            chunk.copy_from_slice(&bytes[..chunk.len()]);
        }
        id
    });
    if let Err(e) = std::fs::write(&path, id) {
        log::warn!("failed to persist account id: {e}");
    }
    log::info!("net: generated a new device account id (128-bit, OS CSPRNG)");
    id
}

/// The account identity as a 32-character hex string, for the transfer dialog.
///
/// ★ Why a transfer path has to exist at all: the identity lives ONLY in the
/// app-private dir. Uninstall the app or move to a new phone and the server-side
/// warehouse, balances and character backups become unreachable forever — the
/// server has no other way to recognise the player. With more than one user that
/// is not a corner case, it is a certainty, so the code has to be readable out and
/// typeable back in (see `MainActivity.promptAccountCode`).
///
/// This is also, unavoidably, the credential. The dialog says so.
pub fn account_id_hex(data_dir: &Path) -> String {
    read_or_create_account_id(data_dir)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// 16 bytes from the kernel CSPRNG, or `None` if it cannot be read.
///
/// `/dev/urandom` rather than a crate: it is always present on Android/Linux,
/// needs no new dependency, and is the same pool `getrandom(2)` draws from. Note
/// this only runs ONCE per install — the id is persisted after that — so the read
/// cost is irrelevant.
fn random_id() -> Option<[u8; 16]> {
    use std::io::Read;
    let mut f = std::fs::File::open("/dev/urandom").ok()?;
    let mut buf = [0u8; 16];
    f.read_exact(&mut buf).ok()?;
    // A kernel CSPRNG never returns all zeros in practice; treating it as a
    // failure means a stubbed/emulated device cannot silently give every install
    // the same "random" id.
    if buf == [0u8; 16] { None } else { Some(buf) }
}
