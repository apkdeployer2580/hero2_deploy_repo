// Host-driven recovery backup: build the same `04 3d` upload the game's own recovery
// screen sends, without the player having to walk the network menus.
//
// ── WHY THE HOST DOES THIS AT ALL ────────────────────────────────────────────
// The player's report (2026-08-06): "네트워크 창고에 접속하지 않아도 백업이 올라가야
// 하는건데". In the original game a backup only exists if you went online and asked for
// one, so 초기화 destroys a character that was never uploaded and 복구 has nothing to
// restore. The host can close that gap because it owns both halves: it can read the
// slot record out of the guest, and it can open its own socket to the private server.
//
// ── WHERE THE 219 BYTES COME FROM ────────────────────────────────────────────
// NOT from the save file on disk — that is MD5-prefixed and encrypted. The game's own
// `04 3d` builder (guest 0x170d88, the recovery screen's OK arm) does exactly this:
//
//     rec  = 0x194f18("./Hero3GameSave", slot)   ; load + decrypt + verify
//     blob = *(u32*)rec + 8                      ; KTF handle payload
//     put_u8(0x04); put_u8(0x3d); put_u8(slot); put_blob(blob, 0xdb)
//
// `0xdb` = 219 is not an arbitrary prefix: it is the size of the save's FIRST block,
// the one `serialiseSave` writes with `0x1973d0(&cursor=0, buf)` — 25 bytes of header
// + 9 equipment records of 20 + a 14-byte tail. So we make the identical call and take
// the identical bytes, which is why this cannot drift from what the game would send.
//
// This module is DEPENDENCY-FREE so `backup_selftest.rs` can `include!` the real source
// (same contract as endpoint.rs / buffs.rs — see CLAUDE.md). Everything here is pure:
// the guest calls and the socket live in `emulator.rs` / `hero3_link.rs`.

/// `loadSaveRecord(const char *name, int slot) -> handle` (Thumb bit set).
///
/// Ordinary function — it builds its own `sl`, so the host may call it directly. Returns
/// 0 when the record is missing OR its MD5 does not match, which is exactly the "nothing
/// worth backing up" answer we want: an empty slot uploads nothing.
pub const FN_LOAD_SAVE_RECORD: u32 = 0x0019_4f18 | 1;

/// WIPI relocatable-handle payload offset, the same `*handle + 8` the rest of the guest
/// uses (item grid, popup labels).
pub const HANDLE_PAYLOAD_ADJ: u32 = 8;

/// Bytes of the decrypted record the recovery upload carries.
pub const BACKUP_LEN: usize = 0xdb; // 219

/// Slots the game has. The wire byte is 0..255 but only these exist.
pub const SLOT_MAX: u32 = 2;

/// Plausible guest pointer — the binary/static image or the heap. Same window
/// `items`/`save` use; a handle outside it means the call did not return what we think.
fn plausible_ptr(p: u32) -> bool {
    (0x10_0000..0x8000_0000).contains(&p)
}

/// Pull the 219-byte backup body out of the handle `FN_LOAD_SAVE_RECORD` returned.
///
/// `read(addr, len)` is the guest-memory reader. `None` = the handle did not resolve, or
/// the record is short — either way there is nothing safe to upload, and uploading a
/// wrong 219 bytes would overwrite a good backup with rubbish.
pub fn body_from_handle(read: &impl Fn(u32, usize) -> Vec<u8>, handle: u32) -> Option<Vec<u8>> {
    if !plausible_ptr(handle) {
        return None;
    }
    let block = read(handle, 4);
    if block.len() != 4 {
        return None;
    }
    let block = u32::from_le_bytes([block[0], block[1], block[2], block[3]]);
    if !plausible_ptr(block) {
        return None;
    }
    let payload = block.checked_add(HANDLE_PAYLOAD_ADJ)?;
    let body = read(payload, BACKUP_LEN);
    if body.len() != BACKUP_LEN {
        return None;
    }
    // An all-zero body is what a freshly cleared slot reads as. Uploading it would
    // replace a real backup with a blank character — the one outcome worse than not
    // backing up at all.
    if body.iter().all(|b| *b == 0) {
        return None;
    }
    Some(body)
}

// ── the wire frames (server side is hero3_server/hero3_protocol.py) ──────────
//
// Framing is `[u32 LE total_len][cat][op][body…]`, total_len INCLUDING the 4-byte
// header — the same shape `network.rs::frame_tag` reads.

fn frame(cat: u8, op: u8, body: &[u8]) -> Vec<u8> {
    let total = 4 + 2 + body.len();
    let mut out = Vec::with_capacity(total);
    out.extend_from_slice(&(total as u32).to_le_bytes());
    out.push(cat);
    out.push(op);
    out.extend_from_slice(body);
    out
}

/// `01 01 02` — the client's opening hello.
pub fn frame_hello() -> Vec<u8> {
    frame(0x01, 0x01, &[0x02])
}

/// `01 3d <16B account id><20B pid><5B tail>` — login.
///
/// The trailing 25 bytes are copied from a device capture of the real client's login.
/// The private server reads only the 16-byte identity (it keys every account on those
/// bytes), but sending the full shape keeps this frame indistinguishable from the
/// game's own — so the server never needs a special case for us, and a future server
/// that DOES parse the tail will not reject the host's backups.
pub fn frame_login(account: &[u8; 16]) -> Vec<u8> {
    let mut body = Vec::with_capacity(41);
    body.extend_from_slice(account);
    body.push(b'3');
    body.extend_from_slice(&[0u8; 19]);
    body.extend_from_slice(&[0x01, 0x00, 0x00, 0x00, 0x00]);
    frame(0x01, 0x3d, &body)
}

/// `04 3d <slot><219B>` — the backup upload itself.
pub fn frame_backup(slot: u8, body: &[u8]) -> Option<Vec<u8>> {
    if body.len() != BACKUP_LEN || u32::from(slot) > SLOT_MAX {
        return None;
    }
    let mut b = Vec::with_capacity(1 + BACKUP_LEN);
    b.push(slot);
    b.extend_from_slice(body);
    Some(frame(0x04, 0x3d, &b))
}

/// Split a received byte stream into whole frames, returning `(cat, op)` for each.
///
/// Used only to notice the server's acks; a partial tail is left for the next read.
pub fn frames_in(buf: &[u8]) -> Vec<(u8, u8)> {
    let mut out = Vec::new();
    let mut i = 0usize;
    while i + 6 <= buf.len() {
        let total = u32::from_le_bytes([buf[i], buf[i + 1], buf[i + 2], buf[i + 3]]) as usize;
        if total < 6 || i + total > buf.len() {
            break;
        }
        out.push((buf[i + 4], buf[i + 5]));
        i += total;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn reader(mem: HashMap<u32, Vec<u8>>) -> impl Fn(u32, usize) -> Vec<u8> {
        move |addr, len| {
            mem.get(&addr)
                .map(|v| v.iter().copied().take(len).collect())
                .unwrap_or_default()
        }
    }

    #[test]
    fn frames_carry_a_length_prefix_that_includes_itself() {
        let h = frame_hello();
        assert_eq!(h, vec![7, 0, 0, 0, 0x01, 0x01, 0x02]);
        let l = frame_login(&[0xab; 16]);
        assert_eq!(u32::from_le_bytes([l[0], l[1], l[2], l[3]]) as usize, l.len());
        assert_eq!(&l[4..6], &[0x01, 0x3d]);
        assert_eq!(&l[6..22], &[0xab; 16]);
        assert_eq!(l.len(), 4 + 2 + 41, "login body is the 41 bytes the client sends");
    }

    #[test]
    fn backup_frame_is_226_bytes_and_range_checks_the_slot() {
        let body = vec![7u8; BACKUP_LEN];
        let f = frame_backup(2, &body).expect("slot 2 is valid");
        assert_eq!(f.len(), 226, "4 header + cat/op + slot + 219, the device-captured size");
        assert_eq!(&f[4..7], &[0x04, 0x3d, 0x02]);
        assert_eq!(&f[7..], &body[..]);
        assert!(frame_backup(3, &body).is_none(), "only slots 0..2 exist");
        assert!(frame_backup(0, &body[..10]).is_none(), "a short body is never sent");
    }

    #[test]
    fn body_follows_the_handle_and_refuses_anything_implausible() {
        let good = vec![9u8; BACKUP_LEN];
        let mut mem = HashMap::new();
        mem.insert(0x4000_0000, 0x4100_0000u32.to_le_bytes().to_vec());
        mem.insert(0x4100_0008, good.clone());
        let read = reader(mem);
        assert_eq!(body_from_handle(&read, 0x4000_0000), Some(good));
        // A null / out-of-range handle is the guest saying "no such record".
        assert_eq!(body_from_handle(&read, 0), None);
        assert_eq!(body_from_handle(&read, 0xffff_ffff), None);
        // The handle resolves but the block does not.
        let mut bad = HashMap::new();
        bad.insert(0x4000_0000, 1u32.to_le_bytes().to_vec());
        assert_eq!(body_from_handle(&reader(bad), 0x4000_0000), None);
    }

    #[test]
    fn an_all_zero_body_is_refused_so_a_wiped_slot_cannot_erase_a_backup() {
        let mut mem = HashMap::new();
        mem.insert(0x4000_0000, 0x4100_0000u32.to_le_bytes().to_vec());
        mem.insert(0x4100_0008, vec![0u8; BACKUP_LEN]);
        assert_eq!(body_from_handle(&reader(mem), 0x4000_0000), None);
    }

    #[test]
    fn frames_in_splits_a_stream_and_stops_on_a_partial_tail() {
        let mut s = frame_hello();
        s.extend_from_slice(&frame(0x01, 0x3d, &[]));
        s.extend_from_slice(&[0xff, 0x00]); // partial
        assert_eq!(frames_in(&s), vec![(0x01, 0x01), (0x01, 0x3d)]);
        assert!(frames_in(&[0, 0, 0, 0, 0, 0]).is_empty(), "a zero-length frame cannot loop");
    }
}
