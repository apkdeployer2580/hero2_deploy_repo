// Standalone self-test of hero2's Java-side readers and write producers.
//
//   rustc --edition 2021 --test hero2java_selftest.rs -o t.exe && ./t.exe
//
// `include!`s the REAL sources — `guest_anchors.rs` (the table) and `hero2java.rs` (the
// walks) — the same two files `wie_android::guest_build` and `wie_probe` splice in. The
// tests live in `hero2java.rs` under `#[cfg(test)]`, against a fake guest built from the
// probe-measured shapes; a `use crate::…` in either file breaks this build, which is the
// intended alarm (they must stay dependency-free for the probe).

include!("wie_android/src/guest_anchors.rs");
include!("wie_android/src/hero2java.rs");

#[test]
fn flight_uses_live_map_after_loading_slot_is_cleared() {
    let sv = GuestBuild::Hero2Ktf.hero2_save().unwrap();
    for (map, state, expected) in [(0, 5, true), (67, 5, true), (100, 5, true), (113, 5, false), (0, 3, false)] {
        let words = [(Hero2Java::MAP_SLOT, u32::MAX), (0x0019_3aec, state),
            (sv.area.stage_slot, 0x48001000), (0x48001000, 0x48002000),
            (0x48002004 + sv.area.stage_type_off, map)];
        let read = |addr, len| words.iter().find(|(a, _)| *a == addr)
            .map(|(_, v)| v.to_le_bytes()[..len].to_vec()).unwrap_or_default();
        assert_eq!(h2_flight_allowed(&read), expected);
    }
}

#[test]
fn demon_intelligence_keeps_midrange_and_has_continuous_linear_tail() {
    for i in 0..=200 {
        assert_eq!(DemonPatch::skill_int_power(i), (i + i * i / 50).max(20 + 2 * i));
    }
    for i in 201..=1000 {
        assert_eq!(DemonPatch::skill_int_power(i) - DemonPatch::skill_int_power(i - 1), 5);
    }
}

#[test]
fn skill_hotkey_badge_patch_is_guarded_and_idempotent() {
    let sites = GuestBuild::Hero2Ktf.skill_hotkey_badges_off();
    assert_eq!(sites, &[(0x0015_ff70, 0xb5f0, 0x4770)]);
    for build in [GuestBuild::Ktf, GuestBuild::Lgt, GuestBuild::Skt] {
        assert!(build.skill_hotkey_badges_off().is_empty());
    }
    let read = |word: u16| move |address, size| {
        assert_eq!((address, size), (0x0015_ff70, 2));
        word.to_le_bytes().to_vec()
    };
    assert_eq!(h2_halfword_writes(&read(0xb5f0), sites), (vec![(0x0015_ff70, [0x70, 0x47])], 0, 1));
    assert_eq!(h2_halfword_writes(&read(0x4770), sites), (vec![], 1, 1));
    assert_eq!(h2_halfword_writes(&read(0xffff), sites), (vec![], 0, 1));
    assert_eq!(h2_halfword_writes(&|_, _| vec![], sites), (vec![], 0, 1));
}
