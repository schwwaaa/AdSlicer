#!/usr/bin/env python3
"""Evaluate Rust OpenCV probe CSV outputs against the synthetic controls.

This intentionally validates the *evidence layer*, not commercial-cut policy.
Future milestones can extend this evaluator with boundary/fade decisions.
"""
from __future__ import annotations

import csv
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent
RESULTS = ROOT / "results"


def load(name: str):
    path = RESULTS / name / "opencv_frame_metrics.csv"
    if not path.exists():
        raise FileNotFoundError(path)
    with path.open(newline="") as f:
        rows = list(csv.DictReader(f))
    for r in rows:
        for k, v in list(r.items()):
            if k in {"frame_index", "width", "height", "min_luma", "max_luma"}:
                r[k] = int(float(v))
            else:
                r[k] = float(v)
    return rows


def at(rows, i):
    for r in rows:
        if r["frame_index"] == i:
            return r
    raise AssertionError(f"frame {i} not found")


def check(label: str, ok: bool, detail: str = ""):
    status = "PASS" if ok else "FAIL"
    print(f"[{status}] {label}" + (f" — {detail}" if detail else ""))
    return ok


def main() -> int:
    passed = 0
    total = 0

    def t(label, ok, detail=""):
        nonlocal passed, total
        total += 1
        passed += int(bool(ok))
        return check(label, bool(ok), detail)

    try:
        clean = load("01_clean_black")
        single = load("02_single_black_frame")
        near = load("03_two_frame_near_black")
        valley = load("04_temporal_luma_valley")
        dark = load("05_dark_textured_content")
        hard = load("06_hard_cut_no_black")
        blue = load("07_uniform_blue_slate")
    except FileNotFoundError as e:
        print(f"Missing probe result: {e}")
        print("Run ./run_probe_suite.sh first.")
        return 2

    t("clean black: 30-frame strict-black run",
      all(at(clean, i)["black_pixel_ratio"] >= 0.99 for i in range(90, 120)))

    t("single black: exact frame is strict black",
      at(single, 90)["black_pixel_ratio"] >= 0.99,
      f"ratio={at(single, 90)['black_pixel_ratio']:.3f}")
    t("single black: neighbor before is not black",
      at(single, 89)["black_pixel_ratio"] < 0.50)
    t("single black: neighbor after is not black",
      at(single, 91)["black_pixel_ratio"] < 0.50)

    t("near-black: both raised-black frames are near-black",
      all(at(near, i)["near_black_pixel_ratio"] >= 0.99 for i in (90, 91)))
    t("near-black: raised-black frames are not strict black",
      all(at(near, i)["black_pixel_ratio"] < 0.50 for i in (90, 91)))

    valley_rows = [at(valley, i) for i in range(90, 98)]
    valley_min = min(valley_rows, key=lambda r: r["mean_luma"])
    t("temporal valley: local minimum preserved near expected center",
      valley_min["frame_index"] in (93, 94, 95),
      f"min frame={valley_min['frame_index']} luma={valley_min['mean_luma']:.2f}")
    t("temporal valley: fall/rise evidence exists",
      at(valley, 90)["mean_luma"] > valley_min["mean_luma"] and
      at(valley, 97)["mean_luma"] > valley_min["mean_luma"])

    dark_max_near = max(r["near_black_pixel_ratio"] for r in dark)
    t("dark textured content: does not become a near-black field",
      dark_max_near < 0.50,
      f"max near-black ratio={dark_max_near:.3f}")

    hard_peak = max(hard, key=lambda r: r["frame_delta_mean"])
    t("hard cut: strong frame delta near frame 90",
      abs(hard_peak["frame_index"] - 90) <= 1 and hard_peak["frame_delta_mean"] > 40.0,
      f"peak frame={hard_peak['frame_index']} delta={hard_peak['frame_delta_mean']:.2f}")
    t("hard cut: no strict-black frame created",
      max(r["black_pixel_ratio"] for r in hard) < 0.50)

    blue_cards = [at(blue, i) for i in range(90, 105)]
    t("blue slate: uniform luma variance",
      all(r["stddev_luma"] < 1.0 for r in blue_cards))
    t("blue slate: luma alone can look near-black",
      all(r["near_black_pixel_ratio"] >= 0.99 for r in blue_cards),
      "this is why color evidence matters")
    t("blue slate: saturation exposes the color-card false positive",
      all(r["mean_saturation"] > 200.0 for r in blue_cards))
    t("blue slate: blue channel is dominant",
      all(r["mean_blue"] > r["mean_green"] + 100 and r["mean_blue"] > r["mean_red"] + 100 for r in blue_cards))

    print(f"\nCV-1 evidence checks: {passed}/{total} passed")
    return 0 if passed == total else 1


if __name__ == "__main__":
    raise SystemExit(main())
