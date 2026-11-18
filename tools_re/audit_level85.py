import hashlib
import json
import re
from collections import Counter
from pathlib import Path

import build_balance_survey as survey
import endgame_probe as probe
import skill_gear


ROOT = Path(__file__).resolve().parents[1]


def digest(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def main():
    source = ROOT / "wie_android/src/guest_anchors.rs"
    items = {(r["typeId"], r["index"]): r for r in survey.GB.decode_all(probe.ZIP)}
    selected = {k: v for k, v in items.items() if v.get("level") == 85}
    gear = survey.parse_gear(source)
    declared = int(re.search(r"pub const ITEMS: \[GearItem; (\d+)\]", source.read_text(encoding="utf-8")).group(1))
    assert len(gear) == declared
    assert set(gear) <= set(selected)
    special = {(3, 36): "RoguePatch::gear_words", (12, 24): "RoguePatch::gear_words"}
    assert set(selected) - set(gear) == set(special)
    model_keys = {k for _, _, slots, _, _ in survey.SETTINGS for k in slots.values()}
    measured_keys = {k for _, prefix, *_ in probe.PLANS for k in probe.setting(prefix)[2].values()}
    drops = survey.GB.drop_rates(probe.ZIP, items, selected)
    notes = {
        (6, 56): "확정 열세: 심연의 판금갑옷과 같은 기본 능력치. 반격 중에만 피해 -25%; 심연은 상시 -25%와 HP +20%.",
        (6, 60): "강한 범용 우위 위험: 장비 수×0.6%를 최종 피해에 곱함(2026-10-21 사용자 0.8 → 0.6%). 47개 약 +28.2%, 127개 약 +76.2%.",
        (6, 58): "중력 대비 평타 피해 열세: 같은 경갑 기본 능력치. 38개부터 중력의 최종 피해 증가가 기본공격 +30%를 넘음.",
        (6, 59): "현재 피해 +20%. 중력 대비 특화 피해 열세 위험: 같은 경갑 기본 능력치. 다른 가산 보너스가 없을 때 장비 26개부터 중력이 이를 넘음.",
        (5, 34): "역할 협소: 바람정령의 머리띠와 같은 기본 능력치. SP 5%p 추가 절약 대신 쿨감 10% 포기; SP 제한 회전에서 확인 필요.",
        (8, 53): "특화 효율 낮음 후보: 연환 대비 해당 스킬 +5%p와 방어 +5, 대신 다른 직업 스킬 +20% 포기. 엄밀한 하위호환은 아님.",
        (0, 34): "보스 단일전 처치 회복 비활성. 공격력은 오딘보다 높으므로 단순 하위호환 아님.",
        (12, 21): "소량 HP·힘·지력의 혼합 장신구. 용의 심장과 기초 능력치가 달라 단순 하위호환으로 판정하지 않음.",
        (12, 22): "게이지 90%/폭발 10초 효과는 실제 폭발을 사용하는 회전 필요. 기존 PLANS 공격에는 폭발이 없음.",
        (12, 20): "타격 시 1% 쿨 초기화가 기존 전투 모델에서 제외됨. 다타수·무리전에서의 가치를 SP -15%만으로 평가하면 안 됨.",
        (11, 20): "처치 시 1% 피해 감소 발동은 기존 모델에서 제외됨. 단일 보스와 연속 사냥의 효과가 다름.",
        (0, 37): "악의와 격분을 왕복하는 장기 회전 미검증.",
        (1, 31): "사방 범위·처치 회복을 이동/무리에서 실측해야 함.",
        (2, 32): "SP 소모 154/256=60.16% 유지로 설명 -40%와 일치. 신의가호 범위는 별도 조건.",
        (3, 40): "악의 맹독·중독 대상 조건을 실측해야 함.",
        (4, 36): "악의 중 둔기강타의 추가 행동 이점 별도 실측 필요.",
        (3, 35): "런타임 추가 타수는 양손 합산. 현재 모델도 합산으로 수정됨. 마지막 춤 착용 시 타수 이점 소멸; 응보의 +50%가 +30%보다 유리.",
        (3, 36): "효과 표 밖 RoguePatch 적용. 양손 추가 타수 합산: 백 번째 칼날과 +11타, 기본 포함 19타. 현재 모델 정상. 같은 단검 2개의 처치 연쇄 합산은 모델 미반영.",
        (12, 24): "효과 표 밖 RoguePatch 적용. 3타 고정×5이므로 단검 추가 타수와의 상충은 의도된 조합 제약.",
        (9, 58): "이동용. 고정 위치 DPS로 가치를 판정하면 안 됨.",
        (9, 61): "포션/이동용. 포션 없는 고정 위치 DPS로 가치를 판정하면 안 됨.",
    }
    rows = []
    for key, item in selected.items():
        expected = None
        if key in skill_gear.GUARDIAN_GEAR_LINES:
            expected = skill_gear.GUARDIAN_GEAR_LINES[key]
        elif key in skill_gear.ITEM_KEYS:
            expected = skill_gear.lines_of(skill_gear.ITEM_KEYS[key])
        norm = lambda s: re.sub(r"\s+", "", s)
        text_ok = None if expected is None else norm(item.get("desc", "")) == norm("".join(expected))
        rows.append({
            "key": list(key), "name": item["name"], "level": item["level"],
            "base_stats": {k: item[k] for k in ("power", "powerMin", "armourClass", "optionRanges", "refineMax", "slots", "reqHonor", "atk", "def", "hp", "sp") if k in item},
            "asset_description": item.get("desc", ""),
            "source_description_matches_asset": text_ok,
            "effects": gear.get(key, []), "special_handler": special.get(key),
            "in_existing_model_loadouts": key in model_keys,
            "in_existing_probe_loadouts": key in measured_keys,
            "guardian_drop_estimate": drops[key],
            "assessment": notes.get(key, "정적 수치·효과 경로 확인. 개별 성능 합격을 뜻하지 않으며 해당 조건의 실전 비교 필요."),
        })
    old = json.loads((ROOT / "tools_re/rebalance_2026-10-20.json").read_text(encoding="utf-8"))
    for left, right in [((6, 56), (6, 53)), ((6, 58), (6, 60)), ((6, 59), (6, 60))]:
        for field in ("power", "optionRanges", "armourClass", "refineMax", "slots", "reqHonor"):
            assert items[left][field] == items[right][field], (left, right, field)
    gravity47 = (47 * 8 * 256 + 500) // 1000
    damage_add = lambda key: sum(int(args[3]) for name, args in gear[key] if name == "Dmg" and args[:3] == ("2", "10", "10"))
    slaughter_other = 256 + sum(damage_add(k) for k in [(3, 35), (5, 36), (8, 58), (9, 59)])
    slaughter_armor = damage_add((6, 59))
    result = {
        "scope": "현재 배포 ZIP의 요구 레벨 85 장비 전체; 정적 전수 감사와 기존 실측 커버리지 감사",
        "sha256": {"asset": digest(probe.ZIP), "anchors": digest(source), "probe": digest(probe.PROBE)},
        "counts": {"items": len(rows), "gear_table": len(gear), "special_handlers": len(special),
                   "by_type": dict(Counter(k[0] for k in selected)),
                   "in_model": len(set(selected) & model_keys),
                   "in_probe_plans": len(set(selected) & measured_keys),
                   "asset_description_checks": sum(r["source_description_matches_asset"] is not None for r in rows),
                   "asset_description_mismatches": sum(r["source_description_matches_asset"] is False for r in rows)},
        "old_measurement_probe_matches_current": old["probe_sha256"] == digest(probe.PROBE),
        "conditional_comparisons": {
            "gravity_47_items_multiplier": 1 + gravity47 / 256,
            "slaughter_armor_swap_damage_ratio_at_47_items": slaughter_other * (1 + gravity47 / 256) / (slaughter_other + slaughter_armor),
            "slaughter_armor_swap_condition": "백 번째 칼날 + 무희 두건 + 칼춤 장갑 + 잔영의 신; 무희의 옷을 중력의 갑주로 교체. 같은 대상·회전의 살상연희 피해 배율 비교이며 전체 DPS 실측 아님.",
            "thorn_plate_vs_abyss_hp_ratio_when_no_other_hp_percent": (256 + 51) / 256,
        },
        "limitations": [
            "전 아이템의 독립적인 전투 실측 및 모든 조합 전수 측정은 아님.",
            "기존 모델은 LV99 기준이며 LV85 착용 시점 검증을 대체하지 못함.",
            "기존 PLANS는 15세팅, 고정 위치 숲 NORMAL, 1회 측정. 이동·9종 레이드·장기 회전·반복 변동 미검증.",
            "드랍 수치는 기존 해독 모델의 추정이며 조합 및 레이드 보상을 포함하지 않음.",
            "일반 세팅과 악의 세팅의 야수/버프 사용 정책이 달라 직업 순위로 해석하면 안 됨.",
            "살상연희 양손 추가 타수는 현재 모델에서 수정됨. 이전 보고서의 14타 계산 표는 여전히 과거 결과.",
            "무모한 칼춤 2개는 런타임 처치당 +4/최대16이나 모델은 +2/최대8로 덮어씀. 기본 포함 20타에서 실제 총 상한30, 모델28타.",
        ],
        "latest_validation": {
            "python_benchmark": "8 passed",
            "skill_gear_check": "passed",
            "hero2java_selftest": "compile failed: hero2java.rs:35457 CELLS expected array length 1, actual 2",
            "anchors_selftest": "80 passed, 2 failed: Java call/table count expectations 35 vs 37/36",
        },
        "items": rows,
    }
    fresh_path = ROOT / "tools_re/level85_probe_2026-10-20.json"
    swaps_path = ROOT / "tools_re/level85_swaps_2026-10-20.json"
    fresh = json.loads(fresh_path.read_text(encoding="utf-8")) if fresh_path.exists() else None
    swaps = json.loads(swaps_path.read_text(encoding="utf-8")) if swaps_path.exists() else None
    current_path = ROOT / "tools_re/level85_swaps_2026-10-21.json"
    current = json.loads(current_path.read_text(encoding="utf-8")) if current_path.exists() else None
    if current:
        result["current_comparison"] = {
            "file": str(current_path.relative_to(ROOT)),
            "builds": len(current["results"]),
            "equipment_count": current["equipment_count"],
            "probe_matches": current["probe_sha256"] == digest(probe.PROBE),
            "asset_matches": current["asset_sha256"] == digest(probe.ZIP),
            "sources_match": all(digest(ROOT / p) == h for p, h in current["source_sha256"].items()),
            "all_windows_valid": all(r["single"]["valid"] and r["pack"]["valid"] for r in current["results"]),
            "frozen_inputs": current.get("frozen_inputs"),
        }
    if fresh:
        result["fresh_measurement"] = {
            "file": str(fresh_path.relative_to(ROOT)),
            "builds": len(fresh["results"]),
            "complete": len(fresh["results"]) == len(probe.PLANS),
            "probe_matches": fresh["probe_sha256"] == digest(probe.PROBE),
            "all_windows_valid": all(s["valid"] and p["valid"] for _, s, p in fresh["results"]),
            "level": 99,
            "scope": "10월 20일 무희 +40/50/40 빌드의 과거 결과; 최신 소스 검증 아님",
        }
    if swaps:
        result["swap_measurement"] = {"file": str(swaps_path.relative_to(ROOT)), "builds": len(swaps["results"])}
    out = ROOT / "tools_re/level85_audit_2026-10-21.json"
    out.write_text(json.dumps(result, ensure_ascii=False, indent=2), encoding="utf-8")
    lines = [
        "85레벨 아이템 재감사 — 2026-10-21 (10월 20일 시작, 재개 후 최신 소스 재확인)",
        "",
        "판정: 전체 밸런싱 완료/합격으로 보기 어렵다.",
        "대상 91종 = 무기 28, 투구 12, 갑옷 14, 장갑 12, 신발 15, 방패 2, 장신구 8.",
        "91종의 데이터·효과 경로·기존 측정 커버리지를 전수 대조했다. 91종 개별 실전 성능을 모두 측정했다는 의미는 아니다.",
        "",
        "확정 문제",
        "1. 가시 판금: 같은 기본 능력치의 심연의 판금갑옷에 방어 성능이 밀린다. 반격 중 -25% 대 상시 -25% + 최대 HP 20%.",
        "2. 살상연희 과거 모델의 14타 오류는 현재 합산 19타로 수정됨. 단, 같은 무모한 칼춤 2개의 처치 연쇄는 런타임 +4/최대16과 모델 +2/최대8이 여전히 다름(총 타수 실제 상한30, 모델28).",
        "",
        "조건부 밸런스 위험",
        "3. 중력의 갑주: 47개 장비에서 최종 피해 ×1.375. 검은 비늘과 같은 경갑 기본 능력치이며 38개부터 평타 +30%를 넘어선다.",
        f"   백 번째 칼날/무희 두건/칼춤 장갑/잔영의 신 조합에서 무희의 옷을 중력으로 바꾸면 살상연희 피해 약 {(result['conditional_comparisons']['slaughter_armor_swap_damage_ratio_at_47_items'] - 1) * 100:.2f}% 증가.",
        "   이는 동일 대상·회전의 피해 계수 비교. 가방 공간의 기회비용이 있으므로 무조건 우위로 일반화하지 않는다.",
        "4. 집중의 관: 같은 기본 능력치의 바람정령 머리띠 대비 SP 약 5%p 추가 절약, 쿨감 약 10% 포기. SP 부족 상황 외에 선택 이유가 약함.",
        "5. 파쇄자의 손갑: 연환의 장갑 대비 특화 피해는 5%p 높고 방어는 5 높음. 다른 스킬 +20%를 포기하므로 특화 회전 재검증 필요.",
        "",
        "정상 확인",
        "89종은 GearPatch, 무모한 칼춤/마지막 춤 2종은 RoguePatch 처리. 효과 표에 없는 것을 누락 버그로 판단하지 않았다.",
        "배포 설명과 생성기 설명 88종 일치. 나머지 3종은 해당 생성기 설명표 밖의 1차 장신구.",
        "기존 드랍 해독 모델에서 91종 모두 가디언 드랍 후보 확인.",
        "10월 20일 감사 당시 Rust hero2java 422개 + anchor 82개, Python benchmark 8개 테스트 통과. 장비 생성기 --check 통과.",
        "재개 후 최신 hero2java 전체 테스트는 35457행의 아이콘 배열 기대값 길이 1/실제 2 불일치로 컴파일 실패. 밸런스와 별개인 테스트지만 최신 전체 통과로 보고할 수 없음.",
        "최신 anchor 테스트는 80개 통과, Java 호출/테이블 개수 기대값 2개 실패(35 대 37/36). Python 8개와 장비 생성기 검사는 재실행 통과.",
        "",
        "검증 범위의 공백",
        f"기존 모델 세팅에 포함된 장비 {result['counts']['in_model']}/91종. 기존 실측 계획에 포함된 장비 {result['counts']['in_probe_plans']}/91종.",
        *result["limitations"],
        "기존 결과 파일의 측정기 SHA256은 현재 실행 파일과 다르다.",
        "",
        "조정 우선순위",
        "가시 판금 역할 차별화 → 중력과 특화 갑옷 교체 비교 → 같은 단검 2개 처치 연쇄 모델 정정 → 집중/파쇄자 역할 검증.",
        "사용자가 정한 중력 0.6%(2026-10-21, 0.8%에서 하향)를 포함해 게임의 밸런스 수치는 이번 감사에서 변경하지 않았다.",
    ]
    if fresh:
        lines += [
            "", "10월 20일 변경 전 빌드의 15세팅 측정 — 무희 +40/50/40 시점. 현재 +20/20/20과 구분",
            "LV99, 제련 0, 고정 위치 숲 NORMAL, 세팅당 1회",
            "숫자는 이 시험 조건의 관찰값. 직업 순위 또는 레이드 통과 판정으로 일반화하지 않는다.",
            "세팅 | 단일 DPS | 단일 사망 시각(초) | 8마리 중 처치 수 | 무리 사망 시각(초) | 무리 전멸 시각(초) | 계측 유효",
        ]
        for name, single, pack in fresh["results"]:
            lines.append(f"{name} | {single['dps']:.1f} | {single['died']} | {pack['kills']}/{pack['targets']} | {pack['died']} | {pack['clear_s']} | {single['valid']}/{pack['valid']}")
    if fresh and swaps:
        base_results = {n: (s, p) for n, s, p in fresh["results"]}
        lines += ["", "변경 전 빌드의 갑옷 교체 실측 (현재 +20% 무희의 옷과 다름)"]
        for swap in swaps["results"]:
            s, p = swap["single"], swap["pack"]
            lines.append(f"{swap['label']}: 단일 DPS {s['dps']:.1f}, 단일 사망 {s['died']}초, 무리 {p['kills']}/{p['targets']}, 사망 {p['died']}초, 전멸 {p['clear_s']}초.")
            if swap["baseline"] in base_results:
                bs, bp = base_results[swap["baseline"]]
                lines.append(f"원래 세팅 대비 단일 DPS {s['dps'] / bs['dps']:.3f}배. 원래 무리 처치 {bp['kills']}, 사망 {bp['died']}초.")
            lines.append("사망 시점이 다르면 생존 구간 평균 DPS를 직접 우열로 해석하지 않는다. 원본 로그의 착용/능력치와 함께 판단.")
    if current:
        lines += ["", "최신 재비교 — 현재 무희 +20%, 양쪽 장비 47개 (가방 검사로 확인)"]
        for r in current["results"]:
            s, p = r["single"], r["pack"]
            lines.append(f"{r['label']}: 능력치 {s['stats']}, 단일 피해 {s['dmg']}, DPS {s['dps']:.1f}, 단일 사망 {s['died']}, 무리 처치 {p['kills']}/{p['targets']}, 전멸 {p['clear_s']}초.")
        if len(current["results"]) == 2:
            left, right = [r["single"] for r in current["results"]]
            lines.append(f"중력/무희 DPS 비 {right['dps'] / left['dps']:.4f}; 능력치 동일={left['stats'] == right['stats']}; 측정시간 동일={left['elapsed_s'] == right['elapsed_s']}.")
        lines.append("1회 비교이며 장비 수, 회전, 위치를 바꾸면 결과도 달라질 수 있다. 일반 사냥 전체 및 레이드 검증은 아님.")
        lines.append("최종 비교는 실행 파일과 게임 ZIP을 target/audit85_swaps_current에 복사해 고정한 입력으로 수행했다.")
    lines += ["", "91종별 명세와 검증 현황"]
    for row in rows:
        t, i = row["key"]
        lines += [
            "",
            f"[{t}/{i}] {row['name']}",
            f"효과: {row['effects'] or row['special_handler']}",
            f"기초: {json.dumps(row['base_stats'], ensure_ascii=False)}",
            f"기존 모델 포함={row['in_existing_model_loadouts']}; 기존 실측 세팅 포함={row['in_existing_probe_loadouts']}",
            row["assessment"],
        ]
    (ROOT / "HERO2_LEVEL85_AUDIT.txt").write_text("\n".join(lines) + "\n", encoding="utf-8")
    print(json.dumps({k: v for k, v in result.items() if k != "items"}, ensure_ascii=False, indent=2))
    for row in rows:
        if row["source_description_matches_asset"] is False:
            print("DESCRIPTION MISMATCH", row["key"], row["name"], repr(row["asset_description"]))
    print("No guardian drop:", [r["name"] for r in rows if r["guardian_drop_estimate"][1] == 0])


if __name__ == "__main__":
    import sys
    sys.stdout.reconfigure(encoding="utf-8")
    main()
