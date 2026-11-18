// Standalone self-test of the costume STATE and its save binding (의상 아바타 A2 · ★ A3c 네 칸).
//
//   rustc --edition 2021 --test avatar_state_selftest.rs -o t.exe && ./t.exe
//
// ★ `include!`s the REAL `wie_android/src/avatar_state.rs` — the rules under test are the
// shipping rules (same arrangement as savevault_selftest.rs / quickpage_selftest.rs).
//
// What is at stake: every decision here is "does this character own this costume after this
// load". Wrong one way and a costume gained after the last save survives a death (the user's
// rule broken, and a reward can be farmed); wrong the other way and a saved costume vanishes.

include!("wie_android/src/avatar_state.rs");

#[cfg(test)]
mod tests {
    use super::*;

    /// Every slot — a whole set, as the catalog gives Ian's 200.
    const ALL: [u8; 5] = [SLOT_HEAD, SLOT_ARMOUR, SLOT_SHOES, SLOT_WEAPON, SLOT_OFFHAND];

    /// A `/g` the way `ag.g(B)V` writes one: five `bg.a`-encrypted blobs, then four bytes.
    fn g_file(plains: [&[u8]; 5]) -> Vec<u8> {
        let mut out = Vec::new();
        for (k, pt) in plains.iter().enumerate() {
            let key = G_KEYS[k];
            let mut e: Vec<u8> = pt.iter().enumerate().map(|(i, b)| b ^ key[(i + 1) % 6]).collect();
            let sum: u32 = (0..pt.len()).map(|i| u32::from(key[(i + 1) % 6])).sum();
            e.push((sum & 0xff) as u8);
            out.extend_from_slice(&(e.len() as u16).to_be_bytes());
            out.extend_from_slice(&e);
        }
        out.extend_from_slice(&[0, 0, 0, 0]);
        out
    }

    fn save(level: u8) -> Vec<u8> {
        g_file([&[40, 20, 28, 2, 1], &[level, 0, 0, 7, 9], &[1, 2, 3], &[0], &[level, 0]])
    }

    // ── the state ────────────────────────────────────────────────────────────

    #[test]
    fn owning_wearing_and_receipts() {
        let mut s = AvatarState::fresh(0xabc);
        let a = s.grant(200, SLOT_ARMOUR, 0, 0);
        let b = s.grant(200, SLOT_ARMOUR, 1, 0);
        assert_ne!(a, b, "the same item twice is two instances");
        assert_eq!(s.count(200), 2);
        assert!(s.wear(SLOT_ARMOUR, Some(b)));
        assert_eq!(s.worn_avatars(), [None, Some(200), None, None, None]);
        assert!(!s.wear(SLOT_ARMOUR, Some(999)), "only an owned instance can be worn");
        assert!(!s.wear(SLOT_HEAD, Some(a)), "…and only in its own slot");
        assert!(!s.wear(9, None), "a slot that is not one");
        assert_eq!(s.wear, [None, Some(b), None, None, None], "a refused wear changes nothing");
        assert!(s.wear(SLOT_ARMOUR, None));
        assert_eq!(s.worn_avatars(), [None; SLOTS]);
        // One reward once per (source, mode, occurrence); a completion from before the reward
        // existed is never paid.
        let r = s.receipt(7, 0, 0).expect("first time");
        assert!(s.receipt(7, 0, 0).is_none(), "paid twice");
        assert!(s.receipt(7, 1, 0).is_some(), "the next mode pays again");
        s.done.push((8, 0));
        assert!(s.receipt(8, 0, 0).is_none(), "a baseline is never paid retroactively");
        assert!(s.receipt(8, 1, 0).is_some());
        assert!(r > b, "ids are shared and increasing");
    }

    /// ★ A3c — the user's rule 3/4: a set is separate items, and slots combine sets. ★ A4: five.
    #[test]
    fn a_set_is_its_items_and_the_slots_mix_sets() {
        let mut s = AvatarState::fresh(1);
        let ian = s.grant_set(200, &ALL, 0, 0);
        assert_eq!(ian.len(), 5);
        assert_eq!(s.items.iter().map(|i| (i.avatar, i.slot)).collect::<Vec<_>>(), vec![(200, 0), (200, 1), (200, 2), (200, 3), (200, 4)]);
        // Ulrike's set has only clothes (the dress) — its catalog names one item.
        let dress = s.grant_set(201, &[SLOT_ARMOUR], 1, 0)[0];
        // Ian's head, Ulrike's dress, Ian's shoes, no weapon look.
        assert!(s.wear(SLOT_HEAD, Some(ian[0])) && s.wear(SLOT_ARMOUR, Some(dress)) && s.wear(SLOT_SHOES, Some(ian[2])));
        assert_eq!(s.worn_avatars(), [Some(200), Some(201), Some(200), None, None]);
        let has = |_: u8, n: i32| n == 200 || n == 201;
        // The loader's six parts (body · arms · legs · head · weapon · off-hand): each from its slot.
        assert_eq!(costume_parts(s.worn_avatars(), has), [(201, -1), (201, -1), (200, -1), (200, -1), (-1, -1), (-1, -1)]);
        // Taking the dress off gives the body back to the equipment; the head stays Ian's.
        s.wear(SLOT_ARMOUR, None);
        assert_eq!(costume_parts(s.worn_avatars(), has), [(-1, -1), (-1, -1), (200, -1), (200, -1), (-1, -1), (-1, -1)]);
    }

    #[test]
    fn the_slot_table_matches_the_parts() {
        for (i, part) in COSTUME_PARTS.iter().enumerate() {
            let slot = SLOT_OF_PART[i];
            assert!(SLOT_PARTS[usize::from(slot)].contains(part), "part {part} is slot {slot}'s");
        }
        let all: usize = SLOT_PARTS.iter().map(|p| p.len()).sum();
        assert_eq!(all, COSTUME_PARTS.len(), "every part belongs to exactly one slot");
        for (i, k) in SLOT_KEYS.iter().enumerate() {
            assert_eq!(slot_of_key(k), Some(i as u8));
            assert_eq!(slot_key(i as u8), Some(*k));
        }
        assert_eq!((slot_of_key("gloves"), slot_key(5)), (None, None));
        assert_eq!(slot_key(SLOT_OFFHAND), Some("offhand"));
    }

    #[test]
    fn records_round_trip_and_keep_what_they_do_not_know() {
        let mut s = AvatarState::fresh(0x1234_5678_9abc_def0);
        let i = s.grant(200, SLOT_WEAPON, 2, 5);
        let h = s.grant(201, SLOT_HEAD, 0, 0);
        s.wear(SLOT_WEAPON, Some(i));
        s.wear(SLOT_HEAD, Some(h));
        s.receipt(3, 2, 0);
        s.done.push((9, 1));
        s.unknown.push("dye 1 200 3".into());
        for kind in [RecordKind::Committed, RecordKind::Pending] {
            let text = encode_record(kind, 0xfeed, &s);
            assert!(text.starts_with("hero2-avatar 3\n"), "{text}");
            assert!(text.contains(&format!("\nwear {h} - - {i} -\n")), "{text}");
            let r = decode_record(&text).expect("decodes");
            assert_eq!(r, Record { kind, key: 0xfeed, state: s.clone() });
            assert_eq!(encode_record(kind, 0xfeed, &r.state), text, "byte-stable");
        }
        // A record naming ids at or past `next` gets `next` repaired — a grant after loading
        // must not reuse an id.
        let hand = "hero2-avatar 2\nbind 00000000000000ff\nchar 0000000000000001\nnext 1\nitem 5 200 0 0 1\nwear - 5 - -\n";
        let r = decode_record(hand).unwrap();
        assert_eq!(r.state.next_id, 6);
        assert_eq!(r.state.worn_avatars(), [None, Some(200), None, None, None], "a schema-2 wear line (four slots) reads");
        assert!(decode_record("hero2-avatar 9\nbind 1\nchar 1\n").is_err(), "a newer schema is refused, not guessed at");
        assert!(decode_record("hello\n").is_err());
        assert!(decode_record("hero2-avatar 2\nchar 1\n").is_err(), "no key");
        assert!(decode_record("hero2-avatar 2\nbind 1\n").is_err(), "no character");
        assert!(decode_record("hero2-avatar 2\nbind 1\nchar 1\nitem x 200 0 0 1\n").is_err());
        assert!(decode_record("hero2-avatar 2\nbind 1\nchar 1\nitem 1 70000 0 0 1\n").is_err(), "avatar is a u16");
        assert!(decode_record("hero2-avatar 3\nbind 1\nchar 1\nitem 1 200 0 0 5\n").is_err(), "a slot that is not one");
        assert!(decode_record("hero2-avatar 2\nbind 1\nchar 1\nwear - -\n").is_err(), "a wear line of the wrong shape");
        assert!(decode_record("hero2-avatar 3\nbind 1\nchar 1\nwear - - - - - -\n").is_err(), "six slots is no schema");
    }

    /// ★ A3c: an A2/A3 record (schema 1 — one instance was the whole costume) reads as the
    /// same character: the costume becomes its items (★ A4 five), worn wherever it was worn.
    #[test]
    fn a_schema_1_record_is_migrated_to_items() {
        let old = "hero2-avatar 1\nbind 00000000000000ab\nchar 00000000000000cd\nnext 4\nitem 1 200 0 0\nitem 3 200 1 7\nwear 3\nrecv 2 5 0 0\n";
        let r = decode_record(old).expect("an older schema reads");
        let s = &r.state;
        assert_eq!((r.kind, r.key, s.character), (RecordKind::Committed, 0xab, 0xcd));
        assert_eq!(s.items.len(), 10, "two costumes → ten items");
        assert_eq!(s.items.iter().filter(|i| i.slot == SLOT_ARMOUR).map(|i| i.id).collect::<Vec<_>>(), vec![1, 3], "the clothes keep the old ids");
        assert!(s.items.iter().filter(|i| i.receipt == 7).all(|i| i.mode == 1 && i.avatar == 200), "mode and receipt carried to each item");
        assert_eq!(s.worn_avatars(), [Some(200); SLOTS], "the worn costume is worn in every slot");
        assert!(s.wear.iter().all(|w| s.items.iter().any(|i| Some(i.id) == *w && i.receipt == 7)), "…the one that was worn (instance 3's items)");
        let mut ids: Vec<u32> = s.items.iter().map(|i| i.id).chain(s.receipts.iter().map(|r| r.id)).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), 11, "every id still unique");
        assert!(s.next_id > *ids.last().unwrap(), "and the next grant cannot reuse one");
        // Written back, it is a schema-3 record that reads the same.
        let again = decode_record(&encode_record(RecordKind::Committed, 0xab, s)).unwrap();
        assert_eq!(&again.state, s);
        // The records the phones hold today: schema 1 with nothing owned.
        let empty = decode_record("hero2-avatar 1\nbind 0000000000000001\nchar 0000000000000002\nnext 1\nwear -\n").unwrap();
        assert_eq!((empty.state.items.len(), empty.state.worn_avatars()), (0, [None; SLOTS]));
    }

    /// ★ A4: an A3c record (schema 2 — the weapon item drew the off-hand too) reads as the same
    /// character: each weapon item gets an off-hand twin, worn where the weapon was.
    #[test]
    fn a_schema_2_weapon_becomes_a_weapon_and_an_off_hand() {
        let old = "hero2-avatar 2\nbind 00000000000000ab\nchar 00000000000000cd\nnext 6\nitem 1 200 0 0 0\nitem 2 200 0 0 1\nitem 3 200 0 0 2\nitem 4 200 0 0 3\nitem 5 201 1 9 3\nwear 1 - - 4\nrecv 9 1 1 0\n";
        let s = decode_record(old).expect("schema 2 reads").state;
        assert_eq!(s.items.len(), 7, "five items + an off-hand for each of the two weapons");
        let off: Vec<&Instance> = s.items.iter().filter(|i| i.slot == SLOT_OFFHAND).collect();
        assert_eq!(off.iter().map(|i| (i.avatar, i.mode, i.receipt)).collect::<Vec<_>>(), vec![(200, 0, 0), (201, 1, 9)], "the weapon's set, mode and receipt");
        assert!(off.iter().all(|i| i.id >= 10), "new ids, past every id the record used (receipt 9)");
        assert_eq!(s.worn_avatars(), [Some(200), None, None, Some(200), Some(200)], "worn where the weapon was");
        assert_eq!(s.worn_instance(SLOT_OFFHAND).map(|i| i.avatar), Some(200));
        assert!(s.next_id > s.items.iter().map(|i| i.id).max().unwrap());
        // Written back as schema 3, it reads the same — no second split.
        let text = encode_record(RecordKind::Committed, 0xab, &s);
        assert!(text.starts_with("hero2-avatar 3\n"));
        assert_eq!(decode_record(&text).unwrap().state, s);
        // A schema-3 record's weapon is only a weapon.
        let three = "hero2-avatar 3\nbind 1\nchar 1\nnext 2\nitem 1 200 0 0 3\nwear - - - 1 -\n";
        assert_eq!(decode_record(three).unwrap().state.items.len(), 1);
    }

    /// ★ A4: the boot grant gives a character its own class's items — the class is in its save.
    #[test]
    fn the_class_is_read_from_the_save() {
        assert_eq!(g_class(&save(48)), Some(1), "save() writes a knight");
        let with_class = |c: u8| g_file([&[40, 20, 28, 2, c], &[48, 0, 0, 7, 9], &[1, 2, 3], &[0], &[48, 0]]);
        assert_eq!([0u8, 1, 2].map(|c| g_class(&with_class(c))), [Some(0), Some(1), Some(2)]);
        assert_eq!(g_class(&with_class(3)), None, "not a class");
        let g = save(48);
        assert_eq!(g_class(&g[..g.len() - 1]), None, "not a whole save");
        assert_eq!(g_class(&g_file([&[40, 20, 28], &[48], &[1], &[0], &[48]])), None, "a stage blob too short");
        assert_eq!([Some(0), Some(1), Some(2), None].map(class_label), ["berserker", "knight", "rogue", "class unknown"]);
    }

    #[test]
    fn the_clear_rewards_table() {
        let of = |m: u8| clear_rewards(m).iter().map(|r| (r.set, r.min_difficulty, r.notice_row)).collect::<Vec<_>>();
        assert_eq!(of(0), vec![(200, 0, 65)], "NORMAL → 이안, any 난이도 (EASY or above)");
        assert_eq!(of(1), vec![(201, 0, 66)], "HARDCORE → 율리케, any 난이도");
        // ★ 2026-10-02: the two HELL sets traded difficulties (user) — 할로윈 for any HELL clear, 수영장 파티
        // for one played at NORMAL or above; each keeps the receipt it had.
        // ★ 2026-10-06: + 할로윈 파티 205 at HARD or above (user: *"hell 난이도 hard이상 클리어"*), 「오딘 아바타 획득」.
        // ★ 2026-10-09: + 백은의 사신 210 at HARD or above (「사신 아바타 획득」, itmtp 74).
        assert_eq!(of(2), vec![(204, 0, 68), (203, 1, 67), (205, 2, 69), (210, 2, 74)], "HELL → 할로윈 (any) · 수영장 파티 (NORMAL+) · 할로윈 파티 · 백은의 사신 (HARD+)");
        // ★ 2026-10-08 가디언: GUARDIAN → 제복 206 at EASY or above (「제복 아바타 획득」, itmtp 70); nothing for a mode above.
        // ★ 2026-10-09: + 진홍의 검사 207 (EASY+, 71) · 순백의 해머 208 (NORMAL+, 72) · 달빛 엘프 209 (HARD+, 73).
        assert_eq!(of(3), vec![(206, 0, 70), (207, 0, 71), (208, 1, 72), (209, 2, 73)], "GUARDIAN → 제복 · 진홍의 검사 (any) · 순백의 해머 (NORMAL+) · 달빛 엘프 (HARD+)");
        assert_eq!(of(MODE_GUARDIAN), of(3));
        assert!(of(4).is_empty());
        // ★ 2026-10-09: a GUARDIAN clear is HELL at HARD; nothing else implies anything (the save shows the rest).
        assert_eq!([0, 1, 2, 3, 4].map(implied_clear), [None, None, None, Some(MODE_HELL), None]);
        assert_eq!(MODE_HELL, 2);
        let source_of = |set: u16| CLEAR_REWARDS.iter().find(|r| r.set == set).map(|r| r.source);
        assert_eq!((source_of(203), source_of(204)), (Some(CLEAR_REWARD_SOURCE), Some(CLEAR_REWARD_SOURCE_HALLOWEEN)), "a receipt names its set");
        assert_eq!(source_of(205), Some(CLEAR_REWARD_SOURCE_HALLOWEEN_PARTY));
        assert_eq!(source_of(206), Some(CLEAR_REWARD_SOURCE_UNIFORM));
        assert_eq!(
            [207, 208, 209, 210].map(source_of),
            [CLEAR_REWARD_SOURCE_GUARDIAN_EASY, CLEAR_REWARD_SOURCE_GUARDIAN_NORMAL, CLEAR_REWARD_SOURCE_GUARDIAN_HARD, CLEAR_REWARD_SOURCE_HELL_HARD].map(Some)
        );
        // No two rewards share a (source, mode) receipt, and none is a refund's (100 · 101).
        for (i, a) in CLEAR_REWARDS.iter().enumerate() {
            assert!(CLEAR_REWARDS[i + 1..].iter().all(|b| (b.source, b.mode) != (a.source, a.mode)), "set {}: receipt shared", a.set);
            assert!(a.source != RECEIPT_SIGHT_REFUND && a.source != RECEIPT_BODYSLAM_REFUND);
        }
        // Every notice row is its own, in the asset's order (`make_hero2_asset.py` ITMTP_APPEND: 64 부활의 구, then 65..).
        let mut rows: Vec<u32> = CLEAR_REWARDS.iter().map(|r| r.notice_row).collect();
        rows.sort_unstable();
        assert_eq!(rows, (65..=74).collect::<Vec<_>>());
        // Two rewards of one mode never share a receipt — or the second would read as paid.
        for m in 0..4 {
            let r = clear_rewards(m);
            for (i, a) in r.iter().enumerate() {
                assert!(r[i + 1..].iter().all(|b| b.source != a.source), "mode {m}: receipt shared");
            }
        }
        // A4's receipts keep their key (source 1): a character paid by vC 63 is not paid twice.
        assert_eq!((CLEAR_REWARDS[0].source, CLEAR_REWARDS[1].source), (CLEAR_REWARD_SOURCE, CLEAR_REWARD_SOURCE));
        assert_eq!(STARTER_SETS, [202], "클레르의 잠옷");
        assert_eq!([0, 1, 2, 3, 4].map(mode_label), ["NORMAL", "HARDCORE", "HELL", "GUARDIAN", "?"]);
        assert_eq!([0, 1, 2, 3].map(difficulty_label), ["EASY", "NORMAL", "HARD", "?"]);
    }

    /// ★ A5: 「난이도 노말인채로 진행도가 95프로 이상」.
    #[test]
    fn the_ninety_five_percent_rule() {
        assert!(difficulty_met(0, (0, 0)), "EASY asks nothing — not even a quest");
        assert!(!difficulty_met(1, (0, 0)), "a round the host saw nothing of proves nothing");
        assert!(difficulty_met(1, (19, 20)), "95 %");
        assert!(!difficulty_met(1, (18, 20)), "90 %");
        assert!(difficulty_met(1, (38, 40)), "95 %");
        assert!(!difficulty_met(1, (37, 40)), "92.5 %");
        assert!(difficulty_met(1, (1, 1)));
        assert!(!difficulty_met(1, (0, 1)), "the whole round at EASY, the difficulty switched for the clear");
    }

    /// ★ A5: the marks follow the game's quest states; the setting a quest was done at sticks.
    #[test]
    fn quest_marks_follow_the_game() {
        let mut s = AvatarState::fresh(1);
        let mut done = [false; QUESTS];
        done[3] = true;
        done[7] = true;
        assert_eq!(s.observe_quests(&done, 1, false), vec![3, 7]);
        assert_eq!(s.run_progress(1), (2, 2));
        done[9] = true;
        assert_eq!(s.observe_quests(&done, 0, false), vec![9], "the next one at EASY");
        assert_eq!((s.run_progress(1), s.run_progress(0)), ((2, 3), (3, 3)));
        assert!(s.observe_quests(&done, 2, false).is_empty(), "switching up later re-marks nothing");
        assert_eq!(s.run_progress(1), (2, 3));
        assert_eq!(s.run_progress(2), (0, 3), "HARD counts only what was done at HARD");
        // The next round: the game opened every quest.
        assert!(s.observe_quests(&[false; QUESTS], 1, false).is_empty());
        assert_eq!(s.run_progress(0), (0, 0));
        assert_eq!(s.quests, [QUEST_OPEN; QUESTS]);
        // A load: a done quest the record has no mark for is unknown — in the total, never NORMAL+.
        let mut t = AvatarState::fresh(2);
        t.quests[3] = 1;
        let mut done = [false; QUESTS];
        done[3] = true;
        done[4] = true;
        assert!(t.observe_quests(&done, 2, true).is_empty(), "the baseline marks nothing with the setting");
        assert_eq!((t.quests[3], t.quests[4]), (1, QUEST_UNKNOWN));
        assert_eq!((t.run_progress(1), t.run_progress(0)), ((1, 2), (1, 2)));
        // The baseline takes no mark away — the next look does.
        let mut u = AvatarState::fresh(3);
        u.quests[5] = 1;
        u.observe_quests(&[false; QUESTS], 0, true);
        assert_eq!(u.quests[5], 1);
        u.observe_quests(&[false; QUESTS], 0, false);
        assert_eq!(u.quests[5], QUEST_OPEN, "the next look follows the game");
        // A setting out of range marks the top difficulty, never a value no rule reads.
        let mut v = AvatarState::fresh(4);
        let mut one = [false; QUESTS];
        one[0] = true;
        v.observe_quests(&one, 9, false);
        assert_eq!(v.quests[0], DIFFICULTIES - 1);
    }

    /// ★ A5: the marks ride in the record (schema 3 — an older build keeps the line verbatim).
    #[test]
    fn quest_marks_ride_in_the_record() {
        let mut s = AvatarState::fresh(0x55);
        s.quests[0] = 0;
        s.quests[10] = 1;
        s.quests[20] = QUEST_UNKNOWN;
        s.quests[50] = 2;
        let text = encode_record(RecordKind::Committed, 9, &s);
        let line = text.lines().find(|l| l.starts_with("quest ")).expect("a quest line");
        assert_eq!(line.len(), "quest ".len() + QUESTS);
        assert_eq!(&line[6..], format!("0{}1{}?{}2", "-".repeat(9), "-".repeat(9), "-".repeat(29)));
        assert_eq!(decode_record(&text).unwrap().state.quests, s.quests);
        assert!(!encode_record(RecordKind::Committed, 9, &AvatarState::fresh(1)).contains("quest"), "no line while nothing is marked");
        // Lenient: a short line, a mark this build does not know.
        let r = decode_record("hero2-avatar 3\nbind 0000000000000009\nchar 0000000000000001\nquest 1x\n").unwrap();
        assert_eq!((r.state.quests[0], r.state.quests[1], r.state.quests[2]), (1, QUEST_UNKNOWN, QUEST_OPEN));
        assert!(r.state.unknown.is_empty(), "read, not kept as an unknown line");
    }

    /// ★★ 2026-10-22 가디언 레벨: the GL rides in the record as one `gl` line (schema 3 unchanged), only when > 0; a bad
    /// number reads as no GL, never a lost record.
    #[test]
    fn the_guardian_level_rides_in_the_record() {
        let mut s = AvatarState::fresh(0x77);
        assert!(!encode_record(RecordKind::Committed, 9, &s).contains("\ngl "), "no line below 99");
        s.gl = 137;
        let text = encode_record(RecordKind::Committed, 9, &s);
        assert!(text.starts_with("hero2-avatar 3\n"), "schema 3: an older build still reads it");
        assert!(text.lines().any(|l| l == "gl 137"));
        let back = decode_record(&text).unwrap();
        assert_eq!((back.state.gl, back.state.unknown.len()), (137, 0));
        assert_eq!(encode_record(RecordKind::Committed, 9, &back.state), text, "byte-stable");
        let r = decode_record("hero2-avatar 3\nbind 0000000000000009\nchar 0000000000000001\ngl x\n").unwrap();
        assert_eq!(r.state.gl, 0);
    }

    #[test]
    fn the_hash_and_the_character_id() {
        assert_eq!(fnv1a64(b""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fnv1a64(b"a"), 0xaf63_dc4c_8601_ec8c);
        assert_ne!(fnv1a64(&save(1)), fnv1a64(&save(2)));
        let ids: Vec<u64> = (0..100).map(character_id).collect();
        assert!(ids.iter().all(|&v| v != 0));
        let mut d = ids.clone();
        d.sort_unstable();
        d.dedup();
        assert_eq!(d.len(), ids.len());
    }

    #[test]
    fn a_g_file_is_whole_or_it_is_not() {
        let g = save(48);
        assert!(g_complete(&g));
        for cut in 0..g.len() {
            assert!(!g_complete(&g[..cut]), "cut at {cut}");
        }
        let mut longer = g.clone();
        longer.push(0);
        assert!(!g_complete(&longer), "a byte past the tail");
        // A blob whose checksum byte is not the key sum: a file the game has not finished
        // writing (zero-filled past the write point).
        let mut torn = g.clone();
        let first_len = usize::from(u16::from_be_bytes([g[0], g[1]]));
        torn[2 + first_len - 1] ^= 0x5a;
        assert!(!g_complete(&torn));
        assert!(!g_complete(&[0u8; 30]), "zero lengths");
    }

    // ── loading and committing — the user's rules as scenarios ────────────────

    fn committed(g: &[u8], s: &AvatarState) -> Record {
        Record { kind: RecordKind::Committed, key: fnv1a64(g), state: s.clone() }
    }

    fn pending(base: Bind, s: &AvatarState) -> Record {
        Record { kind: RecordKind::Pending, key: base, state: s.clone() }
    }

    #[test]
    fn a_costume_gained_after_the_last_save_is_gone_after_a_death() {
        // Save 1 with one item worn.
        let mut working = AvatarState::fresh(1);
        let i = working.grant(200, SLOT_ARMOUR, 0, 0);
        working.wear(SLOT_ARMOUR, Some(i));
        let g1 = save(10);
        let (bind, state) = commit_after_save(&g1, None, Some(&working)).expect("whole save");
        assert_eq!(bind, fnv1a64(&g1));
        let rec = committed(&g1, &state);
        // Play on: a second item, worn — then die. The game loads the SAME g1.
        let j = working.grant(201, SLOT_ARMOUR, 0, 0);
        working.wear(SLOT_ARMOUR, Some(j));
        match resolve_load(Some(&g1), Some(&rec), None) {
            Loaded::Committed(s) => {
                assert_eq!(s.count(201), 0, "the unsaved item is gone");
                assert_eq!(s.worn_avatars()[usize::from(SLOT_ARMOUR)], Some(200), "and the saved one is worn again");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_saved_costume_survives_a_restart() {
        let mut working = AvatarState::fresh(2);
        let i = working.grant(200, SLOT_HEAD, 1, 0);
        working.wear(SLOT_HEAD, Some(i));
        let g = save(20);
        let (bind, state) = commit_after_save(&g, None, Some(&working)).unwrap();
        let rec = Record { kind: RecordKind::Committed, key: bind, state };
        assert_eq!(resolve_load(Some(&g), Some(&rec), None), Loaded::Committed(working));
    }

    #[test]
    fn a_new_game_in_the_slot_inherits_nothing() {
        // The old character's record belongs to its save. A new game starts from `fresh` (the
        // worker never loads a record for it); its FIRST save replaces the record.
        let mut old = AvatarState::fresh(3);
        old.grant_set(200, &ALL, 0, 0);
        let g_old = save(47);
        let rec_old = committed(&g_old, &old);
        let new = AvatarState::fresh(4);
        let g_new = save(1);
        let (bind, state) = commit_after_save(&g_new, None, Some(&new)).unwrap();
        assert_eq!(state.count(200), 0);
        assert_ne!(bind, rec_old.key);
        // And if the new game is never saved, the slot still holds the old save — which still
        // loads with the old record.
        assert_eq!(resolve_load(Some(&g_old), Some(&rec_old), None), Loaded::Committed(old));
    }

    #[test]
    fn a_save_the_record_does_not_match_loads_empty() {
        let mut s = AvatarState::fresh(5);
        s.grant(200, SLOT_ARMOUR, 0, 0);
        let rec = committed(&save(30), &s);
        // A vault generation from before the costume (older `/g`, no record of its own), a
        // save copied in from elsewhere, a pre-A2 save: nothing matches.
        assert!(matches!(resolve_load(Some(&save(29)), Some(&rec), None), Loaded::Legacy(_)));
        assert!(matches!(resolve_load(Some(&save(29)), None, None), Loaded::Legacy(_)));
        assert!(matches!(resolve_load(None, Some(&rec), None), Loaded::Legacy(_)));
    }

    #[test]
    fn the_journal_recovers_a_commit_the_crash_took() {
        let mut s1 = AvatarState::fresh(6);
        s1.grant(200, SLOT_ARMOUR, 0, 0);
        let g1 = save(40);
        let rec1 = committed(&g1, &s1);
        // Save 2 begins: the journal is written at the first edge with base = hash(g1)…
        let mut s2 = s1.clone();
        let k = s2.grant(201, SLOT_HEAD, 0, 0);
        s2.wear(SLOT_HEAD, Some(k));
        let jr = pending(fnv1a64(&g1), &s2);
        // …the game writes g2, and the process dies before the commit.
        let g2 = save(41);
        match resolve_load(Some(&g2), Some(&rec1), Some(&jr)) {
            Loaded::Recovered { state, bind } => {
                assert_eq!(state, s2);
                assert_eq!(bind, fnv1a64(&g2));
            }
            other => panic!("{other:?}"),
        }
        // Died BEFORE the game wrote g2: g1 is still on disk — the committed record wins.
        assert_eq!(resolve_load(Some(&g1), Some(&rec1), Some(&jr)), Loaded::Committed(s1.clone()));
        // Died while g2 was half written: not whole — the journal is not trusted.
        let half = &g2[..g2.len() - 3];
        assert!(matches!(resolve_load(Some(half), Some(&rec1), Some(&jr)), Loaded::Legacy(_)));
        // A journal from another chain (its base is not the committed save) is not ours.
        let stray = pending(fnv1a64(&save(7)), &s2);
        assert!(matches!(resolve_load(Some(&g2), Some(&rec1), Some(&stray)), Loaded::Legacy(_)));
        // The very first save of a slot: no committed record, the journal's base is the empty
        // slot (or a pre-A2 save).
        let first = pending(UNBOUND, &s2);
        assert!(matches!(resolve_load(Some(&g2), None, Some(&first)), Loaded::Recovered { .. }));
    }

    #[test]
    fn the_commit_after_a_save_takes_the_journal_or_the_worker() {
        let mut worker = AvatarState::fresh(7);
        worker.grant(200, SLOT_ARMOUR, 0, 0);
        let g = save(12);
        // Not whole yet: nothing.
        assert!(commit_after_save(&g[..g.len() - 1], None, Some(&worker)).is_none());
        // The journal is the state as of the save's first edge.
        let mut at_edge = worker.clone();
        at_edge.grant(202, SLOT_SHOES, 0, 0);
        let jr = pending(fnv1a64(&save(11)), &at_edge);
        assert_eq!(commit_after_save(&g, Some(&jr), Some(&worker)).unwrap().1, at_edge);
        // A journal whose base IS this `/g` (nothing changed) is not a save of it.
        let same = pending(fnv1a64(&g), &at_edge);
        assert_eq!(commit_after_save(&g, Some(&same), Some(&worker)).unwrap().1, worker);
        // No journal and no character in the world: nothing to commit.
        assert!(commit_after_save(&g, None, None).is_none());
    }

    #[test]
    fn the_worn_costume_asks_only_for_the_sheets_the_jar_has() {
        let has = |part: u8, n: i32| n == 200 && part != 2 && part != 7;
        let whole = [Some(200); SLOTS];
        assert_eq!(costume_parts(whole, has), [(200, -1), (-1, -1), (200, -1), (200, -1), (200, -1), (-1, -1)]);
        assert_eq!(costume_parts([Some(201); SLOTS], has), [(-1, -1); 6]);
        assert_eq!(costume_parts([None; SLOTS], has), [(-1, -1); 6]);
    }

    /// ★ 2026-10-01 (율리케의 웨딩드레스): a gown's armour item takes the legs — its skirt sheet
    /// `hero/3/<n + 1000>` — over the shoes slot; without one, the shoes slot keeps them.
    #[test]
    fn a_gown_draws_the_legs_over_the_shoes_slot() {
        // Set 201 = a gown (body, arms, a skirt, legs for its shoes); 200 = Ian's (no skirt).
        let has = |part: u8, n: i32| matches!((part, n), (1 | 2 | 3, 201) | (1 | 2 | 3 | 0, 200) | (3, 1201));
        let worn = |head, armour, shoes| [head, armour, shoes, None, None];
        // The gown with its own shoes, with Ian's boots, with none: the skirt each time.
        for shoes in [Some(201), Some(200), None] {
            assert_eq!(costume_parts(worn(None, Some(201), shoes), has)[LEGS_AT], (1201, -1), "{shoes:?}");
        }
        assert_eq!(costume_parts(worn(Some(200), Some(201), Some(200)), has), [(201, -1), (201, -1), (1201, -1), (200, -1), (-1, -1), (-1, -1)]);
        // No gown: the shoes slot's legs — the gown set's own shoes are legs like any.
        assert_eq!(costume_parts(worn(None, Some(200), Some(201)), has)[LEGS_AT], (201, -1));
        assert_eq!(costume_parts(worn(None, None, Some(201)), has)[LEGS_AT], (201, -1));
        // A set whose skirt the jar does not carry (an old asset) leaves the legs alone.
        let no_skirt = |part: u8, n: i32| has(part, n) && n != 1201;
        assert_eq!(costume_parts(worn(None, Some(201), None), no_skirt)[LEGS_AT], (-1, -1));
        assert_eq!(costume_parts(worn(None, Some(201), Some(200)), no_skirt)[LEGS_AT], (200, -1));
    }

    /// ★ 2026-10-09 (사용자: 앞이나 뒤를 덮지 않는 드레스는 신발 아바타를 바꾸면 보여야 한다): a gown whose skirt leaves the
    /// legs in sight (210's slit skirt) — the asset carries it over every OTHER set's legs, `hero/3/<dress × 1000 + shoes>`,
    /// so the shoes slot shows under it. Its own shoes, no shoes avatar, or a set the asset made no sheet for keep the skirt.
    #[test]
    fn a_layered_gown_shows_the_shoes_slot_under_it() {
        assert_eq!((skirt_over_shoes(210, 208), skirt_over_shoes(207, 210)), (210_208, 207_210));
        let has = |part: u8, n: i32| matches!((part, n), (0..=3, 210) | (0..=3, 208) | (0..=3, 200) | (3, 1210) | (3, 210_208));
        let worn = |armour, shoes| [None, armour, shoes, None, None];
        assert_eq!(costume_parts(worn(Some(210), Some(208)), has)[LEGS_AT], (210_208, -1), "208's legs under 210's skirt");
        assert_eq!(costume_parts(worn(Some(210), Some(208)), has)[..2], [(210, -1), (210, -1)], "body and arms stay the gown's");
        assert_eq!(costume_parts(worn(Some(210), Some(210)), has)[LEGS_AT], (1210, -1), "its own shoes = the skirt sheet");
        assert_eq!(costume_parts(worn(Some(210), None), has)[LEGS_AT], (1210, -1), "no shoes avatar = the skirt sheet");
        assert_eq!(costume_parts(worn(Some(210), Some(200)), has)[LEGS_AT], (1210, -1), "no sheet for that pair = the skirt sheet");
        // Without the skirt (an old asset) there is no gown: the shoes slot keeps its legs.
        let no_skirt = |part: u8, n: i32| has(part, n) && n != 1210;
        assert_eq!(costume_parts(worn(Some(210), Some(208)), no_skirt)[LEGS_AT], (208, -1));
        // Off with the gown: the shoes slot's own legs again.
        assert_eq!(costume_parts(worn(None, Some(208)), has)[LEGS_AT], (208, -1));
    }

    // ── ★ A3c: the one-shot grant (the boot marker) ───────────────────────────

    /// Ian's items for a knight or a rogue, as the catalog lists them (★ A4 five — the off-hand).
    const IAN: [(u16, u8); 5] = [(200, SLOT_HEAD), (200, SLOT_ARMOUR), (200, SLOT_SHOES), (200, SLOT_WEAPON), (200, SLOT_OFFHAND)];

    fn tmp_files(tag: &str) -> AvatarFiles {
        let dir = std::env::temp_dir().join(format!("h2_avatar_grant_{tag}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        AvatarFiles::new(dir)
    }

    /// ★ 2026-10-16 보스 레이드: the temple's gate reads the COMMITTED record bound to the save being loaded — a GUARDIAN HARD
    /// receipt there says yes; no record, a pending-only record, a NORMAL receipt, another save's bytes or another slot say no.
    #[test]
    fn the_raid_gate_reads_the_committed_record_bound_to_the_save() {
        let files = tmp_files("raid_gate");
        let g = b"the save as the game wrote it".to_vec();
        let mut s = AvatarState::fresh(7);
        assert!(!s.cleared_guardian_hard());
        assert!(!raid_gate_for_save(&files, 1, &g), "no record");
        s.receipt(CLEAR_REWARD_SOURCE_GUARDIAN_NORMAL, MODE_GUARDIAN, 0).unwrap();
        files.write_committed(1, fnv1a64(&g), &s).unwrap();
        assert!(!s.cleared_guardian_hard() && !raid_gate_for_save(&files, 1, &g), "a NORMAL receipt is not HARD");
        s.receipt(CLEAR_REWARD_SOURCE_GUARDIAN_HARD, MODE_GUARDIAN, 0).unwrap();
        assert!(s.cleared_guardian_hard());
        files.write_pending(1, fnv1a64(&g), &s).unwrap();
        assert!(!raid_gate_for_save(&files, 1, &g), "pending is not committed");
        files.write_committed(1, fnv1a64(&g), &s).unwrap();
        assert!(raid_gate_for_save(&files, 1, &g));
        assert!(!raid_gate_for_save(&files, 1, b"another save"), "bound to that save only");
        assert!(!raid_gate_for_save(&files, 2, &g), "slot 2 has no record");
        let _ = std::fs::remove_dir_all(&files.dir);
    }

    #[test]
    fn missing_items_are_granted_once_and_never_worn() {
        let mut s = AvatarState::fresh(1);
        s.grant(200, SLOT_HEAD, 1, 9);
        let added = s.grant_missing(&[IAN[0], IAN[1], IAN[1], IAN[3], (200, 7)], 0);
        assert_eq!(added, vec![IAN[1], IAN[3]], "the head is owned already; a repeat and a non-slot are skipped");
        assert_eq!(s.grant_missing(&IAN, 0), vec![IAN[2], IAN[4]]);
        assert!(s.grant_missing(&IAN, 0).is_empty(), "twice adds nothing");
        assert_eq!(s.items.len(), 5);
        assert_eq!(s.wear, [None; SLOTS], "ownership only");
        assert!(s.items.iter().filter(|i| i.receipt == 0).all(|i| i.mode == 0));
    }

    #[test]
    fn the_grant_goes_into_the_slot_record_bound_to_its_save() {
        let files = tmp_files("record");
        let g = save(48);
        // No save in the slot: nothing is written.
        assert!(grant_into_record(&files, 1, None, &IAN, 5).is_err());
        assert!(!files.committed_path(1).exists());
        // A save with no record (every save before costumes): a costume record is made for it.
        let got = grant_into_record(&files, 1, Some(&g), &IAN, 5).expect("granted");
        assert_eq!((got.added.len(), got.owned), (5, 5));
        let rec = files.read(1, &mut Vec::new()).0.expect("committed");
        assert_eq!(rec.key, fnv1a64(&g), "bound to the save on disk — the next 이어하기 finds it");
        match resolve_load(Some(&g), Some(&rec), None) {
            Loaded::Committed(s) => assert_eq!((s.items.len(), s.worn_avatars()), (5, [None; SLOTS])),
            other => panic!("{other:?}"),
        }
        // Again: nothing added, the record the same character.
        let again = grant_into_record(&files, 1, Some(&g), &IAN, 6).unwrap();
        assert!(again.added.is_empty());
        let rec2 = files.read(1, &mut Vec::new()).0.unwrap();
        assert_eq!(rec2.state, rec.state);
        // A character that already wears something keeps it and gains only what it lacks.
        let mut worn = AvatarState::fresh(77);
        let head = worn.grant(200, SLOT_HEAD, 2, 3);
        worn.wear(SLOT_HEAD, Some(head));
        files.write_committed(2, fnv1a64(&g), &worn).unwrap();
        let got = grant_into_record(&files, 2, Some(&g), &IAN, 1).unwrap();
        assert_eq!(got.added, IAN[1..].to_vec());
        let s = files.read(2, &mut Vec::new()).0.unwrap().state;
        assert_eq!((s.character, s.worn_avatars()), (77, [Some(200), None, None, None, None]));
        // A journal the crash left is the character (as the recovery would take it), then gone.
        let mut j = AvatarState::fresh(88);
        j.grant(201, SLOT_ARMOUR, 0, 0);
        files.write_pending(3, UNBOUND, &j).unwrap();
        grant_into_record(&files, 3, Some(&g), &IAN, 1).unwrap();
        let s = files.read(3, &mut Vec::new()).0.unwrap().state;
        assert_eq!((s.character, s.count(201), s.count(200)), (88, 1, 5));
        assert!(!files.pending_path(3).exists());
        // An unreadable record is not overwritten — one that does not decode, and one whose bytes
        // are not even UTF-8 (a corrupt record must not read as "no record").
        std::fs::write(files.committed_path(4), "garbage").unwrap();
        assert!(grant_into_record(&files, 4, Some(&g), &IAN, 1).is_err());
        assert_eq!(std::fs::read_to_string(files.committed_path(4)).unwrap(), "garbage");
        let mut bad = encode_record(RecordKind::Committed, fnv1a64(&g), &worn).into_bytes();
        bad[20] = 0xff;
        std::fs::write(files.committed_path(5), &bad).unwrap();
        let mut errors = Vec::new();
        assert!(files.read(5, &mut errors).0.is_none() && errors.len() == 1, "a record that is there but unreadable is an error");
        assert!(grant_into_record(&files, 5, Some(&g), &IAN, 1).is_err());
        assert_eq!(std::fs::read(files.committed_path(5)).unwrap(), bad, "…and is left alone");
        let _ = std::fs::remove_dir_all(&files.dir);
    }

    #[test]
    fn a_grant_marker_is_read_however_it_was_written() {
        assert_eq!(grant_marker_slots(b"1", 5), vec![1]);
        assert_eq!(grant_marker_slots(b"1, 3 9\r\n", 5), vec![1, 3], "9 is not a slot");
        assert_eq!(grant_marker_slots(b"3 1 1", 5), vec![1, 3], "ascending, once each");
        assert_eq!(grant_marker_slots(b"\xef\xbb\xbf1\r\n", 5), vec![1], "a UTF-8 BOM");
        assert_eq!(grant_marker_slots(&[0xff, 0xfe, b'1', 0, b'\r', 0, b'\n', 0], 5), vec![1], "PowerShell 5.1's `>`: UTF-16LE");
        assert_eq!(grant_marker_slots(&[0xfe, 0xff, 0, b'2', 0, b' ', 0, b'4'], 5), vec![2, 4], "UTF-16BE");
        assert_eq!(grant_marker_slots(b"slot=1;slot=12", 5), vec![1], "12 is one number, not a slot");
        assert!(grant_marker_slots(b"one", 5).is_empty() && grant_marker_slots(b"0", 5).is_empty() && grant_marker_slots(b"", 5).is_empty());
        // ★ A5: costume set numbers (200..=999) beside the slots — never read as slots.
        assert_eq!(grant_marker_slots(b"1 2 3 4 5 202 203 204", 5), vec![1, 2, 3, 4, 5]);
        assert_eq!(grant_marker_sets(b"1 2 3 4 5 204 202 203 202"), vec![202, 203, 204], "ascending, once each");
        assert!(grant_marker_sets(b"1, 3 9").is_empty(), "no set named = every set");
        assert!(grant_marker_sets(b"199 1000").is_empty(), "outside the costume numbers");
        assert_eq!(grant_marker_sets(&[0xff, 0xfe, b'1', 0, b' ', 0, b'2', 0, b'0', 0, b'3', 0]), vec![203], "UTF-16LE too");
    }

    #[test]
    fn a_grant_marker_run_writes_only_when_it_has_slots_and_items() {
        let files = tmp_files("run");
        let g = save(33);
        let live = |slot: u8| if slot == 1 { Some(g.clone()) } else { None };
        let ian_for = |class: Option<u8>| if class == Some(0) { IAN[..4].to_vec() } else { IAN.to_vec() };
        let lines = grant_marker_run(&files, b"x", 5, &ian_for, &live, 1);
        assert!(lines[0].contains("names no save slot") && !files.committed_path(1).exists());
        let lines = grant_marker_run(&files, b"1", 5, &|_| Vec::new(), &live, 1);
        assert!(lines[0].contains("makes no avatar item") && !files.committed_path(1).exists(), "an asset without items writes nothing");
        let lines = grant_marker_run(&files, b"1 2", 5, &ian_for, &live, 1);
        assert_eq!(lines.len(), 2);
        assert!(lines[0].contains("slot 1 (knight)") && lines[0].contains("gained 5"), "{lines:?}");
        assert!(lines[1].contains("no save"), "{lines:?}");
        assert!(files.committed_path(1).exists() && !files.committed_path(2).exists());
        let _ = std::fs::remove_dir_all(&files.dir);
    }

    /// ★ A5: a marker naming sets gives only those sets' items — and names a set the asset lacks → nothing.
    #[test]
    fn a_grant_marker_with_sets_gives_only_those_sets() {
        let files = tmp_files("runsets");
        let g = save(33);
        let live = |slot: u8| if slot == 1 { Some(g.clone()) } else { None };
        let all = |_: Option<u8>| vec![(200u16, SLOT_HEAD), (200, SLOT_ARMOUR), (202, SLOT_HEAD), (202, SLOT_ARMOUR), (203, SLOT_HEAD), (204, SLOT_WEAPON)];
        let lines = grant_marker_run(&files, b"1 777", 5, &all, &live, 1);
        assert!(lines[0].contains("no item of the named set(s) [777]") && !files.committed_path(1).exists(), "{lines:?}");
        let lines = grant_marker_run(&files, b"1 202 203 204", 5, &all, &live, 1);
        assert!(lines[0].contains("gained 4 item(s) of set(s) [202, 203, 204]"), "{lines:?}");
        let rec = decode_record(&std::fs::read_to_string(files.committed_path(1)).unwrap()).unwrap();
        let mut got: Vec<(u16, u8)> = rec.state.items.iter().map(|i| (i.avatar, i.slot)).collect();
        got.sort_unstable();
        assert_eq!(got, vec![(202, SLOT_HEAD), (202, SLOT_ARMOUR), (203, SLOT_HEAD), (204, SLOT_WEAPON)], "no 200 — it was not named");
        // Without sets, the rest of the asset (what the character lacks).
        let lines = grant_marker_run(&files, b"1", 5, &all, &live, 1);
        assert!(lines[0].contains("gained 2 item(s) [(200, 0), (200, 1)]"), "{lines:?}");
        let _ = std::fs::remove_dir_all(&files.dir);
    }

    // ── the book, end to end, over a fake host ────────────────────────────────

    use std::cell::RefCell;
    use std::collections::HashMap;

    /// The disk (a temp dir for the records + the `/g` files in memory) and the vault's two
    /// hooks (edges, journal) the way `vault::before_overwrite` runs them.
    struct FakeIo {
        dir: std::path::PathBuf,
        g: RefCell<HashMap<u8, Vec<u8>>>,
        edges: RefCell<[u64; 6]>,
        working: RefCell<Option<AvatarState>>,
        armed: RefCell<[bool; 6]>,
        notes: RefCell<Vec<String>>,
    }

    impl FakeIo {
        fn new(tag: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("h2_avatar_book_{tag}_{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            Self::on(dir, HashMap::new())
        }
        /// A new process over the same disk.
        fn restart(&self) -> Self {
            Self::on(self.dir.clone(), self.g.borrow().clone())
        }
        fn on(dir: std::path::PathBuf, g: HashMap<u8, Vec<u8>>) -> Self {
            Self { dir, g: RefCell::new(g), edges: RefCell::new([0; 6]), working: RefCell::new(None), armed: RefCell::new([true; 6]), notes: RefCell::new(Vec::new()) }
        }
        /// `ag.l()V`: the first `/g` edge (journal while `/g` is still the old save), the write.
        fn game_saves(&self, slot: u8, new_g: &[u8]) {
            let armed = std::mem::replace(&mut self.armed.borrow_mut()[usize::from(slot)], false);
            if armed {
                if let Some(w) = self.working.borrow().as_ref() {
                    let cur = self.g.borrow().get(&slot).cloned();
                    write_journal(&AvatarFiles::new(self.dir.clone()), slot, cur.as_deref(), w).unwrap();
                }
            }
            self.g.borrow_mut().insert(slot, new_g.to_vec());
            self.edges.borrow_mut()[usize::from(slot)] += 2; // truncate + write
        }
        fn record(&self, slot: u8) -> Option<Record> {
            AvatarFiles::new(self.dir.clone()).read(slot, &mut Vec::new()).0
        }
        fn pending_exists(&self, slot: u8) -> bool {
            AvatarFiles::new(self.dir.clone()).pending_path(slot).exists()
        }
    }

    impl AvatarIo for FakeIo {
        fn live_g(&self, slot: u8) -> Option<Vec<u8>> {
            self.g.borrow().get(&slot).cloned()
        }
        fn files(&self) -> Option<AvatarFiles> {
            Some(AvatarFiles::new(self.dir.clone()))
        }
        fn g_edges(&self, slot: u8) -> u64 {
            self.edges.borrow()[usize::from(slot)]
        }
        fn publish(&self, working: Option<&AvatarState>) {
            *self.working.borrow_mut() = working.cloned();
            *self.armed.borrow_mut() = [true; 6];
        }
        fn rearm(&self, slot: u8) {
            self.armed.borrow_mut()[usize::from(slot)] = true;
        }
        fn note(&self, _warn: bool, line: String) {
            self.notes.borrow_mut().push(line);
        }
    }

    #[test]
    fn the_book_saves_rolls_back_and_starts_new_characters_clean() {
        let io = FakeIo::new("flow");
        io.g.borrow_mut().insert(1, save(47)); // a save from before costumes
        let mut book = AvatarBook::new(&io);
        // 이어하기 on a pre-costume save: nothing, and nothing written.
        book.enter_world(&io, 1, false, 11);
        assert_eq!(book.worn(), [None; SLOTS]);
        assert!(io.record(1).is_none());
        // Gain and wear set 200, SAVE.
        book.grant_and_wear(&io, 200, &ALL, 0).unwrap();
        assert_eq!(book.worn(), [Some(200); SLOTS]);
        io.game_saves(1, &save(48));
        book.poll(&io);
        let rec = io.record(1).expect("committed");
        assert_eq!(rec.key, fnv1a64(&save(48)));
        assert_eq!(rec.state.worn_avatars(), [Some(200); SLOTS]);
        assert!(!io.pending_exists(1), "the journal is gone once committed");
        // Gain and wear 201's clothes, do NOT save, die → title → 이어하기.
        book.grant_and_wear(&io, 201, &[SLOT_ARMOUR], 0).unwrap();
        assert_eq!(book.worn(), [Some(200), Some(201), Some(200), Some(200), Some(200)], "one slot changed, the rest kept");
        book.leave_world(&io);
        book.enter_world(&io, 1, false, 12);
        let (_, s) = book.working.clone().unwrap();
        assert_eq!((s.count(200), s.count(201), s.worn_avatars()), (5, 0, [Some(200); SLOTS]), "what was not saved is gone");
        // 새로하기 in the same slot: owns nothing. Quit without saving → the old character loads
        // with its costume.
        book.leave_world(&io);
        book.enter_world(&io, 1, true, 13);
        assert_eq!(book.worn(), [None; SLOTS]);
        let new_id = book.working.as_ref().unwrap().1.character;
        book.leave_world(&io);
        book.enter_world(&io, 1, false, 14);
        assert_eq!(book.worn(), [Some(200); SLOTS], "an unsaved new game did not touch the slot");
        let old_id = book.working.as_ref().unwrap().1.character;
        assert_ne!(new_id, old_id);
        // 새로하기 again, and this time SAVE: the new character's empty record replaces the old.
        book.leave_world(&io);
        book.enter_world(&io, 1, true, 15);
        io.game_saves(1, &save(1));
        book.poll(&io);
        book.leave_world(&io);
        book.enter_world(&io, 1, false, 16);
        let (_, s) = book.working.clone().unwrap();
        assert_eq!((s.items.len(), s.worn_avatars()), (0, [None; SLOTS]), "no inheritance");
        assert_ne!(s.character, old_id);
        let _ = std::fs::remove_dir_all(&io.dir);
    }

    /// ★ 2026-10-06 「심안」: the one-time 독 연마 refund's receipt — unpaid on a save from before the build, paid once
    /// marked, kept by the save that carries it (and a restart), gone with a reload that did not save; nobody in the
    /// world = no answer (the refund waits).
    /// ★★ 2026-10-22 가디언 레벨: the book follows the block's GL; a save keeps it, a death and reload without saving
    /// hands back the GL of the save (with the points the same routine gave, which ride the save itself).
    #[test]
    fn the_guardian_level_is_kept_only_with_a_save() {
        let io = FakeIo::new("gl");
        io.g.borrow_mut().insert(2, save(10));
        let mut book = AvatarBook::new(&io);
        assert_eq!(book.guardian_level(), None, "nobody in the world");
        book.enter_world(&io, 2, false, 21);
        assert_eq!(book.guardian_level(), Some(0), "a record from before GL");
        assert!(book.set_guardian_level(&io, 1));
        assert!(!book.set_guardian_level(&io, 1), "nothing to change");
        assert!(book.set_guardian_level(&io, 4));
        io.game_saves(2, &save(11));
        book.poll(&io);
        assert_eq!(io.record(2).unwrap().state.gl, 4, "committed with the save");
        assert!(book.set_guardian_level(&io, 9));
        book.leave_world(&io);
        assert!(!book.set_guardian_level(&io, 10), "nobody in the world");
        book.enter_world(&io, 2, false, 22);
        assert_eq!(book.guardian_level(), Some(4), "not saved: the save's GL");
        let io2 = io.restart();
        let mut book2 = AvatarBook::new(&io2);
        book2.enter_world(&io2, 2, false, 23);
        assert_eq!(book2.guardian_level(), Some(4), "a new process reads it back");
    }

    /// ★★ 2026-10-22 가디언 레벨 — review P1: a GL-up in the same tick as a save. The journal is written from the last
    /// PUBLISHED state (GL 4), the save holds GL 5's stat points; the guest's `/g` writer snapshotted GL 5 and the commit
    /// takes it — a reload then hands back GL 5 with its points, not GL 4.
    #[test]
    fn a_save_made_right_after_a_gl_up_keeps_that_gl() {
        let io = FakeIo::new("glsave");
        io.g.borrow_mut().insert(3, save(20));
        let mut book = AvatarBook::new(&io);
        book.enter_world(&io, 3, false, 31);
        book.set_guardian_level(&io, 4);
        // the tick: GL 4 → 5 in the guest, then the quick save (the journal sees GL 4) — before the host syncs
        io.game_saves(3, &save(21));
        assert_eq!(AvatarFiles::new(io.dir.clone()).read(3, &mut Vec::new()).1.map(|r| r.state.gl), Some(4), "the journal holds the published GL");
        book.note_save_gl(&io, 5);
        book.poll(&io);
        assert_eq!(io.record(3).unwrap().state.gl, 5, "the commit takes the GL the save was made at");
        book.leave_world(&io);
        book.enter_world(&io, 3, false, 32);
        assert_eq!(book.guardian_level(), Some(5));
        // no snapshot (an older block, a save while not armed): the commit is the state as before
        book.set_guardian_level(&io, 6);
        io.game_saves(3, &save(22));
        book.poll(&io);
        assert_eq!(io.record(3).unwrap().state.gl, 6);
        // a snapshot belongs to the world it was taken in
        book.note_save_gl(&io, 9);
        book.leave_world(&io);
        book.enter_world(&io, 3, false, 33);
        book.set_guardian_level(&io, 6);
        io.game_saves(3, &save(23));
        book.poll(&io);
        assert_eq!(io.record(3).unwrap().state.gl, 6, "a snapshot from before the reload is not used");
    }

    /// ★ 2026-10-28 code review P1: a commit that does not land is NOT 「caught up」. The save was made at GL 11, the
    /// journal holds the published GL 10, the record's write fails — 세이브내보내기 must not take that disk (it would carry
    /// GL 10 for a save of GL 11's points). A save that commits clears it; so does a load that recovers the slot.
    #[test]
    fn a_commit_that_did_not_land_holds_the_export() {
        let io = FakeIo::new("caught");
        io.g.borrow_mut().insert(2, save(30));
        let mut book = AvatarBook::new(&io);
        book.enter_world(&io, 2, false, 41);
        assert_eq!(book.caught_up(&io), Some(true), "nothing saved, nothing owed");
        book.set_guardian_level(&io, 10);
        io.game_saves(2, &save(31));
        book.note_save_gl(&io, 11);
        assert_eq!(book.caught_up(&io), Some(false), "an edge not polled yet");
        // The record cannot be written: a directory stands where its temp file goes.
        let obstacle = AvatarFiles::new(io.dir.clone()).committed_path(2).with_file_name(".2.tmp");
        std::fs::create_dir_all(&obstacle).unwrap();
        book.poll(&io);
        assert_eq!(book.caught_up(&io), None, "the commit failed — the export is refused");
        assert_eq!(AvatarFiles::new(io.dir.clone()).read(2, &mut Vec::new()).1.map(|r| r.state.gl), Some(10), "the journal holds GL 10");
        // Back at the title: still the same disk, still refused.
        book.leave_world(&io);
        assert_eq!(book.caught_up(&io), None);
        book.enter_world(&io, 2, false, 42);
        assert_eq!(book.caught_up(&io), None, "the load could not recover it either");
        // The next save of the slot is judged on its own: pending first, then landed.
        std::fs::remove_dir_all(&obstacle).unwrap();
        io.game_saves(2, &save(32));
        book.note_save_gl(&io, 11);
        assert_eq!(book.caught_up(&io), Some(false), "the failure from before is not this save's");
        book.poll(&io);
        assert_eq!(book.caught_up(&io), Some(true));
        assert_eq!(io.record(2).unwrap().state.gl, 11);
        // A failed commit, then a load that recovers the slot from its journal: one moment again.
        std::fs::create_dir_all(&obstacle).unwrap();
        io.game_saves(2, &save(33));
        book.poll(&io);
        assert_eq!(book.caught_up(&io), None);
        std::fs::remove_dir_all(&obstacle).unwrap();
        book.leave_world(&io);
        book.enter_world(&io, 2, false, 43);
        assert_eq!(io.record(2).unwrap().key, fnv1a64(&save(33)), "recovered and committed");
        assert_eq!(book.caught_up(&io), Some(true));
        let _ = std::fs::remove_dir_all(&io.dir);
    }

    #[test]
    fn the_sight_refund_is_paid_once_and_rides_the_save() {
        let io = FakeIo::new("sight");
        io.g.borrow_mut().insert(1, save(47));
        let mut book = AvatarBook::new(&io);
        assert_eq!(book.sight_refunded(), None, "nobody in the world");
        assert!(!book.mark_sight_refunded(&io, 3));
        book.enter_world(&io, 1, false, 1);
        assert_eq!(book.sight_refunded(), Some(false), "a save from before the build");
        assert!(book.mark_sight_refunded(&io, 3));
        assert_eq!(book.sight_refunded(), Some(true));
        assert!(!book.mark_sight_refunded(&io, 3), "once");
        assert!(io.record(1).is_none(), "not committed by itself");
        // Not saved: a reload takes it back (the game reloads the character without the refund too).
        book.leave_world(&io);
        book.enter_world(&io, 1, false, 2);
        assert_eq!(book.sight_refunded(), Some(false));
        // Marked and SAVED: the record keeps it, across a restart too.
        assert!(book.mark_sight_refunded(&io, 3));
        io.game_saves(1, &save(48));
        book.poll(&io);
        assert!(io.record(1).expect("committed").state.paid(RECEIPT_SIGHT_REFUND, 0, 0));
        let io2 = io.restart();
        let mut book2 = AvatarBook::new(&io2);
        book2.enter_world(&io2, 1, false, 3);
        assert_eq!(book2.sight_refunded(), Some(true), "paid, after a restart");
        // A new game has no receipt yet (its −1 refunds nothing, and the first repair records it).
        book2.leave_world(&io2);
        book2.enter_world(&io2, 1, true, 4);
        assert_eq!(book2.sight_refunded(), Some(false));
        let _ = std::fs::remove_dir_all(&io.dir);
    }

    #[test]
    fn the_book_wears_item_by_item() {
        let io = FakeIo::new("items");
        io.g.borrow_mut().insert(2, save(30));
        let mut book = AvatarBook::new(&io);
        book.enter_world(&io, 2, false, 1);
        let ian = book.give(&io, 200, &ALL, 0).unwrap();
        assert_eq!(book.worn(), [None; SLOTS], "given, not worn");
        assert!(!book.wear_instance(&io, SLOT_HEAD, Some(ian[1])), "the clothes are not a head");
        assert!(book.wear_instance(&io, SLOT_WEAPON, Some(ian[3])));
        assert_eq!(book.worn(), [None, None, None, Some(200), None]);
        assert!(io.working.borrow().as_ref().is_some_and(|w| w.worn_avatars() == [None, None, None, Some(200), None]), "published for the journal");
        assert!(book.wear_avatar(&io, Some(200)));
        assert_eq!(book.worn(), [Some(200); SLOTS]);
        assert!(!book.wear_avatar(&io, Some(999)), "nothing of that set");
        assert!(book.wear_avatar(&io, None));
        assert_eq!(book.worn(), [None; SLOTS]);
        book.leave_world(&io);
        assert!(!book.wear_instance(&io, SLOT_HEAD, Some(ian[0])), "nobody in the world");
        // The grant written before the world is entered (the boot marker) is what 이어하기 finds.
        grant_into_record(&io.files().unwrap(), 2, io.live_g(2).as_deref(), &IAN, 3).unwrap();
        book.enter_world(&io, 2, false, 2);
        assert_eq!(book.state().map(|s| s.items.len()), Some(5));
        let _ = std::fs::remove_dir_all(&io.dir);
    }

    /// ★ A4: a clear pays its mode's set once per character — not worn, kept only if saved.
    #[test]
    fn a_clear_pays_its_reward_once_and_only_a_save_keeps_it() {
        let io = FakeIo::new("clear");
        io.g.borrow_mut().insert(1, save(50));
        let mut book = AvatarBook::new(&io);
        assert_eq!(book.clear_rewards(&io, 0, &|_| ALL.to_vec()), vec![ClearOutcome::NoCharacter]);
        book.enter_world(&io, 1, false, 1);
        // A knight: the whole set, five items — a berserker's grant would name four slots.
        let knight = |set: u16| if set == 200 { ALL.to_vec() } else { vec![SLOT_HEAD, SLOT_ARMOUR, SLOT_SHOES] };
        match book.clear_rewards(&io, 0, &knight).remove(0) {
            ClearOutcome::Paid { reward, ids, slots } => {
                assert_eq!((reward.set, ids.len(), slots), (200, 5, ALL.to_vec()));
            }
            other => panic!("{other:?}"),
        }
        let s = book.state().unwrap().clone();
        assert_eq!((s.count(200), s.worn_avatars()), (5, [None; SLOTS]), "owned, not worn");
        assert!(s.items.iter().all(|i| i.mode == 0 && i.receipt != 0), "gained in NORMAL, paid by a receipt");
        assert_eq!(s.receipts.iter().map(|r| (r.source, r.mode)).collect::<Vec<_>>(), vec![(CLEAR_REWARD_SOURCE, 0)]);
        assert_eq!(book.clear_rewards(&io, 0, &knight), vec![ClearOutcome::AlreadyPaid { reward: CLEAR_REWARDS[0] }], "the same mode twice pays once");
        // The asset makes nothing of a set for this class: nothing paid, no receipt spent.
        assert_eq!(book.clear_rewards(&io, 1, &|_| Vec::new()), vec![ClearOutcome::NothingToGive { reward: CLEAR_REWARDS[1] }]);
        assert_eq!(book.state().unwrap().receipts.len(), 1);
        match book.clear_rewards(&io, 1, &knight).remove(0) {
            ClearOutcome::Paid { reward, slots, .. } => assert_eq!((reward.set, slots), (201, vec![SLOT_HEAD, SLOT_ARMOUR, SLOT_SHOES]), "HARDCORE: 율리케's three"),
            other => panic!("{other:?}"),
        }
        // ★ HELL pays 할로윈 at any 난이도; 수영장 파티 asks for NORMAL on quests this round never did.
        let hell = book.clear_rewards(&io, 2, &knight);
        assert!(matches!(&hell[0], ClearOutcome::Paid { reward, .. } if reward.set == 204), "{hell:?}");
        assert_eq!(hell[1], ClearOutcome::Short { reward: CLEAR_REWARDS[3], done_at: 0, done: 0 });
        assert_eq!(CLEAR_REWARDS[3].set, 203);
        assert!(io.working.borrow().as_ref().is_some_and(|w| w.count(201) == 3), "published for the journal");
        // Not saved: a death and a reload hand back the save — no items, no receipts, so the next
        // clear of NORMAL (that save replayed) pays again.
        book.leave_world(&io);
        book.enter_world(&io, 1, false, 2);
        assert_eq!(book.state().map(|s| (s.items.len(), s.receipts.len())), Some((0, 0)));
        assert!(matches!(book.clear_rewards(&io, 0, &knight)[..], [ClearOutcome::Paid { .. }]));
        // Saved: it stays, and so does its receipt.
        io.game_saves(1, &save(51));
        book.poll(&io);
        book.leave_world(&io);
        book.enter_world(&io, 1, false, 3);
        assert_eq!(book.state().map(|s| (s.count(200), s.receipts.len())), Some((5, 1)));
        assert_eq!(book.clear_rewards(&io, 0, &knight), vec![ClearOutcome::AlreadyPaid { reward: CLEAR_REWARDS[0] }]);
        let _ = std::fs::remove_dir_all(&io.dir);
    }

    /// ★ A5 · ★ 2026-10-02: a HELL round played at EASY (switched to NORMAL for the end) pays only 할로윈; HELL
    /// repeats, and a round played at NORMAL — or HARD — then pays 수영장 파티; the marks start over at every clear.
    #[test]
    fn hell_pays_the_pool_party_only_for_a_round_played_at_normal() {
        let io = FakeIo::new("hell");
        io.g.borrow_mut().insert(1, save(50));
        let mut book = AvatarBook::new(&io);
        book.enter_world(&io, 1, true, 1);
        let all = |_: u16| ALL.to_vec();
        let mut done = [false; QUESTS];
        for q in done.iter_mut().take(19) {
            *q = true;
        }
        for k in 1..=19 {
            let mut part = [false; QUESTS];
            part[..k].copy_from_slice(&done[..k]);
            book.observe_quests(&io, &part, 0);
        }
        done[19] = true;
        book.observe_quests(&io, &done, 1);
        let out = book.clear_rewards(&io, 2, &all);
        assert!(matches!(&out[0], ClearOutcome::Paid { reward, .. } if reward.set == 204), "{out:?}");
        assert_eq!(out[1], ClearOutcome::Short { reward: CLEAR_REWARDS[3], done_at: 1, done: 20 }, "EASY all round, NORMAL for the last quest");
        assert_eq!(book.state().unwrap().run_progress(0), (0, 0), "the next round starts over");
        // HELL again: one quest at EASY, nineteen at NORMAL — 95 %.
        let mut done = [false; QUESTS];
        done[0] = true;
        book.observe_quests(&io, &done, 0);
        for q in 1..20 {
            done[q] = true;
            book.observe_quests(&io, &done, 1);
        }
        let out = book.clear_rewards(&io, 2, &all);
        assert_eq!(out[0], ClearOutcome::AlreadyPaid { reward: CLEAR_REWARDS[2] });
        assert!(matches!(&out[1], ClearOutcome::Paid { reward, slots, .. } if reward.set == 203 && slots.len() == 5), "{out:?}");
        let s = book.state().unwrap();
        assert_eq!((s.count(203), s.count(204), s.worn_avatars()), (5, 5, [None; SLOTS]), "owned, none worn");
        let keys: Vec<(u32, u8)> = s.receipts.iter().map(|r| (r.source, r.mode)).collect();
        assert_eq!(keys, vec![(CLEAR_REWARD_SOURCE_HALLOWEEN, 2), (CLEAR_REWARD_SOURCE, 2)]);
        // A fresh character whose HELL round was all HARD gets both at once.
        book.leave_world(&io);
        book.enter_world(&io, 1, true, 2);
        let mut done = [false; QUESTS];
        for q in 0..10 {
            done[q] = true;
            book.observe_quests(&io, &done, 2);
        }
        let out = book.clear_rewards(&io, 2, &all);
        assert!(out.iter().all(|o| matches!(o, ClearOutcome::Paid { .. })), "{out:?}");
        // ★ 2026-10-09: all FOUR — 백은의 사신 210 is HARD's too.
        assert_eq!(out.len(), 4);
        let s = book.state().unwrap();
        assert_eq!([204, 203, 205, 210].map(|set| s.count(set)), [5; 4]);
        let _ = std::fs::remove_dir_all(&io.dir);
    }

    /// Marks for `n` quests done at `difficulty` this round (the first look after an entry is a baseline — nothing yet).
    fn play_round(book: &mut AvatarBook, io: &FakeIo, n: usize, difficulty: u8) {
        book.observe_quests(io, &[false; QUESTS], difficulty);
        let mut done = [false; QUESTS];
        for q in 0..n {
            done[q] = true;
            book.observe_quests(io, &done, difficulty);
        }
    }

    fn sets_of(out: &[ClearOutcome]) -> Vec<(u16, &'static str)> {
        out.iter()
            .map(|o| match o {
                ClearOutcome::Paid { reward, .. } => (reward.set, "paid"),
                ClearOutcome::AlreadyPaid { reward } => (reward.set, "had"),
                ClearOutcome::Short { reward, .. } => (reward.set, "short"),
                ClearOutcome::NothingToGive { reward } => (reward.set, "nothing"),
                ClearOutcome::NoReward => (0, "none"),
                ClearOutcome::NoCharacter => (0, "nobody"),
            })
            .collect()
    }

    /// ★ 2026-10-09 (user: *"가디언 이지클리어하면 헬모드 하드아바타 해금조건까지 클리어한거나 마찬가지인거야"*): a character whose HELL
    /// round was played at EASY (할로윈 only) clears GUARDIAN at EASY — its own EASY sets, and with them every HELL set it did
    /// not have, whatever difficulty they ask for, under HELL's receipts. A second GUARDIAN clear at HARD pays GUARDIAN's
    /// NORMAL and HARD sets and nothing of HELL again.
    #[test]
    fn a_guardian_clear_is_a_hell_clear_at_hard() {
        let io = FakeIo::new("guardian");
        io.g.borrow_mut().insert(1, save(70));
        let mut book = AvatarBook::new(&io);
        let all = |_: u16| ALL.to_vec();
        book.enter_world(&io, 1, true, 1);
        play_round(&mut book, &io, 20, 0);
        assert_eq!(sets_of(&book.clear_rewards(&io, 2, &all)), vec![(204, "paid"), (203, "short"), (205, "short"), (210, "short")], "HELL at EASY");
        // GUARDIAN at EASY.
        play_round(&mut book, &io, 20, 0);
        let out = book.clear_rewards(&io, MODE_GUARDIAN, &all);
        assert_eq!(
            sets_of(&out),
            vec![(206, "paid"), (207, "paid"), (208, "short"), (209, "short"), (204, "had"), (203, "paid"), (205, "paid"), (210, "paid")],
            "GUARDIAN's own first, then HELL's at HARD"
        );
        let notices: Vec<u32> = out.iter().filter_map(|o| if let ClearOutcome::Paid { reward, .. } = o { Some(reward.notice_row) } else { None }).collect();
        assert_eq!(notices, vec![70, 71, 67, 69, 74], "one notice per set paid");
        let s = book.state().unwrap().clone();
        let mut keys: Vec<(u32, u8)> = s.receipts.iter().map(|r| (r.source, r.mode)).collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            vec![
                (CLEAR_REWARD_SOURCE, 2),
                (CLEAR_REWARD_SOURCE_HALLOWEEN, 2),
                (CLEAR_REWARD_SOURCE_HALLOWEEN_PARTY, 2),
                (CLEAR_REWARD_SOURCE_UNIFORM, MODE_GUARDIAN),
                (CLEAR_REWARD_SOURCE_GUARDIAN_EASY, MODE_GUARDIAN),
                (CLEAR_REWARD_SOURCE_HELL_HARD, 2),
            ],
            "the HELL sets under HELL's own receipts"
        );
        assert!(s.cleared_guardian());
        assert_eq!(s.worn_avatars(), [None; SLOTS], "none worn");
        // A HELL clear now (were the game to play one) has nothing left to pay.
        assert!(book.clear_rewards(&io, 2, &all).iter().all(|o| matches!(o, ClearOutcome::AlreadyPaid { .. })));
        // GUARDIAN again, at HARD: NORMAL's and HARD's sets.
        play_round(&mut book, &io, 20, 2);
        assert_eq!(
            sets_of(&book.clear_rewards(&io, MODE_GUARDIAN, &all)),
            vec![(206, "had"), (207, "had"), (208, "paid"), (209, "paid"), (204, "had"), (203, "had"), (205, "had"), (210, "had")]
        );
        // A fresh character whose GUARDIAN round was played at NORMAL: 208 but not 209 — and every HELL set.
        book.leave_world(&io);
        book.enter_world(&io, 1, true, 2);
        play_round(&mut book, &io, 20, 1);
        assert_eq!(
            sets_of(&book.clear_rewards(&io, MODE_GUARDIAN, &all)),
            vec![(206, "paid"), (207, "paid"), (208, "paid"), (209, "short"), (204, "paid"), (203, "paid"), (205, "paid"), (210, "paid")]
        );
        let _ = std::fs::remove_dir_all(&io.dir);
    }

    /// ★ 2026-10-18 보스 레이드 (user: *"보스레이드 해금도 95%이상 hard 로 진행해야 해금이 되야되는게 맞아"*): the temple's gate
    /// opens with 달빛 엘프 209's receipt only — 95 % of the round at HARD. A GUARDIAN round played at EASY and cleared with
    /// HARD set at the very end opens nothing; the same round played at HARD does, once saved.
    #[test]
    fn the_raid_gate_asks_for_95_percent_of_the_round_at_hard() {
        let io = FakeIo::new("raid_95");
        io.g.borrow_mut().insert(1, save(80));
        let mut book = AvatarBook::new(&io);
        let all = |_: u16| ALL.to_vec();
        book.enter_world(&io, 1, true, 1);
        play_round(&mut book, &io, 20, 0);
        book.observe_quests(&io, &[true; QUESTS], 2); // the last quests at HARD: 31 of 51
        assert!(sets_of(&book.clear_rewards(&io, MODE_GUARDIAN, &all)).contains(&(209, "short")));
        assert!(!book.cleared_guardian_hard(), "a HARD finish of a round played lower is not a HARD clear");
        play_round(&mut book, &io, 20, 2);
        assert!(sets_of(&book.clear_rewards(&io, MODE_GUARDIAN, &all)).contains(&(209, "paid")));
        assert!(book.cleared_guardian_hard());
        io.game_saves(1, &save(81));
        book.poll(&io);
        assert!(raid_gate_for_save(&io.files().unwrap(), 1, &save(81)));
        let _ = std::fs::remove_dir_all(&io.dir);
    }

    /// ★ 2026-10-09: a record holding a GUARDIAN receipt from an earlier build (제복 206, paid for a clear at EASY or above)
    /// is owed, on its next load, GUARDIAN's other EASY set and every HELL set — at once, committed with that save, and
    /// nothing again on the reload after.
    #[test]
    fn a_guardian_receipt_is_owed_the_new_sets_on_load() {
        let io = FakeIo::new("guardian_earned");
        let g = save_round(3, 100);
        io.g.borrow_mut().insert(1, g.clone());
        // The record an earlier build committed: 이안 · 율리케 · 할로윈 (the save's) and 제복 for its GUARDIAN clear.
        let mut old = AvatarState::fresh(77);
        for (set, source, mode) in [(200, CLEAR_REWARD_SOURCE, 0), (201, CLEAR_REWARD_SOURCE, 1), (204, CLEAR_REWARD_SOURCE_HALLOWEEN, 2), (206, CLEAR_REWARD_SOURCE_UNIFORM, MODE_GUARDIAN)] {
            let receipt = old.receipt(source, mode, 0).unwrap();
            old.grant_set(set, &[SLOT_HEAD, SLOT_ARMOUR, SLOT_SHOES], mode, receipt);
        }
        io.files().unwrap().write_committed(1, fnv1a64(&g), &old).unwrap();
        let mut book = AvatarBook::new(&io);
        let all = |_: u16| ALL.to_vec();
        book.enter_world(&io, 1, false, 1);
        let paid = book.grant_earned(&io, g_progress(&g).unwrap(), &all);
        assert_eq!(paid.iter().map(|r| (r.set, r.notice_row)).collect::<Vec<_>>(), vec![(207, 71), (203, 67), (205, 69), (210, 74)], "GUARDIAN's EASY set, then HELL's as a HARD clear");
        let s = book.state().unwrap().clone();
        assert_eq!([207, 203, 205, 210, 208, 209].map(|set| s.count(set)), [5, 5, 5, 5, 0, 0], "not NORMAL's or HARD's: the receipt proves EASY only");
        assert_eq!(io.record(1).expect("committed at once").state.items.len(), 12 + 20);
        book.leave_world(&io);
        book.enter_world(&io, 1, false, 2);
        assert!(book.grant_earned(&io, g_progress(&g).unwrap(), &all).is_empty(), "paid already: no second notice");
        // Without a GUARDIAN receipt the same save is owed nothing of GUARDIAN's (its clear is not in the bytes).
        let io2 = FakeIo::new("guardian_unproven");
        io2.g.borrow_mut().insert(1, g.clone());
        let mut book2 = AvatarBook::new(&io2);
        book2.enter_world(&io2, 1, false, 1);
        let paid = book2.grant_earned(&io2, g_progress(&g).unwrap(), &all);
        assert_eq!(paid.iter().map(|r| r.set).collect::<Vec<_>>(), vec![200, 201, 204], "a GUARDIAN save proves HELL — at EASY");
        let _ = std::fs::remove_dir_all(&io.dir);
        let _ = std::fs::remove_dir_all(&io2.dir);
    }

    /// ★ A5: every character owns the starter set — a new game from its start, a load on entry.
    #[test]
    fn every_character_owns_the_starter_set() {
        let io = FakeIo::new("starter");
        io.g.borrow_mut().insert(1, save(10));
        let mut book = AvatarBook::new(&io);
        let starter = [(202u16, SLOT_HEAD), (202, SLOT_ARMOUR), (202, SLOT_SHOES)];
        assert_eq!(book.grant_starter(&io, &starter), 0, "nobody in the world");
        book.enter_world(&io, 1, true, 1);
        assert_eq!(book.grant_starter(&io, &starter), 3, "a new game owns them from its start");
        assert_eq!(book.worn(), [None; SLOTS], "none worn");
        assert!(book.state().unwrap().items.iter().all(|i| i.receipt == 0), "owned, not earned");
        assert_eq!(book.grant_starter(&io, &starter), 0, "owned once");
        // A load of a save from before this build (no record): owned on entry, no save needed.
        book.leave_world(&io);
        book.enter_world(&io, 1, false, 2);
        assert_eq!(book.state().unwrap().items.len(), 0);
        assert_eq!(book.grant_starter(&io, &starter), 3);
        let _ = std::fs::remove_dir_all(&io.dir);
    }

    // ── ★ 2026-10-02: what a save had cleared before this build ──────────────

    /// A knight's `/g` whose third blob is `ag.b()[B`'s own shape: `ag.q[32] · ag.r[32] · ag.k[3] · ag.j[4] ·
    /// ag.i · ag.j · ag.b` count.
    fn save_round(mode: u8, achievement: u8) -> Vec<u8> {
        let mut b2 = vec![0u8; 32 + 32 + 3];
        b2.extend_from_slice(&[12, 10, 0xff, 13, mode, achievement, 0]);
        g_file([&[40, 20, 28, 2, 1], &[48, 0, 0, 7, 9], &b2, &[0], &[48, 0]])
    }

    #[test]
    fn the_save_says_its_mode_and_its_achievement() {
        assert_eq!(g_progress(&save_round(1, 49)), Some(SaveProgress { mode: 1, achievement: 49 }), "the knight LV47 test save: HARDCORE after a NORMAL clear of 50 quests");
        assert_eq!(g_progress(&save_round(2, 100)), Some(SaveProgress { mode: 2, achievement: 100 }));
        // ★ 2026-10-08 가디언: mode 3 is a mode now (MODE_GUARDIAN); 4 is not.
        assert_eq!(g_progress(&save_round(3, 0)), Some(SaveProgress { mode: 3, achievement: 0 }), "GUARDIAN");
        assert_eq!(g_progress(&save_round(4, 0)), None, "not a mode");
        assert_eq!(MODE_GUARDIAN, 3);
        assert_eq!(g_progress(&save_round(2, 101)), None, "above the game's clamp");
        assert_eq!(g_progress(&save(48)), None, "a third blob too short to hold them");
        let g = save_round(1, 49);
        assert_eq!(g_progress(&g[..g.len() - 1]), None, "not a whole save");
        // The 80 line: NORMAL and HARDCORE add 50 and 30 at most (`ag.m`), HELL the last 20.
        assert_eq!((ACHIEVEMENT_WEIGHTS, ACHIEVEMENT_BEFORE_HELL), ([50, 30, 20], 80));
        let cleared = |mode: u8, achievement: u8| cleared_before(SaveProgress { mode, achievement });
        assert_eq!(cleared(0, 0), Vec::<u8>::new(), "NORMAL: nothing cleared yet");
        assert_eq!(cleared(1, 49), vec![0], "HARDCORE: NORMAL was cleared");
        assert_eq!(cleared(2, 78), vec![0, 1], "HELL: NORMAL and HARDCORE — the first HELL round");
        assert_eq!(cleared(2, 80), vec![0, 1], "80 is still reachable without a HELL clear");
        assert_eq!(cleared(2, 81), vec![0, 1, 2], "above 80 only a HELL clear can be");
        assert_eq!(cleared(2, 100), vec![0, 1, 2]);
        assert_eq!(cleared(1, 100), vec![0], "a 달성도 without the mode proves nothing of HELL");
        assert_eq!(cleared(3, 0), vec![0, 1, 2], "GUARDIAN: only a HELL clear gets there — whatever the 달성도 (its weight is 0)");
        assert_eq!(cleared(3, 100), vec![0, 1, 2], "a guardian clear leaves no mark in these two bytes");
    }

    /// ★ The user's example: a save that had cleared HELL owns 클레르 (the starter) · 이안 · 율리케 · 할로윈 on its
    /// first load — paid at once and committed with that save (no save needed, no second notice) — and its next HELL
    /// clear at NORMAL is what pays 수영장 파티.
    #[test]
    fn a_save_that_had_cleared_hell_is_paid_ian_ulrike_and_halloween() {
        let io = FakeIo::new("earned");
        let g = save_round(2, 97);
        io.g.borrow_mut().insert(1, g.clone());
        let mut book = AvatarBook::new(&io);
        let knight = |set: u16| if set == 201 { vec![SLOT_HEAD, SLOT_ARMOUR, SLOT_SHOES] } else { ALL.to_vec() };
        assert!(book.grant_earned(&io, g_progress(&g).unwrap(), &knight).is_empty(), "nobody in the world");
        book.enter_world(&io, 1, false, 1);
        book.grant_starter(&io, &[(202, SLOT_HEAD), (202, SLOT_ARMOUR), (202, SLOT_SHOES)]);
        let paid = book.grant_earned(&io, g_progress(&g).unwrap(), &knight);
        assert_eq!(paid.iter().map(|r| (r.set, r.notice_row)).collect::<Vec<_>>(), vec![(200, 65), (201, 66), (204, 68)], "NORMAL · HARDCORE · HELL as EASY clears, in order");
        let s = book.state().unwrap().clone();
        assert_eq!((s.count(202), s.count(200), s.count(201), s.count(204), s.count(203)), (3, 5, 3, 5, 0), "no 수영장 파티 — it asks for NORMAL");
        assert_eq!(s.worn_avatars(), [None; SLOTS], "none worn");
        let keys: Vec<(u32, u8)> = s.receipts.iter().map(|r| (r.source, r.mode)).collect();
        assert_eq!(keys, vec![(CLEAR_REWARD_SOURCE, 0), (CLEAR_REWARD_SOURCE, 1), (CLEAR_REWARD_SOURCE_HALLOWEEN, 2)]);
        assert!(s.items.iter().filter(|i| i.avatar != 202).all(|i| i.receipt != 0), "paid by receipts");
        // Committed with the save it loaded: a death and a reload of that save find it all, and nothing is paid twice.
        let rec = io.record(1).expect("committed at once");
        assert_eq!((rec.key, rec.state.items.len()), (fnv1a64(&g), 16));
        book.leave_world(&io);
        book.enter_world(&io, 1, false, 2);
        assert_eq!(book.state().unwrap().items.len(), 16);
        assert!(book.grant_earned(&io, g_progress(&g).unwrap(), &knight).is_empty(), "paid already: no second notice");
        // The next HELL clear: 할로윈 is paid; 수영장 파티 waits for a round played at NORMAL.
        let out = book.clear_rewards(&io, 2, &knight);
        assert_eq!(out[0], ClearOutcome::AlreadyPaid { reward: CLEAR_REWARDS[2] });
        assert_eq!(out[1], ClearOutcome::Short { reward: CLEAR_REWARDS[3], done_at: 0, done: 0 });
        // (The first look after a load is the baseline — nothing done yet — then twenty quests at NORMAL.)
        book.observe_quests(&io, &[false; QUESTS], 1);
        let mut done = [false; QUESTS];
        for q in done.iter_mut().take(20) {
            *q = true;
        }
        book.observe_quests(&io, &done, 1);
        let out = book.clear_rewards(&io, 2, &knight);
        assert!(matches!(&out[1], ClearOutcome::Paid { reward, .. } if reward.set == 203), "{out:?}");
        let _ = std::fs::remove_dir_all(&io.dir);
    }

    /// A save in its first HELL round (or under the 80 line) gets 이안 and 율리케 only; its HELL clear pays 할로윈.
    /// A save still in NORMAL, and a new game, get nothing and write nothing.
    #[test]
    fn a_save_is_paid_only_what_it_proves() {
        let io = FakeIo::new("earned_first_hell");
        let g = save_round(2, 78);
        io.g.borrow_mut().insert(1, g.clone());
        io.g.borrow_mut().insert(2, save_round(0, 0));
        let mut book = AvatarBook::new(&io);
        let all = |_: u16| ALL.to_vec();
        book.enter_world(&io, 1, false, 1);
        let paid = book.grant_earned(&io, g_progress(&g).unwrap(), &all);
        assert_eq!(paid.iter().map(|r| r.set).collect::<Vec<_>>(), vec![200, 201]);
        let out = book.clear_rewards(&io, 2, &all);
        assert!(matches!(&out[0], ClearOutcome::Paid { reward, .. } if reward.set == 204), "{out:?}");
        book.leave_world(&io);
        book.enter_world(&io, 2, false, 2);
        assert!(book.grant_earned(&io, g_progress(&save_round(0, 0)).unwrap(), &all).is_empty());
        assert!(io.record(2).is_none(), "nothing paid, nothing written");
        let _ = std::fs::remove_dir_all(&io.dir);
    }

    /// A set the character owns already (an adb grant, receipt 0) is not given twice: only its receipt is written,
    /// so neither path pays it again — and no notice says it was gained. A set the asset does not make for the
    /// class is not paid and spends no receipt.
    #[test]
    fn an_owned_set_gets_only_its_receipt() {
        let io = FakeIo::new("earned_owned");
        let g = save_round(1, 50);
        io.g.borrow_mut().insert(1, g.clone());
        grant_into_record(&io.files().unwrap(), 1, Some(&g), &IAN, 7).unwrap();
        let mut book = AvatarBook::new(&io);
        book.enter_world(&io, 1, false, 1);
        assert!(book.grant_earned(&io, g_progress(&g).unwrap(), &|_| ALL.to_vec()).is_empty(), "owned: no notice");
        let s = book.state().unwrap();
        assert_eq!((s.count(200), s.receipts.len()), (5, 1), "the same five items, one receipt");
        assert_eq!(book.clear_rewards(&io, 0, &|_| ALL.to_vec()), vec![ClearOutcome::AlreadyPaid { reward: CLEAR_REWARDS[0] }]);
        // Nothing for this class: no receipt either.
        let io2 = FakeIo::new("earned_nothing");
        io2.g.borrow_mut().insert(1, save_round(2, 100));
        let mut book2 = AvatarBook::new(&io2);
        book2.enter_world(&io2, 1, false, 1);
        assert!(book2.grant_earned(&io2, g_progress(&save_round(2, 100)).unwrap(), &|_| Vec::new()).is_empty());
        assert_eq!(book2.state().unwrap().receipts.len(), 0);
        assert!(io2.record(1).is_none());
        let _ = std::fs::remove_dir_all(&io.dir);
        let _ = std::fs::remove_dir_all(&io2.dir);
    }

    /// ★ A4: the boot grant reads each slot's class from its save — a berserker gets no off-hand.
    #[test]
    fn the_boot_grant_gives_each_slot_its_own_class() {
        let files = tmp_files("byclass");
        let berserker = g_file([&[40, 20, 28, 2, 0], &[48, 0, 0, 7, 9], &[1, 2, 3], &[0], &[48, 0]]);
        let rogue = g_file([&[40, 20, 28, 2, 2], &[48, 0, 0, 7, 9], &[1, 2, 3], &[0], &[48, 0]]);
        let live = |slot: u8| match slot {
            1 => Some(berserker.clone()),
            2 => Some(rogue.clone()),
            _ => None,
        };
        let items_for = |class: Option<u8>| if class == Some(0) { IAN[..4].to_vec() } else { IAN.to_vec() };
        let lines = grant_marker_run(&files, b"1 2", 5, &items_for, &live, 1);
        assert!(lines[0].contains("slot 1 (berserker)") && lines[0].contains("gained 4"), "{lines:?}");
        assert!(lines[1].contains("slot 2 (rogue)") && lines[1].contains("gained 5"), "{lines:?}");
        let s1 = files.read(1, &mut Vec::new()).0.unwrap().state;
        assert!(!s1.owns(200, SLOT_OFFHAND), "no off-hand for a berserker");
        let s2 = files.read(2, &mut Vec::new()).0.unwrap().state;
        assert!(s2.owns(200, SLOT_OFFHAND), "the rogue's second dagger");
        let _ = std::fs::remove_dir_all(&files.dir);
    }

    #[test]
    fn the_book_recovers_the_commit_a_crash_took() {
        let io = FakeIo::new("crash");
        io.g.borrow_mut().insert(2, save(20));
        let mut book = AvatarBook::new(&io);
        book.enter_world(&io, 2, false, 1);
        book.grant_and_wear(&io, 200, &ALL, 1).unwrap();
        io.game_saves(2, &save(21));
        book.poll(&io);
        // Next save: 201's clothes gained, the game writes `/g` — and the process dies before a poll.
        book.grant_and_wear(&io, 201, &[SLOT_ARMOUR], 1).unwrap();
        io.game_saves(2, &save(22));
        assert!(io.pending_exists(2));
        drop(book);
        let io2 = io.restart();
        let mut book2 = AvatarBook::new(&io2);
        book2.enter_world(&io2, 2, false, 2);
        let (_, s) = book2.working.clone().unwrap();
        assert_eq!((s.count(200), s.count(201), s.worn_avatars()), (5, 1, [Some(200), Some(201), Some(200), Some(200), Some(200)]), "the saved state, recovered");
        assert_eq!(io2.record(2).unwrap().key, fnv1a64(&save(22)), "and committed now");
        assert!(!io2.pending_exists(2));
        // A crash BEFORE the game wrote anything: the journal is dropped, the last commit loads.
        book2.grant_and_wear(&io2, 202, &[SLOT_HEAD], 1).unwrap();
        write_journal(&io2.files().unwrap(), 2, Some(&save(22)), &book2.working.as_ref().unwrap().1).unwrap();
        drop(book2);
        let io3 = io2.restart();
        let mut book3 = AvatarBook::new(&io3);
        book3.enter_world(&io3, 2, false, 3);
        assert_eq!(book3.working.as_ref().unwrap().1.count(202), 0);
        assert!(!io3.pending_exists(2), "a journal of a save that never landed is dropped");
        let _ = std::fs::remove_dir_all(&io.dir);
    }

    /// A commit that cannot be written (a full disk) must not take the journal with it: the next
    /// load recovers the save from it.
    #[test]
    fn a_failed_commit_keeps_the_journal() {
        let io = FakeIo::new("failcommit");
        io.g.borrow_mut().insert(4, save(40));
        let mut book = AvatarBook::new(&io);
        book.enter_world(&io, 4, false, 1);
        book.grant_and_wear(&io, 200, &ALL, 0).unwrap();
        io.game_saves(4, &save(41));
        book.poll(&io);
        let first = io.record(4).expect("the first save commits");
        // The next save: the record cannot be replaced (a directory stands where the rename lands).
        book.grant_and_wear(&io, 201, &[SLOT_ARMOUR], 0).unwrap();
        io.game_saves(4, &save(42));
        let path = AvatarFiles::new(io.dir.clone()).committed_path(4);
        std::fs::remove_file(&path).unwrap();
        std::fs::create_dir(&path).unwrap();
        book.poll(&io);
        assert!(io.notes.borrow().iter().any(|n| n.contains("commit FAILED")));
        assert!(io.pending_exists(4), "the journal is kept");
        // The disk frees up; the process died before another save. The next load recovers.
        std::fs::remove_dir(&path).unwrap();
        AvatarFiles::new(io.dir.clone()).write_committed(4, first.key, &first.state).unwrap();
        drop(book);
        let io2 = io.restart();
        let mut book2 = AvatarBook::new(&io2);
        book2.enter_world(&io2, 4, false, 2);
        let (_, s) = book2.working.clone().unwrap();
        assert_eq!((s.count(201), s.worn_avatars()[usize::from(SLOT_ARMOUR)]), (1, Some(201)), "the save's state, not an empty character");
        assert_eq!(io2.record(4).unwrap().key, fnv1a64(&save(42)));
        let _ = std::fs::remove_dir_all(&io.dir);
    }

    #[test]
    fn the_book_waits_for_a_whole_save_and_gives_up_on_a_torn_one() {
        let io = FakeIo::new("torn");
        io.g.borrow_mut().insert(3, save(30));
        let mut book = AvatarBook::new(&io);
        book.enter_world(&io, 3, false, 1);
        book.grant_and_wear(&io, 200, &ALL, 0).unwrap();
        let whole = save(31);
        io.game_saves(3, &whole[..whole.len() - 2]);
        book.poll(&io);
        assert!(io.record(3).is_none(), "not whole yet — nothing committed");
        // The rest of the write arrives (same save, no new journal).
        io.g.borrow_mut().insert(3, whole.clone());
        book.poll(&io);
        assert_eq!(io.record(3).unwrap().key, fnv1a64(&whole));
        // A save that never completes is given up after COMMIT_POLLS.
        io.game_saves(3, &whole[..10]);
        for _ in 0..AvatarBook::COMMIT_POLLS {
            book.poll(&io);
        }
        assert!(io.notes.borrow().iter().any(|n| n.contains("never read whole")));
        assert_eq!(io.record(3).unwrap().key, fnv1a64(&whole), "the last good commit stands");
        let _ = std::fs::remove_dir_all(&io.dir);
    }

    #[test]
    fn the_files_are_written_whole() {
        let dir = std::env::temp_dir().join(format!("h2_avatar_selftest_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let files = AvatarFiles::new(dir.clone());
        let mut errors = Vec::new();
        assert_eq!(files.read(3, &mut errors), (None, None));
        let mut s = AvatarState::fresh(8);
        s.grant(200, SLOT_ARMOUR, 0, 0);
        files.write_committed(3, 0x11, &s).unwrap();
        files.write_pending(3, 0x11, &s).unwrap();
        let (c, p) = files.read(3, &mut errors);
        assert_eq!(c.unwrap().key, 0x11);
        assert_eq!(p.unwrap().kind, RecordKind::Pending);
        std::fs::write(files.pending_path(3), "garbage").unwrap();
        let (_c, p) = files.read(3, &mut errors);
        assert!(p.is_none() && errors.len() == 1, "an unreadable record is reported and ignored");
        files.remove_pending(3);
        assert!(!files.pending_path(3).exists());
        files.remove_all(3);
        assert!(!files.committed_path(3).exists());
        let leftovers: Vec<_> = std::fs::read_dir(&dir).unwrap().flatten().collect();
        assert!(leftovers.is_empty(), "no temp files left behind");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
