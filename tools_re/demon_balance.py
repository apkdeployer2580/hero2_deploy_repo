#!/usr/bin/env python3
"""「악마화」(야수 데몬 계열 일곱 스킬 리뉴얼) 전수 밸런스 모델 (2026-10-13).

사용자(2026-10-13): *"야수스킬중 대지분노 스킬을 악의 스킬로 변경해줘 … 체1 의1 지1씩 올린 나이트의 악의 기본 공격의 공격력이
[올힘 버서커의 기본공격보다] 1.5배가량 … 공격속도는 버서커의 2배 … 콤보가 최대 3개 … 1단 6칸 할퀴기 2번, 2단 쓸어올리기 1번,
3단 5x5 내려찍기 … 버서커의 3콤보 넣는 시간의 1/2 … 악마화가 되면 암살자 → 지옥칼날 (7x7 토글, 0.11틱, 암살자 SP 1/4, 암살자
공격력의 70%) · 피의 계약 → 추종 (1분 버프, 악마게이지 100% = 이동 +50% 공속 +20% 흡혈 20%, 끝나면 맵 전체 폭발 + 기절) ·
지옥불 → 용암불 (5초 용암지대, 지옥불 데미지 그대로, 중첩) · 암흑폭발 → 돌진 (5초 이동 +50%, 3x3 밀치기, 암흑폭발 데미지) ·
학살화염 → 멸화 (0.5초 뒤 13x13) · 심판번개 → 어둠심판 (9x9 토글, 몬스터마다 0.22틱, 진입 즉시 타격) … 악의 중 일반 공격
스킬·궁극기 금지, 버프만 … 밸런스를 다시 전수 조사 … 힘2체1 버서커가 악의를 배웠을 때 … 허점"*

    python tools_re/demon_balance.py <hero2_ktf.zip> [--md HERO2_DEMON_BALANCE.md] [--claw-share 1.0 0.5] [--uptime 0.5]

식과 적·장비·판정은 `guardian_balance.py`(§6 의 결투) 그대로이고, 여기는 그 위에 **악마화 수치**를 얹는다. 모든 악마화 수치는
`DEMON` 표의 손잡이다 — 사용자가 「추후 밸런스 파악 후 조정」이라 한 값들. 실측이 아니라 **식**이다(실측 근거 = 머리말 아래).

── 확정 사실 (디스어셈블 · probe, HERO2_DEMON_PLAN.md §2) ───────────────────────────────────────────────────────
  기본공격 박자     stage 길이 = 시퀀스 kind+4 의 프레임 수(버서커 5·5·5·6 · 나이트 4·4·4 · 로그 4·5·3·4·8) + 단 사이 전환 1 프레임.
                    probe(숲 버서커, OK 매 프레임): 4 단 콤보 ≈ 24 게임 프레임 → **3 콤보 ≈ 17 프레임 = 0.85 s** (한 타 ≈ 5.7 프레임)
  버서커 공격력     a.h = (힘' + 장비힘) × (10 + a.p[14]) / 10 (패시브 3 → ×1.3) · 공격 = a.h + 무기 굴림 (`a.a(B)I`)
  나이트 공격력     a.h = 힘' × 7 / 10 · 지력은 공격에 **전혀** 안 든다 — 체1의1지1 나이트의 평타는 무기 굴림이 거의 전부
  피해             플레이어 타격 = (공격 − 몬스터 방어) × 직업 함수 × 치명 ×1.5 (방어는 한 타마다 전부 뺀다 — §8.61)
  야수 피해 (10-12) 링(지옥불·대지분노·빛의파동) `(bn.d + 30 + 2·지력')·(lv+3)/2` · 강공격(심판번개·학살화염·암흑폭발)
                    `(bn.d·5/2 + 20 + 2·지력')·(lv+2)/2` · 암살자 상태 3 틱 `(bn.d·27/10 + 20 + 2·지력')·lv` (한 번 걸면 틱 2)
                    → 전부 `bd.a(IBB)V` 가 **템플릿 방어를 빼고** 속성 /10 (맹독 틱처럼 `bd.a(IB)V` 로 직접 넣으면 방어 없음)
  야수 SP/쿨 (10-12) 암살자 45 / 64 틱 · 링 89−6lv / (14−lv)·8 · 강공격 115−4lv / (21−lv)·8  (lv 5: 59/72 · 95/128 틱)
  야수 형태 (b.bdf) 데몬 계열만 이 일곱을 든다: 피의계약|암살자 · 지옥불 · 대지분노 · (심판번개|암흑폭발|학살화염) — 한 형태에
                    피의계약과 암살자는 **함께 없다** → 추종과 지옥칼날은 한 야수에 공존하지 않는다
  받는 피해         악의 중 ×0.85 (사용자) — 철괴 · 가디언 옵션과 곱
"""

import argparse
import os
import statistics as st
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import guardian_balance as GB  # noqa: E402

FPS = 20.0

# ── 악마화 손잡이 (★ 전부 Claude 초안 — 사용자 「데미지는 추후 밸런스 파악 후 조정」) ─────────────────────────────
DEMON = dict(
    # 악의
    duration_s=60.0,        # 지속 (사용자 미지정 — 격분과 같이 1 분)
    cooldown_s=60.0,        # 끝난 뒤 쿨 (미지정)
    taken=0.85,             # 받는 피해 ×0.85 (사용자)
    # 할퀴기 공격력 = CLAW_SHARE × a.a()I + CLAW_INT × 지력'  →  d = 그것 − 방어 → 기본공격 단계로 타격(장비 기본공격 +% · 치명 적용)
    claw_share=0.5,         # 자기 공격력의 몫 (1.0 = 전부 — 힘 버서커 허점, 아래 표)
    claw_int=None,          # None = 아래 calibrate() 가 「체1의1지1 나이트 LV85 의 할퀴기 한 타 = 올힘 버서커 평타 × 1.5」로 푼다
    stage_mul=(1.0, 1.2, 1.5),   # 1 단 할퀴기(×2 타) · 2 단 쓸어올리기 · 3 단 내려찍기 5×5 — 사용자 미지정
    combo_frames=9,         # 3 콤보 = 2·2·3 프레임 + 전환 2 = 9 (버서커 17 의 ½ — 사용자 「1/2」)
    combo_hits=(2, 1, 1),   # 단마다 타수 (사용자)
    # 지옥칼날 (암살자 자리 · 악의 중 토글 · 7×7)
    blade_period_f=2,       # 0.11 s ≈ 2 프레임 (사용자 0.11)
    blade_tick_of_T=0.07,   # 한 틱 = 암살자 틱 T 의 몫 (사용자 「70 %」를 **초당** 70 % 로 읽음 = 2 프레임마다 7 %; 글자 그대로면 0.70)
    blade_sp_per_s=45 / 4,  # 암살자 SP 의 ¼ / 초 (사용자)
    blade_bypass_def=True,  # 맹독 틱처럼 방어 없이 (True 권장 — 7 % 틱은 방어에 먹힌다)
    # 어둠심판 (심판번개 자리 · 악의 중 토글 · 9×9 · 몬스터마다 자기 틱)
    judge_period_f=4,       # 0.22 s → 4 프레임(0.20) 또는 5(0.25) — 20 fps 라 0.22 는 없다
    judge_tick_of_H=0.20,   # 한 틱 = 강공격 H 의 몫 (사용자 미지정 — H 그대로면 5H/s = 과잉)
    judge_sp_per_s=95 / 8,  # (사용자 미지정) 심판번개 SP 의 ⅛ / 초
    judge_bypass_def=True,
    # 멸화 (학살화염 자리 · 13×13 한 방 · 0.5 s 선딜)
    fire_mul_of_H=2.0,      # 한 방 = 2H (학살화염 = 부모 + 흩뿌림 ≈ 한 마리에 2 타)
    # 돌진 (암흑폭발 자리 · 5 s · 3×3 경로 · 밀치기)
    charge_contacts=3,      # 한 번 돌진에 한 마리가 닿는 횟수(밀려나므로 3 으로 — 사용자 미지정)
    charge_mul_of_H=1.0,    # 닿을 때마다 H (사용자 「암흑폭발 데미지와 동일」)
    # 용암불 (지옥불 자리 · 칸마다 5 s 용암 · 지옥불 d 그대로)
    lava_period_f=20,       # 용암 재타격 주기 (사용자 미지정 — 1 초)
    lava_s=5.0,
    lava_share=0.5,         # 적이 용암 위에 서 있는 시간 몫 (걷는 적은 벗어난다 — 가정)
    lava_tick_of_R=1.0,     # 용암 한 틱 = 링 피해 R 의 몫 (사용자 「그대로」 = 1.0 · 권장 0.5)
    # 추종 (피의계약 자리 · 1 분 버프)
    follow_gauge_hits=25,   # 격분과 같은 25 타 (사용자 미지정)
    follow_aspd=1.20,
    follow_lifesteal=0.20,
    follow_blast_of_H=3.0,  # 끝의 어둠 폭발 = 3H (사용자 미지정) + 기절
)
LEVELS = (70, 80, 85, 90, 99)
BEAST_LV = 5  # 야수 스킬 레벨 (모델: 5)

# ★ 권장안 → ★★ 2026-10-13 사용자 결정 (HERO2_DEMON_PLAN.md §6 「★ 2026-10-13 결정」 — D5 에서 결정값으로 갱신): 할퀴기 = 공격력 몫 **1.0**
#   + 지력' × **4.73**(= LV 85 체1의1지1 나이트의 한 타가 올힘 버서커 평타 × 1.0; `DemonPatch::CLAW_INT_NUM/DEN`) · 콤보 = 17 프레임 + 3 단
#   뒤 회복 10 = 27 · 단 배율 1·1·1 · 장비 「기본공격 흡혈」은 **그대로**(사용자 ⑨ "그냥 냅두자") · 추종 흡혈 5 % · 용암 틱 ½ · 토글 SP =
#   최대 SP 의 6 % / 8 % 매초(⑦) · 지속 60 s · **쿨 90 s 는 켜진 시점부터**(④) → 끝난 뒤 30 s = 가동률 ⅔. `recommended()` 가 DEMON 위에 얹는다.
#   (옛 권장안 — 몫 ½ · 1/1.5 배 · 장비 흡혈 ¼ · SP 절대값 — 은 §1 의 보정표와 §3b 의 후보 격자에 비교행으로만 남는다.)
RECOMMENDED = dict(claw_share=1.0, claw_int=4.73, combo_frames=27, stage_mul=(1.0, 1.0, 1.0), gear_lifesteal_share=1.0, follow_lifesteal=0.05, lava_tick_of_R=0.5,
                   blade_sp_pct=0.12, blade_sp_floor=15, blade_tick_of_T=0.05, judge_sp_pct=0.08, duration_s=60.0, cooldown_s=30.0)
# ★★ 2026-10-16 (사용자: *"지옥칼날 SP소모량 올리고 … 데미지 낮추기"*): 지옥칼날 틱 T × 7 % → **5 %** · SP 최대의 6 % → **12 %/s, 최소 15/s**
#   (`DemonPatch::BLADE_PCT` 5 · `BLADE_SP_PCT` 12 · `BLADE_SP_MIN` 15) — 의지가 낮은 캐릭터가 자연 회복으로 무한히 켜 두던 것을 막는다.
UPTIME = RECOMMENDED["duration_s"] / (RECOMMENDED["duration_s"] + RECOMMENDED["cooldown_s"])

# ★ D5 (2026-10-13): what the probe measured on the LV 85 benchmark (`target/bench_dmn`: 숲 슬롯 2 → 체1의1지1 나이트 LV 85, 힘 10 · 지력 96 +
#   장비 지력, 야수 bn.d 26, 숲의 LV 22 몬스터 HP 30,000) against the block's own formulas — the model's constants are the block's constants, so
#   these agree to the integer wherever the formula is the whole story. §8 prints them. `None` = not measured yet.
MEASURED = dict(
    claw_hit=(508, "할퀴기 한 타 = a.a()I + 지력' × 4.73 − DEF (D1: 504~508 · 평타 50~54)"),
    blade_tick=(70, "지옥칼날 틱 = (bn.d·27/10 + 20 + 2·지력')·lv × 5 % (D3: bn.d 26 · 지력' 96 · lv 5 → 1410 × 0.05; ★★ 2026-10-16 7 % → 5 %, 실측 98 → 70)"),
    judge_tick=(193, "어둠심판 틱 = 강공격 × 20 % (D3)"),
    blast=(2907, "추종의 어둠 폭발 = 강공격 × 3 (D4: 969 × 3)"),
    charge=(969, "돌진 접촉 = 강공격 (D4)"),
    fire=(1938, "멸화 = 2H (D5: 13×13 안 네 마리 30,000 → 28,062 · SP −95 · 슬롯 카운터 128 = 학살화염의 것 · 선딜 10 틱)"),
    lava_tick=(496, "용암 틱 = d × ½, d = 지옥불 링의 992 = (bn.d + 30 + 2·지력')·(lv+3)/2 (D5: 12 칸 · 칸마다 1 s 에 496, 5 번 · ce 자신의 타 986 = d − DEF 6 은 그대로)"),
    claw_dps=(1619, "할퀴기만, OK 연타 · 한 마리 30,000 → 8,958 in 13 s (probe f 900→1680) · 3.0~3.2 타/s · 한 타 504~508"),
)


def recommended(cal=None, share=None):
    """The decided numbers on top of DEMON (`cal` · `share` are accepted for the older call sites and ignored)."""
    cfg = dict(DEMON, claw_share=RECOMMENDED["claw_share"], claw_int=RECOMMENDED["claw_int"], combo_frames=RECOMMENDED["combo_frames"], stage_mul=RECOMMENDED["stage_mul"],
               follow_lifesteal=RECOMMENDED["follow_lifesteal"], lava_tick_of_R=RECOMMENDED["lava_tick_of_R"], duration_s=RECOMMENDED["duration_s"], cooldown_s=RECOMMENDED["cooldown_s"],
               blade_sp_pct=RECOMMENDED["blade_sp_pct"], blade_sp_floor=RECOMMENDED["blade_sp_floor"], blade_tick_of_T=RECOMMENDED["blade_tick_of_T"],
               judge_sp_pct=RECOMMENDED["judge_sp_pct"])
    return cfg


def with_claw_lifesteal(p, share):
    """장비의 「기본공격 흡혈」을 할퀴기에 share 만큼만 — 복사본."""
    o = dict(p["opts"])
    o["lifesteal"] = p["opts"]["lifesteal"] * share
    q = dict(p)
    q["opts"] = o
    return q


def skill_int_power(intelligence):
    i = max(0, int(intelligence))
    return 1000 + (i - 200) * 5 if i > 200 else max(i + i * i // 50, 20 + 2 * i)


def skill_power(power, intelligence, attack):
    i = max(0, int(intelligence))
    return (int(power) + skill_int_power(i) * 85 // 100 + int(attack) * 30 // 100) * (600 + min(i, 200)) // 500


def demon_fields(p, g):
    """(hit, critx, DEF, INT', atk, H(강공격), T(암살자 틱), R(링 한 칸))."""
    L = g["L"] + 1
    Ls = GB.seen_level(L)
    hit = (100 - GB.clamp(p["ak"] + Ls + g["e"] / 5, 8, 90)) / 100
    crit = p["crit"] / 100 * GB.clamp(20 + Ls - p["level"], 8, 50) / 100
    critx = 1 + crit * (GB.CALIB["crit_mul"] - 1)
    INT = p["eff"][4]
    bd = p["level"]  # bn.d = 야수 레벨 = 플레이어 레벨로 센다 (guardian_balance 와 같은 가정)
    H = skill_power(bd * 5 // 2, INT, p["atk"]) * (BEAST_LV + 2) // 2
    T = skill_power(bd * 27 // 10, INT, p["atk"]) * BEAST_LV
    R = (bd + 30 + 2 * INT) * (BEAST_LV + 3) / 2
    return hit, critx, g["defence"], INT, p["atk"], H, T, R


def claw_atk(p, cfg):
    return cfg["claw_share"] * p["atk"] + cfg["claw_int"] * p["eff"][4]


def basic_hit_dps(p, g, frames_per_hit, mul=1.0):
    """일반 기본공격(한 타)의 DPS — 체인 박자로."""
    hit, critx, DEF, *_ = demon_fields(p, g)
    d = max(p["atk"] - DEF, 0) * mul * GB.CALIB["combo_avg"][p["cls"]] * (1 + p["opts"]["dmg"].get(0, 0))
    return d * hit * critx * FPS / frames_per_hit


def claw_combo_dps(p, g, cfg):
    """악의 3 콤보(한 대상이 전부 맞을 때)의 DPS 와 한 타(1 단 할퀴기) 피해."""
    hit, critx, DEF, *_ = demon_fields(p, g)
    one = max(claw_atk(p, cfg) - DEF, 0) * (1 + p["opts"]["dmg"].get(0, 0))
    per_combo = sum(h * m for h, m in zip(cfg["combo_hits"], cfg["stage_mul"])) * one
    return per_combo * hit * critx * FPS / cfg["combo_frames"], one * hit * critx


def beast_tick_dps(p, g, cfg):
    """악의 중 토글 둘(한 야수엔 둘 중 하나만 있지만 상한을 보려 둘 다) · 멸화 · 돌진 · 용암불의 DPS (한 마리 기준)."""
    hit, critx, DEF, INT, atk, H, T, R = demon_fields(p, g)
    x2 = 2 if p["opts"]["beast_x2"] else 1
    cut = lambda v, bypass: v if bypass else max(v - DEF, 0)
    blade = cut(T * cfg["blade_tick_of_T"], cfg["blade_bypass_def"]) * x2 * FPS / cfg["blade_period_f"]
    judge = cut(H * cfg["judge_tick_of_H"], cfg["judge_bypass_def"]) * x2 * FPS / cfg["judge_period_f"]
    heavy_cd = (21 - BEAST_LV) * 8 / FPS
    ring_cd = (14 - BEAST_LV) * 8 / FPS
    fire = max(H * cfg["fire_mul_of_H"] - DEF, 0) * x2 / heavy_cd
    charge = max(H * cfg["charge_mul_of_H"] - DEF, 0) * x2 * cfg["charge_contacts"] / (5.0 + heavy_cd)
    lava_ticks = cfg["lava_s"] * FPS / cfg["lava_period_f"] * cfg["lava_share"]
    lava_factor = max(100, 50 + int(INT) // 2) / 100 * (600 + min(max(int(INT), 0), 200)) / 500
    lava = (max(R - DEF, 0) + max(R * cfg["lava_tick_of_R"] * lava_factor - DEF, 0) * lava_ticks) * x2 / max(ring_cd, cfg["lava_s"])
    return dict(blade=blade, judge=judge, fire=fire, charge=charge, lava=lava, H=H, T=T, R=R)


def calibrate(items, gear, cfg, lv=85, share=None):
    """CLAW_INT: 체1의1지1 나이트 LV lv 의 할퀴기 한 타 − 방어 = 1.5 × (올힘 버서커 평타 − 방어), 방어 = 80 대 띠 평균."""
    share = cfg["claw_share"] if share is None else share
    b = GB.player(0, lv, items, None, slots=GB.LOADOUTS["버서커"][2], archetype=(1.0, 0, 0, 0, 0), opts=GB.gear_options(gear, list(GB.LOADOUTS["버서커"][2].values()), 0))
    k = GB.player(1, lv, items, None, slots=GB.LOADOUTS["나이트"][2], archetype=(0, 1 / 3, 0, 1 / 3, 1 / 3), opts=GB.gear_options(gear, list(GB.LOADOUTS["나이트"][2].values()), 1))
    DEF = 48.0
    target = 1.5 * (b["atk"] - DEF) + DEF
    m = (target - share * k["atk"]) / k["eff"][4]
    return m, b, k


def demon_duel(p, g, diff, cfg, uptime, toggles, beast_cd=1.0, fourth="fire"):
    """악의 나이트의 결투: 악의 중 = 할퀴기 콤보 + 토글(들) + 야수 쿨 스킬, 밖 = 나이트 평타(체1의1지1 = 약함) + 야수 쿨 스킬.
    `toggles` = 첫 칸의 토글/버프("blade" 지옥칼날 · "follow" 추종 · "judge" 어둠심판 — 넷째 칸이 심판번개일 때) · `fourth` = 넷째 칸이
    쿨 스킬일 때 "fire"(멸화) / "charge"(돌진) / 아니면 None(= 어둠심판 토글). 용암불(둘째 칸)은 데몬 계열 전부."""
    o = p["opts"]
    L = g["L"] + 1
    Ls = GB.seen_level(L)
    hp = (g["hp"] + 20) * diff  # ★★ 2026-10-16 monster HP is an int — no 32,767
    atk = min((g["atk"] + 3) * diff, 32767)
    claw_dps, one = claw_combo_dps(p, g, cfg)
    t = beast_tick_dps(p, g, cfg)
    beast_cd_dps = (t["lava"] + (t[fourth] if fourth in ("fire", "charge") else 0.0)) / beast_cd
    toggle_dps = sum(t.get(k, 0.0) for k in toggles)
    inside = claw_dps + toggle_dps + beast_cd_dps
    outside = basic_hit_dps(p, g, 5) + beast_cd_dps
    dps = uptime * inside + (1 - uptime) * outside
    taken = max(atk - p["defence"], 3) * o["taken"] * (uptime * cfg["taken"] + (1 - uptime))
    mcrit = GB.clamp(20 + Ls - p["level"], 8, 50) / 100
    evade = GB.clamp(p["am"] - L - g["e"] / 10, 8, 90) / 100
    mon_rate = FPS / GB.CALIB["monster_attack_frames"]
    taken_dps = taken * (1 + mcrit * (GB.CALIB["mon_crit_mul"] - 1)) * (1 - evade) * mon_rate
    leech = uptime * claw_dps * (o["lifesteal"] + (cfg["follow_lifesteal"] if "follow" in toggles else 0))
    kill_s = hp / dps if dps > 0 else float("inf")
    net = taken_dps - leech
    die_s = p["hp"] / net if net > 0 else float("inf")
    loss = max(net * kill_s - o["heal_hp"] * p["hp"], 0) / p["hp"] * 100
    # ★ D5: the decided toggles drain a share of max SP a second (user ⑦); the older absolute numbers when the config has none
    blade_sp = max(p["sp"] * cfg["blade_sp_pct"], cfg.get("blade_sp_floor", 0)) if "blade_sp_pct" in cfg else cfg["blade_sp_per_s"]
    judge_sp = p["sp"] * cfg["judge_sp_pct"] if "judge_sp_pct" in cfg else cfg["judge_sp_per_s"]
    sp_s = blade_sp * ("blade" in toggles) + judge_sp * ("judge" in toggles)
    return dict(kill_s=kill_s, die_s=die_s, margin=die_s / kill_s if kill_s else float("inf"), loss=loss, dps=dps, inside=inside, outside=outside, taken_dps=taken_dps, leech=leech, sp_s=sp_s, one=one, claw_dps=claw_dps, t=t)


def band_rows(rows, blo):
    lo, hi = GB.band_range(blo)
    return [(e, g) for e, h, g in rows if lo <= g["L"] <= hi and e["gate"] != 0]


def fmt(v):
    return "∞" if v == float("inf") or v >= 999 else f"{v:.0f}" if v >= 10 else f"{v:.1f}"


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("source")
    ap.add_argument("--md", default="HERO2_DEMON_BALANCE.md")
    ap.add_argument("--uptime", type=float, default=UPTIME)  # ★ D5: 60 s on, the cooldown 90 s from the cast → ⅔
    ap.add_argument("--anchors", default=os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "wie_android", "src", "guest_anchors.rs"))
    a = ap.parse_args()
    sys.stdout.reconfigure(encoding="utf-8")
    items = {(r["typeId"], r["index"]): r for r in GB.decode_all(a.source)}
    ens = [e for e in GB.enemies(a.source) if e["maps"]]
    rows = []
    for e in ens:
        h = GB.hell_template(e)
        rows.append((e, h, GB.guardian_template(h, e["gate"])))
    rows.sort(key=lambda r: (r[2]["L"], r[0]["index"]))
    gear = GB.parse_gear_items(a.anchors)
    KNT, BSK, RG = GB.LOADOUTS["나이트"][2], GB.LOADOUTS["버서커"][2], GB.LOADOUTS["로그"][2]
    opts_k = GB.gear_options(gear, list(KNT.values()), 1)
    opts_b = GB.gear_options(gear, list(BSK.values()), 0)
    opts_r = GB.gear_options(gear, list(RG.values()), 2)
    A111 = (0, 1 / 3, 0, 1 / 3, 1 / 3)
    out = []
    out.append("# 악마화(야수 데몬 계열 7 스킬 리뉴얼) 전수 밸런스 (모델 · 2026-10-13 · ★ D5 결정값으로 재생성) — 생성 `tools_re/demon_balance.py`\n")
    out.append("식 · 적 · 장비 · 판정은 `guardian_balance.py` §6 의 결투 그대로(가디언 모드 · 가디언 한 벌 + 개성 옵션 · 환경설정 난이도 · 개체 k=1). "
               "악마화 수치는 전부 `DEMON` 표(스크립트 머리말)의 손잡이 — ⚠ 「사양 그대로」 열은 Claude 가 사용자 지시를 읽은 것이고, "
               "**「결정안」 열은 2026-10-13 사용자 결정(`HERO2_DEMON_PLAN.md` §6 「★ 2026-10-13 결정」) = D1~D5 가 구현한 값**이다(`RECOMMENDED`). "
               f"가동률 = 지속 {RECOMMENDED['duration_s']:.0f} s / 쿨 90 s(켜진 시점부터) = {UPTIME:.0%}. 식의 상수는 블록의 상수라 §8 의 probe 실측과 정수까지 맞는다. 정본 사양 = `HERO2_DEMON_PLAN.md`.\n")
    # ── 1. 할퀴기 보정 ──
    out.append("## 1. 할퀴기 계수 — 「체1의1지1 나이트의 악의 할퀴기 한 타 = 올힘 버서커 평타 × 1.5」\n")
    out.append("할퀴기 공격력 = `CLAW_SHARE × 자기 공격력(a.a()I) + CLAW_INT × 지력'` → 게임의 타격 경로(`a.a(Lbd;BIBB)Z`, 기본공격 단계 종류)로 들어가 "
               "방어를 빼고 치명 ×1.5 · 장비 「기본공격 피해 +%」 · 흡혈이 걸린다. 보정 = LV 85 · 80 대 띠 방어 48 에서 한 타가 올힘 버서커 평타의 1.5 배(방어 뒤).\n")
    out.append("| CLAW_SHARE | 풀린 CLAW_INT | 나이트 체1의1지1 LV85 공격/지력' | 할퀴기 공격력 | 올힘 버서커 LV85 공격 |\n|---|---|---|---|---|")
    cal = {}
    for share in (1.0, 0.5, 0.0):
        m, b, k = calibrate(items, gear, DEMON, 85, share)
        cal[share] = m
        out.append(f"| {share} | **{m:.2f}** | {k['atk']:.0f} / {k['eff'][4]:.0f} | {share * k['atk'] + m * k['eff'][4]:.0f} | {b['atk']:.0f} |")
    out.append("")
    out.append("→ 지력 1 당 할퀴기 공격력 **≈ 8~10**(공격력 몫에 따라). 지력은 레벨당 +1 이라 할퀴기의 레벨 성장은 버서커 힘(레벨당 +3 × 1.3)보다 완만하다 — 아래 띠 표에서 LV 99 로 갈수록 비가 떨어진다.\n")
    # ── 2. 띠별 참조 피해 ──
    out.append("## 2. 띠별 한 타 · DPS — 올힘 버서커 평타 vs 악의 할퀴기 (CLAW_SHARE 0.5 · 1.0)\n")
    out.append("박자 = 체인 박자(OK 연타, 단 사이 전환 포함): 버서커 한 타 5.7 프레임 · 나이트 평타 5 · **악의 3 콤보 9 프레임 = 0.45 s 에 4 타**(1 단 2 + 2 단 1 + 3 단 1, 배율 1 · 1.2 · 1.5). "
               "한 대상이 네 타를 다 맞는다고 본다(앞 6 칸 · 5×5 가 앞 칸을 품는다). 방어 · 명중 · 치명은 그 띠의 평균 적.\n")
    out.append("| LV | 띠 | 버서커 올힘 평타 한 타 | 평타 DPS | 버서커 회전 DPS(§6 식) | 악의 한 타(0.5) | 콤보 DPS(0.5) | ÷평타 DPS | ÷회전 DPS | 악의 한 타(1.0) | 콤보 DPS(1.0) | ÷회전 DPS |\n|---|---|---|---|---|---|---|---|---|---|---|---|")
    for lv in LEVELS:
        blo = 70 if lv < 80 else 80 if lv < 90 else 90
        band = band_rows(rows, blo)
        b = GB.player(0, lv, items, None, slots=BSK, archetype=(1.0, 0, 0, 0, 0), opts=opts_b)
        k = GB.player(1, lv, items, None, slots=KNT, archetype=A111, opts=opts_k)
        bd = st.mean(basic_hit_dps(b, g, 5.7) for _, g in band)
        bone = st.mean(max(b["atk"] - g["defence"], 0) for _, g in band)
        brot = st.mean(GB.rotation_dps(b, g)[0] for _, g in band)
        cells = [f"| {lv} | {blo} 대 | {bone:.0f} | {bd:.0f} | {brot:.0f}"]
        for share in (0.5, 1.0):
            cfg = dict(DEMON, claw_share=share, claw_int=cal[share])
            cd = st.mean(claw_combo_dps(k, g, cfg)[0] for _, g in band)
            one = st.mean(max(claw_atk(k, cfg) - g["defence"], 0) for _, g in band)
            cells.append(f"{one:.0f} | {cd:.0f} | ×{cd / bd:.1f} | ×{cd / brot:.1f}" if share == 0.5 else f"{one:.0f} | {cd:.0f} | ×{cd / brot:.1f}")
        out.append(" | ".join(cells) + " |")
    out.append("")
    out.append("⛔ **사용자 사양 그대로면 단일 대상 DPS 가 버서커 평타의 ≈ 6~7 배, 버서커 스킬 회전의 ≈ 3~4 배다**(1.5 배 × 4 타 / 0.45 s). "
               "그것도 앞 6 칸 · 5×5 범위에 **전부**. 이것이 「악의 중 일반 스킬 금지」로 상쇄될 크기인지가 아래 3 의 판정이다.\n")
    # ── 3. 결투 — 악의 나이트 vs §6 직업 ──
    out.append(f"## 3. 결투 — 악의 나이트(체1의1지1) vs §6 의 직업들 (악의 가동률 {a.uptime:.0%} = 지속 {RECOMMENDED['duration_s']:.0f} s / 쿨 90 s 는 켜진 시점부터 = 끝난 뒤 {RECOMMENDED['cooldown_s']:.0f} s)\n")
    out.append("악의 안 = 할퀴기 콤보 + 토글 하나(야수가 가진 쪽: 지옥칼날 또는 어둠심판) + 쿨 스킬(멸화 · 돌진 · 용암불 중 야수가 가진 것 — 데몬 계열은 넷째 칸이 셋 중 하나라 멸화/돌진 중 하나 + 용암불) · "
               "악의 밖 = 체1의1지1 나이트 평타(공격력 ≈ 230 = 무기뿐) + 같은 야수 쿨 스킬. 받는 피해 = 악의 중 ×0.85. "
               "판정 기준은 §6 과 같다(여유 < 1 ❌ · < 2 어려움 · < 4 조금 어려움 · < 10 보통 · 그 위 쉬움). 비교 행 = §6b 의 버서커 · 나이트 · 로그 · 야수나이트(같은 모델에서 다시 계산).\n")
    out.append("야수 = 데몬 계열 최종형 셋: **레이위치**(암살자 → 지옥칼날 · 심판번개 → 어둠심판, 토글 둘) · **이프리트**(암살자 → 지옥칼날 · 학살화염 → 멸화) · **퓨리알**(피의계약 → 추종 · 학살화염 → 멸화). 용암불은 셋 다.\n")
    out.append("| 띠 | 난이도 | 버서커 | 나이트 | 로그 | 야수나이트 | 악의 나이트 **사양 그대로** (레이위치) | **결정안** 레이위치 | 결정안 이프리트 | 결정안 퓨리알 |\n|---|---|---|---|---|---|---|---|---|---|")
    base_rows = {}
    for name in ("버서커", "나이트", "로그", "야수나이트"):
        cls, arch, slots = GB.LOADOUTS[name]
        base_rows[name] = (cls, arch, slots, GB.gear_options(gear, list(slots.values()), cls), name.startswith("야수"))
    summary = []
    for blo, label in GB.BANDS:
        band = band_rows(rows, blo)
        for dname, diff in GB.DIFFICULTY:
            cells = [f"| {label.split()[0]} | {dname}"]
            for name, (cls, arch, slots, opts, beast) in base_rows.items():
                p = GB.player(cls, blo, items, None, slots=slots, archetype=arch, opts=opts)
                ds = [GB.duel(p, g, diff, beast) for _, g in band]
                w = min(d["margin"] for d in ds)
                la = st.mean(d["loss"] for d in ds)
                cells.append(f"×{fmt(w)} {GB.SHORT[GB.judge(w, la)]}")
            for variant, toggles, fourth in (("literal", ("blade", "judge"), None), ("rec", ("blade", "judge"), None), ("rec", ("blade",), "fire"), ("rec", ("follow",), "fire")):
                p = GB.player(1, blo, items, None, slots=KNT, archetype=A111, opts=opts_k)
                if variant == "literal":
                    cfg = dict(DEMON, claw_share=0.5, claw_int=cal[0.5])
                else:
                    cfg = recommended()
                    p = with_claw_lifesteal(p, RECOMMENDED["gear_lifesteal_share"])
                ds = [demon_duel(p, g, diff, cfg, a.uptime, toggles, fourth=fourth) for _, g in band]
                w = min(d["margin"] for d in ds)
                la = st.mean(d["loss"] for d in ds)
                ks = st.mean(min(d["kill_s"], 999) for d in ds)
                cells.append(f"×{fmt(w)} {GB.SHORT[GB.judge(w, la)]} ({ks:.1f} s)")
                summary.append((label, dname, variant, toggles, w, la, ks, st.mean(d["sp_s"] for d in ds)))
            out.append(" | ".join(cells) + " |")
    out.append("")
    out.append("⛔ **여유(margin)가 ∞ 로 나오는 이유** = 가디언 나이트 한 벌의 신발 「비호의 장화」(기본공격 흡혈 20 %)가 할퀴기(기본공격 단계)에 그대로 걸려 회복이 피격을 넘는다(5-③ · 5-④ 5). "
               "그래서 이 표에서 볼 것은 **처치 s** — 다른 직업 3~6 s 에 대해 악의 나이트 사양 그대로는 **0.6~0.8 s**(8 배), 결정안은 그 사이(「각성 창」의 몫 — 가동률은 ⅔).\n")
    out.append("### 3b. 후보 격자 — 한 타 배수 × 콤보 시간 (80 대 · 90 대 NORMAL · 90 대 HARD · 이프리트 = 지옥칼날 + 멸화 + 용암불 ½ · 장비 흡혈 ¼ · 추종 흡혈 5 %) → 처치 s · 여유\n")
    out.append("한 타 배수 = 「올힘 버서커 평타의 N 배」(사양 1.5) · 콤보 시간 = 3 콤보 네 타가 끝나기까지의 프레임(사양 9 = 버서커의 ½ · 17 = 버서커와 같음 · 27 = 17 + 3 단 뒤 회복 10 프레임). 단 배율은 1·1·1(사양 1·1.2·1.5 는 첫 행).\n")
    out.append("| 후보 | 80 대 NORMAL 처치 s · 여유 | 90 대 NORMAL | 90 대 HARD | 비교: 나이트 §6 (80 NORMAL) | 로그 §6 |\n|---|---|---|---|---|---|")
    cands = [("**사양 그대로** 1.5× · 9 f · 1/1.2/1.5", 1.0, 9, (1, 1.2, 1.5)), ("1.5× · 17 f · 1/1/1", 1.0, 17, (1, 1, 1)), ("1.5× · 27 f · 1/1/1", 1.0, 27, (1, 1, 1)),
             ("1.0× · 27 f · 1/1/1 (몫 ½ — 옛 권장)", 1 / 1.5, 27, (1, 1, 1)), ("1.0× · 17 f · 1/1/1", 1 / 1.5, 17, (1, 1, 1)), ("0.75× · 27 f · 1/1/1", 0.5, 27, (1, 1, 1)), ("0.5× · 17 f · 1/1/1", 1 / 3, 17, (1, 1, 1)),
             ("**결정안** 몫 1.0 + 4.73·지력' · 27 f · 1/1/1 · 장비 흡혈 그대로", None, 27, (1, 1, 1))]
    for label, fac, frames, sm in cands:
        cells = [f"| {label}"]
        for blo, diff in ((80, 1.5), (90, 1.5), (90, 2.0)):
            band = band_rows(rows, blo)
            if fac is None:  # ★ D5: the decision itself
                p = with_claw_lifesteal(GB.player(1, blo, items, None, slots=KNT, archetype=A111, opts=opts_k), RECOMMENDED["gear_lifesteal_share"])
                cfg = recommended()
            else:
                p = with_claw_lifesteal(GB.player(1, blo, items, None, slots=KNT, archetype=A111, opts=opts_k), 0.25)
                cfg = dict(DEMON, claw_share=0.5 * fac, claw_int=cal[0.5] * fac, combo_frames=frames, stage_mul=sm, follow_lifesteal=0.05)
            cfg["lava_tick_of_R"] = RECOMMENDED["lava_tick_of_R"]
            ds = [demon_duel(p, g, diff, cfg, a.uptime, ("blade",), fourth="fire") for _, g in band]
            w = min(d["margin"] for d in ds)
            ks = st.mean(min(d["kill_s"], 999) for d in ds)
            cells.append(f"{ks:.1f} s · ×{fmt(w)}")
        for name in ("나이트", "로그"):
            cls, arch, slots = GB.LOADOUTS[name]
            q = GB.player(cls, 80, items, None, slots=slots, archetype=arch, opts=GB.gear_options(gear, list(slots.values()), cls))
            ds = [GB.duel(q, g, 1.5, False) for _, g in band_rows(rows, 80)]
            cells.append(f"{st.mean(min(d['kill_s'], 999) for d in ds):.1f} s · ×{fmt(min(d['margin'] for d in ds))}")
        out.append(" | ".join(cells) + " |")
    out.append("")
    # ── 4. 토글 · 쿨 스킬 수치 ──
    out.append("## 4. 야수 스킬 쪽 — 사용자 수치 그대로 vs 권장 (LV 85 체1의1지1 나이트, 지력' ≈ 104, 80 대 띠 평균 방어 48 · HP 3,989)\n")
    k85 = GB.player(1, 85, items, None, slots=KNT, archetype=A111, opts=opts_k)
    band80 = band_rows(rows, 80)
    gavg = dict(L=85, hp=3989, atk=555, e=100, defence=48)
    hit, critx, DEF, INT, atk, H, T, R = demon_fields(k85, gavg)
    out.append(f"야수 식의 재료(LV 85 · 지력' {INT:.0f} · 야수 레벨 85 로): 강공격 H = **{H:.0f}** · 암살자 틱 T = **{T:.0f}** · 링 한 칸 R = **{R:.0f}** (방어 전; 방어 48 이면 −48).\n")
    out.append("| 스킬 | 사용자 사양 | 글자 그대로의 DPS(한 마리) | 권장 손잡이 | 권장 DPS | 가디언 80 대 적(HP 3,989) 처치 s | 비고 |\n|---|---|---|---|---|---|---|")
    lit_blade = T * 0.70 * FPS / 2
    rec_blade = T * DEMON["blade_tick_of_T"] * FPS / DEMON["blade_period_f"]
    out.append(f"| 지옥칼날 | 7×7 · 0.11 s 틱 · 암살자 공격력 70 % · SP 암살자 ¼/s | **{lit_blade:,.0f}** (틱 {T * 0.7:,.0f} × 10/s) | 틱 = T × {DEMON['blade_tick_of_T']} (= 초당 70 %) · 방어 없이(맹독 틱처럼) | {rec_blade:,.0f} | {3989 / rec_blade:.1f} s | 글자 그대로면 0.3 초에 몹이 녹는다(전 맵 7×7). 「70 %」를 초당으로 읽었다 |")
    lit_judge = H * FPS / 4
    rec_judge = H * DEMON["judge_tick_of_H"] * FPS / DEMON["judge_period_f"]
    out.append(f"| 어둠심판 | 9×9 · 몬스터마다 0.22 s 틱 · 진입 즉시 타격 · SP 지속 | **{lit_judge:,.0f}** (심판번개 H {H:,.0f} 를 0.2 s 마다) | 틱 = H × {DEMON['judge_tick_of_H']} · {DEMON['judge_period_f']} 프레임 · SP 심판번개 ⅛/s | {rec_judge:,.0f} | {3989 / rec_judge:.1f} s | 사용자가 틱 피해를 안 정했다 — H 그대로면 과잉 |")
    fire = H * DEMON["fire_mul_of_H"]
    out.append(f"| 멸화 | 13×13 · 0.5 s 선딜 · 지력 비례 | — (피해 미정) | 한 방 = 2H = **{fire:,.0f}**(방어 전) · 쿨 = 학살화염 {(21 - BEAST_LV) * 8} 틱 | {(fire - DEF) / ((21 - BEAST_LV) * 8 / FPS):,.0f} | {max(1, -(-3989 // (fire - DEF)))} 방 | 학살화염(부모 + 흩뿌림 1 ≈ 2H)과 같은 총량, 범위만 13×13 |")
    charge = H * DEMON["charge_mul_of_H"]
    out.append(f"| 돌진 | 5 s · 이동 +50 % · 3×3 밀치기 · 암흑폭발 피해 | 닿을 때마다 H = **{charge:,.0f}** | 닿는 횟수 ≈ 3(밀려나서) · 쿨 = 암흑폭발 {(21 - BEAST_LV) * 8} 틱 | {(charge - DEF) * 3 / (5 + (21 - BEAST_LV) * 8 / FPS):,.0f} | — | 밀치기 = 방패치기 넉백(`h2_acc_knock_unit`) 그대로 · 보스 안 밀림 |")
    lava_ticks = DEMON["lava_s"] * FPS / DEMON["lava_period_f"]
    out.append(f"| 용암불 | 지옥불 칸마다 5 s 용암 · 지옥불 피해 그대로 · 중첩 | 한 칸 위 적: 첫 타 R {R:,.0f} + 5 s 동안 용암 틱 R = **{(R - DEF) * (1 + lava_ticks) / 5:,.0f}/s** | 용암 틱 = R × ½ · 1 초마다 · 최대 {lava_ticks:.0f} 틱 · 같은 칸 겹침은 최신 것만 | 서 있으면 {((R - DEF) + (R / 2 - DEF) * lava_ticks) / 5:,.0f}/s | {max(1, -(-3989 // (R - DEF)))} 틱 | 12 칸(Lv5) × 6 타 = 한 번 시전에 최대 {12 * 6} 타 — 걷는 적은 벗어나지만 보스·기절은 못 벗어난다 |")
    blast = H * DEMON["follow_blast_of_H"]
    out.append(f"| 추종 | 1 분 · 게이지 100 % = 이동 +50 % · 공속 +20 % · 흡혈 20 % · 끝 = 맵 전체 폭발 + 기절 | 폭발 피해 미정 | 게이지 = 격분식 25 타 · 폭발 = 3H = **{blast:,.0f}** · 기절 2 s · 쿨 60 s | — | — | 흡혈 20 % × 할퀴기 콤보 DPS = 아래 5-③ |")
    out.append("")
    # ── 5. 허점 ──
    out.append("## 5. 허점 검사 (사용자: *\"혹시 허점이 있을지도 검사해줘. 예를들어 기본공격 셋팅을한 힘2체1 버서커가 악의를 배웠을때\"*)\n")
    out.append("### 5-① 다른 직업·다른 빌드가 악의를 배우면 — 할퀴기 한 타(LV 85 · 80 대 띠 평균 방어, 치명 포함 기대값)\n")
    out.append("| 빌드 | 공격력 | 지력' | 치명 % | CLAW_SHARE 1.0 한 타 | ÷ 체1의1지1 나이트 | CLAW_SHARE 0.5 한 타 | ÷ 나이트 | CLAW_SHARE 0 한 타 | ÷ 나이트 |\n|---|---|---|---|---|---|---|---|---|---|")
    builds = [
        ("나이트 체1의1지1 (기준)", 1, A111, KNT, opts_k),
        ("버서커 힘2체1", 0, (2 / 3, 1 / 3, 0, 0, 0), BSK, opts_b),
        ("버서커 올힘", 0, (1.0, 0, 0, 0, 0), BSK, opts_b),
        ("로그 민첩형 (§6 원형)", 2, GB.ARCHETYPE[2], RG, opts_r),
        ("나이트 기본형 힘6체3민1", 1, GB.ARCHETYPE[1], KNT, opts_k),
        ("나이트 지력 몰빵 (체0 의0 지3)", 1, (0, 0, 0, 0, 1.0), KNT, opts_k),
    ]
    ref = {}
    for share in (1.0, 0.5, 0.0):
        cfg = dict(DEMON, claw_share=share, claw_int=cal[share])
        p = GB.player(1, 85, items, None, slots=KNT, archetype=A111, opts=opts_k)
        ref[share] = st.mean(claw_combo_dps(p, g, cfg)[1] for _, g in band80)
    for name, cls, arch, slots, opts in builds:
        p = GB.player(cls, 85, items, None, slots=slots, archetype=arch, opts=opts)
        cells = [f"| {name} | {p['atk']:.0f} | {p['eff'][4]:.0f} | {p['crit']:.0f}"]
        for share in (1.0, 0.5, 0.0):
            cfg = dict(DEMON, claw_share=share, claw_int=cal[share])
            one = st.mean(claw_combo_dps(p, g, cfg)[1] for _, g in band80)
            cells.append(f"{one:,.0f} | ×{one / ref[share]:.2f}")
        out.append(" | ".join(cells) + " |")
    out.append("")
    ratio = {}
    for name, cls, arch, slots, opts in builds[:3]:
        p = GB.player(cls, 85, items, None, slots=slots, archetype=arch, opts=opts)
        for share in (1.0, 0.5):
            cfg = dict(DEMON, claw_share=share, claw_int=cal[share])
            ratio[(name, share)] = st.mean(claw_combo_dps(p, g, cfg)[1] for _, g in band80) / ref[share]
    out.append(f"→ **CLAW_SHARE 1.0 이면 올힘 버서커의 할퀴기가 지력 나이트의 ×{ratio[('버서커 올힘', 1.0)]:.2f}, 힘2체1 이 ×{ratio[('버서커 힘2체1', 1.0)]:.2f}** — 공격력이 그대로 들어가 "
               "1.5 배 + 범위 + 2 배 속도를 공짜로 받는다(사용자가 짚은 바로 그 허점). "
               f"**0.5 면 ×{ratio[('버서커 올힘', 0.5)]:.2f} · ×{ratio[('버서커 힘2체1', 0.5)]:.2f}**(힘 빌드가 악의를 켜면 자기 평타보다는 세지만 지력 빌드의 ⅔ 아래) · "
               "**0(공격력 무관)이면 지력 빌드만 쓴다**(힘 빌드 ×0.2). 로그는 치명 76 % 가 할퀴기에도 걸려(표의 기대값에 포함) 공격력 몫이 작아도 ×0.25~0.37 — 그리고 로그 「암살」(첫 타 +20..100 %)은 "
               "할퀴기(기본공격 단계)에도 걸린다. 지력 몰빵 나이트(체0 의0 지3)는 ×2.3~2.7 — HP 2,500 · SP 540 을 내는 값. 권장 = **0.5** + 아래 5-④ 1 의 직업 결정.\n")
    out.append("### 5-② 지력 적층 — 할퀴기 공격력 = 지력' × CLAW_INT 이라 지력 +1 = 공격력 +8~9\n")
    out.append("| 길 | 지력 | 근거 |\n|---|---|---|")
    out.append("| 레벨 (체1의1지1) | 1/레벨 → LV 85 = 96 | 기본 12 + 84 |")
    out.append("| 인장 「상급」 (지력의인장) | **+15~20** (무기 한 자루에 능력치 둘까지 — 지력 하나) | §8.65, 가디언 노말+ 1 % |")
    out.append("| 가디언 장신구 「야수조련 목걸이」 | +20 (+ 야수 피해 ×2 · 범위 ×2 — 토글·멸화·용암불 전부 ×2) | §8.42 |")
    out.append("| 가디언 방어구 랜덤 옵션 기대값 | 모델이 이미 센다(칸당 ≈ +2~4) | §6 |")
    out.append("| 세트 효과 | 다섯 칸 같은 세트 = 능력치 전부 +2 | §7 모델 밖 |")
    out.append("→ 현실적인 상한 ≈ 지력' 150 (LV 99 + 인장 + 목걸이) = 기준 104 의 **1.45 배 할퀴기** — 그리고 야수조련 목걸이면 토글 피해까지 ×2. 지력 몰빵(체0 의0 지3) 은 HP · SP 를 버리는 대신 할퀴기 ×1.4 (5-① 마지막 행). 상한은 선형이라 폭주는 없다.\n")
    out.append("### 5-③ 추종 100 % 흡혈 20 % + 할퀴기 콤보 = 받는 피해를 넘는 회복\n")
    out.append("| LV | 띠 · 난이도 | 할퀴기 콤보 DPS(0.5) | 흡혈/s (20 %) | 받는 피해/s (악의 ×0.85) | 회복 ÷ 피격 |\n|---|---|---|---|---|---|")
    for lv in (85, 99):
        blo = 80 if lv < 90 else 90
        band = band_rows(rows, blo)
        cfg = dict(DEMON, claw_share=0.5, claw_int=cal[0.5])
        p = GB.player(1, lv, items, None, slots=KNT, archetype=A111, opts=opts_k)
        for dname, diff in GB.DIFFICULTY:
            ds = [demon_duel(p, g, diff, cfg, 1.0, ("follow",)) for _, g in band]
            cd = st.mean(d["claw_dps"] for d in ds)
            lee = st.mean(d["leech"] for d in ds)
            tk = st.mean(d["taken_dps"] for d in ds)
            out.append(f"| {lv} | {blo} 대 · {dname} | {cd:,.0f} | {lee:,.0f} | {tk:,.0f} | ×{lee / tk:.1f} |")
    out.append("→ 한 마리를 상대로도 **흡혈이 피격을 몇 배 넘는다** = 추종 100 % 동안은 사실상 불사. 격분(10 %)도 같은 모양인데 격분은 버서커 평타(버서커 회전의 절반 DPS)에 10 % 라 작았다. 권장: 흡혈 20 % → **5 %**, 또는 흡혈을 한 타 상한(최대 HP 의 2 %)으로.\n")
    out.append("### 5-④ 그 밖의 허점 — 기계 쪽\n")
    out.append("| # | 허점 | 왜 생기나 | 막는 법 (계획서에 반영) |\n|---|---|---|---|")
    out.append("| 1 | 악의를 **버서커 · 로그**도 배운다 | 야수 스킬은 직업 무관 — 데몬 계열 야수면 누구나 대지분노 칸이 있다 | 악의의 사용 가능 = 나이트만? ⛔ 사용자 결정(5-① 이 수치). 직업 무관으로 두면 CLAW_SHARE 0.33 이하 + 치명 제외 |")
    out.append("| 2 | 악의 중 **버프 스킬**은 허용 → 철괴(−25 %) · 신의가호 + 악의(−15 %) 곱 · 버서커면 분노태세 +30 % 피해 · 로그면 맹독감염(평타 중독 2.0 배/0.5 s) + 암살 | 「일반 스킬은 버프기만」 = 사용자 규칙 | 받는 피해 곱은 그대로(철괴 25 % × 악의 15 % = 36 %) — 수용 가능. 맹독감염 + 할퀴기는 로그만의 것 = 허점 1 로 묶음 |")
    out.append("| 3 | 토글(지옥칼날 · 어둠심판)의 SP = 암살자 ¼/s ≈ 11/s — 의지 1/레벨 나이트는 SP 1,133 + 자연 회복 5 %/s(≈ 57/s) → **영구 가동** | SP 회복이 소모를 넘는다 | 토글의 SP 를 「초당 최대 SP 의 2 %」(≈ 23/s)로 — 그래도 자연 회복이 이긴다. 토글에 **지속 상한 20 s + 쿨**을 두거나, 자연 회복을 토글 중 멈춘다(권장) |")
    out.append("| 4 | 악의 자체가 **토글**이면 영구 악마 | 지속 미정 | 지속 60 s · 끝난 뒤 쿨 60 s (격분과 같은 꼴) — 사용자 결정 |")
    out.append("| 5 | 할퀴기가 **기본공격 단계**로 들어가면 SP 를 **얻는다**(#16 콤보 SP 획득) + 가디언 「기본공격 피해 +50 %」(독사의 장갑) · 「기본공격 흡혈 20 %」(비호의 장화) · 격분 카운터 전부 적용 | 사용자 「기본공격 올라가는 장비면 더 세져야」 = 의도 | 의도대로 둔다. 비호의 장화 20 % + 추종 20 % = 40 % 흡혈은 5-③ 의 두 배 → 추종 흡혈을 5 % 로 |")
    out.append("| 6 | 용암불 12 칸 × 6 타 = 한 번에 72 타 · 쿨 3.6 s · 지옥불 d ≈ 1,300 — **보스(안 움직임)에겐 초당 ≈ 2,600** | 용암 재타격이 방어만 뺀다 | 용암 틱을 R 의 ½ 로, 주기 1 s, 같은 칸 중첩은 **가장 최근 것만**(겹친 칸이 두 번 안 치게) |")
    out.append("| 7 | 어둠심판 「진입 즉시 타격」 + 돌진(이동 +50 %) = 걸어 다니며 전 맵 타격 | 사용자 사양 | 틱 피해를 H × 0.2 로(권장) — 그러면 9×9 에 들어온 적 하나당 즉시 1 타 = 0.2H = ≈ 300 |")
    out.append("| 8 | 멸화 13×13 ≈ 화면 전체(176 px = 11 칸) | 사용자 사양 | 피해 2H 한 방 = 학살화염 총량과 같다 — 범위만 넓다. 쿨이 같으니 수용 |")
    out.append("| 9 | 죽음 · 다시 불러오기 · 다른 슬롯 — 악마 상태 · 토글 · 모습이 남는다 | 블록이 캐릭터보다 오래 산다 | 격분의 OWNER 규율: 캐릭터 핸들이 바뀌면 전부 0 + 아바타 워드 통과 |")
    out.append("| 10 | 마을(비전투 맵)에서 토글 → SP 만 샌다 · 악의 시전 | `bf.a:Z` 0 | 야수 시전 자체가 전투 맵에서만(`a.f(B)V` 첫 검사) — 토글은 맵이 바뀌면 끈다(장신구 채널과 같이) |")
    out.append("| 11 | 추종 폭발 「기절」 = 보스는 안 걸린다(게임의 25 기절은 `bd` 만) | `a.a(Lbd;BI)V` 25 → `m.b(I)V` | 보스는 피해만 — 명시 |")
    out.append("| 12 | 퀵슬롯 값 10+k 는 야수 칸 — 악의 중 「일반 스킬 금지」는 `a.d(B)Z`(값 1..9) 래퍼로 막고 야수 값은 안 막는다 → 토글 전부 가능 | 라우팅 `n.a(I)V` | 의도 |")
    out.append("")
    # ── 6. 결정안 (★ D5: 2026-10-13 사용자 결정 = 구현값; 옛 권장은 괄호) ──
    out.append("## 6. 결정안 (2026-10-13 사용자 결정 = D1~D5 가 구현한 값; 괄호 = 그 전의 Claude 권장)\n")
    out.append("| 손잡이 | 사용자 사양 | 결정 (구현) | 근거 · 자리 |\n|---|---|---|---|")
    out.append(f"| 할퀴기 공격력 | 평타 1.5 배(체1의1지1 나이트 vs 올힘 버서커) | `1.0 × 공격력 + 4.73 × 지력'` = 올힘 버서커 평타 × **1.0** (권장은 몫 ½ · {cal[0.5] / 1.5:.1f}) | ① *\"공격력 안줄여도돼. 힘버서커도 쓸수있어\"* · ② 1.0 배 — `DemonPatch::CLAW_INT_NUM` 473 |")
    out.append("| 콤보 | 1 단 2 타 · 2 단 1 타 · 3 단 5×5 · 시간 ½ | 네 타 = 17 프레임(사람 시퀀스 길이 그대로) **+ 3 단 뒤 회복 10 틱**(잠금) · 단 배율 1·1·1 | ③ — `RECOVER` 10 · `HIT_FRAMES` |")
    out.append(f"| 악의 지속/쿨 | 미정 | **60 s · 쿨 90 s 는 켜진 시점부터**(끝난 뒤 30 s) · SP 최대 ¼ | ④ *\"쿨타임 템 맞추면 무한 지속 가능해\"* — 가동률 {UPTIME:.0%} (권장은 60/60 = 50 %) |")
    out.append("| 지옥칼날 | 0.11 s 틱 · 암살자 70 % | 2 틱마다 T × **5 %** (= 초당 50 %) · 방어 없이 · SP = **최대 SP 의 12 %/s · 최소 15/s**(첫 1 초 선불) | ⑤ b · ⑦ b · ★★ 2026-10-16 사용자 「SP 소모 올리고 데미지 낮추기」(전 7 % · 6 %/s) |")
    out.append("| 어둠심판 | 0.22 s · 몬스터마다 | 4 틱마다 H × 20 % · 방어 없이 · 진입 즉시 1 틱 · 선딜 1 s · SP = **최대 SP 의 8 %/s** | ⑥ a · ⑦ b |")
    out.append("| 토글 공통 | SP 지속 | 자연 회복 그대로(권장의 「정지」 폐기) · 토글마다 퀵바 1 초 잠금 · 악의가 끝나면 꺼짐 | ⑦ b |")
    out.append("| 멸화 | 13×13 · 0.5 s | **2H 한 방**(방어 없이, `bd.a(IB)V`) · 선딜 10 틱 · 쿨 · SP = 학살화염의 것 | ⑪ a — `FIRE_MUL` 2 · `FIRE_WINDUP` 10 |")
    out.append("| 돌진 | 5 s · +50 % · 3×3 · 암흑폭발 피해 | **15 s · 쿨 20 s**(★★ 2026-10-16 사용자 — 전 5 s · 야수 자신의 쿨) · H 닿을 때마다(방어 없이) · 같은 적 10 틱에 한 번 · 일반 · 폭 1 몬스터만 한 칸 밀기 · 이동 = 홀수 틱에 한 걸음 더 | §3.4 — `CHARGE_REHIT` 10 · `CHARGE_FRAMES` 300 · `CHARGE_COOLDOWN` 400 |")
    out.append("| 용암불 | 5 s · 지옥불 피해 · 중첩 | 지옥불은 그대로 + 칸마다 **용암 틱 = d × ½ · 1 s 마다 · 5 s** · 같은 칸은 최신 것만 | ⑩ a — `LAVA_PCT` 50 · `LAVA_EVERY` 20 · `LAVA_FRAMES` 100 |")
    out.append("| 추종 | 1 분 · 100 % = 이동 +50 % 공속 +20 % 흡혈 20 % · 폭발 + 기절 | 게이지 = 할퀴기 1 타 +4 % · 피격 +2 % · 할퀴기 외 피해 +2 %(★ 2026-10-16 사용자 — 전 25 타 준 타격만) · 흡혈 **5 %** · 폭발 3H(게이지 무관) · 기절 2 s(보스 제외) · 끝난 뒤 쿨 **30 s**(★ 2026-10-16 — 전 60 s) | ⑧ a · ★ 2026-10-16 |")
    out.append("| 장비 「기본공격 흡혈」(비호의 장화 20 % · 진군의 철화 10 %) | — | **그대로**(권장 ¼ 폐기) | ⑨ *\"그냥 냅두자\"* — §3 의 여유 ∞ 는 이것 |")
    out.append("| 악의의 직업 | 미정 | **직업 무관** (권장은 나이트만) | ① |")
    out.append("| 여섯 스킬의 조건 | — | **악의 중에만**(밖에선 배포본 야수 스킬) | ⑫ *\"악의가 풀리면 옛스킬로 돌아가야해\"* |")
    out.append("")
    # ── 7. 민감도 ──
    out.append("## 7. 민감도 — 결정안에서 손잡이 하나씩 (80 대 띠 · NORMAL · 악의 나이트 결정안 + 지옥칼날)\n")
    out.append("| 바꾼 것 | 여유 최소 | 처치 s 평균 | 판정 | DPS 악의 안 / 밖 |\n|---|---|---|---|---|")
    band = band_rows(rows, 80)
    p = with_claw_lifesteal(GB.player(1, 80, items, None, slots=KNT, archetype=A111, opts=opts_k), RECOMMENDED["gear_lifesteal_share"])
    rec = recommended()
    variants = [
        ("결정안 그대로", {}),
        ("2·3 단 배율 1.2 · 1.5 (사양)", dict(stage_mul=(1.0, 1.2, 1.5))),
        ("회복 없이 17 프레임", dict(combo_frames=17)),
        ("사양의 9 프레임", dict(combo_frames=9)),
        ("한 타 ×0.8", dict(claw_int=rec["claw_int"] * 0.8, claw_share=rec["claw_share"] * 0.8)),
        ("한 타 ×1.2", dict(claw_int=rec["claw_int"] * 1.2, claw_share=rec["claw_share"] * 1.2)),
        ("한 타 ×1.5 (사양)", dict(claw_int=rec["claw_int"] * 1.5, claw_share=rec["claw_share"] * 1.5)),
        ("장비 흡혈 ¼ (옛 권장)", dict(_lifesteal=0.25)),
        ("공격력 몫 ½ (옛 권장)", dict(claw_share=0.5)),
        ("지옥칼날 틱 T × 3.5 % (초당 35 %)", dict(blade_tick_of_T=0.035)),
        ("지옥칼날 글자 그대로 (틱마다 70 %)", dict(blade_tick_of_T=0.70)),
        ("받는 피해 ×1.0 (감소 없음)", dict(taken=1.0)),
        ("용암 틱 = R 그대로 (사양)", dict(lava_tick_of_R=1.0)),
        ("가동률 100 % (악의 토글)", dict()),
    ]
    for name, over in variants:
        cfg = dict(rec)
        cfg.update({k: v for k, v in over.items() if not k.startswith("_")})
        q = with_claw_lifesteal(p, over["_lifesteal"]) if "_lifesteal" in over else p
        up = 1.0 if "100 %" in name else a.uptime
        ds = [demon_duel(q, g, 1.5, cfg, up, ("blade",), fourth="fire") for _, g in band]
        w = min(d["margin"] for d in ds)
        la = st.mean(d["loss"] for d in ds)
        ks = st.mean(min(d["kill_s"], 999) for d in ds)
        ins = st.mean(d["inside"] for d in ds)
        outs = st.mean(d["outside"] for d in ds)
        out.append(f"| {name} | ×{fmt(w)} | {ks:.1f} | {GB.judge(w, la)} | {ins:,.0f} / {outs:,.0f} |")
    out.append("")
    out.append("모델 밖(§6 과 같다): 포션 · 부활의 구 · 보스 특수기 · 범위 타격의 「여러 마리」 몫(한 마리 결투라 **범위기의 이득은 전혀 안 센다** — 실전은 표보다 악의 쪽이 유리) · 기절 · 밀치기의 피격 회피 몫 · 야수 스킬 레벨 5 가정 · 야수 레벨 = 플레이어 레벨 가정.\n")
    # ── 8. ★ D5: probe 실측 vs 식 ──
    out.append("## 8. ★ D5 — probe 실측 vs 식 (2026-10-13, `target/bench_dmn`: LV 85 체1의1지1 나이트 · 숲 LV 22 몬스터 HP 30,000)\n")
    out.append("블록이 쓰는 상수가 이 모델의 상수다 — 한 타 · 한 틱은 식과 **정수까지** 맞아야 하고(안 맞으면 블록이나 모델 중 하나가 틀린 것), "
               "박자(초당 타수)는 게임의 프레임 규칙이라 실측으로만 안다. 벤치의 나이트는 LV 26 무기라 공격력 ≈ 60(가디언 한 벌의 230 이 아니다) — "
               "그래서 아래는 「식 ↔ 실측」의 일치이고, §3 의 절대값은 가디언 장비 기준이다.\n")
    out.append("| 항목 | 실측 (probe) | 식 | 비고 |\n|---|---|---|---|")
    bench = dict(atk=60, int=96, bd=26, lv=5, def_mob=6)
    H_b = (bench["bd"] * 5 // 2 + 20 + 2 * bench["int"]) * (bench["lv"] + 2) // 2
    T_b = (bench["bd"] * 27 // 10 + 20 + 2 * bench["int"]) * bench["lv"]
    rows8 = [
        ("할퀴기 한 타", MEASURED["claw_hit"], f"{bench['atk']} + {RECOMMENDED['claw_int']}·{bench['int']} − DEF ≈ {bench['atk'] + RECOMMENDED['claw_int'] * bench['int'] - bench['def_mob']:.0f}", "D1 — 지력' 96 · 평타 50~54 와의 차 454 = 96 × 4.73"),
        ("지옥칼날 틱", MEASURED["blade_tick"], f"T × 5 % = {T_b} × 0.05 ≈ {T_b * 5 // 100}", "D3 · ★★ 2026-10-16"),
        ("어둠심판 틱", MEASURED["judge_tick"], f"H × 20 % = {H_b} × 0.2 ≈ {H_b * 20 // 100}", "D3"),
        ("어둠 폭발", MEASURED["blast"], f"3H = {3 * H_b}", "D4 — 맵 전체 7 마리"),
        ("돌진 접촉", MEASURED["charge"], f"H = {H_b}", "D4 — 같은 적 10 틱에 한 번"),
        ("멸화 한 방", MEASURED["fire"], f"2H = {2 * H_b}", "D5 — 13×13"),
        ("용암 틱", MEASURED["lava_tick"], f"d × ½ = {(bench['bd'] + 30 + 2 * bench['int']) * (bench['lv'] + 3) // 2} × ½ = {(bench['bd'] + 30 + 2 * bench['int']) * (bench['lv'] + 3) // 2 // 2}", "D5 — 1 s 마다 · 5 번 · 같은 칸 최신 것만"),
        ("할퀴기 콤보 DPS (OK 연타)", MEASURED["claw_dps"], f"4 타 × {MEASURED['claw_hit'][0]} / (27 틱 / 20) ≈ {4 * MEASURED['claw_hit'][0] * 20 / 27:,.0f}/s (+ 치명 9 % × 1.5 ≈ {4 * MEASURED['claw_hit'][0] * 20 / 27 * 1.045:,.0f})", "D5 — 한 마리 HP 30,000 의 감소량 / 초 → 콤보 박자 17 + 회복 10 이 그대로 보인다(모델의 27 틱 ✓)"),
    ]
    for name, m, formula, note in rows8:
        if m is None:
            out.append(f"| {name} | — (미실측) | {formula} | {note} |")
        else:
            out.append(f"| {name} | **{m[0]:,}** | {formula} | {m[1]} · {note} |")
    out.append("")
    text = "\n".join(out) + "\n"
    with open(a.md, "w", encoding="utf-8", newline="\n") as f:
        f.write(text)
    print(f"wrote {a.md} ({len(rows)} enemies)")


if __name__ == "__main__":
    main()
