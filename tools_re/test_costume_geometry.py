import os
import unittest

import costume_geometry as geometry
import costume_sheets as cs
import venom_effect


class CostumeGeometryTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.dirs = {cs.costume_number(d): d for d in cs.list_costumes()}
        cls.original = cs.open_jar(os.path.join(cs.ROOT, 'dist', '영웅서기2-1.zip'))

    def legs(self, number):
        return cs.load_part(self.dirs[number], 'leg')[0]

    def test_shipped_equipment_retains_every_pose_and_pixel(self):
        for stem in cs.shipped_stems(self.original, 'leg'):
            source = cs.shipped_sheet(self.original, 3, stem)
            expanded = geometry.duplicate(source)
            for i, f in enumerate(expanded.frames):
                self.assertEqual(f.pixels, source.frames[i % 11].pixels)
            self.assertEqual(expanded.palette, source.palette)

    def test_animation_preserves_position_flip_and_other_parts(self):
        for c in range(3):
            original = self.original.read(f'hero/hero{c}')
            changed = geometry.patch_animation(original)
            self.assertEqual(changed, geometry.patch_animation(changed))
            source = venom_effect.decode(original)[2]
            result = venom_effect.decode(changed)[2]
            for sa, sb in zip(source, result):
                for fa, fb in zip(sa, sb):
                    for d, (da, db) in enumerate(zip(fa, fb)):
                        head = next((s for s in da if s[0] & 127 == 0 and s[1] in (0, 1, 2)), None)
                        view = d if head is None else {0: 0, 2: 1}.get(head[1], 3 if head[0] & 128 else 2)
                        for a, b in zip(da, db):
                            if a[0] & 127 == 3 and a[1] != 255:
                                self.assertEqual(a[1], b[1] % 11)
                                self.assertEqual(a[0], b[0])
                                self.assertEqual(a[2:], b[2:])
                                bank = view + 1 if view < 2 or not a[0] & 128 else 6 - view
                                self.assertEqual(b[1] // 11, bank)
                            else:
                                self.assertEqual(a, b)

    def test_every_mirrored_sprite_has_its_flipped_copy(self):
        # ★ 2026-10-10: the part loader makes the mirrored copy cf.a[part + 8][f] only of the frames in the part's
        # flip list and the drawer skips a null image — vC 98 moved the legs to 11..54 with the list left at 0..10,
        # so every mirrored leg pose (walking up/down, standing facing up, facing right) was drawn without legs.
        for c in range(3):
            for blob in (self.original.read(f'hero/hero{c}'), geometry.patch_animation(self.original.read(f'hero/hero{c}'))):
                _, parts, seqs = venom_effect.decode(blob)
                for p, (flip_list, _second) in enumerate(parts):
                    self.assertLessEqual(geometry.flipped_frames(seqs, p), set(flip_list), (c, p))
                    self.assertLess(len(flip_list), 128, (c, p))
            _, parts, seqs = venom_effect.decode(geometry.patch_animation(self.original.read(f'hero/hero{c}')))
            self.assertTrue(set(parts[3][0]) and min(parts[3][0]) >= 11, c)

    def test_shoes_are_below_the_knee_in_every_pose(self):
        for n in (201, 207, 209, 210):
            sheet = geometry.footwear(self.legs(n), n)
            colors = set(geometry.MATERIALS[n]['shoe'][:2])
            for i, f in enumerate(sheet.frames):
                points = [(x, y) for y, row in enumerate(f.pixels) for x, v in enumerate(row) if v]
                top = min(y for x, y in points)
                shoes = [(x, y) for x, y in points if sheet.palette[f.pixels[y][x]] in colors]
                self.assertTrue(shoes, (n, i))
                self.assertTrue(all(y >= top + 5 for x, y in shoes), (n, i))

    def dressed(self, n):
        """[(bank, k, rows, top, panty cells, shoe colours)] of the set's dressed legs."""
        source = self.legs(n)
        sheet = geometry.footwear(source, n)
        shoe = set(geometry.MATERIALS[n].get('shoe', (None, None))[:2]) - {None}
        out = []
        for bank in range(geometry.BANKS):
            for k in range(geometry.STRIDE):
                f = source.frames[k]
                src = geometry.rows_of(source, k, f.w, f.h)
                top = min(y for y, row in enumerate(src) if any(c is not None for c in row))
                cells, string = geometry.panty_cells(src, top, geometry.BANK_VIEW[bank])
                rows = geometry.rows_of(sheet, bank * geometry.STRIDE + k, f.w, f.h)
                out.append((bank, k, rows, top, cells, shoe, string))
        return out

    def stocking_colors(self, n):
        """[(bank, k, [stocking colours])] — the leg pixels that are neither panty nor shoe."""
        return [(bank, k, [c for y, row in enumerate(rows) for x, c in enumerate(row)
                           if c is not None and (x, y) not in cells and c not in shoe])
                for bank, k, rows, top, cells, shoe, _string in self.dressed(n)]

    def test_white_stockings_are_white_with_skin_showing_through(self):
        # ★ 2026-10-10 (사용자): vC 98 was skin with a quarter of white — it read as bare legs. Now the white tights'
        # own shading with some skin under it: pale but warmer than the bare white ramp. ★ (3) 210 = more skin (45 %).
        limit = {201: .16, 209: .16, 210: .24}
        for n in (201, 209, 210):
            for bank, k, cols in self.stocking_colors(n):
                self.assertTrue(cols, (n, bank, k))
                sat = sum((max(c) - min(c)) / max(c) for c in cols) / len(cols)
                warmth = sum(c[0] - c[2] for c in cols) / len(cols)
                self.assertLess(sat, limit[n], (n, bank, k))
                self.assertGreater(warmth, 0, (n, bank, k))
                self.assertGreater(sum(sum(c) / 3 for c in cols) / len(cols), 150, (n, bank, k))

    def test_reaper_wears_white_heels_and_a_white_thong_under_warmer_stockings(self):
        # ★ 2026-10-10 (3) (사용자): 「백은의 사신 … 흰스타킹과 흰 구두 … 스타킹만 좀더 살색을 섞은 색으로 하고 구두와 팬티를 흰색으로」.
        warmth = lambda cols: sum(c[0] - c[2] for c in cols) / len(cols)
        sat = lambda cols: sum((max(c) - min(c)) / max(c) for c in cols) / len(cols)
        reaper = [c for _b, _k, cols in self.stocking_colors(210) for c in cols]
        elf = [c for _b, _k, cols in self.stocking_colors(209) for c in cols]
        self.assertGreater(warmth(reaper), warmth(elf) + 5)
        dark, light, rows = geometry.MATERIALS[210]['shoe']
        self.assertGreaterEqual(min(light), 240)
        self.assertEqual(rows, 2)
        base, band = geometry.MATERIALS[210]['panty']
        self.assertGreaterEqual(min(base), 240)
        for bank, k, rows_, top, cells, shoe, string in self.dressed(210):
            thong = [rows_[y][x] for x, y in cells if y > string]
            stock = [c for y, row in enumerate(rows_) for x, c in enumerate(row)
                     if c is not None and (x, y) not in cells and c not in shoe]
            self.assertTrue(thong and stock, (bank, k))
            self.assertLess(sat(thong), sat(stock), (bank, k))
            self.assertTrue(any(c == light for row in rows_ for c in row), (bank, k))

    def test_stockings_and_shoes_wear_a_thong_in_every_pose(self):
        # ★ 2026-10-10 (사용자): 「스타킹류와 구두류 아바타에 팬티가 보여야」 → (2) 「팬티가 너무 커 약간 t팬티처럼」 — the string
        # across the hip one row below the waistline, then only a narrow piece down the middle to the crotch.
        widest = {'front': 4, 'back': 2, 'side': 3}
        for n in (201, 207, 209, 210):
            base, band = geometry.MATERIALS[n]['panty']
            for bank, k, rows, top, cells, shoe, string in self.dressed(n):
                view = geometry.BANK_VIEW[bank]
                self.assertEqual(string, top + 1, (n, bank, k))
                self.assertEqual(len([x for x, y in cells if y == string]), sum(c is not None for c in rows[string]), (n, bank, k))
                below = [(x, y) for x, y in cells if y > string]
                self.assertTrue(below, (n, bank, k))
                for y in {y for x, y in below}:
                    self.assertLessEqual(len([1 for x, yy in below if yy == y]), widest[view], (n, bank, k, y))
                hips = sum(c is not None for row in rows[top:max(y for x, y in cells) + 1] for c in row)
                self.assertLess(len(cells), hips * .6, (n, bank, k))
                family = [c for ref in (base, band) for c in (ref, geometry.mix(ref, (0, 0, 0), .35), geometry.mix(ref, (255, 255, 255), .12))]
                family += [geometry.mix(c, (235, 230, 240), geometry.PANTY_UNDER_WHITE) for c in family]
                for x, y in cells:
                    c = rows[y][x]
                    near = min(sum(abs(a - b) for a, b in zip(c, ref)) for ref in family)
                    self.assertLess(near, 110, (n, bank, k, x, y, c))

    def test_side_panels_occlude_legs_and_differ_from_front(self):
        for n in (207, 210):
            parts = cs.costume_parts(self.dirs[n])
            under, over = cs.skirt_layers(self.dirs[n], parts)
            legs = geometry.footwear(self.legs(201), 201)
            sheet = geometry.garment(legs, under, over)
            rgba = cs.sheet_rgba(sheet)
            for k in range(11):
                self.assertNotEqual(rgba[k + 22][2], rgba[k + 33][2], (n, k))
                front = rgba[k + 22][2]
                side = rgba[k + 33][2]
                bare = cs.sheet_rgba(legs)[k][2]
                hidden = sum(b[3] and s != b and a == b for ar, sr, br in zip(front, side, bare)
                             for a, s, b in zip(ar, sr, br))
                self.assertGreater(hidden, 0, (n, k))

    def test_short_skirt_preserves_bare_feet(self):
        source = self.legs(208)
        result = geometry.short_skirt(source)
        for bank in range(5):
            for k in range(11):
                original = cs.sheet_rgba(source)[k][2]
                changed = cs.sheet_rgba(result)[bank * 11 + k][2]
                self.assertEqual(original[-1], changed[-1], (bank, k))

    def test_every_costume_and_combination_has_safe_directional_frames(self):
        members = cs.skirt_combo_members(list(self.dirs.values()))
        for directory in self.dirs.values():
            members.update(cs.costume_members(directory)[0])
        count = 0
        for name, data in members.items():
            if name.startswith('hero/3/') and name.endswith('.mph'):
                sheet = cs.sheet_decode(data, members[name[:-4] + '_0.mpd'], name)
                self.assertEqual(len(sheet.frames), 55, name)
                for k, f in enumerate(sheet.frames):
                    self.assertEqual((f.w, f.h), cs.PARTS['leg']['sizes'][k % 11], (name, k))
                count += 1
        self.assertEqual(count, 32)


if __name__ == '__main__':
    unittest.main()
