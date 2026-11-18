# 악의 B 경로 — 연속 프레임 시트 받아 오기 (2026-10-14 밤)

사용자 결정(2026-10-14): **「A 먼저, B 오면 교체」**.

- **A (지금 들어가 있는 것)**: 예상도 12 장면(`../ingame`)에서 이펙트 · 악마를 픽셀째 오려 정점 프레임으로 쓰고(`tools_re/astra_cut.py`, `tools_re/demon_draft.py`), 그 사이 프레임은 같은 그림을 움직여 만든다(`tools_re/demon_fx_cut.py`). 초안에 한 장면만 있는 것은 그 장면이 정점이고, 초안에 없는 순간(나타남 · 사라짐 · 걷기 사이 · 뒷모습 · 쓰러짐)은 파생이다.
- **B (이 폴더)**: 같은 도구(ASTRA, 내장 image_gen)로 **프레임마다 그린 시트**를 받아 그 프레임을 그대로 쓴다. 시트 한 장 = 효과 하나(또는 악마 75 칸). 들이면 `costumes/astra_fx/frames/<이름>/` 에 저장되고, 다음 `python tools_re/demon_fx.py build`(이펙트) · `python tools_re/demon_draft.py build`(악마)부터 A 대신 쓰인다. 비운 칸은 A 그대로 남는다 — 일부만 받아도 된다.

## 받는 순서 (권장)

1. **악마 75 칸** — 지금 파생으로 채운 뒷모습 18 · 할퀴기 사이 동작(뼈대로 굽힌 것, `tools_re/demon_rig.py`) · 걷기 · 쓰러짐 3 이 가장 초안과 멀다.
2. **멸화 `fire_blast` · 변신 `transform` · 추종 폭발 `burst`** — 길이(12 · 12 · 10 장)가 길어 움직임 차이가 가장 크게 보인다.
3. 나머지 (할퀴기 네 컷 · 돌진 두 층 · 반구 · 인장 · 낫날 · 낙뢰 · 선딜 · 몬스터 피격 · 용암 질감 둘).

## 시트 규약 (모든 시트 공통)

- **배경 = 단색 `#00FF00`** — 그라데이션 · 그림자 · 비네트 · 바닥 없음. 이펙트에 순수 초록을 쓰지 않는다(빼는 색이다).
- **격자 = `templates/<이름>.png`** 를 그대로 입력 이미지로 준다. 칸 하나 = 프레임 하나, 왼쪽 위부터 줄 단위. 칸 테두리 · 번호 · 옅은 격자선 · 빨간 십자는 지워도 되고(들여오기가 무시하지 않으므로 **지우는 편이 낫다**) 칸 크기 · 위치만 지킨다.
- **빨간 십자 = 기준점** — 악마 · 몬스터의 발바닥(용암 질감은 칸 왼쪽 위). 그 자리에 발이 오도록, 칸 밖으로 나가지 않게.
- **배율 = 게임 1 px = 시트 3 px**, 가장 가까운 이웃 확대 그대로의 도트(안티앨리어싱 · 흐린 빛 번짐 금지). 들여오기가 격자 위상을 찾아 블록마다 대표색을 뽑는다 — 3 배가 아니어도 칸 크기에서 배율을 다시 잰다.
- **그림체 = `../ingame` 예상도와 같은 것**: 작은 남청색 악마(빨간 눈 · 두 뿔 · 검은 갈기 · 붉은 발톱), 효과는 4~6 색 계단 띠, 흰 하이라이트는 작게.
- 악마 본체는 **악마 시트에만** 그린다. 이펙트 시트에는 악마 · 몬스터 · 배경을 그리지 않는다(위치 참고가 필요하면 회색 실루엣을 넣었다가 지운다).

## 시트 목록

칸 크기 · 기준점은 게임 px(시트에서는 × 3). 프레임 수가 계약 길이보다 적으면 시간축으로 늘여 쓴다(예: 반구 8 장 → 16 그리기).

| 이름 | 프레임 · 열 | 칸 (w × h) | 기준점 | 내용 |
|---|---|---|---|---|
| `demon` | 75 · 4 | 56 × 50 | (28, 46) | 악마 — 18 자세 줄(stand · walk0..3 · c0_wind · c0_hitA · c0_windB · c0_hitB · c1_coil · c1_dash · c1_hit · c1_follow · c2_coil · c2_leap · c2_hit · c2_impact · roar) × 4 열(뒤 · 앞 · 왼 · 오른) + 마지막 줄 쓰러짐 3(앞만, fall0 웅크림 → fall1 무릎 → fall2 누움). 할퀴기 단마다 네 칸 = 그 단의 게임 프레임 0..3(타격 = 0 단 1 · 3 · 1 단 2 · 2 단 2). 알파는 불투명/투명 둘뿐 · 255 색 이하 |
| `claw0` | 6 · 6 | 104 × 88 | (12, 76) | 할퀴기 1 단 첫 손: 오른쪽을 보는 악마 앞을 ／ 로 긋는 붉은 참격 — 0 = 친 순간(가장 밝음) → 5 = 가늘어지며 사라짐 |
| `claw1` | 6 · 6 | 104 × 88 | (12, 76) | 둘째 손: ＼ 를 더해 X, 끝의 흰 접촉 섬광 · 튀는 검은 조각 |
| `claw2` | 6 · 6 | 104 × 96 | (12, 84) | 2 단: 아래에서 위로 쓸어 올리는 금빛 초승달 · 흙먼지 |
| `claw3` | 6 · 6 | 112 × 88 | (12, 76) | 3 단: 앞 땅으로 내리꽂는 세 줄 금빛 참격 · 땅의 금 · 흙먼지 · 튀는 돌 |
| `charge_ground_right` | 8 · 4 | 120 × 56 | (100, 44) | 돌진 땅 층(반복): 오른쪽으로 달리는 악마 **뒤의** 남청 잔상 · 붉은 속도선 · 흙먼지 |
| `charge_top_right` | 8 · 4 | 72 × 48 | (28, 44) | 돌진 위 층(반복): 머리 위를 긋는 붉은 호 · 앞쪽 충돌 섬광 |
| `follow_dome` | 8 · 4 | 112 × 96 | (56, 84) | 추종 100 %(반복): 악마를 감싸는 반투명 보라 반구 · 빛나는 테 · 안쪽 검은 결정 |
| `burst` | 10 · 5 | 144 × 112 | (72, 96) | 추종 폭발: 섬광 → 검은 결정 가시 부채가 솟음 → 부서져 흩어짐 |
| `burst_mob` | 6 · 6 | 48 × 56 | (24, 48) | 폭발에 맞은 몬스터 위의 분홍 섬광 · 작은 가시 |
| `seal` | 8 · 4 | 176 × 56 | (88, 32) | 지옥칼날 바닥 인장(반복): 붉은 마법진 · 룬, 빛이 고리를 따라 돈다 |
| `blade_rise0` | 8 · 8 | 40 × 72 | (8, 60) | 지옥칼날 낫날: 몬스터 오른쪽 땅에서 솟음(0..2) · 머묾(3..5) · 가라앉음(6..7) — 왼쪽 낫(`blade_rise1`)은 거울로 만든다 |
| `judge_orb` | 10 · 5 | 64 × 80 | (32, 76) | 어둠심판 선딜: 머리 위에 검은 구체가 응축 |
| `bolt0` | 4 · 4 | 56 × 144 | (28, 136) | 낙뢰: 하늘에서 몬스터 발밑으로 — 0 = 가장 밝음 → 3 = 사라짐 |
| `fire_core` · `fire_ground` | 10 · 5 | 96 × 80 | (48, 64) | 멸화 선딜 위 층(가슴의 붉은 핵) · 땅 층(발밑 붉은 빛 · 갈라짐) |
| `fire_blast` | 12 · 4 | 176 × 136 | (88, 120) | 멸화 폭발 위 층: 불기둥 무리 — **가운데 악마 몸 자리(대략 30 × 40)는 비운다**(악마는 이 층 밑에 그려진다) |
| `fire_mob` | 6 · 6 | 48 × 56 | (24, 48) | 멸화에 맞은 몬스터 위의 불꽃 |
| `fire_scorch` | 8 · 4 | 144 × 56 | (72, 28) | 멸화 땅 층: 그을린 땅 · 식는 금 · 잔불(첫 두 장은 악마 뒤의 불) |
| `transform` · `transform_ground` | 12 · 4 | 112 × 112 | (56, 92) | 변신 위 층(분홍 띠 · 그림자 · 모이는 조각 · 붉은 핵 → 출현의 보라 결정) · 땅 층(그림자 · 분홍 고리 → 흙먼지 고리 · 튀는 돌) |
| `lava_cool` · `lava_hot` | 1 · 1 | 64 × 64 | (0, 0) | 용암 질감: **이음매 없이 반복**되는 현무암 판 · 균열 — 식음(붉은 실금) · 끓음(같은 판 무늬, 주황 · 노랑 균열) |

## 프롬프트 (ASTRA — image_gen)

공통 머리말(모든 시트 앞에 붙인다):

> Use case: game sprite sheet for an existing Korean J2ME RPG (176×220 logical screen, 16 px tiles). Draw ONLY on the supplied grid template: one animation frame per cell, cells read left-to-right, top-to-bottom. Background of the whole sheet is a single flat pure green #00FF00 with no gradient, shadow, vignette, ground or texture; remove the template's grid lines, labels and red crosses but keep every cell's size and position. The red cross in each cell marks the anchor (the character's feet / the tile corner): place the effect relative to it and keep everything inside its cell. Pixel art at exactly 3 screen pixels per logical pixel, nearest-neighbor look, no anti-aliasing, no soft bloom, 4–6 colour stepped ramps, tiny white highlights. Match the art style, colours and scale of the second input image (the in-game mockup) exactly — it is the approved look. Never use pure green #00FF00 inside the art.

시트별 본문:

- **demon** — *Character sprite sheet of the SAME small navy-blue demon as in the in-game mockup (red eyes, two upright horns, black shaggy mane on the back, red claws, white chest mark, no weapon), about 26–34 logical px tall. Rows = poses: stand, walk0, walk1, walk2, walk3 (a 4-frame crouched walk cycle), then three 4-frame claw combos played at 20 fps — c0_wind (crouch, near claw cocked back), c0_hitA (lunge, near claw thrust forward), c0_windB (recoil, other claw raised), c0_hitB (second swipe); c1_coil (body pulled back low, coiled), c1_dash (springing forward, claw reaching), c1_hit (low four-legged sweep), c1_follow (claw swept through, leaning in); c2_coil (deep crouch), c2_leap (in the air, arm raised high behind), c2_hit (smashing the raised claw down in front), c2_impact (landed, crouched over the claw); roar (head up, arms spread); columns = facing back (away from the viewer), front (towards the viewer), left, right; last row = fall0 (hunched), fall1 (on one knee), fall2 (lying on the ground), front view only. Feet on the red cross in every cell. Fully opaque pixels only (no translucency).* 입력: `templates/demon.png` · `../ingame/02_charge_dark.png` · `../../../../costumes/demon_300/` 의 지금 75 장 격자(`python tools_re/demon_draft.py grid <png>` 로 만든 것).
- **claw0..3** — *Six-frame slash animation for one claw strike of the demon facing RIGHT (the demon itself is NOT drawn; its feet are at the red cross). Frame 0 is the impact (brightest, longest), frames 1–5 thin out and break apart; flying black chips with red rims drift outward and fall.* + 위 표의 내용. 입력: 템플릿 · `../ingame/01_transform_claw.png` · `../boards/02_claw_charge.png`.
- **charge_ground_right / charge_top_right** — *Looping 8-frame dash effect behind (ground layer) / above (top layer) the demon running to the RIGHT; the demon itself is not drawn.* 입력: 템플릿 · `../ingame/02_charge_dark.png`.
- **follow_dome · burst · burst_mob · seal · blade_rise0** — 위 표의 내용 그대로. 입력: 템플릿 · `../ingame/02_charge_dark.png` · `../boards/03_dark_arts.png`.
- **judge_orb · bolt0 · fire_* · lava_*** — 위 표의 내용 그대로. 입력: 템플릿 · `../ingame/03_judgment_fire_lava.png` · `../boards/03_dark_arts.png` 또는 `../boards/04_inferno_lava.png`. 용암 질감은 *seamlessly tileable 64×64 logical px texture, the same plates in both sheets* 를 덧붙인다.

## 들여오기 · 확인

```
python tools_re/astra_frames.py templates docs/skill/astra_demon/astra_frames/templates   # (다시) 템플릿
python tools_re/astra_frames.py import <받은 시트.png> burst                             # 표의 이름이면 칸 · 기준점 자동
python tools_re/astra_frames.py import <받은 시트.png> demon --demon                      # 악마 75 칸
python tools_re/astra_frames.py list
python tools_re/demon_draft.py build && python tools_re/demon_fx.py build                 # 들인 것부터 A 대신 쓰인다
python tools_re/demon_fx.py compare --out <dir> && python tools_re/demon_fx.py timeline --out <png>
```

들여오기의 정확도는 지금 라이브러리의 폭발 10 장을 3 배 시트로 그렸다가(배율 0.98 로 흔들어) 다시 들여 잰 값이 **픽셀당 평균 0.6 ~ 2.1 단계 / 255** 다. 지우려면 `costumes/astra_fx/frames/<이름>/` 을 지우면 A 로 돌아간다.
