from pathlib import Path
from PIL import Image, ImageDraw, ImageFont

ROOT = Path(__file__).resolve().parents[1]
SOURCE = ROOT / 'target/astra_validation'
OUT = ROOT / 'docs/skill/astra_demon/applied'
SHOTS = [
    ('변신 · 응축', 'claw', 684), ('변신 · 출현', 'claw', 714),
    ('기본공격 · 할퀴기', 'claw', 807), ('돌진 · 이동', 'charge', 837),
    ('추종 · 최대 게이지', 'follow', 1299), ('추종 · 가시 폭발', 'follow', 1329),
    ('지옥칼날', 'blade', 822), ('어둠심판 · 구체', 'judge', 777),
    ('어둠심판 · 낙뢰', 'judge', 837), ('멸화 · 분출', 'fire', 792),
    ('용암불 · 지속', 'lava', 792), ('용암불 · 냉각', 'lava', 882),
]


def frame(scene, number):
    path = SOURCE / scene / f'frame_{number:05d}_176x220.rgba'
    return Image.frombytes('RGBA', (176, 220), path.read_bytes()).convert('RGB')


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    font = ImageFont.truetype('C:/Windows/Fonts/malgun.ttf', 19)
    sheet = Image.new('RGB', (4 * 368, 3 * 482), '#11121b')
    draw = ImageDraw.Draw(sheet)
    cards = []
    for i, (label, scene, number) in enumerate(SHOTS):
        name = f'{i + 1:02d}_{scene}_{number}'
        im = frame(scene, number)
        im.save(OUT / f'{name}.png')
        x, y = i % 4 * 368 + 8, i // 4 * 482
        draw.text((x, y + 8), label, font=font, fill='#eee9ff')
        sheet.paste(im.resize((352, 440), Image.Resampling.NEAREST), (x, y + 36))
        cards.append(f'<figure><img src="{name}.png"><figcaption>{label} · frame {number}</figcaption></figure>')
    sheet.save(OUT / 'runtime_overview.png')
    ranges = [('claw', 681, 870), ('charge', 795, 933), ('follow', 1251, 1359),
              ('blade', 765, 855), ('judge', 765, 855), ('fire', 765, 855), ('lava', 765, 945)]
    for scene, start, end in ranges:
        frames = [frame(scene, n).resize((352, 440), Image.Resampling.NEAREST) for n in range(start, end + 1, 3)]
        frames[0].save(OUT / f'{scene}.gif', save_all=True, append_images=frames[1:], duration=60, loop=0)
    gifs = ''.join(f'<figure><img src="{s}.gif"><figcaption>{s}</figcaption></figure>' for s, _, _ in ranges)
    html = '''<!doctype html><html lang="ko"><meta charset="utf-8"><title>악의 실제 적용 화면</title>
<style>body{background:#11121b;color:#eee9ff;font:16px sans-serif;max-width:1500px;margin:30px auto;padding:20px}main{display:flex;flex-wrap:wrap;gap:20px}figure{margin:0}img{width:352px;image-rendering:pixelated}figcaption{padding:12px 0}a{color:#e2afff}</style>
<h1>악의 · 실제 게임 적용 화면</h1><p>2026-10-14 · 동일 게임 엔진의 헤드리스 실행 캡처. 원본 176×220, 화면 표시 2배. 콘셉트 합성 이미지가 아닙니다.</p>
<p>20ms/프레임, 3프레임 간격 캡처. 검증용 세이브와 스킬·적 HP 설정으로 연출을 재생했습니다. 휴대폰 실기 촬영은 아닙니다.</p>
<h2>대표 장면</h2><main>''' + ''.join(cards) + '</main><h2>동작 미리보기</h2><main>' + gifs + '</main></html>'
    (OUT / 'index.html').write_text(html, encoding='utf-8')


if __name__ == '__main__':
    main()
