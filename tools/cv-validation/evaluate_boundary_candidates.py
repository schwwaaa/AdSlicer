#!/usr/bin/env python3
"""Validate CV-3 Rust shadow boundary outputs against synthetic ground truth."""
from __future__ import annotations
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parent
RESULTS = ROOT / "results"
TRUTH = json.loads((ROOT / "boundary_ground_truth.json").read_text())


def load(case: str):
    path = RESULTS / case / "boundary" / "opencv_boundary_candidates.json"
    if not path.exists():
        raise FileNotFoundError(path)
    return json.loads(path.read_text())


def matches(row, expected):
    if "role" in expected and row.get("role") != expected["role"]:
        return False
    if "anchor_frame" in expected and row.get("anchor_frame") != expected["anchor_frame"]:
        return False
    if "min_score" in expected and float(row.get("boundary_score", 0.0)) < expected["min_score"]:
        return False
    return True


def main() -> int:
    passed = 0
    total = 0
    for case in TRUTH["cases"]:
        total += 1
        name = case["case"]
        try:
            result = load(name)
        except FileNotFoundError as exc:
            print(f"[FAIL] {name}: missing {exc}")
            continue

        candidates = result.get("candidates", [])
        evidence_only = result.get("evidence_only", [])
        ok = True
        notes = []

        if "max_candidates" in case:
            maxc = case["max_candidates"]
            if len(candidates) > maxc:
                ok = False
                notes.append(f"candidate count {len(candidates)} > {maxc}")

        for expected in case.get("required_candidates", []):
            found = [r for r in candidates if matches(r, expected)]
            if not found:
                ok = False
                notes.append(f"missing candidate {expected}")
            else:
                notes.append(
                    f"candidate {expected['role']} score={found[0]['boundary_score']:.3f} frame={found[0]['anchor_frame']}"
                )

        for expected in case.get("required_evidence_only", []):
            found = [r for r in evidence_only if matches(r, expected)]
            if not found:
                ok = False
                notes.append(f"missing evidence-only {expected}")
            else:
                notes.append(
                    f"evidence-only {expected['role']} score={found[0]['boundary_score']:.3f}"
                )

        print(f"[{'PASS' if ok else 'FAIL'}] {name} — {'; '.join(notes) if notes else f'{len(candidates)} candidates'}")
        passed += int(ok)

    print(f"\nCV-3 boundary checks: {passed}/{total} passed")
    return 0 if passed == total else 1


if __name__ == "__main__":
    raise SystemExit(main())
