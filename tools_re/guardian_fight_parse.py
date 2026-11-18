# hero2 가디언 밸런스 실측 — guardian_fight.sh 가 남긴 target/fight_<tag>.log 의 「한 마리뿐인 창」 분석 (2026-10-08).
#   usage: python tools_re/guardian_fight_parse.py <tag> [<tag> ...]
#   플레이어 타격(= atk − 27 ± 6 정상 · ×1.5 치명) · 피격(ATK×0.9..1.1 − DEF 정상 · ×2.5 치명) · 몬스터 상태 진입 수(t=3 = 공격).
import re,sys,statistics as S
for tag in sys.argv[1:]:
    L=open(f'target/fight_{tag}.log',encoding='utf-8',errors='replace').read().splitlines()
    mob={}; php={}; alive={}; st=None; mt={}
    for l in L:
        m=re.match(r'\[h2-mobs\] f=(\d+) .*alive=(\d+)',l)
        if m:
            f=int(m.group(1)); alive[f]=int(m.group(2))
            for mm in re.finditer(r'bd\((\d+),(\d+)\)@\(\d+,\d+\) t(\d+) w(-?\d+) (\d+)/(\d+) LV(\d+) atk(\d+)',l):
                if (mm.group(1),mm.group(2))==('16','17'): mob[f]=(int(mm.group(5)),int(mm.group(6)),int(mm.group(7)),int(mm.group(8))); mt[f]=(int(mm.group(3)),int(mm.group(4)))
        g=re.match(r'\[gauges\] f=(\d+) .*hp=Some\((\d+)\)/Some\((\d+)\).*state=(\d+)',l)
        if g: php[int(g.group(1))]=(int(g.group(2)),int(g.group(3)),int(g.group(4)))
        if l.startswith('[h2-stats] f=560'): st=l
    DEF=int(re.search(r'def=(-?\d+)',st).group(1)); ATKp=int(re.search(r' atk=(-?\d+)',st).group(1)); LV=int(re.search(r'lv=(\d+)',st).group(1))
    # window: from 572 to first frame alive==0 or player hp==0 or alive>1 after 600
    end=max(alive)
    for f in sorted(alive):
        if f>600 and (alive[f]!=1): end=f; break
    for f in sorted(php):
        if f>600 and php[f][0]==0: end=min(end,f); break
    fs=[f for f in sorted(mob) if 572<=f<=end]
    ATKm=mob[fs[0]][3]; LVm=mob[fs[0]][2]
    hits=[mob[a][0]-mob[b][0] for a,b in zip(fs,fs[1:]) if mob[b][0]<mob[a][0]]
    base=ATKp-27
    pn=[h for h in hits if abs(h-base)<=6]; pc=[h for h in hits if abs(h-base*1.5)<=8]; po=[h for h in hits if h not in pn and h not in pc]
    pf=[f for f in sorted(php) if 572<=f<=end]; taken=[php[a][0]-php[b][0] for a,b in zip(pf,pf[1:]) if php[b][0]<php[a][0]]
    lo,hi=ATKm*0.9-DEF-4, ATKm*1.1-DEF+4
    tn=[t for t in taken if lo<=t<=hi]; tc=[t for t in taken if lo*2.5-4<=t<=hi*2.5+4]; to=[t for t in taken if t not in tn and t not in tc]
    # monster attack-state entries
    ts=[(f,mt[f][0]) for f in fs]; hist={}
    for f,t in ts: hist[t]=hist.get(t,0)+1
    ent={}
    for (f0,a),(f1,b) in zip(ts,ts[1:]):
        if a!=b: ent[b]=ent.get(b,0)+1
    print(f'{tag}: LV{LV} atk{ATKp} def{DEF} vs LV{LVm} atk{ATKm}  window {fs[0]}..{end} ({end-fs[0]} fr)')
    print(f'   player hits {len(hits)}: normal {len(pn)} mean {S.mean(pn) if pn else 0:.1f} (atk-27={base}) crit {len(pc)} ({len(pc)/max(len(hits),1)*100:.0f}%) other {po}')
    print(f'   taken {len(taken)}: normal {len(tn)} mean {S.mean(tn) if tn else 0:.1f} [{lo:.0f}..{hi:.0f}] crit {len(tc)} ({len(tc)/max(len(taken),1)*100:.0f}%) other {to}')
    print(f'   mob state hist {hist} entries {ent}')
