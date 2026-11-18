//! Host-side TCP sockets for the WIPI net API (Track C — private server).
//!
//! The guest contract this must satisfy is documented in
//! `TRACK_C_PRIVATE_SERVER.md` §3 and was established by device tracing +
//! disassembly of `client.bin`:
//!
//! ```text
//!   slot 2   socket(af=2, type=1) -> fd
//!   slot 30  connect(fd, addr, port, name, cb, param)  -> 0/-19, result via cb
//!   slot 31  send(fd, buf, len)  -> bytes accepted, negative = teardown
//!   slot 32  recv(fd, buf, max)  -> n>0 | -19 no data | 0/other negative = peer gone
//!   slot 33  close(fd)
//! ```
//!
//! Two hard requirements shape this module:
//!
//! 1. **Nothing may block.** The emulator runs the interpreter on a worker
//!    thread with a small per-tick budget; a blocking syscall inside a WIPI-C
//!    call freezes the game. Every operation here is either non-blocking or is
//!    handed to a helper thread and polled.
//! 2. **`recv` must distinguish "no data yet" from "peer gone".** The guest
//!    treats -19 as healthy-but-idle and *anything else* non-positive as a dead
//!    connection, so `WouldBlock` and EOF must never be conflated.
//!
//! The original servers (210.222.18.28:8091, 211.239.150.70:2903) are dead, so
//! `REDIRECT` sends every connect to our private server instead. That keeps the
//! game archive unpatched — the hardcoded addresses stay in the binary and we
//! rewrite the destination here, logging the original.

use std::{
    collections::{HashMap, HashSet},
    io::{ErrorKind, Read, Write},
    net::{IpAddr, Ipv4Addr, SocketAddr, TcpStream, ToSocketAddrs},
    path::PathBuf,
    sync::{
        Mutex,
        atomic::{AtomicI32, Ordering},
        mpsc::{Receiver, TryRecvError, channel},
    },
    time::{Duration, Instant},
};

/// `M_E_WOULDBLOCK` — see the module docs. Must match
/// `wie_wipi_c::api::net::M_E_WOULDBLOCK`.
pub const WOULDBLOCK: i32 = -19;
/// Generic failure the guest treats as fatal for the socket.
pub const ERROR: i32 = -1;

/// `true` = rewrite the game's hardcoded (dead) address to our private server.
/// `false` = connect to whatever the game asks for (every original server is
/// gone today, so this only times out — kept as an escape hatch for testing).
///
/// The dead Hanbit addresses (210.222.18.28:8091 / 211.239.150.70:2903) are baked
/// into the game binary; rather than patch the archive we rewrite the destination
/// here and log the original.
///
/// ★ Where the destination comes from (2026-07-31): it is no longer a compile-time
/// constant. [`crate::config::read_server_endpoint`] resolves, in order, the
/// on-device `wie_server.txt` (settable in-app), the `HERO3_SERVER_HOST`/`_PORT`
/// build-time env, then localhost for the `adb reverse` dev loop. It is re-read on
/// every connect, so changing the address takes effect on the game's next login —
/// no rebuild, no reinstall, no restart. A *hostname* is preferred over an IP so
/// the server can move without touching any installed APK; DNS happens in
/// [`connect_resolved`].
const REDIRECT_ENABLED: bool = true;

/// Total budget for one connect attempt: DNS resolution plus the TCP handshakes.
///
/// Must stay UNDER the guest's own patience or the host and the game disagree
/// about whether a connect is still alive. The guest polls its connect callback
/// on a 50 ms × 240 timer (~12 s) before declaring failure, so 8 s leaves room
/// for the guest to see our verdict first. The budget is shared across every
/// address a name resolves to (a 5 s-per-address loop over four A records would
/// blow straight past the guest's deadline).
const CONNECT_BUDGET: Duration = Duration::from_secs(8);
/// Floor for the per-address share of [`CONNECT_BUDGET`], so a name with many
/// records still gives each candidate a usable chance instead of ~0 ms.
const MIN_PER_ADDR_TIMEOUT: Duration = Duration::from_millis(1200);

/// How long a "nothing on the socket" answer may be reused before the host looks
/// again. See [`AndroidNetwork::recv`] — this is the fix for the warehouse frame
/// drop (`MIGRATION_LGT.md` §2.24).
///
/// The guest's server-wait loop polls `recv` about 25,000 times a second, and every
/// poll used to cost a `read` syscall (plus rustls bookkeeping) on the emulator
/// thread. At this window the host looks at most 500 times a second — 98 % fewer —
/// and the worst case a message can sit undelivered is 2 ms, against a server
/// round trip of 30-250 ms. Already-decrypted bytes are NEVER withheld (the gate is
/// consulted only after the plaintext buffer is found empty), so this delays
/// *noticing* new bytes, never delivering known ones.
const RECV_EMPTY_REUSE: Duration = Duration::from_millis(2);

/// Instrumentation for the throttle above. `POLLS` counts what the guest asked for,
/// `ABSORBED` what the gate answered without a syscall, and `SOCKET_NS` the time the
/// remaining real polls spent in the socket. `absorbed/polls` near 1.0 with a small
/// `SOCKET_NS` is the healthy shape; `ABSORBED` at 0 while `POLLS` is in the tens of
/// thousands per second is the pre-fix behaviour, and is what §2.24 measured.
static RECV_POLLS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static RECV_ABSORBED: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static RECV_SOCKET_NS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Set when the app leaves the foreground; cleared by the first network call after it
/// comes back. See [`note_backgrounded`].
static BACKGROUNDED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
/// Raised when the resume probe actually found a dead connection, so the player can be
/// told. Consumed by [`take_drop_notice`], which the emulator loop turns into the toast
/// AND into the game's own disconnect modal (`emulator::show_disconnect_modal`).
///
/// ── WHAT THIS FLAG REPLACED (2026-08-15, 48차) ────────────────────────────────────
/// 47차-C had the host feed the guest a synthetic `05 41`/`05 42` reply frame here, on
/// the theory that only a reply frame dismisses the client's WAIT modal. The device
/// refuted the useful half of that: the box opened in the cash shop and NOT in the
/// network warehouse, and closing it left the player on the dead screen anyway. The
/// frame is gone — it could not fix the screen, and it would now actively fight the
/// modal poke (its handler calls the game's `showModal` with a plain dismissible box,
/// which would overwrite modal 5 and take the way out with it).
static DROPPED_NOTICE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// The app went to background/lock. Called from the UI thread (`app::set_paused`).
///
/// Android resets the game's sockets on the way out, so anything still marked
/// `Connected` is suspect from here on — but only suspect, not condemned: a short
/// background may leave the connection intact, and [`AndroidNetwork::check_resume`]
/// asks the socket rather than assuming.
pub fn note_backgrounded() {
    BACKGROUNDED.store(true, Ordering::Relaxed);
}

/// `true` ONCE if the resume probe dropped a connection — the player is owed a message.
/// Polled by the emulator loop, which turns it into an on-screen notice.
pub fn take_drop_notice() -> bool {
    DROPPED_NOTICE.swap(false, Ordering::Relaxed)
}

/// `(polls, absorbed, socket_ns)` since the last call, resetting the counters so the
/// caller reports an interval rate. Read by the 30 s gauge in `emulator.rs`.
pub fn recv_stats_take() -> (u64, u64, u64) {
    (
        RECV_POLLS.swap(0, Ordering::Relaxed),
        RECV_ABSORBED.swap(0, Ordering::Relaxed),
        RECV_SOCKET_NS.swap(0, Ordering::Relaxed),
    )
}

/// How long one outstanding REQUEST may go unanswered before the host declares the
/// connection dead, so the game leaves its loading modal instead of hanging forever.
///
/// ── WHY A TIMER AND NOT JUST BETTER ERROR DETECTION (2026-08-15) ────────────────
/// Player report: background the app for 1-2 minutes, come back, touch anything that
/// talks to the server, and the game loads forever. The server log names the cause —
///
///     17:43:36  RX #22 [00 0a]        heartbeat
///     17:43:44  [s1] connection reset ← the moment the app went to background
///     17:43:44  [s2] connection reset
///
/// Android RESETS both sockets when the app leaves the foreground. It is not the
/// server's idle timeout (that was the 46차 theory, and raising the budget to 20
/// minutes changed nothing — the socket is gone within seconds, long before it).
/// Nothing the server does can help: by the time the player is back there is no
/// connection left to keep alive.
///
/// The host already tries to surface a dead socket (see [`AndroidNetwork::poll_readable`]),
/// and the guest does have a teardown path. But that chain has several links — the
/// error has to be noticed, the read waiter has to be armed, the guest has to call
/// `recv` — and the player is still stuck, so at least one link is not firing after a
/// resume. This timer does not care which: ANY reason the game sits waiting on a
/// server that will not answer ends the same way, on a bound the player can feel.
///
/// ★ Why "one outstanding request" and not "no bytes for 30 s": the guest heartbeats
/// (`00 0a`) every ~5 s and the server DELIBERATELY does not answer those, so a plain
/// silence timer would kill a perfectly healthy idle connection during normal field
/// play. Only a non-heartbeat frame arms this, and any byte from the server disarms it.
/// In the field the guest sends nothing but heartbeats, so the timer is not even armed.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// Bytes withheld from every `recv` so the guest cannot overrun its own receive
/// object. See [`AndroidNetwork::recv`] for the derivation — it is exactly the
/// 8-byte gap between what the guest asks for and what its 1028-byte buffer
/// holds. The largest frame this permits is 1016 bytes, which is also the cap
/// enforced on the server side (`hero3_server/protocol.py::MAX_FRAME`).
const GUEST_RECV_SLACK: usize = 8;

/// Result of polling an in-flight connect.
pub enum ConnectPoll {
    Pending,
    Connected,
    Failed,
}

/// Resolve `host:port` (DNS) and connect to the first address that accepts,
/// spending no more than `budget` in total.
///
/// `TcpStream::connect_timeout` needs an already-resolved `SocketAddr`, but the
/// destination is normally a hostname (so the server can move without an APK
/// rebuild), so resolution happens here. This runs on the connect helper thread,
/// where a blocking DNS lookup is fine — the emulator thread never sees it.
///
/// ★ The budget covers DNS *and* the handshakes. `to_socket_addrs` has no
/// timeout of its own, and on a captive-portal / broken-resolver network it can
/// sit for tens of seconds; charging it against the budget means a slow resolver
/// eats the connect attempt instead of leaving the host dialling long after the
/// game has already given up (which is what made the old fixed 5 s-per-address
/// loop unsafe once the destination became a name rather than `127.0.0.1`).
fn connect_resolved(host: &str, port: u16, budget: Duration) -> std::io::Result<TcpStream> {
    let start = std::time::Instant::now();
    let addrs: Vec<SocketAddr> = (host, port).to_socket_addrs()?.collect();
    if addrs.is_empty() {
        return Err(std::io::Error::new(
            ErrorKind::Other,
            "host resolved to no addresses",
        ));
    }
    let spent = start.elapsed();
    if spent >= budget {
        log::warn!("net: DNS for {host:?} took {spent:?}, over the {budget:?} connect budget");
        return Err(std::io::Error::new(
            ErrorKind::TimedOut,
            "name resolution used the whole connect budget",
        ));
    }
    let per_addr = (budget - spent)
        .checked_div(addrs.len() as u32)
        .unwrap_or(MIN_PER_ADDR_TIMEOUT)
        .max(MIN_PER_ADDR_TIMEOUT);

    let mut last = std::io::Error::new(ErrorKind::Other, "no address accepted");
    for addr in addrs {
        match TcpStream::connect_timeout(&addr, per_addr) {
            Ok(stream) => return Ok(stream),
            Err(e) => last = e,
        }
        // Honour the overall budget even when a name has many records.
        if start.elapsed() >= budget {
            break;
        }
    }
    Err(last)
}

/// An established connection, encrypted or not.
///
/// TLS is per-endpoint (`tls://` in the configured address), because one build has
/// to serve both a public server — where the account id must not be harvestable
/// off the wire — and the plain local `server.py` behind `adb reverse`.
enum Conn {
    Plain(TcpStream),
    Tls(Box<crate::tls::TlsStream>),
}

enum State {
    /// Created by `socket()` but not yet connected.
    Fresh,
    /// A helper thread is running the connect (and, for `tls://`, the handshake).
    Connecting(Receiver<std::io::Result<Conn>>),
    Connected(Conn),
    /// Connect failed or the peer went away. Kept (rather than removed) so the
    /// guest's later `close(fd)` is not a use-after-free of the fd number.
    Dead,
}

pub struct AndroidNetwork {
    sockets: Mutex<HashMap<i32, State>>,
    next_fd: AtomicI32,
    /// fds with an outstanding `MC_netSetReadCB` registration. One entry per
    /// armed socket, cleared when the waiter fires.
    read_armed: Mutex<HashSet<i32>>,
    /// When the host last actually asked the socket and was told "nothing yet",
    /// per fd. Drives [`RECV_EMPTY_REUSE`]. Absent = ask now.
    recv_gate: Mutex<HashMap<i32, Instant>>,
    /// When the guest's oldest unanswered REQUEST went out, per fd. Armed by a
    /// non-heartbeat `send`, cleared by any byte received. Drives [`REQUEST_TIMEOUT`];
    /// absent = the guest is not waiting on anything.
    pending: Mutex<HashMap<i32, Instant>>,
    /// App-private dir. Both the configured server endpoint (`wie_server.txt`) and
    /// the 16-byte account identity (`wie_account_id`) are re-read from here at the
    /// moment they are used — see [`connect`] and [`send`] — so changing either in
    /// the settings takes effect on the game's next login rather than at next
    /// app start.
    data_dir: PathBuf,
}

impl Default for AndroidNetwork {
    fn default() -> Self {
        Self::new(PathBuf::new())
    }
}

impl AndroidNetwork {
    pub fn new(data_dir: PathBuf) -> Self {
        Self {
            sockets: Mutex::new(HashMap::new()),
            // Start at 3 so guest fds never look like stdio handles; the value
            // is opaque to the guest but keeps logs readable.
            next_fd: AtomicI32::new(3),
            read_armed: Mutex::new(HashSet::new()),
            recv_gate: Mutex::new(HashMap::new()),
            pending: Mutex::new(HashMap::new()),
            data_dir,
        }
    }

    /// First network call after the app came back to the foreground: ask every live
    /// socket whether it survived, and drop the ones that did not.
    ///
    /// ── WHY THIS EXISTS ALONGSIDE THE 30 s WATCHDOG (2026-08-15) ──────────────────
    /// Player: "그냥 백그라운드에 내려갔다가 올라와서 캐시샵에서 뭔가하면 어차피 끊기니까
    /// 동작하면 로딩안하고 바로 네트워크가 끊겼습니다 팝업 뜨게 하는게 나을거같아."
    /// Exactly right — after a background the connection is already gone (Android RST),
    /// so making the player watch a 30-second loading bar to be told so is pure waiting.
    /// This turns that case into an immediate answer, and the watchdog goes back to being
    /// what it should be: the backstop for hangs whose cause we do not know.
    ///
    /// ★ It PROBES, it does not assume. A socket is only dropped if the socket itself
    /// says so, so a two-second background that left the connection intact keeps playing.
    /// When Android tore it down the probe gets ECONNRESET and the verdict is immediate;
    /// in the rarer case where the peer went away with no RST reaching us, `peek` still
    /// says WouldBlock, we keep the socket, and the 30 s watchdog is the one that ends it.
    ///
    /// Cheap: one atomic load on the hot `recv` path, and the probe itself runs only on
    /// the single call that follows a resume.
    fn check_resume(&self) {
        if !BACKGROUNDED.swap(false, Ordering::Relaxed) {
            return;
        }
        let mut dropped = Vec::new();
        {
            let mut sockets = self.sockets.lock().unwrap();
            for (fd, state) in sockets.iter_mut() {
                let dead = match state {
                    State::Connected(Conn::Plain(stream)) => {
                        let mut probe = [0u8; 1];
                        match stream.peek(&mut probe) {
                            Ok(0) => true, // clean EOF while we were away
                            Ok(_) => false,
                            Err(e) if e.kind() == ErrorKind::WouldBlock => false,
                            Err(e) if e.kind() == ErrorKind::Interrupted => false,
                            Err(e) => {
                                log::warn!("net: fd {fd} did not survive the background ({e})");
                                true
                            }
                        }
                    }
                    State::Connected(Conn::Tls(stream)) => match stream.readable() {
                        Ok(_) => false,
                        Err(e) => {
                            log::warn!("net: fd {fd} TLS did not survive the background ({e})");
                            true
                        }
                    },
                    _ => false,
                };
                if dead {
                    dropped.push(*fd);
                }
            }
            for fd in &dropped {
                sockets.insert(*fd, State::Dead);
            }
        }
        if dropped.is_empty() {
            log::info!("net: resume — every live socket survived the background");
            return;
        }
        log::warn!("net: resume — dropped {} socket(s) killed while backgrounded: {dropped:?}", dropped.len());
        for fd in dropped {
            self.pending.lock().unwrap().remove(&fd);
            self.recv_gate.lock().unwrap().remove(&fd);
            // Leave `read_armed` ALONE: the waiter is what wakes the guest into the
            // `recv` that returns the error, which is how the guest runs its own
            // teardown (device log, 47차-B: `socket_recv -> -1`, then its
            // `MC_netSocketClose` + `MC_netClose`). Clearing it here would leave the
            // guest parked on a callback that never fires.
        }
        DROPPED_NOTICE.store(true, Ordering::Relaxed);
    }

    /// Has this fd had a request outstanding for longer than [`REQUEST_TIMEOUT`]?
    ///
    /// Also DISARMS when it fires, so the verdict is delivered once per stuck request
    /// rather than on every one of the ~25,000 `recv` polls the guest's wait loop makes
    /// each second.
    fn request_timed_out(&self, fd: i32) -> bool {
        let mut pending = self.pending.lock().unwrap();
        match pending.get(&fd) {
            Some(since) if since.elapsed() >= REQUEST_TIMEOUT => {
                pending.remove(&fd);
                true
            }
            _ => false,
        }
    }

    /// Same question, without consuming the verdict — for [`AndroidNetwork::poll_readable`],
    /// whose job is only to wake the guest so that its `recv` reaches the check above.
    fn request_stuck(&self, fd: i32) -> bool {
        self.pending
            .lock()
            .unwrap()
            .get(&fd)
            .is_some_and(|since| since.elapsed() >= REQUEST_TIMEOUT)
    }

    /// Give up on `fd`: mark it dead so every later `send`/`recv` fails fast, which is
    /// what walks the guest out of its loading modal.
    fn abandon(&self, fd: i32, why: &str) {
        log::warn!("net: fd {fd} {why} — dropping the connection so the game leaves its loading screen");
        self.pending.lock().unwrap().remove(&fd);
        self.recv_gate.lock().unwrap().remove(&fd);
        let mut sockets = self.sockets.lock().unwrap();
        if sockets.contains_key(&fd) {
            sockets.insert(fd, State::Dead);
        }
    }

    /// See [`wie_backend::Network::arm_read`].
    pub fn arm_read(&self, fd: i32) -> bool {
        let newly = self.read_armed.lock().unwrap().insert(fd);
        if newly {
            log::debug!("net: fd {fd} read-callback armed");
        }
        newly
    }

    /// See [`wie_backend::Network::poll_readable`].
    ///
    /// Readability is tested with a non-blocking `peek`, which does not consume
    /// — the guest's own `recv` must still see the bytes. EOF counts as
    /// readable so an armed guest learns the peer is gone (its next `recv`
    /// returns 0 and it tears down) instead of waiting forever.
    ///
    /// ★ A *failed* socket also counts as readable (2026-07-31). The WIPI read
    /// waiter (`wie_wipi_c::api::net`) only fires the guest's callback for verdict
    /// 1; on anything negative it drops the waiter *silently*, which leaves the
    /// guest's receive pump gated off with no event ever coming — the game sits in
    /// "서버 응답 기다리는 중" forever, and its only escape (a 5-minute idle timer)
    /// is reset by touching the screen. Over `adb reverse` / LAN this never fired
    /// because those connections only ever end in a clean FIN, but on a public
    /// network a carrier-NAT reset or a server restart delivers ECONNRESET, so it
    /// would have been the first thing every remote player hit. Reporting 1 makes
    /// the guest call `recv`, which returns a negative and runs its normal
    /// teardown — the same path a clean EOF takes.
    ///
    /// -1 is now reserved for "this fd is gone" (the guest closed it), which is
    /// the one case where firing the callback would be wrong.
    pub fn poll_readable(&self, fd: i32) -> i32 {
        self.check_resume();
        // A request that has gone unanswered past REQUEST_TIMEOUT counts as readable, so
        // a guest parked on its read callback is woken and calls `recv` — which is where
        // the verdict is turned into the error that ends the loading screen. Without this
        // the watchdog could only fire for a guest that polls `recv` on its own.
        if self.request_stuck(fd) {
            self.read_armed.lock().unwrap().remove(&fd);
            return 1;
        }
        let verdict = {
            let mut sockets = self.sockets.lock().unwrap();
            let mut died = false;
            let verdict = match sockets.get_mut(&fd) {
                Some(State::Connected(Conn::Plain(stream))) => {
                    let mut probe = [0u8; 1];
                    match stream.peek(&mut probe) {
                        Ok(0) => 1,  // EOF — readable in the sense that matters
                        Ok(_) => 1,  // data waiting
                        Err(e) if e.kind() == ErrorKind::WouldBlock => 0,
                        Err(e) if e.kind() == ErrorKind::Interrupted => 0,
                        Err(e) => {
                            // Reset/aborted by the peer or the network. Mark it
                            // dead so the guest's recv is guaranteed to fail
                            // fast, and report readable so that recv happens.
                            log::warn!("net: fd {fd} peek failed ({e}) — reporting readable so the guest tears down");
                            died = true;
                            1
                        }
                    }
                }
                // TLS cannot be peeked — a record must be decrypted to know
                // whether it carried payload — so the stream owns a plaintext
                // buffer and "readable" means that buffer is non-empty. See
                // `tls::TlsStream`.
                Some(State::Connected(Conn::Tls(stream))) => match stream.readable() {
                    Ok(true) => 1,
                    Ok(false) => 0,
                    Err(e) => {
                        log::warn!("net: fd {fd} TLS read failed ({e}) — reporting readable so the guest tears down");
                        died = true;
                        1
                    }
                },
                // Connect failed / peer gone: let the guest find out via recv.
                Some(State::Dead) => 1,
                // Fresh or still connecting: nothing to read yet, but healthy.
                Some(_) => 0,
                None => -1,
            };
            if died {
                sockets.insert(fd, State::Dead);
            }
            verdict
        };

        if verdict != 0 {
            self.read_armed.lock().unwrap().remove(&fd);
        }
        verdict
    }

    /// WIPI slot 2. `af`/`ty` are accepted for the trace but only AF_INET +
    /// SOCK_STREAM is meaningful here — Hero3 only ever asks for (2, 1).
    pub fn socket(&self, af: i32, ty: i32) -> i32 {
        if af != 2 || ty != 1 {
            log::warn!("net: socket({af}, {ty}) — only AF_INET/SOCK_STREAM is supported");
            return ERROR;
        }
        let fd = self.next_fd.fetch_add(1, Ordering::Relaxed);
        self.sockets.lock().unwrap().insert(fd, State::Fresh);
        log::info!("net: socket() -> fd {fd}");
        fd
    }

    /// WIPI slot 30. Starts a connect; completion is observed via
    /// [`poll_connect`]. `addr` arrives as the game's own `inet_addr` output,
    /// i.e. the four octets in memory order, and `port` is already htons'd.
    pub fn connect(&self, fd: i32, addr: u32, port_be: u16, name: &str) -> i32 {
        let octets = addr.to_le_bytes();
        let requested = SocketAddr::new(
            IpAddr::V4(Ipv4Addr::new(octets[0], octets[1], octets[2], octets[3])),
            u16::from_be(port_be),
        );

        let (host, port, tls) = if REDIRECT_ENABLED {
            // Re-read per connect: the player may have changed the address in the
            // settings since the app started, and the game opens its socket fresh
            // for every login.
            let ep = crate::config::read_server_endpoint(&self.data_dir);
            log::info!(
                "net: connect(fd {fd}, {name:?}) {requested} -> REDIRECTED to {}{}:{}",
                if ep.tls { "tls://" } else { "" },
                ep.host,
                ep.port
            );
            (ep.host, ep.port, ep.tls)
        } else {
            log::info!("net: connect(fd {fd}, {name:?}) {requested}");
            (requested.ip().to_string(), requested.port(), false)
        };
        // Read the pin here, on the emulator thread, rather than inside the helper
        // closure: it keeps all configuration reads in one place per connect.
        let pin = if tls { crate::config::read_server_pin(&self.data_dir) } else { None };

        let mut sockets = self.sockets.lock().unwrap();
        match sockets.get(&fd) {
            Some(State::Fresh) => {}
            Some(_) => {
                log::warn!("net: connect on fd {fd} which is not fresh");
                return ERROR;
            }
            None => {
                log::warn!("net: connect on unknown fd {fd}");
                return ERROR;
            }
        }

        // TcpStream::connect blocks, so it runs on a helper thread and the
        // emulator polls the channel. One thread per connect is fine: Hero3
        // opens a single socket at a time and closes it on teardown.
        //
        // ★ The TLS handshake happens here too, still on the BLOCKING socket. It
        // is several round trips; doing it lazily on the emulator thread would
        // either park the interpreter or need the handshake re-entered from
        // `recv`. Getting it out of the way before the guest sees a connected
        // socket keeps `send`/`recv` exactly as simple as the plain path.
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            let result = connect_resolved(&host, port, CONNECT_BUDGET).and_then(|stream| {
                if !tls {
                    // Non-blocking from here on; recv must return WouldBlock
                    // rather than parking the emulator thread.
                    let _ = stream.set_nonblocking(true);
                    let _ = stream.set_nodelay(true);
                    return Ok(Conn::Plain(stream));
                }
                let trust = match pin {
                    Some(pin) => crate::tls::Trust::Pinned(pin),
                    None => crate::tls::Trust::WebPki,
                };
                crate::tls::TlsStream::handshake(stream, &host, trust)
                    .and_then(|s| s.into_nonblocking())
                    .map(|s| Conn::Tls(Box::new(s)))
            });
            let _ = tx.send(result);
        });

        sockets.insert(fd, State::Connecting(rx));
        0
    }

    /// Poll an in-flight connect. Called from the WIPI layer's completion task.
    pub fn poll_connect(&self, fd: i32) -> ConnectPoll {
        let mut sockets = self.sockets.lock().unwrap();
        let Some(state) = sockets.get_mut(&fd) else {
            return ConnectPoll::Failed;
        };
        match state {
            State::Connecting(rx) => match rx.try_recv() {
                Ok(Ok(conn)) => {
                    log::info!(
                        "net: fd {fd} connected ({})",
                        match &conn {
                            Conn::Plain(_) => "plain",
                            Conn::Tls(_) => "TLS",
                        }
                    );
                    *state = State::Connected(conn);
                    ConnectPoll::Connected
                }
                Ok(Err(e)) => {
                    log::warn!("net: fd {fd} connect failed: {e}");
                    *state = State::Dead;
                    ConnectPoll::Failed
                }
                Err(TryRecvError::Empty) => ConnectPoll::Pending,
                Err(TryRecvError::Disconnected) => {
                    log::warn!("net: fd {fd} connect thread vanished");
                    *state = State::Dead;
                    ConnectPoll::Failed
                }
            },
            State::Connected(_) => ConnectPoll::Connected,
            _ => ConnectPoll::Failed,
        }
    }

    /// WIPI slot 31. Returns bytes accepted, or a negative error.
    ///
    /// A partial write is reported honestly: the guest's sender treats the
    /// return as "bytes taken", and claiming more than we wrote would silently
    /// truncate a request.
    pub fn send(&self, fd: i32, data: &[u8]) -> i32 {
        // ★ Identity injection: the vanilla client hardcodes the 16-byte account
        // id ("0123456789") in its `01 3d` login packet, so every user would be
        // the same server account. Rewrite it with this device's persistent id.
        // Frame = [u32 LE len][01 3d][16B phone]...; the phone field is at frame
        // offset 6..22 (payload offset 2). Only the one login packet matches
        // `01 3d`; heartbeats/other packets pass through untouched.
        let injected;
        let data: &[u8] = if data.len() >= 22 && data[4] == 0x01 && data[5] == 0x3d {
            let mut v = data.to_vec();
            // Read the identity HERE rather than caching it at construction, so
            // "account code" changes made in the settings apply from the next login
            // instead of the next app start — the same freshness the endpoint has,
            // and what the dialog promises. This is one file read per login packet.
            let id = crate::config::read_or_create_account_id(&self.data_dir);
            v[6..22].copy_from_slice(&id);
            log::info!("net: injected 16B device account id into 01 3d login");
            injected = v;
            &injected
        } else {
            data
        };

        // ★ PROMOTE A FINISHED-BUT-UNCLAIMED CONNECT FIRST (2026-08-11).
        //
        // The guest can send on a socket whose completion callback has not run yet, and
        // on LGT it reliably does: the game opens TWO sockets back to back (fd 3, fd 4),
        // both handshakes finish, fd 3's completion task fires its guest callback, and the
        // guest immediately writes its login packet on fd 4 — before fd 4's own task has
        // reached its first 50 ms poll. The socket was connected; only the bookkeeping was
        // late. Without this the device log read
        //     tls: handshake complete ("TLSv1_3")   x2
        //     net: fd 3 connected (TLS)
        //     net: send on fd 4 which is not connected
        //     slot31 socket_send(fd=4, len=7) -> -1
        // and the game sat on "서버 응답 기다리는중" for ever (MIGRATION_LGT.md §2.18).
        //
        // `poll_connect` takes the lock itself, so this must happen BEFORE we take it.
        // It is a `try_recv`, so a genuinely still-connecting socket stays pending and
        // this stays a no-op.
        let _ = self.poll_connect(fd);
        self.check_resume();
        let mut sockets = self.sockets.lock().unwrap();
        let Some(State::Connected(conn)) = sockets.get_mut(&fd) else {
            log::warn!("net: send on fd {fd} which is not connected");
            return ERROR;
        };
        let result = match conn {
            Conn::Plain(stream) => stream.write(data),
            Conn::Tls(stream) => stream.write_plain(data),
        };
        match result {
            Ok(n) => {
                log::info!("net: send fd {fd} {n}/{} bytes{}", data.len(), frame_tag(data));
                // Arm the stuck-request timer. `cat == 0x00` is the keep-alive, which the
                // server deliberately leaves unanswered — arming on it would make the
                // watchdog fire during ordinary idle play (see REQUEST_TIMEOUT). Only the
                // OLDEST unanswered request is tracked, so a burst of requests does not
                // keep pushing the deadline out.
                if data.len() >= 6 && data[4] != 0x00 {
                    self.pending.lock().unwrap().entry(fd).or_insert_with(Instant::now);
                }
                n as i32
            }
            Err(e) if e.kind() == ErrorKind::WouldBlock => WOULDBLOCK,
            Err(e) => {
                log::warn!("net: send fd {fd} failed: {e}");
                ERROR
            }
        }
    }

    /// WIPI slot 32. `n > 0` = data, [`WOULDBLOCK`] = nothing yet (socket still
    /// healthy), `0` = peer closed cleanly, negative = error.
    ///
    /// The 0-vs-WOULDBLOCK distinction is load-bearing: returning 0 for "no
    /// data" makes the game tear down a perfectly good connection.
    ///
    /// ★ The guest over-asks by 8 bytes and we must not take it at its word.
    /// Its receive object is one 1028-byte allocation (client.bin image+0x7e0fc
    /// `ldr r0, =0x404`) whose wire bytes start at obj+0xc, yet the pump calls
    /// `recv(fd, obj + 12 + used, 1024 - used)`. Satisfying that request in full
    /// writes through obj+1035 — eight bytes past the object. Since
    /// `used = 1024 - max`, the space actually left is
    /// `1028 - (12 + used) = max - 8`, so delivery is clamped to that. This is a
    /// latent bug in the original game; it never fired against the original
    /// server but is trivially reachable from a private one.
    ///
    /// ★ THROTTLED (2026-08-13, `MIGRATION_LGT.md` §2.24). While the game waits on a
    /// server reply — which on LGT is every warehouse page turn, behind the server's
    /// message box — its wait loop calls this ~25,000 times a second. Each call used to
    /// reach the socket, so the emulator thread spent about 40 % of its wall clock in
    /// `read`-returns-EAGAIN instead of interpreting, and the guest fell from ~34 fps to
    /// 5. The answer "nothing yet" is now reused for [`RECV_EMPTY_REUSE`]; see there for
    /// why that cannot delay a message that has already arrived.
    pub fn recv(&self, fd: i32, buf: &mut [u8]) -> i32 {
        RECV_POLLS.fetch_add(1, Ordering::Relaxed);
        let safe_len = buf.len().saturating_sub(GUEST_RECV_SLACK);
        if safe_len == 0 {
            // The guest left itself no usable room; nothing can be delivered
            // safely, but the socket is fine, so report "not yet".
            return WOULDBLOCK;
        }
        let buf = &mut buf[..safe_len];

        // Same late-bookkeeping race as `send` — see the note there. The guest polls
        // `recv` on a socket whose completion callback has not run yet, and answering
        // ERROR there makes it give up on a connection that is actually up.
        //
        // Cheap when there is nothing to promote (a `try_recv` on an empty channel),
        // and it must stay ahead of the throttle: a socket that finished connecting
        // during the reuse window has to be picked up on this very call.
        let _ = self.poll_connect(fd);
        self.check_resume();
        // ★ The stuck-request watchdog. Checked BEFORE the reuse gate below, because that
        // gate answers most polls without ever reaching the socket — a verdict placed
        // after it would only be consulted every RECV_EMPTY_REUSE and, more importantly,
        // never at all once the socket stops producing events.
        if self.request_timed_out(fd) {
            self.abandon(fd, &format!("no answer for {}s", REQUEST_TIMEOUT.as_secs()));
            return ERROR;
        }
        let mut sockets = self.sockets.lock().unwrap();
        let Some(State::Connected(conn)) = sockets.get_mut(&fd) else {
            log::warn!("net: recv on fd {fd} which is not connected");
            return ERROR;
        };
        // Decrypted-but-uncollected bytes bypass the gate entirely; only an EMPTY
        // buffer may be answered from the timestamp. (A plain socket has no buffer of
        // its own, so for it the gate is the whole test.)
        let buffered = match conn {
            Conn::Plain(_) => false,
            Conn::Tls(stream) => stream.buffered(),
        };
        if !buffered
            && let Some(last) = self.recv_gate.lock().unwrap().get(&fd).copied()
            && last.elapsed() < RECV_EMPTY_REUSE
        {
            RECV_ABSORBED.fetch_add(1, Ordering::Relaxed);
            return WOULDBLOCK;
        }

        let started = Instant::now();
        let result = match conn {
            Conn::Plain(stream) => stream.read(buf),
            Conn::Tls(stream) => stream.read_plain(buf),
        };
        RECV_SOCKET_NS.fetch_add(started.elapsed().as_nanos() as u64, Ordering::Relaxed);
        // Stamp AFTER the attempt and only when it came up empty: a transfer in
        // progress must not be made to wait out the window between chunks, and a
        // stamp written before the read would do exactly that.
        {
            let mut gate = self.recv_gate.lock().unwrap();
            match &result {
                Err(e) if e.kind() == ErrorKind::WouldBlock => {
                    gate.insert(fd, Instant::now());
                }
                _ => {
                    gate.remove(&fd);
                }
            }
        }
        match result {
            // A 0-length read on a blocking-style API means EOF; pass it
            // through as 0 so the guest tears down, which is correct here.
            Ok(0) => {
                log::info!("net: recv fd {fd} EOF (peer closed)");
                0
            }
            Ok(n) => {
                log::info!("net: recv fd {fd} {n} bytes{}", frame_tag(&buf[..n]));
                // The server answered, so nothing is stuck. Disarming on ANY byte (rather
                // than trying to match replies to requests) is deliberate: the protocol
                // has no request ids, and a server that is talking to us at all is not
                // the failure this watchdog exists for.
                self.pending.lock().unwrap().remove(&fd);
                n as i32
            }
            Err(e) if e.kind() == ErrorKind::WouldBlock => WOULDBLOCK,
            Err(e) if e.kind() == ErrorKind::Interrupted => WOULDBLOCK,
            Err(e) => {
                log::warn!("net: recv fd {fd} failed: {e}");
                ERROR
            }
        }
    }

    /// WIPI slot 33.
    pub fn close(&self, fd: i32) {
        self.read_armed.lock().unwrap().remove(&fd);
        self.recv_gate.lock().unwrap().remove(&fd);
        self.pending.lock().unwrap().remove(&fd);
        if self.sockets.lock().unwrap().remove(&fd).is_some() {
            log::info!("net: close fd {fd}");
        }
    }

    /// WIPI slot 1 (`MC_netClose`) — drop every socket.
    pub fn close_all(&self) {
        self.read_armed.lock().unwrap().clear();
        self.recv_gate.lock().unwrap().clear();
        self.pending.lock().unwrap().clear();
        let n = self.sockets.lock().unwrap().drain().count();
        if n > 0 {
            log::info!("net: close_all dropped {n} socket(s)");
        }
    }
}

/// The frame's `cat`/`op` header as a log fragment, or empty when the buffer is not
/// even a whole header.
///
/// ★ HEADER ONLY, never the payload (player report 2026-08-06: "#으로 초기화하고 #으로
/// 다시 복구 선택하면 창고로 접속이 되고있어"). That symptom is a conversation between
/// the guest client and the private server, and the host is the only place that sees
/// both halves of it — but the payload carries the 16-byte account id and the 219 B
/// character save, so logging it would put both in logcat, where anything on the phone
/// can read them. `cat`/`op` alone splits the two candidate causes: a `04 3e` sent with
/// no `04 3e` coming back means the DEPLOYED server does not have the RESTORE reply
/// (the code and its selftest here do), while `04 3e` in both directions moves the bug
/// to how the client handles the reply.
///
/// Frame = `[u32 LE len][cat][op]…`, the same layout the login injection above relies on.
fn frame_tag(data: &[u8]) -> String {
    if data.len() < 6 {
        return String::new();
    }
    format!(" cat={:02x} op={:02x}", data[4], data[5])
}

impl wie_backend::Network for AndroidNetwork {
    fn socket(&self, af: i32, ty: i32) -> i32 {
        AndroidNetwork::socket(self, af, ty)
    }

    fn connect(&self, fd: i32, addr: u32, port_be: u16, name: &str) -> i32 {
        AndroidNetwork::connect(self, fd, addr, port_be, name)
    }

    fn poll_connect(&self, fd: i32) -> i32 {
        match AndroidNetwork::poll_connect(self, fd) {
            ConnectPoll::Pending => 0,
            ConnectPoll::Connected => 1,
            ConnectPoll::Failed => -1,
        }
    }

    fn send(&self, fd: i32, data: &[u8]) -> i32 {
        AndroidNetwork::send(self, fd, data)
    }

    fn recv(&self, fd: i32, buf: &mut [u8]) -> i32 {
        AndroidNetwork::recv(self, fd, buf)
    }

    fn arm_read(&self, fd: i32) -> bool {
        AndroidNetwork::arm_read(self, fd)
    }

    fn poll_readable(&self, fd: i32) -> i32 {
        AndroidNetwork::poll_readable(self, fd)
    }

    fn close(&self, fd: i32) {
        AndroidNetwork::close(self, fd)
    }

    fn close_all(&self) {
        AndroidNetwork::close_all(self)
    }
}
