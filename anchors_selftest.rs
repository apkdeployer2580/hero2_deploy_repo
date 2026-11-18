//! Standalone test for the per-build guest anchor table.
//!
//!     rustc --edition 2021 --test anchors_selftest.rs -o t.exe && ./t.exe
//!
//! ★ `include!`s the REAL source (`wie_android/src/guest_anchors.rs`), which is the
//! same file `wie_android::guest_build` and `wie_probe` splice in. That is the point:
//! the table exists so the app and the probe cannot disagree about where CHAR is, and
//! a test that exercised a copy would not defend that.
//!
//! What is actually worth asserting here is not the numbers for their own sake but the
//! DIFFERENCES between the builds — every one of them was a bug at some point:
//!   * KTF reaches CHAR through a pointer global, LGT's record IS a static. Treating
//!     the LGT static as a pointer reads a sprite (that mistake is written up in
//!     MIGRATION_LGT.md §2.7).
//!   * the state byte moved `+0x7c` → `+0x78`.
//!   * containers are `*(handle) + 8` on KTF and direct pointers on LGT. Getting this
//!     one wrong is silent: the chain still resolves, on the wrong bytes.
//! `cargo test -p wie_android` cannot run on this host (linked winit/android deps die
//! at startup), which is why this is a standalone file — see CLAUDE.md.

include!("wie_android/src/guest_anchors.rs");

#[cfg(test)]
mod tests {
    use super::*;

    /// KTF: `*(sl+0x444)` with the observed static fallback. Both are pointers to
    /// follow, never bases to read fields off.
    #[test]
    fn ktf_reaches_char_through_a_pointer() {
        match GuestBuild::Ktf.char_anchor() {
            Some(CharAnchor::Pointer { global, fallback }) => {
                assert_eq!(global, 0x1b_3084);
                assert_eq!(fallback, 0x1b_7b6c);
            }
            _ => panic!("KTF must resolve CHAR through a pointer global"),
        }
    }

    /// LGT: the record itself is a `.bss` static — there is no pointer to null-check,
    /// which is why `items::resolve_char` has to judge it by its CONTENTS.
    #[test]
    fn lgt_char_is_a_static() {
        match GuestBuild::Lgt.char_anchor() {
            Some(CharAnchor::Static(base)) => assert_eq!(base, 0x150_3a88),
            _ => panic!("LGT must resolve CHAR as a static"),
        }
    }

    /// The `.bss` base has to be consistent with the other LGT anchor measured the same
    /// day: the camera/scroll pair at `.bss 0x150d914` is also `CHAR+0x9e8c`
    /// (MIGRATION_LGT.md §2.7). If either number is ever retyped wrong this fails.
    #[test]
    fn lgt_char_base_agrees_with_the_camera_anchor() {
        let Some(CharAnchor::Static(base)) = GuestBuild::Lgt.char_anchor() else {
            panic!("LGT anchor shape changed");
        };
        assert_eq!(base + 0x9e8c, 0x150_d914);
    }

    /// SKT has never been measured; nothing may claim to know where its CHAR is.
    #[test]
    fn skt_has_no_char_anchor() {
        assert!(GuestBuild::Skt.char_anchor().is_none());
    }

    #[test]
    fn the_state_byte_moved_between_builds() {
        assert_eq!(GuestBuild::Ktf.state_off(), 0x7c);
        assert_eq!(GuestBuild::Lgt.state_off(), 0x78);
        assert_ne!(GuestBuild::Ktf.state_off(), GuestBuild::Lgt.state_off());
    }

    /// The container convention. KTF's carrier runtime hands the game handles, LGT's
    /// hands it direct pointers; swapping these is the silent failure mode, because a
    /// wrong-but-in-heap word still dereferences.
    #[test]
    fn containers_are_handles_on_ktf_and_direct_on_lgt() {
        assert_eq!(GuestBuild::Ktf.container_data_off(), Some(8));
        assert_eq!(GuestBuild::Lgt.container_data_off(), None);
    }

    /// The blend-suppression global is a KTF address; on LGT it has not been located,
    /// and claiming an address there would read an unrelated word of a different
    /// compile. `None` = never suppress, which is what KTF does in practice anyway.
    #[test]
    fn noblend_global_is_only_claimed_where_it_was_measured() {
        assert_eq!(GuestBuild::Ktf.minimap_noblend_global(), Some(0x001a_4bc4));
        assert_eq!(GuestBuild::Lgt.minimap_noblend_global(), None);
        assert_eq!(GuestBuild::Skt.minimap_noblend_global(), None);
    }

    /// The bottom-HUD skip: both builds have the block, and each one's branch has to
    /// encode as a Thumb-1 `b` that actually lands on that build's epilogue. Recomputing
    /// the immediate here is the point — a mistyped branch would jump into the middle of
    /// the renderer, and the byte-check in front of the write cannot catch that.
    #[test]
    fn hud_bottom_branches_reach_their_epilogues() {
        // (build, epilogue the branch must land on)
        for (build, epilogue) in [(GuestBuild::Ktf, 0x14_a324u32), (GuestBuild::Lgt, 0x00_304au32)] {
            let (site, _orig, branch) = build.hud_bottom_patch().expect("both builds are located");
            assert_eq!(branch & 0xf800, 0xe000, "{} patch must be an unconditional Thumb b", build.name());
            let imm11 = i32::from(branch & 0x7ff);
            assert!(imm11 <= 0x3ff, "{}: imm11 {imm11:#x} would be a negative branch", build.name());
            assert_eq!(site as i32 + 4 + imm11 * 2, epilogue as i32, "{} branch target", build.name());
        }
    }

    /// The instruction being overwritten, as read back from the live guest in a probe
    /// run: KTF `ldr r3,[sp,#0x100]` = 0x9b40, LGT `ldr r5,[sp,#0x94]` = 0x9d25. Both
    /// are `ldr rX,[sp,#imm]`, which is what the block starts with on both builds.
    #[test]
    fn hud_bottom_original_is_an_sp_relative_load() {
        for build in [GuestBuild::Ktf, GuestBuild::Lgt] {
            let (_site, orig, _branch) = build.hud_bottom_patch().unwrap();
            assert_eq!(orig & 0xf800, 0x9800, "{}: original must be ldr rX,[sp,#imm]", build.name());
        }
        assert_eq!(GuestBuild::Ktf.hud_bottom_patch().unwrap().1, 0x9b40);
        assert_eq!(GuestBuild::Lgt.hud_bottom_patch().unwrap().1, 0x9d25);
    }

    #[test]
    fn skt_has_no_hud_patch() {
        assert!(GuestBuild::Skt.hud_bottom_patch().is_none());
    }

    /// The EXP divisor is located on both builds, at addresses that must not be
    /// confused with each other (the KTF one is 0x14654e, the LGT one 0x4d3cc — and the
    /// LGT `.text` is only 0x5db85 long, so a KTF address there is not even in range).
    #[test]
    fn exp_divisor_sites_are_per_build_and_in_range() {
        let ktf = GuestBuild::Ktf.exp_divisor_site().unwrap();
        let lgt = GuestBuild::Lgt.exp_divisor_site().unwrap();
        assert_eq!(ktf, 0x14_654e);
        assert_eq!(lgt, 0x04_d3cc);
        assert_ne!(ktf, lgt);
        // LGT is an ELF with `.text` at vaddr 0x1000, size 0x5db85 on Ver 01.00.07.
        assert!((0x1000..0x1000 + 0x5db85).contains(&lgt), "LGT site must be inside .text");
        assert!(!(0x1000..0x1000 + 0x5db85).contains(&ktf), "the KTF site is not an LGT address");
        assert!(GuestBuild::Skt.exp_divisor_site().is_none());
    }

    /// The 천상의 영약 target: the game's own `checkLevelUp`, THUMB bit set, inside LGT
    /// `.text`. It is the kill path's own routine (named alongside `exp_divisor_site`),
    /// so `level_up_fn() - 1` and `exp_divisor_site` must live in the same image.
    #[test]
    fn level_up_fn_is_a_thumb_lgt_text_address() {
        let f = GuestBuild::Lgt.level_up_fn().expect("located on LGT");
        assert_eq!(f, 0x4_d715);
        assert_eq!(f & 1, 1, "must carry the Thumb bit for call_guest_function");
        assert!((0x1000..0x1000 + 0x5db85).contains(&(f & !1)), "inside LGT .text");
        assert!(GuestBuild::Ktf.level_up_fn().is_none(), "not ported to KTF (APK ships LGT)");
        assert!(GuestBuild::Skt.level_up_fn().is_none());
    }

    /// The network-warehouse deposit grade gate: one halfword, `cmp r3,#3` -> `cmp r3,#7`,
    /// inside LGT `.text`. The patch must be size-neutral (both are `cmp rN,#imm8`,
    /// encoding `0x2800 | imm`) and change ONLY the immediate, or it would corrupt the
    /// instruction stream rather than widen the grade bound.
    #[test]
    fn warehouse_grade_gate_is_a_size_neutral_immediate_bump() {
        let (site, orig, patched) = GuestBuild::Lgt.warehouse_grade_gate().expect("located on LGT");
        assert_eq!(site, 0x6f9a);
        assert!((0x1000..0x1000 + 0x5db85).contains(&site), "inside LGT .text");
        // cmp r3,#imm8 = 0x2b00 | imm. Same register, only the immediate moves.
        assert_eq!(orig & 0xff00, 0x2b00, "orig is cmp r3,#imm8");
        assert_eq!(patched & 0xff00, 0x2b00, "patched stays cmp r3,#imm8");
        assert_eq!(orig & 0xff, 3, "orig admits grade <= 3 (하늘)");
        assert_eq!(patched & 0xff, 7, "patched admits grade <= 7 (all real grades top out at 5)");
        assert!(GuestBuild::Ktf.warehouse_grade_gate().is_none());
        assert!(GuestBuild::Skt.warehouse_grade_gate().is_none());
    }

    /// The equip-level relief, one shape per build. KTF rewrites the one-halfword
    /// register move at 0x13d0aa; LGT has no such move, so it biases the PLAYER LEVEL
    /// load at 0x477ce instead of the requirement load at 0x477d2 (MIGRATION_LGT.md
    /// §2.12: the requirement-side rewrite proposed in §2.8 would turn an `ldrsb` into
    /// an `ldrb` and flip any item whose requirement byte is >= 0x80 from allowed to
    /// blocked).
    ///
    /// ★ The `assert_ne!` below is the point of this test: 0x477d2 is four bytes further
    /// on and is the address the older plan named, so an edit that "restores" it would
    /// look like a fix and be the regression.
    #[test]
    fn equip_relief_shapes_are_per_build_and_lgt_biases_the_level_side() {
        match GuestBuild::Ktf.equip_relief_site() {
            Some(EquipReliefSite::MoveHalfword { site }) => assert_eq!(site, 0x13_d0aa),
            _ => panic!("KTF relieves the equip check by rewriting the move halfword"),
        }
        match GuestBuild::Lgt.equip_relief_site() {
            Some(EquipReliefSite::LevelBias { site }) => {
                assert_eq!(site, 0x4_77ce, "LGT biases the player-level load");
                assert_ne!(site, 0x4_77d2, "that is the requirement load — it must keep its ldrsb");
                // Inside LGT `.text` (vaddr 0x1000, size 0x5db85 on Ver 01.00.07),
                // where the KTF move site is not.
                assert!((0x1000..0x1000 + 0x5db85).contains(&site));
                assert!(!(0x1000..0x1000 + 0x5db85).contains(&0x13_d0aau32));
            }
            _ => panic!("LGT relieves the equip check by biasing the level load"),
        }
        assert!(GuestBuild::Skt.equip_relief_site().is_none());
    }

    /// `popup_handle[8]` — the array whose row-0 label is the ONLY thing that can start
    /// a cash buff (`items::cash_use_pending`), hence the only thing that can make the
    /// EXP-divisor and relief patches fire at all. Same Pointer/Static disagreement as
    /// the CHAR record, for the same reason.
    #[test]
    fn popup_handle_anchors_are_per_build() {
        match GuestBuild::Ktf.popup_handles() {
            Some(PopupHandles::Pointer { global }) => assert_eq!(global, 0x1b_2df4),
            _ => panic!("KTF reaches the popup labels through an sl-relative data slot"),
        }
        match GuestBuild::Lgt.popup_handles() {
            Some(PopupHandles::Static(base)) => {
                assert_eq!(base, 0x151_2934);
                // `.bss` is 0x1500000 + 0x12958 on Ver 01.00.07, so the array is inside
                // it — and nowhere near the KTF slot, which is `sl`-relative.
                assert!((0x150_0000..0x150_0000 + 0x1_2958).contains(&base), "must be in .bss");
                assert!(!(0x150_0000..0x150_0000 + 0x1_2958).contains(&0x1b_2df4u32));
            }
            _ => panic!("LGT's popup handle array is a .bss static"),
        }
        assert!(GuestBuild::Skt.popup_handles().is_none());
    }

    /// The commit half of the 퀵슬롯등록 rewrite. The two builds use DIFFERENT mechanisms
    /// and that is the point: KTF calls the game's own `popup_key`, LGT synthesises the
    /// `OK` a finger would produce (probe-verified, MIGRATION_LGT.md §2.16). A build with
    /// `None` here must never OFFER the row — `items::popup_relabel` and
    /// `items::popup_sel_addr` both gate on this, not on `guest_code_addresses_valid`.
    #[test]
    fn popup_commit_is_per_build_and_lgt_needs_no_guest_address() {
        match GuestBuild::Ktf.popup_commit() {
            Some(PopupCommit::GuestCall { fn_addr, select_key }) => {
                assert_eq!(fn_addr, 0x0013_a86c | 1, "Thumb bit must be set for a guest call");
                assert_eq!(select_key, (-5i32) as u32);
            }
            _ => panic!("KTF commits a popup row by calling its own popup_key"),
        }
        assert_eq!(
            GuestBuild::Lgt.popup_commit(),
            Some(PopupCommit::SyntheticOk),
            "LGT must not carry a guest address here — a keypress cannot be stale"
        );
        assert!(GuestBuild::Skt.popup_commit().is_none());
        // A KTF address must never leak into the LGT arm: 0x13a86c is outside LGT `.text`
        // (vaddr 0x1000, size 0x5db85 on Ver 01.00.07) anyway, and this pins that the LGT
        // arm carries no address at all.
        assert!(!matches!(GuestBuild::Lgt.popup_commit(), Some(PopupCommit::GuestCall { .. })));
    }

    /// The inventory page's pre-emptor bytes (51차). The invariant is a COUPLING, not a
    /// pair of numbers: only a build whose commit BYPASSES the game's own key router
    /// (KTF's direct `popup_key` call) may carry any, because that is the only commit
    /// shape the game cannot refuse itself. LGT's synthetic OK rides the router, so its
    /// list must stay empty — the KTF offsets (`0x9c89`, notably `0xa220`, which sits in
    /// the region where the LGT record is documented to shift: `0xa384`→`0xa370`,
    /// `0xa3f8`→`0xa410`) read unrelated LGT fields, and checking them is what kept
    /// every CONSUMABLE popup un-relabelled on the device while skills worked.
    #[test]
    fn popup_preemptors_only_gate_the_build_whose_commit_bypasses_the_router() {
        assert_eq!(GuestBuild::Ktf.popup_preempt_offsets(), &[0x9c89, 0xa220]);
        assert!(GuestBuild::Lgt.popup_preempt_offsets().is_empty(), "LGT's OK rides the game's own router");
        assert!(GuestBuild::Skt.popup_preempt_offsets().is_empty());
        for b in [GuestBuild::Ktf, GuestBuild::Lgt, GuestBuild::Skt] {
            let bypasses_router = matches!(b.popup_commit(), Some(PopupCommit::GuestCall { .. }));
            assert_eq!(
                !b.popup_preempt_offsets().is_empty(),
                bypasses_router,
                "{}: pre-emptor checks belong to exactly the commit that skips the game's own gate",
                b.name()
            );
        }
    }

    #[test]
    fn names_are_the_ones_the_logs_print() {
        assert_eq!(GuestBuild::Ktf.name(), "KTF");
        assert_eq!(GuestBuild::Lgt.name(), "LGT");
        assert_eq!(GuestBuild::Skt.name(), "SKT");
    }

    /// MIGRATION_LGT.md §4-3: the save chain became a per-build table on 2026-08-11
    /// (9차). The LGT addresses come from the save routine at 0x2515e..0x25186, whose
    /// literal pool names all three functions and both record strings in one place.
    #[test]
    fn save_chain_is_per_build_and_carries_each_builds_proof() {
        let ktf = GuestBuild::Ktf.save_chain().unwrap();
        assert_eq!(ktf.fn_serialise, 0x19_60bc | 1);
        assert_eq!(ktf.fn_slot_summary, 0x19_6b90 | 1);
        assert_eq!(ktf.fn_write_record, 0x19_51b0 | 1);
        // KTF scratch: inside the reachability-proven dead range [0x14a32e, 0x14a6aa).
        assert!((0x14_a32e..0x14_a6aa - 3).contains(&ktf.scratch));
        assert_eq!(ktf.slot_off, 0xa3f8);
        assert!(matches!(ktf.names, SaveNames::SlPtrs { game: 0x1b_2d78, slot: 0x1b_2d7c }));

        let lgt = GuestBuild::Lgt.save_chain().unwrap();
        assert_eq!(lgt.fn_serialise, 0x1_ad88 | 1);
        assert_eq!(lgt.fn_slot_summary, 0x1_a6d0 | 1);
        assert_eq!(lgt.fn_write_record, 0x1_a264 | 1);
        // All three must be .text addresses on LGT.
        for f in [lgt.fn_serialise, lgt.fn_slot_summary, lgt.fn_write_record] {
            assert!((0x1000..0x1000 + 0x5db85).contains(&(f & !1)), "{f:#x} must be inside LGT .text");
        }
        // LGT scratch: the page-rounding tail past the declared .bss end — past the
        // maximum literal+imm5 reach (0x15129d0), inside the mapped page (0x1513000).
        assert!((0x151_29d0..0x151_3000 - 3).contains(&lgt.scratch));
        assert_eq!(lgt.scratch % 4, 0);
        // ⚠ The slot offset is the third documented offset that did NOT port
        // (0x7c→0x78, 0xa384→0xa370, 0xa3f8→0xa410) — keep the builds apart.
        assert_eq!(lgt.slot_off, 0xa410);
        assert_ne!(ktf.slot_off, lgt.slot_off);
        // LGT record names are the .text strings themselves.
        assert!(matches!(lgt.names, SaveNames::Direct { game: 0x5_bbb4, slot: 0x5_bbc0 }));

        assert!(GuestBuild::Skt.save_chain().is_none());
    }

    /// LGT `.text` is `0x1000..0x1000+0x5db85` (01.00.07). Every LGT code site in the
    /// table has to land in it — a stray KTF address copied into an LGT arm would
    /// otherwise be written into whatever LGT has at that offset.
    fn in_lgt_text(a: u32) -> bool {
        (0x1000..0x1000 + 0x5db85).contains(&a)
    }

    /// The buff band's three x immediates: all `adds rN,#imm8` (low byte = the imm, so
    /// the table stores the opcode with a ZERO low byte), and the icon is the one that
    /// carries the +2 — patching it with the frame's constant is what tore the band
    /// apart the first time.
    #[test]
    fn band_x_sites_are_add_imm8_with_the_icon_two_across() {
        for b in [GuestBuild::Ktf, GuestBuild::Lgt] {
            let sites = b.band_x_sites();
            assert_eq!(sites.len(), 3, "{} must have frame + icon + number", b.name());
            for &(addr, opcode, delta) in sites {
                assert_eq!(opcode & 0x00ff, 0, "{addr:#x} opcode must carry no immediate");
                assert_eq!(opcode & 0xf800, 0x3000, "{addr:#x} must be a Thumb ADDS/SUBS imm8");
                assert_eq!(opcode & 0x0800, 0, "{addr:#x} must be ADDS, not SUBS");
                assert!(delta <= 2);
            }
            assert_eq!(sites.iter().map(|s| s.2).collect::<Vec<_>>(), vec![0, 2, 0]);
        }
        for &(addr, ..) in GuestBuild::Lgt.band_x_sites() {
            assert!(in_lgt_text(addr), "{addr:#x}");
        }
        assert!(GuestBuild::Skt.band_x_sites().is_empty());
    }

    /// The target box's immediates. The invariant that catches a mistyped constant is
    /// that the PRISTINE halfword and the stated signed base must encode each other:
    /// `items::target_box_x_patches` rewrites the site as `base - shift`, so a base that
    /// does not match the byte actually in the binary would silently move the box by the
    /// wrong amount on every site that disagreed.
    #[test]
    fn target_box_sites_encode_their_own_constants() {
        let enc = |v: i32| -> u16 {
            if v >= 0 {
                0x3100 | v as u16
            } else {
                0x3900 | (-v) as u16
            }
        };
        for b in [GuestBuild::Ktf, GuestBuild::Lgt] {
            let sites = b.target_box_x_sites();
            for &(addr, orig, base) in sites {
                assert_eq!(orig, enc(base), "{} {addr:#x}: halfword and base disagree", b.name());
                // Every site must still encode the maximum shift the app may ask for.
                assert!(
                    (-255..=255).contains(&(base - 211)),
                    "{} {addr:#x}: base {base} cannot encode a 211 px shift",
                    b.name()
                );
            }
            // No address may appear twice: the poke pass writes each one once and a
            // duplicate would mean a site is missing from the list instead.
            let mut addrs: Vec<u32> = sites.iter().map(|s| s.0).collect();
            addrs.sort_unstable();
            let n = addrs.len();
            addrs.dedup();
            assert_eq!(addrs.len(), n, "{} has a duplicated site", b.name());
        }
        // KTF = two byte-identical copies of six. LGT = two copies of four plus the two
        // HP-bar-fill draws its two paths SHARE (both reach 0x27e4).
        assert_eq!(GuestBuild::Ktf.target_box_x_sites().len(), 12);
        assert_eq!(GuestBuild::Lgt.target_box_x_sites().len(), 10);
        // ...and both builds' constants are the same multiset per copy, because it is the
        // same C: -2 frame, +1 bar bg, -44 name, +15 second run, +31/+31 bar fill.
        for b in [GuestBuild::Ktf, GuestBuild::Lgt] {
            let mut c: Vec<i32> = b.target_box_x_sites().iter().map(|s| s.2).collect();
            c.sort_unstable();
            c.dedup();
            assert_eq!(c, vec![-44, -2, 1, 15, 31], "{}", b.name());
        }
        for &(addr, ..) in GuestBuild::Lgt.target_box_x_sites() {
            assert!(in_lgt_text(addr), "{addr:#x}");
        }
        assert!(GuestBuild::Skt.target_box_x_sites().is_empty());
    }

    /// The quickbar page flip: the DATA writes are open on both measured builds, and only
    /// the guest CALL is per-build. Asserting them together is the point — the bug this
    /// replaced had one flag answering both questions, which turned "we have not located
    /// LGT's `skill_bind`" into "the page button does nothing".
    #[test]
    fn quickbar_writes_are_open_where_the_offsets_were_measured() {
        assert!(GuestBuild::Ktf.quickbar_writes_ok());
        assert!(GuestBuild::Lgt.quickbar_writes_ok());
        assert!(!GuestBuild::Skt.quickbar_writes_ok(), "nothing has been measured on SKT");

        assert_eq!(GuestBuild::Ktf.skill_bind_fn(), Some(0x14_5c48 | 1));
        assert_eq!(GuestBuild::Ktf.skill_bind_fn().unwrap() & 1, 1, "Thumb bit or the core enters in ARM state");
        assert_eq!(GuestBuild::Lgt.skill_bind_fn(), None, "unlocated: the raw-write path covers LGT");
        assert_eq!(GuestBuild::Skt.skill_bind_fn(), None);
    }

    /// The game's own "connection lost" modal. Everything asserted here is a relationship
    /// that a mistyped constant would break, because the numbers themselves are only as
    /// good as the disassembly they came out of — and this table's whole reason for
    /// existing is that 47차 shipped three theories the device refuted.
    ///
    /// The load-bearing one is the LAST assert: `state_word` and the four fields around
    /// it are `base + {0xa438, 0xa43c, 0xa7fc, 0xa7fd, 0xa3dc}` off `getBase()` =
    /// `0x1503ab0`, which is also `CHAR + 0x28`. So the modal addresses and the character
    /// anchor have to agree, and a digit dropped from any of them stops agreeing.
    #[test]
    fn net_disconnect_modal_addresses_agree_with_the_char_anchor() {
        let m = GuestBuild::Lgt.net_disconnect_modal().expect("located on LGT");
        let Some(CharAnchor::Static(char_base)) = GuestBuild::Lgt.char_anchor() else {
            panic!("LGT anchor shape changed");
        };
        // `getBase()` (guest 0x1068) returns CHAR + 0x28; every field is base + offset.
        let base = char_base + 0x28;
        assert_eq!(base, 0x150_3ab0, "the game's own accessor returns this");
        assert_eq!(m.state_word, base + 0xa438);
        assert_eq!(m.text_buf, base + 0xa43c);
        assert_eq!(m.panel_flag, base + 0xa7fc);
        assert_eq!(m.sub_flag, base + 0xa7fd);
        assert_eq!(m.clear_word, base + 0xa3dc);
        // The text buffer starts one word past the state word and is the memset width
        // `showModal` uses; a larger cap would run into `0xa53c`+ and a smaller one would
        // truncate messages the game itself fits (the 56-byte idle-timeout string).
        assert_eq!(m.text_buf, m.state_word + 4);
        assert_eq!(m.text_cap, 0x100);
        // Modal 5 is the one whose OK key runs netTeardown and leaves the screen. 7 is
        // the plain dismissible box 47차-C used, and the device showed it goes nowhere;
        // 6 swallows every key forever (`0xbe64` is `movs r0,#1; bx lr`).
        assert_eq!(m.disconnect_state, 5);
        assert_ne!(m.disconnect_state, 7, "type 7 only closes itself — that was 47차-C");
        assert_ne!(m.disconnect_state, 6, "modal 6 eats every key = soft lock");
        assert_ne!(m.disconnect_state, 0, "0 means no modal at all");
        // Every field is inside `.bss` (0x1500000 + 0x12958 on Ver 01.00.07), including
        // the far end of the text buffer.
        let bss = 0x150_0000..0x150_0000 + 0x1_2958;
        for a in [m.state_word, m.text_buf, m.panel_flag, m.sub_flag, m.clear_word] {
            assert!(bss.contains(&a), "{a:#x} must be in .bss");
        }
        assert!(bss.contains(&(m.text_buf + m.text_cap as u32 - 1)), "the buffer must fit in .bss");

        assert!(GuestBuild::Ktf.net_disconnect_modal().is_none(), "not located on KTF (the APK ships LGT)");
        assert!(GuestBuild::Skt.net_disconnect_modal().is_none());
    }

    /// The writes are `showModal(text, 5, 1, 1)` as data. What matters is that the state
    /// word lands LAST (before it, the tables still index the old modal; after it, every
    /// field the drawer reads is already there) and that the text is NUL-terminated —
    /// the drawer prints a C string, so a buffer with no terminator runs off the end.
    #[test]
    fn net_disconnect_writes_are_show_modal_with_the_state_last() {
        let m = GuestBuild::Lgt.net_disconnect_modal().unwrap();
        let w = m.writes(NET_DISCONNECT_TEXT_EUCKR);
        assert_eq!(w.len(), 5, "text + panel flag + sub flag + cleared word + state");

        let (addr, bytes) = &w[w.len() - 1];
        assert_eq!(*addr, m.state_word, "the state word must be written LAST");
        assert_eq!(bytes.as_slice(), &5u32.to_le_bytes());

        // The text buffer write is the whole memset width, text at the front, NUL after.
        let (addr, buf) = &w[0];
        assert_eq!(*addr, m.text_buf);
        assert_eq!(buf.len(), m.text_cap);
        assert_eq!(&buf[..NET_DISCONNECT_TEXT_EUCKR.len()], NET_DISCONNECT_TEXT_EUCKR);
        assert!(buf[NET_DISCONNECT_TEXT_EUCKR.len()..].iter().all(|&b| b == 0), "zero-filled tail");

        // Both flags are one byte and both are 1 — a 0 panel flag draws the words with
        // no panel behind them, straight onto the map.
        assert_eq!(w[1], (m.panel_flag, vec![1u8]));
        assert_eq!(w[2], (m.sub_flag, vec![1u8]));
        assert_eq!(w[3], (m.clear_word, vec![0u8; 4]));

        // Nothing may be written twice: each address here is a different field, and a
        // duplicate would mean one of them is missing.
        let mut addrs: Vec<u32> = w.iter().map(|(a, _)| *a).collect();
        let n = addrs.len();
        addrs.sort_unstable();
        addrs.dedup();
        assert_eq!(addrs.len(), n);
    }

    /// A message longer than the buffer must be truncated with room left for the NUL,
    /// never written to the last byte — that is the difference between a clipped
    /// sentence and the text renderer walking off the end of `.bss`.
    #[test]
    fn net_disconnect_text_is_always_nul_terminated() {
        let m = GuestBuild::Lgt.net_disconnect_modal().unwrap();
        let long = vec![0x41u8; m.text_cap * 2];
        let (_, buf) = m.writes(&long).into_iter().next().unwrap();
        assert_eq!(buf.len(), m.text_cap);
        assert_eq!(buf[m.text_cap - 1], 0, "the last byte is the terminator");
        assert_eq!(buf[..m.text_cap - 1], long[..m.text_cap - 1]);
        // An empty message is legal (the drawer just skips the text) and must not panic.
        let (_, empty) = m.writes(&[]).into_iter().next().unwrap();
        assert!(empty.iter().all(|&b| b == 0));
    }

    /// The notice text, in the encoding the guest's font renderer takes. The bytes are
    /// checked against the game's OWN strings rather than eyeballed: `끊어졌습니다.` is the
    /// tail of `0x5c5b4` ("서버와의 접속이 끊어졌습니다.") and `연결이 ` sits inside
    /// `0x5c13c` ("서버와 연결이 종료되었습니다."), both read out of `binary.mod`.
    #[test]
    fn net_disconnect_text_is_euckr_the_game_itself_uses() {
        assert_eq!(NET_DISCONNECT_TEXT_EUCKR.len(), 29);
        assert!(!NET_DISCONNECT_TEXT_EUCKR.contains(&0), "no embedded NUL — it is not a C string yet");
        // 끊어졌습니다.
        assert!(NET_DISCONNECT_TEXT_EUCKR.ends_with(&[
            0xb2, 0xf7, 0xbe, 0xee, 0xc1, 0xb3, 0xbd, 0xc0, 0xb4, 0xcf, 0xb4, 0xd9, 0x2e
        ]));
        // 연결이 (+ the space), the run the game's 0x5c13c also carries.
        assert!(NET_DISCONNECT_TEXT_EUCKR
            .windows(7)
            .any(|w| w == [0xbf, 0xac, 0xb0, 0xe1, 0xc0, 0xcc, 0x20]));
        // Every non-ASCII byte is a EUC-KR lead/trail byte (0xa1..0xfe), i.e. the text
        // never picked up a stray UTF-8 byte on its way into the table.
        assert!(NET_DISCONNECT_TEXT_EUCKR.iter().all(|&b| b == 0x20 || b == 0x2e || (0xa1..=0xfe).contains(&b)));
    }

    /// Auto-loot. The regression this defends is not a wrong number, it is a number that
    /// was only ever right for ONE build while the code asked a blanket question: the
    /// forcing path used to be gated on `guest_code_addresses_valid()` (KTF only) with a
    /// hardcoded `0x2d7`, so on the shipped LGT build it silently did nothing — and the
    /// server had delisted the `오토루팅` cash item *because* the host was supposed to be
    /// doing it. Two features, one flag, and the player got neither.
    ///
    /// ★ 50차 rewrote the answer: it is an ARRAY at `CHAR+0x2d7` indexed by the SAVE SLOT
    /// (`CHAR+0xa410`, the word `SaveChain::slot_off` already names), and the three
    /// "refuted candidates" were elements 0, 1 and 6 of it. The LGT `useItem` arm
    /// `0x5523c` computes `base + *(CHAR+0xa410) + 0x2af` and the reader `0x3ac02`
    /// recomputes the identical address, so both halves are pinned from both sides.
    ///
    /// These asserts encode the three things that each cost a session when they were
    /// wrong: the base is shared with KTF (not one-off per build), the index exists on
    /// LGT and IS the save slot, and the value must have bit 7 because the reader
    /// sign-extends the byte and branches on `< 0`.
    #[test]
    fn autoloot_flag_is_a_slot_indexed_array_with_a_bit7_value() {
        let ktf = GuestBuild::Ktf.autoloot_flag().expect("KTF located");
        let lgt = GuestBuild::Lgt.autoloot_flag().expect("LGT located");
        assert_eq!(GuestBuild::Skt.autoloot_flag(), None, "nothing measured on SKT");

        // Element 0 is the same place on both builds — KTF's documented `CHAR+0x2d7` is
        // simply the slot-0 case, which is why the two ever looked like different answers.
        assert_eq!(ktf.base_off, 0x2d7);
        assert_eq!(lgt.base_off, 0x2d7);
        assert_eq!(lgt.slot_off, Some(0xa410), "the save slot is what picks the element");
        assert_eq!(ktf.slot_off, None, "KTF has only ever been seen on slot 0");

        // ★ The value is a BIT, not a boolean. The LGT reader does
        // `ldrb; lsls #24; asrs #24; cmp #0; bge skip`, so `1` can never arm it.
        assert_eq!(lgt.value, 0x81);
        assert_ne!(lgt.value & 0x80, 0, "bit 7 or the reader's signed test never fires");
        assert_eq!(ktf.value, 0x01, "KTF's own arm stores a plain 1");

        // The slot index must agree with the save chain's — they are the same word, and
        // letting them drift would put the flag in another character's element.
        assert_eq!(lgt.slot_off, Some(GuestBuild::Lgt.save_chain().unwrap().slot_off));

        // Indexing: no slot word yet must NOT silently mean element 0, and an out-of-range
        // word must be refused rather than written somewhere arbitrary in the record.
        assert_eq!(lgt.offset_for(Some(0)), Some(0x2d7));
        assert_eq!(lgt.offset_for(Some(1)), Some(0x2d8), "what 12차 measured on device");
        assert_eq!(lgt.offset_for(Some(6)), Some(0x2dd), "one of the three 'refuted' bytes");
        assert_eq!(lgt.offset_for(Some(AutolootFlag::MAX_SLOT)), Some(0x2d7 + 15));
        assert_eq!(lgt.offset_for(Some(AutolootFlag::MAX_SLOT + 1)), None, "past the array");
        assert_eq!(lgt.offset_for(None), None, "unreadable slot must refuse, not default to 0");
        // KTF is unindexed, so it answers the same however the slot reads.
        assert_eq!(ktf.offset_for(None), Some(0x2d7));
        assert_eq!(ktf.offset_for(Some(9)), Some(0x2d7));

        for b in [GuestBuild::Ktf, GuestBuild::Lgt, GuestBuild::Skt] {
            if let Some(f) = b.autoloot_flag() {
                assert!(b.char_anchor().is_some(), "{b:?}: a flag with no record to apply it to");
                let last = f.offset_for(Some(AutolootFlag::MAX_SLOT)).unwrap_or(f.base_off);
                assert!(last < 0xa800, "{b:?}: {last:#x} is past the end of the record");
                assert_ne!(f.value, 0, "{b:?}: writing 0 would be the OFF state");
            }
        }
    }

    // ── 영웅서기2 (2026-09-09) ─────────────────────────────────────────────────
    //
    // A second GAME in the table, not a second carrier. What is worth asserting is
    // exactly the set of differences that would be a silent bug if collapsed — each of
    // these was a real hazard the moment `emulator.rs` started calling `set_hero2_ktf`.

    /// hero2 reaches CHAR through a **Java object**, so the anchor is neither a pointer
    /// global nor a base to read fields off. Treating it as `Pointer` would take one hop
    /// instead of two and land on the object header; treating it as `Static` would read
    /// the static slot itself as a character record.
    #[test]
    fn hero2_reaches_char_through_a_java_object() {
        match GuestBuild::Hero2Ktf.char_anchor() {
            Some(CharAnchor::JavaObject { slot }) => assert_eq!(slot, 0x0019_3b5c),
            _ => panic!("hero2's anchor must be a JavaObject"),
        }
        // ...and no other build may claim that shape: their records are not Java objects.
        for b in [GuestBuild::Ktf, GuestBuild::Lgt, GuestBuild::Skt] {
            assert!(
                !matches!(b.char_anchor(), Some(CharAnchor::JavaObject { .. })),
                "{b:?} is not a Java-object build"
            );
        }
    }

    /// The gauge layout is per build, and the two traps in it are:
    ///   * hero3 stores current HP as a HALFWORD and hero2 as an `int`. A word read at
    ///     hero3's `+0xf2` splices `+0xf4` in as the high half.
    ///   * hero2 has a real max-SP FIELD; hero3's max SP is the AddSP clamp constant.
    #[test]
    fn char_layouts_differ_in_width_and_in_whether_max_sp_is_a_field() {
        let h3 = GuestBuild::Ktf.char_layout().expect("hero3 KTF layout");
        let h2 = GuestBuild::Hero2Ktf.char_layout().expect("hero2 layout");

        assert_eq!(h3.hp_bytes, 2, "hero3's current HP is a halfword (CHAR+0xf2)");
        assert_eq!(h2.hp_bytes, 4, "hero2 stores it in an int");

        assert_eq!(h3.sp_max, None, "hero3 clamps SP to a constant, it has no field");
        assert_eq!(h3.sp_max_const, 1000);
        assert_eq!(h2.sp_max, Some(0x7c), "hero2 carries max SP in the record");

        // The hero2 offsets the game's own STATUS screen confirmed. If one of these
        // moves, the HUD draws another field's number and nothing says so.
        assert_eq!(
            (h2.level, h2.hp, h2.hp_max, h2.sp, h2.exp, h2.exp_next),
            (0x5c, 0x6c, 0x78, 0x70, 0x74, 0x80),
            "HERO2_GUEST_RE.md §2.1: LV 48 / HP 777·780 / SP 390·390 / EXP 195·112128"
        );
        // hero2's gauges are one word apart in the order the record declares them. A
        // transposition (hp<->sp, exp<->hp_max) is the classic HUD bug, and this is the
        // cheapest guard against re-introducing it.
        assert!(h2.hp < h2.sp && h2.sp < h2.exp && h2.exp < h2.hp_max && h2.hp_max < h2.exp_next);

        // ★ 2026-09-18: hero2's unspent points are located — `a.c:S` / `a.d:S`, the two
        // slots between the level and HP (HERO2_GUEST_RE.md §3.2) — and the skill count
        // is a SHORT there where hero3 keeps a byte.
        assert_eq!(h2.points, Some((0x64, 0x68)));
        assert!(h2.level < 0x64 && 0x68 < h2.hp, "between the level and HP");
        assert_eq!(h2.skill_points_bytes, 2);
        assert_eq!(h3.points, Some((0x18a, 0x18c)));
        assert_eq!(h3.skill_points_bytes, 1);
        assert_eq!(h3.point_caps, (276, 44), "hero3's addStatPoints / addSkillPoints clamps");

        // Every build that resolves a record must know how to read it, and vice versa.
        for b in [GuestBuild::Ktf, GuestBuild::Lgt, GuestBuild::Skt, GuestBuild::Hero2Ktf] {
            assert_eq!(
                b.char_anchor().is_some(),
                b.char_layout().is_some(),
                "{b:?}: an anchor with no layout (or a layout with nothing to apply it to)"
            );
        }
    }

    /// ⛔ The correction this session had to make to `HERO2_FEATURE_PARITY.md` §9.3,
    /// encoded so it cannot be un-made: hero2's state word is NOT in the character
    /// record and its values are NOT hero3's.
    #[test]
    fn hero2_state_word_is_an_absolute_static_with_its_own_values() {
        match GuestBuild::Hero2Ktf.app_state().expect("hero2 app_state") {
            AppState::Absolute { addr, field, window, shell, in_world, bottom_overlay, script } => {
                assert_eq!(addr, 0x0019_3aec);
                assert_eq!(script, &[4, 12], "dialogue and ending both use the cutscene HUD");
                assert_eq!(field, 3, "3 = FIELD on hero2, where hero3 uses 4");
                assert_eq!(window, 5);
                assert_eq!(shell, &[1, 2], "1 = slot select, 2 = title (hero3 folds both into 3)");
                assert_eq!(in_world, &[4, 8], "4 = NPC dialogue, 8 = NPC service window (device 2026-09-17)");
                assert_eq!(bottom_overlay, &[4], "only the dialogue box is bottom-anchored");
            }
            AppState::CharRelative { .. } => panic!("hero2's state word is not CHAR-relative"),
        }
        for b in [GuestBuild::Ktf, GuestBuild::Lgt] {
            let st = b.app_state().expect("hero3 app_state");
            assert!(matches!(st, AppState::CharRelative { .. }), "{b:?}");
            assert_eq!(st.field(), 4);
            assert_eq!(st.shell(), &[3]);
        }
        // The helpers the mode gates actually call.
        let h2 = GuestBuild::Hero2Ktf.app_state().unwrap();
        assert!(h2.in_game(3) && h2.in_game(5));
        assert!(h2.in_game(4) && h2.in_game(8), "dialogue and NPC window keep the plates");
        assert!(!h2.in_game(0) && !h2.in_game(1) && !h2.in_game(2));
        assert!(h2.bottom_overlay(4) && !h2.bottom_overlay(3) && !h2.bottom_overlay(5) && !h2.bottom_overlay(8));
        for b in [GuestBuild::Ktf, GuestBuild::Lgt] {
            let st = b.app_state().unwrap();
            assert!(!st.bottom_overlay(4) && !st.in_game(8), "hero3 reserves no rows and has no state 8: {b:?}");
        }
        assert!(h2.is_shell(1) && h2.is_shell(2) && !h2.is_shell(3));
        // The script screen is what `Mode::Cutscene` follows on hero2; hero3 answers it
        // from its window id instead (`None` here).
        assert_eq!(h2.is_script(4), Some(true));
        assert_eq!(h2.is_script(12), Some(true), "ending hides menu controls and the status HUD");
        assert!(!h2.in_game(12), "ending does not enable field actions");
        assert!(!h2.is_shell(12), "ending retains cutscene tap-to-advance");
        assert_eq!(h2.is_script(3), Some(false));
        assert_eq!(h2.is_script(5), Some(false));
        assert_eq!(GuestBuild::Ktf.app_state().unwrap().is_script(4), None);
        // The choice byte + its cursor are hero2 table entries (bb.b:B / bb.d:Z).
        let t = GuestBuild::Hero2Ktf.hero2_java().expect("hero2 table");
        assert_eq!(t.script_mode_slot, 0x0019_3d8c);
        assert_eq!(t.script_choice_sel_slot, 0x0019_3e3c);
        // ★ hero3's field value `4` is NOT hero2's field: on hero2 it is the NPC dialogue
        // (device, 2026-09-17) — in-world, but never free-roam. The values are per build
        // and not shared: sharing them would have made a dialogue read as the field.
        assert_ne!(h2.field(), 4, "4 is the dialogue on hero2, not the field");
    }

    /// hero2 says "field vs window" with the state word alone, so it needs no window-id
    /// byte — and must not be handed hero3's, which is an unrelated field there.
    #[test]
    fn only_the_hero3_record_has_the_extra_field_gate_bytes() {
        for b in [GuestBuild::Ktf, GuestBuild::Lgt] {
            let g = b.field_gate().expect("hero3 field gate");
            assert_eq!((g.window_id_off, g.window_none, g.selector_off), (0xa0c0, 3, 0x9c6c));
            assert!(b.char_record_is_hero3());
        }
        assert_eq!(GuestBuild::Hero2Ktf.field_gate(), None);
        assert_eq!(GuestBuild::Skt.field_gate(), None);
        assert!(!GuestBuild::Hero2Ktf.char_record_is_hero3());
        assert!(!GuestBuild::Skt.char_record_is_hero3());
        // The flag and the gate are the same claim, so they must never disagree.
        for b in [GuestBuild::Ktf, GuestBuild::Lgt, GuestBuild::Skt, GuestBuild::Hero2Ktf] {
            assert_eq!(b.char_record_is_hero3(), b.field_gate().is_some(), "{b:?}");
        }
    }

    /// ★★ The one place hero2 opens something hero3 keeps shut, and the reason is
    /// geometric rather than a matter of taste: hero2's HUD strip has no world under it.
    #[test]
    fn only_hero2_lets_the_host_paint_over_the_games_own_hud() {
        assert_eq!(GuestBuild::Hero2Ktf.guest_hud_band(), Some((194, 24)));
        // 194 + 24 = 218 has to stay inside the 220-row logical frame, or the mask would
        // be clipped and leave a sliver of the strip showing.
        let (y, h) = GuestBuild::Hero2Ktf.guest_hud_band().unwrap();
        assert!(y + h <= 220, "the band must fit the logical frame");
        for b in [GuestBuild::Ktf, GuestBuild::Lgt, GuestBuild::Skt] {
            assert_eq!(b.guest_hud_band(), None, "{b:?}: its box sits ON the world");
        }
    }

    /// ⛔ THE regression guard for this whole session: opening hero2's data anchors must
    /// not open one hero3 guest ADDRESS. Every accessor whose value came from hero3
    /// reversing has to answer closed on `Hero2Ktf`.
    #[test]
    fn hero2_opens_no_hero3_guest_address() {
        let b = GuestBuild::Hero2Ktf;
        assert!(b.popup_handles().is_none(), "cash/popup label array");
        // ★ 2026-09-17: the commit is OPEN — but it is the address-free kind. hero2's
        // popup is driven by a cursor byte + a synthesised OK (`PopupCommit::SyntheticOk`),
        // and its rewrite goes through `Hero2Java` + `java_write_ok`, none of which is a
        // hero3 address. A `GuestCall` here would be a regression.
        assert_eq!(b.popup_commit(), Some(PopupCommit::SyntheticOk), "hero2 commits by synthetic OK only");
        assert!(b.popup_preempt_offsets().is_empty());
        // ★ 2026-09-18: the bottom-HUD skip is OPEN on hero2 — but it is hero2's OWN
        // site (`n.d()V`, an AOT Java method), not either hero3 address.
        // ★ 2026-09-25: no longer an early return at the entry — a HEAD + TAIL branch pair
        // around the target-monster panel (the widget the host keeps). Their encodings are
        // pinned in `hero2_hud_patch_keeps_only_the_target_monster_panel`.
        let (site, orig, patched) = b.hud_bottom_patch().expect("hero2's own HUD painter");
        assert_eq!(site, 0x0016_39a0, "n.d()V, right after `fp = ax.i − 26` — HERO2_GUEST_RE.md §4");
        assert_eq!(orig, 0x9007, "str r0,[sp,#0x1c]");
        assert_eq!(patched & 0xf800, 0xe000, "an unconditional Thumb b, size-neutral");
        for h3 in [GuestBuild::Ktf, GuestBuild::Lgt] {
            assert_ne!(site, h3.hud_bottom_patch().unwrap().0, "not a hero3 address");
            assert!(h3.viewport_full_patch().is_none(), "hero3 has no viewport constant to patch");
            assert!(h3.hud_bottom_tail().is_none(), "hero3's one branch already lands on its epilogue");
        }
        // The viewport `− 26` → `− 0` (2026-09-18): `subs r1, #imm8` keeps its opcode
        // byte and only the immediate changes, so the write is size-neutral by shape.
        let (vsite, vorig, vpatched) = b.viewport_full_patch().expect("hero2's n.<init> constant");
        assert_eq!(vsite, 0x0016_24ec);
        assert_eq!(vorig & 0xff00, 0x3900, "subs r1, #imm8");
        assert_eq!(vorig & 0xff, 26, "the HUD band's height");
        assert_eq!(vpatched, 0x3900, "subs r1, #0");
        assert_eq!(b.hud_reserved_rows(), 0, "nothing to reserve once the viewport is the framebuffer");
        assert!(b.warehouse_grade_gate().is_none());
        assert!(b.exp_divisor_site().is_none());
        assert!(b.level_up_fn().is_none());
        assert!(b.equip_relief_site().is_none());
        assert!(b.band_x_sites().is_empty());
        // ★ 2026-09-25: the target box's x IS open on hero2 — its one site in `n.d()V`,
        // not any of hero3's twelve/ten (pinned in `hero2_target_box_is_one_r7_site`).
        for &(addr, ..) in b.target_box_x_sites() {
            for h3 in [GuestBuild::Ktf, GuestBuild::Lgt] {
                assert!(h3.target_box_x_sites().iter().all(|s| s.0 != addr), "{addr:#x} is a hero3 target-box site");
            }
        }
        assert!(b.skill_bind_fn().is_none());
        assert!(b.autoloot_flag().is_none());
        // ★★ 2026-09-19: hero2 SAVES, and `save_chain()` is still `None` — those are not
        // in tension. hero3's chain is three raw ARM addresses; hero2's save is one
        // JVM call by NAME (`ag.l()V`), so there is no hero3 address to open and
        // nothing for this field to hold. If a `SaveChain` ever appears here it means
        // someone tried to express the Java call as hero3 addresses, which is the
        // mistake this file exists to catch.
        assert!(b.save_chain().is_none(), "hero2 saves by name, not by hero3's three addresses");
        assert!(b.net_disconnect_modal().is_none());
        assert!(b.minimap_noblend_global().is_none());
        // A hero3-shaped quickbar WRITE (four parallel arrays in CHAR) stays shut: hero2's
        // bar is `ag.j`, reached through `Hero2Java` and gated by `java_write_ok`.
        assert!(!b.quickbar_writes_ok());
        // ...while the three read-only axes that WERE measured are open.
        assert!(b.char_anchor().is_some() && b.char_layout().is_some() && b.app_state().is_some());
        assert_eq!(b.name(), "hero2-KTF");
    }

    /// ★★ hero2's host save (2026-09-19) — one JVM call, hero2's own fields, and NOT a
    /// hero3 address anywhere.
    ///
    /// The point of this test is the pairing: `hero2_save()` is `Some` while
    /// `save_chain()` is `None`, on the same build. A future reader who "tidies" that
    /// by filling in a `SaveChain` would be turning a name-resolved call back into
    /// guessed addresses on a build where none have been measured.
    #[test]
    fn hero2_save_is_by_name_and_hero2_only() {
        for b in [GuestBuild::Ktf, GuestBuild::Lgt, GuestBuild::Skt] {
            assert!(b.hero2_save().is_none(), "{b:?} is not hero2");
        }
        let sv = GuestBuild::Hero2Ktf.hero2_save().expect("hero2 saves");
        // The receiver and the method are NAMES: nothing here is an address, so the
        // call survives a rebuild of the guest image.
        assert_eq!((sv.class, sv.field, sv.descriptor, sv.method), ("ag", "a", "Lag;", "l"));
        // The area gate's three numbers, measured off `az.a()Z` @0x120fc0.
        assert_eq!(sv.area.stage_slot, 0x0019_3afc, "ag.a:Lbf;");
        assert_eq!(sv.area.stage_type_off, 0x0, "bf.b:B");
        assert_eq!(sv.area.max_saveable_slot, 0x0019_3fbc, "bf.a:B");
        // Every one of them is in hero2's OWN static block, not hero3's CHAIN. The
        // hero3 builds' save chains live in a different image entirely; assert the
        // scalar addresses cannot be confused for them.
        for h3 in [GuestBuild::Ktf, GuestBuild::Lgt] {
            let c = h3.save_chain().expect("hero3 builds have a chain");
            for a in [sv.area.stage_slot, sv.area.max_saveable_slot] {
                assert_ne!(a, c.scratch, "not hero3 scratch");
                assert_ne!(a, c.fn_serialise & !1, "not a hero3 entry point");
            }
        }
    }

    /// ★★ #16 (2026-09-23): the COMBO's SP. The negation that made the cost a
    /// subtraction is dropped at the one `a.a(I)V` call — and (2026-09-24) put back on
    /// the SKILL branch that shares it, by negating the cost register `r7` before that
    /// branch jumps to the call. This checks the three encodings themselves: the first
    /// differs from shipped only in RSB vs ADD of the same registers, the other two keep
    /// `a.i` flowing into `r1` and negate `r7` in place. hero2-only: our image's addresses.
    #[test]
    fn the_combo_sp_patch_is_hero2_only_and_skills_keep_paying() {
        for b in [GuestBuild::Ktf, GuestBuild::Lgt, GuestBuild::Skt] {
            assert!(b.combo_sp_gain().is_empty(), "{b:?} must have no combo-SP patch");
        }
        let sites = GuestBuild::Hero2Ktf.combo_sp_gain();
        assert_eq!(sites.len(), 3, "the shared call + two halfwords on the skill branch");
        // `rsbs rd, rm, #0` = 0x4240 | rm<<3 | rd ; `adds rd, rm, #0` = 0x1c00 | rm<<3 | rd.
        const fn rsbs(rd: u16, rm: u16) -> u16 {
            0x4240 | (rm << 3) | rd
        }
        const fn adds(rd: u16, rm: u16) -> u16 {
            0x1c00 | (rm << 3) | rd
        }
        let (site, orig, patched) = sites[0];
        assert_eq!(site, 0x0014_f7e4, "the halfword after the a.a(I)V cell load");
        assert_eq!(orig, rsbs(5, 7), "shipped: rsbs r5, r7, #0");
        assert_eq!(patched, adds(5, 7), "patched: adds r5, r7, #0");
        // ⛔ Same registers, same width — the ONLY difference is the sign.
        assert_eq!(orig & 0x003f, patched & 0x003f, "the operands must not move");
        // The skill branch (0x14f7f6..0x14f82c) reaches the same call by `b 0x14f7e2` with
        // the cost in r7: `adds r5,r0,#0 · adds r1,r5,#0` (r5 dead until the call rewrites
        // it) become `adds r1,r0,#0 · rsbs r7,r7,#0`, so the call sees r5 = -cost.
        assert_eq!(sites[1], (0x0014_f80c, adds(5, 0), adds(1, 0)), "skill branch: a.i still lands in r1");
        assert_eq!(sites[2], (0x0014_f80e, adds(1, 5), rsbs(7, 7)), "skill branch: the cost is negated in place");
        assert_eq!(sites[2].0 - sites[1].0, 2, "adjacent halfwords");
        // Nothing here touches the shared call's PC-relative literal load at 0x14f7e2.
        for (site, _, _) in sites {
            assert_ne!(*site, 0x0014_f7e2);
        }
    }

    /// ★ #17 (2026-09-24): the cooldown and skill-power patches are hero2-only.
    /// (#20's DROP WINDOW patch shared this test until 2026-09-25, when it was removed: it
    /// made NORMAL roll each enemy's HARDCORE/HELL gear slots — see `guest_anchors`.)
    #[test]
    fn the_skill_balance_patches_are_hero2_only() {
        for b in [GuestBuild::Ktf, GuestBuild::Lgt, GuestBuild::Skt] {
            assert!(b.skill_cooldown_half().is_empty(), "{b:?} must have no cooldown patch");
            assert!(b.skill_power_patch().is_none(), "{b:?} must have no skill-power redirect");
        }
        // ⛔ And nothing may re-open the drop window: the two immediates in `bd.q()V` that
        // pick NORMAL's slots 0·1·2 vs HARDCORE/HELL's 2·3·4 stay as shipped.
        for list in [
            GuestBuild::Hero2Ktf.skill_point_curve(),
            GuestBuild::Hero2Ktf.skill_hotkey_badges_off(),
            GuestBuild::Hero2Ktf.combo_sp_gain(),
            GuestBuild::Hero2Ktf.strength_training_sp_off(),
            GuestBuild::Hero2Ktf.counter_stance_attack(),
            GuestBuild::Hero2Ktf.counter_stance_duration(),
            GuestBuild::Hero2Ktf.buff_cooldown_minute(),
            GuestBuild::Hero2Ktf.skill_cooldown_half(),
            GuestBuild::Hero2Ktf.card_drop_off(),
            GuestBuild::Hero2Ktf.honor_penalty_off(),
            GuestBuild::Hero2Ktf.item_table_refresh(),
            GuestBuild::Hero2Ktf.skill_popup_cancel(),
            GuestBuild::Hero2Ktf.qty_box_wrap(),
            GuestBuild::Hero2Ktf.title_menu_no_network(),
            GuestBuild::Hero2Ktf.roar_whole_map(),
            GuestBuild::Hero2Ktf.rogue_skill_sp_cut(),
            GuestBuild::Hero2Ktf.agility_coefficients(),
            GuestBuild::Hero2Ktf.sword_wave_range(),
            GuestBuild::Hero2Ktf.knight_strength_coefficient(),
            GuestBuild::Hero2Ktf.knight_active_seven(),
            GuestBuild::Hero2Ktf.rage_shipped_off(),
            GuestBuild::Hero2Ktf.roar_cooldown_up(),
            GuestBuild::Hero2Ktf.blessing_aura_rework(),
            GuestBuild::Hero2Ktf.agility_stance_invuln(),
            GuestBuild::Hero2Ktf.iron_wall_rework(),
            GuestBuild::Hero2Ktf.beast_recovery_buff(),
            GuestBuild::Hero2Ktf.beast_int_weight(),
            GuestBuild::Hero2Ktf.beast_attack_sp_cut(),
            GuestBuild::Hero2Ktf.beast_attack_cooldown_cut(),
            GuestBuild::Hero2Ktf.beast_attack_power(),
            GuestBuild::Hero2Ktf.seal_top_tier(),
        ] {
            for (site, _, _) in list {
                assert!(
                    !(0x0012_8230..=0x0012_8238).contains(site),
                    "{site:#x}: the drop window (movs r4,#2 · adds r5,#3) must stay shipped"
                );
            }
        }
    }

    /// ★★ 2026-10-16: the 개수 box wrap — hero3 builds have none; on hero2 every site is inside `aq.a(II)Z`
    /// (0x11dcf0..0x11def8), the two new branches land on the UP clamp's `h ← aq.a:B` (0x11de20), and the LEFT path
    /// keeps its own shipped `ble` target nowhere.
    #[test]
    fn the_qty_box_wraps_onto_the_up_clamp() {
        for b in [GuestBuild::Ktf, GuestBuild::Lgt, GuestBuild::Skt] {
            assert!(b.qty_box_wrap().is_empty(), "{b:?} must have no 개수 box patch");
        }
        let sites = GuestBuild::Hero2Ktf.qty_box_wrap();
        assert_eq!(sites.len(), 21, "LEFT one branch · DOWN seven · RIGHT five · UP eight halfwords");
        // (the 일괄판매 rows the same asset run appends to /t/shop follow its 26 shipped rows, in SHOP_APPEND's order)
        assert_eq!(BulkSell::SHOP_ROWS_ASK, [BulkSell::SHOP_ROWS_SHIPPED, BulkSell::SHOP_ROWS_SHIPPED + 1, BulkSell::SHOP_ROWS_SHIPPED + 2]);
        assert_eq!(BulkSell::SHOP_ROW_NONE, BulkSell::SHOP_ROWS_SHIPPED + 3);
        assert_eq!((BulkSell::SHOP_ROW_SOLD, BulkSell::SHOP_ROW_NONE_SHIPPED), (21, 23), "/t/shop 판매하였습니다. · 판매할 아이템이 없습니다.");
        let target = |site: u32, hw: u16| -> u32 {
            // B<cond>: imm8 signed, ×2, from pc + 4.
            (site as i64 + 4 + 2 * i64::from((hw & 0xff) as u8 as i8)) as u32
        };
        for (site, orig, patched) in sites {
            assert!((0x0011_dcf0..0x0011_def8).contains(site), "{site:#x} is aq.a(II)Z");
            assert_ne!(orig, patched);
        }
        assert_eq!(target(0x0011_dda6, 0xdd3b), 0x0011_de20, "LEFT on 1 → the ceiling");
        assert_eq!(target(0x0011_dda6, 0xddd2), 0x0011_dd4e, "as shipped: LEFT on 1 → return");
        assert_eq!(target(0x0011_ddcc, 0xd028), 0x0011_de20, "DOWN on 1 → the ceiling");
        assert_eq!(target(0x0011_ddd0, 0xddbd), 0x0011_dd4e, "DOWN on 2..10 → return, as shipped");
        // `b` (imm11) from 0x11ddd6 rejoins the shipped `lsls r3,r4,#24` of the −10 path.
        let hw = 0xe002u16;
        assert_eq!(hw >> 11, 0b11100);
        assert_eq!(0x0011_ddd6 + 4 + 2 * u32::from(hw & 0x7ff), 0x0011_ddde);

        // ★★ UP / RIGHT on the ceiling → 1: both join the RIGHT path's shipped tail 0x11dd84 (`lsls r3,r4,#24` … putfield).
        let patched = |site: u32| sites.iter().find(|(s, _, _)| *s == site).map(|&(_, _, p)| p).unwrap();
        let b_to = |site: u32| -> u32 {
            // B (imm11) signed, ×2, from pc + 4.
            let hw = patched(site);
            assert_eq!(hw >> 11, 0b11100, "{site:#x} is an unconditional b");
            let imm = i64::from(((hw & 0x7ff) << 5) as i16 >> 5);
            (site as i64 + 4 + 2 * imm) as u32
        };
        const JOIN: u32 = 0x0011_dd84;
        // RIGHT: bge → `movs r4,#1`; else `adds r4,r6,#1` (r6 = h).
        assert_eq!(target(0x0011_dd74, patched(0x0011_dd74)), 0x0011_dd7a, "RIGHT on the ceiling → 1");
        assert_eq!(target(0x0011_dd74, 0xdaeb), 0x0011_dd4e, "as shipped: RIGHT on the ceiling → return");
        assert_eq!(patched(0x0011_dd76), 0x1c00 | 1 << 6 | 6 << 3 | 4, "adds r4,r6,#1");
        assert_eq!(patched(0x0011_dd7a), 0x2000 | 4 << 8 | 1, "movs r4,#1");
        assert_eq!(b_to(0x0011_dd78), JOIN);
        assert_eq!(b_to(0x0011_dd7c), JOIN);
        // UP: bge → `movs r4,#1`; else r4 = h + 10, ≤ ceiling → write it, over → the shipped clamp 0x11de20.
        assert_eq!(target(0x0011_de00, patched(0x0011_de00)), 0x0011_de0c, "UP on the ceiling → 1");
        assert_eq!(target(0x0011_de00, 0xdb17), 0x0011_de32, "as shipped: UP below the ceiling → +10");
        assert_eq!(patched(0x0011_de02), 0x1c00 | 6 << 3 | 4, "adds r4,r6,#0");
        assert_eq!(patched(0x0011_de04), 0x3000 | 4 << 8 | 10, "adds r4,#10");
        assert_eq!(patched(0x0011_de06), 0x4280 | 4, "cmp r4,r0");
        assert_eq!(patched(0x0011_de08) >> 8, 0xdd, "ble");
        assert_eq!(target(0x0011_de08, patched(0x0011_de08)), JOIN, "h + 10 ≤ ceiling → write it");
        assert_eq!(b_to(0x0011_de0a), 0x0011_de20, "over the ceiling → the shipped clamp");
        assert_eq!(patched(0x0011_de0c), 0x2000 | 4 << 8 | 1, "movs r4,#1");
        assert_eq!(b_to(0x0011_de0e), JOIN);
        // The clamp the LEFT / DOWN / UP branches land on, and the tail they join, are never written.
        for (site, _, _) in sites {
            assert!(!(0x0011_de20..0x0011_de32).contains(site), "{site:#x}: the shipped clamp stays");
            assert!(!(JOIN..0x0011_dd98).contains(site), "{site:#x}: the shipped putfield tail stays");
        }
    }

    /// ★ #17 (2026-09-24): the COOLDOWN halving. One shift amount per job, 16 -> 17, and
    /// the sites are the sign-extension right after each `<job>.a(I)S` short-array read.
    #[test]
    fn the_cooldown_halving_is_one_shift_per_job() {
        let sites = GuestBuild::Hero2Ktf.skill_cooldown_half();
        assert_eq!(sites.len(), 3, "ay · bl · bx");
        for (site, orig, patched) in sites {
            // ASR Rd, Rm, #imm5 = 0x1000 | imm << 6 | Rm << 3 | Rd, r0 -> r0.
            assert_eq!(*orig, 0x1000 | (16 << 6), "{site:#x}: shipped asrs r0, r0, #16");
            assert_eq!(*patched, 0x1000 | (17 << 6), "{site:#x}: patched asrs r0, r0, #17");
        }
        // Each is 0x1e past its body (push · mov · push · ldr · ldr · mov · add · adds · add
        // · bl · adds · bl · lsls — then the asrs), and the bodies are the three jobs'.
        let bodies = [0x0012_0c10u32, 0x0013_38c4, 0x0013_bd58];
        for ((site, _, _), body) in sites.iter().zip(bodies) {
            assert_eq!(site - body, 0x1e);
        }
        // And the skill-power defs are those jobs' OTHER override, `a(BILbd;)I`.
        let sp = GuestBuild::Hero2Ktf.skill_power_patch().unwrap();
        assert_eq!(sp.jobs.map(|(_, body, ..)| body), [0x0012_0c40, 0x0013_38f4, 0x0013_be3c]);
        assert_eq!(SkillPowerPatch::BLOCK_LEN, 3 * SkillPowerPatch::JOB_BLOCK);
    }

    /// ★ 2026-09-26 (user: "버서커의 근력단련스킬이 리뉴얼이 필요할거같아" → "스킬데미지 증가로
    /// 바꿀수도있어?"): 근력단련's SP term is zeroed by ONE immediate in the berserker's `bl.c()V`
    /// (`movs r0,#7` → `#0` ahead of `muls r4,r0,r4 ; … ; subs r5,r5,r4`), and only the
    /// berserker's skill-power tuple names the passive — `a.p[4 + 8]`, its 4 stages + sk0 스킬 8.
    #[test]
    fn strength_training_trades_its_sp_term_for_skill_damage_on_the_berserker_only() {
        for b in [GuestBuild::Ktf, GuestBuild::Lgt, GuestBuild::Skt] {
            assert!(b.strength_training_sp_off().is_empty(), "{b:?} must have no 근력단련 patch");
        }
        let sites = GuestBuild::Hero2Ktf.strength_training_sp_off();
        assert_eq!(sites.len(), 1, "one immediate");
        let (site, orig, patched) = sites[0];
        // Inside bl.c()V @0x13358c..0x133794 — the BERSERKER's job hook (class 0 = bl).
        assert!((0x0013_358c..0x0013_3794).contains(&site), "{site:#x}: bl.c()V");
        // MOVS Rd, #imm8 = 0x2000 | Rd << 8 | imm — same register, only the 7 goes.
        assert_eq!(orig, 0x2000 | 7, "shipped: movs r0, #7");
        assert_eq!(patched, 0x2000, "patched: movs r0, #0");
        assert_eq!(orig & 0xff00, patched & 0xff00, "the destination must not move");
        let sp = GuestBuild::Hero2Ktf.skill_power_patch().unwrap();
        // jobs[] = ay 로그 · bl 버서커 · bx 나이트, and only bl carries the passive.
        assert_eq!(sp.jobs.map(|(.., boost, _)| boost), [None, Some(12), None]);
        let (_, _, stages, boost, _) = sp.jobs[1];
        assert_eq!(boost, Some(stages + 8), "근력단련 = sk0 스킬 8 after the berserker's combo stages");
        assert_eq!(sp.skills_off, GuestBuild::Hero2Ktf.hero2_java().unwrap().char_skill_levels_off, "a.p");
        // 「레벨당 3%」 (user, 2026-09-26): 246 / 2^13 = 3.003 %, 3 levels.
        assert_eq!((SkillPowerPatch::BOOST_MUL, SkillPowerPatch::BOOST_SHIFT, SkillPowerPatch::BOOST_MAX_LEVEL), (246, 13, 3));
        let pct = f64::from(SkillPowerPatch::BOOST_MUL) / f64::from(1u32 << SkillPowerPatch::BOOST_SHIFT) * 100.0;
        assert!((pct - 3.0).abs() < 0.01, "{pct} % a level");
    }

    /// ★ 2026-09-26 (user: "그냥 반격시 나의 공격력만큼 반격데미지를 입히면돼" → "너가말한대로
    /// 적용하고"): 반격태세's case in `bl.a(BILbd;)I` becomes a VIRTUAL call of `a.a()I` on `this`
    /// scaled by the level factor the skill-power block hands in as `d`. Decoded the way the
    /// CPU runs it: the retargeted `bl` lands on the virtual-call helper, the factor comes
    /// from `fp` (the body's saved `d`), anything ≤ 255 is exactly 2048 (100 %), the product
    /// is shifted by the block's COUNTER_SHIFT into r6, the `b` lands on the common return
    /// (`adds r0, r6, #0`), and the load's literal names the cell `bl.h(B)Z` calls `a.a()I` by.
    #[test]
    fn the_counter_stance_returns_the_characters_own_attack() {
        for b in [GuestBuild::Ktf, GuestBuild::Lgt, GuestBuild::Skt] {
            assert!(b.counter_stance_attack().is_empty(), "{b:?} must have no 반격태세 patch");
        }
        let sites = GuestBuild::Hero2Ktf.counter_stance_attack();
        assert_eq!(sites.len(), 11);
        for w in sites[..10].windows(2) {
            assert_eq!(w[1].0, w[0].0 + 2, "one contiguous sequence 0x133996..=0x1339a8");
        }
        // After the call: mov r1, fp (0x4659) · cmp r1, #255 · bhi +2 · movs r1,#1 · lsls r1,r1,#11
        // · muls r0, r1 · asrs r6, r0, #11 — then the b.
        assert_eq!(sites[2], (0x0013_399a, 0x4641, 0x4659), "mov r1, r8 -> mov r1, fp: the factor");
        assert_eq!(sites[3].2, 0x2900 | 0xff, "cmp r1, #255");
        let bhi = sites[4];
        assert_eq!((bhi.0, bhi.2 & 0xff00), (0x0013_399e, 0xd800), "bhi");
        assert_eq!(bhi.0 + 4 + 2 * u32::from(bhi.2 & 0xff), 0x0013_39a4, "a real factor skips the 100 % default");
        assert_eq!(sites[5].2, 0x2101, "movs r1, #1");
        // LSL Rd, Rm, #imm5 = imm5 << 6 | Rm << 3 | Rd ; ASR = 0x1000 | … — both by COUNTER_SHIFT.
        let sh = u16::from(SkillPowerPatch::COUNTER_SHIFT);
        assert_eq!(sites[6].2, (sh << 6) | (1 << 3) | 1, "lsls r1, r1, #11 = 2048 = COUNTER_ONE");
        assert_eq!(1u32 << sh, SkillPowerPatch::COUNTER_ONE);
        assert_eq!(sites[7], (0x0013_39a4, 0x250e, 0x4340 | (1 << 3)), "muls r0, r1: attack × factor");
        assert_eq!(sites[8].2, 0x1000 | (sh << 6) | 6, "asrs r6, r0, #11");
        // The shipped halfwords at the eight rewritten slots are the old `a.p[8]·14 + 20` code.
        assert_eq!(sites[2..10].iter().map(|s| s.1).collect::<Vec<_>>(), [0x4641, 0xf035, 0xf822, 0x0600, 0x1604, 0x250e, 0x436c, 0x1c26]);
        // Thumb-1 BL: hi = 0xf000 | off[22:12], lo = 0xf800 | off[11:1], target = at + 4 + off.
        let bl = |at: u32, hi: u16, lo: u16| {
            let mut off = (u32::from(hi & 0x7ff) << 12) | (u32::from(lo & 0x7ff) << 1);
            if off & 0x40_0000 != 0 {
                off |= 0xff80_0000;
            }
            at.wrapping_add(4).wrapping_add(off)
        };
        let (a, h, l) = (sites[0].0, sites[0], sites[1]);
        assert_eq!(l.0, a + 2, "the two halves of one bl");
        assert_eq!(bl(a, h.1, l.1), 0x0016_80a4, "shipped: the instance-field GET helper (a.p)");
        assert_eq!(bl(a, h.2, l.2), 0x0016_917c, "patched: the VIRTUAL call helper (a.a()I)");
        // B = 0xe000 | imm11, target = at + 4 + 2 * sext(imm11).
        let (at, _orig, b) = sites[9];
        assert_eq!(at, 0x0013_39a8);
        let imm = i32::from(b & 0x7ff) - if b & 0x400 != 0 { 0x800 } else { 0 };
        assert_eq!(b & 0xf800, 0xe000);
        assert_eq!((at as i32 + 4 + 2 * imm) as u32, 0x0013_3950, "the common return: adds r0, r6, #0");
        // The case's own ldr at 0x133990 (4837 = ldr r0, [pc, #0xdc]) reads the literal.
        assert_eq!(((0x0013_3990u32 + 4) & !3) + 0x37 * 4, sites[10].0, "the case-8 ldr's literal");
        // sl-relative cell offsets: 0x195ca4 + 0xfffe_50a4 = 0x17ad48 (a.p) -> + 0xfffe_4f90 = 0x17ac34.
        let cell = |lo: u16| 0x0019_5ca4u32.wrapping_add(0xfffe_0000 | u32::from(lo));
        assert_eq!(cell(sites[10].1), 0x0017_ad48, "shipped: this site's own a.p cell");
        assert_eq!(cell(sites[10].2), 0x0017_ac34, "patched: bl.h(B)Z's a.a()I cell");
        // The factor the block hands in: its counter skill is this case (kind 8).
        let sp = GuestBuild::Hero2Ktf.skill_power_patch().unwrap();
        assert_eq!(sp.jobs[1].4, Some(8), "bl's counter skill = 반격태세 = a.p[4 + 4]");
    }

    /// ★ 2026-09-26 (user: "지속시간도 1레벨을 6초고 5렙찍으면 20초까지"): the buff that holds the
    /// stance is registered with `70·lv + 50` frames instead of 120 — the two immediates of
    /// the dead `a.p:I ← lv·14 + 20` become 70 and 50 and the buff call takes that `r4`.
    /// ★ 2026-10-06 (버프 쿨 1분 · 만렙 75초 · ×1.25): the immediates are 250 and 250 — 25 → 75 s.
    #[test]
    fn the_counter_stance_lasts_twenty_five_seconds_at_level_one_and_seventy_five_at_five() {
        for b in [GuestBuild::Ktf, GuestBuild::Lgt, GuestBuild::Skt] {
            assert!(b.counter_stance_duration().is_empty(), "{b:?} must have no 반격태세 length patch");
        }
        let sites = GuestBuild::Hero2Ktf.counter_stance_duration();
        // MOVS Rd,#imm = 0x2000 | Rd<<8 | imm ; ADDS Rdn,#imm = 0x3000 | Rdn<<8 | imm ; ADDS Rd,Rn,#0 = 0x1c00 | Rn<<3 | Rd.
        assert_eq!(sites[0], (0x0013_30cc, 0x2300 | 14, 0x2300 | 250), "movs r3, #14 -> #250");
        assert_eq!(sites[1], (0x0013_30d4, 0x3400 | 20, 0x3400 | 250), "adds r4, #20 -> #250");
        assert_eq!(sites[2], (0x0013_30e8, 0x2300 | 120, 0x1c00 | (4 << 3) | 3), "movs r3, #120 -> adds r3, r4, #0");
        // Inside bl.a()Z (the berserker's action driver, entry 8 = 0x13309c .. 0x1330fa).
        for (site, _, _) in sites {
            assert!((0x0013_309c..0x0013_30fc).contains(site), "{site:#x}");
        }
        // 20 game frames a second (the game's own 50 ms step).
        let secs = |lv: u32| f64::from(250 * lv + 250) / 20.0;
        assert_eq!([1, 2, 3, 4, 5].map(secs), [25.0, 37.5, 50.0, 62.5, 75.0]);
    }

    /// ★ 2026-10-06: the buff cooldowns at one minute — `bl.a[3]`/`[4]` (분노태세 · 반격태세) through the one register
    /// both store, and the beast's `bn.a(B)I` for kinds 1 · 4 (빛의수호 · 빛의회복) through kind 4's case; 5 얼음장막's
    /// case and every other kind's entry stay as shipped.
    #[test]
    fn the_buff_cooldowns_are_a_minute_and_leave_the_ice_wall_alone() {
        for b in [GuestBuild::Ktf, GuestBuild::Lgt, GuestBuild::Skt] {
            assert!(b.buff_cooldown_minute().is_empty(), "{b:?} must have no buff cooldown patch");
        }
        let s = GuestBuild::Hero2Ktf.buff_cooldown_minute();
        assert_eq!(s.len(), 5);
        // bl.<clinit>: `movs r4,#imm ; … ; lsls r4,r4,#n` → bl.a[3] = bl.a[4] = imm << n, halved by #17's `asrs #17`.
        let (imm, sh) = (s[0].2 & 0xff, (s[1].2 >> 6) & 0x1f);
        assert_eq!((s[0].0, s[0].1, s[0].2 >> 8), (0x0013_3b92, 0x2482, 0x24), "movs r4,#130 -> movs r4,#imm");
        assert_eq!((s[1].0, s[1].1, s[1].2 & 0xf83f), (0x0013_3b9e, 0x0064, 0x0024), "lsls r4,r4,#1 -> lsls r4,r4,#n");
        assert_eq!((u32::from(imm) << sh) / 2, 1200, "분노태세 · 반격태세: 60 s after the halving");
        // bn.a(B)I kind 4: `movs r0,#imm ; lsls r0,r0,#n ; b return` — NOT halved (the beast's own table).
        let (imm, sh) = (s[2].2 & 0xff, (s[3].2 >> 6) & 0x1f);
        assert_eq!((s[2].0, s[2].1, s[2].2 >> 8), (0x0013_551e, 0x2082, 0x20));
        assert_eq!((s[3].0, s[3].1, s[3].2 & 0xf83f), (0x0013_5520, 0x0040, 0x0000));
        assert_eq!(u32::from(imm) << sh, 1200, "빛의회복 (and 빛의수호 through it): 60 s");
        // The jump table @0x170634 (entries relative to the table): kind 1's entry moves from its own case (245) to
        // kind 4's; the high halfword 0xfffc is the same for both targets.
        let table = 0x0017_0634u32;
        let target = |lo: u16| table.wrapping_add(0xfffc_0000 | u32::from(lo));
        assert_eq!(s[4].0, table + 4, "kind 1 = 빛의수호");
        assert_eq!((target(s[4].1), target(s[4].2)), (0x0013_5510, 0x0013_551e));
        // 얼음장막 (kind 5): its entry at table + 20 and its case 0x135524 (`movs r0,#230`) are not touched.
        for (site, _, _) in s {
            assert!(*site != table + 20 && !(0x0013_5524..0x0013_5528).contains(site), "{site:#x}: 얼음장막 is left out");
        }
    }

    /// ★ 2026-09-24 (user: "카드 드랍이 되면 안돼"): the SPELL-CARD roll, off. One
    /// compare immediate in `bd.q()V` — the roll is `h.a(1,100)` ∈ 1..=100, so a compare
    /// against 100 before a `bgt` is "never". hero2-only: it is our image's address.
    #[test]
    fn the_card_drop_patch_is_hero2_only_and_only_raises_the_compare() {
        for b in [GuestBuild::Ktf, GuestBuild::Lgt, GuestBuild::Skt] {
            assert!(b.card_drop_off().is_empty(), "{b:?} must have no card-drop patch");
        }
        let sites = GuestBuild::Hero2Ktf.card_drop_off();
        assert_eq!(sites.len(), 1, "bd.q()V has ONE card roll");
        let (site, orig, patched) = sites[0];
        assert_eq!(site, 0x0012_8482, "the compare after the h.a(1,100) card roll");
        // CMP r0, #imm8 = 0x2800 | imm — the instruction stays a compare of r0; only
        // the immediate moves, from the shipped 89 to the unreachable 100.
        assert_eq!(orig, 0x2800 | 89, "shipped: cmp r0,#89 (the 11 % card branch)");
        assert_eq!(patched, 0x2800 | 100, "patched: cmp r0,#100 — a 1..=100 roll never exceeds it");
    }

    /// ★ 2026-09-26 (user: "버서커로 사자후 스킬 사용하면 오류로 중단됨" + 「맵 전체」): the
    /// roar's scan window becomes the map. Fourteen CONTIGUOUS halfwords that are one new
    /// instruction sequence, so this decodes them the way the CPU will — where each
    /// PC-relative load lands in the routine's own literal pool, where each `bl` and the
    /// closing `b` go — and checks the shipped side decodes to the screen-width read it
    /// replaces. A typo here is a jump into the middle of `a.e(B)V`, not a wrong number.
    #[test]
    fn the_roar_scan_is_the_whole_map_and_rejoins_the_games_own_loop() {
        for b in [GuestBuild::Ktf, GuestBuild::Lgt, GuestBuild::Skt] {
            assert!(b.roar_whole_map().is_empty(), "{b:?} must have no roar patch");
        }
        let all = GuestBuild::Hero2Ktf.roar_whole_map();
        assert_eq!(all.len(), 14 + 10, "the scan in a.e(B)V, then the dying cut in bd.f()V");
        let (sites, dying) = all.split_at(14);
        const START: u32 = 0x0015_195e; // right after `str r0,[sp,#0x20]` (the stage)
        for (i, (site, _, _)) in sites.iter().enumerate() {
            assert_eq!(*site, START + 2 * i as u32, "contiguous, in order");
        }
        let new: Vec<u16> = sites.iter().map(|s| s.2).collect();
        let old: Vec<u16> = sites.iter().map(|s| s.1).collect();
        // LDR Rd,[pc,#imm8·4]: base = (pc + 4) & !3.
        let ldr_pool = |site: u32, hw: u16| {
            assert_eq!(hw & 0xf800, 0x4800, "{site:#x}: must be ldr rX,[pc,#imm]");
            assert_eq!((hw >> 8) & 7, 0, "{site:#x}: into r0 — the helper's argument");
            ((site + 4) & !3) + u32::from(hw & 0xff) * 4
        };
        // BL: two halfwords, offset = sign-extended (hi[10:0] << 12 | lo[10:0] << 1).
        let bl = |site: u32, hi: u16, lo: u16| {
            assert_eq!(hi & 0xf800, 0xf000, "{site:#x}: bl high half");
            assert_eq!(lo & 0xf800, 0xf800, "{site:#x}: bl low half");
            let off = ((i32::from(hi & 0x7ff) << 21) >> 9) | (i32::from(lo & 0x7ff) << 1);
            (site as i32 + 4 + off) as u32
        };
        // The routine's own pool entries (0x151b04.. — `a.e(B)V` ends at 0x151b48), whose
        // literals `sl + lit` are the cells: n.a 0x17ed8c · bf.d 0x17ed9c · bf.e 0x17eda4.
        const POOL_BF_D: u32 = 0x0015_1b14;
        const POOL_BF_E: u32 = 0x0015_1b18;
        const POOL_N_A: u32 = 0x0015_1b04;
        const STATIC_READ: u32 = 0x0016_7e84;
        // Patched: w = bf.d → [sp,#0x1c] · h = bf.e → [sp,#0x18] · sx = 0 → [sp,#0x14] ·
        // sy = 0 → fp · b to the loop set-up.
        assert_eq!(ldr_pool(START, new[0]), POOL_BF_D, "w reads the map WIDTH");
        assert_eq!(new[1], 0x4450, "add r0, sl");
        assert_eq!(bl(START + 4, new[2], new[3]), STATIC_READ);
        assert_eq!(new[4], 0x9000 | (0x1c / 4), "str r0,[sp,#0x1c] — the loop's column bound");
        assert_eq!(ldr_pool(START + 10, new[5]), POOL_BF_E, "h reads the map HEIGHT");
        assert_eq!(new[6], 0x4450, "add r0, sl");
        assert_eq!(bl(START + 14, new[7], new[8]), STATIC_READ);
        assert_eq!(new[9], 0x9000 | (0x18 / 4), "str r0,[sp,#0x18] — the loop's row bound");
        assert_eq!(new[10], 0x2000, "movs r0,#0");
        assert_eq!(new[11], 0x9000 | (0x14 / 4), "str r0,[sp,#0x14] — sx = 0");
        // MOV (high): 0100 0110 H1 H2 Rm Rd → fp = r11 (H1 = 1, Rd = 3), Rm = r0.
        assert_eq!(new[12], 0x4683, "mov fp, r0 — sy = 0");
        let b = |site: u32, hw: u16| {
            assert_eq!(hw & 0xf800, 0xe000, "{site:#x}: must be an unconditional b");
            (site as i32 + 4 + (((hw & 0x7ff) as i32) << 21 >> 21) * 2) as u32
        };
        assert_eq!(b(START + 26, new[13]), 0x0015_19ec, "rejoins at the attack read before `i = 0`");
        // Shipped: `n.a` (the SCREEN width) through the same helper, then `/ 16` by the
        // idiv helper — the read the patch replaces with the map's own size.
        assert_eq!(ldr_pool(START, old[0]), POOL_N_A);
        assert_eq!(bl(START + 4, old[2], old[3]), STATIC_READ);
        assert_eq!(old[4], 0x2110, "movs r1,#16");
        assert_eq!(bl(START + 10, old[5], old[6]), 0x0016_95fc, "the idiv helper");
        // The three unchanged halfwords are the patched sequence's too, kept for the verify.
        assert_eq!(sites.iter().filter(|s| s.1 == s.2).count(), 3);

        // ── bd.f()V: the not-visible branch, ten halfwords from its first sight compare ──
        const CUT: u32 = 0x0012_6530;
        for (i, (site, _, _)) in dying.iter().enumerate() {
            assert_eq!(*site, CUT + 2 * i as u32, "contiguous, in order");
        }
        let d: Vec<u16> = dying.iter().map(|s| s.2).collect();
        // Bcond: 1101 cond imm8 → pc + 4 + sign-extended imm8 · 2.
        let bc = |site: u32, hw: u16, cond: u16| {
            assert_eq!(hw >> 12, 0xd, "{site:#x}: a conditional branch");
            assert_eq!((hw >> 8) & 0xf, cond, "{site:#x}: condition");
            (site as i32 + 4 + i32::from((hw & 0xff) as u8 as i8) * 2) as u32
        };
        const UPDATE: u32 = 0x0012_6582; // the shipped update path (bd.r countdown · k() · b() · n())
        const OUT_OF_SIGHT: u32 = 0x0012_6552; // `this.c == null` → return
        const IN_SIGHT: u32 = 0x0012_6562; // the 4-tile cut
        // LDR Rd,[Rn,#imm5·4] = 0110 1 imm5 Rn Rd: ptr_fields of `this` (r6), then the word at
        // +4+0x3c — `g.t`, where the probe reads 6 for a dying monster.
        assert_eq!(d[0], 0x6800 | (6 << 3) | 1, "ldr r1,[r6]");
        assert_eq!(d[1], 0x6800 | ((0x40 / 4) << 6) | (1 << 3) | 1, "ldr r1,[r1,#0x40] — g.t");
        assert_eq!(d[2], 0x2900 | 6, "cmp r1,#6 — dying");
        assert_eq!(bc(CUT + 6, d[3], 0x0), UPDATE, "beq: a dying monster always updates");
        // The shipped decision on the shipped values: r7 = dx, r8 = dy, r5 = the sight range.
        assert_eq!(d[4], 0x4280 | (5 << 3) | 7, "cmp r7,r5");
        assert_eq!(bc(CUT + 10, d[5], 0xc), OUT_OF_SIGHT, "bgt, as the shipped 0x126532");
        assert_eq!(d[6], 0x4644, "mov r4,r8 — the shipped 0x12654a");
        assert_eq!(d[7], 0x4280 | (5 << 3) | 4, "cmp r4,r5");
        assert_eq!(bc(CUT + 16, d[8], 0xd), IN_SIGHT, "ble, as the shipped 0x126550");
        assert_eq!(b(CUT + 18, d[9]), OUT_OF_SIGHT, "and the fall-through, now a branch");
        // Shipped: the first compare and the javac re-read of `this.a` it replaces.
        assert_eq!(dying[0].1, 0x4287, "cmp r7,r0");
        assert_eq!(bc(CUT + 2, dying[1].1, 0xc), OUT_OF_SIGHT);
    }

    /// ★ 2026-09-25 (user: "명예등급이 낮으면 장비 능력이 감소되는 기능 … 제거해야해"): the
    /// honour penalty lives in the two weapon getters, one `blt` each, and both become the
    /// canonical Thumb nop. The shipped branches are decoded back to the attenuation block
    /// right after them, so a typo'd site cannot pass as "a blt somewhere".
    #[test]
    fn the_honour_penalty_patch_is_hero2_only_and_nops_the_two_blts() {
        for b in [GuestBuild::Ktf, GuestBuild::Lgt, GuestBuild::Skt] {
            assert!(b.honor_penalty_off().is_empty(), "{b:?} must have no honour patch");
        }
        let sites = GuestBuild::Hero2Ktf.honor_penalty_off();
        assert_eq!(sites.len(), 2, "bo.c()S and bo.d()S — the only readers that turn 명예 into a number");
        let target = |site: u32, hw: u16| (site as i32 + 4 + ((hw & 0xff) as u8 as i8 as i32) * 2) as u32;
        // (site, the attenuation block the shipped blt reaches: `get bu.k; movs r5,#10 …`)
        for ((site, orig, patched), block) in sites.iter().zip([0x0013_610cu32, 0x0013_61ac]) {
            assert_eq!(orig & 0xff00, 0xdb00, "{site:#x}: shipped blt");
            assert_eq!(target(*site, *orig), block, "{site:#x}: the blt reaches the v*(10-k+h)/10 block");
            assert_eq!(*patched, 0x46c0, "{site:#x}: mov r8, r8 — the Thumb-1 nop, no flags");
        }
        // Inside the two getters' bodies (bo.c @0x1360c4, bo.d @0x136164), 0x3e past each —
        // the same offset, because the two are the same method twice.
        assert_eq!(sites[0].0 - 0x0013_60c4, 0x3e);
        assert_eq!(sites[1].0 - 0x0013_6164, 0x3e);
    }

    /// ★★ 2026-10-17 (user: "이름 가격 자동 갱신 진행하고 요구레벨도 표 따르게 해줘"): owned items load with the current item
    /// table's name, price and 요구레벨. The table parsers take their «fresh item» path for those three on the load path too
    /// (the price `beq` → nop, the name / equipment `bne` → `b` to the SAME target) and the loaders skip their stores (the name
    /// block jumps to the offset advance, the price · level putfield `bl`s become nops). Every branch is decoded here, so a
    /// typo'd site or target cannot pass.
    #[test]
    fn the_item_table_refresh_reads_the_table_on_load_and_skips_the_saved_copies() {
        for b in [GuestBuild::Ktf, GuestBuild::Lgt, GuestBuild::Skt] {
            assert!(b.item_table_refresh().is_empty(), "{b:?} must have no item refresh");
        }
        let s = GuestBuild::Hero2Ktf.item_table_refresh();
        assert_eq!(s.len(), 9);
        let cond = |site: u32, hw: u16| (site as i32 + 4 + ((hw & 0xff) as u8 as i8 as i32) * 2) as u32;
        let uncond = |site: u32, hw: u16| (site as i32 + 4 + ((((hw & 0x7ff) as i32) << 21) >> 20)) as u32;
        let at = |site: u32| *s.iter().find(|x| x.0 == site).unwrap_or_else(|| panic!("{site:#x} missing"));
        // the base parser: flag 1 false skipped the price (beq to the +4) — now it falls into the fresh path's `b 0x140e56`
        let (a, o, p) = at(0x0014_0d7c);
        assert_eq!((o & 0xff00, cond(a, o), p), (0xd000, 0x0014_0d80, 0x46c0));
        // … and the name: the shipped `bne` reaches the read (0x140e0c), the patched `b` the same place
        let (a, o, p) = at(0x0014_0d96);
        assert_eq!((o & 0xff00, cond(a, o), p & 0xf800, uncond(a, p)), (0xd100, 0x0014_0e0c, 0xe000, 0x0014_0e0c));
        // the equipment parser: the shipped `bne` reaches the body[1..=3] read (0x10d7aa), the patched `b` the same
        let (a, o, p) = at(0x0010_d792);
        assert_eq!((o & 0xff00, cond(a, o), p & 0xf800, uncond(a, p)), (0xd100, 0x0010_d7aa, 0xe000, 0x0010_d7aa));
        // the loader: len ≤ 0 nulled the name (0x14129e) — now it goes straight to the price read (0x141258), the length's
        // bytes are skipped with `adds r6, r6, r7` (0x141256) instead of copied into a new array
        let (a, o, p) = at(0x0014_120c);
        assert_eq!((o & 0xff00, cond(a, o), p & 0xff00, cond(a, p)), (0xdd00, 0x0014_129e, 0xdd00, 0x0014_1258));
        let (a, _o, p) = at(0x0014_120e);
        assert_eq!((p & 0xf800, uncond(a, p)), (0xe000, 0x0014_1256));
        // the putfield `bl`s (both halves) of the saved price and level → nops
        for (first, hi, lo) in [(0x0014_126eu32, 0xf026u16, 0xfec9u16), (0x0010_db90, 0xf05a, 0xfa38)] {
            assert_eq!(at(first), (first, hi, 0x46c0));
            assert_eq!(at(first + 2), (first + 2, lo, 0x46c0));
            // the shipped pair is a `bl` to the putfield helper 0x168004
            let off = ((((u32::from(hi) & 0x7ff) << 12) | ((u32::from(lo) & 0x7ff) << 1)) as i32) << 9 >> 9;
            assert_eq!((first as i32 + 4 + off) as u32, 0x0016_8004, "{first:#x}: putfield");
        }
    }

    /// ★ 2026-09-25 (user: "몹 잡을때 몹 이름이랑 피통이 안보이는데 hero3처럼 미니맵 왼쪽에"):
    /// the HUD painter keeps exactly one widget. The HEAD skips the badge blink straight to
    /// the target-panel test, the TAIL sends the panel's exit (and the idle arm) to the
    /// method's own epilogue, and the plan writes the tail FIRST — the head alone would run
    /// the panel and then fall into the notice block and the whole bottom HUD.
    #[test]
    fn hero2_hud_patch_keeps_only_the_target_monster_panel() {
        let b = GuestBuild::Hero2Ktf;
        let (head, head_orig, head_b) = b.hud_bottom_patch().unwrap();
        let (tail, tail_orig, tail_b) = b.hud_bottom_tail().expect("hero2's painter has a tail");
        // Unconditional Thumb `b`: pc + 4 + sign-extended imm11 · 2.
        let land = |site: u32, hw: u16| {
            assert_eq!(hw & 0xf800, 0xe000, "{site:#x}: must be an unconditional b");
            (site as i32 + 4 + (((hw & 0x7ff) as i32) << 21 >> 21) * 2) as u32
        };
        assert_eq!(land(head, head_b), 0x0016_39f6, "head -> the `n.e > 0 && n.a != null` panel test");
        assert_eq!(land(tail, tail_b), 0x0016_3f14, "tail -> `add sp,#0x24` — n.d()V's own epilogue");
        assert_eq!(tail, 0x0016_3a1e, "where the panel's far `bl` and the idle arm meet");
        assert_eq!(tail_orig, 0x48b9, "ldr r0,[pc,#0x2e4] — the notice block's first instruction");
        assert_eq!(head_orig, 0x9007);
        // The entry (`push`) is NOT a site any more: the painter must run to reach the panel.
        assert!(head != 0x0016_3964 && tail != 0x0016_3964);

        // The plan, against a fake image.
        let img = |cells: &[(u32, u16)]| {
            let cells: Vec<(u32, u16)> = cells.to_vec();
            move |a: u32, n: usize| -> Vec<u8> {
                cells.iter().find(|(c, _)| *c == a).map(|(_, v)| v.to_le_bytes()[..n.min(2)].to_vec()).unwrap_or_default()
            }
        };
        let hw = |w: &[(u32, [u8; 2])]| w.iter().map(|(a, b)| (*a, u16::from_le_bytes(*b))).collect::<Vec<_>>();
        // pristine: tail, THEN head
        assert_eq!(hw(&b.hud_bottom_writes(&img(&[(tail, tail_orig), (head, head_orig)]))), vec![(tail, tail_b), (head, head_b)]);
        // a retry after the tail landed: the head alone
        assert_eq!(hw(&b.hud_bottom_writes(&img(&[(tail, tail_b), (head, head_orig)]))), vec![(head, head_b)]);
        // done: nothing
        assert!(b.hud_bottom_writes(&img(&[(tail, tail_b), (head, head_b)])).is_empty());
        // a tail that is neither — another build's code, or unreadable: touch NOTHING
        assert!(b.hud_bottom_writes(&img(&[(tail, 0x1234), (head, head_orig)])).is_empty());
        assert!(b.hud_bottom_writes(&img(&[(head, head_orig)])).is_empty());
        // hero3's single-site builds still plan their one branch
        let (k_site, k_orig, k_b) = GuestBuild::Ktf.hud_bottom_patch().unwrap();
        assert_eq!(hw(&GuestBuild::Ktf.hud_bottom_writes(&img(&[(k_site, k_orig)]))), vec![(k_site, k_b)]);
        // and hero2's box geometry is its own image's: 66x28 (`createImage(0x42, 0x1c)`)
        assert_eq!(b.target_box_guest_size(), (66, 28));
        assert_eq!(GuestBuild::Ktf.target_box_guest_size(), (90, 30));
    }

    /// ★ 2026-09-25: hero2's target box moves by ONE immediate — `subs r7,#0x42` — because
    /// the name/level image and its HP bar both take their x from r7 (via r8). The planner
    /// has to keep r7: the hero3 encoder it replaced wrote every site as r1, which on this
    /// site would have left r7 = W and moved nothing while corrupting r1.
    #[test]
    fn hero2_target_box_is_one_r7_site() {
        let b = GuestBuild::Hero2Ktf;
        assert_eq!(b.target_box_x_sites(), &[(0x0016_40f2, 0x3f42, -66)]);
        assert_eq!(b.target_box_shift_max(), 255 - 66);
        assert_eq!(GuestBuild::Ktf.target_box_shift_max(), 255 - 44);
        assert_eq!(GuestBuild::Lgt.target_box_shift_max(), 255 - 44);
        assert_eq!(GuestBuild::Skt.target_box_shift_max(), 0, "no sites, no shift");
        let at = |v: u16| move |a: u32, n: usize| if a == 0x0016_40f2 { v.to_le_bytes()[..n.min(2)].to_vec() } else { vec![] };
        let one = |w: Vec<(u32, [u8; 2])>| w.into_iter().map(|(a, b)| (a, u16::from_le_bytes(b))).collect::<Vec<_>>();
        assert_eq!(one(b.target_box_x_writes(&at(0x3f42), 50)), vec![(0x0016_40f2, 0x3f74)], "subs r7,#116");
        assert!(b.target_box_x_writes(&at(0x3f42), 0).is_empty(), "shift 0 on a pristine site writes nothing");
        assert_eq!(one(b.target_box_x_writes(&at(0x3f74), 10)), vec![(0x0016_40f2, 0x3f4c)], "our own encoding re-shifts");
        assert_eq!(one(b.target_box_x_writes(&at(0x3f74), 0)), vec![(0x0016_40f2, 0x3f42)], "and goes back home");
        assert_eq!(one(b.target_box_x_writes(&at(0x3f42), 189)), vec![(0x0016_40f2, 0x3fff)], "the ceiling encodes");
        assert!(b.target_box_x_writes(&at(0x3f42), 190).is_empty(), "past the ceiling: nothing, not half a box");
        assert!(b.target_box_x_writes(&at(0x3142), 50).is_empty(), "an add/sub on ANOTHER register is not ours");
        assert!(b.target_box_x_writes(&at(0x4770), 50).is_empty(), "foreign code is left alone");
        // hero3's r1 sites keep their register through the shared encoder (KTF +1 site).
        assert_eq!(GuestBuild::add_sub_imm8(0x3101, 1 - 5), Some(0x3904), "adds r1,#1 shifted 5 = subs r1,#4");
        assert_eq!(GuestBuild::add_sub_imm8(0x392c, -44 + 30), Some(0x390e));
        assert_eq!(GuestBuild::add_sub_imm8(0x3f42, 3), Some(0x3703), "a positive constant is adds r7");
        assert_eq!(GuestBuild::add_sub_imm8(0x3f42, -256), None);
    }

    /// ★ 2026-09-24 (S9 logcat): the shipped SKILL-window CANCEL crash. Three halfwords
    /// that turn `sel != 0 → detail` into a three-way split. A wrong imm8 here is a
    /// branch into the middle of `s.a(BB)V`, so both targets are decoded back and pinned
    /// to the blocks the shipped code already jumps to.
    #[test]
    fn the_skill_popup_cancel_fix_is_a_three_way_split_onto_existing_blocks() {
        for b in [GuestBuild::Ktf, GuestBuild::Lgt, GuestBuild::Skt] {
            assert!(b.skill_popup_cancel().is_empty(), "{b:?} must have no skill-popup patch");
        }
        let sites = GuestBuild::Hero2Ktf.skill_popup_cancel();
        assert_eq!(sites.len(), 3);
        // Where each conditional branch lands: `pc + 4 + sign-extended imm8 · 2`.
        let target = |site: u32, hw: u16| (site as i32 + 4 + ((hw & 0xff) as u8 as i8 as i32) * 2) as u32;
        // The cj (스킬상세정보) block and the epilogue — both reached by the SHIPPED code
        // already (the shipped `bne` and the level-up's `b 0x1482b0`), so nothing new is
        // jumped into.
        const DETAIL: u32 = 0x0014_82da;
        const EPILOGUE: u32 = 0x0014_82b0;

        // 1: the shipped `bne` becomes `bgt` to the SAME block — B<cond> = 0xd000 | c<<8.
        let (s0, o0, p0) = sites[0];
        assert_eq!(s0, 0x0014_82c6);
        assert_eq!(o0 & 0xff00, 0xd100, "shipped: bne");
        assert_eq!(p0 & 0xff00, 0xdc00, "patched: bgt");
        assert_eq!(target(s0, o0), DETAIL, "the shipped bne reaches the cj block");
        assert_eq!(target(s0, p0), DETAIL, "the bgt must reach the same cj block");

        // 2: a dead `adds r5,r7,#0` becomes `cmp r3,#0` — r3 is still `sel` here (set by
        //    `mov r3, sb` at 0x1482c2, untouched by the ldr/adds/add/mov between).
        assert_eq!(sites[1], (0x0014_82d0, 0x1c00 | (7 << 3) | 5, 0x2800 | (3 << 8)));

        // 3: a dead `mov r4,r8` becomes `blt` to the epilogue.
        let (s2, o2, p2) = sites[2];
        assert_eq!(s2, 0x0014_82d2);
        assert_eq!(o2, 0x4644, "shipped: mov r4, r8 (high-register MOV)");
        assert_eq!(p2 & 0xff00, 0xdb00, "patched: blt");
        assert_eq!(target(s2, p2), EPILOGUE, "the cancel must land on the epilogue");

        // The PC-relative `ldr r0, [pc, #0x114]` at 0x1482c8 (the s.a(B)V cell) is NOT a
        // site: moving it would re-align its base.
        assert!(sites.iter().all(|(a, _, _)| *a != 0x0014_82c8), "the literal load must not move");
    }

    /// ★ 2026-09-24 (user: "메인 메뉴내에서 네트워크 창고라는 메뉴를 없애줘"): the title
    /// carousel drops to six stops and its jump table shifts entries 4·5 down one. Three
    /// kinds of site — a count, a range check, two table words, a label index — so this
    /// pins each encoding, and for the table words that entry N takes EXACTLY the value
    /// entry N+1 shipped with (the case bodies themselves do not move).
    #[test]
    fn the_title_menu_patch_shifts_the_jump_table_down_one_and_shrinks_the_count() {
        for b in [GuestBuild::Ktf, GuestBuild::Lgt, GuestBuild::Skt] {
            assert!(b.title_menu_no_network().is_empty(), "{b:?} must have no title-menu patch");
        }
        let sites = GuestBuild::Hero2Ktf.title_menu_no_network();
        assert_eq!(sites.len(), 5);
        // MOVS r3,#imm8 = 0x2300 | imm — the ctor's stop count, 7 -> 6, the ONE place
        // the count lives (the shared `j.c(B)V` wraps against `j.i`).
        assert_eq!(sites[0], (0x0014_95e2, 0x2300 | 7, 0x2300 | 6));
        // CMP r0,#imm8 = 0x2800 | imm — the OK handler's range check follows the count.
        assert_eq!(sites[1], (0x0014_96cc, 0x2800 | 6, 0x2800 | 5));
        // The 7-word jump table @0x17093c holds base-relative entries whose high
        // halfwords are all 0xfffd, so only the LOW halfword of words 4 and 5 moves.
        const JT: u32 = 0x0017_093c;
        assert_eq!(sites[2].0, JT + 4 * 4, "word 4's low halfword");
        assert_eq!(sites[3].0, JT + 4 * 5, "word 5's low halfword");
        // Entry 4 takes entry 5's shipped value — a handler is inherited, never invented.
        assert_eq!(sites[2].2, sites[3].1, "stop 4 must take the handler stop 5 shipped with");
        // Decoded back to targets (base + sign-extended entry): the shipped stop 4 was
        // the 네트워크창고 case, and the patched stops are the shipped 정보이용료 and
        // 게임종료 cases of `t.a(II)Z` — measured, not derived.
        let target = |lo: u16| JT.wrapping_add(0xfffd_0000 | u32::from(lo));
        assert_eq!(target(sites[2].1), 0x0014_9786, "shipped stop 4 = the 네트워크창고 case");
        assert_eq!(target(sites[2].2), 0x0014_97fa, "patched stop 4 = the 정보이용료 case");
        assert_eq!(target(sites[3].2), 0x0014_9820, "patched stop 5 = the 게임종료 case");
        // MOVS r2,#imm8 = 0x2200 | imm — bh's header index follows its label, com[5] ->
        // com[4] (the asset moves the 정보이용료 text there; census: bh is the only
        // constant reader of #5 and nothing reads #4 or #6).
        assert_eq!(sites[4], (0x0012_e9ec, 0x2200 | 5, 0x2200 | 4));
        // No two halfword tables may claim the same site — a second write would undo
        // the first table's verify-then-write.
        let b = GuestBuild::Hero2Ktf;
        let mut all: Vec<u32> = sites.iter().map(|s| s.0).collect();
        for list in [
            GuestBuild::Hero2Ktf.skill_point_curve(),
            GuestBuild::Hero2Ktf.skill_hotkey_badges_off(),
            GuestBuild::Hero2Ktf.combo_sp_gain(),
            GuestBuild::Hero2Ktf.strength_training_sp_off(),
            GuestBuild::Hero2Ktf.counter_stance_attack(),
            GuestBuild::Hero2Ktf.counter_stance_duration(),
            GuestBuild::Hero2Ktf.buff_cooldown_minute(),
            GuestBuild::Hero2Ktf.skill_cooldown_half(),
            GuestBuild::Hero2Ktf.card_drop_off(),
            GuestBuild::Hero2Ktf.honor_penalty_off(),
            GuestBuild::Hero2Ktf.item_table_refresh(),
            GuestBuild::Hero2Ktf.skill_popup_cancel(),
            GuestBuild::Hero2Ktf.qty_box_wrap(),
            GuestBuild::Hero2Ktf.roar_whole_map(),
            GuestBuild::Hero2Ktf.rogue_skill_sp_cut(),
            GuestBuild::Hero2Ktf.agility_coefficients(),
            GuestBuild::Hero2Ktf.sword_wave_range(),
            GuestBuild::Hero2Ktf.knight_strength_coefficient(),
            GuestBuild::Hero2Ktf.knight_active_seven(),
            GuestBuild::Hero2Ktf.rage_shipped_off(),
            GuestBuild::Hero2Ktf.roar_cooldown_up(),
            GuestBuild::Hero2Ktf.blessing_aura_rework(),
            GuestBuild::Hero2Ktf.agility_stance_invuln(),
            GuestBuild::Hero2Ktf.iron_wall_rework(),
            GuestBuild::Hero2Ktf.beast_recovery_buff(),
            GuestBuild::Hero2Ktf.beast_int_weight(),
            GuestBuild::Hero2Ktf.beast_attack_sp_cut(),
            GuestBuild::Hero2Ktf.beast_attack_cooldown_cut(),
            GuestBuild::Hero2Ktf.beast_attack_power(),
            GuestBuild::Hero2Ktf.seal_top_tier(),
        ] {
            all.extend(list.iter().map(|s| s.0));
        }
        // ★ 2026-09-27: the combat block's halfwords (the hit guard's `bx r0`; ★ 2026-10-03 the wave's `bne → b`) too.
        all.push(b.combat_patch().unwrap().guard_arm.0);
        all.push(b.combat_patch().unwrap().wave_no_hit.0);
        // ★ 2026-10-05: 철괴's `bx r0` in the hit routine.
        all.push(b.combat_patch().unwrap().iron_arm.0);
        // ★ 2026-09-29: the 맹독감염 block's halfwords — the cast, the cooldown, the tick's `bx r0`.
        let venom = b.venom_patch().unwrap();
        all.extend(venom.cast.iter().chain(venom.cooldown.iter()).map(|s| s.0));
        all.push(venom.tick_arm.0);
        all.push(venom.effect_arm.0);
        // ★ 2026-09-30: the 야수진화제 sell price — the sale's and the painter's `bx r0`.
        let shop = b.shop_sell_patch().unwrap();
        all.push(shop.sell_arm.0);
        all.push(shop.paint_arm.0);
        // ★ 2026-09-30: the 의상 아바타 override — every halfword of its regions in cf.a(I)V
        // (★ A3b: six — head, weapon and off-hand too), and the four branches it retargets.
        for r in GuestBuild::Hero2Ktf.avatar_patch().unwrap().regions {
            all.extend((0..r.shipped.len() as u32).map(|i| r.at + 2 * i));
            all.extend(r.branch.map(|b| b.0));
        }
        // ★ 2026-09-25: and the painter's two branches and the target box's x site are
        // halfwords of their own too — no table may write them.
        all.extend(b.hud_bottom_patch().map(|s| s.0));
        all.extend(b.hud_bottom_tail().map(|s| s.0));
        all.extend(b.target_box_x_sites().iter().map(|s| s.0));
        // ★ 2026-10-05 일풍격: a.b()V's two halfwords and the painter's two direct calls (two halfwords each).
        let ilp = b.ilpung_patch().unwrap();
        all.extend(ilp.slots.iter().map(|s| s.0));
        for site in [ilp.paint_level.0, ilp.paint_max.0] {
            all.extend([site, site + 2]);
        }
        // ★ 2026-10-10 보스 치명: the boss hit routine's `bx r0` and both halfwords of its pool word.
        let crit = b.boss_crit_patch().unwrap();
        all.extend([crit.arm.0, crit.pool.0, crit.pool.0 + 2]);
        // ★ 2026-10-13 등급 줄: the info painter's grade draw — its `bx r0` and both halfwords of its pool word.
        let grade = b.grade_line_patch().unwrap();
        all.extend([grade.arm.0, grade.pool.0, grade.pool.0 + 2]);
        // ★★ 2026-10-21 보스 방어 관통: the hit routine's DEF and shield sites — two `bx r0`s and both halfwords of each pool word.
        let pierce = b.boss_pierce_patch().unwrap();
        all.extend([pierce.def_arm.0, pierce.def_pool.0, pierce.def_pool.0 + 2, pierce.shield_arm.0, pierce.shield_pool.0, pierce.shield_pool.0 + 2]);
        let n = all.len();
        all.sort_unstable();
        all.dedup();
        assert_eq!(all.len(), n, "two patch tables claim the same halfword");
    }

    /// ★★ 2026-10-10 보스 치명 (user: 「투지의 구 → 보스에게 데미지 0」, fix 「2」): the boss hit routine's crit pool
    /// word and arm are its own (no other host block arms them — the guardian level hook in the same routine
    /// sits at 0x116074 · pool 0x116328), hero2-only, and the rule keeps the shipped product above the weapon's.
    #[test]
    fn the_boss_crit_patch_is_hero2_only_and_arms_its_own_words() {
        for b in [GuestBuild::Ktf, GuestBuild::Lgt, GuestBuild::Skt] {
            assert!(b.boss_crit_patch().is_none(), "{b:?} must have no boss crit block");
        }
        let b = GuestBuild::Hero2Ktf;
        let crit = b.boss_crit_patch().unwrap();
        assert_eq!(crit.pool, (0x0011_636c, 0xfffe_0eb4));
        assert_eq!(crit.arm, (0x0011_61bc, 0x4641, 0x4700), "mov r1,r8 → bx r0");
        assert_eq!(crit.resume, 0x0011_61d2);
        assert!(crit.arm.0 < crit.pool.0 && crit.pool.0 - crit.arm.0 < 0x400, "the pool is ahead of its ldr");
        let shop = b.shop_sell_patch().unwrap();
        let vp = b.venom_patch().unwrap();
        let cp = b.combat_patch().unwrap();
        let mut words: Vec<u32> = vec![crit.pool.0, shop.sell_pool.0, shop.paint_pool.0, vp.strike.0, vp.tick_pool.0, vp.cast_pool.0, vp.effect_pool.0];
        // ★ 2026-10-13: the grade line's pool word, in the same painter as the sell price's.
        words.push(b.grade_line_patch().unwrap().pool.0);
        words.extend([cp.bludgeon_pool.0, cp.guard_pool.0, cp.iron_pool.0]);
        // The guardian level hooks (one of them in this same routine, 0x116074) and the gear crit read.
        words.extend(b.guardian_patch().unwrap().hooks.iter().map(|h| h.1));
        words.push(b.gear_patch().unwrap().crit_hook.1);
        let n = words.len();
        words.sort_unstable();
        words.dedup();
        assert_eq!(words.len(), n, "two host blocks arm the same pool word");
        // And the guardian hook's halfwords in ah.a are not ours: its `ldr` · `add r0,sl` · the `bl` pair.
        let g = b.guardian_patch().unwrap().hooks[3];
        assert_eq!(g.0, 0x0011_6074, "the boss's miss hook");
        assert!(!(g.0..g.0 + 10).contains(&crit.arm.0), "the miss hook and the crit arm overlap");
        // The rule (the probe's and the tests'): a 9 % berserker's 0 becomes the weapon's ×1.7, the rogue's ×6 stays.
        assert_eq!(BossCritPatch::shipped(200, 9), 0);
        assert_eq!(BossCritPatch::boss_crit(200, 9, 17), 340);
        assert_eq!(BossCritPatch::boss_crit(200, 15, 17), 340, "the shipped ×1 is below the weapon's ×1.7");
        assert_eq!(BossCritPatch::boss_crit(200, 20, 17), 400, "the shipped ×2 is above it — kept");
        assert_eq!(BossCritPatch::boss_crit(200, 69, 22), 1_200, "the rogue's ×6 — kept");
        assert_eq!(BossCritPatch::boss_crit(200, 90, 26), 1_800);
    }

    /// ★★ 2026-10-21 보스 방어 관통 (user: *"전체 보스들의 데미지중 … 20%를 방어무시데미지로"*): the hit routine's DEF read and
    /// shield call are armed by this block alone (the routine's other host arms — the hit guard 0x151c04, 철괴 0x151d6a, the
    /// guardian level hook 0x151d0a — sit elsewhere), hero2-only, and the rule is `max(R − DEF, R/5)`.
    #[test]
    fn the_boss_pierce_patch_is_hero2_only_and_arms_its_own_words() {
        for b in [GuestBuild::Ktf, GuestBuild::Lgt, GuestBuild::Skt] {
            assert!(b.boss_pierce_patch().is_none(), "{b:?} must have no boss pierce block");
        }
        let b = GuestBuild::Hero2Ktf;
        let p = b.boss_pierce_patch().unwrap();
        assert_eq!((p.def_pool, p.def_arm, p.def_resume), ((0x0015_1ef4, 0xfffe_9168), (0x0015_1ce4, 0x1c21, 0x4700), 0x0015_1cec));
        assert_eq!((p.shield_pool, p.shield_arm, p.shield_resume), ((0x0015_2098, 0xfffe_9180), (0x0015_2004, 0x1c31, 0x4700), 0x0015_200c));
        for (arm, pool) in [(p.def_arm.0, p.def_pool.0), (p.shield_arm.0, p.shield_pool.0)] {
            assert!(arm < pool && pool - arm < 0x400, "the pool is ahead of its ldr");
        }
        let cp = b.combat_patch().unwrap();
        let mut words: Vec<u32> = vec![p.def_pool.0, p.shield_pool.0, cp.guard_pool.0, cp.iron_pool.0, b.boss_crit_patch().unwrap().pool.0];
        words.extend(b.guardian_patch().unwrap().hooks.iter().map(|h| h.1));
        let n = words.len();
        words.sort_unstable();
        words.dedup();
        assert_eq!(words.len(), n, "two host blocks arm the same pool word");
        let g = b.guardian_patch().unwrap().hooks[2];
        assert_eq!(g.0, 0x0015_1d0a, "the monster's crit / status hook in the same routine");
        for arm in [p.def_arm.0, p.shield_arm.0] {
            assert!(!(g.0..g.0 + 10).contains(&arm), "the level hook and a pierce arm overlap");
        }
        assert_eq!(p.boss_descs, Hero2Java::BOSS_CLASS_DESCS);
        assert_eq!(BossPiercePatch::PIERCE_PCT, 20);
        // R 240 (the raid's NORMAL lap-1 attack) against a full build's 382: shipped 3 (the floor) → 48; a light defence as shipped.
        assert_eq!(240 - BossPiercePatch::defence(240, 382, 20), 48);
        assert_eq!(240 - BossPiercePatch::defence(240, 50, 20), 190);
        assert_eq!(BossPiercePatch::defence(240, 382, 0), 382, "0 % = shipped");
        assert_eq!(BossPiercePatch::shield(190, 300, 48), 142, "the shield stops the defended part only");
        assert_eq!(BossPiercePatch::shield(190, 300, 0), 300);
    }

    /// ★★ #18 (2026-09-23): the SKILL-POINT curve. Five halfwords that are ONE rewritten
    /// condition inside the level-up (`a.l()V`), so this checks the encodings themselves:
    /// a typo here is a branch into the middle of the routine, not a wrong number on a
    /// screen. hero3 builds must have none of it — these are hero2's own addresses.
    #[test]
    fn the_skill_point_curve_is_hero2_only_and_its_branches_land_where_the_shipped_ones_did() {
        for b in [GuestBuild::Ktf, GuestBuild::Lgt, GuestBuild::Skt] {
            assert!(b.skill_point_curve().is_empty(), "{b:?} must have no skill-point curve patch");
        }
        let sites = GuestBuild::Hero2Ktf.skill_point_curve();
        assert_eq!(sites.len(), 5, "the ten bytes of `movs r1,#2; bl; cmp r0,#1; beq`");

        // Contiguous halfwords, in order, starting at the `movs r1,#2`.
        const BASE: u32 = 0x0014_fba4;
        // The award block the shipped `beq` jumps to (`a.d:S += 1`).
        const AWARD: u32 = 0x0014_fbfe;
        for (i, (site, orig, patched)) in sites.iter().enumerate() {
            assert_eq!(*site, BASE + 2 * i as u32, "site {i} is not the {i}th halfword");
            assert_ne!(orig, patched, "site {site:#x} would be a no-op");
        }

        // What the game ships, so a re-measured image that disagrees fails loudly.
        let shipped: Vec<u16> = sites.iter().map(|(_, o, _)| *o).collect();
        assert_eq!(
            shipped,
            vec![0x2102, 0xf019, 0xfd05, 0x2801, 0xd027],
            "movs r1,#2 / bl 0x1695b4 / cmp r0,#1 / beq"
        );
        // The `bl` those middle two halfwords make, decoded back: the div/mod helper.
        let bl = ((shipped[1] as u32 & 0x7ff) << 12) | ((shipped[2] as u32 & 0x7ff) << 1);
        assert_eq!(BASE + 2 + 4 + bl, 0x0016_95b4, "the shipped call is not the div/mod helper");

        // And what we put there.
        let patched: Vec<u16> = sites.iter().map(|(_, _, p)| *p).collect();
        assert_eq!(patched[0], 0x2800 | 20, "cmp r0,#20 — the threshold the user asked for");
        assert_eq!(patched[2], 0x07c0, "lsls r0,r0,#31 — test bit 0");
        assert_eq!(patched[4], 0x46c0, "nop — fall through to the common tail");

        // ⛔ Both conditional branches must reach the SAME award block the shipped `beq`
        // reached. An off-by-one imm8 lands inside that block's literal load and the
        // game executes data.
        let branch_target = |site: u32, hw: u16| -> u32 {
            let imm = (hw & 0xff) as u8 as i8 as i32;
            (site as i32 + 4 + imm * 2) as u32
        };
        assert_eq!(patched[1] & 0xff00, 0xdd00, "halfword 1 must be `ble`");
        assert_eq!(branch_target(sites[1].0, patched[1]), AWARD, "the `ble` misses the award block");
        assert_eq!(patched[3] & 0xff00, 0xd100, "halfword 3 must be `bne`");
        assert_eq!(branch_target(sites[3].0, patched[3]), AWARD, "the `bne` misses the award block");
        // The shipped `beq` is what proves AWARD is where we think it is.
        assert_eq!(branch_target(sites[4].0, shipped[4]), AWARD, "the shipped beq does not go to AWARD");
    }

    /// ★★ 경험치 배율 (2026-09-23): the definition-record redirect of `a.d(I)V`. hero3
    /// builds must have none of it — a hero3 image has no method table at that address
    /// and its EXP path is the cash-shop divisor patch (`buffs.rs`), which is a
    /// different, time-limited thing. On hero2 the record is a whole number of
    /// definitions into the table and names the body the disassembler starts at.
    #[test]
    fn the_exp_rate_patch_is_hero2_only_and_names_the_gain_method() {
        for b in [GuestBuild::Ktf, GuestBuild::Lgt, GuestBuild::Skt] {
            assert!(b.exp_rate_patch().is_none(), "{b:?} must have no exp-rate patch");
        }
        let p = GuestBuild::Hero2Ktf.exp_rate_patch().expect("hero2's a.d(I)V");
        assert_eq!(p.body, 0x0014_fa64, "a.d(I)V @0x14fa64 (hero2_dis.py)");
        // 0x185644 + idx * 0x1c — `JavaMethodDefinition`s are 0x1c bytes (dump_hero2_java.py).
        assert_eq!(p.def, 0x0018_5644 + 906 * 0x1c, "def_fo 571740 + 0x100000");
        assert_eq!(p.body & 1, 0, "the body is stored even; the Thumb bit is added when written");
        assert_eq!(ExpRatePatch::MULT + 4, ExpRatePatch::BODY);
        assert_eq!(ExpRatePatch::BODY + 4, ExpRatePatch::BLOCK_LEN);
        // The probe's check calls the SAME method the redirect sits on.
        let sp = JavaCall::CharGainExp.spec();
        assert_eq!((sp.class, sp.method, sp.descriptor), ("a", "d", "(I)V"));
    }

    /// ★ 드랍률 (2026-10-16): the filter sits on `bf.a(BBBB)V`'s definition word — hero2 only, a whole number
    /// of definitions into the table, the body the disassembler starts at, the cell the HELL pass already calls
    /// through, and a word no other host block arms.
    #[test]
    fn the_drop_rate_filter_is_hero2_only_and_arms_its_own_word() {
        for b in [GuestBuild::Ktf, GuestBuild::Lgt, GuestBuild::Skt] {
            assert!(b.drop_rate_patch().is_none(), "{b:?} must have no drop-rate filter");
        }
        let b = GuestBuild::Hero2Ktf;
        let p = b.drop_rate_patch().expect("hero2's bf.a(BBBB)V");
        assert_eq!((p.def, p.body), (0x0018_83a8, 0x0012_d37c));
        assert_eq!(p.def, 0x0018_5644 + 415 * 0x1c, "def_fo 0x883a8 + 0x100000 (#415)");
        assert_eq!(p.body & 1, 0);
        assert_eq!(b.hell_drop_patch().unwrap().cell_put, 0x0017_9734, "the HELL pass drops through the same method's cell");
        assert!(DropRatePatch::EQUIP_MAX == 12 && DropRatePatch::BLOCK_LEN % 4 == 0);
        let sd = b.sky_draw_patch().unwrap();
        for other in [
            b.exp_rate_patch().unwrap().def,
            b.clear_hook_patch().unwrap().def,
            b.equip_cursor_patch().unwrap().def,
            b.cooldown_clock_patch().unwrap().def,
            b.hell_drop_patch().unwrap().death.0,
            sd.ground_draw.0,
            sd.hud_draw.0,
            b.venom_patch().unwrap().strike.0,
        ] {
            assert_ne!(p.def, other, "{other:#x} is another block's word");
        }
    }

    /// ★ 2026-10-01 의상 아바타 A4: the round-clear counter sits on `ag.y()V`'s definition word —
    /// hero2 only, a whole number of definitions into the table, and a word no other host block
    /// arms (the def words of the EXP, skill-power, combat, venom and cursor patches).
    #[test]
    fn the_clear_hook_is_hero2_only_and_arms_its_own_word() {
        for b in [GuestBuild::Ktf, GuestBuild::Lgt, GuestBuild::Skt] {
            assert!(b.clear_hook_patch().is_none(), "{b:?} must have no clear hook");
        }
        let b = GuestBuild::Hero2Ktf;
        let p = b.clear_hook_patch().expect("hero2's ag.y()V");
        assert_eq!((p.def, p.body, p.mode_slot), (0x0018_640c, 0x0011_4e60, 0x0019_3c4c));
        assert_eq!(p.def, 0x0018_5644 + 126 * 0x1c, "ag.y()V's definition (cell 0x176284 = 0x5400fd)");
        assert_eq!(p.body & 1, 0);
        assert_eq!((p.mode_slot - 0x0019_3860) % 0x10, 0x0c, "a static's value word: +12 of its definition");
        assert_eq!((ClearHookPatch::MODE_LIT, ClearHookPatch::BODY, ClearHookPatch::COUNT, ClearHookPatch::MODE, ClearHookPatch::BLOCK_LEN), (0x14, 0x18, 0x1c, 0x20, 0x24));
        let mut words = vec![p.def, b.exp_rate_patch().unwrap().def, b.equip_cursor_patch().unwrap().def, b.venom_patch().unwrap().strike.0];
        words.extend(b.skill_power_patch().unwrap().jobs.iter().map(|j| j.0));
        // ★ 2026-10-08 가디언: cq.a()V's definition word and the four hooked bd.n reads' pool words are its own too.
        let gp = b.guardian_patch().expect("hero2's guardian patch");
        assert_eq!(gp.template, (0x0018_ab78, 0x0014_53a0));
        assert_eq!(gp.template.0, 0x0018_5644 + 779 * 0x1c, "cq.a()V's definition (#779)");
        assert_eq!(gp.mode_slot, p.mode_slot, "the mode the clear hook reads");
        assert_eq!((gp.level_off, gp.hp_off, gp.atk_off, gp.def_off, gp.agi_off, gp.monster_level_off), (0x20, 0x24, 0x28, 0x2c, 0x34, 0x34 + 0x28 + 0x10));
        assert_eq!(gp.hooks.map(|h| h.0), [0x0012_7794, 0x0015_0994, 0x0015_1d0a, 0x0011_6074], "bd.n reads: the player's miss · evade · the monster's critical · the boss's miss");
        for (site, pool, shipped, _) in gp.hooks {
            assert_eq!(pool % 4, 0, "{pool:#x}: a pool word");
            assert!(pool > site + 8 && pool < site + 0x400, "{site:#x}: its pool word is ahead within an ldr's reach");
            assert!(shipped >= 0xfffe_0000, "{shipped:#x}: an sl-relative cell offset (negative)");
            words.push(pool);
        }
        words.push(gp.template.0);
        // ★ 2026-10-08 밤 (A1 · A3): the boss gate is cq.f:S; the kill-EXP hook is bd.q()V's a.d(I)V call site.
        assert_eq!(gp.gate_off, 0x44, "cq.f:S — the drop gate (dump_hero2_java.py --class cq)");
        let (site, pool, shipped) = gp.exp_hook;
        assert_eq!((site, pool, shipped), (0x0012_8120, 0x0012_82dc, 0xfffe_3b30), "bd.q()V: ldr r0,[pc,#0x1b8] → pool 0x1282dc = cell 0x1797d4 − sl");
        assert_eq!(((site + 4) & !3) + 0x1b8, pool, "the pool word is where the ldr reads");
        assert_eq!(shipped.wrapping_add(0x0019_5ca4), 0x0017_97d4, "sl + the word = the a.d(I)V cell");
        assert!(gp.hooks.iter().all(|h| h.0 != site), "no bd.n hook shares the site");
        words.push(pool);
        assert_eq!((GuardianPatch::BOSS_HP_MUL, GuardianPatch::KILL_EXP_MUL, GuardianPatch::WORDS), (307, 384, 18), "bosses ×1.2 · kill EXP ×1.5 (A1 · A3) + the gear pointer word + ★ (4) the evade-true word + ★ 2026-10-16 the three raid words + ★ 2026-10-17 the raid's map count");
        assert_eq!((GuardianPatch::W_EVADE_TRUE, GuardianPatch::EVADE_TRUE), (13, 1));
        // ★ 2026-10-16 보스 레이드: map 111 (tools_re/raid_gate.py) · LV 110 · HP and attack ×1.5 over guardian — and the map static the
        // wrapper reads is ag.h:B @0x193b4c (find_field_uses ag.h --desc B --static).
        // ★★ 2026-10-17: nine stage maps 113..121 (raid_gate.py STAGES) — the first map and the count.
        assert_eq!((GuardianPatch::W_RAID_MAP, GuardianPatch::W_RAID_LEVEL, GuardianPatch::W_RAID_MUL, GuardianPatch::W_RAID_COUNT), (14, 15, 16, 17));
        assert_eq!(&GuardianPatch::words()[14..], &[113, 110, 384, 9]);
        assert_eq!((GuardianPatch::raid_stage(112), GuardianPatch::raid_stage(113), GuardianPatch::raid_stage(121), GuardianPatch::raid_stage(122)), (None, Some(0), Some(8), None));
        assert_eq!((gp.map_slot, Hero2Java::MAP_SLOT), (0x0019_3b4c, 0x0019_3b4c));
        // ★★ 2026-10-17: the read piece's raid test reads the LIVE stage (ag.a:Lbf; · bf.b:B) — ag.h is −1 once a map is up.
        assert_eq!((gp.stage_slot, gp.stage_map_off), (b.hero2_save().unwrap().area.stage_slot, b.hero2_save().unwrap().area.stage_type_off));
        assert_eq!((GuardianPatch::W_BOSS_HP_MUL, GuardianPatch::W_KILL_EXP_MUL, GuardianPatch::W_GEAR_PTR), (10, 11, 12));
        assert_eq!(GuardianPatch::words()[GuardianPatch::W_GEAR_PTR], 0, "no gear block until its install");
        // ★ gear options (31, 2026-10-09): its definition words are the ones other blocks wrap (chained), its hook word the
        //   combat block's, a.m()V's word and the crit read's pool word nobody else's.
        let gear = b.gear_patch().expect("hero2's gear patch");
        assert_eq!(gear.stats, (0x0018_b9cc, 0x0014_fd00), "a.m()V");
        assert_eq!((gear.stats.0 - 0x0018_5644) % 0x1c, 0);
        words.push(gear.stats.0);
        assert_eq!(gear.crit_hook, (0x0012_7c46, 0x0012_7cdc, 0xfffe_390c), "bd.a(La;BIZBBBB)Z's first crit roll reads a.l:I here");
        assert!(gp.hooks.iter().all(|h| h.0 != gear.crit_hook.0 && h.1 != gear.crit_hook.1), "not a guardian hook site");
        assert_ne!(gear.crit_hook.1, gp.exp_hook.1);
        assert_eq!(gear.buff.0, b.buff_refresh_patch().unwrap().def, "g.a(BI)V — chained after the buff-refresh block");
        assert_eq!(gear.sp.map(|s| s.0), [b.berserker_patch().unwrap().sp_cost.0, b.combat_patch().unwrap().sp_cost.0, b.ilpung_patch().unwrap().sp_cost.0]);
        assert_eq!(gear.damage.map(|s| s.0), [b.skill_power_patch().unwrap().jobs[1].0, b.skill_power_patch().unwrap().jobs[2].0, b.skill_power_patch().unwrap().jobs[0].0]);
        assert_eq!(gear.damage[2].0, b.accessory_patch().unwrap().damage_rogue.0, "ay.a(BILbd;)I — the accessory block wraps it too");
        assert_eq!((gear.char_slot, gear.cell_damage, gear.dispatch), (gp.mode_slot - 0x0019_3c4c + 0x0019_3b5c, b.venom_patch().unwrap().cell_damage, 0x0016_917c));
        // ★ 2026-10-09 (3): ag.a(IZ)V (the beast factor) and bd.q()V (용의 심장) are chained too — the kill word after the
        //   HELL drop block's wrapper, the cooldown start after nobody's (일풍격 copies its definition instead).
        assert_eq!(gear.kill, b.hell_drop_patch().unwrap().death, "bd.q()V — chained after the HELL drop block");
        assert_eq!(gear.cooldown_start, (b.ilpung_patch().unwrap().cd_start.1, b.ilpung_patch().unwrap().cd_start.2), "ag.a(IZ)V");
        assert_eq!((gear.cooldown_start.0 - 0x0018_5644) % 0x1c, 0);
        assert_eq!((gear.quickbar_slot, gear.cooldown_slot), (b.hero2_java().unwrap().quickbar_slot, b.hero2_java().unwrap().cooldown_slot));
        assert_eq!((gear.cell_random, gear.call_static), (b.hell_drop_patch().unwrap().cell_random, b.hell_drop_patch().unwrap().call_static));
        assert!(GearPatch::CODE_AT >= GearPatch::RESETS + 4 && GearPatch::W_DRAGON + 4 <= GearPatch::EFFECTS_LEN);
        assert_eq!(GearPatch::ITEMS.len(), 89, "★★ 2026-10-20 + 피의 연쇄 · ★★ 2026-10-14 + 번개의 호흡 + the 2nd-gen table items (+ A's six · G's three · B1 · B2 · C's five · F1 · F2 · H1 · H2 · H3; ★★ 2026-10-15 − E2 휘두르기 도끼 · + E8 스프링 부츠) · ★★ 2026-10-16 + the five 악의 weapons · ★★ 2026-10-17 + the six 발도 items");
        assert!(GearPatch::W_THIEF + 4 <= GearPatch::EFFECTS_LEN && GearPatch::THIEF_SECS + 4 <= GearPatch::CODE_AT && GearPatch::THIEF_HITS == GearPatch::BSK_EXT + 4);
        assert!(GearPatch::W_COUNTER_SPLASH + 4 <= GearPatch::EFFECTS_LEN && GearPatch::W_STANCE_TAKEN >= GearPatch::W_FURY_DMG + 4);
        assert!(GearPatch::W_FURY_DMG + 4 <= GearPatch::EFFECTS_LEN && GearPatch::W_FURY_HIT >= GearPatch::W_COUNTER_LS + 4, "★★ the 격분 words in the effect words");
        assert!(GearPatch::BSK_EXT + 4 <= GearPatch::CODE_AT && GearPatch::BSK_EXT >= GearPatch::RESETS + 4);
        assert!(GearPatch::item(0, 33).is_some() && GearPatch::item(12, 21).is_some() && GearPatch::item(0, 32).is_none());
        assert_eq!(GearPatch::effects_none().len() as u32, GearPatch::EFFECTS_LEN);
        assert_eq!(VenomPatch::GEAR_PTR, 0x17c, "past the three venom pieces, inside the 0x180 block");
        assert!(VenomPatch::GEAR_PTR + 4 <= VenomPatch::BLOCK_LEN);
        words.push(gear.crit_hook.1);
        for build in [GuestBuild::Ktf, GuestBuild::Lgt, GuestBuild::Skt] {
            assert!(build.gear_patch().is_none());
        }
        assert_eq!((GuardianPatch::hp_mul_for(0), GuardianPatch::hp_mul_for(300), GuardianPatch::hp_mul_for(100)), (307, 384, 384));
        assert_eq!((GuardianPatch::kill_exp(1900, 3), GuardianPatch::kill_exp(1900, 2), GuardianPatch::kill_exp(1, 3)), (2850, 1900, 1));
        assert_eq!((GuardianPatch::MODE, GuardianPatch::HELL_BASE, GuardianPatch::LEVEL_BASE, GuardianPatch::LEVEL_MUL, GuardianPatch::LEVEL_CAP), (3, 60, 70, 448, 105), "HELL 60..80 → 70..105");
        assert_eq!((GuardianPatch::SEEN_MUL, GuardianPatch::CORRECT), (146, 1), "the formulas see the HELL level (4/7 of the stretch)");
        assert_eq!([60, 68, 78, 80].map(GuardianPatch::level), [70, 84, 101, 105]);
        assert_eq!([70, 84, 101, 105].map(|l| GuardianPatch::seen_level(l, 3)), [60, 67, 77, 79]);
        assert_eq!((GuardianPatch::HP_MUL, GuardianPatch::ATK_MUL, GuardianPatch::DEF_MUL, GuardianPatch::AGI_MUL), (384, 307, 307, 256), "×1.5 · ×1.2 · ×1.2 · ×1.0 in /256");
        assert_eq!(HellDropPatch::LAST_HELL_SLOT, 9, "the asset's HELL slots 5..9 (9 = the guardian recipes, 2026-10-09); the guardian pass is 10..");
        let cp = b.combat_patch().unwrap();
        words.extend([cp.weapon_getters[0].0, cp.weapon_getters[1].0, cp.sweep.0, cp.wave_ctor.0, cp.wave_paint.0, cp.hp_delta.0, cp.seq_map.0, cp.cast.0, cp.sp_cost.0, cp.cooldown.0]);
        let n = words.len();
        words.sort_unstable();
        words.dedup();
        assert_eq!(words.len(), n, "the clear hook's def word is no other patch's");
    }

    /// ★ 2026-10-02: the cooldown tick counter sits on `ag.i()V`'s definition word — hero2 only,
    /// a whole number of definitions into the table, and a word no other host block arms.
    #[test]
    fn the_cooldown_clock_is_hero2_only_and_arms_its_own_word() {
        for b in [GuestBuild::Ktf, GuestBuild::Lgt, GuestBuild::Skt] {
            assert!(b.cooldown_clock_patch().is_none(), "{b:?} must have no cooldown clock");
        }
        let b = GuestBuild::Hero2Ktf;
        let p = b.cooldown_clock_patch().expect("hero2's ag.i()V");
        assert_eq!((p.def, p.body), (0x0018_6118, 0x0011_20a8));
        assert_eq!(p.def, 0x0018_5644 + 99 * 0x1c, "ag.i()V's definition (word 0 = 0x6199 + 0x10bf10)");
        assert_eq!(p.body & 1, 0);
        assert_eq!((CooldownClockPatch::COUNT, CooldownClockPatch::BODY, CooldownClockPatch::BLOCK_LEN), (0x0c, 0x10, 0x14));
        let mut words = vec![p.def, b.clear_hook_patch().unwrap().def, b.exp_rate_patch().unwrap().def, b.equip_cursor_patch().unwrap().def, b.venom_patch().unwrap().strike.0];
        words.extend(b.skill_power_patch().unwrap().jobs.iter().map(|j| j.0));
        let cp = b.combat_patch().unwrap();
        words.extend([cp.weapon_getters[0].0, cp.weapon_getters[1].0, cp.sweep.0, cp.wave_ctor.0, cp.wave_paint.0, cp.hp_delta.0, cp.seq_map.0, cp.cast.0, cp.sp_cost.0, cp.cooldown.0]);
        let n = words.len();
        words.sort_unstable();
        words.dedup();
        assert_eq!(words.len(), n, "the cooldown clock's def word is no other patch's");
    }

    /// ★ 2026-10-05: a recast buff restarts its duration — the redirect sits on `g.a(BI)V`'s
    /// definition word, hero2 only, a whole number of definitions into the table, and a word no
    /// other host block arms.
    #[test]
    fn the_buff_refresh_is_hero2_only_and_arms_its_own_word() {
        for b in [GuestBuild::Ktf, GuestBuild::Lgt, GuestBuild::Skt] {
            assert!(b.buff_refresh_patch().is_none(), "{b:?} must have no buff refresh");
        }
        let b = GuestBuild::Hero2Ktf;
        let p = b.buff_refresh_patch().expect("hero2's g.a(BI)V");
        assert_eq!((p.def, p.body), (0x0018_c8ac, 0x0015_c09c));
        assert_eq!(p.def, 0x0018_5644 + 1046 * 0x1c, "g.a(BI)V's definition (word 0 = 0x5018d + 0x10bf10)");
        assert_eq!((p.beast_def, p.beast_body), (0x0018_c8e4, 0x0015_c19c));
        assert_eq!(p.beast_def, 0x0018_5644 + 1048 * 0x1c, "g.b(BBI)V's definition (word 0 = 0x5028d + 0x10bf10)");
        // ★ 2026-10-06: the entries grew (the lengths they set) — 0x60 · 0xc0 → 0x80 · 0x120.
        assert_eq!((BuffRefreshPatch::BEAST_AT, BuffRefreshPatch::BLOCK_LEN), (0x80, 0x120));
        assert!(matches!(b.char_anchor(), Some(CharAnchor::JavaObject { slot }) if slot == p.char_slot), "only the character's buffs are refreshed");
        // ★ 2026-10-06: the offsets the lengths are read through are the game's — a.p (the char layout's), the
        // character's beast field and the beast's level array (the accessory block's).
        let h2 = b.hero2_java().unwrap();
        assert_eq!((p.skills_off, p.beast_off), (h2.char_skill_levels_off, h2.beast_off));
        assert_eq!(p.beast_levels_off, AccessoryPatch::BEAST_LEVELS_OFF);
        // l.<init>(BBI)V's narrowing (`lsls r2,r2,#24` · `asrs r2,r2,#24`) — taken out by the install.
        assert_eq!(p.widen, [(0x0016_16d4, (24 << 6) | (2 << 3) | 2, 0x46c0), (0x0016_16d6, 0x1000 | (24 << 6) | (2 << 3) | 2, 0x46c0)]);
        // The lengths: the beast's cycles cover 250·(lv + 1) frames in whole loops (★ 2026-10-07 분노태세 is a toggle —
        // `BerserkerPatch::RAGE_ON_FRAMES`, past any elapsed count the `b:S` tick reaches).
        assert!(BerserkerPatch::RAGE_ON_FRAMES > i16::MAX as u32);
        let guard = [1, 2, 3, 4, 5].map(|lv| BuffRefreshPatch::beast_cycles(BuffRefreshPatch::BEAST_GUARD, lv));
        let regen = [1, 2, 3, 4, 5].map(|lv| BuffRefreshPatch::beast_cycles(BuffRefreshPatch::BEAST_REGEN, lv));
        assert_eq!(guard, [63, 94, 125, 157, 188], "빛의수호, 8-frame loops");
        assert_eq!(regen, [84, 125, 167, 209, 250], "빛의회복, 6-frame loops");
        for lv in 1..=5u8 {
            for (kind, c) in [(BuffRefreshPatch::BEAST_GUARD, guard[lv as usize - 1]), (BuffRefreshPatch::BEAST_REGEN, regen[lv as usize - 1])] {
                let frames = c * BuffRefreshPatch::beast_unit(kind);
                let want = BuffRefreshPatch::LEN_STEP * (lv as u32 + 1);
                assert!(frames >= want && frames < want + BuffRefreshPatch::beast_unit(kind), "kind {kind} lv {lv}: {frames} for {want}");
                assert!(c <= 255, "a byte in the block's table");
            }
        }
        assert_eq!(p.body & 1, 0);
        assert_eq!(
            (BuffRefreshPatch::BUFFS_OFF, BuffRefreshPatch::AV_ID_OFF, BuffRefreshPatch::AV_UNIT_OFF, BuffRefreshPatch::AV_ELAPSED_OFF, BuffRefreshPatch::AV_LEN_OFF, BuffRefreshPatch::AV_ENDED_OFF),
            (0x38, 0x00, 0x04, 0x08, 0x0c, 0x10)
        );
        let mut words = vec![
            p.def,
            p.beast_def,
            b.cooldown_clock_patch().unwrap().def,
            b.clear_hook_patch().unwrap().def,
            b.exp_rate_patch().unwrap().def,
            b.equip_cursor_patch().unwrap().def,
            b.venom_patch().unwrap().strike.0,
            b.hell_drop_patch().unwrap().death.0,
            b.sky_draw_patch().unwrap().ground_draw.0,
            b.sky_draw_patch().unwrap().hud_draw.0,
        ];
        words.extend(b.skill_power_patch().unwrap().jobs.iter().map(|j| j.0));
        let cp = b.combat_patch().unwrap();
        words.extend([cp.weapon_getters[0].0, cp.weapon_getters[1].0, cp.sweep.0, cp.wave_ctor.0, cp.wave_paint.0, cp.hp_delta.0, cp.seq_map.0, cp.cast.0, cp.sp_cost.0, cp.cooldown.0]);
        let ap = b.accessory_patch().unwrap();
        // `damage_rogue` is left out: it is the skill-power rogue job's word, chained on purpose.
        words.extend([ap.sp_cost.0, ap.cooldown.0, ap.beast_damage.0, ap.usable_rogue.0, ap.drive_rogue.0, ap.strike_rogue.0, ap.arm.0, ap.update.0, ap.draw.0]);
        let n = words.len();
        words.sort_unstable();
        words.dedup();
        assert_eq!(words.len(), n, "the buff refresh's def word is no other patch's");
    }

    /// ★★ 2026-10-08: 천공의검's picture block — hero2 only; it wraps the character draw the accessory and rogue blocks
    /// already chain (so it goes in after them), on the Graphics words they measured; fillRect's pointer is `sl + 0x9c`
    /// (setColor's `sl + 0x88`, setAlpha's `sl + 0x17c`); its own drawLine word names Graphics (class 0x169) name-table
    /// index 0x59; its data sits under the code; the prongs are CombatPatch's front diagonals and the facing; the
    /// timeline is the user's (three prongs 0.15 s apart · the break 0.3 s on · the blast 0.3 s after, inside the 27
    /// frames the cast writes into a.n[15]). ★ (3) Its second arm is the ground draw — a whole definition whose image
    /// word is that body; the tears taper from blunt to a point; the floor's snowflake fits the bytes it is stored in.
    #[test]
    fn the_sky_draw_patch_is_hero2_only_and_chains_the_character_draw() {
        for b in [GuestBuild::Ktf, GuestBuild::Lgt, GuestBuild::Skt] {
            assert!(b.sky_draw_patch().is_none(), "{b:?} must have no sky draw block");
        }
        let b = GuestBuild::Hero2Ktf;
        let p = b.sky_draw_patch().expect("hero2's sky draw block");
        let ap = b.accessory_patch().unwrap();
        let rp = b.rogue_patch().unwrap();
        let cp = b.combat_patch().unwrap();
        assert_eq!((p.draw, p.draw), (ap.draw.0, rp.draw), "the character draw the accessory and rogue blocks chain");
        assert_eq!((p.draw - 0x0018_5644) % 0x1c, 0, "a whole definition into the table");
        assert_eq!((p.iface_call, p.graphics_slot, p.set_alpha_ref, p.set_color_ref), (ap.iface_call, ap.graphics_slot, ap.set_alpha_ref, ap.set_color_ref));
        assert_eq!(p.fill_rect_ref, p.set_color_ref + (0x9c - 0x88), "fillRect's pointer at sl + 0x9c");
        assert_eq!((p.set_alpha_ref, p.set_color_ref), (0x0019_5ca4 + 0x17c, 0x0019_5ca4 + 0x88));
        assert_eq!((p.class_slot, p.char_slot), (rp.class_slot, rp.char_slot));
        assert_eq!((p.facing_off, p.frame_off), (cp.facing_off, cp.action_frame_off));
        assert_eq!((SkyDrawPatch::LINE_REF_WORD, SkyDrawPatch::CLASS, SkyDrawPatch::SEQ), (0x0059_0169, 1, CombatPatch::SKY_SEQ));
        assert!(SkyDrawPatch::DATA_LEN <= SkyDrawPatch::CODE_AT && SkyDrawPatch::Y2 + 4 <= SkyDrawPatch::DATA_LEN);
        assert_eq!(cp.frames_slot, rp.frames_slot, "a.n:[B, the one static");
        assert_eq!((CombatPatch::SKY_SLASH_FRAMES, CombatPatch::SKY_BREAK_FRAME, CombatPatch::SKY_BLAST_FRAME, CombatPatch::SKY_FRAMES), ([4, 10, 16], 24, 36, 60));
        assert_eq!((p.ground_draw.0 - 0x0018_5644) % 0x1c, 0, "the ground draw: a whole definition into the table");
        assert_eq!((p.ground_draw.0, p.ground_draw.1), (0x0018_83e0, 0x0012_d4e4), "bf.a(Graphics;II)V: image word 0x215d5 + 0x10bf10");
        assert!(SkyDrawPatch::DATA_LEN >= SkyDrawPatch::SKY_AGE + 4 && SkyDrawPatch::DATA_LEN <= SkyDrawPatch::CODE_AT, "the state words inside the data");
        let at = [SkyDrawPatch::SKY_AT, SkyDrawPatch::BLAST_AT, SkyDrawPatch::FLOOR_AT, SkyDrawPatch::FLOWER_AT, SkyDrawPatch::SKYFLOOR_AT, SkyDrawPatch::TEAR_AT, SkyDrawPatch::WEB_AT, SkyDrawPatch::BLOOD_CHAIN, SkyDrawPatch::BLOOD_AT, SkyDrawPatch::BLOOD_PAINT_AT, SkyDrawPatch::BLOOD_MOB_AT, SkyDrawPatch::BLOCK_LEN];
        assert!(at.windows(2).all(|w| w[0] < w[1]) && at[0] == SkyDrawPatch::CODE_AT, "the six pieces in order inside the block");
        assert_eq!((SkyDrawPatch::STROKE_TAG >> 16, SkyDrawPatch::STROKE_SOFT_TAG >> 16), (0x4833, 0x4834), "beside the alpha tag's 0x4832");
        // ★ (5) the tear: blunt at the back, widest just in front of the knight, a point at the head; the void inside the rim
        assert_eq!((SkyDrawPatch::RIM_W.len(), SkyDrawPatch::VOID_W.len() + 1), (SkyDrawPatch::ALONG.len(), SkyDrawPatch::ALONG.len()));
        assert!(SkyDrawPatch::RIM_W[1..].windows(2).all(|w| w[0] > w[1]) && SkyDrawPatch::RIM_W[6] == 0 && SkyDrawPatch::RIM_W[0] < SkyDrawPatch::RIM_W[1], "the rim: blunt at the back, widest by the knight, a point at the head");
        assert!(SkyDrawPatch::VOID_W.iter().zip(&SkyDrawPatch::RIM_W).all(|(v, r)| v < r), "the void inside the rim");
        assert!(SkyDrawPatch::ALONG.windows(2).all(|w| w[0] < w[1]) && SkyDrawPatch::ALONG[6] == 64 && SkyDrawPatch::ALONG[0] < 0, "the blade reaches behind the knight");
        // ★ 2026-10-09 the picture × SIZE (4/5) — the box is untouched, the picture a fifth smaller
        assert_eq!((SkyDrawPatch::SIZE, SkyDrawPatch::sz(448), SkyDrawPatch::sz(-16), SkyDrawPatch::sz(5)), ((4, 5), 358, -13, 4));
        assert!(i32::from(SkyDrawPatch::HEADS[2]) * i32::from(-SkyDrawPatch::ALONG[0]) / 64 >= SkyDrawPatch::sz(5 * 16 - 4), "five tiles back × SIZE");
        assert_eq!((SkyDrawPatch::HEADS[2], SkyDrawPatch::PRONG_S), (SkyDrawPatch::sz(i32::from(CombatPatch::SKY_DEPTH) * 16) as u16, [-8, 0, 8]), "28 tiles × SIZE ahead; the fan's slopes ∓½ · 0 · ±½");
        assert_eq!((SkyDrawPatch::RIM_W, SkyDrawPatch::GLOW_W, SkyDrawPatch::SKY_UNFOLD), ([19, 32, 27, 22, 15, 8, 0], (45, 6), [5, 7, 10, 11, 12, 13]));
        assert_eq!(SkyDrawPatch::CRACK_SEGS[1], (26, 0, 31, -13), "the across offsets × SIZE, the along (64ths of the head) as they were");
        assert_eq!(SkyDrawPatch::CRACK_REVEAL[2] as usize, SkyDrawPatch::CRACK_SEGS.len());
        for (a1, p1, a2, p2) in SkyDrawPatch::CRACK_SEGS {
            assert!(a1 <= 64 && a2 <= 64 && (a1, p1) != (a2, p2), "a crack along the prong, with a length");
        }
        assert_eq!(1u8 << SkyDrawPatch::FADE_SHIFT, SkyDrawPatch::FADE_FULL);
        assert_eq!(SkyDrawPatch::FLOOR_SQUASH, (9, 4), "★ (5) laid a little more than 11/16 (user: 「조금 더 누워져야해」)");
        assert_eq!(SkyDrawPatch::floor_segments().len(), 114, "the snowflake: heart 6 + long 6 × 11 + short 6 × 7");
        for (x1, y1, x2, y2) in SkyDrawPatch::floor_segments() {
            assert!([x1, x2].iter().all(|v| v.abs() <= 100) && [y1, y2].iter().all(|v| v.abs() <= 60), "inside the laid ellipse");
        }
        let dots = SkyDrawPatch::floor_dots();
        assert_eq!((dots.len(), dots[3], dots[0]), (12, (0, 51), (70, 0)), "tips: long 91 px down = 51 laid, short 70 across");
        assert_eq!(SkyDrawPatch::floor_pillars().len(), SkyDrawPatch::PILLAR_H.len());
        assert_eq!((SkyDrawPatch::sky_floor_cracks().len(), SkyDrawPatch::sky_floor_burst().len()), (24, 8), "twelve floor cracks of two segments, eight rays");
        for (x1, y1, x2, y2) in SkyDrawPatch::sky_floor_cracks() {
            assert!([x1, x2].iter().all(|v| v.abs() <= 100) && [y1, y2].iter().all(|v| v.abs() <= 60), "a floor crack inside the laid disc");
        }
        // the two fans differ: the facing's axes laid — a vertical ground distance comes out at 9/16
        assert_eq!(SkyDrawPatch::ground_to_screen(4, 100, 0), (100, 0));
        assert_eq!(SkyDrawPatch::ground_to_screen(1, 100, 0), (0, -57), "−900 >> 4: the shift floors");
        assert_eq!(SkyDrawPatch::ground_to_screen(4, 0, 100), (0, 56));
        assert_eq!(SkyDrawPatch::ground_to_screen(1, 0, 100), (100, 0));
        assert_eq!(SkyDrawPatch::ground_to_screen(9, 100, 0), SkyDrawPatch::ground_to_screen(2, 100, 0), "a bogus facing draws as facing down");
        assert_eq!(SkyDrawPatch::prong_point(0, 448, 0), (448, -224), "the first prong's head: 448 ahead, 224 to the side");
        // ★ (6) the glass: the slower timeline (a prong every 6 frames · the break 8 after the last · the blast 12 after ·
        // a 24-frame tail = 60 = 3 s), the seventh piece, the cut and the widening blade, the web · the dark,
        // the shards — the generators' shapes, the same bytes in the app, the probe and the tests
        assert_eq!((CombatPatch::SKY_TAIL, SkyDrawPatch::WEB_AT, SkyDrawPatch::BLOCK_LEN, SkyDrawPatch::KW), (24, 0x1b00, 0x2c00, 0x74), "★ 2026-10-09 0x2400 → 0x2c00: 격분's blood");
        assert!(SkyDrawPatch::KW + 4 <= SkyDrawPatch::DATA_LEN, "the blade's width factor inside the data");
        assert_eq!(SkyDrawPatch::COLOURS.len(), 12);
        // ★ 2026-10-09 격분's blood: the HUD painter n.d()V (#1165, the image word 0x57a55 + 0x10bf10), its data past the
        // glass, the berserker block's record under its pieces, the tables a draw each
        assert_eq!(p.hud_draw, (0x0018_d5b0, 0x0016_3964));
        assert_eq!((p.hud_draw.0 - 0x0018_5644) % 0x1c, 0, "a whole definition into the table");
        assert_eq!((p.hud_draw.0 - 0x0018_5644) / 0x1c, 1165);
        assert_eq!((SkyDrawPatch::BLOOD_CHAIN, SkyDrawPatch::BLOOD_DATA_LEN, SkyDrawPatch::BLOOD_AT), (0x2400, 0x40, 0x2440));
        assert!(SkyDrawPatch::BN + 4 <= SkyDrawPatch::BLOOD_CHAIN + SkyDrawPatch::BLOOD_DATA_LEN, "the blood's words inside its data");
        assert_eq!((SkyDrawPatch::COLOURS[9], SkyDrawPatch::COLOURS[10], SkyDrawPatch::COLOURS[11]), (0xed3440, 0x640f26, 0x9c1630));
        let d = usize::from(SkyDrawPatch::BLOOD_DRAWS);
        for len in [SkyDrawPatch::BLOOD_FLASH_A.len(), SkyDrawPatch::BLOOD_ALPHA.len(), SkyDrawPatch::BURST_R2.len(), SkyDrawPatch::BURST_W.len(), SkyDrawPatch::SPLAT_W.len(), SkyDrawPatch::DROP_R.len(), SkyDrawPatch::DROP_W.len()] {
            assert_eq!(len, d, "a value a draw");
        }
        assert!(SkyDrawPatch::BLOOD_ALPHA.iter().all(|a| *a > 0) && SkyDrawPatch::BLOOD_ALPHA.windows(2).all(|w| w[0] >= w[1]), "the blood fades, never vanishes before its last draw");
        assert!(SkyDrawPatch::BURST_R2.windows(2).all(|w| w[0] <= w[1]) && SkyDrawPatch::DROP_R.windows(2).all(|w| w[0] <= w[1]), "flying outward");
        for (x, y) in SkyDrawPatch::DROP_DIRS {
            let r2 = i32::from(x) * i32::from(x) + i32::from(y) * i32::from(y);
            assert!((60 * 60..=68 * 68).contains(&r2), "a unit direction in 64ths: ({x}, {y})");
        }
        assert_eq!((BerserkerPatch::BLOOD_SEQ, BerserkerPatch::BLOOD_N, BerserkerPatch::BLOOD_PX, BerserkerPatch::BLOOD_PY, BerserkerPatch::BLOOD_XY, BerserkerPatch::BLOOD_MAX), (0x2c, 0x30, 0x34, 0x38, 0x8a0, 16));
        assert!(BerserkerPatch::FURY_OWNER < BerserkerPatch::BLOOD_SEQ && BerserkerPatch::BLOOD_PY + 4 <= BerserkerPatch::HAS_BUFF_AT, "the record's words between the state and the code");
        // ★ 2026-10-09 (2): the voices' switch is the data's last word · the pieces in order with room (TAKEN 256 · the drive
        // 768) · the gauge = 25 hits, dealt or taken · the game's sound level the voices follow (`bs.b:B`) · ★ 2026-10-09 (11)
        // the drive 832 (the blast by 격분's level)
        assert_eq!((BerserkerPatch::FURY_VOICE, BerserkerPatch::HAS_BUFF_AT, BerserkerPatch::BLOCK_LEN), (0x3c, 0x40, 0xc00));
        let at = [BerserkerPatch::TAKEN_AT, BerserkerPatch::MOB_AT, BerserkerPatch::UPDATE_AT, BerserkerPatch::DRIVE_AT, BerserkerPatch::BLOOD_XY];
        assert_eq!(at, [0x320, 0x420, 0x4a0, 0x560, 0x8a0]);
        assert!(at.windows(2).all(|w| w[0] < w[1]) && BerserkerPatch::DAMAGE_AT < BerserkerPatch::TAKEN_AT);
        assert_eq!((BerserkerPatch::FURY_HITS_FULL, BerserkerPatch::FURY_GAUGE_FULL, BerserkerPatch::gauge_of(1), BerserkerPatch::gauge_of(25), BerserkerPatch::gauge_of(40)), (25, 100, 4, 100, 100));
        assert_eq!((Hero2Java::SOUND_LEVEL_SLOT, Hero2Java::CHAR_ACTION_QUEUE_OFF, Hero2Java::CHAR_ACTION_AT_OFF), (0x0019_419c, 0xac, 0xb0));
        let bp = GuestBuild::Hero2Ktf.berserker_patch().unwrap();
        assert_eq!((bp.queue_off, bp.queue_at_off), (Hero2Java::CHAR_ACTION_QUEUE_OFF, Hero2Java::CHAR_ACTION_AT_OFF), "the action queue the drives read");
        assert_eq!(BerserkerPatch::BLOOD_XY + 8 * u32::from(BerserkerPatch::BLOOD_MAX), BerserkerPatch::GEAR_PTR, "the monsters' px, then ★★ 2026-10-14 the gear tail");
        assert!(BerserkerPatch::HIT_TICK + 4 <= BerserkerPatch::FURY_CD_AT && BerserkerPatch::FURY_CD_AT < BerserkerPatch::FGEAR_AT && BerserkerPatch::FGEAR_AT < BerserkerPatch::BLOCK_LEN);
        assert_eq!((SkyDrawPatch::COLOURS[usize::from(SkyDrawPatch::PURPLE)], SkyDrawPatch::COLOURS[usize::from(SkyDrawPatch::DARK)], SkyDrawPatch::COLOURS[usize::from(SkyDrawPatch::SPACE)]), (0xbd91ed, 0x171225, 0x302750));
        // ★ (7) closer to the reference: the tears end with the break's flash, a wide white glow under the blue, a spray of
        // white rays, the dark deepest at the blast then the space, slabs for shards
        assert_eq!(SkyDrawPatch::TEAR_END, CombatPatch::SKY_BREAK_FRAME + 2);
        assert!(SkyDrawPatch::WHITE_GLOW_W.0 > SkyDrawPatch::GLOW_W.0 && SkyDrawPatch::WHITE_GLOW_ALPHA < SkyDrawPatch::GLOW_ALPHA);
        assert_eq!((SkyDrawPatch::SPRAY_RAYS, SkyDrawPatch::DIRS16.len(), SkyDrawPatch::spray_table().len()), (16, 16, usize::from(CombatPatch::SKY_FRAMES)));
        for (dx, dy) in SkyDrawPatch::DIRS16 {
            let r2 = i32::from(dx) * i32::from(dx) + i32::from(dy) * i32::from(dy);
            assert!((60 * 60..=68 * 68).contains(&r2), "a unit direction in 64ths: ({dx}, {dy})");
        }
        assert_eq!((SkyDrawPatch::spray_q(0), SkyDrawPatch::spray_q(CombatPatch::SKY_SLASH_FRAMES[0]), SkyDrawPatch::spray_q(CombatPatch::SKY_SLASH_FRAMES[0] + 1), SkyDrawPatch::spray_q(CombatPatch::SKY_BREAK_FRAME), SkyDrawPatch::spray_q(CombatPatch::SKY_BLAST_FRAME)), (0, 0, 5, 112, 0));
        assert!(i32::from(SkyDrawPatch::spray_q(CombatPatch::SKY_SLASH_FRAMES[0] + 1)) * 4 > i32::from(SkyDrawPatch::SPRAY_R0), "the first spray reaches past its start");
        assert!(SkyDrawPatch::SHARD_SLIVER_W.0 > SkyDrawPatch::SHARD_W.0);
        assert_eq!((SkyDrawPatch::THIN_ALPHA.len(), SkyDrawPatch::BLADE_K), (3, [0, 2, 4, 3, 4]), "the blade opens, recoils and settles");
        assert!(SkyDrawPatch::THIN_W.0 < SkyDrawPatch::RIM_W[1] && SkyDrawPatch::THIN_GLOW_W.0 < SkyDrawPatch::GLOW_W.0, "the cut is thinner than the blade");
        assert!(SkyDrawPatch::HEADS.iter().all(|h| *h == SkyDrawPatch::HEADS[2]), "★ (6) the cut spans the whole prong from its first draw");
        let draws = usize::from(SkyDrawPatch::BLAST_DRAWS);
        assert!([SkyDrawPatch::FLASH_HALF.len(), SkyDrawPatch::FLASH_ALPHA.len(), SkyDrawPatch::RAY_LEN.len(), SkyDrawPatch::RAY_ALPHA.len()].iter().all(|n| *n == draws), "the flash's tables");
        assert!(SkyDrawPatch::FLASH_ALPHA.windows(2).all(|w| w[0] > w[1]) && SkyDrawPatch::FLASH_HALF.windows(2).all(|w| w[0] < w[1]), "the flash grows and fades");
        assert_eq!((SkyDrawPatch::SHARD_ALPHA.len(), SkyDrawPatch::shards().len()), (usize::from(SkyDrawPatch::SHARD_DRAWS), SkyDrawPatch::SHARDS));
        let glints: Vec<u8> = SkyDrawPatch::SHARD_ALPHA.chunks(3).map(|c| *c.iter().max().unwrap()).collect();
        assert!(glints.windows(2).all(|w| w[0] > w[1]) && SkyDrawPatch::SHARD_DRAWS > SkyDrawPatch::BLAST_DRAWS, "the shards outlive the flash; successive glints fade");
        for (dx, dy, r0, v, verts) in SkyDrawPatch::shards() {
            assert!((dx != 0 || dy != 0) && v >= 4 && r0 >= 16, "a shard flies out (× SIZE): {:?}", (dx, dy, r0, v));
            assert!(verts.iter().all(|(x, y)| x.abs() <= 32 && y.abs() <= 32), "a slab, not a speck, still small: {verts:?}");
        }
        let segs = SkyDrawPatch::web_segments();
        let reveal = SkyDrawPatch::web_reveal();
        assert_eq!(segs.len(), 6 * SkyDrawPatch::WEB_RAYS, "the web: three steps, two connectors and a branch per ray");
        assert!(segs.windows(2).all(|w| w[0].4 <= w[1].4), "sorted by the radius they start at");
        assert!(segs.iter().all(|s| [s.0, s.1, s.2, s.3].iter().all(|v| i32::from(v.abs()) <= SkyDrawPatch::WEB_RADIUS_END)), "inside WEB_RADIUS_END");
        assert_eq!(reveal.len(), usize::from(CombatPatch::SKY_FRAMES));
        let (first, brk, blast) = (usize::from(CombatPatch::SKY_SLASH_FRAMES[0]), usize::from(CombatPatch::SKY_BREAK_FRAME), usize::from(CombatPatch::SKY_BLAST_FRAME));
        assert!(reveal[..first].iter().all(|n| *n == 0) && reveal[blast..].iter().all(|n| *n == 0), "nothing before the first prong, nothing from the blast");
        assert!(reveal[first..brk].windows(2).all(|w| w[0] <= w[1]) && reveal[first] < reveal[brk - 1], "revealed outward");
        assert_eq!(usize::from(reveal[brk]), segs.len(), "all of it at the break");
        assert_eq!((SkyDrawPatch::web_radius(0), SkyDrawPatch::web_radius(CombatPatch::SKY_BREAK_FRAME), SkyDrawPatch::web_radius(CombatPatch::SKY_BLAST_FRAME)), (0, SkyDrawPatch::sz(420), 0), "★ 2026-10-09 × SIZE");
        assert_eq!(SkyDrawPatch::DARK_ALPHA.len(), usize::from(CombatPatch::SKY_FRAMES - CombatPatch::SKY_BREAK_FRAME), "the dark: a value for every draw from the break");
        let blast_at = usize::from(CombatPatch::SKY_BLAST_FRAME - CombatPatch::SKY_BREAK_FRAME);
        assert!(SkyDrawPatch::DARK_ALPHA[blast_at] == 205 && SkyDrawPatch::DARK_ALPHA[..blast_at].iter().all(|a| *a < 205) && SkyDrawPatch::DARK_ALPHA[35] == 0, "deepest at the blast, gone by the end");
        assert!(SkyDrawPatch::DARK_ALPHA[blast_at..].windows(2).all(|w| w[0] >= w[1]), "the space lifts");
        assert!(u32::from(SkyDrawPatch::DARK_HALF) >= 448 + 100, "the sheet covers the prongs' heads");
        let _ = cp.front_diag;
    }

    /// ★★ 2026-10-06: the rogue block (살상연희 · 심안) — hero2 only; its four chained words are exactly the 일풍격 / accessory
    /// definitions it wraps (so the install order puts it last), 심안's paint a whole definition no other host block arms;
    /// the cells, helpers, statics and offsets the sibling blocks already measured; the data words inside the data, the
    /// pieces in order inside the block; and 심안's slot the one the venom no longer reads.
    #[test]
    fn the_rogue_patch_is_hero2_only_and_arms_its_own_words() {
        for b in [GuestBuild::Ktf, GuestBuild::Lgt, GuestBuild::Skt] {
            assert!(b.rogue_patch().is_none(), "{b:?} must have no rogue block");
        }
        let b = GuestBuild::Hero2Ktf;
        let p = b.rogue_patch().expect("hero2's rogue block");
        let ip = b.ilpung_patch().unwrap();
        let ap = b.accessory_patch().unwrap();
        let cp = b.combat_patch().unwrap();
        assert_eq!(p.mob_paint, (0x0018_7b90, 0x0012_601c));
        for def in [p.mob_paint.0, p.usable, p.drive, p.cooldown, p.draw] {
            assert_eq!((def - 0x0018_5644) % 0x1c, 0, "{def:#x}: a whole definition into the table");
        }
        assert_eq!(p.mob_paint.1 & 1, 0);
        // the chained four: 일풍격's usable · drive · cooldown and the accessory block's character draw
        assert_eq!((p.usable, p.drive, p.cooldown, p.draw), (ip.usable.0, ip.drive.0, ip.cooldown.0, ap.draw.0));
        // the cells, helpers and statics the sibling blocks measured
        assert_eq!((p.cell_attack, p.cell_hit, p.cell_sound), (ip.cell_attack, ip.cell_hit, ip.cell_sound), "ay.a(BBI)Z's own cells, 발도's sound");
        assert_eq!((p.cell_attack, p.cell_hit, p.cell_draw), (ap.cell_attack, ap.cell_hit, ap.cell_draw));
        assert_eq!(p.cell_sound, cp.cell_sound);
        assert_eq!((p.dispatch, p.call_static, p.iface_call), (ip.dispatch, ip.call_static, ap.iface_call));
        assert_eq!((p.dispatch, p.call_static), (ap.dispatch, ap.call_static));
        assert_eq!((p.class_slot, p.char_slot), (ip.class_slot, ip.char_slot));
        assert_eq!(p.class_slot, b.hero2_java().unwrap().class_slot);
        assert!(matches!(b.char_anchor(), Some(CharAnchor::JavaObject { slot }) if slot == p.char_slot));
        assert_eq!((p.graphics_slot, p.set_alpha_ref), (ap.graphics_slot, ap.set_alpha_ref), "the accessory block's draw alpha");
        // the character's offsets
        assert_eq!(p.skills_off, b.hero2_java().unwrap().char_skill_levels_off, "the a.p the rest of the host reads");
        assert_eq!((p.skills_off, p.weapon_l_off, p.queue_off, p.queue_at_off, p.frame_off), (ip.skills_off, ip.weapon_l_off, ip.queue_off, ip.queue_at_off, ip.frame_off));
        assert_eq!((p.invuln_off, p.weapon_l_off, p.weapon_m_off, p.frame_off), (cp.invuln_off, cp.weapon_l_off, cp.weapon_m_off, cp.action_frame_off), "a.s — the COMBAT guards' flag");
        // ad 0x34 + g 0x28 = bd's 0x5c: bd.a:Lcq; +0x4 · bd.h:Z +0x44; g.t is g's +0x8 on ad's 0x34; cq has no parent.
        assert_eq!((p.mob_template_off, p.mob_hidden_off, p.mob_state_off, p.template_ghost_off), (0x5c + 0x4, 0x5c + 0x44, 0x34 + 0x8, 0x48));
        assert_eq!(p.mob_state_off, AccessoryPatch::STATE_OFF);
        // the skill
        assert_eq!(RoguePatch::KIND, 5 + 5, "번개난타's slot = sk2 skill 5 after the rogue's five stages");
        assert_eq!(RoguePatch::SEQ, RoguePatch::KIND + 4, "a rogue's action sequence = kind + 4");
        assert_eq!(RoguePatch::COOLDOWN_INDEX, RoguePatch::KIND - 5);
        assert_eq!(u32::from(RoguePatch::COOLDOWN), 10 * 20, "10 s at 20 fps");
        assert_eq!((RoguePatch::TOTALS[1], RoguePatch::TOTALS[5]), (3, 8), "the user's Lv1 and Lv5");
        assert!(RoguePatch::TOTALS.windows(2).all(|w| w[0] <= w[1]));
        // ★ 2026-10-08 (5): the area 51×51 (the user's 50×50 centred), 개화 19×19 (its 18×18), the landings by the target's
        // body (the window reaches 오딘's rim: its body y ±2, one more), LANDS of them kept before the afterimages; no cap
        // ★ 2026-10-16: 개화 = the whole map (a map's W/H are one byte each)
        assert_eq!((RoguePatch::REACH, RoguePatch::BLOOM_REACH, RoguePatch::LAND_WINDOW, RoguePatch::LANDS), (25, 255, 3, 24));
        assert!(RoguePatch::LAND + 4 * u32::from(RoguePatch::LANDS) <= RoguePatch::TRAIL && RoguePatch::TRAIL + 16 * u32::from(RoguePatch::TRAILS) <= RoguePatch::DATA_LEN);
        assert!(RoguePatch::PLAN_AT < RoguePatch::LANDS_AT && RoguePatch::LANDS_AT < RoguePatch::DRIVE_AT && RoguePatch::DRIVE_AT < RoguePatch::BLOCK_LEN);
        // ★ 2026-10-06: the landing steps past a boss's body — 오딘's `al.t()V` covers y −2..=2, so from the first
        // neighbour two more rows, and one more while it stands between two tiles
        assert!(AccessoryPatch::BODY_PASS >= 2 + 1);
        // ★ 2026-10-06: a strike / blow reaches its own target one knockback (one tile) further than the eight around
        assert_eq!(AccessoryPatch::KNOCKBACK_REACH, 2);
        assert!(u32::from(RoguePatch::HOP_FRAMES) * u32::from(RoguePatch::TOTALS[5]) < u32::from(RoguePatch::SEQ_FRAMES), "every hit fits the sequence");
        assert_eq!(usize::from(RoguePatch::TRAIL_DRAWS), RoguePatch::TRAIL_ALPHAS.len() - 1);
        assert_eq!((IlpungPatch::SIGHT_SLOT, VenomPatch::MASTERY_SKILL), (12, 12), "심안 sits where 독 연마 was");
        assert!(Hero2Java::BORN_CLASS_SKILLS.contains(&(RoguePatch::CLASS, u32::from(IlpungPatch::SIGHT_SLOT), true)), "every rogue is held at 1, refunded once");
        // the layout: the data words in the data, the pieces in order
        type P = RoguePatch;
        let words = [P::PLAN_N, P::PLAN_T, P::PLAN_D2, P::RUN_KIND, P::TOTAL, P::HOPS, P::CUR_T, P::ANCHOR_X, P::ANCHOR_Y, P::ROT, P::LAST, P::OWN_S, P::LAST_SEQ, P::LAST_DIR, P::LAST_FRAME, P::TRAIL_HEAD, P::CHAIN_USABLE, P::CHAIN_DRIVE, P::CHAIN_COOLDOWN, P::CHAIN_DRAW, P::CHAIN_SEQ, P::FLOWER, P::LAND_N, P::LAND, P::TRAIL];
        let lens = [4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4 * u32::from(P::LANDS), 16 * u32::from(P::TRAILS)];
        for (i, (w, l)) in words.iter().zip(lens).enumerate() {
            assert_eq!(w % 4, 0, "{w:#x}");
            assert!(w + l <= words.get(i + 1).copied().unwrap_or(P::DATA_LEN), "{w:#x} + {l} runs into the next");
        }
        let at = [P::DATA_LEN, P::COOLDOWN_AT, P::SEQ_AT, P::SIGHT_AT, P::DRAW_AT, P::PLAN_AT, P::LANDS_AT, P::DRIVE_AT, P::BLOCK_LEN];
        assert!(at.windows(2).all(|w| w[0] <= w[1]) && at.iter().all(|a| a % 4 == 0), "data · cooldown · the map · 심안 · draw · plan · landings · drive");
        // 심안's paint is no other host block's def word; the chained four are 일풍격's / the accessory's own on purpose
        let mut armed = vec![
            b.buff_refresh_patch().unwrap().def,
            b.buff_refresh_patch().unwrap().beast_def,
            b.cooldown_clock_patch().unwrap().def,
            b.clear_hook_patch().unwrap().def,
            b.exp_rate_patch().unwrap().def,
            b.equip_cursor_patch().unwrap().def,
            b.venom_patch().unwrap().strike.0,
            b.hell_drop_patch().unwrap().death.0,
        ];
        armed.extend(b.skill_power_patch().unwrap().jobs.iter().map(|j| j.0));
        armed.extend([cp.weapon_getters[0].0, cp.weapon_getters[1].0, cp.sweep.0, cp.wave_ctor.0, cp.wave_paint.0, cp.hp_delta.0, cp.seq_map.0, cp.cast.0, cp.sp_cost.0, cp.cooldown.0]);
        armed.extend([ap.sp_cost.0, ap.cooldown.0, ap.beast_damage.0, ap.usable_rogue.0, ap.drive_rogue.0, ap.strike_rogue.0, ap.arm.0, ap.update.0, ap.draw.0]);
        armed.extend([ip.queue.0, ip.sp_cost.0, ip.cooldown.0, ip.max_level.0, ip.cursor.0, ip.usable.0, ip.drive.0, ip.load.0]);
        assert!(!armed.contains(&p.mob_paint.0), "{:#x}: 심안's def word is another patch's", p.mob_paint.0);
        for chained in [p.usable, p.drive, p.cooldown, p.draw] {
            assert!(armed.contains(&chained), "{chained:#x}: chained onto a block that arms it");
        }
    }

    /// ★★ 세이브 슬롯 5 (2026-09-27): `ag`'s file-name tables and the slot screen's code.
    /// hero3 builds must have none of it — their slots are record-store records. On hero2
    /// the three statics are `ag`'s own (the class's static slots are 16 bytes apart, the
    /// value word at +12 of each field definition), and every code site is inside one of
    /// the four methods the disassembly named.
    #[test]
    fn the_save_slots_patch_is_hero2_only_and_stays_in_its_methods() {
        for b in [GuestBuild::Ktf, GuestBuild::Lgt, GuestBuild::Skt] {
            assert!(b.save_slots_patch().is_none(), "{b:?} must have no save-slot patch");
        }
        let p = GuestBuild::Hero2Ktf.save_slots_patch().expect("hero2's ag tables");
        assert_eq!(p.class, "ag");
        assert_eq!((p.prog_names, p.char_names, p.previews), (0x0019_39ec, 0x0019_39fc, 0x0019_3a7c));
        assert_eq!(p.char_names - p.prog_names, 0x10, "ag.a:[String then ag.b:[String");
        // The save slot the tables are indexed by is the one the rest of the host reads.
        assert_eq!(GuestBuild::Hero2Ktf.hero2_java().unwrap().save_slot_slot, 0x0019_3a9c);
        // ag.k()V @0x112ad8..0x112be4 · be.<init> @0x12886c..0x1288d8 · be.a()V ..0x128948 ·
        // be.a(II)Z ..0x128d38 · be.b()V @0x128dcc..0x128e20 (hero2_dis.py extents).
        let methods = [(0x0011_2ad8, 0x0011_2be4), (0x0012_886c, 0x0012_8d38), (0x0012_8dcc, 0x0012_8e20)];
        for (at, _, _) in p.table_sites.iter().chain(p.screen_sites) {
            assert!(methods.iter().any(|(lo, hi)| (*lo..*hi).contains(at)), "{at:#x} outside the measured methods");
        }
        assert!(p.table_sites.iter().all(|s| (0x0011_2ad8..0x0011_2be4).contains(&s.0)), "the table half is ag.k()V alone");
        // Five slots, and every slot a single digit — the vault and the file names both
        // spell a slot with one.
        assert!(SaveSlotsPatch::SLOTS <= 9 && SaveSlotsPatch::SLOTS > SaveSlotsPatch::SHIPPED);
    }

    /// ★★ #14 (2026-09-21): the `q`-grid shift that makes room for the page-tab rail
    /// INSIDE the window. hero3 builds must have none of it — these are hero2's own
    /// addresses, and writing them into an LGT/KTF image would land in unrelated code.
    ///
    /// The arithmetic assertions are the part worth keeping: they say the rail cannot
    /// reach the cells. If someone widens the rail or shrinks the shift, the first bag
    /// tab starts covering the first column of items and taps go to the wrong record —
    /// silently, because both are drawn by different owners.
    #[test]
    fn the_inventory_grid_shift_is_hero2_only_and_the_rail_clears_the_cells() {
        for b in [GuestBuild::Ktf, GuestBuild::Lgt, GuestBuild::Skt] {
            assert!(b.inventory_grid_shift().is_empty(), "{b:?} must have no q-grid shift");
        }
        let sites = GuestBuild::Hero2Ktf.inventory_grid_shift();
        assert_eq!(sites.len(), 2, "q.a(II)V and q.b(II)V");
        // The measured halfwords, so a typo cannot pass for a patch.
        assert_eq!(sites[0], (0x0014_6da4, 0x3407, 0x340e), "q.a(II)V: adds r4,#7 -> #14");
        assert_eq!(sites[1], (0x0014_70a4, 0x1c16, 0x1dd6), "q.b(II)V: adds r6,r2,#0 -> #7");
        for (site, orig, patched) in sites {
            assert_ne!(orig, patched, "site {site:#x} would be a no-op");
        }
        // Where the grid ends up, and the room that leaves.
        let margin = Hero2Java::INV_CELL_X0 + Hero2Java::INV_GRID_SHIFT;
        assert_eq!(margin, 14, "the panel's left margin after the shift");
        // ⛔ The rail must stop before the first cell column starts.
        assert!(
            Hero2Java::BAG_RAIL_X + Hero2Java::BAG_RAIL_W <= margin,
            "the rail ({}..{}) reaches into cell column 0 (starts at {margin})",
            Hero2Java::BAG_RAIL_X,
            Hero2Java::BAG_RAIL_X + Hero2Java::BAG_RAIL_W
        );
        // ⛔ And the grid must not be pushed off the panel's right edge: column 6's box
        // ends at `margin + 6·pitch + pitch`, which is the panel width exactly at +7.
        let grid_right = margin + Hero2Java::INV_COLS * Hero2Java::INV_CELL_PITCH;
        assert!(grid_right <= Hero2Java::PANEL_W, "grid right edge {grid_right} is past the panel ({})", Hero2Java::PANEL_W);
        // Five tabs stand beside the four grid rows and nowhere else (the 소지금 bar and
        // the 재료가방 label own the full width above and below).
        let band = Hero2Java::INV_CELL_PITCH * 4;
        assert_eq!(band, 60);
        assert!(band / 5 >= 8, "a {}-px tab is not a touch target", band / 5);
    }

    /// ★★ hero2's Java-side table (2026-09-17): the measured slots, the class map and the
    /// write gate. hero3 builds have none of it — and must not: `hero2_java()` is what
    /// every hero2-only reader dispatches on, so a `Some` on hero3 would run hero2's
    /// pointer walks against hero3's flat C image.
    #[test]
    fn hero2_java_table_is_hero2_only_and_pins_the_measured_slots() {
        for b in [GuestBuild::Ktf, GuestBuild::Lgt, GuestBuild::Skt] {
            assert!(b.hero2_java().is_none(), "{b:?} has no Java data");
            for w in JavaWrite::ALL {
                assert!(!b.java_write_ok(w), "{b:?} must refuse {w:?}");
            }
        }
        let h2 = GuestBuild::Hero2Ktf.hero2_java().expect("hero2 has the table");
        // The quickbar and its cooldowns: `ag.j:[B` / `ag.a:[I`, probe-read as
        // `[1, 10, 5, 2]` / `[-1, -1, -1, -1]` at f=1200 (HERO2_QUICKSLOT_TOUCH_PLAN.md).
        assert_eq!(h2.quickbar_slot, 0x19_3bac);
        assert_eq!(h2.cooldown_slot, 0x19_3bbc);
        assert_eq!(h2.class_slot, 0x19_3a6c);
        // The window chain root and the geometry the game itself computes.
        assert_eq!(h2.menu_slot, 0x19_422c);
        assert_eq!(h2.menu_origin, (0x19_41ec, 0x19_41fc));
        assert_eq!(h2.screen, (0x19_4b4c, 0x19_4b5c, 0x19_4b6c, 0x19_4b7c));
        // Class descriptors are distinct, and all in the image's descriptor table.
        let c = h2.classes;
        let descs = [c.window, c.popup, c.skill_window, c.beast_window, c.field_menu, c.beast];
        for (i, d) in descs.iter().enumerate() {
            assert!((0x19_2238..=0x19_3144).contains(d), "descriptor {d:#x} outside the table");
            assert!(!descs[i + 1..].contains(d), "descriptor {d:#x} listed twice");
        }
        // The cooldown tables name the three class descriptors, each once.
        let mut seen = Vec::new();
        for (d, slot) in h2.cooldown_tables {
            assert!((0x19_2238..=0x19_3144).contains(&d));
            assert!(!seen.contains(&d));
            seen.push(d);
            assert!(slot > 0x19_3000 && slot < 0x19_5000, "static slot {slot:#x}");
        }
        // The popup box formula is the game's: 74 wide, 6 + 16n tall.
        assert_eq!(Hero2Java::POPUP_W, 74);
        assert_eq!(Hero2Java::POPUP_PAD_Y + Hero2Java::POPUP_ROW_H * 6, 102, "6 rows = 102 px, probe-measured");
        assert_eq!(Hero2Java::QUICKBAR_LEN, 4);
        assert_eq!(Hero2Java::BEAST_BASE, 10);
        // S7: the recovery cycle static (`bq.j:[B`, dump_hero2_java.py `slot@0x1940fc`)
        // and the inventory popup's two shapes — 3 rows is the 32-byte bucket's floor
        // for a `byte[][]` and 5 rows (28 B) is still inside it.
        assert_eq!(h2.heal_cycle_slot, 0x19_40fc);
        assert_eq!(h2.inventory_slot, 0x19_3b7c);
        assert_eq!((Hero2Java::ITEM_POPUP_NATIVE_ROWS, Hero2Java::ITEM_POPUP_OUR_ROWS), (3, 5));
        assert!(4 * u32::from(Hero2Java::ITEM_POPUP_OUR_ROWS) + 8 <= 32, "our label array must stay in the native bucket");
        assert_eq!(Hero2Java::POPUP_KIND_ITEM_USE, 6);
        // ★ 2026-09-23 (기본공격): the combo split. `a.i:[B` @0x19455c is the per-class
        // stage ceiling and `a.h:B` (inst +0x4c, so +0xa8 absolute — 0x5c past `a.p`'s
        // own +0xdc/+0x138 pair, which is how the two were cross-checked) is how many are
        // open. ⛔ `a.p` is 15 for every class and the stages come FIRST, so the split is
        // the only thing standing between a skill reset and the basic attack.
        assert_eq!(h2.combo_limits_slot, 0x19_455c);
        assert_eq!(h2.char_combo_open_off, 0xa8);
        assert_eq!(h2.char_skill_levels_off - h2.char_combo_open_off, 0x138 - 0xa8);
        assert_eq!(Hero2Java::CHAR_SKILL_SLOTS, 15);
        // 4 + 11 = 3 + 12 = 5 + 10: every class's stages plus its own skill table
        // (`t/sk0` · `sk1` · `sk2`) come to exactly the array's length.
        for (stages, skills) in [(4u32, 11u32), (3, 12), (5, 10)] {
            assert_eq!(stages + skills, Hero2Java::CHAR_SKILL_SLOTS);
        }
        // Every write kind is open on hero2 — the per-kind split exists so one can be
        // shut again without shutting the rest.
        // ⛔ The length assertion is what makes the two loops above and below a GATE
        // rather than a list somebody remembered to extend: a new variant fails here
        // until it has been considered on both sides.
        // 12 UI-data kinds + ★ #22's three CHARACTER-field kinds, which are a different
        // thing entirely: the eleven above are widget state the game rebuilds every time
        // it opens a window, these are fields it serialises into the next save — plus
        // ★ 2026-09-23's `CharCombo`, the 기본공격 repair, which is save-bearing too and
        // is besides the only one of the sixteen that can happen unasked — plus
        // ★ 2026-09-24's `BagOrder`, the ACCOUNT warehouse's Vector slots rotated so the
        // 재료가방 is cell 0 (host-owned bag, serialised into the host's own file) — plus
        // ★ 2026-09-24's `CharFacing`, a walking character's facing flipped to the exact
        // opposite between two tiles (the value the game writes one frame later anyway) —
        // plus `GroundDropLife`, a ground drop's lifetime byte refilled to the game's 120 —
        // plus 오토루팅's `GroundDropMove` (a fitting drop's tile set to the player's) and
        // `NoticeTaken` (`n.o` ← 0 once the host has copied the notice out) — plus
        // ★ 2026-09-25's `CharRevive` (부활의 구: HP/SP to their maxima, then the motion
        // words the game's own `g.b(1)` writes), save-bearing on its HP/SP half — plus
        // ★ 2026-09-26's `RecordEquipSlot`: a warehouse record's leftover worn flag (`aa.f`)
        // ← −1, the word the game's own unequip `a.e(B)Z` stores — on the host's bag only —
        // plus ★ 2026-09-27's `SaveFileNames`: `ag`'s two file-name statics pointed at the
        // host's five-name `String[]`s before anything has read them (세이브 슬롯 5).
        // …and ★ 2026-09-29's `MonsterStats`: the 난이도 multiplier on every monster's HP /
        // max HP / attack words (and the `ca` template's attack) — and ★ 2026-10-02's
        // `BagPageCells`: a paged bag window's `j.i`, 15 on the short fifth page.
        // …and ★ 2026-10-03's `CharSkillUnlock`: 발도's slot opened with 방어의지 on a knight.
        // …and ★ 2026-10-09's `PickerList`: a paged item picker's list, one page of the list the game built.
        // …and ★★ 2026-10-16's `CharRefill`: 엘릭서's HP · SP to their maxima — and `IconArrayRef`: 악의's skin exchanges in
        // the beast icon array `cf.i`.
        // …and ★★ 2026-10-16's `BagSold` · `SellWindow`: the inventory Vector and the sell window's list after the bulk sale.
        // …and ★★ 2026-10-20's `ShopStock`: the shop stock static `v.a` the 상점주인의 혼 fills before posting the shop event.
        assert_eq!(JavaWrite::ALL.len(), 36);
        assert_eq!(Hero2Java::ITEM_EQUIP_SLOT_OFF, 0x34, "aa.f:B — ch's 0x20 + aa's own +0x14");
        assert_eq!(Hero2Java::EQUIP_RECORD_CLASS_DESCS, [0x19_225c, 0x19_28d4, 0x19_27fc, 0x19_2d9c], "aa · bu · bo · z (dump_hero2_java.py)");
        for w in JavaWrite::ALL {
            assert!(GuestBuild::Hero2Ktf.java_write_ok(w));
        }
    }

    /// ★★ hero2's CALL gate (2026-09-21, #13 step 1): JVM calls by name. hero3 builds have
    /// no JVM and must refuse every one; hero2 opens exactly the measured six, spelled as
    /// the image's own method table spells them — and the save call is the SAME method the
    /// `Hero2Save` table has been calling since 2026-09-19, so the two cannot drift apart.
    #[test]
    fn hero2_java_calls_are_hero2_only_and_spell_the_measured_methods() {
        for b in [GuestBuild::Ktf, GuestBuild::Lgt, GuestBuild::Skt] {
            for c in JavaCall::ALL {
                assert!(!b.java_call_ok(c), "{b:?} must refuse {c:?}");
            }
        }
        for c in JavaCall::ALL {
            assert!(GuestBuild::Hero2Ktf.java_call_ok(c), "hero2 opens {c:?}");
        }
        let sv = GuestBuild::Hero2Ktf.hero2_save().expect("hero2 saves by name");
        let sp = JavaCall::Save.spec();
        assert_eq!((sp.class, sp.method, sp.descriptor, sp.kind), (sv.class, sv.method, "()V", CallKind::Virtual));
        assert_eq!(sp.singleton, Some((sv.class, sv.field, sv.descriptor)), "the save's receiver is the same singleton");
        // The container methods are instance methods of the bag class `bq` (desc 0x192844);
        // the record factory is the one STATIC — `ch.a(BBZZ)Lch;` @0x1408fc.
        for c in [JavaCall::BagInsertRecord, JavaCall::BagRemoveAt, JavaCall::BagAddCount, JavaCall::BagConsumeAt] {
            assert_eq!(c.spec().class, "bq", "{c:?}");
            assert_eq!(c.spec().kind, CallKind::Virtual, "{c:?} takes a bag as its receiver");
            assert!(c.spec().singleton.is_none(), "{c:?}: the bag is the caller's handle");
        }
        assert_eq!(JavaCall::RecordNew.spec(), JavaMethodSpec { class: "ch", method: "a", descriptor: "(BBZZ)Lch;", kind: CallKind::Static, singleton: None });
        // #13 step 5: the two constructors and the HUD flag call, spelled as the image does.
        assert_eq!(JavaCall::BagNew.spec(), JavaMethodSpec { class: "bq", method: "<init>", descriptor: "(B)V", kind: CallKind::Constructor, singleton: None });
        assert_eq!(JavaCall::InventoryWindowNew.spec(), JavaMethodSpec { class: "q", method: "<init>", descriptor: "(Lj;Lbq;Z)V", kind: CallKind::Constructor, singleton: None });
        assert_eq!(JavaCall::HudDirty.spec(), JavaMethodSpec { class: "n", method: "b", descriptor: "()V", kind: CallKind::Virtual, singleton: Some(("n", "a", "Ln;")) });
        assert_eq!(JavaCall::BagSerialize.spec().descriptor, "()[B");
        assert_eq!(JavaCall::BagDeserialize.spec().descriptor, "([B)V");
        assert_eq!(JavaCall::TableRows3.spec(), JavaMethodSpec { class: "i", method: "a", descriptor: "(III)[[B", kind: CallKind::Virtual, singleton: Some(("bw", "a", "Li;")) });
        // The game's own ‘개수를 입력하세요.’ box: `aq.<init>(Lj;II)V` @0x11dc94 =
        // `(parent, unitPrice, max)`. Nothing in the image constructs it (the network
        // warehouse died with its server), so this is the host's door to it.
        assert_eq!(JavaCall::QuantityPopupNew.spec(), JavaMethodSpec { class: "aq", method: "<init>", descriptor: "(Lj;II)V", kind: CallKind::Constructor, singleton: None });
        // 경험치 배율 (2026-09-23): the probe's end-to-end check of the EXP redirect —
        // `a.d(I)V` on the character singleton, the game's one "gain EXP" method.
        assert_eq!(JavaCall::CharGainExp.spec(), JavaMethodSpec { class: "a", method: "d", descriptor: "(I)V", kind: CallKind::Virtual, singleton: Some(("ag", "a", "La;")) });
        // 15 + ★ 2026-09-24's four PROBE-only checks (#17 · #20/#21): the facing monster,
        // its death routine, the class skill multiplier, the class cooldown total.
        // + ★ 2026-09-24's two for 오토루팅: the game's own pickup (static `ag.a()Z`) and
        // its notice renderer `n.a([BII)V` on the HUD singleton.
        assert_eq!(JavaCall::FieldPickup.spec(), JavaMethodSpec { class: "ag", method: "a", descriptor: "()Z", kind: CallKind::Static, singleton: None });
        assert_eq!(JavaCall::Notice.spec(), JavaMethodSpec { class: "n", method: "a", descriptor: "([BII)V", kind: CallKind::Virtual, singleton: Some(("n", "a", "Ln;")) });
        // + ★ 2026-09-25's PROBE-only `a.b(I)V` — the game's one HP-delta method, which
        // `--h2-die` hands `−hp` to so the fall the revive answers is the game's own.
        assert_eq!(JavaCall::CharHpDelta.spec(), JavaMethodSpec { class: "a", method: "b", descriptor: "(I)V", kind: CallKind::Virtual, singleton: Some(("ag", "a", "La;")) });
        // + ★ 2026-09-26's `ba.<init>(Lj;[B)V` @0x121404 — the game's own MESSAGE BOX
        // `(parent, text)`, which every notice of the warehouse and the 망각의 구 now is
        // (user: "안내팝업은 항상 기존 게임 팝업으로"). The class the touch router already
        // knows as `message_window` (tap = OK): the two must name the same class.
        assert_eq!(JavaCall::MessageBoxNew.spec(), JavaMethodSpec { class: "ba", method: "<init>", descriptor: "(Lj;[B)V", kind: CallKind::Constructor, singleton: None });
        assert_eq!(GuestBuild::Hero2Ktf.hero2_java().unwrap().classes.message_window, 0x19_2604, "ba's descriptor");
        // + ★ 2026-09-27's PROBE-only `a(I)I` — the class's SP cost table (`--h2-skill`).
        assert_eq!(JavaCall::CharSkillSpCost.spec(), JavaMethodSpec { class: "a", method: "a", descriptor: "(I)I", kind: CallKind::Virtual, singleton: Some(("ag", "a", "La;")) });
        // + ★ 2026-09-30's `PartsReload` — the game's own parts loader `cf.a(I)V` on the `cf`
        // singleton, the reload after a costume change (의상 아바타 A0).
        assert_eq!(JavaCall::PartsReload.spec(), JavaMethodSpec { class: "cf", method: "a", descriptor: "(I)V", kind: CallKind::Virtual, singleton: Some(("cf", "a", "Lcf;")) });
        // + ★ 2026-10-01's three: the rest of what the EQUIPMENT window's `cc.b(B)V` runs per
        // changed slot — clear a part's images (`cf.b(I)V`), the sparkle (`cf.a(Z)V`), the
        // swing sound (`cf.a(B)V`) — so the window's character follows the equipment live.
        assert_eq!(JavaCall::PartsClear.spec(), JavaMethodSpec { class: "cf", method: "b", descriptor: "(I)V", kind: CallKind::Virtual, singleton: Some(("cf", "a", "Lcf;")) });
        assert_eq!(JavaCall::PartsShine.spec(), JavaMethodSpec { class: "cf", method: "a", descriptor: "(Z)V", kind: CallKind::Virtual, singleton: Some(("cf", "a", "Lcf;")) });
        assert_eq!(JavaCall::PartsSwing.spec(), JavaMethodSpec { class: "cf", method: "a", descriptor: "(B)V", kind: CallKind::Virtual, singleton: Some(("cf", "a", "Lcf;")) });
        // + ★★ 2026-10-16 일괄판매: the grade names (`i.a(IIII)[[B` on the itmtp table), the sell window's own popup opener
        // (`j.a(BI[[B)V`, inherited — the receiver must be a live `cl`) and the static gold add the game's own sale calls.
        assert_eq!(JavaCall::ItemTypeRows4.spec(), JavaMethodSpec { class: "i", method: "a", descriptor: "(IIII)[[B", kind: CallKind::Virtual, singleton: Some(("cf", "c", "Li;")) });
        assert_eq!(JavaCall::SellPopupNew.spec(), JavaMethodSpec { class: "cl", method: "a", descriptor: "(BI[[B)V", kind: CallKind::Virtual, singleton: None });
        assert_eq!(JavaCall::GoldAdd.spec(), JavaMethodSpec { class: "ag", method: "g", descriptor: "(I)V", kind: CallKind::Static, singleton: None });
        assert_eq!(JavaCall::ALL.len(), 38);
        // ★★ 2026-10-21: the field menu's release — the game's own close, on the menu singleton (the 비행서 / 상점주인의 혼 exit).
        assert_eq!(JavaCall::MenuRelease.spec(), JavaMethodSpec { class: "bw", method: "a", descriptor: "(Z)V", kind: CallKind::Virtual, singleton: Some(("bw", "a", "Lbw;")) }, "bw.a(Z)V @0x13ab98 — what ag.g()V's close event 7 calls after ag.a <- 3");
        // ★★ 2026-10-20 상점주인의 혼: the stock loader and the event post the portable shop uses.
        assert_eq!(JavaCall::ShopStockLoad.spec(), JavaMethodSpec { class: "z", method: "a", descriptor: "()[Ljava/util/Vector;", kind: CallKind::Static, singleton: None }, "z.a()[Ljava/util/Vector; @0x140a90 — the shop stock from /itm/forshop");
        assert_eq!(JavaCall::EventPost.spec(), JavaMethodSpec { class: "ag", method: "a", descriptor: "(BB)V", kind: CallKind::Static, singleton: None }, "ag.a(BB)V @0x11085c — post (ag.d, ag.e); (4, 0) = the shop");
        assert_eq!(Hero2Java::SHOP_EVENT, (4, 0), "ag.g()V jump table entry 4 → 0x1116ca, ag.e 0 → 0x11172a = ag.h(7) · v.a()Lv; · v.a()V");
        assert_eq!(Hero2Java::SHOP_STOCK_SLOT, 0x0019_454c, "v.a:[Ljava/util/Vector; — cf.o()V's put, p.a()V's get");
        assert_eq!(JavaCall::ScriptGroup.spec(), JavaMethodSpec { class: "bb", method: "a", descriptor: "(B)V", kind: CallKind::Virtual, singleton: Some(("bb", "a", "Lbb;")) }, "★★ 2026-10-17 bb.a(B)V @0x122008 — run a trigger group's auto records (the raid's advance)");
        assert_eq!(JavaCall::BeastRefund.spec(), JavaMethodSpec { class: "bn", method: "d", descriptor: "()V", kind: CallKind::Virtual, singleton: None }, "★★ 2026-10-16 bn.d()V @0x135254 — the evolution's skill-point return, on the beast about to be discarded");
        let h2 = GuestBuild::Hero2Ktf.hero2_java().unwrap();
        assert_eq!(h2.menu_last_row_slot, 0x19_424c, "bw.a:B (dump_hero2_java.py --class bw)");
        assert_eq!(Hero2Java::MENU_ROW_ACCOUNT, 6, "CARD was row 6 of bw.b()Z");
        assert_eq!((Hero2Java::GM_ROW_ACCOUNT_FOCUSED, Hero2Java::GM_ROW_MATERIAL_BAG_TITLE), (19, 57));
        assert_eq!((Hero2Java::GM_ROW_TO_ACCOUNT, Hero2Java::GM_ROW_TO_BAG), (78, 79), "appended by make_hero2_asset.py after the 78 shipped rows");
        // ★ 2026-09-26: the notice rows follow the four row labels, in GM_APPEND's order —
        // the tool asserts the same list back after it writes the table.
        assert_eq!(
            [
                Hero2Java::GM_ROW_NOTE_EQUIPPED,
                Hero2Java::GM_ROW_NOTE_TO_ACCOUNT,
                Hero2Java::GM_ROW_NOTE_TO_BAG,
                Hero2Java::GM_ROW_NOTE_STAT_RESET,
                Hero2Java::GM_ROW_NOTE_SKILL_RESET,
                Hero2Java::GM_ROW_NOTE_NOTHING_TO_RESET,
                Hero2Java::GM_ROW_NOTE_RESET_FAILED,
                Hero2Java::GM_ROW_NOTE_NO_ACCOUNT,
            ],
            [82, 83, 84, 85, 86, 87, 88, 89]
        );
        assert_eq!(Hero2Java::GM_ROW_RESET_SKILL + 1, Hero2Java::GM_ROW_NOTE_EQUIPPED, "no gap after the last row label");
        // ★★ 2026-10-16 엘릭서: its own 사용하기 row and the full-vitals notice follow the notices (GM_APPEND rows 90 · 91)
        assert_eq!((Hero2Java::GM_ROW_USE_ELIXIR, Hero2Java::GM_ROW_NOTE_VITALS_FULL), (Hero2Java::GM_ROW_NOTE_NO_ACCOUNT + 1, Hero2Java::GM_ROW_NOTE_NO_ACCOUNT + 2));
        assert_eq!((Hero2Java::ORB_ID_ELIXIR, Hero2Java::BEAST_LIST_WINDOW_SLOT), (7, 0x0019_427c));
        // Overloads resolve by descriptor, so no two calls on one class may share one.
        for (i, a) in JavaCall::ALL.iter().enumerate() {
            for b in &JavaCall::ALL[i + 1..] {
                let (x, y) = (a.spec(), b.spec());
                assert!(x.class != y.class || x.method != y.method || x.descriptor != y.descriptor, "{a:?} and {b:?} name the same method");
            }
        }
        // The insert + remove pair is the identity-preserving move: the insert takes the
        // record OBJECT and a count, the removal takes an index (`bq.b(B)V`).
        assert_eq!(JavaCall::BagInsertRecord.spec().descriptor, "(Lch;I)Z");
        assert_eq!(JavaCall::BagRemoveAt.spec().descriptor, "(B)V");
        // The three bags are consecutive statics of `ag` — 0x10 apart, `ag.a` first
        // (dump_hero2_java.py --class ag: slot@0x193b7c / 0x193b8c / 0x193b9c).
        let h2 = GuestBuild::Hero2Ktf.hero2_java().expect("hero2 has the table");
        assert_eq!(h2.material_bag_slot, h2.inventory_slot + 0x10);
        assert_eq!(h2.card_bag_slot, h2.inventory_slot + 0x20);
        // The capacity the host raises the bag to, and why it stops there: indices are
        // signed bytes. Five windows of 28 cover it, the last one 15 deep.
        assert_eq!(Hero2Java::BAG_CAPACITY, i8::MAX);
        assert_eq!(Hero2Java::BAG_PAGE, 28);
        assert_eq!(Hero2Java::BAG_PAGES, (Hero2Java::BAG_CAPACITY as usize).div_ceil(Hero2Java::BAG_PAGE));
        assert_eq!(Hero2Java::BAG_CAPACITY as usize - 4 * Hero2Java::BAG_PAGE, 15);
    }

    // ── 2026-09-27: 로그 SP · 민첩 계수 · 무기 · 나이트 범위기 · 신의가호 · 민첩태세 무적 ──────────

    /// Thumb-1 decoders the 2026-09-27 tests share (the encodings the CPU runs).
    fn t1_movs(h: u16) -> Option<(u16, u16)> {
        (h & 0xf800 == 0x2000).then_some(((h >> 8) & 7, h & 0xff))
    }
    fn t1_b_target(at: u32, h: u16) -> u32 {
        assert_eq!(h & 0xf800, 0xe000, "{at:#x}: {h:#06x} is not an unconditional b");
        let imm = i32::from(h & 0x7ff) - if h & 0x400 != 0 { 0x800 } else { 0 };
        (at as i32 + 4 + 2 * imm) as u32
    }
    const T1_NOP: u16 = 0x46c0; // mov r8, r8

    #[test]
    fn the_2026_09_27_combat_rebalance_is_hero2_only() {
        for b in [GuestBuild::Ktf, GuestBuild::Lgt, GuestBuild::Skt] {
            assert!(b.rogue_skill_sp_cut().is_empty(), "{b:?} must have no rogue SP patch");
            assert!(b.agility_coefficients().is_empty(), "{b:?} must have no agility patch");
            assert!(b.sword_wave_range().is_empty(), "{b:?} must have no 검기 range patch");
            assert!(b.knight_strength_coefficient().is_empty(), "{b:?} must have no knight strength patch");
            assert!(b.knight_active_seven().is_empty(), "{b:?} must have no SKILL-window count patch");
            assert!(b.rage_shipped_off().is_empty() && b.berserker_patch().is_none(), "{b:?} must have no berserker block");
            // ★ 2026-10-13: nor the demon block.
            assert!(b.demon_patch().is_none(), "{b:?} must have no demon block");
            assert!(b.roar_cooldown_up().is_empty(), "{b:?} must have no 사자후 cooldown patch");
            assert!(b.blessing_aura_rework().is_empty(), "{b:?} must have no 신의가호 patch");
            assert!(b.agility_stance_invuln().is_empty(), "{b:?} must have no 민첩태세 patch");
            // ★ 2026-10-05: nor 철괴 (the old 방어의지).
            assert!(b.iron_wall_rework().is_empty(), "{b:?} must have no 철괴 patch");
            // ★ 2026-10-05: nor the 빛의회복 buff.
            assert!(b.beast_recovery_buff().is_empty(), "{b:?} must have no 빛의회복 patch");
            // ★ 2026-10-06: nor the beast's 지력 weight and attack-skill SP.
            assert!(b.beast_int_weight().is_empty(), "{b:?} must have no beast 지력 patch");
            assert!(b.beast_attack_sp_cut().is_empty(), "{b:?} must have no beast SP patch");
            // ★ 2026-10-12: nor the beast's attack cooldown / power, the seals' 상급, the slot-screen focus.
            assert!(b.beast_attack_cooldown_cut().is_empty(), "{b:?} must have no beast cooldown patch");
            assert!(b.beast_attack_power().is_empty(), "{b:?} must have no beast power patch");
            assert!(b.seal_top_tier().is_empty(), "{b:?} must have no seal patch");
            assert!(b.slot_focus_patch().is_none(), "{b:?} must have no slot focus");
            // ★ 2026-10-06: nor the buff cooldowns at a minute.
            assert!(b.buff_cooldown_minute().is_empty(), "{b:?} must have no buff cooldown patch");
            assert!(b.combat_patch().is_none(), "{b:?} must have no combat block");
            // ★ 2026-09-29: nor the 맹독감염 buff.
            assert!(b.venom_patch().is_none(), "{b:?} must have no 맹독감염 block");
            // ★ 2026-09-30: nor the 의상 아바타 override.
            assert!(b.avatar_patch().is_none(), "{b:?} must have no avatar override");
            // ★ 2026-09-30: nor the 야수진화제 sell price.
            assert!(b.shop_sell_patch().is_none(), "{b:?} must have no sell-price block");
            // ★ 2026-10-10: nor the boss crit floor.
            assert!(b.boss_crit_patch().is_none(), "{b:?} must have no boss crit block");
            // ★ 2026-10-13: nor the grade line.
            assert!(b.grade_line_patch().is_none(), "{b:?} must have no grade line");
            // ★ 2026-10-02: nor the HELL bonus drop.
            assert!(b.hell_drop_patch().is_none(), "{b:?} must have no HELL drop block");
            // ★ 2026-10-02: nor the six skill-changing accessories.
            assert!(b.accessory_patch().is_none(), "{b:?} must have no accessory block");
            // ★ 2026-10-05: nor 일풍격.
            assert!(b.ilpung_patch().is_none(), "{b:?} must have no 일풍격 block");
        }
        // ★ 2026-09-30: the avatar override is verify-then-write too — every region differs
        // from the shipped bytes, every site is halfword-aligned inside hero2's text, and
        // the literal words it fills lie inside its own regions (never in the text around).
        for r in GuestBuild::Hero2Ktf.avatar_patch().unwrap().regions {
            assert_ne!(r.shipped, r.patched);
            assert_eq!(r.at & 1, 0);
            assert!((0x0010_0000..0x0019_0000).contains(&r.at), "{:#x}: hero2's text", r.at);
            assert!(r.lit_at >= r.at && r.lit_at + 4 <= r.at + 2 * r.shipped.len() as u32, "{:#x}: the literal inside the region", r.lit_at);
            if let Some((at, shipped, patched)) = r.branch {
                assert!((0x0010_0000..0x0019_0000).contains(&at), "{at:#x}: hero2's text");
                assert_ne!(shipped, patched);
            }
        }
        // Every table is verify-then-write: a site's shipped and patched halfwords differ,
        // and every site is a halfword address inside hero2's text.
        let b = GuestBuild::Hero2Ktf;
        let venom = b.venom_patch().unwrap();
        for list in [b.rogue_skill_sp_cut(), b.agility_coefficients(), b.sword_wave_range(), b.knight_active_seven(), b.rage_shipped_off(), b.roar_cooldown_up(), b.blessing_aura_rework(), b.agility_stance_invuln(), b.iron_wall_rework(), b.beast_recovery_buff(), b.beast_int_weight(), b.beast_attack_sp_cut(), b.beast_attack_cooldown_cut(), b.beast_attack_power(), b.seal_top_tier(), b.buff_cooldown_minute(), venom.cast, venom.cooldown, &b.buff_refresh_patch().unwrap().widen[..]] {
            assert!(!list.is_empty());
            for (site, orig, patched) in list {
                assert_ne!(orig, patched, "{site:#x}: a no-op entry");
                assert_eq!(site & 1, 0, "{site:#x}: halfword aligned");
                assert!((0x0010_0000..0x0019_0000).contains(site), "{site:#x}: hero2's text");
            }
        }
    }

    /// ★ 2026-10-12: the beast tables as the user's numbers — 지력 ×2 (the 「/2」 halfwords keep 지력 whole), the four
    /// damage formulas `/3 → /2` (×1.5), the attack cooldown 80 → 64 and ×10 → ×8 (×0.8), the attack SP once more ×0.8;
    /// 철괴's 지력 term and cap; the seal table's id-3 numbers.
    #[test]
    fn the_2026_10_12_beast_seal_and_knight_numbers() {
        let b = GuestBuild::Hero2Ktf;
        // movs r1,#3 → #2 at the four divides
        let power = b.beast_attack_power();
        assert_eq!(power.len(), 4);
        assert!(power.iter().all(|(_, o, p)| t1_movs(*o) == Some((1, 3)) && t1_movs(*p) == Some((1, 2))));
        // the cooldown: 암살자 80 → 64 · the 7..12 tail ×10 → ×8
        assert_eq!(b.beast_attack_cooldown_cut().iter().map(|(_, o, p)| (t1_movs(*o).unwrap(), t1_movs(*p).unwrap())).collect::<Vec<_>>(), vec![((0, 80), (0, 64)), ((4, 10), (4, 8))]);
        // the SP stubs: 45 · 89 − 6·lv · 115 − 4·lv (each ≈ ×0.8 of 56 · 112 − 8·lv · 144 − 5·lv)
        let sp: std::collections::BTreeMap<u32, u16> = b.beast_attack_sp_cut().iter().map(|(a, _, p)| (*a, *p)).collect();
        assert_eq!(t1_movs(sp[&0x0013_55b0]), Some((0, 45)));
        assert_eq!((t1_movs(sp[&0x0013_55c2]), t1_movs(sp[&0x0013_55c4])), (Some((1, 89)), Some((0, 6))));
        assert_eq!((t1_movs(sp[&0x0013_55ca]), t1_movs(sp[&0x0013_55cc])), (Some((1, 115)), Some((0, 4))));
        for lv in 1..=5i32 {
            assert!(((89 - 6 * lv) - (112 - 8 * lv) * 4 / 5).abs() <= 2, "rings lv {lv}");
            assert!(((115 - 4 * lv) - (144 - 5 * lv) * 4 / 5).abs() <= 1, "heavy lv {lv}");
        }
        // 지력 ×2: the two 「/2」 halfwords now keep it whole (adds rX,rY,#0)
        let int: std::collections::BTreeMap<u32, u16> = b.beast_int_weight().iter().map(|(a, _, p)| (*a, *p)).collect();
        assert_eq!((int[&0x0013_4ee0], int[&0x0013_4b60]), (0x1c36, 0x1c2e));
        assert_eq!(b.beast_int_weight().len(), 17, "11 (2026-10-06) + 6 for the two status ticks");
        // 철괴 · 치유오라
        assert_eq!((CombatPatch::IRON_CUT_PER_INT, CombatPatch::IRON_CUT_MAX), (2, 500), "지력 5 = 1 %p, 50 % at most");
        assert_eq!(CombatPatch::iron_reduction_permille(5, 5, 125), 500);
        assert_eq!(CombatPatch::heal_amount(400, 100), 800);
        let c = b.combat_patch().unwrap();
        assert_eq!((c.heal_pool, c.heal_arm, c.heal_cell), ((0x0013_b7a4, 0xfffe_5f48), (0x0013_b540, 0x4450, 0x4780), 0x0017_bbec));
        assert!(CombatPatch::IRON_AT < CombatPatch::MAX_AT && CombatPatch::MAX_AT < CombatPatch::HEAL_AT && CombatPatch::HEAL_AT < CombatPatch::BLOCK_LEN);
        // the seals
        assert_eq!([0u8, 1, 2, 3].map(GuestBuild::seal_range), [(0, 4), (2, 7), (5, 10), (15, 20)]);
        assert_eq!(b.seal_top_tier().len(), 16);
        assert_eq!(t1_b_target(0x0014_b6e4, 0xe01a), 0x0014_b71c);
        // the slot focus
        let f = b.slot_focus_patch().unwrap();
        assert_eq!((f.def - 0x0018_5644) % 0x1c, 0, "a method definition record");
        assert!(SlotFocusPatch::LAST + 4 <= SlotFocusPatch::BLOCK_LEN && SlotFocusPatch::LAST % 4 == 0);
    }

    /// ★ 2026-10-07 버서커 분노태세 ON/OFF · 도발: the definitions are records of the method table, the ones the
    /// accessory / SkillPower blocks wrap are the same words (the berserker block chains onto them), the statics,
    /// descriptors and offsets are the measured ones, the pieces sit in order inside the block, and the window
    /// gives the berserker eight rows / four right cells while the other classes keep seven / three.
    #[test]
    fn the_berserker_patch_is_measured_and_chains_onto_the_blocks_it_shares_words_with() {
        let b = GuestBuild::Hero2Ktf;
        let p = b.berserker_patch().unwrap();
        let acc = b.accessory_patch().unwrap();
        for (def, body) in [p.sp_cost, p.cooldown, p.max_level, p.drive, p.damage, p.mob_update] {
            assert_eq!((def - 0x0018_5644) % 0x1c, 0, "{def:#x}: a method definition record");
            assert_eq!(body & 1, 0);
        }
        assert_eq!((p.sp_cost, p.cooldown), (acc.sp_cost, acc.cooldown), "the accessory block's berserker words — chained");
        assert_eq!((p.damage.0, p.damage.1), (b.skill_power_patch().unwrap().jobs[1].0, b.skill_power_patch().unwrap().jobs[1].1), "SkillPower's bl");
        assert_eq!((p.char_slot, p.stage_slot, p.monster_descs), (acc.char_slot, acc.stage_slot, acc.monster_descs));
        assert_eq!((p.dispatch, p.call_static, p.divide), (acc.dispatch, acc.call_static, b.ilpung_patch().unwrap().divide));
        // a.n:Z +0xa8 on the character's 0x5c · bd.c / bd.f / bd.g +0x38 / +0x3c / +0x40 on the monster's 0x5c
        assert_eq!(p.rage_off, 0x5c + 0xa8);
        assert_eq!((p.mob_target_off, p.mob_visible_off, p.mob_aggro_off), (0x5c + 0x38, 0x5c + 0x3c, 0x5c + 0x40));
        assert_eq!((p.skills_off, p.queue_off, p.queue_at_off, p.frame_off), (AccessoryPatch::SKILLS_OFF, AccessoryPatch::QUEUE_OFF, AccessoryPatch::QUEUE_AT_OFF, AccessoryPatch::FRAME_OFF));
        assert_eq!((p.state_off, p.next_off, p.stage_head_off, p.buffs_off), (AccessoryPatch::STATE_OFF, AccessoryPatch::NEXT_OFF, AccessoryPatch::STAGE_HEAD_OFF, BuffRefreshPatch::BUFFS_OFF));
        // the pieces in order, inside the block, the data words first
        type BP = BerserkerPatch;
        let at = [BP::HAS_BUFF_AT, BP::SP_AT, BP::COOLDOWN_AT, BP::MAX_AT, BP::SEQ_AT, BP::USABLE_AT, BP::DAMAGE_AT, BP::TAKEN_AT, BP::MOB_AT, BP::UPDATE_AT, BP::DRIVE_AT];
        assert!(BP::FURY_LAST_MUL + 4 <= at[0]);
        assert!(at.windows(2).all(|w| w[0] < w[1]) && at[10] < BP::BLOCK_LEN && at.iter().all(|a| a & 3 == 0));
        // ★ 2026-10-07 (2): the window — eight ACTIVE cells for every class (the two awakenings fill the knight's and the
        // rogue's eighth); the right column 4 cells (`movs r6,#3 → #4`)
        assert_eq!(
            b.knight_active_seven(),
            &[(0x0014_9332, 0x2407, 0x2408), (0x0014_9356, 0x2406, 0x2408), (0x0014_8b46, 0xd000, 0xd100), (0x0014_8b4a, 0x2603, 0x2604), (0x0014_8d04, 0x2602, 0x2604)]
        );
        // the awakenings' words: 격분 kind 16 / value 9 / cell 7, the sky and the bloom kind 15 · 16 / value 8 / cell 7
        assert_eq!((BP::FURY_KIND, BP::FURY_VALUE, BP::FURY_CELL, BP::FURY_ID), (16, 9, 7, 33));
        assert_eq!((CombatPatch::SKY, CombatPatch::SKY_VALUE, CombatPatch::SKY_CD_INDEX), (15, 8, 7));
        assert_eq!((RoguePatch::BLOOM_KIND, RoguePatch::BLOOM_VALUE, RoguePatch::BLOOM_CD_INDEX), (16, 8, 7));
        assert_eq!((CombatPatch::SKY_COOLDOWN, RoguePatch::BLOOM_COOLDOWN, BP::FURY_COOLDOWN, BP::FURY_FRAMES), (800, 800, 600, 1200), "40 s · 40 s · 30 s · 60 s");
        // 몸통치기 retired: locked in the level table (`movs r2,#8 → #99`) and the HP short-cut gone (`lsls/asrs → adds r5,r5,#0`)
        assert_eq!(b.bodyslam_locked(), &[(0x0015_36d0, 0x2208, 0x2263)]);
        assert_eq!(b.damage_cap_off(), &[(0x0012_7fa6, 0x042b, 0x1c2d), (0x0012_7faa, 0x141d, 0x1c2d)]);
        // ★★ 2026-10-16 monster HP is an int: the other five stores' `lsls #16 ; asrs #16` pairs → moves (lsls #15 → asrs #1
        // for the template's x/2). Every shipped halfword is an lsls/asrs by an immediate; every patched one is
        // `adds rD,rS,#0` except the template's asrs, and none touches an address another list patches.
        let hp = b.monster_hp_int();
        assert_eq!(hp.len(), 10);
        for (at, shipped, patched) in hp {
            assert!(at % 2 == 0 && (0x0012_0000..0x0017_0000).contains(at), "{at:#x}");
            assert!(shipped >> 11 == 0b00000 || shipped >> 11 == 0b00010, "{at:#x}: shipped {shipped:#06x} is lsls/asrs #imm");
            assert!(patched >> 6 == 0x1c00 >> 6 || *patched == 0x1043, "{at:#x}: patched {patched:#06x}");
            assert!(!b.damage_cap_off().iter().any(|(a, _, _)| a == at));
        }
        for build in [GuestBuild::Ktf, GuestBuild::Lgt, GuestBuild::Skt] {
            assert!(build.bodyslam_locked().is_empty() && build.damage_cap_off().is_empty() && build.monster_hp_int().is_empty());
        }
        assert_eq!(BuffRefreshPatch::RAGE_ID, BerserkerPatch::RAGE_ID);
    }

    /// ★ 2026-09-29 맹독감염 → buff (user: *"맹독감염 5레벨 + 독연마 마스터하면 … 20초지속 30초
    /// 쿨타임에서 29초지속 30초 쿨타임"* · *"0.5프레임당 300데미지로"*): the words it arms are no
    /// other patch's, the cells and slots it shares with the combat block agree, and the numbers
    /// are the user's.
    /// ★★ 2026-10-13 「악마화」 D1: the 악의 block's words are hero2's own — each a method-definition record whose body is
    /// a Thumb entry, shared with the blocks it chains onto — and its offsets are the beast's (on `ad`'s 0x34) and the
    /// character's (on `g`'s 0x5c); the user's numbers; the whitelist by quickbar value; the claw windows.
    #[test]
    fn the_demon_patch_is_measured_and_chains_onto_the_blocks_it_shares_words_with() {
        let b = GuestBuild::Hero2Ktf;
        let p = b.demon_patch().unwrap();
        let (bp, acc, cp, ip, h2) = (b.berserker_patch().unwrap(), b.accessory_patch().unwrap(), b.combat_patch().unwrap(), b.ilpung_patch().unwrap(), b.hero2_java().unwrap());
        for (def, body) in [p.cast, p.arm, p.drives[0], p.drives[1], p.drives[2], p.update, p.beast_sp, p.beast_cooldown, p.draw, p.ground_draw, p.hud_draw, p.spawn] {
            assert_eq!((def - 0x0018_5644) % 0x1c, 0, "{def:#x}: a method definition record");
            assert_eq!(body & 1, 0);
        }
        // ★ D2: the static sprite drawer n.a(BBBII)V (definition #1151 — dump_hero2_java --json: def_fo 0x8d428, body 0x57099 + 0x10bf10) ·
        // the OK key's arming ag.c(B)V = the accessory block's word (the cast goes through it) · the static a.n:[B
        assert_eq!(p.draw, (0x0018_d428, 0x0016_2fa8));
        assert_eq!((p.draw.0 - 0x0018_5644) / 0x1c, 1151);
        assert_eq!(p.key_arm, acc.arm.0, "ag.c(B)V — the accessory block wraps it; the cast reads its word at run time");
        assert_eq!(p.frames_slot, cp.frames_slot, "a.n:[B");
        assert_eq!((DP::STATE_TRANSFORM, DP::STATE_ON, DP::STATE_REVERT), (1, 2, 3));
        assert_eq!((DP::TRANSFORM_FRAMES, DP::T1_GRACE, DP::T1_TIMEOUT, DP::T3_TIMEOUT), (20, 4, 60, 100));
        assert_eq!((DP::COSTUME, DP::BODY_BASE, DP::STAGE0_SEQ, DP::SEQ_TABLE_LEN, DP::SEQ_NONE, DP::BUFF_ID, DP::ICON_CELL), (300, 9, 4, 64, 0xff, 201, 9));
        assert!(DP::COSTUME >= AvatarPatch::COSTUME_MIN && DP::COSTUME <= AvatarPatch::COSTUME_MAX, "a costume number the loader's path and the jar's sheets accept");
        assert_ne!(DP::BUFF_ID, GearPatch::DRAGON_BUFF_ID);
        assert_eq!(DP::SEQ_TABLE + DP::SEQ_TABLE_LEN as u32, DP::LINE_REF, "★ D3: the table ends where the by-address words begin");
        assert!(DP::TRANSFORM_SEQ + 4 <= DP::BLADE && DP::JUDGE_DMG + 4 <= DP::SEQ_TABLE, "the D1 · D2 words, then the D3 words, then the table");
        // ★ D3: the toggles' words sit within an `ldr rt,[rn,#imm]` (≤ 124) of the block register; the judgement table ends before
        // the first piece; the two drawing words and the camera are past the table (reached by address)
        for off in [DP::BLADE, DP::JUDGE, DP::BLADE_SP_AT, DP::JUDGE_SP_AT, DP::BLADE_HIT_AT, DP::JUDGE_FROM, DP::BLADE_HITS, DP::JUDGE_HITS, DP::BLADE_SLOT, DP::JUDGE_SLOT, DP::TOGGLES, DP::BLADE_DMG, DP::JUDGE_DMG] {
            assert!(off >= DP::TRANSFORM_SEQ + 4 && off + 4 <= DP::SEQ_TABLE && off <= 124 && off % 4 == 0, "{off:#x}");
        }
        assert!(DP::LINE_REF < DP::CHAIN_GROUND && DP::CHAIN_GROUND < DP::CHAIN_HUD && DP::CHAIN_HUD < DP::CAM_X && DP::CAM_X < DP::CAM_Y && DP::CAM_Y < DP::CX && DP::CX < DP::CY && DP::CY < DP::EPTR && DP::EPTR + 4 <= DP::JUDGE_TAB);
        assert_eq!(DP::JUDGE_TAB + DP::JUDGE_SLOTS * DP::JUDGE_ENTRY, DP::CAST_AT, "the table ends where the pieces begin");
        assert_eq!((DP::JUDGE_SLOTS, DP::JUDGE_ENTRY, DP::TOGGLE_TARGETS), (32, 16, 25));
        // ★ D3: the ground draw · the HUD painter · the Graphics refs = the sky block's; bd.a(IB)V's cell = the venom block's;
        // the quickbar statics = Hero2Java's; bn.d:S on ad's 0x34; ad.a:S / ad.b:S the world px
        let (sd, vp) = (b.sky_draw_patch().unwrap(), b.venom_patch().unwrap());
        assert_eq!((p.ground_draw, p.hud_draw), (sd.ground_draw, sd.hud_draw));
        assert_eq!((p.iface_call, p.graphics_slot, p.set_alpha_ref, p.set_color_ref, p.fill_rect_ref), (sd.iface_call, sd.graphics_slot, sd.set_alpha_ref, sd.set_color_ref, sd.fill_rect_ref));
        assert_eq!(p.cell_mob_hurt, vp.cell_damage, "bd.a(IB)V — the poison tick's way in");
        assert_eq!((p.quickbar_slot, p.cooldown_slot), (h2.quickbar_slot, h2.cooldown_slot));
        assert_eq!((p.beast_power_off, p.px_x_off, p.px_y_off), (0x34 + 0x20, 0x0, 0x4));
        // ★ D3: the user's numbers — 암살자 3 · 심판번개 10 · 7×7 every 2 ticks at 7 % (70 %/s) · 9×9 every 4 ticks at 20 % after a
        // second · SP 6 % / 8 % a second · a second's lock on the slot; the formulas as the game computes them now
        assert_eq!((DP::BLADE_KIND, DP::JUDGE_KIND, DP::BLADE_WINDOW, DP::JUDGE_HALF), (3, 10, DP::window(6, 3, true), 4));
        // ★★ 2026-10-16 (user: SP up · damage down): the tick 7 → 5 % (50 %/s) · the drain 6 → 12 % with a 15-a-second floor
        assert_eq!((DP::BLADE_EVERY, DP::BLADE_PCT, DP::JUDGE_EVERY, DP::JUDGE_PCT, DP::JUDGE_WINDUP), (2, 5, 4, 20, 20));
        assert_eq!((DP::BLADE_SP_PCT, DP::BLADE_SP_MIN, DP::JUDGE_SP_PCT, DP::SP_EVERY, DP::TOGGLE_CD), (12, 15, 8, 20, 20));
        assert_eq!(DP::BLADE_EVERY * DP::BLADE_PCT as u32 * (DP::SP_EVERY / DP::BLADE_EVERY) / DP::BLADE_EVERY, 50, "5 % × 10 ticks a second");
        assert_eq!((DP::toggle_drain(1000, DP::BLADE_SP_PCT, DP::BLADE_SP_MIN), DP::toggle_drain(100, DP::BLADE_SP_PCT, DP::BLADE_SP_MIN), DP::toggle_drain(100, DP::JUDGE_SP_PCT, 0)), (120, 15, 8));
        assert_eq!(DP::RESET_WORDS, [DP::BLADE, DP::JUDGE, DP::FOLLOW, DP::CHARGE, DP::FIRE]);
        // ★★ 2026-10-24 TOGGLE moved behind FXT (its switch-on sound outgrew 0xa80's 320 B); RESET · SLOTCD stay where they were,
        // in the TRANSFORM piece's spare room now — and the SND piece between FXT and TOGGLE, before the D6 words
        assert!(DP::TRANSFORM_AT < DP::RESET_AT && DP::RESET_AT < DP::SLOTCD_AT && DP::SLOTCD_AT < DP::BLADE_AT, "RESET · SLOTCD after TRANSFORM");
        assert!(DP::FXT_AT < DP::SND_AT && DP::SND_AT < DP::TOGGLE_AT && DP::TOGGLE_AT < DP::D6 && DP::D6 - DP::TOGGLE_AT >= 320, "SND · TOGGLE in FXT's spare room");
        assert_eq!(DP::SND_CUES.map(|c| c.0), [DP::HIT_SOUND, DP::HIT_SOUND, DP::FIRE_SOUND, DP::BUFF_SOUND, DP::FIRE_SOUND]);
        assert_eq!((DP::HIT_SOUND, DP::FIRE_SOUND, DP::BUFF_SOUND), (4, 3, 2), "br.a(B)V slots: the monster hit · 속성공격합 · the beast's buff");
        assert_eq!(DP::assassin_tick(22, 13, 3), 360);
        assert_eq!(DP::heavy_hit(22, 13, 1), 172);
        for power in [0, 55, 59, 1000] {
            assert_eq!(DP::skill_power(power, 0), (power + 17) * 600 / 500);
            assert_eq!(DP::skill_power_with_attack(power, 200, 500), (power + 1000) * 800 / 500);
            for int in 0..600 {
                assert!(DP::skill_power(power, int + 1) >= DP::skill_power(power, int));
                assert!(DP::skill_power_with_attack(power, int, 1000) > DP::skill_power_with_attack(power, int, 500));
            }
        }
        assert_eq!([0, 20, 100, 200, 600].map(|i| DP::lava_power(1000, i)), [1200, 1240, 1400, 2400, 5600]);
        assert_eq!((DP::blade_tick(0, 0, 1), DP::judge_tick(0, -10, 1), DP::judge_tick(0, 0, 1)), (1, 6, 6));
        assert_eq!((DP::toggle_cost(1000, DP::BLADE_SP_PCT), DP::toggle_cost(1000, DP::JUDGE_SP_PCT)), (120, 80));
        assert!(DP::in_square(3, 3, -3) && !DP::in_square(3, 4, 0) && DP::in_square(4, -4, 4));
        assert_ne!(DP::BLADE_BUFF_ID, DP::BUFF_ID);
        assert_ne!(DP::JUDGE_BUFF_ID, DP::BLADE_BUFF_ID);
        assert_eq!(DP::SHIPPED_SEQS, [23, 27, 23], "berserker_taunt · awakenings / knight_iaido · awakenings / rogue_* · awakenings — the asset's sequence counts before demon_look.py");
        for c in 0..3u8 {
            let row = DP::MAPPED[usize::from(c)];
            assert_eq!(row.len(), DP::MAPPED_COUNT);
            assert!(row.windows(2).all(|w| w[0] < w[1]) && row.iter().all(|s| *s < DP::SHIPPED_SEQS[usize::from(c)]));
            assert_eq!(DP::transform_seq(c), Some(DP::SHIPPED_SEQS[usize::from(c)] + DP::MAPPED_COUNT as u8));
            assert_eq!(DP::demon_seq(c, 0), Some(DP::SHIPPED_SEQS[usize::from(c)]), "class {c}: the stand's copy is the first");
        }
        assert_eq!(DP::demon_seq(0, 14), None, "an effect-only sequence has no copy");
        assert_eq!((p.drives[0], p.update), (bp.drive, bp.update), "the berserker block's drive and update — chained");
        assert_eq!(p.drives[1], cp.cast, "the combat block's 나이트 cast drive — chained");
        assert_eq!(p.drives[2], acc.drive_rogue, "the accessory block's 로그 drive — chained");
        assert_eq!(p.arm, ip.queue, "일풍격's a.d(B)Z — chained");
        assert_eq!((p.dispatch, p.call_static, p.divide), (bp.dispatch, bp.call_static, bp.divide));
        assert_eq!((p.cell_sound, p.cell_attack), (bp.cell_sound, bp.cell_attack));
        assert_eq!((p.char_slot, p.stage_slot, p.class_slot, p.monster_descs), (bp.char_slot, bp.stage_slot, bp.class_slot, bp.monster_descs));
        assert_eq!((p.beast_off, p.beast_ids_off, p.beast_levels_off), (h2.beast_off, h2.beast_skill_ids_off, AccessoryPatch::BEAST_LEVELS_OFF));
        // bn on ad's 0x34: bn.j +0x8 · bn.k +0xc · bn.a:B +0x2c — ⛔ not g's 0x5c (HERO2_GUEST_RE §8.67's 0x88 was that mistake)
        assert_eq!((p.beast_ids_off, p.beast_levels_off, p.beast_state_off), (0x34 + 0x8, 0x34 + 0xc, 0x34 + 0x2c));
        assert_eq!((p.sp_off, p.max_sp_off), (GearPatch::CHAR_SP_OFF, GearPatch::CHAR_SP_MAX_OFF));
        assert_eq!((p.int_off, p.bonus_off), (CombatPatch::ABILITY_INT_OFF, CombatPatch::EQUIP_BONUS_OFF));
        assert_eq!((p.queue_off, p.queue_at_off, p.frame_off, p.facing_off, p.state_off), (bp.queue_off, bp.queue_at_off, bp.frame_off, Hero2Java::CHAR_FACING_OFF, bp.state_off));
        assert_eq!((p.next_off, p.stage_head_off), (bp.next_off, bp.stage_head_off));
        assert_eq!((p.tile_x_off, p.tile_y_off), (Hero2Java::CHAR_TILE_X_OFF, Hero2Java::CHAR_TILE_Y_OFF));
        assert_eq!((p.attr_l_off, p.attr_m_off), (0x5c + 0x78, 0x5c + 0x7c), "a.l:B · a.m:B — the hit's two bytes");
        assert_eq!((p.hit_def - 0x0018_5644) % 0x1c, 0, "a.a(Lbd;BIBB)Z's definition record");
        assert_eq!(p.combat_slot, 0x0019_400c, "bf.a:Z");
        // the pieces in order inside the block, the words first
        type DP = DemonPatch;
        // ★★ 2026-10-16 (악의 무기) the ARM piece in the claw's old room, the CLAW piece at the end of the block
        let at = [
            DP::CAST_AT, DP::SP_AT, DP::CD_AT, DP::TAKEN_AT, DP::TICK_AT, DP::DRIVE_AT[0], DP::DRIVE_AT[1], DP::DRIVE_AT[2], DP::ARM_AT, DP::GROUND_AT, DP::TRANSFORM_AT,
            DP::BLADE_AT, DP::JUDGE_AT, DP::HUD_AT, // ★★ 2026-10-24 TOGGLE moved behind FXT (asserted with SND above)
        ];
        assert!(DP::ENDS + 4 <= at[0] && at.windows(2).all(|w| w[0] < w[1]) && at[at.len() - 1] < DP::BLOCK_LEN && at.iter().all(|a| a & 3 == 0));
        // ★★ 2026-10-15 HD: the COLLECT piece lives behind the DRAW piece (its ellipse test outgrew the old 0xc0 room)
        assert!(DP::DRAW_AT < DP::COLLECT_AT && DP::COLLECT_AT + 0x140 == DP::CLAW_AT && DP::COLLECT_AT & 3 == 0);
        assert!(DP::CLAW_AT + 0x3c0 == DP::BLOCK_LEN && DP::CLAW_AT & 3 == 0, "★★ 2026-10-16 the claw behind COLLECT, 960 bytes");
        // ★★ (5): the POSE words after the D5 room, the DRAW piece after them — every word within one ldr of block + D6
        assert!(DP::FXT_AT < DP::D6 && DP::D6 < DP::DRAW_AT && DP::DRAW_AT < DP::BLOCK_LEN && DP::DRAW_AT & 3 == 0);
        for w in [DP::SH_UNTIL, DP::SH_BLADE, DP::SH_JUDGE, DP::SH_FIRES, DP::SH_LAVAS, DP::SH_FOLLOWS, DP::SH_BLASTS, DP::POSE_KIND, DP::POSE_START, DP::POSES_STARTED] {
            assert!(w >= DP::D6 && w + 4 <= DP::DRAW_AT && (w - DP::D6) <= 124 && w & 3 == 0, "{w:#x}");
        }
        assert_eq!((DP::POSE_LENS[DP::POSE_JUDGE as usize] as u32 - 4, DP::JUDGE_WINDUP), (20, 20), "어둠심판's fist comes down at its wind-up");
        assert!(DP::POSE_LENS[DP::POSE_FIRE as usize] as u32 > DP::FIRE_WINDUP, "멸화's roar after the gather");
        assert_eq!((DP::pose_seq(0, 0), DP::pose_seq(1, DP::POSE_IDLE), DP::pose_seq(2, DP::POSE_IDLE)), (Some(42), Some(53), Some(49)));
        // the user's numbers: 60 s · 90 s from the cast · SP ¼ · −15 % · 3 stages · 17 + 10 · 1.0 × an all-힘 berserker's 기본공격
        assert_eq!((DP::KIND, DP::FRAMES, DP::COOLDOWN, DP::SP_SHIFT, DP::TAKEN_NUM, DP::TAKEN_DEN, DP::STAGES, DP::RECOVER), (9, 1200, 1800, 2, 85, 100, 3, 10));
        assert_eq!((DP::taken(100), DP::taken(0), DP::taken(-3)), (85, 0, -3));
        assert_eq!(DP::claw_attack(230, 104), 721, "the LV 85 benchmark: the knight's 230 + 104 × 4.73 ≈ the berserker's 722");
        assert_eq!(DP::HIT_FRAMES, [&[1u8, 3][..], &[2][..], &[2][..]]);
        // the whitelist by quickbar value: 분노태세 7 → 4 · 반격태세 8 → 5 · 도발 8 / 치유오라 3 → 1 · 신의가호 7 → 5 · 철괴 8 → 6 / 민첩태세 6 → 2 · 맹독감염 8 → 4
        assert_eq!(DP::BUFF_VALUES, [[BerserkerPatch::RAGE - BerserkerPatch::STAGES + 1, BerserkerPatch::RAGE + 1 - BerserkerPatch::STAGES + 1, BerserkerPatch::TAUNT_VALUE], [1, 5, 6], [2, 4, 0]]);
        for (c, value) in [(0u8, BerserkerPatch::FURY_VALUE), (1, CombatPatch::SKY_VALUE), (2, RoguePatch::BLOOM_VALUE), (0, 1), (1, 2), (2, 1), (1, 7), (2, 7)] {
            assert!(!DP::value_allowed(c, value), "class {c} value {value} (an attack skill or awakening) is refused");
        }
        for (c, value) in [(0u8, 4u8), (0, 5), (0, 8), (1, 1), (1, 5), (1, 6), (2, 2), (2, 4), (0, 0), (1, 0), (2, 0), (3, 0)] {
            assert!(DP::value_allowed(c, value), "class {c} value {value} (a buff or the claw) goes on");
        }
        assert!(!DP::value_allowed(3, 1) && !DP::value_allowed(2, 0xff));
        // ★★ 2026-10-15 the windows = the effects' footprints + 1 (CLAW_WINDOWS): stages 0 · 1 three deep · ±2 aside, the rend four
        // deep; a facing out of 1..=4 or a stage past 2 hits nothing
        assert_eq!(DP::CLAW_WINDOWS, [(3, 2), (3, 2), (4, 2)]);
        assert!(DP::in_window(0, 1, 0, -1) && DP::in_window(0, 1, 1, -2) && DP::in_window(0, 1, -2, -3) && !DP::in_window(0, 1, 0, -4) && !DP::in_window(0, 1, 3, -1));
        assert!(!DP::in_window(0, 1, 0, 1) && !DP::in_window(0, 1, 0, 0) && DP::in_window(1, 4, 3, 2) && !DP::in_window(1, 4, 4, 0) && DP::in_window(1, 3, -1, 0));
        assert!(DP::in_window(2, 2, 0, 4) && !DP::in_window(2, 2, 0, 5) && DP::in_window(2, 1, 2, -1) && !DP::in_window(2, 1, 0, 1) && !DP::in_window(0, 0, 0, -1) && !DP::in_window(0, 5, 0, -1) && !DP::in_window(3, 1, 0, -1));
        // the COLLECT windows: hx | hy << 8 | ellipse << 16 — the seal 6 × 3 and the fire · the blast 6 × 4 (ellipses), 돌진 ±2, the judgement ±4
        assert_eq!((DP::window(6, 3, true), DP::BURST_WINDOW, DP::FIRE_WINDOW, DP::CHARGE_HALF), (0x1_0306, DP::window(6, 4, true), DP::window(6, 4, true), 2));
        assert!(DP::in_collect_window(DP::BLADE_WINDOW, 6, 0) && DP::in_collect_window(DP::BLADE_WINDOW, 0, 3) && !DP::in_collect_window(DP::BLADE_WINDOW, 3, 3) && DP::in_collect_window(DP::BLADE_WINDOW, 4, 2) && !DP::in_collect_window(DP::BLADE_WINDOW, 7, 0));
        assert!(DP::in_collect_window(DP::window(2, 0, false), 2, -2) && !DP::in_collect_window(DP::window(2, 0, false), 3, 0) && DP::in_collect_window(DP::window(4, 2, false), 4, 2) && !DP::in_collect_window(DP::window(4, 2, false), 0, 3));
        // ★★ D4 (2026-10-13): 추종 · 돌진 — the three damage functions = the gear block's (chained outermost) · a.b(I)V = the
        // berserker's · a.a(Lbd;BI)V (dump_hero2_java --json def_fo 0x8bcdc) · g.b()Z (def_fo 0x8c740, body 0x15b1c0 — the
        // update's `m.b()Z` call) · the accessory block's can_stand / relocate · ad's width
        let gp = b.gear_patch().unwrap();
        assert_eq!(p.damage, gp.damage, "bl/bx/ay.a(BILbd;)I — the gear block wraps them; the demon block goes on top");
        assert_eq!(p.hp_delta_def, bp.hp_delta_def);
        assert_eq!((p.status_def, p.step_def), (0x0018_bcdc, 0x0018_c740));
        for d in [p.status_def, p.step_def, p.hp_delta_def, p.damage[0].0, p.damage[1].0, p.damage[2].0] {
            assert_eq!((d - 0x0018_5644) % 0x1c, 0, "{d:#x}: a method definition record");
        }
        assert_eq!((p.body_can_stand, p.body_relocate, p.width_off), (acc.body_can_stand, acc.body_relocate, AccessoryPatch::WIDTH_OFF));
        assert_eq!(p.walk_request_slot, h2.walk_request_slots.0, "ag.k — the extra step waits while a stop or a turn is pending");
        // the user's numbers (HERO2_DEMON_PLAN.md §6 ⑧ · §3.4 · ★ 2026-10-16): a minute · the gauge +4 % a claw hit · +2 % a hit
        // taken · +2 % any other damage dealt · 5 % · every fifth swing · 3H · 2 s stun (25) · 30 s after · 5 s · 3×3 · once in 10 ticks
        assert_eq!((DP::FOLLOW_KIND, DP::CHARGE_KIND), (2, 12), "피의계약 · 암흑폭발 in t/bsk.tdf's order");
        assert_eq!((DP::FOLLOW_FRAMES, DP::FOLLOW_GAIN_CLAW, DP::FOLLOW_GAIN_TAKEN, DP::FOLLOW_GAIN_DEALT, DP::FOLLOW_GAUGE_FULL, DP::FOLLOW_LEECH_PCT, DP::FOLLOW_SWING_EVERY), (1200, 4, 2, 2, 100, 5, 5));
        assert_eq!((DP::FOLLOW_BLAST_MUL, DP::FOLLOW_STUN, DP::FOLLOW_STUN_FRAMES, DP::FOLLOW_COOLDOWN), (3, AccessoryPatch::STUN, 40, 600));
        assert!(DP::is_claw_kind(0) && DP::is_claw_kind(2) && !DP::is_claw_kind(3) && !DP::is_claw_kind(8), "the claw = a.p kinds 0..3 for every class");
        // the new words sit within an `ldr` of `block + D4`, before the charge table; the MOBHP piece in FXG's spare room, then the D5 words
        assert!(DP::CLAW_MARK - DP::D4 <= 124 && DP::FOLLOW_DEALT - DP::D4 <= 124 && DP::FOLLOW_DEALT + 4 <= DP::CHARGE_TAB);
        assert_eq!((DP::MOBHP_AT, DP::FTAKEN_AT), (0x2400, 0x2500));
        assert!(DP::FXG_AT < DP::MOBHP_AT && DP::FTAKEN_AT < DP::D5);
        assert_eq!(p.mob_damage, (0x0018_7e84, 0x0012_7f6c), "bd.a(IB)V — v66, the one record every monster class shares");
        assert_eq!((p.mob_damage.0 - 0x0018_5644) % 0x1c, 0);
        assert_eq!((DP::CHARGE_FRAMES, DP::CHARGE_COOLDOWN, DP::CHARGE_HALF, DP::CHARGE_REHIT, DP::CHARGE_SLOTS, DP::CHARGE_ENTRY), (300, 400, 2, 10, 8, 8), "★★ 2026-10-16 돌진 15 s · 20 s (user)");
        assert_eq!((DP::follow_gauge_add(96, 4), DP::follow_gauge_add(98, 4), DP::follow_gauge_add(0, 2), DP::follow_leech(200), DP::blast_damage(22, 104, 3)), (100, 100, 2, 10, 1150 * 3));
        assert_eq!((DP::push_delta(1), DP::push_delta(2), DP::push_delta(3), DP::push_delta(4), DP::push_delta(0)), (Some((0, -1)), Some((0, 1)), Some((-1, 0)), Some((1, 0)), None));
        assert!(DP::FOLLOW_BUFF_ID != DP::JUDGE_BUFF_ID && DP::CHARGE_BUFF_ID != DP::FOLLOW_BUFF_ID && DP::FOLLOW_BUFF_ID != GearPatch::DRAGON_BUFF_ID);
        // the D4 words sit past the HUD piece, within an `ldr rt,[rn,#imm]` of `block + D4`; the two tables, then the pieces in order
        assert_eq!(DP::D4, DP::HUD_AT + 0x300, "after the HUD piece's room");
        for off in [
            DP::FOLLOW, DP::FOLLOW_UNTIL, DP::FOLLOW_HITS, DP::FOLLOW_GAUGE, DP::FOLLOW_SLOT, DP::FOLLOW_TICK, DP::BLASTS, DP::BLAST_DMG, DP::CHARGE, DP::CHARGE_UNTIL, DP::CHARGE_HITS, DP::CHARGE_DMG,
            DP::CHARGE_SLOT, DP::LEECHED, DP::STEPS, DP::SWINGS, DP::BLAST_SEQ, DP::BLAST_N, DP::BLAST_PX, DP::BLAST_PY, DP::BLAST_AGE, DP::BLAST_LAST, DP::FOLLOWS, DP::CHARGES, DP::STEP_TICK, DP::PUSHES,
        ] {
            assert!(off >= DP::D4 && off - DP::D4 <= 124 && off % 4 == 0 && off + 4 <= DP::CHARGE_TAB, "{off:#x}");
        }
        assert_eq!(DP::CHARGE_TAB + DP::CHARGE_SLOTS * DP::CHARGE_ENTRY, DP::BLAST_XY);
        assert_eq!(DP::BLAST_XY + 8 * u32::from(DP::BLAST_MAX), DP::FOLLOW_AT);
        let d4 = [DP::FOLLOW_AT, DP::FEND_AT, DP::CHARGE_AT, DP::DTICK_AT, DP::CLAWX_AT, DP::DMG_AT[0], DP::DMG_AT[1], DP::DMG_AT[2], DP::DMGC_AT, DP::FXG_AT];
        assert!(d4.windows(2).all(|w| w[0] < w[1]) && d4[9] < DP::D5 && d4.iter().all(|a| a & 3 == 0));
        // ★★ D5 (2026-10-13): 용암불 · 멸화 — the ring spawner bn.a(BIBI)V = the accessory block's (its BEAST_RANGE wrapper under this
        // block's LAVA piece) · the direction tables co.a / co.b (dump_hero2_java: static byte arrays) · the map size = the accessory's
        assert_eq!(p.spawn, acc.ring_spawn, "bn.a(BIBI)V — the accessory block wraps it; the demon block goes on top");
        assert_eq!((p.dir_dx_slot, p.dir_dy_slot), (0x0019_439c, 0x0019_43ac), "co.a:[B · co.b:[B");
        assert_eq!((p.map_w_slot, p.map_h_slot), (acc.map_w_slot, acc.map_h_slot));
        // the user's numbers (HERO2_DEMON_PLAN.md §6 ⑩ ⑪ ⑫): 지옥불 8 · 학살화염 11 · the ring's tier 1 · 5 s · a second · ½ · 13×13 · 2H · 0.5 s
        assert_eq!((DP::LAVA_KIND, DP::FIRE_KIND, DP::LAVA_TIER), (8, 11, 1));
        assert_eq!((DP::LAVA_FRAMES, DP::LAVA_EVERY, DP::LAVA_PCT, DP::LAVA_SLOTS, DP::LAVA_ENTRY), (100, 20, 50, 32, 24));
        assert_eq!((DP::FIRE_WINDUP, DP::FIRE_WINDOW, DP::FIRE_MUL, DP::FIRE_SOUND, DP::FIRE_MAX), (10, DP::window(6, 4, true), 2, 3, 16));
        assert_eq!((DP::lava_tick(1300), DP::lava_tick(1), DP::lava_tick(0)), (650, 1, 1), "never below 1");
        assert_eq!(DP::fire_damage(22, 104, 5), DP::heavy_hit(22, 104, 5) * 2);
        assert_eq!(DP::LAVA_SLOTS * DP::LAVA_ENTRY / 4, 192, "the teardown's word count fits a movs");
        // the D5 words sit past the BLASTD piece, within an `ldr rt,[rn,#imm]` of `block + D5`; the record, the table, then the pieces
        assert_eq!(DP::D5, DP::FXG_AT + 0x5c0, "after the FXG piece's room (D4's AURA · BLASTD rooms, 2026-10-14)");
        for off in [
            DP::LAVAS, DP::LAVA_HITS, DP::LAVA_DMG, DP::LAVA_LIVE, DP::FIRE, DP::FIRE_DUE, DP::FIRE_SLOT, DP::FIRE_HITS, DP::FIRE_DMG, DP::FIRES, DP::FIRE_SEQ, DP::FIRE_N, DP::FIRE_PX, DP::FIRE_PY,
            DP::FIRE_AGE, DP::FIRE_LAST, DP::FIRE_TICK,
        ] {
            assert!(off >= DP::D5 && off - DP::D5 <= 124 && off % 4 == 0 && off + 4 <= DP::FIRE_XY, "{off:#x}");
        }
        assert_eq!(DP::FIRE_XY + 8 * u32::from(DP::FIRE_MAX), DP::LAVA_TAB);
        assert_eq!(DP::LAVA_TAB + DP::LAVA_SLOTS * DP::LAVA_ENTRY, DP::HOLD_AT, "★★ 2026-10-18 the HOLD piece in the LAVA piece's old room");
        let d5 = [DP::HOLD_AT, DP::FIRE_AT, DP::D5TICK_AT, DP::FXT_AT];
        assert!(d5.windows(2).all(|w| w[0] < w[1]) && d5[3] < DP::BLOCK_LEN && d5.iter().all(|a| a & 3 == 0));
        // ★★ 2026-10-18 야수조련 목걸이's lava: the LAVA piece in the HUD piece's spare room up to the D4 words, the accessory block's
        // word among the D5 words (an ldr of `block + D5`, between the map key and the pieces), the piece codes past the ring's
        // and the spares
        assert!(DP::HUD_AT + 0x40 == DP::LAVA_AT && DP::LAVA_AT < DP::D4 && DP::LAVA_AT & 3 == 0);
        assert!(DP::ACC_PTR == DP::LAVA_MAP + 4 && DP::ACC_PTR + 4 <= DP::LAVA_PIECE && DP::ACC_PTR - DP::D5 <= 124);
        assert_eq!((DP::LAVA_WIDE, DP::LAVA_PIECE_NONE), (DP::LAVA_PIECES, 0xff));
        assert!(u32::from(DP::LAVA_WIDE) + DP::LAVA_RING.len() as u32 <= 0xff);
        assert_eq!((DP::D6, DP::BLOCK_LEN), (0x3740, 0x4000), "★ (5): the POSE words and the DRAW piece after the D5 room (★ HD: the piece grew · ★★ 2026-10-16 the claw at the end)");
        assert!(DP::HD >= DP::D6 && DP::HD - DP::D6 <= 124 && DP::HD % 4 == 0 && DP::HD + 4 <= DP::DRAW_AT, "★ HD: a D6 word before the DRAW piece");
        assert_eq!(DP::COSTUME_HD, DP::COSTUME + 1, "★ HD: the blank body sheet right after the demon's");
        // ★★ 2026-10-14 the effect library: the canvas tag; the ids in `tools_re/demon_fx.py`'s order (each family after the
        // last, a family's members consecutive); every looping effect's length a power of two (the pieces mask the tick)
        assert_eq!(DP::FX_TAG, 0x4835_0000);
        let ids = [
            DP::FX_TRANSFORM, DP::FX_TRANSFORM_GROUND, DP::FX_CLAW, DP::FX_CHARGE_GROUND, DP::FX_CHARGE_TOP, DP::FX_FOLLOW_BUILD, DP::FX_FOLLOW_DOME, DP::FX_BURST, DP::FX_BURST_MOB,
            DP::FX_SEAL, DP::FX_BLADE_RISE, DP::FX_JUDGE_ORB, DP::FX_BOLT, DP::FX_FIRE_CORE, DP::FX_FIRE_GROUND, DP::FX_FIRE_BLAST, DP::FX_FIRE_MOB, DP::FX_FIRE_SCORCH, DP::FX_LAVA,
            DP::FX_MARKER, DP::FX_LAVA_TOP, AccessoryPatch::FX_ILSTREAK, AccessoryPatch::FX_ILFLASH, AccessoryPatch::FX_ILSLASH, AccessoryPatch::FX_ILBOLT,
            DP::FX_BODY, DP::FX_FIRE_BACK, DP::FX_CLAW_SPIN, DP::FX_CLAW_BIG, DP::FX_LAVA_WIDE, DP::FX_LAVA_WIDE_TOP, DP::FX_AURA, DP::FX_COUNT,
        ];
        // ★★ 2026-10-14 일섬 「번개의 호흡」 after the 79: 60 streaks (4 facings × 5 rows × 3 columns) · a flash · 2 slashes · 4 bolts
        // ★★ 2026-10-15 HD: the demon's body (one effect) after them · ★★ (5) 멸화's back half (one effect) after the body
        // ★★ 2026-10-16 the 악의 weapons: four spin cuts · sixteen big ones (`tools_re/demon_weapon_fx.py`)
        // ★★ 2026-10-18 야수조련 목걸이's lava: the ring's twelve at 2× · their tops (`tools_re/lava_wide_fx.py`)
        // ★★ 2026-10-22 (4) the aura under her feet while 악의 runs (one effect, the last — `tools_re/malice_wings.py`)
        let widths = [1u8, 1, 16, 4, 4, 4, 1, 1, 1, 1, 2, 1, 4, 1, 1, 1, 1, 1, 16, 1, 16, 60, 1, 2, 4, 1, 1, 4, 16, 12, 12, 1];
        for (w, (a, b)) in widths.iter().zip(ids.windows(2).map(|w| (w[0], w[1]))) {
            assert_eq!(a + w, b, "the family at {a} is {w} wide");
        }
        for m in [DP::FX_CHARGE_MASK, DP::FX_FOLLOW_MASK, DP::FX_SEAL_MASK, DP::FX_BLADE_MASK] {
            assert_eq!((m as u32 + 1) & m as u32, 0, "{m}: a mask");
        }
        assert!(DP::FX_TRANSFORM_LEN > u32::from(DP::TRANSFORM_FRAMES) + DP::T1_GRACE, "the effect outlives STATE 1 into the demon's reveal");
        assert!(DP::FX_CLAW_LEN >= 5 && DP::FX_BOLT_LEN <= DP::JUDGE_EVERY, "a cut's draws · a bolt between two strikes");
        assert!(DP::JUDGE_DMG < DP::CLAW && DP::CLAW < DP::CLAW_LAST && DP::CLAW_LAST < DP::CLAW_AGE && DP::CLAW_AGE + 4 <= DP::SEQ_TABLE && DP::CLAW_AGE <= 124, "the claw's cue words: past the D3 words, before the table, within an ldr of the block");
    }

    #[test]
    fn the_venom_patch_shares_nothing_it_should_not_and_says_the_users_numbers() {
        let b = GuestBuild::Hero2Ktf;
        let vp = b.venom_patch().unwrap();
        let cp = b.combat_patch().unwrap();
        let sp = b.skill_power_patch().unwrap();
        let ep = b.exp_rate_patch().unwrap();
        // Every WORD a host block arms (def words, pool words) belongs to one patch only.
        let mut words: Vec<u32> = vec![vp.strike.0, vp.tick_pool.0, vp.cast_pool.0, vp.effect_pool.0, ep.def];
        words.extend(sp.jobs.iter().map(|j| j.0));
        words.extend([cp.weapon_getters[0].0, cp.weapon_getters[1].0, cp.sweep.0, cp.wave_ctor.0, cp.wave_paint.0, cp.hp_delta.0, cp.seq_map.0, cp.cast.0, cp.sp_cost.0, cp.cooldown.0, cp.bludgeon_pool.0, cp.guard_pool.0, cp.iron_pool.0]);
        let n = words.len();
        words.sort_unstable();
        words.dedup();
        assert_eq!(words.len(), n, "two host blocks arm the same word");
        // ay.a(BB)Z is not the rogue's skill-damage def (that one is SkillPowerPatch's).
        assert_ne!(vp.strike.0, sp.jobs[0].0);
        assert_eq!(sp.jobs[0].2, VenomPatch::STAGES, "the rogue's 기본공격 stages, as the skill-power block counts them");
        // What it shares with the combat block is the same game.
        assert_eq!((vp.cell_attack, vp.dispatch, vp.class_table_slot, vp.skills_off), (cp.cell_attack, cp.dispatch, cp.class_table_slot, cp.skills_off));
        assert_eq!(vp.skills_off, b.hero2_java().unwrap().char_skill_levels_off, "a.p");
        assert_eq!(vp.char_slot, 0x0019_3b5c, "ag.a:La; — the CHAR anchor's static slot");
        assert_eq!(vp.class_table_slot, 0x0019_5ca4 + 0x20, "sl + 0x20");
        // Buff 33 is no skill's: 30 · 31 · 32 버서커, 34 · 35 나이트, 36 민첩태세.
        assert!(![30u8, 31, 32, 34, 35, 36].contains(&VenomPatch::BUFF_ID));
        assert!((30..=36).contains(&VenomPatch::BUFF_ID), "bm.i has entries for 30..=36 only");
        // The user's numbers (20 fps): ★ 2026-10-06 (버프 쿨 1분 · 만렙 75초 · ×1.25) the length is 25 → 75 s over
        // 맹독감염's levels and nothing from 독 연마 (it was 20 s → 29 s with 독 연마 3, 3 s a level); 300 % a tick at 5 + 3.
        assert_eq!([1, 2, 3, 4, 5].map(|v| VenomPatch::buff_frames_mastered(v) as f64 / 20.0), [25.0, 37.5, 50.0, 62.5, 75.0]);
        assert_eq!(VenomPatch::buff_frames(5, 0), VenomPatch::buff_frames(5, 3), "독 연마 adds no length");
        assert_eq!(VenomPatch::tick_percent(5, 3), 300, "★ 2026-10-14: back to 300 % (2026-10-09 (4) made it 200 %)");
        // 75 s on a 60 s cooldown (ay.a[3] = 2400 before #17's ½): a recast while it runs restarts it (BuffRefreshPatch).
        assert_eq!(vp.cooldown.last().map(|s| (s.2 >> 6) & 0x1f), Some(4), "lsls r2,r2,#4: 150 << 4 = 2400");
        assert!(VenomPatch::buff_frames_mastered(VenomPatch::VENOM_MAX) > 2400 / 2, "75 s > the 60 s cooldown");
        // The a.p indices: 맹독감염 = sk2 스킬 3, 독 연마 = sk2 스킬 7, after the rogue's 5 stages.
        assert_eq!((VenomPatch::VENOM_SKILL, VenomPatch::MASTERY_SKILL), (5 + 3, 5 + 7));
        // The tick's arm is `adds r1,r7,#0` → `bx r0` right after the `ldr r0,[pc,#…]` of its pool word.
        assert_eq!((vp.tick_arm.1, vp.tick_arm.2), (0x1c39, 0x4700));
        assert!(vp.tick_arm.0 < vp.tick_resume && vp.tick_resume - vp.tick_arm.0 < 0x100, "the bubble code is further down the same case");
        // The aura: `bm.a(II)V`'s `ldr r0,[pc,#0x54]` (0x133d3c) reads the pool word, the
        // `adds r1,r4,#0` after it becomes `bx r0`, and it goes on at the very next halfword.
        assert_eq!(((0x0013_3d3cu32 + 4) & !3) + 0x54, vp.effect_pool.0);
        assert_eq!((vp.effect_arm.0, vp.effect_arm.1, vp.effect_arm.2), (0x0013_3d3e, 0x1c21, 0x4700));
        assert_eq!(vp.effect_resume, vp.effect_arm.0 + 2);
        assert_eq!(vp.effect_life_off, 0xc, "av.a:I, bm.<init>(BI)V's lifetime");
        assert!(VenomPatch::BUFF_ID >= 30 && VenomPatch::buff_frames(1, 0) > u32::from(VenomPatch::SPARK_LEN), "the shortest buff outlives the hit spark");
    }

    /// ★ 2026-09-30 야수진화제 판매가 (user: *"판매가격은 5만원, 적은 7만원인데 이걸 팔떄는 만원,
    /// 적은 12000원으로"*): the two pool words are no other host block's, the record offsets are
    /// `ch`'s, and the prices are the user's — the shop's asking price stays the asset's.
    #[test]
    fn the_shop_sell_patch_arms_its_own_words_and_says_the_users_prices() {
        let b = GuestBuild::Hero2Ktf;
        let shop = b.shop_sell_patch().unwrap();
        let vp = b.venom_patch().unwrap();
        let cp = b.combat_patch().unwrap();
        let sp = b.skill_power_patch().unwrap();
        let ep = b.exp_rate_patch().unwrap();
        let mut words: Vec<u32> = vec![shop.sell_pool.0, shop.paint_pool.0, vp.strike.0, vp.tick_pool.0, vp.cast_pool.0, vp.effect_pool.0, ep.def];
        words.extend(sp.jobs.iter().map(|j| j.0));
        words.extend([cp.weapon_getters[0].0, cp.weapon_getters[1].0, cp.sweep.0, cp.wave_ctor.0, cp.wave_paint.0, cp.hp_delta.0, cp.seq_map.0, cp.cast.0, cp.sp_cost.0, cp.cooldown.0, cp.bludgeon_pool.0, cp.guard_pool.0, cp.iron_pool.0]);
        let n = words.len();
        words.sort_unstable();
        words.dedup();
        assert_eq!(words.len(), n, "two host blocks arm the same word");
        for w in [shop.sell_pool.0, shop.paint_pool.0] {
            assert_eq!(w & 3, 0, "{w:#x}: a pool word is word-aligned");
            assert!((0x0010_0000..0x0019_0000).contains(&w), "{w:#x}: hero2's text");
        }
        // `mov r1,sb` / `adds r1,r4,#0` → `bx r0`, each right after its site's `ldr r0,[pc,#…]`.
        assert_eq!((shop.sell_arm.1, shop.sell_arm.2), (0x4649, 0x4700));
        assert_eq!((shop.paint_arm.1, shop.paint_arm.2), (0x1c21, 0x4700));
        assert!(shop.sell_arm.0 < shop.sell_pool.0 && shop.sell_pool.0 - shop.sell_arm.0 < 0x400, "the pool is ahead of its ldr");
        assert!(shop.paint_arm.0 < shop.paint_pool.0 && shop.paint_pool.0 - shop.paint_arm.0 < 0x400);
        // The record's field block: ch.g type +0x0 · ch.h id +0x4 · ch.d price +0xc.
        assert_eq!((shop.type_off, shop.id_off, shop.price_off), (0x0, 0x4, 0xc));
        assert_eq!(shop.paint_sell_flag_sp, 0xc0, "the painter's third boolean, after ch at [sp,#0xb4]");
        // The user's numbers (2026-10-02: 10,000 / 12,000 → 2,500 / 4,000) — and the shop's
        // asking price (make_hero2_asset.py PRICE_FIXES) is 20× / 17.5× of them.
        assert_eq!(ShopSellPatch::PRICES, [(0x1400, 200), (0x1600, 2_500), (0x1601, 2_500), (0x1602, 4_000)]);
        assert_eq!(ShopSellPatch::sell_price(20, 0, 1_000), 200, "제련석: an old 1,000 record");
        assert_eq!(ShopSellPatch::sell_price(20, 0, 500), 200, "제련석: bought at 500 — not the quarter 125");
        assert_eq!(ShopSellPatch::sell_price(20, 1, 1_000), 250, "no other type-20 id");
        // 2026-10-03: 사자후's cooldown table entry (bl.a[6] = 160) × 1.5 = 240; 발도 200 → 140.
        assert_eq!(GuestBuild::Hero2Ktf.roar_cooldown_up(), &[(0x0013_3bc8, 0x22a0, 0x22f0)]);
        assert_eq!((0x22f0 & 0xff, 0x22a0 & 0xff), (240, 160));
        assert_eq!(CombatPatch::IAIDO_COOLDOWN, 140);
        assert_eq!(ShopSellPatch::sell_price(22, 0, 50_000), 2_500);
        assert_eq!(ShopSellPatch::sell_price(22, 1, 50_000), 2_500);
        assert_eq!(ShopSellPatch::sell_price(22, 2, 70_000), 4_000);
        // Everything else sells as shipped, a quarter of the price.
        assert_eq!(ShopSellPatch::sell_price(21, 3, 5_000), 1_250);
        assert_eq!(ShopSellPatch::sell_price(22, 3, 2_000), 500);
        assert_eq!(ShopSellPatch::GOLD_SLOT, 0x0019_3c0c, "ag.f:I");
    }

    /// ★★ 2026-10-02 스킬을 바꾸는 장신구 6종: every word the block arms is a method definition
    /// (word 0, the body even) no other host block arms — but the two damage definitions, which it
    /// CHAINS to (SkillPower's block, or the body); the cells, statics and offsets are the shipped
    /// routines' own; the pieces sit in order inside the block; and 야수조련's ring table doubles
    /// the radius exactly (the shipped tiles ∪ the extras = the tiles whose halved offset is a
    /// shipped one, with no tile twice).
    #[test]
    fn the_accessory_patch_arms_its_own_words_and_doubles_the_ring_exactly() {
        type P = AccessoryPatch;
        let b = GuestBuild::Hero2Ktf;
        let a = b.accessory_patch().unwrap();
        // ★ 2026-10-05 극사's target query: the site sits in the rogue strike ay.a(BBI)Z (0x11fc88..0x11fd34), the
        // cell's literal is that method's own (cell = sl + 0xfffe241c), the definition is record 927 = a.a()Lbd;,
        // and the target piece fits between the data words and the block's end.
        let (site, cell, def, body) = a.target_query;
        assert!((a.strike_rogue.1..0x0011_fd34).contains(&site) && site & 1 == 0, "{site:#x}: in ay.a(BBI)Z");
        assert_eq!(cell, 0x0019_5ca4u32.wrapping_add(0xfffe_241c), "the cell the site's literal names");
        assert_eq!(def, 0x0018_5644 + 927 * 0x1c, "a.a()Lbd;'s definition record");
        assert_eq!(body, 0x0015_1330);
        assert!(P::GK_LOCK == P::GK_WANT + 4 && P::GK_WANT >= P::BEAST_RANGE_AT + 0x200, "the beast-range piece keeps 512 B");
        assert!(P::FAKE_TARGET >= P::GK_LOCK + 4 && P::FAKE_TARGET + P::DEF_LEN <= P::TARGET_AT && P::TARGET_AT < P::BLOCK_LEN);
        let defs = [
            a.sp_cost, a.cooldown, a.damage_rogue, a.beast_damage, a.usable_rogue, a.drive_rogue, a.strike_rogue, a.arm, a.update, a.draw,
            a.ring_spawn, a.assassin, a.scatter,
        ];
        for (d, body) in defs {
            assert_eq!((d - 0x0018_5644) % 0x1c, 0, "{d:#x}: a method definition record");
            assert!((0x0018_5644..0x0019_3860).contains(&d), "{d:#x}");
            assert_eq!(body & 1, 0, "{body:#x}: stored even");
            assert!((0x0010_0000..0x0017_0000).contains(&body), "{body:#x}: in the text");
        }
        // ★★ 2026-10-14 번개의 호흡's lightning sits on the HUD painter's word — the sky and 악의 blocks (installed after) chain on it
        assert_eq!(a.hud_draw, b.sky_draw_patch().unwrap().hud_draw);
        assert_eq!(a.hud_draw, b.demon_patch().unwrap().hud_draw);
        let hd = b.hell_drop_patch().unwrap();
        let shop = b.shop_sell_patch().unwrap();
        let vp = b.venom_patch().unwrap();
        let cp = b.combat_patch().unwrap();
        let sp = b.skill_power_patch().unwrap();
        let mut others: Vec<u32> = vec![hd.death.0, shop.sell_pool.0, shop.paint_pool.0, vp.strike.0, vp.tick_pool.0, vp.cast_pool.0, vp.effect_pool.0];
        others.extend([b.exp_rate_patch().unwrap().def, b.clear_hook_patch().unwrap().def, b.equip_cursor_patch().unwrap().def]);
        others.extend([cp.weapon_getters[0].0, cp.weapon_getters[1].0, cp.sweep.0, cp.wave_ctor.0, cp.wave_paint.0, cp.hp_delta.0, cp.seq_map.0, cp.cast.0, cp.sp_cost.0, cp.cooldown.0, cp.bludgeon_pool.0, cp.guard_pool.0, cp.iron_pool.0]);
        let chained = [a.damage_rogue.0];
        for (d, _) in defs {
            assert!(!others.contains(&d), "{d:#x}: another host block arms it");
            if chained.contains(&d) {
                assert!(sp.jobs.iter().any(|j| j.0 == d), "{d:#x}: chained only where SkillPower is");
            } else {
                assert!(!sp.jobs.iter().any(|j| j.0 == d), "{d:#x}: SkillPower's word, not chained");
            }
        }
        let mut words: Vec<u32> = defs.iter().map(|d| d.0).collect();
        words.push(a.desc_exit);
        let n = words.len();
        words.sort_unstable();
        words.dedup();
        assert_eq!(words.len(), n, "a word armed twice");
        assert_eq!(a.desc_exit & 3, 0, "the exit's literal must sit word-aligned after its two halfwords");
        // the helpers and statics every other block uses
        assert_eq!((a.dispatch, a.call_static, a.direct_call, a.new_instance), (0x0016_917c, 0x0016_916e, 0x0016_9160, 0x0016_7c10));
        assert_eq!((a.char_slot, a.stage_slot, a.class_table_slot), (0x0019_3b5c, hd.stage_slot, vp.class_table_slot));
        for s in [a.char_slot, a.class_slot, a.stage_slot, a.combat_map_slot, a.map_w_slot, a.map_h_slot, a.frames_slot, a.cam_centre.0, a.cam_centre.1, a.cam_target.0, a.cam_target.1, a.cam_live.0, a.cam_live.1] {
            assert_eq!((s - 0x0019_3860) % 0x10, 0x0c, "{s:#x}: a static's value word");
        }
        // the cells sit in the literal pools of the routines whose calls the pieces repeat
        for c in [a.cell_attack, a.cell_bonus, a.cell_hit, a.cell_status, a.cell_tile_strike, a.cell_sp_cost, a.cell_sp_delta, a.cell_sweep, a.cell_draw, a.cell_ce_init, a.cell_stage_add, a.cell_assassin_tile, a.text_cell] {
            assert!((0x0017_5000..0x0018_2000).contains(&c) && c % 4 == 0, "{c:#x}: a cell");
        }
        assert_eq!((a.cell_ce_init, a.cell_stage_add, a.cell_assassin_tile), (0x0017_b070, 0x0017_b074, 0x0017_b078), "bn.a(BIBI)V's two, bn.g()V's one");
        // the pieces in order, the data words first
        // ★★ 2026-10-14 the lightning's unit took 일섬's old room (BOLT_AT); 일섬's unit sits at the end, past the lightning's words
        let at = [P::DESC_AT, P::SP_COST_AT, P::COOLDOWN_AT, P::DAMAGE_ROGUE_AT, P::BEAST_AT, P::ROGUE_AT, P::GK_AT, P::BOLT_AT, P::CHANNEL_AT, P::KNOCK_AT, P::BEAST_RANGE_AT, P::BLOCK_LEN];
        assert!(P::DETAIL_PAINT_AT < P::BOLT_SET && P::BOLT_SET + P::BOLT_DATA_LEN <= P::ILSEOM_AT && P::ILSEOM_AT < P::BLOCK_LEN && P::ILSEOM_AT % 4 == 0);
        assert!(at.windows(2).all(|w| w[0] < w[1] && w[0] % 4 == 0), "{at:?}");
        let data = [P::ACTIVE, P::CHANNEL, P::CTICK, P::PULSE, P::MFRAME, P::ILSEOM_HIT, P::GK_TARGET, P::GK_X, P::GK_Y, P::TRAIL, P::TRAIL_K, P::TRAIL_C, P::TRAIL_DIR, P::TRAIL_X, P::TRAIL_Y];
        assert!(data.windows(2).all(|w| w[0] + 4 == w[1]) && P::TRAIL_Y + 4 <= P::DESC_AT, "the data words, one after another, end before the first piece");
        // 일섬: five rows ahead (2026-10-02 night, 3 before); its afterimages: translucent (a fifth at
        // most), fainter with distance, one every TRAIL_STEP rows of the dash — two at most
        assert_eq!(P::DASH_TILES, 5);
        assert_eq!(P::TRAIL_STEP, 2);
        assert_eq!(P::TRAIL_ALPHAS.len(), (P::DASH_TILES / P::TRAIL_STEP) as usize);
        assert!(P::TRAIL_ALPHAS[0] <= 64 && P::TRAIL_ALPHAS.windows(2).all(|w| w[0] > w[1]) && P::TRAIL_ALPHAS.iter().all(|a| *a > 0), "{:?}", P::TRAIL_ALPHAS);
        assert_eq!(P::TRAIL_FRAMES, 8, "the fade divides by 8");
        assert_eq!(P::ALPHA_TAG & 0xff, 0, "the alpha rides in the low byte");
        assert!(P::ALPHA_TAG >> 8 != 0, "never a value the game itself passes (0..=255)");
        assert_eq!((a.graphics_slot - 0x0019_3860) % 0x10, 0x0c, "ax.a: a static's value word");
        assert_eq!((a.set_alpha_ref, a.set_color_ref), (0x0019_5ca4 + 0x17c, 0x0019_5ca4 + 0x88), "sl + 0x17c · sl + 0x88: the Graphics method references");
        assert_eq!(a.cell_far_strike, a.cell_tile_strike + 8, "a.e(BBI)Z's pool: a.a(BBIBB)Z · a.b(BBI)Z · a.c(BBI)Z");
        // the channels: a quarter of a cast per second; 폭풍강타 every 7 frames (0.35 s), each pulse 90/256 ≈ 7/20 of it
        assert_eq!((P::PULSE_FRAMES, P::STORM_PULSE_FRAMES, P::CHANNEL_COST_SHIFT), (20, 7, 2));
        assert_eq!((P::STORM_COST_MUL, P::STORM_COST_SHIFT), (90, 8));
        assert_eq!((u32::from(P::STORM_COST_MUL) * 20 + 128) >> 8, u32::from(P::STORM_PULSE_FRAMES), "the pulse's share of a second");
        // the storm's 5×5 is the combat sweep's: the same kind number as 발도, told apart by the class static
        let combat = GuestBuild::Hero2Ktf.combat_patch().expect("hero2");
        assert_eq!((combat.class_slot, a.class_slot), (0x0019_3a6c, 0x0019_3a6c));
        assert_eq!((CombatPatch::IAIDO, P::STORM_KIND, CombatPatch::STORM_TILES), (9, 9, 24));
        // the painter's flag sits right after its record among the stacked arguments
        assert_eq!(P::PAINT_FULL, P::PAINT_REC + 4);
        // the items: six, distinct, accessories (11 necklace · 12 ring) after the shipped 0..=16
        let mut ids: Vec<(u8, u8)> = P::ITEMS.iter().map(|i| (i.0, i.1)).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), 6);
        assert!(P::ITEMS.iter().all(|i| (i.0 == 11 || i.0 == 12) && i.1 >= 17 && i.3 <= 3));
        // the skill-name swap: two rogue skills (sk2 = 10 skills · 20 rows), their rows past the shipped 20 — and past
        // ★ 2026-10-05 일풍격's 20 · 21 (the detail window of a.p[15] reads rows 2·(15 − 5))
        let ilpung_row = 2 * u32::from(IlpungPatch::KIND - IlpungPatch::STAGES);
        assert_eq!(ilpung_row, P::SK2_SHIPPED_ROWS, "일풍격 takes the first appended pair");
        // ★ 2026-10-07 (2): and past 개화's 22 · 23 — ★★ 2026-10-09 (11) and 암살's 24 · 25 (the detail window of a.p[17]
        // reads rows 2·(17 − 5)) — the appended rows are 일풍격 · 개화 · 암살 · 일섬 · 극사 (ten)
        let amsal_row = 2 * u32::from(IlpungPatch::AMSAL_ICON);
        assert_eq!(amsal_row, ilpung_row + 4, "암살 right after 개화");
        // ★★ 2026-10-17 + 「벽력일섬」 (ILSEOM_BOLT_NAMES — 벽력일섬 worn with a 번개의 호흡) at 30 · 31: twelve appended
        for (code, skill, row) in P::SKILL_NAME_SWAPS {
            assert!(code == P::ILSEOM || code == P::GEUKSA || code == P::ILSEOM_BOLT_NAMES, "{code}");
            assert!(skill < 10 && row >= amsal_row + 2 && row % 2 == 0 && row + 2 <= P::SK2_SHIPPED_ROWS + 12, "{code}: {skill} -> {row}");
        }
        let rows: Vec<u32> = P::SKILL_NAME_SWAPS.iter().map(|s| s.2).collect();
        assert!(rows.iter().enumerate().all(|(i, r)| !rows[..i].contains(r)), "each swap its own rows");
        assert_eq!((P::name_key(P::ILSEOM, true), P::name_key(P::ILSEOM, false), P::name_key(P::GEUKSA, true)), (P::ILSEOM_BOLT_NAMES, P::ILSEOM, P::GEUKSA));
        assert_eq!((a.menu_slot, P::MENU_SKILL_TABLE_OFF, P::TEXT_TABLE_ROWS_OFF), (b.hero2_java().unwrap().menu_slot, 0x28, 0), "bw.c: bw's own field after j's 0x28 · i.a:[Object first");
        assert_eq!(a.text_table_desc, 0x0019_2fdc, "class i");
        // ★ 야수조련's ring: co.a / co.b (directions 0..=12, as `co.<clinit>` builds them)
        let co: [(i32, i32); 13] = [(0, 0), (0, -1), (0, 1), (-1, 0), (1, 0), (-1, -1), (1, -1), (-1, 1), (1, 1), (0, -2), (0, 2), (-2, 0), (2, 0)];
        let half = |v: i32| v.signum() * ((v.abs() + 1) / 2);
        for level in 1..=3 {
            let dirs: Vec<usize> = (1..=12).filter(|&d| d <= 4 * level).collect();
            let shipped: Vec<(i32, i32)> = dirs.iter().map(|&d| co[d]).collect();
            let mut all = shipped.clone();
            for &d in &dirs {
                let (skip_at_3, tiles) = P::RING_EXTRA[d - 1];
                if skip_at_3 && level >= 3 {
                    continue;
                }
                all.extend(tiles.iter().map(|&(x, y)| (i32::from(x), i32::from(y))));
            }
            let n = all.len();
            all.sort_unstable();
            all.dedup();
            assert_eq!(all.len(), n, "level {level}: a tile spawned twice");
            let mut want: Vec<(i32, i32)> = (-4..=4).flat_map(|x| (-4..=4).map(move |y| (x, y))).filter(|&(x, y)| shipped.contains(&(half(x), half(y)))).collect();
            want.sort_unstable();
            assert_eq!(all, want, "level {level}: the doubled radius");
        }
        assert_eq!((P::RING_TIER_MAX, P::RING_DIRS, P::ASSASSIN_STATUS, P::ASSASSIN_DIRS, P::ASSASSIN_FAR), (2, 12, 3, 4, 2));
        // the beast's fields: bn's own after ad (0x34)
        assert_eq!((P::BEAST_LEVELS_OFF, P::BEAST_SKILL_OFF, P::BEAST_L_OFF), (0x34 + 0xc, 0x34 + 0x34, 0x34 + 0x48));
    }

    /// ★★ 2026-10-02 HELL 보너스 드랍 (user: *"헬 장비드랍이 하드코어보다 조금 더 좋아야해"* ·
    /// *"hell모드에서 하드코어보다 장신구도 잘 나와야해"*): `bd.q()V`'s definition word is no other
    /// host block's, the drop window the shipped routine rolls stays shipped, and the cells, slots
    /// and offsets are the ones `bd.q()V`'s own table drop uses.
    #[test]
    fn the_hell_drop_patch_arms_its_own_word_and_leaves_the_shipped_window_alone() {
        let b = GuestBuild::Hero2Ktf;
        let hd = b.hell_drop_patch().unwrap();
        let shop = b.shop_sell_patch().unwrap();
        let vp = b.venom_patch().unwrap();
        let cp = b.combat_patch().unwrap();
        let sp = b.skill_power_patch().unwrap();
        let mut words: Vec<u32> = vec![hd.death.0, shop.sell_pool.0, shop.paint_pool.0, vp.strike.0, vp.tick_pool.0, vp.cast_pool.0, vp.effect_pool.0];
        words.extend([b.exp_rate_patch().unwrap().def, b.clear_hook_patch().unwrap().def, b.equip_cursor_patch().unwrap().def]);
        words.extend(sp.jobs.iter().map(|j| j.0));
        words.extend([cp.weapon_getters[0].0, cp.weapon_getters[1].0, cp.sweep.0, cp.wave_ctor.0, cp.wave_paint.0, cp.hp_delta.0, cp.seq_map.0, cp.cast.0, cp.sp_cost.0, cp.cooldown.0, cp.bludgeon_pool.0, cp.guard_pool.0, cp.iron_pool.0]);
        let n = words.len();
        words.sort_unstable();
        words.dedup();
        assert_eq!(words.len(), n, "two host blocks arm the same word");
        // bd.q()V: definition #369 of the method table, its body where the disassembler starts.
        assert_eq!(hd.death, (0x0018_7ea0, 0x0012_8080));
        assert_eq!(hd.death.0, 0x0018_5644 + 369 * 0x1c);
        assert_eq!(hd.death.1 & 1, 0, "the body is stored even; the Thumb bit is added when armed");
        // The cells sit in bd.q()V's own literal pools (0x12820c's h.a(II)I, 0x128400's
        // bf.a(BBBB)V); the helpers are the static and the virtual call the routine makes.
        assert_eq!((hd.cell_random, hd.cell_put), (0x0017_96ec, 0x0017_9734));
        assert_eq!((hd.call_static, hd.dispatch), (0x0016_916e, 0x0016_917c));
        assert_eq!(hd.dispatch, vp.dispatch, "the same virtual-call helper the venom block uses");
        // ag.i and ag.a:Lbf; — a static's value word sits +12 into its definition.
        assert_eq!((hd.mode_slot, hd.stage_slot), (0x0019_3c4c, 0x0019_3afc));
        for s in [hd.mode_slot, hd.stage_slot] {
            assert_eq!((s - 0x0019_3860) % 0x10, 0x0c, "{s:#x}: a static's value word");
        }
        assert_eq!(hd.mode_slot, b.clear_hook_patch().unwrap().mode_slot, "the mode the clear hook reads");
        // bd's own fields start after ad (0x34) + g (0x28): bd.a:Lcq; is 0x5c + 4.
        assert_eq!(hd.template_off, 0x34 + 0x28 + 0x4);
        assert_eq!((hd.tile_x_off, hd.tile_y_off, hd.gate_off, hd.slots_off), (0x10, 0x14, 0x44, 0x54));
        // Every field the block reads is one `ldr rt,[rn,#imm]` away (imm ≤ 124 past the +4).
        for off in [hd.template_off, hd.tile_x_off, hd.tile_y_off, hd.gate_off, hd.slots_off] {
            assert!(off + 4 <= 124 && off % 4 == 0, "{off:#x}");
        }
        // Slots 5 · 6 = bytes 25..35: a record with fewer bytes is a five-slot table.
        assert_eq!(HellDropPatch::FIRST_SLOT * HellDropPatch::SLOT_BYTES, 25);
        assert_eq!(HellDropPatch::MIN_SLOT_BYTES, 7 * HellDropPatch::SLOT_BYTES);
        assert_eq!(HellDropPatch::ROLL_MAX, 0x7fff, "h.a(1, 0x7fff) — the shipped roll");
        // HELL is ag.i 2, as the shipped HELL potion tests it (`cmp r0,#1 ; bgt`).
        assert_eq!(HellDropPatch::LAST_NOT_HELL, 1);
        // The pick is the shipped strict interval rule on the bonus slots' own axis.
        assert_eq!(HellDropPatch::pick(1, 2950, 3606), Some(5));
        assert_eq!(HellDropPatch::pick(2949, 2950, 3606), Some(5));
        assert_eq!(HellDropPatch::pick(2950, 2950, 3606), None);
        assert_eq!(HellDropPatch::pick(2951, 2950, 3606), Some(6));
        assert_eq!(HellDropPatch::pick(3606, 2950, 3606), None);
        // ★ 2026-10-02 저녁 (드랍 티어): the same rule over the asset's four bonus slots 5..8 — and
        //   pick_slot agrees with pick on two.
        let cums = [1312, 1411, 1739, 1773];
        for (roll, want) in [(0, None), (1, Some(5)), (1311, Some(5)), (1312, None), (1313, Some(6)), (1410, Some(6)), (1411, None), (1412, Some(7)), (1738, Some(7)), (1739, None), (1740, Some(8)), (1772, Some(8)), (1773, None), (0x7fff, None)] {
            assert_eq!(HellDropPatch::pick_slot(roll, &cums), want, "roll {roll}");
        }
        for roll in [1u32, 2949, 2950, 2951, 3605, 3606] {
            assert_eq!(HellDropPatch::pick_slot(roll, &[2950, 3606]), HellDropPatch::pick(roll, 2950, 3606), "roll {roll}");
        }
        assert_eq!(HellDropPatch::pick_slot(1, &[]), None, "a table with no bonus slot");
        // The gate bit lives above every item type (34) and below the slot type a byte can carry.
        assert_eq!(HellDropPatch::GATE, 0x80);
        assert!(HellDropPatch::TYPE_MAX < HellDropPatch::GATE && HellDropPatch::GATE | HellDropPatch::TYPE_MAX < 0xff);
        // 난이도 NORMAL (index 1 of H2_OPT_DIFFICULTY_VALUES / DIFFICULTY_PCT — hero2java.rs, which the
        // hero2java selftest pins to three) and HARD open it; EASY, the default, does not.
        // ★ 2026-10-03 (user: *"신규 장신구를 헬 노말난이도부터 나오게"*) — HARD only from 2026-10-02 저녁 until then.
        assert_eq!(HellDropPatch::GATE_DIFFICULTY, 1);
        assert!(HellDropPatch::gate_open(2) && HellDropPatch::gate_open(1) && !HellDropPatch::gate_open(0));
        assert!(HellDropPatch::gate_open(3), "an out-of-range 난이도 reads as open — the sanitiser clamps to EASY before the gate ever sees it");
    }

    /// 로그 SP −40 % (user: *"로그의 sp소모량을 전체적으로 줄이고"*): the divisors and the
    /// re-pointed branch, decoded — and the net factor per skill.
    #[test]
    fn the_rogue_skill_sp_cut_is_about_forty_percent_on_every_paid_skill() {
        let s = GuestBuild::Hero2Ktf.rogue_skill_sp_cut();
        assert_eq!(s.len(), 4);
        // ay.a(I)I @0x120af8 — the rogue's cost table.
        for (site, _, _) in s {
            assert!((0x0012_0af8..0x0012_0c10).contains(site), "{site:#x}: ay.a(I)I");
        }
        assert_eq!(t1_movs(s[0].1), Some((1, 10)), "급소가격 shipped: movs r1,#10");
        assert_eq!(t1_movs(s[0].2), Some((1, 17)), "급소가격 patched: movs r1,#17");
        // ASR Rd,Rm,#imm5 = 0x1000 | imm<<6 | Rm<<3 | Rd — the shipped `>> 3`.
        assert_eq!(s[1].1, 0x1000 | (3 << 6), "민첩태세·맹독감염 shipped: asrs r0,r0,#3");
        assert_eq!(t1_movs(s[1].2), Some((1, 13)), "… patched: movs r1,#13 for the divide");
        assert_eq!(s[2].0, s[1].0 + 2, "the case's b follows");
        assert_eq!(t1_b_target(s[2].0, s[2].1), 0x0012_0b62, "shipped: straight to the return");
        assert_eq!(t1_b_target(s[2].0, s[2].2), s[0].0 + 4, "patched: 급소가격's own divide call");
        assert_eq!(t1_movs(s[3].1), Some((1, 2)), "단검난무·번개난타 shipped: / 2");
        assert_eq!(t1_movs(s[3].2), Some((1, 4)), "… patched: / 4");
        // Net: /10 → /17, >>3 → /13, and ×3/2 → ×3/4 on a dagger sum that is +20 % (205/1024).
        let dagger = 1.0 + f64::from(GuestBuild::Hero2Ktf.combat_patch().unwrap().weapon_extra[3]) / 1024.0;
        for (name, ratio) in [("급소가격", 10.0 / 17.0), ("민첩태세·맹독감염", 8.0 / 13.0), ("단검난무·번개난타", 0.5 * dagger)] {
            assert!((0.58..0.62).contains(&ratio), "{name}: ×{ratio:.3} — about −40 %");
        }
    }

    /// ★ 2026-10-06: the beast's SP cut is RUN, not just decoded — the shipped bytes of `bn.b(B)I`'s switch
    /// (0x135598..0x1355d0) and its jump table's low halfwords (@0x170668, high halfword 0xfffc for all), the
    /// patch applied on a copy, and both interpreted for every kind and level: the buffs and heals keep their
    /// cost exactly, 암살자 · the rings · the heavy three lose about a fifth.
    #[test]
    fn the_beast_attack_skills_cost_a_fifth_less_sp_and_the_buffs_as_shipped() {
        const REGION: u32 = 0x0013_5598;
        const SHIPPED: [u16; 28] = [
            0x2150, 0x200a, 0x1c34, 0x4344, 0x1b08, 0xbc08, 0x469a, 0xbd70, 0x2028, 0xe7fa, 0x2000, 0xe7f8, 0x2046, 0xe7f6,
            0x21a0, 0x200f, 0xe7f0, 0x2107, 0x1b89, 0x240a, 0x1c08, 0x4360, 0xe7ed, 0x210e, 0xe7f8, 0x21b4, 0x2006, 0xe7e5,
        ];
        const TABLE: u32 = 0x0017_0668;
        const TABLE_LO: [u16; 13] = [0x4f30, 0x4f40, 0x4f44, 0x4f48, 0x4f4c, 0x4f5e, 0x4f52, 0x4f5e, 0x4f5e, 0x4f5e, 0x4f62, 0x4f62, 0x4f62];
        let mut code = SHIPPED;
        let mut table = TABLE_LO;
        for &(site, shipped, patched) in GuestBuild::Hero2Ktf.beast_attack_sp_cut() {
            if (REGION..REGION + 2 * SHIPPED.len() as u32).contains(&site) {
                let i = ((site - REGION) / 2) as usize;
                assert_eq!(code[i], shipped, "{site:#x}: the table's shipped halfword is the image's");
                code[i] = patched;
            } else {
                assert!((TABLE..TABLE + 4 * 13).contains(&site) && (site - TABLE) % 4 == 0, "{site:#x}: a jump-table entry's low halfword");
                let k = ((site - TABLE) / 4) as usize;
                assert_eq!(table[k], shipped, "{site:#x}: the shipped entry");
                table[k] = patched;
            }
        }
        // The game's dispatch: target = table + (0xfffc << 16 | lo) (a negative offset), then the case runs into the
        // shared epilogue at 0x1355a2 (pop {r3}) with r0 = the cost. r6 = the level.
        let run = |code: &[u16; 28], table: &[u16; 13], kind: usize, lv: i32| -> i32 {
            let mut pc = TABLE.wrapping_add(0xfffc_0000 | u32::from(table[kind]));
            let mut r = [0i32; 8];
            r[6] = lv;
            for _ in 0..40 {
                if pc == REGION + 10 {
                    return r[0]; // 0x1355a2: pop {r3} — the return
                }
                assert!((REGION..REGION + 56).contains(&pc), "kind {kind}: jumped to {pc:#x}, outside the switch");
                let h = code[((pc - REGION) / 2) as usize];
                let (rd, imm8) = ((h >> 8 & 7) as usize, i32::from(h & 0xff));
                let (d, n, m) = ((h & 7) as usize, (h >> 3 & 7) as usize, (h >> 6 & 7) as usize);
                pc += 2;
                match h & 0xf800 {
                    0x2000 => r[rd] = imm8,                    // movs rd,#imm8
                    0xe000 => pc = t1_b_target(pc - 2, h),     // b
                    _ => match h & 0xfe00 {
                        0x1c00 => r[d] = r[n] + m as i32,      // adds rd,rn,#imm3
                        0x1a00 => r[d] = r[n] - r[m],          // subs rd,rn,rm
                        _ if h & 0xffc0 == 0x4340 => r[d] *= r[n], // muls rdm,rn
                        _ => panic!("kind {kind}: {h:#06x} at {:#x} is outside the subset", pc - 2),
                    },
                }
            }
            panic!("kind {kind}: no return");
        };
        for lv in 1..=5 {
            for kind in 0..13 {
                let (was, now) = (run(&SHIPPED, &TABLE_LO, kind, lv), run(&code, &table, kind, lv));
                match kind {
                    // ★ 2026-10-12: −20 % once more (「지금 값에서 −20 %」): 56 → 45 · 112 − 8·lv → 89 − 6·lv · 144 − 5·lv → 115 − 4·lv.
                    3 => assert_eq!((was, now), (70, 45), "암살자"),
                    7..=9 => assert_eq!((was, now), ((14 - lv) * 10, 89 - 6 * lv), "kind {kind} (a ring) lv {lv}"),
                    10..=12 => assert_eq!((was, now), (180 - 6 * lv, 115 - 4 * lv), "kind {kind} (heavy) lv {lv}"),
                    _ => assert_eq!(now, was, "kind {kind} (a buff or a heal) lv {lv} keeps its cost"),
                }
                if matches!(kind, 3 | 7..=12) {
                    let r = f64::from(now) / f64::from(was);
                    assert!((0.62..0.67).contains(&r), "kind {kind} lv {lv}: ×{r:.3} — about ×0.8 × 0.8");
                }
            }
        }
        // The shipped values the table quotes, as the sanity of the interpreter itself.
        assert_eq!(run(&SHIPPED, &TABLE_LO, 0, 2), 60, "치유의물 80 − 10·lv");
        assert_eq!(run(&SHIPPED, &TABLE_LO, 4, 2), 130, "빛의회복 160 − 15·lv");
        assert_eq!(run(&SHIPPED, &TABLE_LO, 5, 2), 120, "얼음장막 (14 − lv)·10");
        assert_eq!(run(&SHIPPED, &TABLE_LO, 6, 2), 50, "정화의불 (7 − lv)·10");
    }

    /// ★ 2026-10-06: the 지력 weight is two short rewrites inside one function each — every site in the ring's
    /// `bn.d(B)V` (0x134e88..0x13504c) or the heavy case of `bn.f()V` (0x134afc..0x134bc0), and each patched halfword
    /// a plain register op (no branch, no memory access) so no control flow moves.
    #[test]
    fn the_beast_int_weight_stays_inside_the_two_formulas() {
        let s = GuestBuild::Hero2Ktf.beast_int_weight();
        assert_eq!(s.len(), 17, "★ 2026-10-12: + the two status ticks (three each)");
        for &(site, _, patched) in s {
            // ★ 2026-10-12: and the monster's status tick `bd.a(Lav;)V` (0x126620..0x1269cc)
            assert!((0x0013_4e88..0x0013_504c).contains(&site) || (0x0013_4afc..0x0013_4bc0).contains(&site) || (0x0012_6620..0x0012_69cc).contains(&site), "{site:#x}");
            // adds rd,rn,#imm3 · adds rd,rn,rm · asrs rd,rm,#imm5 · adds rdn,#imm8
            assert!(patched & 0xfe00 == 0x1c00 || patched & 0xfe00 == 0x1800 || patched & 0xf800 == 0x1000 || patched & 0xf800 == 0x3000, "{site:#x}: {patched:#06x}");
        }
    }

    /// 민첩 → 치명·회피 (user: 「중간」): the multiply-shifts are EXACT divisions over every
    /// agility a character can have, and the shipped shapes are what the table says.
    #[test]
    fn the_agility_coefficients_divide_by_five_and_three_exactly() {
        let s = GuestBuild::Hero2Ktf.agility_coefficients();
        assert_eq!(s.len(), 13);
        // Three five-halfword crit bodies of four contiguous rewrites each, then a.m()V.
        for group in [&s[0..4], &s[4..8], &s[8..12]] {
            for w in group.windows(2) {
                assert_eq!(w[1].0, w[0].0 + 2);
            }
            // lsls r0,r0,#16 — the short cast the saload helper (ldrsh) made redundant.
            assert_eq!(group[0].1, 0x0400);
            assert_eq!(group[0].2, 0x1824, "adds r4,r4,r0: base + equipment");
            assert_eq!(group[2].2, 0x4344, "muls r4,r0");
        }
        let decode = |g: &[(u32, u16, u16)]| {
            let (rd, k) = t1_movs(g[1].2).expect("movs r0,#k");
            assert_eq!(rd, 0);
            let asr = g[3].2;
            assert_eq!(asr & 0xf83f, 0x1000 | (4 << 3) | 4, "asrs r4,r4,#n");
            (u32::from(k), u32::from((asr >> 6) & 0x1f))
        };
        for (g, shipped_shift, div) in [(&s[0..4], 3u16, 5u32), (&s[4..8], 3, 5), (&s[8..12], 2, 3)] {
            assert_eq!(g[3].1, 0x1000 | (shipped_shift << 6) | (4 << 3) | 4, "shipped / {}", 1 << shipped_shift);
            let (k, n) = decode(g);
            for agi in 0..500u32 {
                assert_eq!(agi * k >> n, agi / div, "민첩 {agi}: ·{k} >> {n} must be / {div}");
            }
        }
        // bl.c()V (버서커) · bx.c()V (나이트) · ay.c()V (로그), in that order.
        assert!((0x0013_358c..0x0013_3794).contains(&s[0].0));
        assert!((0x0013_ba8c..0x0013_bd58).contains(&s[4].0));
        assert!((0x0012_0480..0x0012_0af8).contains(&s[8].0));
        // 회피: a.m()V's divisor, movs r1,#5 → #3.
        assert_eq!((t1_movs(s[12].1), t1_movs(s[12].2)), (Some((1, 5)), Some((1, 3))));
    }

    /// 검기: already a piercing line — only its life (tiles) goes 3 → 5.
    #[test]
    fn the_sword_wave_flies_five_tiles() {
        let s = GuestBuild::Hero2Ktf.sword_wave_range();
        assert_eq!(s.len(), 1);
        let (site, orig, patched) = s[0];
        let cp = GuestBuild::Hero2Ktf.combat_patch().unwrap();
        // Inside ak.<init>(La;I)V — the very body the combat block's constructor wrapper calls.
        assert!((cp.wave_ctor.1..cp.wave_ctor.1 + 0x100).contains(&site), "{site:#x}: ak.<init>");
        assert_eq!((t1_movs(orig), t1_movs(patched)), (Some((2, 3)), Some((2, 5))));
    }

    /// 신의가호: 10 … 20 s (Lv5 = the top, `bx.a(I)B`) + 2 s per 오라수련 level, the old HP/SP
    /// boost gone, cooldown 1200 (= 30 s after #17's ½) — user, 2026-09-27: *"만렙시 지속시간
    /// 20초에 쿨 30초로"* · *"오라수련 레벨당 신의가호 지속시간 2초 증가 … 오라수련 다올리면 신의가호는
    /// 쿨타임과 지속시간이 같아지는거야"*. The cast's eight rewritten halfwords decoded the way
    /// the CPU runs them, so the length and the cooldown are checked against each other.
    /// A Thumb `bl` pair's target.
    fn t1_bl_target(at: u32, hi: u16, lo: u16) -> u32 {
        assert_eq!((hi & 0xf800, lo & 0xf800), (0xf000, 0xf800), "{at:#x}: one bl");
        let mut off = (u32::from(hi & 0x7ff) << 12) | (u32::from(lo & 0x7ff) << 1);
        if off & 0x40_0000 != 0 {
            off |= 0xff80_0000;
        }
        at.wrapping_add(4).wrapping_add(off)
    }
    /// A `ldr rd,[pc,#imm]`'s pool word.
    fn t1_pool(at: u32, h: u16) -> (u16, u32) {
        assert_eq!(h & 0xf800, 0x4800, "{at:#x}: {h:#06x} is not a pc-relative ldr");
        ((h >> 8) & 7, ((at + 4) & !3) + 4 * u32::from(h & 0xff))
    }
    /// `ldrb rt,[r0,#8 + i]` → `i` (the element block's byte `i`).
    fn t1_elem(h: u16, rt: u16) -> u32 {
        assert_eq!(h & 0xf83f, 0x7800 | rt, "ldrb r{rt},[r0,#…]");
        u32::from((h >> 6) & 0x1f) - 8
    }

    /// ★ 2026-10-06 (버프 쿨 1분 · 만렙 75초 · 신의가호도 「전부 ×1.25」): 18.75 → 37.5 s over the levels + 7.5 s an 오라수련
    /// level → 75 s with both at 5, cooldown 60 s.
    #[test]
    fn the_blessing_lasts_a_quarter_longer_than_its_level_and_aura_rule_and_cools_down_a_minute() {
        let s = GuestBuild::Hero2Ktf.blessing_aura_rework();
        assert_eq!(s.len(), 21);
        // ★ 2026-10-05: the rewrite starts at the cast's a.m()V call (0x13b736) — its three halfwords,
        // then 0x13b73c..=0x13b758 contiguous (0x13b738 adds r1,r6 · 0x13b73a add r0,sl are kept).
        assert_eq!(s[0].0, 0x0013_b736, "right after bx.a()Z case 7's `a.q ← 1`");
        for w in s[1..16].windows(2) {
            assert_eq!(w[1].0, w[0].0 + 2, "one contiguous rewrite 0x13b73c..=0x13b758");
        }
        assert_eq!(s[1].0, 0x0013_b73c);
        // Shipped: ldr r0,=a.m()V's cell · bl 0x16917c (the virtual call) — the call that goes.
        assert_eq!(t1_bl_target(s[1].0, s[1].1, s[2].1), 0x0016_917c, "shipped: a.m() through the virtual helper");
        // Patched: the a.p cell's pool (the one the shipped getfield read) and the getfield helper.
        let (rd, pool) = t1_pool(s[0].0, s[0].2);
        assert_eq!((rd, pool), (0, t1_pool(0x0013_b740, s[3].1).1), "the a.p cell's pool word, as the shipped 0x13b740 read it");
        assert_eq!(t1_bl_target(s[1].0, s[1].2, s[2].2), 0x0016_80a4, "bl getfield");
        assert_eq!(t1_bl_target(0x0013_b746, s[6].1, s[7].1), 0x0016_80a4, "shipped: the getfield it replaces");
        let p: Vec<u16> = s[3..16].iter().map(|x| x.2).collect();
        assert_eq!(p[0], 0x6800, "ldr r0,[r0]: the array's element block");
        let knight_stages = 3;
        assert_eq!(t1_elem(p[1], 1), knight_stages + 4, "a.p[7] = 신의가호 (sk1 스킬 4)");
        assert_eq!(t1_elem(p[2], 2), knight_stages + 11, "a.p[14] = 오라수련 (sk1 스킬 11)");
        assert!(knight_stages + 11 < Hero2Java::CHAR_SKILL_SLOTS, "inside a.p");
        // ★ 2026-10-05: 오라수련 outside 0..=5 (−1 until the quest 「무사의 장갑」 — it read as 255, 26 minutes).
        assert_eq!(p[3], 0x2800 | (2 << 8) | 5, "cmp r2,#5");
        assert_eq!(p[4], 0xd900, "bls over the next");
        assert_eq!(t1_movs(p[5]), Some((2, 0)));
        // ★ 2026-10-03 (user: 쿨타임 1분 · 지속 30초 · 오라수련 5 면 지속 1분): 75 · 120 · 225 (were 50 · 40 · 150).
        // ★ 2026-10-06 (×1.25): the level part `(75·lv + 225)` plus its quarter, then 150 an 오라수련 level — the two
        //   `nop`s of the 10-03 version hold the quarter, and the shipped `adds r7,#70` becomes the 오라수련 add.
        assert_eq!(t1_movs(p[6]), Some((3, 75)));
        assert_eq!(p[7], 0x4340 | (3 << 3) | 1, "muls r1,r3");
        assert_eq!(p[8], 0x3000 | (1 << 8) | 225, "adds r1,#225");
        assert_eq!(p[9], 0x1000 | (2 << 6) | (1 << 3) | 3, "asrs r3,r1,#2");
        assert_eq!(p[10], 0x1800 | (3 << 6) | (1 << 3) | 7, "adds r7,r1,r3");
        assert_eq!(t1_movs(p[11]), Some((3, 150)), "7.5 s an 오라수련 level at 20 frames a second");
        assert_eq!(p[12], 0x4340 | (3 << 3) | 2, "muls r2,r3");
        // ADD Rd,Rn,Rm = 0x1800 | Rm<<6 | Rn<<3 | Rd — the shipped `adds r7,#70` becomes `adds r7,r7,r2`.
        assert_eq!(s[16], (0x0013_b75c, 0x3700 | 70, 0x1800 | (2 << 6) | (7 << 3) | 7));
        // The straight line as the CPU runs it (the ldr at 0x13b75a between touches neither r2 nor r7).
        let frames = |lv: u32, aura: u32| {
            let r1 = 75 * lv + 225;
            r1 + (r1 >> 2) + 150 * aura
        };
        let secs = |lv: u32, aura: u32| f64::from(frames(lv, aura)) / 20.0;
        assert_eq!([1, 2, 3, 4, 5].map(|lv| frames(lv, 0)), [375, 468, 562, 656, 750], "18.75 … 37.5 s");
        assert_eq!([0, 1, 2, 3, 4, 5].map(|a| secs(5, a)), [37.5, 45.0, 52.5, 60.0, 67.5, 75.0]);
        assert_eq!(t1_b_target(s[17].0, s[17].1), 0x0015_051c, "shipped: into the HP/SP boost");
        assert_eq!(s[17].2, T1_NOP);
        // bx.<clinit>: movs r1,#4 (index) · movs r2,#150 · lsls r2,r2,#4 = 2400, and the
        // shipped adds r0,r4,#0 it replaces is redundant (the store helper keeps r0).
        assert_eq!(s[18].1, 0x1c20);
        assert_eq!(t1_movs(s[18].2), Some((1, 4)), "bx.a[4] = 신의가호 (sk1 스킬 4)");
        assert_eq!(t1_movs(s[19].2), Some((2, 150)));
        assert_eq!(s[20].2, (4 << 6) | (2 << 3) | 2, "lsls r2,r2,#4");
        let cooldown = (150 << 4) / 2; // #17 halves every class cooldown
        assert_eq!(f64::from(cooldown) / 20.0, 60.0, "a minute");
        assert!(cooldown <= i32::from(i16::MAX), "the table is a short[]");
        assert_eq!(secs(5, 5), f64::from(cooldown) / 20.0 * 1.25, "both maxed: the cooldown × 1.25 (75 s on 60)");
        // The shipped fifth store was 180 into index 4.
        assert_eq!((t1_movs(s[19].1), t1_movs(s[20].1)), (Some((1, 4)), Some((2, 180))));
    }

    /// ★★ 2026-10-05 방어의지 → 철괴 (user: Lv1 20 s … Lv5 40 s, +4 s an 오라수련 level, cooldown 60 s, no break on an
    /// attack): the cast's rewritten halfwords decoded the way the CPU runs them, checked against
    /// [`CombatPatch::iron_frames`] — and the cooldown, the attack-break and the walk.
    /// ★ 2026-10-06 (버프 쿨 1분 · 만렙 75초 · ×1.25): Lv1 25 s … Lv5 50 s, +5 s an 오라수련 level → 75 s.
    #[test]
    fn iron_lasts_twenty_five_to_fifty_seconds_plus_the_aura_training_and_cools_down_a_minute() {
        let s = GuestBuild::Hero2Ktf.iron_wall_rework();
        assert_eq!(s.len(), 22);
        assert_eq!(s[0].0, 0x0013_b856, "right after bx.a()Z case 8's `a.r ← 1`");
        for w in s[1..18].windows(2) {
            assert_eq!(w[1].0, w[0].0 + 2, "one contiguous rewrite 0x13b860..=0x13b880");
        }
        assert_eq!(s[1].0, 0x0013_b860, "0x13b858..=0x13b85e (adds r1,r6 · add r0,sl · bl getfield) kept");
        // The kept bl at 0x13b85c is the getfield helper — the shipped re-read of a.r used it too.
        let (rd, pool) = t1_pool(s[0].0, s[0].2);
        assert_eq!((rd, pool), (0, t1_pool(0x0013_b868, s[5].1).1), "a.p's pool word, as the shipped 0x13b868 read it");
        assert_eq!(t1_bl_target(0x0013_b86e, s[8].1, s[9].1), 0x0016_80a4, "shipped: the getfield of a.p");
        assert_eq!(t1_bl_target(0x0013_b874, s[11].1, s[12].1), 0x0016_89e4, "shipped: the byte-array read of a.p[8]");
        let p: Vec<u16> = s[1..18].iter().map(|x| x.2).collect();
        assert_eq!(p[0], 0x6800, "ldr r0,[r0]");
        assert_eq!(t1_elem(p[1], 1), u32::from(CombatPatch::IRON), "a.p[8] = 철괴");
        assert_eq!(p[2], 0x2800 | (1 << 8) | u16::from(CombatPatch::IRON_MAX_LEVEL), "cmp r1,#5");
        assert_eq!(p[3], 0xd900, "bls over the next");
        assert_eq!(t1_movs(p[4]), Some((1, 0)));
        assert_eq!(t1_elem(p[5], 2), u32::from(CombatPatch::AURA_TRAINING), "a.p[14] = 오라수련");
        assert_eq!(p[6], 0x2800 | (2 << 8) | u16::from(CombatPatch::IRON_MAX_LEVEL), "cmp r2,#5");
        assert_eq!(p[7], 0xd900);
        assert_eq!(t1_movs(p[8]), Some((2, 0)));
        assert_eq!(t1_movs(p[9]), Some((3, u16::from(CombatPatch::IRON_FRAMES_PER_LEVEL))));
        assert_eq!(p[10], 0x4340 | (3 << 3) | 1, "muls r1,r3");
        assert_eq!(t1_movs(p[11]), Some((3, u16::from(CombatPatch::IRON_FRAMES_PER_AURA))));
        assert_eq!(p[12], 0x4340 | (3 << 3) | 2, "muls r2,r3");
        assert_eq!(p[13], 0x1800 | (2 << 6) | (1 << 3) | 7, "adds r7,r1,r2");
        // ★ 2026-10-06: + 375 in two adds (187 + 188 — one ADDS #imm8 tops out at 255).
        assert_eq!((p[14] & 0xff00, p[15] & 0xff00), (0x3700, 0x3700), "adds r7,#imm ×2");
        assert_eq!(u16::from(p[14] as u8) + u16::from(p[15] as u8), CombatPatch::IRON_FRAMES_BASE);
        assert_eq!(375, CombatPatch::IRON_FRAMES_BASE);
        // The buff call's cell: the shipped 0x13b87e load's pool word, now loaded from 0x13b880.
        assert_eq!(t1_pool(0x0013_b880, p[16]), (0, t1_pool(0x0013_b87e, s[16].1).1), "the buff call's cell");
        // Simulate the straight-line code for every pair of bytes the game can hold.
        for lv in -1i8..=6 {
            for aura in -1i8..=6 {
                let ok = |v: i8| if (v as u8) <= 5 { u32::from(v as u8) } else { 0 };
                let frames = 125 * ok(lv) + 100 * ok(aura) + u32::from(p[14] as u8) + u32::from(p[15] as u8);
                assert_eq!(frames, CombatPatch::iron_frames(lv, aura), "lv {lv} 오라 {aura}");
            }
        }
        assert_eq!([1, 2, 3, 4, 5].map(|lv| CombatPatch::iron_frames(lv, 0)), [500, 625, 750, 875, 1000], "25 … 50 s");
        assert_eq!([0, 1, 5].map(|a| CombatPatch::iron_frames(5, a) / 20), [50, 55, 75]);
        // bx.<clinit>: bx.a[5] = 150 << 4 = 2400 → 1200 after ½ = 60 s; the index store stays `movs r1,#5`.
        assert_eq!((s[18].0, t1_movs(s[18].1), t1_movs(s[18].2)), (0x0013_bff2, Some((2, 180)), Some((2, 150))));
        assert_eq!((s[19].0, s[19].1, s[19].2), (0x0013_bff4, 0x1c20, (4 << 6) | (2 << 3) | 2));
        assert_eq!((150 << 4) / 2 / 20, 60, "a minute");
        assert_eq!(CombatPatch::iron_frames(5, 5), (150 << 4) / 2 * 5 / 4, "both maxed: the cooldown × 1.25 (75 s on 60)");
        // ag.c(B)V: the branch into `a.c(B)Z(35)` (drop the buff on any attack) goes.
        assert_eq!((s[20].0, s[20].1 & 0xff00, s[20].2), (0x0011_0a9e, 0xd100, T1_NOP));
        // bx.e(II)V: `beq` (a.r clear → the plain walk) → `b` to the same place.
        assert_eq!((s[21].0, s[21].1 & 0xff00, s[21].2 & 0xf800), (0x0013_ba3e, 0xd000, 0xe000));
        assert_eq!(s[21].1 & 0xff, s[21].2 & 0x7ff, "same target");
    }

    /// ★ 2026-10-05 빛의회복 = a buff: the beast drive's type-4 branch (0x1349d6) becomes 얼음장막's
    /// `bn.c(type)` call — the literal it now reads is the one the type-5 code at 0x1349f0 reads
    /// (`bn.c(B)V`'s cell), `r1 = sb` (the beast), `r2 = 4`, and the branch lands on that code's
    /// `bl 0x169160` @0x1349f8, which is followed by 얼음장막's sound and the shared exit.
    #[test]
    fn the_beast_recovery_goes_through_the_ice_curtains_buff_call() {
        let s = GuestBuild::Hero2Ktf.beast_recovery_buff();
        assert_eq!(s.iter().map(|x| x.0).collect::<Vec<_>>(), [0x0013_49d6, 0x0013_49d8, 0x0013_49dc, 0x0013_49de]);
        assert_eq!(t1_pool(s[0].0, s[0].1), (0, 0x0013_4ad8), "shipped: br.a(B)V's cell (the sound)");
        assert_eq!(t1_pool(s[0].0, s[0].2), (0, 0x0013_4ae0), "patched: the pool word type 5 reads …");
        assert_eq!(t1_pool(0x0013_49f0, 0x483b), (0, 0x0013_4ae0), "… at 0x1349f0 for bn.c(B)V");
        assert_eq!(s[1].2, 0x4600 | (9 << 3) | 1, "mov r1, sb — this beast, as type 5 passes it");
        assert_eq!(t1_movs(s[2].2), Some((2, 4)), "r2 = 4 (빛의회복)");
        assert_eq!(t1_bl_target(s[2].0, s[2].1, s[3].1), 0x0016_916e, "shipped: the sound's static call");
        assert_eq!(t1_b_target(s[3].0, s[3].2), 0x0013_49f8, "type 5's bl 0x169160 (bn.c), then its sound and exit");
    }

    /// ★★ 2026-10-05 일풍격: the sixteenth slot (`a.b()V`'s `byte[15]` and its fill), the definition words (each
    /// `0x185644 + idx × 0x1c`, distinct, none claimed by another block), the three cells (word-aligned, in the cell
    /// area) and the painter's two calls (shipped = `bl 0x16917c`, the virtual helper).
    #[test]
    fn ilpung_sits_on_the_rogues_own_words_and_grows_every_array_by_one() {
        let p = GuestBuild::Hero2Ktf.ilpung_patch().unwrap();
        type P = IlpungPatch;
        // ★ 2026-10-07 (2) 각성기: seventeen slots — ★★ 2026-10-09 (11) 암살: eighteen — movs r1,#15 → #18 · cmp r4,#14 → #17
        assert_eq!(p.slots, [(0x0014_d71c, 0x210f, 0x2112), (0x0014_d732, 0x2c0e, 0x2c11)], "movs r1,#15 → #18 · cmp r4,#14 → #17");
        assert_eq!(t1_movs(p.slots[0].2), Some((1, P::SLOTS as u16)));
        assert_eq!(p.slots[1].2 & 0xff, P::SLOTS as u16 - 1, "the -1 fill reaches the new last index");
        // ★★ (11) the PASSIVE tab: the rogue's count movs r1,#9 → #10; the right column's two calls (shipped = the virtual
        // helper, 0x1c apart) and three cells (each its own, in the cell area); the hit and the damage definitions
        assert_eq!(t1_movs(p.passive_cells.1), Some((1, 9)));
        assert_eq!(t1_movs(p.passive_cells.2), Some((1, u16::from(P::PASSIVE_CELLS))));
        for (site, shipped) in [(p.passive_level.0, p.passive_level.1), (p.passive_max.0, p.passive_max.1)] {
            assert_eq!(t1_bl_target(site, shipped[0], shipped[1]), p.dispatch, "{site:#x}: the virtual helper as shipped");
        }
        assert_eq!(p.passive_max.0 - p.passive_level.0, 0x1c, "the PASSIVE right column's two calls");
        let passive_cells = [p.passive_level.2, p.passive_max.2, p.passive_cell];
        for cell in passive_cells {
            assert_eq!(cell & 3, 0);
            assert!((0x0017_0000..0x0018_3000).contains(&cell), "{cell:#x}: the late-binding cell area");
            assert!(![p.paint_level.2, p.paint_max.2, p.paint_cell.0, p.paint_level_left.2, p.paint_max_left.2, p.paint_cell_left].contains(&cell), "{cell:#x}: not an ACTIVE column's");
        }
        for d in [p.hit.0, p.damage.0] {
            assert_eq!((d - 0x0018_5644) % 0x1c, 0, "{d:#x}: a method definition record");
        }
        assert_eq!(p.damage, GuestBuild::Hero2Ktf.accessory_patch().unwrap().damage_rogue, "the rogue's damage definition — the accessory block's too (this block chains it)");
        assert_eq!(p.element_table_slot, 0x0019_440c, "co.a:[[B");
        let defs = [p.queue.0, p.sp_cost.0, p.cooldown.0, p.max_level.0, p.cursor.0, p.usable.0, p.drive.0, p.load.0, p.cd_start.1, p.paint_level.3, p.paint_max.3, p.paint_cell.1, p.register.1];
        for d in defs {
            assert_eq!((d - 0x0018_5644) % 0x1c, 0, "{d:#x}: a method definition record");
        }
        let mut sorted = defs.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), defs.len());
        let acc = GuestBuild::Hero2Ktf.accessory_patch().unwrap();
        assert_eq!((p.usable, p.drive), (acc.usable_rogue, acc.drive_rogue), "the same two the accessory block wraps (and chains)");
        let sp = GuestBuild::Hero2Ktf.skill_power_patch().unwrap();
        for d in [p.queue.0, p.sp_cost.0, p.cooldown.0, p.max_level.0, p.cursor.0] {
            assert!(![acc.sp_cost.0, acc.cooldown.0, acc.damage_rogue.0, acc.strike_rogue.0, acc.usable_rogue.0, acc.drive_rogue.0].contains(&d), "{d:#x}: the accessory block's");
            assert!(!sp.jobs.iter().any(|j| j.0 == d), "{d:#x}: SkillPower's");
        }
        for cell in [p.cd_start.0, p.paint_level.2, p.paint_max.2, p.paint_cell.0, p.paint_level_left.2, p.paint_max_left.2, p.paint_cell_left, p.register.0] {
            assert_eq!(cell & 3, 0);
            assert!((0x0017_0000..0x0018_3000).contains(&cell), "{cell:#x}: the late-binding cell area");
        }
        for (site, shipped) in [(p.paint_level.0, p.paint_level.1), (p.paint_max.0, p.paint_max.1), (p.paint_level_left.0, p.paint_level_left.1), (p.paint_max_left.0, p.paint_max_left.1)] {
            assert_eq!(t1_bl_target(site, shipped[0], shipped[1]), p.dispatch, "{site:#x}: the virtual helper as shipped");
        }
        assert_eq!(p.paint_max.0 - p.paint_level.0, 0x1c, "the right column's two calls");
        // ★ 2026-10-07 (2): the LEFT column's two calls (the same pattern, 0x1c apart, before the right column's) and the
        // register popup's cell (the `ag.a(IIZ)V` call in `s.a(BB)V`: def 0x1860c4, body 0x111f44)
        assert_eq!(p.paint_max_left.0 - p.paint_level_left.0, 0x1c, "the left column's two calls");
        assert!(p.paint_level_left.0 < p.paint_level.0, "the left column is drawn first");
        assert_eq!(p.register, (0x0017_d6d8, 0x0018_60c4, 0x0011_1f44, 0x0014_82ac), "cell · def · body · ★ 2026-10-08 the call site (its bl → the direct helper)");
        assert_eq!((p.register.1 - 0x0018_5644) % 0x1c, 0);
        assert_eq!((P::ROWS[0].len(), P::CELLS), (8, 8));
        for (class, row) in P::ROWS.iter().enumerate() {
            let stages = P::CLASS_STAGES[class];
            assert!(row.iter().all(|&k| k >= stages && u32::from(k) < P::SLOTS), "class {class}: every cell names a skill slot");
            let mut sorted = row.to_vec();
            sorted.sort_unstable();
            sorted.dedup();
            assert_eq!(sorted.len(), 8, "class {class}: no cell twice");
            let (awaken, value, cell) = P::AWAKEN[class];
            assert_eq!(row[usize::from(cell)], awaken, "class {class}: the awakening is its cell's");
            assert_eq!(P::value_of_kind(class as u8, awaken), value);
            assert_eq!(P::kind_of_value(class as u8, value), awaken);
        }
        assert!(!P::ROWS[0].contains(&P::BODYSLAM), "몸통치기 is in no row");
        assert_eq!(P::ROWS[0], [4, 6, 7, 15, 8, 9, 10, 16]);
        assert_eq!(P::ROWS[1], [3, 4, 5, 6, 7, 8, 9, 15]);
        assert_eq!(P::ROWS[2], [5, 6, 7, 8, 9, 10, 15, 16]);
        // the numbers: value 7 ↔ kind 15, the window's cell 6, 발도's cooldown / SP, the reach
        assert_eq!(P::KIND, P::STAGES + 10, "the sixteenth slot");
        assert_eq!((P::QUEUE_VALUE, P::SHIPPED_KIND), (P::KIND - P::STAGES + 1, P::VALUE + P::STAGES - 1));
        assert_eq!((P::WINDOW_CELL, P::COOLDOWN_INDEX), (P::VALUE - 1, P::VALUE - 1));
        assert_eq!(u16::from(P::COOLDOWN), CombatPatch::IAIDO_COOLDOWN, "발도's");
        assert_eq!(P::THROWS as usize, P::DIR8.len());
        for (i, (dx, dy)) in P::DIR8.iter().enumerate() {
            let (nx, ny) = P::DIR8[(i + 1) % 8];
            assert!(dx * ny - dy * nx > 0, "clockwise on screen (y down): {i}");
        }
        assert_eq!(P::DIR8[usize::from(P::START8[1])], (0, -1), "facing up starts north");
        assert_eq!(P::DIR8[usize::from(P::START8[2])], (0, 1), "down → south");
        assert_eq!(P::DIR8[usize::from(P::START8[3])], (-1, 0), "left → west");
        assert_eq!(P::DIR8[usize::from(P::START8[4])], (1, 0), "right → east");
        // Lv5 = attack × (10 · 8 · 115 / 768) × 1.5 ≈ 신의가호 발도's 17.97
        let lv5 = 10.0 * 8.0 * f64::from(P::POWER_MUL) / f64::from(P::POWER_DIV) * 1.5;
        assert!((lv5 - 17.97).abs() < 0.03, "{lv5}");
    }

    /// 민첩태세 = 무적: the eleven cast halfwords are one straight-line program that sets
    /// `a.s:Z` and computes `10·lv + 30` from `a.p[6]` — decoded against the combat block's
    /// own offsets so the two halves cannot drift apart.
    #[test]
    fn the_agility_stance_is_invulnerability_for_two_to_four_seconds() {
        let s = GuestBuild::Hero2Ktf.agility_stance_invuln();
        assert_eq!(s.len(), 16);
        for w in s[..11].windows(2) {
            assert_eq!(w[1].0, w[0].0 + 2, "one contiguous rewrite 0x11fffa..=0x12000e");
        }
        let cp = GuestBuild::Hero2Ktf.combat_patch().unwrap();
        let p: Vec<u16> = s[..11].iter().map(|x| x.2).collect();
        assert_eq!(p[0], 0x6830, "ldr r0,[r6]: this's field block");
        assert_eq!((p[1], p[2]), (0x3080, 0x3080), "adds r0,#0x80 ×2");
        assert_eq!(t1_movs(p[3]), Some((1, 1)));
        // STR Rd,[Rn,#imm5·4] = 0x6000 | imm5<<6 | Rn<<3 | Rd — the block is fields + 4.
        let str_off = 0x100 + 4 * u32::from((p[4] >> 6) & 0x1f);
        assert_eq!(p[4] & 0xf83f, 0x6000 | 1, "str r1,[r0,#…]");
        assert_eq!(str_off, cp.invuln_off + 4, "a.s ← 1");
        let ldr_off = 0x100 + 4 * u32::from((p[5] >> 6) & 0x1f);
        assert_eq!(p[5] & 0xf83f, 0x6800, "ldr r0,[r0,#…]");
        assert_eq!(ldr_off, cp.skills_off + 4, "a.p");
        assert_eq!(p[6], 0x6800, "ldr r0,[r0]: the element block");
        // LDRB Rd,[Rn,#imm5] = 0x7800 | imm5<<6 | Rn<<3 | Rd; elements start at +8.
        assert_eq!(p[7] & 0xf83f, 0x7800 | 3);
        let elem = u32::from((p[7] >> 6) & 0x1f) - 8;
        assert_eq!(elem, 5 + 1, "a.p[L + 1] — 민첩태세 = sk2 스킬 1 after the rogue's 5 stages");
        assert_eq!(t1_movs(p[8]), Some((0, 10)));
        assert_eq!(p[9], 0x4343, "muls r3,r0");
        assert_eq!(p[10], 0x3300 | 30, "adds r3,#30");
        let secs = |lv: u32| f64::from(10 * lv + 30) / 20.0;
        assert_eq!([1, 2, 3, 4, 5].map(secs), [2.0, 2.5, 3.0, 3.5, 4.0]);
        // The buff call's own movs r3,#110 goes, so it takes the r3 above.
        assert_eq!((t1_movs(s[11].1), s[11].2), (Some((3, 110)), T1_NOP));
        assert_eq!(s[11].0, 0x0012_0018);
        // ay.c()V's branch into the old 민첩 bonus.
        assert_eq!(s[12].2, T1_NOP);
        assert_eq!(s[12].1 & 0xf800, 0xe000);
        // ay.<clinit>: [1] = 150 << 3 = 1200 → 600 after ½ → 30 s.
        assert_eq!(t1_movs(s[13].2), Some((1, 1)));
        assert_eq!(t1_movs(s[14].2), Some((2, 150)));
        assert_eq!(s[15].2, (3 << 6) | (2 << 3) | 2);
        assert_eq!((150 << 3) / 2 / 20, 30);
    }

    /// The combat block's layout and tables.
    #[test]
    fn the_combat_block_pieces_fit_and_the_front_tiles_are_the_row_ahead() {
        let cp = GuestBuild::Hero2Ktf.combat_patch().unwrap();
        // Pieces in order, inside the block.
        // ★★ 2026-10-17 the sequence map moved to the end (0xc00) with 발도's piece, the update and the reach helper after it
        let at = [CombatPatch::WEAPON_AT[0], CombatPatch::WEAPON_AT[1], CombatPatch::HP_GUARD_AT, CombatPatch::HIT_GUARD_AT, CombatPatch::WAVE_PAINT_AT, CombatPatch::WAVE_CTOR_AT, CombatPatch::CAST_AT, CombatPatch::SP_COST_AT, CombatPatch::COOLDOWN_AT, CombatPatch::SWEEP_AT, CombatPatch::IRON_AT, CombatPatch::GEAR_RANGE_PTR, CombatPatch::SEQ_MAP_AT, CombatPatch::IAIDO_AT, CombatPatch::UPDATE_AT, CombatPatch::REACH_AT, CombatPatch::IAIDO_TILES, CombatPatch::IAIDO_N, CombatPatch::TWIN_LEFT, CombatPatch::TWIN_SLOT, CombatPatch::TWIN_SAVED, CombatPatch::TWIN_RECAST];
        for w in at.windows(2) {
            assert!(w[0] < w[1], "{:#x} before {:#x}", w[0], w[1]);
        }
        assert!(CombatPatch::TWIN_RECAST + 4 <= CombatPatch::BLOCK_LEN);
        assert!(CombatPatch::IAIDO_TILES + 2 * u32::from(CombatPatch::IAIDO_TILES_MAX) <= CombatPatch::IAIDO_N);
        assert_eq!((cp.update, cp.quickbar_slot, cp.cooldown_slot), ((0x0018_b6bc, 0x0014_dd84), 0x0019_3bac, 0x0019_3bbc), "★★ 쌍월: a.g()V · ag.j · ag.a");
        assert_eq!((cp.quickbar_slot, cp.cooldown_slot), (GuestBuild::Hero2Ktf.hero2_java().unwrap().quickbar_slot, GuestBuild::Hero2Ktf.hero2_java().unwrap().cooldown_slot));
        assert_eq!((cp.update.0 - 0x0018_5644) % 0x1c, 0, "a definition record");
        assert_eq!(CombatPatch::IAIDO_VALUE, 7, "발도 = the knight's quickbar value 7");
        assert!(at.iter().all(|a| a % 4 == 0), "literal pools need word alignment");
        // 도검·둔기 +10 %, 단검 +20 %, 대검·도끼 untouched: 단검 < 검·둔기 < 대검·도끼 stays.
        let pct = |k: u8| f64::from(k) * 100.0 / f64::from(1u32 << CombatPatch::WEAPON_SHIFT);
        assert_eq!(cp.weapon_extra[0..2], [0, 0]);
        assert!((pct(cp.weapon_extra[2]) - 10.0).abs() < 0.1 && (pct(cp.weapon_extra[4]) - 10.0).abs() < 0.1);
        assert!((pct(cp.weapon_extra[3]) - 20.0).abs() < 0.1);
        // The knight's skills the sweep and the extras name.
        assert_eq!((CombatPatch::BLUDGEON, CombatPatch::SHIELD_BASH), (3 + 2, 3 + 3), "sk1 스킬 2·3 after 3 stages");
        let sp = GuestBuild::Hero2Ktf.skill_power_patch().unwrap();
        assert_eq!(sp.extras[0], &[] as &[(u8, u8, u8)], "로그: no extras");
        assert_eq!(sp.extras[1], &[] as &[(u8, u8, u8)], "버서커: no extras");
        let k = sp.extras[2];
        assert_eq!(k.iter().map(|e| e.0).collect::<Vec<_>>(), [4, CombatPatch::BLUDGEON, CombatPatch::SHIELD_BASH, CombatPatch::IAIDO]);
        let extra = |m: u8| f64::from(m) / f64::from(1u32 << SkillPowerPatch::EXTRA_SHIFT);
        let buffed = |m: u8| 1.0 + f64::from(m) / f64::from(1u32 << SkillPowerPatch::BUFF_SHIFT);
        assert!((extra(k[0].1) - 0.30).abs() < 0.005 && (extra(k[1].1) - 0.30).abs() < 0.005, "검기·둔기강타 +30 %");
        assert!((extra(k[2].1) - 0.50).abs() < 0.005, "방패치기 +50 %");
        assert_eq!(k[3].1, 0, "발도: no unbuffed raise");
        assert_eq!(buffed(k[1].2), 2.0, "under 신의가호: 둔기강타 ×2");
        // ★ 2026-10-03 저녁 (user): Lv5, × the attack before the defence — 검기 kept ≈7.8, under 신의가호 ≈12 ·
        // 방패치기 ≈5, under 신의가호 ≈7.8 · 발도 ≈10, under 신의가호 ≈18. Each = what the sweep hands in ×
        // the class body's level factor (the game's own: 검기 (lv+4)/3, none for 방패치기·발도) × 1.5 × the extras.
        let lv = 5.0f64;
        let wave = (1.0 + 1.0 / (8.0 - lv)) * ((lv + 4.0) / 3.0) * 1.5 * (1.0 + extra(k[0].1));
        let bash = (lv + f64::from(CombatPatch::SHIELD_POWER_ADD)) * 2.0 / f64::from(CombatPatch::SHIELD_POWER_DIV) * 1.5 * (1.0 + extra(k[2].1));
        let iaido = (lv + f64::from(CombatPatch::IAIDO_POWER_ADD)) * (lv + f64::from(CombatPatch::IAIDO_POWER_ADD_2)) / f64::from(CombatPatch::IAIDO_POWER_DIV) * 1.5;
        for (name, got, want) in [
            ("검기", wave, 7.8),
            ("신의가호 검기", wave * buffed(k[0].2), 12.0),
            ("방패치기", bash, 5.0),
            ("신의가호 방패치기", bash * buffed(k[2].2), 7.8),
            ("발도", iaido, 10.0),
            ("신의가호 발도", iaido * buffed(k[3].2), 18.0),
        ] {
            assert!((got - want).abs() < 0.06, "{name}: Lv5 ×{got:.3}, wanted ≈{want}");
        }
        // No level of 방패치기 or 발도 loses against the old factors (×2.25 · (lv+5)/2).
        for lv in 1..=5u32 {
            let lvf = f64::from(lv);
            let bash = (lvf + 5.0) * 2.0 / 9.0 * 2.25;
            let iaido = (lvf + 5.0) * (lvf + 3.0) / 12.0 * 1.5;
            assert!(bash >= 2.25 && iaido >= (lvf + 5.0) / 2.0 - 1e-9, "Lv{lv}: 방패치기 ×{bash:.2} · 발도 ×{iaido:.2}");
        }
        assert_eq!(sp.buff_off, cp.blessing_off, "the extras test the flag the combat block tests");
        assert!(SkillPowerPatch::BODY + 4 == SkillPowerPatch::JOB_BLOCK);
        // Front tiles: the row ahead first (ahead, ahead ± side), then the row beyond.
        for facing in 1..=4u8 {
            let t = CombatPatch::front_tiles(facing);
            let (fx, fy) = (cp.code_dx[usize::from(facing)], cp.code_dy[usize::from(facing)]);
            assert_eq!(t[0], (fx, fy), "facing {facing}: straight ahead = the game's own code");
            // The two front diagonals ARE the game's co.c/co.d codes for that facing.
            let mut diag: Vec<(i8, i8)> = cp.front_diag.iter().map(|d| {
                let c = usize::from(d[usize::from(facing)]);
                (cp.code_dx[c], cp.code_dy[c])
            }).collect();
            let mut mine = vec![t[1], t[2]];
            diag.sort();
            mine.sort();
            assert_eq!(mine, diag, "facing {facing}: the front diagonals");
            for (i, &(dx, dy)) in t.iter().enumerate() {
                let ahead = i32::from(dx) * i32::from(fx) + i32::from(dy) * i32::from(fy);
                let side = (i32::from(dx) * i32::from(fy) - i32::from(dy) * i32::from(fx)).abs();
                assert_eq!(ahead, if i < 3 { 1 } else { 2 }, "facing {facing} tile {i}: its row");
                assert!(side <= 1, "facing {facing} tile {i}: within one tile of the line");
            }
            let mut all = t.to_vec();
            all.sort();
            all.dedup();
            assert_eq!(all.len(), 6, "facing {facing}: six distinct tiles");
        }
        assert_eq!((CombatPatch::FRONT_ROW, CombatPatch::FRONT_TWO_ROWS), (3, 6));
        // ★ 2026-10-01: the blessed 둔기강타's ring — the eight lines out of the knight, two tiles each
        // (user: 「8방향 직선 2칸씩」): sixteen distinct tiles, the first eight one tile out, the last
        // eight their doubles, every one straight or diagonal from the knight, none the knight's own.
        let ring = CombatPatch::ring_tiles();
        assert_eq!(ring.len(), usize::from(CombatPatch::RING_TILES));
        let mut all = ring.to_vec();
        all.sort();
        all.dedup();
        assert_eq!(all.len(), 16, "sixteen distinct tiles");
        for (i, &(dx, dy)) in ring.iter().enumerate() {
            let reach = if i < 8 { 1 } else { 2 };
            assert_eq!(dx.abs().max(dy.abs()), reach, "tile {i} ({dx},{dy}): its ring");
            assert!(dx == 0 || dy == 0 || dx.abs() == dy.abs(), "tile {i} ({dx},{dy}): on a straight line");
            if i >= 8 {
                assert_eq!((dx, dy), (2 * ring[i - 8].0, 2 * ring[i - 8].1), "tile {i}: the same line, one further");
            }
        }
        // ★ 2026-10-03: the star (three tiles out) extends the ring; the neighbours are its first eight;
        // the disc is the 7×7 without its corners; the lobes are what the user drew, nearest first.
        let star = CombatPatch::star_tiles();
        assert_eq!(star.len(), usize::from(CombatPatch::STAR_TILES));
        assert_eq!(&star[..16], &ring[..], "the star's first sixteen are the ring");
        assert_eq!(CombatPatch::neighbour_tiles(), ring[..8], "the neighbours are the ring's first eight");
        for (i, &(dx, dy)) in star.iter().enumerate().skip(16) {
            assert_eq!((dx, dy), (3 * ring[i - 16].0, 3 * ring[i - 16].1), "tile {i}: the same line, three out");
        }
        let disc = CombatPatch::disc_tiles();
        assert_eq!(disc.len(), usize::from(CombatPatch::DISC_TILES));
        let mut all = disc.to_vec();
        all.sort();
        all.dedup();
        assert_eq!(all.len(), 36, "thirty-six distinct tiles");
        for &(dx, dy) in &disc {
            assert!((dx, dy) != (0, 0) && dx.abs() <= 3 && dy.abs() <= 3 && dx.abs() + dy.abs() <= 4, "({dx},{dy}): inside the disc");
        }
        assert!(disc.windows(2).all(|w| w[0].0.abs().max(w[0].1.abs()) <= w[1].0.abs().max(w[1].1.abs())), "nearest ring first");
        for (dx, dy) in [(0i8, 0i8), (3, 2), (2, 3), (-3, -2), (4, 0), (0, -4)] {
            assert!(!disc.contains(&(dx, dy)), "({dx},{dy}) is outside");
        }
        for facing in 1..=4u8 {
            let (fx, fy) = (cp.code_dx[usize::from(facing)], cp.code_dy[usize::from(facing)]);
            for big in [false, true] {
                let n = usize::from(if big { CombatPatch::BLESSED_LOBE_TILES } else { CombatPatch::LOBE_TILES });
                let lobe = &CombatPatch::lobe_tiles(facing, big)[..n];
                let mut d = lobe.to_vec();
                d.sort();
                d.dedup();
                assert_eq!(d.len(), n, "facing {facing} big {big}: distinct tiles");
                let mut last = 0;
                for &(dx, dy) in lobe {
                    let ahead = i32::from(dx) * i32::from(fx) + i32::from(dy) * i32::from(fy);
                    let side = (i32::from(dx) * i32::from(fy) - i32::from(dy) * i32::from(fx)).abs();
                    assert!(ahead >= last, "facing {facing}: nearest first");
                    last = ahead;
                    let (depth, half) = if big { (5, 4) } else { (3, 2) };
                    assert!((1..=depth).contains(&ahead) && side <= half, "facing {facing} big {big} ({dx},{dy}): in the lobe");
                    // The far rows are pulled forward: a tile `side` out starts `side` ahead (1 on the centre rows).
                    assert!(ahead >= side.max(1), "facing {facing} big {big} ({dx},{dy}): the row's front");
                }
                // The centre row reaches the full depth; every tile of the small lobe is in the big one.
                assert!(lobe.contains(&(fx * if big { 5 } else { 3 }, fy * if big { 5 } else { 3 })));
                if !big {
                    let bigger = CombatPatch::lobe_tiles(facing, true);
                    assert!(lobe.iter().all(|t| bigger.contains(t)), "facing {facing}: the small lobe inside the big");
                }
            }
        }
        assert_eq!((CombatPatch::LOBE_TILES, CombatPatch::BLESSED_LOBE_TILES, CombatPatch::DISC_TILES), (13, 27, 36));
        assert_eq!((CombatPatch::IAIDO, CombatPatch::IAIDO_GUARD, CombatPatch::IAIDO_CD_INDEX), (9, 8, 6), "sk1 스킬 6 (부활오라's slot) after 3 stages · 방어의지 · the seventh active");
        assert_eq!((CombatPatch::WAVE, CombatPatch::WAVE_HIT_FRAME, CombatPatch::IAIDO_HIT_FRAME), (4, 5, 3));
        assert!(CombatPatch::IAIDO_HIT_FRAME < CombatPatch::IAIDO_FRAMES, "the hit frame is inside the sequence");
        assert_eq!((CombatPatch::WAVE_RING_SEQ, CombatPatch::BLESSED_WAVE_RING_SEQ, CombatPatch::IAIDO_SEQ), (21, 22, 13));
        // The three wrapped defs are method def records in bx's text; the wave's halfword is in ak.b()V.
        for (def, body) in [cp.cast, cp.sp_cost, cp.cooldown] {
            assert_eq!((def - 0x0018_5644) % 0x1c, 0, "{def:#x}: a method def");
            assert!(body > 0x0013_b35c && body < 0x0013_bf88, "{body:#x} lives in bx's text");
        }
        assert_eq!((cp.cast.1, cp.sp_cost.1, cp.cooldown.1), (0x0013_b45c, 0x0013_bd88, 0x0013_bd58));
        assert!((0x0011_78e4..0x0011_7b70).contains(&cp.wave_no_hit.0), "ak.b()V");
        assert_eq!(cp.wave_no_hit, (0x0011_7a5c, 0xd100, 0xe778), "bne 0x117a60 -> b 0x117950");
        // The drawn sequences: two past the knight's shipped nineteen, and the shipped ones they replace.
        assert_eq!((CombatPatch::BLESSED_SLAM_SEQ, CombatPatch::BLESSED_WAVE_SEQ), (19, 20));
        assert_eq!(CombatPatch::SHIPPED_WAVE_SEQ, 17, "ak.b(II)V: movs r1,#0x11");
        assert!(CombatPatch::BLESSED_SLAM_SEQ - 4 < 128, "the map returns a byte");
        // The sequence map's def is a method def record, and the wave's frame cell / the drawer's
        // static helper are the ones ak.b(II)V uses.
        assert_eq!((cp.seq_map.0 - 0x0018_5644) % 0x1c, 0, "bx.a(B)B: a method def");
        assert!(cp.seq_map.1 > 0x0013_b35c && cp.seq_map.1 < 0x0013_bd58, "bx.a(B)B lives in bx's text");
        assert_eq!(cp.wave_frame_off, 0x34 + 0x8, "cb.d:S = ad's 0x34 + 8");
        assert_eq!(cp.static_call, 0x0016_916e, "the static call helper");
        assert_eq!(cp.cell_draw_seq, 0x0019_5ca4u32.wrapping_add(0xfffe_1268), "sl + ak.b(II)V's pool literal");
        // The guard arm: adds r1,r4,#0 → bx r0, between the guard branch's load and its test.
        assert_eq!(cp.guard_arm.2, 0x4700, "bx r0");
        assert!(cp.guard_arm.0 < cp.guard_hit && cp.guard_hit < cp.guard_miss);
        // ★ 2026-10-05 철괴: the same shape on the ×½ branch's `a.m:Z` load — `ldr r0,[pc]` two before the arm,
        // its pool word the one that load reads, the clamp right after the shipped ×½ (0x151d78..=0x151d7c).
        assert_eq!(cp.iron_arm, (0x0015_1d6c, 0x1c21, 0x4700), "adds r1,r4,#0 → bx r0");
        assert_eq!(((cp.iron_arm.0 - 2 + 4) & !3) + 4 * 0x68, cp.iron_pool.0, "ldr r0,[pc,#0x1a0] at 0x151d6a");
        assert_eq!(0x0019_5ca4u32.wrapping_add(cp.iron_pool.1), 0x0017_ee40, "the a.m:Z cell");
        assert_eq!(cp.iron_back, cp.iron_arm.0 + 0x12, "mov r2,r8 — the min-3 clamp");
        assert!(cp.guard_miss < cp.iron_arm.0, "the cut runs after the guard test");
        assert_eq!(cp.halve_off, 0x5c + 0xa4, "a.m:Z");
        assert!(CombatPatch::IRON_AT >= CombatPatch::SWEEP_AT && CombatPatch::IRON_AT < CombatPatch::BLOCK_LEN);
        assert_eq!(CombatPatch::iron_reduction_permille(1, 0, 0), 100);
        assert_eq!(CombatPatch::iron_reduction_permille(5, 0, 0), 200);
        assert_eq!(CombatPatch::iron_reduction_permille(5, 5, 0), 250);
        // The 둔기강타 literal is sl-relative and moves to bx.h(B)Z's cell.
        assert_ne!(cp.bludgeon_pool.1, cp.bludgeon_pool.2);
        // Monster classes: five distinct descriptors.
        let mut d = cp.monster_descs.to_vec();
        d.sort();
        d.dedup();
        assert_eq!(d.len(), 5);
        // The flags the pieces read are the character's, and distinct.
        let mut f = vec![cp.blessing_off, cp.invuln_off, cp.stance_off, cp.halve_off];
        f.sort();
        f.dedup();
        assert_eq!(f.len(), 4);
        assert_eq!(cp.skills_off, GuestBuild::Hero2Ktf.hero2_java().unwrap().char_skill_levels_off, "a.p");
    }
}
