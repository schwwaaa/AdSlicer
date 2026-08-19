#!/usr/bin/env python3
"""Development-only Python mirror of the CV-2 temporal rules.

This is not production code. It exists so the event logic can be inspected and
regression-checked independently of the Rust/OpenCV build environment.
"""
from __future__ import annotations

import csv
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parent
RESULTS = ROOT / "results"

CFG = {
    "near_black_ratio_min": 0.90,
    "strict_black_ratio_min": 0.90,
    "dark_mean_luma_max": 32.0,
    "raised_uniform_luma_max": 40.0,
    "raised_uniform_stddev_max": 6.0,
    "raised_uniform_min_run_frames": 3,
    "micro_bridge_frames": 2,
    "context_frames": 5,
    "fade_step_luma_min": 5.0,
    "fade_monotonic_steps_min": 2,
    "fade_magnitude_min": 20.0,
    "scene_delta_min": 40.0,
    "scene_suppression_frames": 5,
    "short_event_max_frames": 2,
    "chromatic_saturation_min": 120.0,
    "chromatic_channel_spread_min": 40.0,
}

FLOATS = {
    "pts_s", "mean_luma", "stddev_luma", "black_pixel_ratio", "near_black_pixel_ratio",
    "mean_blue", "mean_green", "mean_red", "mean_hue", "mean_saturation", "mean_value",
    "frame_delta_mean",
}
INTS = {"frame_index", "width", "height", "min_luma", "max_luma"}


def load_csv(path: Path):
    with path.open(newline="") as f:
        rows = list(csv.DictReader(f))
    for r in rows:
        for k in FLOATS:
            r[k] = float(r[k])
        for k in INTS:
            r[k] = int(float(r[k]))
    return rows


def mean(vals):
    vals = list(vals)
    return sum(vals) / len(vals) if vals else None


def spread(r):
    return max(abs(r["mean_blue"] - r["mean_green"]), abs(r["mean_blue"] - r["mean_red"]), abs(r["mean_green"] - r["mean_red"]))


def analyze(rows):
    base_dark = [
        r["near_black_pixel_ratio"] >= CFG["near_black_ratio_min"]
        or r["mean_luma"] <= CFG["dark_mean_luma_max"]
        for r in rows
    ]
    raised_candidate = [
        r["mean_luma"] > CFG["dark_mean_luma_max"]
        and r["mean_luma"] <= CFG["raised_uniform_luma_max"]
        and r["stddev_luma"] <= CFG["raised_uniform_stddev_max"]
        for r in rows
    ]
    raised_sustained = [False] * len(rows)
    i = 0
    min_run = max(1, int(CFG["raised_uniform_min_run_frames"]))
    while i < len(rows):
        if not raised_candidate[i]:
            i += 1
            continue
        start = i
        i += 1
        while i < len(rows) and raised_candidate[i]:
            i += 1
        if i - start >= min_run:
            for j in range(start, i):
                raised_sustained[j] = True

    dark_pos = [i for i in range(len(rows)) if base_dark[i] or raised_sustained[i]]
    groups = []
    if dark_pos:
        start = prev = dark_pos[0]
        for pos in dark_pos[1:]:
            missing = rows[pos]["frame_index"] - rows[prev]["frame_index"] - 1
            if missing <= CFG["micro_bridge_frames"]:
                prev = pos
            else:
                groups.append((start, prev))
                start = prev = pos
        groups.append((start, prev))

    events = []
    for s, e in groups:
        seg = rows[s:e+1]
        valley = min(seg, key=lambda r: r["mean_luma"])
        pre0 = max(0, s - CFG["context_frames"])
        post1 = min(len(rows) - 1, e + CFG["context_frames"])
        before = rows[pre0:s]
        after = rows[e+1:post1+1]
        l_before = mean(r["mean_luma"] for r in before) if before else rows[s]["mean_luma"]
        l_after = mean(r["mean_luma"] for r in after) if after else rows[e]["mean_luma"]
        desc = sum(1 for a, b in zip(rows[pre0:s+1], rows[pre0+1:s+1]) if b["mean_luma"] - a["mean_luma"] <= -CFG["fade_step_luma_min"])
        asc = sum(1 for a, b in zip(rows[e:post1], rows[e+1:post1+1]) if b["mean_luma"] - a["mean_luma"] >= CFG["fade_step_luma_min"])
        min_luma = min(r["mean_luma"] for r in seg)
        strict = max(r["black_pixel_ratio"] for r in seg)
        near = max(r["near_black_pixel_ratio"] for r in seg)
        sat = mean(r["mean_saturation"] for r in seg)
        max_spread = max(spread(r) for r in seg)
        chrom = sat >= CFG["chromatic_saturation_min"] and max_spread >= CFG["chromatic_channel_spread_min"]
        duration = rows[e]["frame_index"] - rows[s]["frame_index"] + 1
        fall = max(0.0, l_before - min_luma)
        rise = max(0.0, l_after - min_luma)
        fade = desc >= CFG["fade_monotonic_steps_min"] and asc >= CFG["fade_monotonic_steps_min"] and fall >= CFG["fade_magnitude_min"] and rise >= CFG["fade_magnitude_min"]
        if chrom and strict < CFG["strict_black_ratio_min"]:
            kind = "chromatic_dark"
        elif duration <= CFG["short_event_max_frames"] and strict >= CFG["strict_black_ratio_min"]:
            kind = "black_separator"
        elif duration <= CFG["short_event_max_frames"] and near >= CFG["near_black_ratio_min"]:
            kind = "near_black_separator"
        elif fade:
            kind = "fade_valley"
        elif strict >= CFG["strict_black_ratio_min"]:
            kind = "black_interval"
        else:
            kind = "near_black_interval"
        events.append({"kind": kind, "start_frame": rows[s]["frame_index"], "valley_frame": valley["frame_index"], "end_frame": rows[e]["frame_index"]})

    ranges = [(e["start_frame"], e["end_frame"]) for e in events]
    def suppressed(frame):
        return any(a - CFG["scene_suppression_frames"] <= frame <= b + CFG["scene_suppression_frames"] for a, b in ranges)

    for i in range(1, len(rows)-1):
        r = rows[i]
        if r["frame_delta_mean"] >= CFG["scene_delta_min"] and r["frame_delta_mean"] >= rows[i-1]["frame_delta_mean"] and r["frame_delta_mean"] >= rows[i+1]["frame_delta_mean"] and not suppressed(r["frame_index"]):
            events.append({"kind": "scene_discontinuity", "start_frame": r["frame_index"], "valley_frame": r["frame_index"], "end_frame": r["frame_index"]})
    events.sort(key=lambda e: (e["start_frame"], e["end_frame"], e["kind"]))
    return events


def main():
    for csv_path in sorted(RESULTS.glob("*/opencv_frame_metrics.csv")):
        events = analyze(load_csv(csv_path))
        out = csv_path.parent / "temporal_reference_events.json"
        out.write_text(json.dumps({"events": events}, indent=2))
        print(f"{csv_path.parent.name}: {len(events)} events -> {out}")


if __name__ == "__main__":
    main()
