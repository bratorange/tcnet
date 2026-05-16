#!/usr/bin/env python3
"""LUCHS melodic/percussive + pitch contour helper.

Uses librosa's HPSS for the M/P ratio and Essentia's PitchMelodia (when
installed) for the pitch contour. If either dependency is missing, the helper
emits a low-fidelity fallback derived from short-time RMS so the UI still
gets something to render.

Output JSON schema:

  {
    "mp":    [{"t": <s>, "m": 0..1}, ...],     # melodic fraction per frame
    "pitch": [{"t": <s>, "f0": <Hz>, "v": 0|1}, ...]
  }

Usage:
    luchs_mp_pitch.py <input.wav> <output.json>
"""
import json
import os
import sys
import wave


def real_mp(audio_path: str):
    try:
        import librosa  # type: ignore
        import numpy as np  # type: ignore
    except Exception:
        return None
    try:
        y, sr = librosa.load(audio_path, sr=22_050, mono=True)
        harm, perc = librosa.effects.hpss(y)
        hop = 2048
        frames = max(1, len(y) // hop)
        mp_points = []
        for i in range(frames):
            s = i * hop
            e = min(s + hop, len(y))
            h_rms = float(np.sqrt(np.mean(harm[s:e] ** 2) + 1e-12))
            p_rms = float(np.sqrt(np.mean(perc[s:e] ** 2) + 1e-12))
            ratio = h_rms / (h_rms + p_rms + 1e-12)
            mp_points.append({"t": s / sr, "m": float(max(0.0, min(1.0, ratio)))})
        return mp_points
    except Exception:
        return None


def real_pitch(audio_path: str):
    """Best-effort essentia pitch tracking. Tries PitchYinFFT (general-purpose
    per-frame tracker, well-suited for instrumental music) and falls back to
    PitchMelodia (predominant-melody extraction). Returns [] on any failure.

    Filters by per-frame confidence so unvoiced/noisy frames aren't drawn.
    """
    try:
        import essentia.standard as ess  # type: ignore
    except Exception:
        return []
    sr = 44_100
    frame_size = 2048
    hop = 1024  # ~43 fps — smooth without flooding the renderer.
    # Polyphonic music has noisy pitch estimates: be lenient on confidence so
    # we render movement even on percussive tracks (the VJ wants visual cues,
    # not perfect transcription).
    confidence_threshold = 0.15
    min_freq = 60.0
    max_freq = 1200.0

    try:
        loader = ess.MonoLoader(filename=audio_path, sampleRate=sr)
        audio = loader()
    except Exception:
        return []

    # First choice: PitchYinFFT (real-time per-frame, what the essentia.js
    # demo uses). This tracks the dominant pitched component each frame.
    try:
        pyin = ess.PitchYinFFT(
            frameSize=frame_size,
            sampleRate=sr,
            minFrequency=min_freq,
            maxFrequency=max_freq,
        )
        window = ess.Windowing(type="hann")
        spectrum = ess.Spectrum(size=frame_size)
        points = []
        frame_gen = ess.FrameGenerator(audio, frameSize=frame_size, hopSize=hop)
        for i, frame in enumerate(frame_gen):
            spec = spectrum(window(frame))
            f0, conf = pyin(spec)
            t = (i * hop) / sr
            f0f = float(f0)
            voiced = (
                1
                if float(conf) >= confidence_threshold
                and min_freq <= f0f <= max_freq
                else 0
            )
            points.append({"t": float(t), "f0": f0f, "v": int(voiced)})
        return points
    except Exception:
        pass

    # Fallback: PitchMelodia, downsampled to the same hop.
    try:
        pm = ess.PitchMelodia(frameSize=frame_size, hopSize=hop, sampleRate=sr)
        pitch, conf = pm(audio)
        points = []
        for i, (f0, c) in enumerate(zip(pitch, conf)):
            t = (i * hop) / sr
            f0f = float(f0)
            voiced = (
                1
                if float(c) >= confidence_threshold and min_freq <= f0f <= max_freq
                else 0
            )
            points.append({"t": float(t), "f0": f0f, "v": int(voiced)})
        return points
    except Exception:
        return []


def fallback_mp(audio_path: str):
    """RMS-based M/P estimate. Treats louder frames as more 'melodic' which is
    a poor proxy but produces a varying curve so the strip isn't flat in
    smoke tests.
    """
    try:
        with wave.open(audio_path, "rb") as w:
            channels = w.getnchannels()
            sr = w.getframerate()
            n_frames = w.getnframes()
            sample_width = w.getsampwidth()
            if sample_width != 2 or channels not in (1, 2):
                return []
            raw = w.readframes(n_frames)
    except Exception:
        return []
    import array

    samples = array.array("h", raw)
    if channels == 2:
        mono = [(samples[i] + samples[i + 1]) / 2 for i in range(0, len(samples) - 1, 2)]
    else:
        mono = list(samples)
    hop = max(1, sr // 10)  # ~10 fps
    out = []
    n = len(mono)
    for i in range(0, n, hop):
        chunk = mono[i : i + hop]
        if not chunk:
            continue
        rms = (sum(x * x for x in chunk) / len(chunk)) ** 0.5
        ratio = min(1.0, rms / 12000.0)
        out.append({"t": i / sr, "m": float(ratio)})
    return out


def main():
    if len(sys.argv) != 3:
        print("usage: luchs_mp_pitch.py <audio> <output.json>", file=sys.stderr)
        sys.exit(2)
    audio = sys.argv[1]
    out = sys.argv[2]

    mp = real_mp(audio)
    if not mp:
        mp = fallback_mp(audio)
    pitch = real_pitch(audio)

    os.makedirs(os.path.dirname(out) or ".", exist_ok=True)
    with open(out, "w") as f:
        json.dump({"mp": mp, "pitch": pitch}, f)


if __name__ == "__main__":
    main()
