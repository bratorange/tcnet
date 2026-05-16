#!/usr/bin/env python3
"""LUCHS phrase-segmentation helper.

Tries to invoke the real `allin1` Python package; falls back to a deterministic
stub keyed by file size so the LUCHS test harness can run without a working
allin1 install. Outputs a JSON document compatible with allin1's struct.json
schema:

  {
    "path": "...",
    "bpm": <number or null>,
    "beats": [<float seconds>, ...],
    "downbeats": [<float seconds>, ...],
    "beat_positions": [<int>, ...],
    "segments": [{"start": <s>, "end": <s>, "label": <str>}, ...]
  }

Usage:
    luchs_allin1.py <input.wav> <output.json>
"""
import json
import os
import sys


def real_allin1(audio_path: str):
    try:
        from allin1 import analyze  # type: ignore
    except Exception:
        return None
    try:
        result = analyze(audio_path, out_dir=None, model="harmonix-all")
    except Exception:
        return None
    # `analyze` returns a dataclass with .segments etc.
    segments = [
        {"start": float(s.start), "end": float(s.end), "label": str(s.label)}
        for s in getattr(result, "segments", [])
    ]
    return {
        "path": audio_path,
        "bpm": getattr(result, "bpm", None),
        "beats": [float(b) for b in getattr(result, "beats", []) or []],
        "downbeats": [float(b) for b in getattr(result, "downbeats", []) or []],
        "beat_positions": [int(b) for b in getattr(result, "beat_positions", []) or []],
        "segments": segments,
    }


def stub_segments(audio_path: str):
    """Produce a deterministic phrase outline from file size so the UI has
    something coloured to render when allin1 isn't installed.
    """
    try:
        size = os.path.getsize(audio_path)
    except OSError:
        size = 0
    # Estimate duration from a stable assumption: 16-bit stereo 44.1 kHz PCM
    # ≈ 176_400 bytes/sec. Good enough for the stub timeline; the renderer
    # uses TCNet track_length_ms for real positioning, so timing here only
    # needs to span a plausible track.
    duration_s = max(60.0, min(600.0, size / 176_400.0))
    # Compose a generic dance-track outline.
    layout = [
        ("intro", 0.07),
        ("verse", 0.18),
        ("chorus", 0.18),
        ("break", 0.10),
        ("verse", 0.13),
        ("chorus", 0.20),
        ("inst", 0.07),
        ("outro", 0.07),
    ]
    segments = []
    t = 0.0
    for label, frac in layout:
        end = t + frac * duration_s
        segments.append({"start": t, "end": end, "label": label})
        t = end
    return {
        "path": audio_path,
        "bpm": None,
        "beats": [],
        "downbeats": [],
        "beat_positions": [],
        "segments": segments,
    }


def main():
    if len(sys.argv) != 3:
        print("usage: luchs_allin1.py <audio> <output.json>", file=sys.stderr)
        sys.exit(2)
    audio = sys.argv[1]
    out = sys.argv[2]

    result = real_allin1(audio)
    if result is None or not result.get("segments"):
        result = stub_segments(audio)

    os.makedirs(os.path.dirname(out) or ".", exist_ok=True)
    with open(out, "w") as f:
        json.dump(result, f)


if __name__ == "__main__":
    main()
