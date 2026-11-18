//! TLS for the private-server connection (Track C, 2026-07-31).
//!
//! WHY
//! ---
//! The 16-byte account id the game sends in its `01 3d` login is a BEARER
//! credential: the server keys every warehouse, balance and character backup on
//! whatever 16 bytes arrive, so knowing them IS being that account
//! (`hero3_server/threat_demo.py` demonstrates a full takeover from a 20-line
//! socket script). Per-device ids separate users; they do not authenticate them.
//! With guessing and leakage closed off, the one remaining way to learn someone
//! else's id is to watch the wire — so the wire has to be encrypted.
//!
//! The original WIPI binary knows nothing about TLS. We can wrap it anyway
//! because OUR layer owns the socket: the guest calls slots 30/31/32/33 and never
//! sees bytes we did not hand it.
//!
//! SHAPE
//! -----
//! Two things make this more than "swap in a TLS stream":
//!
//!   1. **The handshake blocks.** It runs on the connect helper thread, on a
//!      still-blocking socket, and only then is the socket switched to
//!      non-blocking — the emulator thread must never park inside a WIPI call.
//!   2. **The guest needs `peek`.** `poll_readable` asks "is there data?" without
//!      consuming it, which TLS cannot answer (a record has to be decrypted to
//!      know if it held payload). So this type owns a plaintext buffer: the pump
//!      decrypts whatever has arrived into it, "readable" means the buffer is
//!      non-empty, and `recv` drains it. The plaintext is therefore buffered
//!      exactly once, here, instead of the guest's `recv` racing the record layer.

use std::{
    io::{ErrorKind, Read, Write},
    net::TcpStream,
    sync::Arc,
};

use rustls::{
    ClientConfig, ClientConnection, RootCertStore,
    client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier},
    pki_types::{CertificateDer, ServerName, UnixTime},
};

/// How the server's certificate is checked.
pub enum Trust {
    /// Validate against the bundled Mozilla root store — for a normal certificate
    /// (Let's Encrypt et al). Requires the endpoint to be a hostname, since that
    /// is what the certificate attests to.
    WebPki,
    /// Accept exactly one certificate, by SHA-256 of its DER. For a self-signed
    /// server; also the only option when connecting to a bare IP.
    Pinned([u8; 32]),
}

/// A TLS connection plus the plaintext the guest has not collected yet.
pub struct TlsStream {
    sock: TcpStream,
    conn: ClientConnection,
    /// Decrypted bytes waiting for the guest's `recv`. See the module docs — this
    /// exists so `poll_readable` can answer without consuming.
    plain: Vec<u8>,
    /// The peer sent `close_notify` (or the socket hit EOF) and `plain` is drained.
    eof: bool,
}

impl TlsStream {
    /// Connect and complete the handshake. BLOCKING: call this on the connect
    /// helper thread only. The socket is left blocking; [`into_nonblocking`] flips
    /// it once the caller is ready to poll.
    pub fn handshake(sock: TcpStream, host: &str, trust: Trust) -> std::io::Result<Self> {
        let config = build_config(trust, host)?;
        // rustls wants the name the certificate should attest to. For a pinned
        // connection to an IP there is nothing to attest, but a ServerName is
        // still required — the verifier ignores it in that case.
        let server_name = match ServerName::try_from(host.to_string()) {
            Ok(name) => name,
            Err(_) => ServerName::try_from("invalid.pinned").map_err(|e| {
                std::io::Error::new(ErrorKind::InvalidInput, format!("server name: {e}"))
            })?,
        };
        let mut conn = ClientConnection::new(Arc::new(config), server_name)
            .map_err(|e| std::io::Error::new(ErrorKind::Other, format!("TLS setup: {e}")))?;

        let mut sock = sock;
        // Drive the handshake to completion on the blocking socket.
        while conn.is_handshaking() {
            let (rd, wr) = conn
                .complete_io(&mut sock)
                .map_err(|e| std::io::Error::new(e.kind(), format!("TLS handshake: {e}")))?;
            if rd == 0 && wr == 0 && conn.is_handshaking() {
                return Err(std::io::Error::new(
                    ErrorKind::UnexpectedEof,
                    "peer closed during the TLS handshake",
                ));
            }
        }
        log::info!(
            "tls: handshake complete ({:?})",
            conn.protocol_version().map(|v| format!("{v:?}")).unwrap_or_default()
        );
        Ok(Self { sock, conn, plain: Vec::new(), eof: false })
    }

    /// Switch to non-blocking. Called once, right before the emulator starts
    /// polling this connection.
    pub fn into_nonblocking(self) -> std::io::Result<Self> {
        self.sock.set_nonblocking(true)?;
        let _ = self.sock.set_nodelay(true);
        Ok(self)
    }

    /// Move whatever has arrived into the plaintext buffer.
    ///
    /// `Ok(())` = made progress or nothing to do; the socket is still healthy.
    /// An error here means the connection is finished (bad record, reset, …).
    fn pump(&mut self) -> std::io::Result<()> {
        loop {
            if !self.conn.wants_read() {
                break;
            }
            match self.conn.read_tls(&mut self.sock) {
                Ok(0) => {
                    // Socket EOF. Anything already decrypted is still deliverable;
                    // `eof` only takes effect once `plain` is drained.
                    self.eof = true;
                    break;
                }
                Ok(_) => {
                    let state = self
                        .conn
                        .process_new_packets()
                        .map_err(|e| std::io::Error::new(ErrorKind::InvalidData, format!("TLS: {e}")))?;
                    if state.plaintext_bytes_to_read() > 0 {
                        let mut buf = vec![0u8; state.plaintext_bytes_to_read()];
                        self.conn.reader().read_exact(&mut buf)?;
                        self.plain.extend_from_slice(&buf);
                    }
                    if state.peer_has_closed() {
                        self.eof = true;
                        break;
                    }
                }
                Err(e) if e.kind() == ErrorKind::WouldBlock => break,
                Err(e) if e.kind() == ErrorKind::Interrupted => continue,
                Err(e) => return Err(e),
            }
        }
        // Flush any handshake/alert/ack bytes rustls wants to send.
        while self.conn.wants_write() {
            match self.conn.write_tls(&mut self.sock) {
                Ok(0) => break,
                Ok(_) => {}
                Err(e) if e.kind() == ErrorKind::WouldBlock => break,
                Err(e) if e.kind() == ErrorKind::Interrupted => continue,
                Err(e) => return Err(e),
            }
        }
        Ok(())
    }

    /// Is there already-decrypted plaintext (or a known end of stream) to hand out
    /// WITHOUT touching the socket?
    ///
    /// This is the question the recv throttle asks (`network.rs`): the guest polls
    /// `recv` tens of thousands of times a second while it waits on a server reply,
    /// and each poll used to reach [`pump`] and therefore a `read` syscall. Answering
    /// from the buffer costs nothing and — importantly — is not a *stale* answer: any
    /// byte already decrypted is visible here immediately.
    pub fn buffered(&self) -> bool {
        !self.plain.is_empty() || self.eof
    }

    /// `Ok(true)` = plaintext is available (or the peer is gone, which the guest
    /// also has to learn about); `Ok(false)` = healthy but nothing yet.
    pub fn readable(&mut self) -> std::io::Result<bool> {
        self.pump()?;
        Ok(!self.plain.is_empty() || self.eof)
    }

    /// Copy out at most `buf.len()` decrypted bytes.
    ///
    /// Returns `Ok(0)` ONLY for a real end of stream — the caller maps that to the
    /// guest's "peer gone". "Nothing yet" is `WouldBlock`, which the guest layer
    /// turns into its -19, because conflating the two makes the game tear down a
    /// healthy connection.
    pub fn read_plain(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.pump()?;
        // The buffer bookkeeping lives in `tls_buf` so it can be unit-tested
        // without rustls (this host cannot build ring for the host target).
        crate::tls_buf::drain_plain(&mut self.plain, self.eof, buf)
    }

    /// Encrypt and send. Returns the number of PLAINTEXT bytes accepted.
    ///
    /// rustls buffers into the connection, so a short write on the socket does not
    /// lose data — but it must still be reported honestly, since the guest's
    /// sender treats the return as "bytes taken".
    pub fn write_plain(&mut self, data: &[u8]) -> std::io::Result<usize> {
        let n = self.conn.writer().write(data)?;
        // Push it out now; whatever does not fit stays queued and goes on the next
        // pump (the guest polls continuously).
        while self.conn.wants_write() {
            match self.conn.write_tls(&mut self.sock) {
                Ok(0) => break,
                Ok(_) => {}
                Err(e) if e.kind() == ErrorKind::WouldBlock => break,
                Err(e) if e.kind() == ErrorKind::Interrupted => continue,
                Err(e) => return Err(e),
            }
        }
        Ok(n)
    }
}

fn build_config(trust: Trust, host: &str) -> std::io::Result<ClientConfig> {
    // The provider is named explicitly: rustls has no process-wide default
    // installed here, and relying on one would make this depend on link order.
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let builder = ClientConfig::builder_with_provider(provider.clone())
        .with_safe_default_protocol_versions()
        .map_err(|e| std::io::Error::new(ErrorKind::Other, format!("TLS versions: {e}")))?;

    let config = match trust {
        Trust::WebPki => {
            let roots = RootCertStore {
                roots: webpki_roots::TLS_SERVER_ROOTS.to_vec(),
            };
            log::info!("tls: validating {host:?} against the bundled root store");
            builder.with_root_certificates(roots).with_no_client_auth()
        }
        Trust::Pinned(pin) => {
            log::info!("tls: validating {host:?} against a pinned certificate");
            builder
                .dangerous()
                .with_custom_certificate_verifier(Arc::new(PinnedVerifier { pin, provider }))
                .with_no_client_auth()
        }
    };
    Ok(config)
}

/// Trusts exactly one certificate, identified by SHA-256 of its DER encoding.
///
/// This is `dangerous()` in rustls terms because it bypasses the PKI, but for a
/// self-signed private server it is STRICTER than CA validation: it accepts one
/// specific key rather than anything a public CA will sign for the name. What it
/// deliberately does not check is expiry or the name in the certificate — the pin
/// is the whole identity, so a long-lived self-signed certificate needs no
/// renewal dance.
#[derive(Debug)]
struct PinnedVerifier {
    pin: [u8; 32],
    provider: Arc<rustls::crypto::CryptoProvider>,
}

impl ServerCertVerifier for PinnedVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        let digest = ring::digest::digest(&ring::digest::SHA256, end_entity.as_ref());
        // Constant-time-ish compare. The pin is not secret, so this is belt and
        // braces rather than a requirement.
        let equal = digest.as_ref().len() == self.pin.len()
            && digest
                .as_ref()
                .iter()
                .zip(self.pin.iter())
                .fold(0u8, |acc, (a, b)| acc | (a ^ b))
                == 0;
        if equal {
            Ok(ServerCertVerified::assertion())
        } else {
            log::error!(
                "tls: certificate does NOT match the pin (got {})",
                hex32(digest.as_ref())
            );
            Err(rustls::Error::General(
                "server certificate does not match the configured pin".into(),
            ))
        }
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        self.provider.signature_verification_algorithms.supported_schemes()
    }
}

fn hex32(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
