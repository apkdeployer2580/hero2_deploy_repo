악마 도트 — 「악의」의 외형 (몸통 시트 hero/1/300 의 프레임 9 + k). 만든 것: python tools_re/demon_draft.py build
예상도(docs/skill/astra_demon/ingame)의 악마를 픽셀째 오린 자세 8 장(demon_draft.SOURCES)이 정점이고, 사이 동작은 그 자세를
뼈대로 굽혀 만든다(tools_re/demon_rig.py · demon_draft.RIG_POSES). 캔버스 56 × 50 · 발바닥 (28, 46) · 255 색.
할퀴기 단마다 네 자세 = 그 단의 게임 프레임 0..3 (타격 = 0 단 f1 · f3 · 1 단 f2 · 2 단 f2 — DemonPatch::HIT_FRAMES).

프레임 번호 k (demon_<k>.png) = 자세 × 4 + 방향(0 뒤 · 1 앞 · 2 왼 · 3 오른), 쓰러짐 72..114 (앞만):
   0.. 3 stand
   4.. 7 walk0
   8..11 walk1
  12..15 walk2
  16..19 walk3
  20..23 c0_wind
  24..27 c0_hitA
  28..31 c0_windB
  32..35 c0_hitB
  36..39 c1_coil
  40..43 c1_dash
  44..47 c1_hit
  48..51 c1_follow
  52..55 c2_coil
  56..59 c2_leap
  60..63 c2_hit
  64..67 c2_impact
  68..71 roar
  72     fall0
  73     fall1
  74     fall2
   75.. 78 idle  (★ 숨쉬기 · 시전 · 달리기 — demon_look.CAST_SEQS)
   79.. 82 run0  (★ 숨쉬기 · 시전 · 달리기 — demon_look.CAST_SEQS)
   83.. 86 run1  (★ 숨쉬기 · 시전 · 달리기 — demon_look.CAST_SEQS)
   87.. 90 blade_wind  (★ 숨쉬기 · 시전 · 달리기 — demon_look.CAST_SEQS)
   91.. 94 blade_thrust  (★ 숨쉬기 · 시전 · 달리기 — demon_look.CAST_SEQS)
   95.. 98 judge_raise  (★ 숨쉬기 · 시전 · 달리기 — demon_look.CAST_SEQS)
   99..102 judge_up  (★ 숨쉬기 · 시전 · 달리기 — demon_look.CAST_SEQS)
  103..106 fire_gather  (★ 숨쉬기 · 시전 · 달리기 — demon_look.CAST_SEQS)
  107..110 lava_rise  (★ 숨쉬기 · 시전 · 달리기 — demon_look.CAST_SEQS)
  111..114 lava_slam  (★ 숨쉬기 · 시전 · 달리기 — demon_look.CAST_SEQS)
