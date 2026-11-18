// Standalone self-test of the 세이브 내보내기 · 불러오기 · 되돌리기 ENGINE (2026-10-28).
//
//   rustc --edition 2021 --test savebundle_selftest.rs -o t.exe && ./t.exe
//
// ★ This `include!`s the real `wie_android/src/savebundle.rs`, so the rules under test ARE the
// shipping rules. Each group below is one of the defects ChronosSword's four code reviews found
// in the engine this one is carried from (its CLAUDE.md 「코드리뷰에서 나온 것」) — every one is
// reproduced here against hero2's paths, and has to come out the safe way.

include!("wie_android/src/savebundle.rs");

#[cfg(test)]
mod tests {
    use super::*;

    const AID: &str = "01035D0B";

    fn tmp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("h2_bundle_{tag}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn put(path: &Path, bytes: &[u8]) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
    }

    fn bundle(files: Vec<(Area, &str, &[u8])>) -> Bundle {
        Bundle { aid: AID.into(), created_ms: 1_790_000_000_000, app: 159, backup: false, files: files.into_iter().map(|(a, n, b)| (a, n.to_string(), b.to_vec())).collect() }
    }

    /// The phone as it stands: every owned path → its bytes.
    fn phone(data: &Path) -> Vec<(Area, String, Vec<u8>)> {
        present(&Layout::new(data, AID)).unwrap()
    }

    fn sample() -> Bundle {
        bundle(vec![
            (Area::Fs, "n1", b"bag one"),
            (Area::Fs, "g1", b"character one"),
            (Area::Account, "bag", b"warehouse"),
            (Area::Avatar, "1", b"costumes"),
            (Area::Host, "wie_quickpages.txt", b"own 1 1\n"),
        ])
    }

    // ── the file ─────────────────────────────────────────────────────────────

    #[test]
    fn a_bundle_reads_back_exactly() {
        let b = sample();
        assert_eq!(parse(&encode(&b)).unwrap(), b);
        let mut k = b.clone();
        k.backup = true;
        assert_eq!(parse(&encode(&k)).unwrap(), k, "the backup flag rides the manifest");
        assert_eq!(b.slots(), vec![1]);
    }

    #[test]
    fn crc32_is_the_zip_one() {
        assert_eq!(crc32(b"123456789"), 0xcbf4_3926);
        assert_eq!(crc32(b""), 0);
    }

    #[test]
    fn only_listed_names_are_accepted() {
        let m: &[u8] = b"format hero2-save 1\naid 01035D0B\n";
        for stranger in ["../evil", "fs/../../evil", "fs/n6", "fs/n1/x", "other/n1", "fs/.n1.xfer", "host/wie_layout.txt", "avatar/9", "/fs/n1"] {
            let z = zip(&[(MANIFEST, m), ("fs/n1", b"x"), (stranger, b"x")]);
            assert!(matches!(parse(&z), Err(Reject::NotOurs(_))), "{stranger} must be refused");
        }
        assert!(parse(&zip(&[("fs/n1", b"x")])).is_err(), "no manifest");
        assert!(parse(b"PK not really").is_err());
        assert!(parse(b"").is_err());
    }

    #[test]
    fn the_manifest_is_checked() {
        let with = |m: &[u8]| zip(&[(MANIFEST, m), ("fs/n1", b"x")]);
        assert!(matches!(parse(&with(b"format hero2-save 2\naid 01035D0B\n")), Err(Reject::Format(_))));
        assert!(matches!(parse(&with(b"format hero2-save 1\naid ../x\n")), Err(Reject::NotOurs(_))));
        assert!(matches!(parse(&with(b"format hero2-save 1\n")), Err(Reject::NotOurs(_))));
        let crlf = parse(&with(b"format hero2-save 1\r\naid 01035D0B\r\ncreated 7\r\napp 67\r\n")).unwrap();
        assert_eq!((crlf.aid.as_str(), crlf.created_ms, crlf.app, crlf.backup), (AID, 7, 67, false));
    }

    #[test]
    fn a_changed_byte_is_damage_not_a_stranger() {
        let mut z = encode(&sample());
        let at = z.windows(7).position(|w| w == b"bag one").unwrap();
        z[at] ^= 1;
        assert!(matches!(parse(&z), Err(Reject::Damaged(_))));
    }

    // ── chronos review 2, P2: the directory is checked whole first ───────────

    /// A directory naming one stretch of data `n` times, in the order `zip` would never write.
    fn repeated(n: usize) -> Vec<u8> {
        let data = vec![7u8; 1000];
        let mut out = zip(&[(MANIFEST, b"format hero2-save 1\naid 01035D0B\n"), ("fs/n1", &data)]);
        // Strip the end record and the directory, keep the two local entries.
        let eocd = out.len() - 22;
        let cd_off = get32(&out, eocd + 16).unwrap();
        let central = out[cd_off..eocd].to_vec();
        out.truncate(cd_off);
        let second = central.windows(4).enumerate().filter(|(_, w)| *w == [0x50, 0x4b, 0x01, 0x02]).map(|(i, _)| i).nth(1).unwrap();
        let entry = central[second..].to_vec();
        let new_off = out.len();
        let mut cd = central[..second].to_vec();
        for _ in 0..n {
            cd.extend_from_slice(&entry);
        }
        out.extend_from_slice(&cd);
        put32(&mut out, 0x0605_4b50);
        put16(&mut out, 0);
        put16(&mut out, 0);
        put16(&mut out, (n + 1) as u16);
        put16(&mut out, (n + 1) as u16);
        put32(&mut out, cd.len() as u32);
        put32(&mut out, new_off as u32);
        put16(&mut out, 0);
        out
    }

    #[test]
    fn a_directory_that_names_one_entry_two_thousand_times_is_refused_before_any_copy() {
        let z = repeated(2000);
        let e = unzip(&z, entries_max(), MEMBER_MAX, FILE_MAX).unwrap_err();
        assert!(e.contains("entries"), "{e}");
        // Within the count, the same name twice is refused too.
        let e = unzip(&repeated(2), entries_max(), MEMBER_MAX, FILE_MAX).unwrap_err();
        assert!(e.contains("twice"), "{e}");
    }

    #[test]
    fn overlapping_and_oversized_entries_are_refused_and_a_reversed_directory_is_fine() {
        // Two names, one stretch of data: entry 2's local header points inside entry 1's data.
        let a = vec![1u8; 200];
        let mut z = zip(&[(MANIFEST, b"format hero2-save 1\naid 01035D0B\n"), ("fs/n1", &a), ("fs/g1", &a)]);
        let eocd = z.len() - 22;
        let cd_off = get32(&z, eocd + 16).unwrap();
        let third = z[cd_off..].windows(4).enumerate().filter(|(_, w)| *w == [0x50, 0x4b, 0x01, 0x02]).map(|(i, _)| cd_off + i).nth(2).unwrap();
        let second_local = get32(&z, z[cd_off..].windows(4).enumerate().filter(|(_, w)| *w == [0x50, 0x4b, 0x01, 0x02]).map(|(i, _)| cd_off + i).nth(1).unwrap() + 42).unwrap();
        let inside = (second_local + 40) as u32;
        z[third + 42..third + 46].copy_from_slice(&inside.to_le_bytes());
        assert!(unzip(&z, entries_max(), MEMBER_MAX, FILE_MAX).is_err());
        // Over the member ceiling.
        let big = vec![0u8; 300];
        assert!(unzip(&zip(&[("fs/n1", &big)]), 10, 256, 4096).unwrap_err().contains("bytes"));
        assert!(unzip(&zip(&[("fs/n1", &big), ("fs/g1", &big)]), 10, 512, 400).unwrap_err().contains("ceiling"));
        // The directory in reverse order: still one entry after another — accepted.
        let fwd = zip(&[("fs/n1", b"aa"), ("fs/g1", b"bb")]);
        let eocd = fwd.len() - 22;
        let cd_off = get32(&fwd, eocd + 16).unwrap();
        let starts: Vec<usize> = fwd[cd_off..eocd].windows(4).enumerate().filter(|(_, w)| *w == [0x50, 0x4b, 0x01, 0x02]).map(|(i, _)| cd_off + i).collect();
        let (e1, e2) = (fwd[starts[0]..starts[1]].to_vec(), fwd[starts[1]..eocd].to_vec());
        let mut rev = fwd[..cd_off].to_vec();
        rev.extend_from_slice(&e2);
        rev.extend_from_slice(&e1);
        rev.extend_from_slice(&fwd[eocd..]);
        assert_eq!(unzip(&rev, 10, 100, 1000).unwrap().len(), 2);
    }

    #[test]
    fn only_plain_stored_entries_are_read() {
        let z = zip(&[("fs/n1", b"abc")]);
        let cd = get32(&z, z.len() - 22 + 16).unwrap();
        for (what, at, value) in [("encrypted", cd + 8, 1u16), ("descriptor", cd + 8, 8), ("deflate", cd + 10, 8), ("local encrypted", 6, 1)] {
            let mut bad = z.clone();
            bad[at..at + 2].copy_from_slice(&value.to_le_bytes());
            assert!(unzip(&bad, 10, 100, 1000).is_err(), "{what}");
        }
    }

    // ── the apply ────────────────────────────────────────────────────────────

    #[test]
    fn a_staged_file_is_applied_at_the_next_start_and_the_phone_before_is_kept() {
        let d = tmp("apply");
        let l = Layout::new(&d, AID);
        put(&l.path(Area::Fs, "n2"), b"old bag two");
        put(&l.path(Area::Fs, "g2"), b"old character two");
        put(&l.path(Area::Fs, "cf"), b"\x3f\x32settings");
        put(&l.path(Area::Host, "wie_difficulty.txt"), b"2");
        stage(&d, &encode(&sample())).unwrap();
        let r = apply_pending(&d, Some(AID), 159, 1_800_000_000_000);
        assert_eq!(r.outcome, Outcome::Applied, "{:?}", r.log);
        assert_eq!(phone(&d), sample().files, "the phone IS the file: slot 2 and the 난이도 are gone");
        assert_eq!(std::fs::read(l.path(Area::Fs, "cf")).unwrap(), b"\x3f\x32settings", "cf is never touched");
        assert!(!pending_path(&d).exists() && !applying_path(&d).exists() && !before_path(&d).exists());
        let (_, b) = newest_backup(&d).unwrap();
        let kept = parse(&std::fs::read(b).unwrap()).unwrap();
        assert!(kept.backup);
        assert_eq!(kept.get(Area::Fs, "g2"), Some(&b"old character two"[..]));
        assert_eq!(kept.get(Area::Host, "wie_difficulty.txt"), Some(&b"2"[..]), "the host files the import replaced are in the backup");
        assert_eq!(kept.get(Area::Fs, "cf"), None);
        // Nothing staged: idle, and nothing changes.
        assert_eq!(apply_pending(&d, Some(AID), 159, 1_800_000_000_001).outcome, Outcome::Idle);
    }

    #[test]
    fn restoring_the_newest_backup_twice_is_a_round_trip() {
        let d = tmp("roundtrip");
        let l = Layout::new(&d, AID);
        put(&l.path(Area::Fs, "n3"), b"mine");
        put(&l.path(Area::Fs, "g3"), b"mine too");
        let mine = phone(&d);
        stage(&d, &encode(&sample())).unwrap();
        assert_eq!(apply_pending(&d, Some(AID), 1, 10).outcome, Outcome::Applied);
        let theirs = phone(&d);
        for (expect, at) in [(&mine, 20u64), (&theirs, 30), (&mine, 40)] {
            let (_, newest) = newest_backup(&d).unwrap();
            stage(&d, &std::fs::read(newest).unwrap()).unwrap();
            assert_eq!(apply_pending(&d, Some(AID), 1, at).outcome, Outcome::Applied);
            assert_eq!(&phone(&d), expect);
        }
    }

    #[test]
    fn the_backups_rotate_to_five_and_a_repeat_is_not_kept_twice() {
        let d = tmp("rotate");
        let l = Layout::new(&d, AID);
        for i in 0..8u64 {
            put(&l.path(Area::Fs, "n1"), format!("bag {i}").as_bytes());
            put(&l.path(Area::Fs, "g1"), format!("char {i}").as_bytes());
            stage(&d, &encode(&sample())).unwrap();
            assert_eq!(apply_pending(&d, Some(AID), 1, 100 + i).outcome, Outcome::Applied);
        }
        assert_eq!(backups(&d).len(), BACKUP_KEEP);
        // The phone is the file now: the next apply keeps it once, and a repeat keeps nothing new.
        stage(&d, &encode(&sample())).unwrap();
        assert_eq!(apply_pending(&d, Some(AID), 1, 200).outcome, Outcome::Applied);
        let before = backups(&d);
        stage(&d, &encode(&sample())).unwrap();
        assert_eq!(apply_pending(&d, Some(AID), 1, 201).outcome, Outcome::Applied);
        let after = backups(&d);
        assert_eq!(after, before, "the newest backup already is this phone");
        // A clock that went back still names the next backup after the newest.
        put(&l.path(Area::Fs, "n1"), b"changed");
        stage(&d, &encode(&sample())).unwrap();
        assert_eq!(apply_pending(&d, Some(AID), 1, 5).outcome, Outcome::Applied);
        assert!(newest_backup(&d).unwrap().0 > after.last().unwrap().0);
    }

    #[test]
    fn a_file_for_another_archive_or_an_unreadable_stage_changes_nothing() {
        let d = tmp("refuse");
        let l = Layout::new(&d, AID);
        put(&l.path(Area::Fs, "n1"), b"mine");
        let mut other = sample();
        other.aid = "01035D0C".into();
        stage(&d, &encode(&other)).unwrap();
        let r = apply_pending(&d, Some(AID), 1, 1);
        assert_eq!(r.outcome, Outcome::NotApplied);
        assert_eq!(std::fs::read(l.path(Area::Fs, "n1")).unwrap(), b"mine");
        assert!(!pending_path(&d).exists(), "given up on = gone, so it is not retried every start");
        put(&pending_path(&d), b"not a zip");
        assert_eq!(apply_pending(&d, Some(AID), 1, 2).outcome, Outcome::NotApplied);
        assert!(!pending_path(&d).exists());
        stage(&d, &encode(&sample())).unwrap();
        assert_eq!(apply_pending(&d, None, 1, 3).outcome, Outcome::NotApplied, "no AID: nowhere to apply");
        assert_eq!(std::fs::read(l.path(Area::Fs, "n1")).unwrap(), b"mine");
    }

    // ── chronos review 1, P1: a read that fails is not 「absent」 ────────────

    #[test]
    fn a_file_that_cannot_be_read_stops_the_apply_before_anything_is_written() {
        let d = tmp("unreadable");
        let l = Layout::new(&d, AID);
        put(&l.path(Area::Fs, "n1"), b"mine");
        // A directory where a member is: present() cannot read it — it must not count as absent.
        std::fs::create_dir_all(l.path(Area::Account, "bag")).unwrap();
        stage(&d, &encode(&sample())).unwrap();
        let r = apply_pending(&d, Some(AID), 1, 1);
        assert_eq!(r.outcome, Outcome::NotApplied, "{:?}", r.log);
        assert_eq!(std::fs::read(l.path(Area::Fs, "n1")).unwrap(), b"mine");
        assert!(backups(&d).is_empty() && !pending_path(&d).exists());
    }

    // ── chronos review 1, P1: never start on a half-applied phone ────────────

    #[test]
    fn a_write_that_fails_half_way_puts_the_phone_back() {
        let d = tmp("rollback");
        let l = Layout::new(&d, AID);
        put(&l.path(Area::Fs, "n1"), b"mine");
        put(&l.path(Area::Fs, "g1"), b"mine too");
        // The file brings a warehouse this phone does not have, and that write cannot happen.
        std::fs::create_dir_all(l.dir(Area::Account).join(".bag.xfer")).unwrap();
        stage(&d, &encode(&sample())).unwrap();
        let r = apply_pending(&d, Some(AID), 1, 1);
        assert_eq!(r.outcome, Outcome::NotApplied, "{:?}", r.log);
        assert_eq!(std::fs::read(l.path(Area::Fs, "g1")).unwrap(), b"mine too", "the save half that WAS written went back");
        assert!(!applying_path(&d).exists() && !before_path(&d).exists());
        assert!(backups(&d).is_empty(), "nothing changed, so no backup of 「now」 is left for 되돌리기 to go back to");
    }

    #[test]
    fn when_neither_way_can_finish_the_game_does_not_start_until_one_can() {
        let d = tmp("blocked");
        let l = Layout::new(&d, AID);
        put(&l.path(Area::Fs, "n1"), b"mine");
        put(&l.path(Area::Fs, "g1"), b"mine too");
        // g1 can be written by neither the file nor the pre-image.
        std::fs::create_dir_all(l.dir(Area::Fs).join(".g1.xfer")).unwrap();
        stage(&d, &encode(&sample())).unwrap();
        let first = apply_pending(&d, Some(AID), 1, 1);
        assert_eq!(first.outcome, Outcome::Blocked, "{:?}", first.log);
        assert!(!first.may_start());
        assert!(applying_path(&d).exists() && before_path(&d).exists(), "the journal is kept");
        assert_eq!(apply_pending(&d, Some(AID), 1, 2).outcome, Outcome::Blocked, "every start, until it can finish");
        std::fs::remove_dir(l.dir(Area::Fs).join(".g1.xfer")).unwrap();
        let third = apply_pending(&d, Some(AID), 1, 3);
        assert_eq!(third.outcome, Outcome::Applied, "{:?}", third.log);
        assert_eq!(phone(&d), sample().files);
        assert!(!applying_path(&d).exists() && !before_path(&d).exists());
        // The phone it replaced survives as the backup — a blocked start must not take it back
        // (code review 2026-10-28 P1: once the journal closed, it was the only copy).
        let (_, newest) = newest_backup(&d).expect("the phone before is kept");
        let kept = parse(&std::fs::read(newest).unwrap()).unwrap();
        assert_eq!(kept.files, vec![(Area::Fs, "n1".to_string(), b"mine".to_vec()), (Area::Fs, "g1".to_string(), b"mine too".to_vec())]);
    }

    /// Code review 2026-10-28, second pass: an apply blocked at its start and put BACK by the next
    /// start must not leave its backup — a copy of the phone as it now is — as the newest. 되돌리기
    /// would then 「go back」 to where the phone already is, forever, and the earlier backup that
    /// matters would never be reached.
    #[test]
    fn a_blocked_apply_the_next_start_puts_back_leaves_the_earlier_backup_newest() {
        let d = tmp("blocked_back");
        let l = Layout::new(&d, AID);
        put(&l.path(Area::Fs, "n1"), b"first");
        put(&l.path(Area::Fs, "g1"), b"first too");
        // An earlier apply: its backup (the phone 「first」) is the one 되돌리기 must still reach.
        let mine = bundle(vec![(Area::Fs, "n1", b"mine"), (Area::Fs, "g1", b"mine too")]);
        stage(&d, &encode(&mine)).unwrap();
        assert_eq!(apply_pending(&d, Some(AID), 1, 10).outcome, Outcome::Applied);
        let earlier = backups(&d);
        assert_eq!(earlier.len(), 1);
        // Now an apply that neither way can finish (g1 cannot be written): blocked, its backup kept.
        std::fs::create_dir_all(l.dir(Area::Fs).join(".g1.xfer")).unwrap();
        stage(&d, &encode(&sample())).unwrap();
        assert_eq!(apply_pending(&d, Some(AID), 1, 20).outcome, Outcome::Blocked);
        assert_eq!(backups(&d).len(), 2, "blocked: undecided, so its backup stays");
        assert!(own_backup_marker(&d).exists());
        // The next start: g1 can be written again, but the file's warehouse cannot — put back.
        std::fs::remove_dir(l.dir(Area::Fs).join(".g1.xfer")).unwrap();
        std::fs::create_dir_all(l.dir(Area::Account).join(".bag.xfer")).unwrap();
        let r = apply_pending(&d, Some(AID), 1, 30);
        assert_eq!(r.outcome, Outcome::NotApplied, "{:?}", r.log);
        assert_eq!(phone(&d), mine.files);
        assert_eq!(backups(&d), earlier, "the backup of 「now」 went; the earlier one is the newest again");
        assert!(!own_backup_marker(&d).exists() && !applying_path(&d).exists() && !before_path(&d).exists());
        // …and 되돌리기 from here reaches 「first」.
        std::fs::remove_dir(l.dir(Area::Account).join(".bag.xfer")).unwrap();
        let (_, newest) = newest_backup(&d).unwrap();
        stage(&d, &std::fs::read(newest).unwrap()).unwrap();
        assert_eq!(apply_pending(&d, Some(AID), 1, 40).outcome, Outcome::Applied);
        assert_eq!(std::fs::read(l.path(Area::Fs, "g1")).unwrap(), b"first too");
    }

    #[test]
    fn a_backup_marker_names_only_a_backup() {
        let d = tmp("marker");
        put(&d.join(BACKUP_DIR).join("01790000000000.zip"), b"x");
        put(&d.join("precious"), b"keep me");
        for bad in ["../precious", "01790000000000.zip/../../precious", "precious", ""] {
            put(&own_backup_marker(&d), bad.as_bytes());
            assert_eq!(own_backup(&d), None, "{bad:?}");
        }
        put(&own_backup_marker(&d), b"01790000000000.zip\n");
        assert_eq!(own_backup(&d), Some(d.join(BACKUP_DIR).join("01790000000000.zip")));
        // With no journal and no stage, a marker is stale: swept, the backup it names kept.
        assert_eq!(apply_pending(&d, Some(AID), 1, 1).outcome, Outcome::Idle);
        assert!(!own_backup_marker(&d).exists() && d.join(BACKUP_DIR).join("01790000000000.zip").exists());
        assert_eq!(std::fs::read(d.join("precious")).unwrap(), b"keep me");
    }

    #[test]
    fn a_process_killed_after_the_commit_is_finished_by_the_next_start() {
        let d = tmp("killed");
        let l = Layout::new(&d, AID);
        put(&l.path(Area::Fs, "n4"), b"mine");
        put(&l.path(Area::Fs, "g4"), b"mine too");
        let mine = Bundle { aid: AID.into(), created_ms: 1, app: 1, backup: true, files: phone(&d) };
        // What a kill between the commit rename and the last write leaves: the journal, the pre-image,
        // and a disk that is half one and half the other.
        put(&applying_path(&d), &encode(&sample()));
        put(&before_path(&d), &encode(&mine));
        put(&l.path(Area::Fs, "n1"), b"bag one");
        let r = apply_pending(&d, Some(AID), 1, 2);
        assert_eq!(r.outcome, Outcome::Applied, "{:?}", r.log);
        assert_eq!(phone(&d), sample().files);
        // …and an unreadable journal goes BACK to the pre-image instead.
        put(&applying_path(&d), b"torn");
        put(&before_path(&d), &encode(&mine));
        assert_eq!(apply_pending(&d, Some(AID), 1, 3).outcome, Outcome::NotApplied);
        assert_eq!(phone(&d), mine.files);
        // A journal with no pre-image and no target cannot be settled: blocked.
        put(&applying_path(&d), b"torn");
        assert_eq!(apply_pending(&d, Some(AID), 1, 4).outcome, Outcome::Blocked);
        assert_eq!(apply_pending(&d, None, 1, 5).outcome, Outcome::Blocked, "no AID: nowhere to finish it");
    }

    #[test]
    fn a_pre_image_no_journal_refers_to_is_swept() {
        let d = tmp("sweep");
        put(&before_path(&d), b"left over");
        put(&doc_file(&d, true, 7), b"an export the last run made");
        put(&doc_file(&d, false, 8), b"an import the last run read");
        let r = apply_pending(&d, Some(AID), 1, 1);
        assert_eq!(r.outcome, Outcome::Idle);
        assert!(!before_path(&d).exists() && !doc_file(&d, true, 7).exists() && !doc_file(&d, false, 8).exists());
    }

    // ── chronos review 4: a stage the game goes on past must be gone ─────────

    #[test]
    fn a_stage_that_fails_is_taken_back_or_called_unsure() {
        let d = tmp("stage");
        // The temp file cannot be made: nothing reached the stage's name — cancelled.
        std::fs::create_dir_all(d.join(IMPORT_DIR).join(".pending.zip.xfer")).unwrap();
        assert!(matches!(stage(&d, b"x"), Err(StageFailed::Cancelled(_))));
        assert!(!pending_path(&d).exists());
        std::fs::remove_dir(d.join(IMPORT_DIR).join(".pending.zip.xfer")).unwrap();
        // Something at the stage's name that cannot be removed: not surely gone — unsure.
        std::fs::create_dir_all(pending_path(&d).join("stuck")).unwrap();
        assert!(matches!(stage(&d, b"x"), Err(StageFailed::Unsure(_))));
        // …and every start is blocked on it until it is cleared.
        assert_eq!(apply_pending(&d, Some(AID), 1, 1).outcome, Outcome::Blocked);
        assert_eq!(apply_pending(&d, Some(AID), 1, 2).outcome, Outcome::Blocked);
        std::fs::remove_dir_all(pending_path(&d)).unwrap();
        assert_eq!(apply_pending(&d, Some(AID), 1, 3).outcome, Outcome::Idle);
        // A plain stage, and taking it back.
        stage(&d, b"y").unwrap();
        assert_eq!(unstage(&d), Ok(true));
        assert_eq!(unstage(&d), Ok(false));
    }

    #[test]
    fn a_write_leaves_no_temp_file_behind() {
        let d = tmp("atomic");
        let p = d.join("x").join("file");
        write_atomically(&p, b"one").unwrap();
        write_atomically(&p, b"two").unwrap();
        assert_eq!(std::fs::read(&p).unwrap(), b"two");
        let names: Vec<String> = std::fs::read_dir(d.join("x")).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
        assert_eq!(names, vec!["file".to_string()]);
    }

    /// Code review 2026-10-28 P2: a directory the apply makes has its NAME synced into the directory
    /// it was made in — every one of them, outermost first — and a directory already there costs nothing.
    #[test]
    fn a_made_directory_is_synced_into_its_parent() {
        let d = tmp("mkdir");
        let deep = d.join("01035D0B").join("account");
        assert_eq!(create_dir_durably(&deep).unwrap(), vec![d.clone(), d.join("01035D0B")]);
        assert!(deep.is_dir());
        assert_eq!(create_dir_durably(&deep).unwrap(), Vec::<PathBuf>::new(), "nothing made, nothing synced");
        assert_eq!(create_dir_durably(&d.join("01035D0B").join("avatar")).unwrap(), vec![d.join("01035D0B")]);
        // A file where a directory must go: an error, never a 「made」.
        put(&d.join("blocked"), b"x");
        assert!(create_dir_durably(&d.join("blocked").join("in")).is_err());
        // The apply's own write goes through it.
        write_atomically(&d.join("new").join("file"), b"one").unwrap();
        assert_eq!(std::fs::read(d.join("new").join("file")).unwrap(), b"one");
        let _ = std::fs::remove_dir_all(&d);
    }

    /// Code review 2026-10-28, second pass P2: a retry finds the directories an earlier attempt made
    /// 「already there」, so the apply's end syncs EVERY directory between an owned file and the data dir
    /// — whatever exists, whoever made it.
    #[test]
    fn the_apply_syncs_every_directory_up_to_the_data_dir() {
        let d = tmp("chain");
        let layout = Layout::new(&d, AID);
        let dirs = apply_sync_dirs(&layout);
        for area in Area::ALL {
            for name in names(area) {
                let p = layout.path(area, &name);
                for a in p.ancestors().skip(1).filter(|a| a.starts_with(&d)) {
                    assert!(dirs.iter().any(|x| x.as_path() == a), "{a:?} (above {p:?}) is not synced by the apply");
                }
            }
        }
        assert!(dirs.contains(&layout.root) && dirs.contains(&d));
        let mut unique = dirs.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), dirs.len(), "each once");
        // And the stage: a stage directory that is already there is no reason to skip the data dir.
        std::fs::create_dir_all(d.join(IMPORT_DIR)).unwrap();
        assert_eq!(stage(&d, b"x"), Ok(()));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn backup_names_are_fourteen_digits() {
        assert_eq!(parse_backup_name("01790000000000.zip"), Some(1_790_000_000_000));
        for no in ["1790000000000.zip", "01790000000000.zip.xfer", ".01790000000000.zip", "0179000000000a.zip", "pending.zip"] {
            assert_eq!(parse_backup_name(no), None, "{no}");
        }
    }

    #[test]
    fn the_stamp_is_local_time() {
        // 2026-10-28 05:30 UTC = 14:30 KST.
        let ms = 1_793_165_400_000u64;
        assert_eq!(civil(ms, 0), (2026, 10, 28, 5, 30));
        assert_eq!(short_stamp(ms, 540), "10-28 14:30");
        assert_eq!(civil(0, 0), (1970, 1, 1, 0, 0));
    }
}
