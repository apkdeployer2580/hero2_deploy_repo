//! Host-initiated link to the private server, used to upload a recovery backup
//! without the player walking the game's network menus.
//!
//! ── WHY A SECOND CONNECTION ──────────────────────────────────────────────────
//! The guest's own socket (`network.rs`) belongs to the guest: it is opened, driven
//! and torn down by game code, and it only exists while the player is inside the
//! network UI. The backup has to happen at moments the game is not online at all
//! (right after a save), so the host opens its own short-lived connection, speaks the
//! three frames it needs, and closes. The server cannot tell the difference — the
//! frames are byte-identical to the client's (`backup.rs`).
//!
//! BLOCKING, on its own thread. Never call this from the emulator worker: a DNS
//! lookup plus a TLS handshake is hundreds of milliseconds and the worker owns a
//! 24 ms frame budget. [`upload_backup`] spawns and returns immediately.

use std::{
    io::{ErrorKind, Read, Write},
    net::TcpStream,
    path::PathBuf,
    time::Duration,
};

use crate::{backup, config, tls};

/// How long the whole exchange may take before we give up. Generous — this runs on a
/// phone radio — but bounded, because a thread parked forever on a dead socket is a
/// leak that accumulates one per save.
const IO_TIMEOUT: Duration = Duration::from_secs(20);

/// Upload `body` as the recovery backup for `slot`, on a background thread.
///
/// Fire-and-forget by design: a failed backup must never disturb the game. Every
/// outcome is logged, and the caller is not told, because there is nothing it could
/// usefully do differently — the next save will try again.
pub fn upload_backup(data_dir: PathBuf, slot: u8, body: Vec<u8>) {
    let name = format!("wie-backup-{slot}");
    let spawned = std::thread::Builder::new()
        .name(name)
        .spawn(move || match exchange(&data_dir, slot, &body) {
            Ok(()) => log::info!("backup: slot {slot} uploaded ({} bytes)", body.len()),
            Err(e) => log::warn!("backup: slot {slot} upload failed: {e}"),
        });
    if let Err(e) = spawned {
        log::warn!("backup: could not spawn the upload thread: {e}");
    }
}

fn exchange(data_dir: &std::path::Path, slot: u8, body: &[u8]) -> std::io::Result<()> {
    let Some(frame) = backup::frame_backup(slot, body) else {
        return Err(std::io::Error::new(ErrorKind::InvalidInput, "backup body/slot rejected"));
    };
    let ep = config::read_server_endpoint(data_dir);
    let account = config::read_or_create_account_id(data_dir);

    let sock = TcpStream::connect_timeout(&resolve(&ep.host, ep.port)?, IO_TIMEOUT)?;
    sock.set_read_timeout(Some(IO_TIMEOUT))?;
    sock.set_write_timeout(Some(IO_TIMEOUT))?;
    let _ = sock.set_nodelay(true);

    // Same trust decision the guest's own connect makes (`network.rs`): a configured
    // certificate pin trusts exactly that certificate, otherwise the public CA set.
    let mut link: Box<dyn Duplex> = if ep.tls {
        let trust = match config::read_server_pin(data_dir) {
            Some(pin) => tls::Trust::Pinned(pin),
            None => tls::Trust::WebPki,
        };
        Box::new(tls::TlsStream::handshake(sock, &ep.host, trust)?)
    } else {
        Box::new(sock)
    };

    // The server's login state machine wants these in order, and answers each one.
    // We WAIT for the answers rather than firing all three: `01 3d` is what binds the
    // connection to this account, and uploading before it lands would be refused
    // (`_require_account`) — silently, which is the worst kind of failure.
    link.write_all(&backup::frame_hello())?;
    await_reply(&mut *link, (0x01, 0x01))?;
    link.write_all(&backup::frame_login(&account))?;
    await_reply(&mut *link, (0x01, 0x3d))?;
    link.write_all(&frame)?;
    // The server acks a stored backup with `04 3d`. Treat a missing ack as failure so
    // the log tells the truth about whether the character is actually safe.
    await_reply(&mut *link, (0x04, 0x3d))
}

/// Read until a frame with this `(cat, op)` arrives.
fn await_reply(link: &mut dyn Duplex, want: (u8, u8)) -> std::io::Result<()> {
    let mut acc = Vec::new();
    let mut buf = [0u8; 512];
    loop {
        let n = link.read(&mut buf)?;
        if n == 0 {
            return Err(std::io::Error::new(
                ErrorKind::UnexpectedEof,
                format!("peer closed while waiting for {:02x} {:02x}", want.0, want.1),
            ));
        }
        acc.extend_from_slice(&buf[..n]);
        if backup::frames_in(&acc).contains(&want) {
            return Ok(());
        }
        if acc.len() > 64 * 1024 {
            return Err(std::io::Error::new(ErrorKind::InvalidData, "reply stream ran away"));
        }
    }
}

fn resolve(host: &str, port: u16) -> std::io::Result<std::net::SocketAddr> {
    use std::net::ToSocketAddrs;
    (host, port)
        .to_socket_addrs()?
        .next()
        .ok_or_else(|| std::io::Error::new(ErrorKind::NotFound, format!("no address for {host}")))
}

/// The two transports look the same to this module.
trait Duplex: Send {
    fn write_all(&mut self, data: &[u8]) -> std::io::Result<()>;
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize>;
}

impl Duplex for TcpStream {
    fn write_all(&mut self, data: &[u8]) -> std::io::Result<()> {
        Write::write_all(self, data)
    }
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        Read::read(self, buf)
    }
}

impl Duplex for tls::TlsStream {
    fn write_all(&mut self, data: &[u8]) -> std::io::Result<()> {
        let mut sent = 0;
        while sent < data.len() {
            match self.write_plain(&data[sent..]) {
                Ok(0) => return Err(std::io::Error::new(ErrorKind::WriteZero, "TLS write stalled")),
                Ok(n) => sent += n,
                Err(e) if e.kind() == ErrorKind::WouldBlock => continue,
                Err(e) => return Err(e),
            }
        }
        Ok(())
    }
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        loop {
            match self.read_plain(buf) {
                Err(e) if e.kind() == ErrorKind::WouldBlock => continue,
                other => return other,
            }
        }
    }
}
