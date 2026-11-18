# 번개 효과음 — 「어둠심판」의 낙뢰 · 「벽력일섬」(벽력일섬 + 번개의 호흡)의 번개 연출이 내는 호스트 클립.
#
# ★★ 2026-10-24 (사용자: *"어둠심판도 소리안나오고있어 번개소리 나야해. 그리고 번개의호흡이랑 벽력일섬 장착하면 번개이펙트 생길떄도
# 혹시 번개소리 안나면 추가해줘"*). 게임의 효과음 11 개(4-bit ADPCM 8 kHz — 크리티컬히트 · 미스 · 속성공격합 · 기합 · 오오라 · 검기 ·
# 난무 · 피격 셋 · 문)에는 번개가 없다(`HERO2_GUEST_RE.md` §8.43). 그래서 각성기 목소리와 같은 길(`voice.rs` →
# `audio_sink::play_host_clip`)로 호스트가 튼다 — 이 스크립트가 그 클립을 결정적으로(고정 시드) 합성한다.
#
# 소리의 구성 (1.0 s, 44.1 kHz 모노 — 다른 목소리 클립과 같은 꼴):
#   ① 선도 방전 — 0..55 ms, 점점 촘촘해지는 「틱틱틱」 일곱 번 (고역 잡음 2~3 ms)
#   ② 귀환 뇌격 — 55 ms 의 「쾅」: 광대역 잡음 (빠른 상승 · 35 ms 감쇠) + 300 → 130 Hz 로 내려가는 저음 타격
#   ③ 지직거림 — 2.5~7 kHz 잡음을 95 Hz 버즈와 4 ms 무작위 게이트로 끊어 120 ms 감쇠 (전기 방전의 결)
#   ④ 우르릉 — 갈색 잡음 150~700 Hz (폰 스피커가 내는 대역 — 그 아래는 소리 없이 크기만 먹는다), 세 번 굴러가며 380 ms 감쇠, 끝 60 ms 페이드
# 크기는 `voice.rs` 가 클립마다 맞추므로(`clip_target_db`) 여기서는 피크 0.89 로만 맞춘다.
#
# 사용법: python tools_re/thunder_sfx.py [out.ogg] [--wav preview.wav]
#   기본 출력 = wie_android/assets/voice/thunder.ogg (앱 · probe 가 include_bytes! 로 싣는다)
import os
import sys

import numpy as np
import soundfile as sf

SR = 44_100
SEED = 20_261_024
LENGTH = 1.0
STRIKE = 0.055


def band(x, lo=None, hi=None):
    """FFT 마스크 대역 필터 (양쪽 4차 버터워스꼴 크기, 위상 0)."""
    spec = np.fft.rfft(x)
    f = np.fft.rfftfreq(len(x), 1 / SR)
    m = np.ones_like(f)
    if lo:
        m *= 1 / (1 + (lo / np.maximum(f, 1e-3)) ** 4)
    if hi:
        m *= 1 / (1 + (f / hi) ** 4)
    return np.fft.irfft(spec * m, len(x))


def smooth(x, ms):
    k = max(1, int(SR * ms / 1000))
    return np.convolve(x, np.ones(k) / k, mode="same")


def thunder():
    rng = np.random.default_rng(SEED)
    n = int(SR * LENGTH)
    t = np.arange(n) / SR
    after = np.clip(t - STRIKE, 0, None)
    on = (t >= STRIKE).astype(float)
    out = np.zeros(n)

    # ① 선도 방전
    leader = np.zeros(n)
    for at, amp in zip([0.000, 0.014, 0.025, 0.034, 0.041, 0.047, 0.052], [0.22, 0.26, 0.30, 0.34, 0.40, 0.46, 0.55]):
        i = int(at * SR)
        k = int(0.003 * SR)
        burst = rng.standard_normal(k) * np.exp(-np.arange(k) / (0.0007 * SR))
        leader[i : i + k] += amp * burst
    out += band(leader, lo=1500)

    # ② 귀환 뇌격 — 쾅 + 내려가는 저음
    attack = 1 - np.exp(-after / 0.0008)
    crack = band(rng.standard_normal(n), lo=300, hi=9000) * on * attack * np.exp(-after / 0.035)
    out += 1.0 * crack
    freq = 130 + 170 * np.exp(-after / 0.025)
    phase = 2 * np.pi * np.cumsum(freq) / SR
    out += 0.45 * np.sin(phase) * on * attack * np.exp(-after / 0.07)

    # ③ 지직거림
    blocks = (rng.random(int(LENGTH * 250) + 2) > 0.42).astype(float)
    gate = smooth(np.repeat(blocks, SR // 250)[:n], 1.0)
    buzz = 0.6 + 0.4 * np.sign(np.sin(2 * np.pi * 95 * t))
    sizzle = band(rng.standard_normal(n), lo=2500, hi=7000) * gate * buzz * on * np.exp(-after / 0.12)
    out += 0.38 * sizzle

    # ④ 우르릉
    brown = np.cumsum(rng.standard_normal(n))
    brown = band(brown - brown.mean(), lo=150, hi=700)
    brown /= np.max(np.abs(brown)) + 1e-9
    roll = 0.55 + 0.45 * np.sin(2 * np.pi * 3.0 * t + rng.random() * 2 * np.pi) ** 2
    late = np.clip(after - 0.02, 0, None)
    rumble = brown * roll * (t >= STRIKE + 0.02) * (1 - np.exp(-late / 0.03)) * np.exp(-after / 0.38)
    out += 1.1 * rumble

    fade = np.clip((LENGTH - t) / 0.06, 0, 1)
    out *= fade
    out[: int(0.001 * SR)] *= np.linspace(0, 1, int(0.001 * SR))
    out *= 0.89 / np.max(np.abs(out))
    return out


def main(argv):
    here = os.path.dirname(os.path.abspath(__file__))
    default = os.path.join(here, "..", "wie_android", "assets", "voice", "thunder.ogg")
    wav = None
    if "--wav" in argv:
        i = argv.index("--wav")
        wav = argv[i + 1]
        argv = argv[:i] + argv[i + 2 :]
    out = argv[0] if argv else default
    pcm = thunder()
    sf.write(out, pcm, SR, format="OGG", subtype="VORBIS")
    if wav:
        sf.write(wav, pcm, SR, subtype="PCM_16")
    back, sr = sf.read(out)
    peak = 20 * np.log10(np.max(np.abs(back)))
    print(f"thunder: {len(back) / sr:.3f} s · {sr} Hz · peak {peak:.1f} dBFS · {os.path.getsize(out)} B → {os.path.normpath(out)}")


if __name__ == "__main__":
    main(sys.argv[1:])
