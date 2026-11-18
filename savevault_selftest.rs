// Standalone self-test of the save-vault POLICY.
//
//   rustc --edition 2021 --test savevault_selftest.rs -o t.exe && ./t.exe
//
// ★ This does NOT mirror the implementation — it `include!`s the real
// `wie_android/src/savevault.rs`, so the rules under test ARE the shipping rules
// and the two cannot drift. (`cargo test -p wie_android` does not run on this
// Windows host; see `endpoint_selftest.rs` for the same arrangement.)
//
// Why this is worth testing at all: every function here decides something about
// a file that IS a player's character. `classify` wrong in one direction vaults
// nothing and the feature silently does not exist; wrong in the other it spends
// the vault on `cf` and `hl2List`. `should_snapshot` wrong collapses a real save
// or burns two seats on one. `drop_list` wrong throws away the generation the
// player is about to ask for.

include!("wie_android/src/savevault.rs");

#[cfg(test)]
mod tests {
    use super::*;

    // ── classify ────────────────────────────────────────────────────────────

    #[test]
    fn both_halves_of_every_slot_are_recognised() {
        for slot in SLOT_MIN..=SLOT_MAX {
            assert_eq!(classify(&format!("n{slot}")), Some((Half::Char, slot)));
            assert_eq!(classify(&format!("g{slot}")), Some((Half::Prog, slot)));
        }
    }

    #[test]
    fn a_leading_slash_is_the_same_file() {
        // The guest names these at the filesystem root; both spellings reach the
        // Filesystem impl depending on how the guest built the string.
        assert_eq!(classify("/n1"), Some((Half::Char, 1)));
        assert_eq!(classify("/g3"), Some((Half::Prog, 3)));
    }

    #[test]
    fn the_games_other_files_are_not_saves() {
        // Everything else the archive ships at the filesystem root. Vaulting any
        // of these would spend the player's seats on nothing.
        for p in ["cf", "hl2List", "m/m/00.m.gz", "wie_server.txt", ""] {
            assert_eq!(classify(p), None, "{p} must not be taken for a save");
        }
    }

    #[test]
    fn a_slot_the_game_does_not_have_is_not_a_save() {
        // Five slots (2026-09-27 — three as shipped). `n0` and `n6` are not saves, and
        // `n10` is not slot 1 — one digit exactly, or it is some other file that happens
        // to start `n`.
        for p in ["n0", "n6", "g0", "g9", "n10", "g11", "n", "g"] {
            assert_eq!(classify(p), None, "{p} must not be taken for a save");
        }
        assert_eq!(SLOT_MAX, 5);
    }

    /// ★ 2026-09-27: the two slots the host added are saves like the game's three — a save
    /// to slot 4 or 5 is a vault edge (and so the ACCOUNT warehouse's commit) like any other.
    #[test]
    fn the_added_slots_are_saves() {
        assert_eq!(classify("/n4"), Some((Half::Char, 4)));
        assert_eq!(classify("g5"), Some((Half::Prog, 5)));
        assert_eq!(parse_key("5|1700000000000"), Some((5, 1_700_000_000_000)));
        assert_eq!(parse_key("6|1700000000000"), None);
    }

    #[test]
    fn guest_path_is_the_inverse_of_classify() {
        for slot in SLOT_MIN..=SLOT_MAX {
            for half in Half::BOTH {
                let p = guest_path(half, slot);
                assert_eq!(classify(&p), Some((half, slot)), "{p}");
            }
        }
    }

    // ── the length band ─────────────────────────────────────────────────────

    #[test]
    fn every_save_this_repo_has_measured_is_usable() {
        // The four archived saves plus the S9's live slot 1, measured 2026-09-19.
        // If a change to the band ever rejects one of these, the band is wrong —
        // these are real saves the game wrote.
        for len in [1352usize, 1338, 1592, 889, 447, 960] {
            assert!(usable_len(len), "{len} is a save the game actually wrote");
        }
    }

    #[test]
    fn a_torn_write_is_refused() {
        // The state a process killed between truncate-to-zero and write leaves
        // behind — the one thing the floor exists for.
        assert!(!usable_len(0));
        assert!(!usable_len(LEN_MIN - 1));
        assert!(usable_len(LEN_MIN));
    }

    #[test]
    fn the_ceiling_stays_far_above_anything_observed() {
        assert!(usable_len(LEN_MAX));
        assert!(!usable_len(LEN_MAX + 1));
        // Narrowing this is a claim about the guest's writer that nothing in this
        // repo has measured — see the constant's doc. Observed maximum is 1,592.
        assert!(LEN_MAX >= 1_592 * 8, "the ceiling must not become a format claim");
    }

    // ── keys ────────────────────────────────────────────────────────────────

    #[test]
    fn a_key_round_trips() {
        for slot in SLOT_MIN..=SLOT_MAX {
            let k = key(slot, 1_758_300_000_123);
            assert_eq!(parse_key(&k), Some((slot, 1_758_300_000_123)));
        }
    }

    #[test]
    fn a_key_from_outside_is_parsed_not_trusted() {
        // This value arrives from a Java dialog across JNI.
        for bad in ["", "1", "1|", "|123", "x|123", "1|x", "1|123|456", "0|123", "6|123", "-1|123"] {
            assert_eq!(parse_key(bad), None, "{bad:?} must not parse");
        }
    }

    #[test]
    fn surrounding_whitespace_does_not_break_a_key() {
        assert_eq!(parse_key("  2 | 7  \n"), Some((2, 7)));
    }

    #[test]
    fn an_offer_line_starts_with_its_own_key() {
        let line = offer_line(2, 1_700_000_000_000, 1352);
        let mut it = line.rsplitn(2, '|');
        let len = it.next().unwrap();
        let k = it.next().unwrap();
        assert_eq!(len, "1352");
        assert_eq!(parse_key(k), Some((2, 1_700_000_000_000)));
    }

    // ── the burst gap ───────────────────────────────────────────────────────

    #[test]
    fn the_first_save_of_a_process_is_always_snapshotted() {
        assert!(should_snapshot(None, 0));
        assert!(should_snapshot(None, 1_758_300_000_000));
    }

    #[test]
    fn one_save_is_one_generation() {
        // `ag.l()` writes n<slot> then g<slot>; the second edge lands milliseconds
        // after the first and must NOT spend a second seat on the same instant.
        let t = 1_758_300_000_000u64;
        assert!(should_snapshot(None, t));
        assert!(!should_snapshot(Some(t), t + 1));
        assert!(!should_snapshot(Some(t), t + 45)); // measured save: 45 ms on the S9
    }

    #[test]
    fn two_deliberate_saves_are_two_generations() {
        // The gap must stay far below the time it takes a human to walk the save
        // menus twice, or the vault quietly keeps half the history it claims to.
        let t = 1_758_300_000_000u64;
        assert!(should_snapshot(Some(t), t + MIN_GAP_MS));
        assert!(MIN_GAP_MS <= 5_000, "the gap must not eat a real save");
    }

    #[test]
    fn a_clock_that_went_backwards_does_not_block_snapshots() {
        // ★ The name of this test was already right and the assertion under it was the
        // opposite (2026-09-20). `saturating_sub` read a backwards step as "no time has
        // passed", so the vault kept NOTHING for as long as the correction lasted plus
        // the gap — fail-CLOSED, in the module whose header says the cost of a missed
        // snapshot is the save it exists to keep. Android moves this clock (NTP after
        // boot or airplane mode, a manual change), so it is not a hypothetical.
        assert!(should_snapshot(Some(1_000), 500));
        assert!(should_snapshot(Some(1_758_300_000_000), 1_758_200_000_000));
        // A step back does not disable the burst rule going forward.
        assert!(!should_snapshot(Some(500), 500 + MIN_GAP_MS - 1));
    }

    // ── retention ───────────────────────────────────────────────────────────

    #[test]
    fn nothing_is_dropped_below_the_ceiling() {
        let gens: Vec<u64> = (0..KEEP_MAX as u64).collect();
        assert!(drop_list(&gens, KEEP_MAX).is_empty());
        assert!(drop_list(&[], KEEP_MAX).is_empty());
    }

    #[test]
    fn the_oldest_go_first_and_the_newest_survive() {
        let gens: Vec<u64> = (0..(KEEP_MAX as u64 + 3)).collect();
        let dropped = drop_list(&gens, KEEP_MAX);
        assert_eq!(dropped, vec![0, 1, 2]);
        let kept = gens.len() - dropped.len();
        assert_eq!(kept, KEEP_MAX);
    }

    #[test]
    fn the_input_order_does_not_change_the_answer() {
        // The caller builds this from a directory listing, whose order is the
        // filesystem's business and not ours.
        let mut shuffled = vec![50u64, 10, 40, 20, 30];
        let a = drop_list(&shuffled, 2);
        shuffled.reverse();
        let b = drop_list(&shuffled, 2);
        assert_eq!(a, b);
        assert_eq!(a, vec![10, 20, 30]);
    }

    #[test]
    fn keeping_none_drops_everything() {
        assert_eq!(drop_list(&[3, 1, 2], 0), vec![1, 2, 3]);
    }
}
