// Standalone self-test of the TLS plaintext-buffer handoff.
//
//   rustc --edition 2021 --test tls_buf_selftest.rs -o t4.exe && ./t4.exe
//
// `include!`s the real `wie_android/src/tls_buf.rs`, so this tests the shipping
// code rather than a copy (same discipline as `endpoint_selftest.rs`).
//
// Why this matters: `drain_plain` sits between the TLS record layer and the
// guest's `recv`, and the guest's contract is unforgiving — it reads -19 as
// "healthy, nothing yet" and ANY other non-positive value as "connection dead"
// and tears down. So the difference between `WouldBlock` and `Ok(0)` here is the
// difference between a working session and the game dropping a live server.
//
// The full TLS integration (handshake, certificate pinning) cannot be exercised
// on this host: `ring` needs a C compiler for the host target and only the
// Android NDK's is installed. That part is covered by the Android build plus the
// Python end-to-end test of the server side, and remains device-unverified.

include!("wie_android/src/tls_buf.rs");

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::ErrorKind;

    #[test]
    fn hands_over_available_plaintext() {
        let mut plain = b"hello".to_vec();
        let mut buf = [0u8; 8];
        assert_eq!(drain_plain(&mut plain, false, &mut buf).unwrap(), 5);
        assert_eq!(&buf[..5], b"hello");
        assert!(plain.is_empty(), "delivered bytes must be consumed");
    }

    #[test]
    fn partial_delivery_keeps_the_remainder_in_order() {
        // The guest re-reads; nothing may be dropped or reordered across calls.
        let mut plain = b"0123456789".to_vec();
        let mut buf = [0u8; 4];
        assert_eq!(drain_plain(&mut plain, false, &mut buf).unwrap(), 4);
        assert_eq!(&buf, b"0123");
        assert_eq!(drain_plain(&mut plain, false, &mut buf).unwrap(), 4);
        assert_eq!(&buf, b"4567");
        assert_eq!(drain_plain(&mut plain, false, &mut buf).unwrap(), 2);
        assert_eq!(&buf[..2], b"89");
        assert!(plain.is_empty());
    }

    #[test]
    fn empty_but_healthy_is_wouldblock_not_eof() {
        // THE load-bearing case: returning Ok(0) here makes the game tear down a
        // perfectly good connection (network.rs maps 0 to "peer gone").
        let mut plain = Vec::new();
        let mut buf = [0u8; 8];
        let err = drain_plain(&mut plain, false, &mut buf).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::WouldBlock);
    }

    #[test]
    fn eof_is_reported_only_after_the_buffer_is_drained() {
        // Plaintext can arrive in the same records as the close. Reporting EOF
        // while bytes are still buffered would silently truncate the last frame.
        let mut plain = b"tail".to_vec();
        let mut buf = [0u8; 8];
        assert_eq!(drain_plain(&mut plain, true, &mut buf).unwrap(), 4, "data first");
        assert_eq!(&buf[..4], b"tail");
        assert_eq!(drain_plain(&mut plain, true, &mut buf).unwrap(), 0, "then EOF");
    }

    #[test]
    fn eof_with_nothing_buffered_is_zero() {
        let mut plain = Vec::new();
        let mut buf = [0u8; 8];
        assert_eq!(drain_plain(&mut plain, true, &mut buf).unwrap(), 0);
    }

    #[test]
    fn zero_length_destination_is_not_end_of_stream() {
        // network.rs withholds 8 bytes from whatever the guest asks for, so a
        // request for exactly its slack arrives here as an empty slice. Answering
        // Ok(0) would look like a closed connection.
        let mut plain = b"data".to_vec();
        let mut buf: [u8; 0] = [];
        let err = drain_plain(&mut plain, false, &mut buf).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::WouldBlock);
        assert_eq!(plain.len(), 4, "nothing consumed");
        // …and still not EOF even when the peer has gone, as long as data remains.
        let err = drain_plain(&mut plain, true, &mut buf).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::WouldBlock);
        assert_eq!(plain.len(), 4);
    }

    #[test]
    fn a_full_game_frame_survives_arbitrary_chunking() {
        // The real traffic is `[u32 LE total_len][payload]`. Whatever record
        // boundaries TLS imposes, reassembly by the caller must see the exact
        // byte stream — so feed it through in every chunk size and compare.
        let payload: Vec<u8> = (0u8..=255).collect();
        let mut frame = ((payload.len() + 4) as u32).to_le_bytes().to_vec();
        frame.extend_from_slice(&payload);

        for chunk in [1usize, 3, 7, 64, 255, 260, 1024] {
            let mut plain = frame.clone();
            let mut out = Vec::new();
            let mut buf = vec![0u8; chunk];
            loop {
                match drain_plain(&mut plain, true, &mut buf) {
                    Ok(0) => break,
                    Ok(n) => out.extend_from_slice(&buf[..n]),
                    Err(e) => panic!("unexpected {e:?} at chunk {chunk}"),
                }
            }
            assert_eq!(out, frame, "byte stream altered at chunk size {chunk}");
            let total = u32::from_le_bytes([out[0], out[1], out[2], out[3]]) as usize;
            assert_eq!(total, frame.len(), "length prefix broken at chunk {chunk}");
        }
    }
}
