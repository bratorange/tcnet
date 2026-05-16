#!/usr/bin/env python3
"""Simulator-side beat-grid extractor.

Uses madmom's `RNNDownBeatProcessor` + `DBNDownBeatTrackingProcessor` to
detect beats AND downbeats. The DBN tracker is HMM-based and produces stable
beat positions with reliable per-beat bar position (1, 2, 3, 4). BPM is
estimated from the median beat interval.

If madmom isn't importable, falls back to librosa.beat.beat_track (no
real downbeats — every 4th beat is marked). If neither is available, returns
an empty grid with BPM=120 and the caller falls back further to a constant-BPM
synthesised grid.

Output JSON:
    {
        "bpm": <float>,
        "beats": [<float seconds>, ...],
        "downbeats": [<float seconds>, ...]
    }

Usage:
    sim_beatgrid.py <audio.wav> <output.json>
"""
import json
import os
import sys


def detect_with_madmom(audio_path):
    try:
        from madmom.features.downbeats import (
            RNNDownBeatProcessor,
            DBNDownBeatTrackingProcessor,
        )  # type: ignore
    except Exception:
        return None
    try:
        activations = RNNDownBeatProcessor()(audio_path)
        proc = DBNDownBeatTrackingProcessor(beats_per_bar=[3, 4], fps=100)
        beats = proc(activations)  # array of (time_sec, beat_position) rows
        if len(beats) == 0:
            return None
        beat_times = [float(row[0]) for row in beats]
        downbeats = [float(row[0]) for row in beats if int(row[1]) == 1]
        # BPM from median consecutive beat interval.
        if len(beat_times) >= 2:
            import statistics
            intervals = [
                beat_times[i + 1] - beat_times[i] for i in range(len(beat_times) - 1)
            ]
            bpm = 60.0 / statistics.median(intervals) if intervals else 120.0
        else:
            bpm = 120.0
        return {"bpm": bpm, "beats": beat_times, "downbeats": downbeats}
    except Exception as e:
        print(f"madmom detection failed: {e}", file=sys.stderr)
        return None


def detect_with_librosa(audio_path):
    try:
        import librosa  # type: ignore
    except Exception:
        return None
    try:
        y, sr = librosa.load(audio_path, sr=22_050, mono=True)
        tempo, beats = librosa.beat.beat_track(y=y, sr=sr, units="time")
        bpm = float(tempo) if hasattr(tempo, "__float__") else 120.0
        beat_list = [float(b) for b in beats]
        # Approximate downbeats: every 4th beat (librosa has no downbeat tracker).
        downbeats = beat_list[::4]
        return {"bpm": bpm, "beats": beat_list, "downbeats": downbeats}
    except Exception:
        return None


def main():
    if len(sys.argv) != 3:
        print("usage: sim_beatgrid.py <audio> <output.json>", file=sys.stderr)
        sys.exit(2)
    audio = sys.argv[1]
    out = sys.argv[2]

    result = detect_with_madmom(audio)
    if result is None:
        result = detect_with_librosa(audio)
    if result is None:
        result = {"bpm": 120.0, "beats": [], "downbeats": []}

    os.makedirs(os.path.dirname(out) or ".", exist_ok=True)
    with open(out, "w") as f:
        json.dump(result, f)


if __name__ == "__main__":
    main()
