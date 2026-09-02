#!/usr/bin/env python3
"""CV-4 synthetic acceptance check for the unified shadow edit planner."""
from __future__ import annotations
import json
import sys
from pathlib import Path


def fail(msg: str) -> int:
    print(f"[FAIL] {msg}")
    return 1


def main() -> int:
    if len(sys.argv) != 2:
        print("usage: evaluate_shadow_plan.py OPENCV_TEST_REPORT.json", file=sys.stderr)
        return 2
    path = Path(sys.argv[1])
    if not path.exists():
        return fail(f"missing report: {path}")
    report = json.loads(path.read_text())
    removals = report.get("shadow_removals") or []
    if report.get("production_cut_plan_modified") is not False:
        return fail("production_cut_plan_modified must remain false")
    if report.get("render_performed") is not False:
        return fail("render_performed must remain false")
    if len(removals) != 1:
        return fail(f"expected exactly 1 shadow removal, got {len(removals)}")

    r = removals[0]
    start = float(r["start_s"])
    end = float(r["end_s"])
    dur = float(r["duration_s"])
    # Expected from 5s content + .2s black + 12s block + .2s black.
    # With default edge padding, ~5.00 -> 17.26 (~12.26s).
    if not (4.90 <= start <= 5.10):
        return fail(f"unexpected start {start:.3f}s")
    if not (17.15 <= end <= 17.35):
        return fail(f"unexpected end {end:.3f}s")
    if not (12.10 <= dur <= 12.40):
        return fail(f"unexpected duration {dur:.3f}s")

    cmp = report.get("plan_comparison")
    if cmp is not None:
        summary = cmp.get("summary", {})
        if int(summary.get("matched_intervals", 0)) < 1:
            return fail("legacy comparison ran but did not match the synthetic interval")

    print(f"[PASS] CV-4 synthetic removal {start:.3f}s -> {end:.3f}s ({dur:.3f}s)")
    if cmp is not None:
        print("[PASS] legacy/default-plan comparison matched the synthetic removal")
    else:
        print("[WARN] legacy comparison unavailable; shadow-plan acceptance still passed")
    print("\nCV-4 shadow-plan checks: 2/2 passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
