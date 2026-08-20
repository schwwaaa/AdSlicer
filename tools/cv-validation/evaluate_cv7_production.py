#!/usr/bin/env python3
import json
import sys
from pathlib import Path


def evaluate(path: Path, expected_policy: str, expected_segments: int) -> None:
    data = json.loads(path.read_text())
    failures = []

    if data.get("engine") != "opencv_adaptive":
        failures.append(f"engine={data.get('engine')!r}")
    if data.get("policy") != expected_policy:
        failures.append(f"policy={data.get('policy')!r}, expected {expected_policy!r}")
    if data.get("segment_count") != expected_segments:
        failures.append(f"segment_count={data.get('segment_count')}, expected {expected_segments}")

    coverage = data.get("coverage") or {}
    if not coverage.get("coverage_pass"):
        failures.append("coverage_pass=false")
    if abs(float(coverage.get("uncovered_duration_s", 999))) > 1e-6:
        failures.append(f"uncovered_duration_s={coverage.get('uncovered_duration_s')}")
    if abs(float(coverage.get("overlap_duration_s", 999))) > 1e-6:
        failures.append(f"overlap_duration_s={coverage.get('overlap_duration_s')}")

    if failures:
        print("CV-7 production integration: FAIL")
        for f in failures:
            print(" -", f)
        raise SystemExit(1)

    print(
        f"CV-7 production integration: PASS — {expected_policy}, "
        f"{expected_segments} segments, zero-loss coverage"
    )


if __name__ == "__main__":
    if len(sys.argv) != 4:
        raise SystemExit("usage: evaluate_cv7_production.py <manifest.json> <policy> <segment_count>")
    evaluate(Path(sys.argv[1]), sys.argv[2], int(sys.argv[3]))
