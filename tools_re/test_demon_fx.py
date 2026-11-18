import pathlib
import re
import unittest

import numpy as np

import demon_fx as fx
import lava_fx


class DemonFxTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.sprites, cls.effects = fx.read_bin(fx.OUT_BIN)

    def test_binary_effect_ids_match_runtime(self):
        source = pathlib.Path(fx.ROOT, "wie_android", "src", "guest_anchors.rs").read_text(encoding="utf-8")
        for name, value in fx.CONTRACT.items():
            self.assertRegex(source, rf"pub const {name}: u8 = {value};")
        for name, value in fx.CONTRACT_U32.items():
            self.assertRegex(source, rf"pub const {name}: u32 = {value};")
        self.assertEqual([(name, len(frames)) for name, frames in self.effects], fx.EFFECTS)

    def test_hd_twins_and_the_body(self):
        import demon_look as dl
        import demon_upscale as du
        sprites, effects, twins = fx.read_bin_hd(fx.OUT_BIN)
        self.assertTrue(twins, "a v2 library carries twins")
        import malice_wings as mw
        body = effects[fx.FX_ID["demon_body"]][1]
        self.assertEqual(len(body), fx.WING_FRAMES)
        for k, parts in enumerate(body):
            # ★★ 2026-10-22 the WINGS: one part a frame at her shoulders (facing × flap phase), a 6× twin — the demon body is gone
            self.assertEqual(len(parts), 1, k)
            s, dx, dy, a, fl = parts[0]
            self.assertEqual((dx, dy, a, fl), mw.ROOT_AT[k // mw.FLAPS] + (255, 0), k)
            self.assertIn(s, twins, f"wing frame {k} has no twin")
            arr, ax, ay = sprites[s]
            tw, tax, tay, scale = twins[s]
            self.assertEqual(scale, mw.SCALE)
            self.assertEqual((tax, tay), (ax * scale, ay * scale))
            self.assertEqual(tw.shape[:2], (arr.shape[0] * scale, arr.shape[1] * scale))
            self.assertLessEqual(len({tuple(c) for c in tw[tw[..., 3] > 0][:, :4]}), 255)
            self.assertTrue(0 < ax < arr.shape[1] and 0 < ay < arr.shape[0], "the root is inside the picture")
        self.assertNotEqual(body[0][0][0], body[2][0][0], "the flap phases are different pictures")
        self.assertNotEqual(body[0][0][0], body[mw.FLAPS][0][0], "the facings are different pictures")
        for cut in range(4):
            for parts in effects[fx.FX_ID[f"claw{cut}_right"]][1]:
                for (s, dx, dy, a, fl) in parts:
                    self.assertIn(s, twins, f"claw{cut}: sprite {s} has no twin")
        # ★★ 2026-10-21 claws grow about the chest when the necklace draws them at 2× (bit 7), not stretched onto tiles.
        for c in range(4):
            for d, vert in (("up", 0), ("down", 0), ("left", 0), ("right", 0)):
                for parts in effects[fx.FX_ID[f"claw{c}_{d}"]][1]:
                    for (_, _, _, _, fl) in parts:
                        self.assertTrue(fl & 128, f"claw{c}_{d} leaves the body")
                        self.assertFalse(fl & 8, f"claw{c}_{d} is not stretched onto tiles")
                        self.assertEqual(fl & 0x30, vert, f"claw{c}_{d} direction")
                for parts in effects[fx.FX_ID[f"claw{c}_{d}_big"]][1]:
                    for (_, _, _, _, fl) in parts:
                        self.assertEqual(fl & 0x30, vert, f"claw{c}_{d}_big direction")
            seen = set()
            for parts in effects[fx.FX_ID[f"claw{c}_spin"]][1]:
                for (_, _, _, _, fl) in parts:
                    self.assertTrue(fl & 128, f"claw{c}_spin leaves the body")
                    self.assertFalse(fl & 8, f"claw{c}_spin is not stretched onto tiles")
                    seen.add(fl & 0x30)
            self.assertEqual(seen, {0}, f"claw{c}_spin has side, up and down")
        for k in range(12):
            for which, base in (("lava_w", "lava"), ("lava_wtop", "lava_top")):
                wide = effects[fx.FX_ID[f"{which}{k}"]][1]
                plain = effects[fx.FX_ID[f"{base}{k}"]][1]
                self.assertEqual([p[1:] for p in wide[30]], [p[1:] for p in plain[30]], f"{which}{k} stays on its tile")
        hd_px = sum(t[0].shape[0] * t[0].shape[1] for t in twins.values())
        # ★★ 2026-10-16: 40 M → 45 M — the 6× body is 4.3 M px (3× was 1.1 M); the rest of the library is 37.6 M
        self.assertLess(hd_px, 45_000_000, "the twins stay under the memory budget")

    def test_lava_flames_are_separate_from_the_floor(self):
        for piece in range(16):
            floor = self.effects[fx.FX_ID["lava0"] + piece][1]
            top = self.effects[fx.FX_ID["lava_top0"] + piece][1]
            for age in range(fx.LAVA_LEN):
                self.assertLessEqual(len(floor[age]), 1)
                if floor[age]:
                    self.assertNotIn(floor[age][0][0], [p[0] for p in top[age]])
            self.assertLess(floor[-1][0][3], floor[30][0][3])
            self.assertFalse(top[-1])
        self.assertTrue(any(self.effects[fx.FX_ID["lava_top0"] + k][1][30] for k in range(len(lava_fx.RING))))

    def test_lava_ring_is_the_runtimes(self):
        source = pathlib.Path(fx.ROOT, "wie_android", "src", "guest_anchors.rs").read_text(encoding="utf-8")
        body = re.search(r"pub const LAVA_RING: \[\(i8, i8\); 12\] = \[(.*?)\];", source).group(1)
        ring = [tuple(int(v) for v in pair.split(",")) for pair in re.findall(r"\((-?\d+, -?\d+)\)", body)]
        self.assertEqual(ring, lava_fx.RING)
        self.assertRegex(source, rf"pub const LAVA_PIECES: u8 = {len(lava_fx.RING) + lava_fx.SPARE};")

    def test_the_ring_pieces_draw_the_draft_field_whole(self):
        """고리 12 칸의 조각을 제 칸에 그리면 (나이 30 = 초안 11 그대로) 오린 지대 한 장 — 픽셀마다 한 조각만."""
        col, alpha, *_ = lava_fx.field()
        H, W = alpha.shape
        canvas = np.zeros((H, W, 3), np.float32)
        for k, (dx, dy) in enumerate(lava_fx.RING):
            x0, y0 = lava_fx.cell_tl(dx, dy)
            fx.draw_effect(canvas, self.sprites, self.effects, fx.FX_ID["lava0"] + k, 30, x0, y0)
        want = np.clip(col, 0, 255).astype(np.uint8).astype(np.float32) * (np.round(alpha * 255) / 255)[..., None]
        np.testing.assert_allclose(canvas, want, atol=2.5)

    def test_each_claw_left_frame_mirrors_right_about_anchor(self):
        for cut in range(4):
            for age in range(fx.CLAW_LEN):
                left = np.zeros((180, 241, 3), dtype=np.float32)
                right = left.copy()
                fx.draw_effect(left, self.sprites, self.effects, fx.FX_ID[f"claw{cut}_left"], age, 120, 110)
                fx.draw_effect(right, self.sprites, self.effects, fx.FX_ID[f"claw{cut}_right"], age, 120, 110)
                np.testing.assert_allclose(left, right[:, ::-1], atol=0.001)

    def test_finisher_rotates_with_vertical_facing(self):
        for age in range(fx.CLAW_LEN):
            def shape(direction):
                sid = self.effects[fx.FX_ID[f"claw3_{direction}"]][1][age][0][0]
                return self.sprites[sid][0].shape[:2]
            h, w = shape("right")
            self.assertEqual(shape("up"), (w, h))
            self.assertEqual(shape("down"), (w, h))


if __name__ == "__main__":
    unittest.main()
