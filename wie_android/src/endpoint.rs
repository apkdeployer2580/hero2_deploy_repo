// Parsing for the private-server endpoint line (`wie_server.txt`).
//
// Split out from `config.rs` deliberately: it has NO dependencies (no `log`, no
// `std::fs`, nothing android-specific), so `endpoint_selftest.rs` can `include!`
// this exact file and test the real parser rather than a copy of it.
// (`cargo test -p wie_android` crashes at startup on this Windows host because of
// the linked winit/android deps — see `items_selftest.rs`.)
//
// ★ Plain `//` comments, not `//!`: an inner doc comment cannot appear inside an
// `include!`, and the module doc therefore lives on the `mod endpoint;` item in
// `lib.rs`. Keep it that way or the selftest stops compiling.

/// Default port for the private server (`hero3_server/server.py`).
pub const DEFAULT_SERVER_PORT: u16 = 8091;

/// Where to reach the private server, and whether to encrypt getting there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Endpoint {
    /// Hostname or IP literal, verbatim — DNS happens at connect time.
    pub host: String,
    pub port: u16,
    /// `true` when the line said `tls://`.
    ///
    /// Opt-in per address rather than a global default, because the two
    /// deployments have opposite needs and both must keep working from one build:
    /// a public server MUST be `tls://` (the 16-byte account id is a bearer
    /// credential — anyone who sees it on the wire owns the account), while the
    /// `adb reverse` protocol-learning loop talks to a plain local `server.py`
    /// over a USB tunnel that never leaves the device.
    pub tls: bool,
}

/// Parse one endpoint line: `[scheme://]host[:port]`.
///
/// `None` = unusable, and the caller falls back to the next configuration layer
/// (build-time env, then localhost) rather than failing the connect. Garbage must
/// never turn into a connect to an unintended host, so anything that cannot be a
/// hostname or IP literal is rejected outright.
///
/// Recognised schemes: `tls://` (encrypted) and `tcp://` (explicitly plain). Any
/// other scheme is stripped and treated as plain, which keeps a pasted
/// `http://…` working — the expected input is a player copying an address out of
/// a chat message, and the transport is decided by the address they were given.
///
/// Also tolerated: surrounding whitespace, a trailing slash, a `#`-commented or
/// empty file, and trailing lines (only the first is read).
pub fn parse_endpoint(raw: &str) -> Option<Endpoint> {
    let line = raw.lines().next().unwrap_or("").trim();
    let (tls, line) = match line.split_once("://") {
        Some((scheme, rest)) => (scheme.trim().eq_ignore_ascii_case("tls"), rest),
        None => (false, line),
    };
    let line = line.trim_end_matches('/').trim();
    if line.is_empty() || line.starts_with('#') {
        return None;
    }
    // Host/port split. Three shapes, in the order they have to be distinguished:
    //
    //   [addr]:port   bracketed — the only unambiguous way to give an IPv6
    //                 literal a port, and the form every tool prints
    //   a:b:c         two or more colons and no brackets — an IPv6 literal, NOT
    //                 host:port. Splitting on the last colon here would turn
    //                 "::1" into host ":" port 1, i.e. a connect to something the
    //                 player never typed
    //   host:port     the ordinary case
    //
    // Note this is about not MANGLING an address the player pasted. IPv6 reach
    // itself does not depend on it: the recommended input is a hostname, and
    // `connect_resolved` resolves AAAA records like any other.
    let (host, port) = if let Some(rest) = line.strip_prefix('[') {
        let (addr, tail) = rest.split_once(']')?;
        let port = match tail.strip_prefix(':') {
            Some(p) => match p.trim().parse::<u16>() {
                Ok(0) | Err(_) => return None,
                Ok(p) => p,
            },
            // "[addr]" with nothing after it is fine; anything else is garbage.
            None if tail.trim().is_empty() => DEFAULT_SERVER_PORT,
            None => return None,
        };
        (addr.trim(), port)
    } else if line.matches(':').count() >= 2 {
        (line, DEFAULT_SERVER_PORT)
    } else {
        match line.rsplit_once(':') {
            Some((h, p)) => match p.trim().parse::<u16>() {
                // Port 0 is not connectable, so treat it as garbage rather than
                // silently substituting the default.
                Ok(0) | Err(_) => return None,
                Ok(p) => (h.trim(), p),
            },
            None => (line, DEFAULT_SERVER_PORT),
        }
    };
    if host.is_empty()
        || host.len() > 253
        || !host
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_' | b':'))
    {
        return None;
    }
    Some(Endpoint { host: host.to_string(), port, tls })
}

/// Parse a certificate-pin line: 64 hex chars = the SHA-256 of the server
/// certificate's DER bytes. Used only for self-signed deployments; a normal
/// (Let's Encrypt) certificate validates against the bundled root store instead.
///
/// Whitespace, `:`/`-` separators and an optional `sha256:` prefix are accepted
/// because that is how fingerprint tools print them.
pub fn parse_pin(raw: &str) -> Option<[u8; 32]> {
    let line = raw.lines().next().unwrap_or("").trim();
    let line = line.strip_prefix("sha256:").unwrap_or(line);
    let hex: String = line
        .chars()
        .filter(|c| !c.is_whitespace() && *c != ':' && *c != '-')
        .collect();
    if hex.len() != 64 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let mut out = [0u8; 32];
    for (i, byte) in out.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).ok()?;
    }
    Some(out)
}
