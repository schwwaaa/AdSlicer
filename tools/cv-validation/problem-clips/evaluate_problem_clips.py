#!/usr/bin/env python3
import csv, json, sys
from pathlib import Path

# Real WCPX/CBS 1995 failure examples supplied during productization.
# Times are intentionally tolerant: we care that the structural break is
# recovered, not whether the black frame lands on one side of the splice.
EXPECT = {
    "1995-WCPX6-CBS_segment_0005": [573.0, 588.2, 618.2],
    "1995-WCPX6-CBS_segment_0009": [483.2],
    "1995-WCPX6-CBS_segment_0010": [30.0],
    "1995-WCPX6-CBS_segment_0014": [30.0, 42.3],
    "1995-WCPX6-CBS_segment_0020": [29.85],
    "1995-WCPX6-CBS_segment_0023": [29.98],
}
TOL = 1.25

def boundaries(plan_csv: Path):
    with plan_csv.open(newline="") as f:
        rows = list(csv.DictReader(f))
    if not rows:
        return []
    # Every segment end except the source tail is a boundary.
    return [float(r["end_s"]) for r in rows[:-1]]

def main(root: Path):
    passed = 0
    total = 0
    report = {}
    for stem, expected in EXPECT.items():
        plan = root / stem / "06-structural-segmentation" / "complete_segments_plan.csv"
        if not plan.exists():
            print(f"MISSING {stem}: {plan}")
            report[stem] = {"status":"missing"}
            continue
        actual = boundaries(plan)
        checks=[]
        for t in expected:
            total += 1
            nearest = min(actual, key=lambda x: abs(x-t)) if actual else None
            ok = nearest is not None and abs(nearest-t) <= TOL
            passed += int(ok)
            checks.append({"expected":t,"nearest":nearest,"pass":ok})
            print(("PASS" if ok else "FAIL"), stem, f"expected ~{t:.2f}s", f"nearest={nearest}")
        report[stem]={"actual_boundaries":actual,"checks":checks}
    print(f"\nProblem-clip boundary checks: {passed}/{total} passed")
    (root / "problem_clip_evaluation.json").write_text(json.dumps(report, indent=2))
    return 0 if passed == total else 1

if __name__ == "__main__":
    if len(sys.argv) != 2:
        raise SystemExit("usage: evaluate_problem_clips.py OUTPUT_ROOT")
    raise SystemExit(main(Path(sys.argv[1])))
