"""demon_look.py 의 검사 — python -W ignore::ResourceWarning -m unittest discover -s tools_re -p test_demon_look.py -v"""
import io
import os
import re
import unittest
import zipfile

import costume_sheets as cs
import demon_look as dl
import demon_skin
import knight_iaido
import venom_effect

# ★★ 2026-10-17 (code review): sequences a class's animation carries AFTER the demon ones — the knight's 발도 radius 4 · 5
# (`knight_iaido.patch_reach_anim`, the same sum `make_hero2_asset.py` checks the built jar against)
AFTER_DEMON_SEQS = {0: 0, 1: knight_iaido.REACH_SEQS, 2: 0}


class DemonLookTest(unittest.TestCase):
    def test_frame_numbering_is_the_contract(self):
        self.assertEqual(dl.FRAMES, 18 * 4 + 3 + 10 * 4)
        self.assertLessEqual(dl.BODY_BASE + dl.FRAMES, 128, "the drawer reads a sequence's frame as a signed byte")
        self.assertEqual(dl.frame_of("stand", dl.BACK), dl.BODY_BASE)
        self.assertEqual(dl.frame_of("walk0", dl.FRONT), dl.BODY_BASE + 5)
        self.assertEqual(dl.frame_of("roar", dl.RIGHT), dl.BODY_BASE + 17 * 4 + 3)
        self.assertEqual(dl.frame_of("fall2", dl.LEFT), dl.BODY_BASE + 18 * 4 + 2)
        # ★ (5): the extra poses after the falls — the first 75 keep their numbers
        self.assertEqual(dl.frame_of("idle", dl.BACK), dl.BODY_BASE + 18 * 4 + 3)
        self.assertEqual(dl.frame_of("lava_slam", dl.RIGHT), dl.BODY_BASE + dl.FRAMES - 1)
        # 옛 이름(3 차 · demon_fx 장면)은 지금 이름의 프레임으로
        for old, new in dl.LEGACY.items():
            self.assertIn(new, dl.POSES)
            self.assertEqual(dl.frame_of(old, dl.RIGHT), dl.frame_of(new, dl.RIGHT))
        self.assertEqual(dl.BODY_BASE, 9)
        self.assertEqual(dl.MAPPED_COUNT, 18)
        for cls in range(3):
            row = dl.MAPPED[cls]
            self.assertEqual(len(row), dl.MAPPED_COUNT)
            self.assertEqual(row, sorted(set(row)))
            self.assertTrue(all(s < dl.SHIPPED_SEQS[cls] for s in row))
            self.assertEqual(row[:7], [0, 1, 2, 3, 4, 5, 6])
            self.assertIn(dl.FALL_SEQ[cls], row)
            self.assertEqual(dl.demon_seq(cls, 0), dl.SHIPPED_SEQS[cls])
            self.assertEqual(dl.transform_seq_no(cls), dl.SHIPPED_SEQS[cls] + 18)
        self.assertIsNone(dl.demon_seq(1, 14))

    def test_demon_copy_keeps_the_frame_count_and_the_effects(self):
        human = [[[[0, 2, 250, 231], [1, 2, 253, 244], [4, 7, 3, 250]] for _ in range(4)] for _ in range(5)]
        for cls in range(3):
            for seq in (4, 7):
                copy = dl.demon_copy(cls, seq, human)
                self.assertEqual(len(copy), 5)
                for f, frame in enumerate(copy):
                    for d, sprites in enumerate(frame):
                        expected = [sp for sp in human[f][d] if sp[0] & 0x7f not in ({4, 6, 7} if seq in dl.CLAW_SEQS else {6, 7})]
                        self.assertEqual(sprites, expected)
            armed = [[[[0, 2, 0, 0], [1, 2, 0, 0], [6, 55, 0, 0], [7, 42, 0, 0]] for _ in range(4)]]
            self.assertTrue(all(sp[0] not in (6, 7) for fr in dl.demon_copy(cls, 4, armed) for d in fr for sp in d))

    def test_claw_plan_strikes_on_the_hit_frames_without_weapons(self):
        """★★ 2026-10-22: 할퀴기 단 = 그 직업의 공격 프레임을 다시 엮은 것 — 길이 = 단 길이, 타격 프레임은 뛰어듦(앞으로 ≥ 3), 첫 프레임은
        물러섬 · 제자리, 2 단의 둘째는 도약; 사본에 무기 · 보조손 · 이펙트가 없고 움직임은 바라보는 쪽을 따른다."""
        for cls, lens in dl.STAGE_LENS.items():
            for stage, n in enumerate(lens):
                plan = dl.CLAW_PLAN[cls][stage]
                self.assertEqual(len(plan), n, f"class {cls} stage {stage}")
                self.assertTrue(all(src in dl.CLAW_SEQS for src, _f, _a, _d in plan))
                for f in dl.HIT_FRAMES[4 + stage]:
                    self.assertGreaterEqual(plan[f][2], 3, f"class {cls} stage {stage} frame {f} lunges")
                self.assertLessEqual(plan[0][2], 0, "the wind-up steps back or stays")
                if stage == 2:
                    self.assertLess(plan[1][3], 0, "the slam leaps")
        human = {s: [[[[0, 2, 250, 231], [2, s, 3, 250], [4, 7, 3, 250], [6, 1, 0, 0], [7, 0, 0, 0]] for _d in range(4)] for _f in range(5)]
                 for s in (4, 5, 6)}
        seqs = [None] * 4 + [human[4], human[5], human[6]]
        copy = dl.claw_copy(0, 0, seqs)
        self.assertEqual(len(copy), 5)
        for f, (src, _sf, ahead, down) in enumerate(dl.CLAW_PLAN[0][0]):
            moves = {dl.RIGHT: (ahead, down), dl.LEFT: (-ahead, down), dl.FRONT: (0, down + ahead), dl.BACK: (0, down - ahead)}
            for view, (mx, my) in moves.items():
                self.assertEqual(copy[f][view], [[0, 2, (250 - 256 + mx) & 0xFF, (231 - 256 + my) & 0xFF], [2, src, (3 + mx) & 0xFF, (250 - 256 + my) & 0xFF]])
        with self.assertRaises(ValueError):
            dl.claw_copy(1, 0, seqs)  # the knight's stage is 4 frames; the sequence here has 5

    def test_claw_stages_strike_on_the_hit_frames(self):
        """할퀴기 단의 타격 프레임(DemonPatch::HIT_FRAMES)에 「친」 자세가 온다 — 단 길이 3 · 4 · 5 (직업마다 다르다) 전부."""
        src = open(os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "wie_android", "src", "guest_anchors.rs"), encoding="utf-8").read()
        m = re.search(r"pub const HIT_FRAMES: \[&'static \[u8\]; 3\] = \[(.*?)\];", src)
        self.assertIsNotNone(m, "DemonPatch::HIT_FRAMES")
        rust = [tuple(int(v) for v in re.findall(r"\d+", g)) for g in re.findall(r"&\[([^\]]*)\]", m.group(1))]
        self.assertEqual(rust, [dl.HIT_FRAMES[4], dl.HIT_FRAMES[5], dl.HIT_FRAMES[6]])
        strikes = {4: {"c0_hitA", "c0_hitB"}, 5: {"c1_hit"}, 6: {"c2_hit"}}
        for seq, hits in dl.HIT_FRAMES.items():
            for n in (3, 4, 5):
                poses = [dl.pose_for(1, seq, f) for f in range(n)]
                self.assertEqual(poses[0], dl.STAGE_POSES[seq][0], "every stage opens with its wind-up")
                for f in hits:
                    if f < n:
                        self.assertIn(poses[f], strikes[seq], f"seq {seq} n {n}: frame {f} is {poses[f]}")
                for f, pose in enumerate(poses):
                    if pose in strikes[seq] and pose != "c0_hitB":
                        self.assertIn(f, hits)
        # 몸의 움직임: 오른쪽 · 왼쪽은 거울, 앞 · 뒤는 세로로
        self.assertEqual(dl.motion("c1_hit", dl.RIGHT), (3, 0))
        self.assertEqual(dl.motion("c1_hit", dl.LEFT), (-3, 0))
        self.assertEqual(dl.motion("c1_hit", dl.FRONT), (0, 3))
        self.assertEqual(dl.motion("c1_hit", dl.BACK), (0, -3))
        self.assertEqual(dl.motion("c2_leap", dl.RIGHT), (1, -5))
        self.assertEqual(dl.motion("stand", dl.RIGHT), (0, 0))

    def test_the_sheet_fits_the_offset_table_with_compressed_demon_frames(self):
        """악마 프레임은 압축 IDAT · 사람 자리(0..8)는 무압축 — 오프셋은 u32(항목 +2, ★ 2026-10-14: 64 KB 한도 없음)."""
        m = dl.members()
        mph, mpd = m[dl.BODY_MEMBERS[0]], m[dl.BODY_MEMBERS[1]]
        n = int.from_bytes(mph[4:8], "big")
        self.assertEqual(n, dl.BODY_BASE + dl.FRAMES)
        self.assertTrue(all(mph[8 + 8 * k: 10 + 8 * k] == bytes(2) for k in range(n)), ".mpd file 0 for every frame")
        offs = [int.from_bytes(mph[8 + 8 * k + 2: 8 + 8 * k + 6], "big") for k in range(n)] + [len(mpd)]
        self.assertEqual(dl.last_frame_start(mph), offs[n - 1])
        for k in range(n):
            chunks = cs.walk_chunks(mpd, offs[k], offs[k + 1])
            idat = [d for t, d in chunks if t == b"IDAT"][0]
            if k < dl.COMPRESSED_FROM:
                self.assertEqual(idat[:3], b"\x78\x01\x01", f"frame {k}: the human slots stay stored (the game flips them)")
            else:
                self.assertNotEqual(idat[:2], b"\x78\x01", f"frame {k}: a demon frame is compressed")
        with self.assertRaises(cs.SheetError):
            cs.sheet_decode(mph, mpd, "strict")  # 압축 프레임을 받는다고 말하지 않으면 거절
        self.assertEqual(len(cs.sheet_decode(mph, mpd, "ok", compressed_from=dl.COMPRESSED_FROM).frames), n)

    def test_pose_sequences_are_the_drawers_contract(self):
        """★★ (5): 자세 시퀀스 — 길이(앞 여섯)는 DemonPatch::POSE_LENS, 돌진 · 숨쉬기 고리는 POSE_RUN_FRAMES · POSE_IDLE_FRAMES,
        종류 순서는 POSE_* — 게스트 DRAW 조각이 프레임을 그 틱으로 고른다."""
        src = open(os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "wie_android", "src", "guest_anchors.rs"), encoding="utf-8").read()
        m = re.search(r"pub const POSE_LENS: \[u8; 6\] = \[([^\]]*)\];", src)
        self.assertIsNotNone(m, "DemonPatch::POSE_LENS")
        self.assertEqual([int(v) for v in re.findall(r"\d+", m.group(1))], dl.POSE_LENS)
        consts = {k: int(v) for k, v in re.findall(r"pub const POSE_([A-Z_]+): u8 = (\d+);", src)}
        names = [n for n, _f in dl.CAST_SEQS]
        for name, kind in [("blade", "BLADE"), ("judge", "JUDGE"), ("fire", "FIRE"), ("lava", "LAVA"), ("follow", "FOLLOW"),
                           ("burst", "BURST"), ("run", "RUN"), ("idle", "IDLE")]:
            self.assertEqual(names.index(name), consts[kind], name)
        self.assertEqual(len(dl.CAST_SEQS), int(re.search(r"pub const POSE_SEQS: usize = (\d+);", src).group(1)))
        self.assertEqual(len(dict(dl.CAST_SEQS)["run"]), consts["RUN_FRAMES"])
        self.assertEqual(len(dict(dl.CAST_SEQS)["idle"]), consts["IDLE_FRAMES"])
        # the strikes land on the game's ticks: 어둠심판's fist up until its wind-up (20) · 멸화's roar from the detonation (10)
        judge, fire = dict(dl.CAST_SEQS)["judge"], dict(dl.CAST_SEQS)["fire"]
        self.assertEqual(judge.index("c2_hit"), 20)
        self.assertEqual(fire.index("roar"), 10)
        for _n, frames in dl.CAST_SEQS:
            self.assertTrue(all(p in dl.POSES + dl.EXTRA_POSES for p in frames))
        seq = dl.pose_seq(["blade_thrust", "stand"])
        self.assertEqual([[sp[0][:2] for sp in fr] for fr in seq],
                         [[[1, dl.frame_of("blade_thrust", d)] for d in range(4)], [[1, dl.frame_of("stand", d)] for d in range(4)]])
        ox, oy = dl.frame_offsets()[dl.frame_of("blade_thrust", dl.RIGHT) - dl.BODY_BASE]
        self.assertEqual(seq[0][dl.RIGHT][0][2:], [(ox + 2) & 0xFF, oy & 0xFF], "the thrust steps 2 px ahead")
        for cls in range(3):
            self.assertEqual(dl.pose_seq_no(cls, 0), dl.transform_seq_no(cls) + 1)
            self.assertLess(dl.pose_seq_no(cls, len(dl.CAST_SEQS) - 1), 64, "below the host's 64-entry sequence table")

    def test_transform_sequence_is_human_sprites_over_the_arc(self):
        stand = [[[[0, 2, 250, 231], [1, 2, 253, 244], [131, 7, 3, 250]] for _ in range(4)]]
        t = dl.transform_seq(stand)
        self.assertEqual(len(t), dl.TRANSFORM_FRAMES)
        for k, frame in enumerate(t):
            for sprites in frame:
                self.assertEqual([sp[:2] for sp in sprites], [[0, 2], [1, 2], [131, 7]])
                self.assertEqual(sprites[0][3], (231 - 256 + dl.TRANSFORM_HEIGHT[k]) & 0xFF)
        self.assertEqual(t[0][0][0][3], (-25 + 2) & 0xFF)
        self.assertEqual(t[12][0][0][3], (-25 - 26) & 0xFF)
        self.assertEqual(t[19][0][0][3], (-25) & 0xFF)

    def test_bsk_rows_are_checked_before_they_are_replaced(self):
        rows = [b""] * dl.BSK_SHIPPED_ROWS
        rows[18] = "대지분노".encode("euc-kr")
        rows[19] = "{지진@을 일으켜 적을 공격".encode("euc-kr")
        rows[7] = "}검은 기운@으로 범위내의 적을 공격".encode("euc-kr")  # ★ D3: 지옥칼날 · 어둠심판의 설명 줄
        rows[21] = "{번개@를 내리쳐 적을 공격".encode("euc-kr")
        rows[5] = "주인공 {HP@를 감소시켜 $SP@를 회복".encode("euc-kr")  # ★ D4: 추종 · 돌진의 설명 줄
        rows[25] = "}암흑@을 폭발시켜 적을 공격".encode("euc-kr")
        rows[17] = "{지옥의 불@을 소환하여 적을 공격".encode("euc-kr")  # ★ D5: 용암불 · 멸화의 설명 줄
        rows[23] = "{화염@을 일으켜 적을 공격".encode("euc-kr")
        out = dl.patch_bsk(rows)
        self.assertEqual(out[17].decode("euc-kr"), "{지옥의 불@을 소환하여 적을 공격;{악의@ 시 }용암불@")
        self.assertEqual(out[23].decode("euc-kr"), "{화염@을 일으켜 적을 공격;{악의@ 시 }멸화@")
        self.assertEqual(out[7].decode("euc-kr"), "}검은 기운@으로 범위내의 적을 공격;{악의@ 시 }지옥칼날@")
        self.assertEqual(out[21].decode("euc-kr"), "{번개@를 내리쳐 적을 공격;{악의@ 시 }어둠심판@")
        self.assertEqual(out[5].decode("euc-kr"), "주인공 {HP@를 감소시켜 $SP@를 회복;{악의@ 시 }추종@")
        self.assertEqual(out[25].decode("euc-kr"), "}암흑@을 폭발시켜 적을 공격;{악의@ 시 }돌진@")
        self.assertEqual(out[18], "악의".encode("euc-kr"))
        self.assertEqual(out[19].decode("euc-kr"), "잠시동안 {야수@에게;모든 주도권을;넘긴다.")
        # ★★ 2026-10-16: the 악의 versions after the shipped rows, name then description, in SKIN's order
        self.assertEqual(len(out), dl.BSK_ROWS)
        for j, (_kind, name, desc) in enumerate(demon_skin.SKIN):
            self.assertEqual(out[dl.BSK_SHIPPED_ROWS + 2 * j].decode("euc-kr"), name)
            self.assertEqual(out[dl.BSK_SHIPPED_ROWS + 2 * j + 1].decode("euc-kr"), desc)
        self.assertEqual([k for k, _n, _d in demon_skin.SKIN], [2, 12, 3, 10, 8, 11])
        rows[18] = "엉뚱".encode("euc-kr")
        with self.assertRaises(SystemExit):
            dl.patch_bsk(rows)

    def test_the_built_asset_carries_the_sheet_and_the_sequences(self):
        if not os.path.exists(cs.DEFAULT_ZIP):
            self.skipTest("no built asset")
        jar = cs.open_jar(cs.DEFAULT_ZIP)
        names = set(jar.namelist())
        if dl.BODY_MEMBERS[0] not in names:
            self.skipTest("the asset predates demon_look")
        try:
            sheet = cs.sheet_decode(jar.read(dl.BODY_MEMBERS[0]), jar.read(dl.BODY_MEMBERS[1]), dl.BODY_STEM, compressed_from=dl.COMPRESSED_FROM)
        except cs.SheetError:
            self.skipTest("the asset predates this demon sheet")
        if len(sheet.frames) != dl.BODY_BASE + dl.FRAMES:
            self.skipTest("the asset predates this demon sheet — rebuild it")
        self.assertTrue(all((f.w, f.h) == (1, 1) for f in sheet.frames[: dl.BODY_BASE]))
        for cls in range(3):
            _h, parts, seqs = venom_effect.decode(jar.read(f"hero/hero{cls}"))
            self.assertTrue(all(f < dl.COMPRESSED_FROM for f in parts[dl.BODY_DIR][0]), "the body flip list stays in the human slots")
            demon_end = dl.SHIPPED_SEQS[cls] + dl.MAPPED_COUNT + 1 + len(dl.CAST_SEQS)
            self.assertEqual(len(seqs), demon_end + AFTER_DEMON_SEQS[cls])
            if cls == 1:
                self.assertEqual(knight_iaido.REACH_SEQ, demon_end, "발도's big rings right after the demon copies")
            for k, (_name, frames) in enumerate(dl.CAST_SEQS):
                d = seqs[dl.pose_seq_no(cls, k)]
                self.assertEqual(len(d), len(frames))
                self.assertTrue(all(sp[0] & 0x7f not in (4, 6, 7) for fr in d for direction in fr for sp in direction))
            for i, s in enumerate(dl.MAPPED[cls]):
                d = seqs[dl.SHIPPED_SEQS[cls] + i]
                self.assertEqual(len(d), len(seqs[s]), f"hero{cls} seq {s}")
                if s in dl.CLAW_SEQS:
                    self.assertEqual(d, dl.claw_copy(cls, s - 4, seqs), f"hero{cls} claw stage {s - 4}")
                    continue
                for f, fr in enumerate(d):
                    for direction, sprites in enumerate(fr):
                        self.assertEqual(sprites, [sp for sp in seqs[s][f][direction] if sp[0] & 0x7f not in (6, 7)])
            t = seqs[dl.transform_seq_no(cls)]
            self.assertEqual(len(t), dl.TRANSFORM_FRAMES)
            for fr in t:
                for sprites in fr:
                    for sp in sprites:
                        if sp[0] & 0x80:
                            self.assertIn(sp[1], parts[sp[0] & 0x7F][0], "a mirrored sprite is in the flip list")
        rows = jar.read(dl.BSK_TDF)
        self.assertEqual(rows[0], dl.BSK_ROWS)


if __name__ == "__main__":
    unittest.main()
