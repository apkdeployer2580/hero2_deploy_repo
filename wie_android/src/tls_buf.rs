// The plaintext-buffer half of `tls.rs`, kept dependency-free so
// `tls_buf_selftest.rs` can `include!` it and test the REAL code.
//
// Why this is split out at all: the TLS integration itself (rustls handshake,
// certificate pinning) cannot be exercised on this build host — `ring` needs a C
// compiler for the host target and only the Android NDK one is installed — but
// this buffer is where a bug would actually cost data. It sits between the record
// layer and the guest's `recv`, and getting it wrong means either losing bytes of
// a game frame or telling the guest "connection closed" when the connection is
// fine (which makes the game tear down a healthy session).
//
// ★ Plain `//` comments, not `//!`: an inner doc comment cannot appear inside an
// `include!`. Same constraint as `endpoint.rs`.

/// Hand at most `buf.len()` decrypted bytes to the guest.
///
/// The three outcomes are deliberately distinct, because the guest treats them
/// very differently (see `network.rs`: -19 vs 0 vs negative):
///
///   * `Ok(n>0)` — data. Partial delivery is fine; the guest re-reads.
///   * `Ok(0)`   — TRUE end of stream. Only when the peer is gone AND everything
///                 already decrypted has been handed over. Returning this early
///                 would discard plaintext that arrived in the same records as
///                 the close.
///   * `WouldBlock` — nothing yet, connection healthy. Conflating this with
///                 `Ok(0)` is what makes the game drop a live connection.
pub fn drain_plain(
    plain: &mut Vec<u8>,
    eof: bool,
    buf: &mut [u8],
) -> std::io::Result<usize> {
    if plain.is_empty() {
        if eof {
            return Ok(0);
        }
        return Err(std::io::Error::new(
            std::io::ErrorKind::WouldBlock,
            "no plaintext yet",
        ));
    }
    // A zero-length destination must not be reported as end-of-stream. The guest
    // asks for `1024 - used` bytes and `network.rs` withholds 8 of them, so a
    // full buffer legitimately arrives here asking for nothing.
    if buf.is_empty() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::WouldBlock,
            "caller left no room",
        ));
    }
    let n = buf.len().min(plain.len());
    buf[..n].copy_from_slice(&plain[..n]);
    plain.drain(..n);
    Ok(n)
}
