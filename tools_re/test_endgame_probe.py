import tempfile
import unittest
from pathlib import Path

import demon_balance as DB
from endgame_probe import parse


class BenchmarkTests(unittest.TestCase):
    def run_log(self, snapshots, single=True, health=None):
        lines = []
        for f, mobs in enumerate(snapshots, 1120):
            hp = health[f - 1120] if health else 100
            lines.append(f"[h2-bench] f={f} hp={hp} hpmax=100 sp=100 spmax=100")
            for actor, state, hp, maximum in mobs:
                lines.append(f"[h2-bench-mob] f={f} id={actor} template=7 state={state} hp={hp} max={maximum}")
        with tempfile.TemporaryDirectory() as work:
            log = Path(work) / "run.log"
            log.write_text("\n".join(lines), encoding="utf-8")
            return parse(log, 1120, len(snapshots) - 1, single)

    def test_hp_reinitialization_is_not_damage(self):
        r = self.run_log([[(1, 0, 400000, 400000)], [(1, 0, 6027, 6027)]])
        self.assertEqual(r["dmg"], 0)
        self.assertFalse(r["valid"])
        self.assertEqual(r["resets"], 1)

    def test_replacement_is_not_damage(self):
        r = self.run_log([[(1, 0, 400000, 400000)], [(2, 0, 6027, 6027)]])
        self.assertEqual(r["dmg"], 0)
        self.assertFalse(r["valid"])

    def test_reordering_does_not_mix_actor_health(self):
        r = self.run_log([[(1, 0, 100, 100), (2, 0, 200, 200)],
                          [(2, 0, 180, 200), (1, 0, 90, 100)]], single=False)
        self.assertEqual(r["dmg"], 30)
        self.assertTrue(r["valid"])

    def test_kills_accumulate_across_frames(self):
        r = self.run_log([[(1, 0, 100, 100), (2, 0, 200, 200)],
                          [(1, 6, 0, 100), (2, 0, 200, 200)],
                          [(2, 6, -50, 200)]], single=False)
        self.assertEqual((r["kills"], r["dmg"]), (2, 300))
        self.assertEqual(r["clear_s"], 0.04)

    def test_damage_stops_at_player_death(self):
        r = self.run_log([[(1, 0, 100, 100)], [(1, 0, 90, 100)], [(1, 0, 50, 100)]], health=[100, 0, 100])
        self.assertEqual(r["dmg"], 10)
        self.assertEqual(r["died"], 0.02)

    def test_rejects_unprepared_character(self):
        with self.assertRaises(ValueError):
            self.run_log([[(1, 0, 100, 100)], [(1, 0, 90, 100)]], health=[20, 10])

    def test_legacy_logs_cannot_be_used_as_new_results(self):
        with tempfile.TemporaryDirectory() as work:
            log = Path(work) / "old.log"
            log.write_text("[h2-mobs] f=1120 monsters=1 alive=1 dying=0 dead=0", encoding="utf-8")
            with self.assertRaises(ValueError):
                parse(log, 1120, 1)

    def test_demon_curve_matches_runtime_reference(self):
        self.assertEqual(DB.skill_power(55, 200, 500), 1688)
        self.assertEqual(DB.skill_power(55, 300, 500), 2368)
        self.assertEqual(DB.skill_int_power(600), 3000)


if __name__ == "__main__":
    unittest.main()
