"""demon_rig.py 의 검사 — python -W ignore::ResourceWarning -m unittest discover -s tools_re -p test_demon_rig.py -v"""
import unittest

import numpy as np

import demon_draft as dd
import demon_rig as R


class DemonRigTest(unittest.TestCase):
    def test_rest_pose_gives_the_draft_back(self):
        """관절을 안 움직이면 초안 그대로 (확대 → 다수결 축소의 왕복 오차만, 칠한 픽셀의 6 % 미만)."""
        for name in R.SKELETONS:
            rig = R.Rig(name)
            a, s = rig.render(rig.joints()), dd.src(name)
            diff = int(np.any(a != s, axis=-1).sum())
            self.assertLess(diff, 0.06 * int((s[..., 3] > 0).sum()), name)

    def test_every_joint_sits_on_the_sprite(self):
        """관절은 그 자세의 그림 위(또는 1 px 안) — 격자에서 잘못 읽은 좌표를 잡는다."""
        for name, J in R.SKELETONS.items():
            op = dd.src(name)[..., 3] > 0
            grown = op.copy()
            grown[1:] |= op[:-1]
            grown[:-1] |= op[1:]
            grown[:, 1:] |= op[:, :-1]
            grown[:, :-1] |= op[:, 1:]
            for k, (x, y) in J.items():
                if k == "crown":  # 머리 뼈의 끝 — 앞모습에선 두 뿔 사이(빈 칸)일 수 있다
                    self.assertLess(y, J["neck"][1], name)
                    continue
                self.assertTrue(grown[int(y), int(x)], f"{name}.{k} = ({x}, {y}) is off the sprite")

    def test_reach_keeps_the_bone_lengths(self):
        rig = R.Rig("side_lunge")
        J = rig.joints()
        R.reach(rig, J, "n", (-4, 5), -1)
        l1 = np.hypot(*(rig.rest["el_n"] - rig.rest["sh_n"]))
        l2 = np.hypot(*(rig.rest["hand_n"] - rig.rest["el_n"]))
        self.assertAlmostEqual(np.hypot(*(J["el_n"] - J["sh_n"])), l1, places=3)
        self.assertAlmostEqual(np.hypot(*(J["hand_n"] - J["el_n"])), l2, places=3)
        np.testing.assert_allclose(J["hand_n"], J["sh_n"] + np.array([-4, 5]), atol=1e-6)

    def test_crouch_keeps_the_feet(self):
        rig = R.Rig("front_roar")
        J = rig.joints()
        R.crouch(rig, J, 3)
        for side in ("n", "f"):
            np.testing.assert_allclose(J[f"ft_{side}"], rig.rest[f"ft_{side}"])
        self.assertAlmostEqual(J["hip"][1] - rig.rest["hip"][1], 3)

    def test_every_pose_is_a_whole_figure(self):
        """사이 동작마다 그림이 한 덩어리이고 크기가 초안과 비슷하다 (뼈가 찢기거나 사라지지 않았다)."""
        import demon_look as dl
        T = dd.pose_table()
        for pose in dl.POSES:
            for view in (dd.FRONT, dd.RIGHT):
                a = T[(pose, view)]
                n = int((a[..., 3] > 0).sum())
                name = dd.RIG_POSES[pose][0 if view == dd.FRONT else 1][0]
                base = int((dd.src(name)[..., 3] > 0).sum())
                self.assertGreater(n, 0.8 * base, f"{pose}/{view}: {n} px vs the draft's {base}")
                self.assertLess(n, 1.2 * base, f"{pose}/{view}")
                self.assertEqual(len([c for c in dd._components(a[..., 3] > 0) if len(c[0]) > 12]), 1, f"{pose}/{view}: one figure")


if __name__ == "__main__":
    unittest.main()
