#!/usr/bin/env python3
"""Validate the development-only Python mirror of CV-2 rules."""
from __future__ import annotations
import json
from pathlib import Path
ROOT = Path(__file__).resolve().parent
RESULTS = ROOT / "results"
TRUTH = json.loads((ROOT / "temporal_ground_truth.json").read_text())

def match(e, expected):
    return all(e.get(k) == v for k, v in expected.items())

def main():
    passed = 0
    total = 0
    for case in TRUTH["cases"]:
        total += 1
        events = json.loads((RESULTS / case["case"] / "temporal_reference_events.json").read_text())["events"]
        ok = True
        if "required_event" in case:
            ok = any(match(e, case["required_event"]) for e in events)
        if "forbidden_kinds" in case:
            forbidden = set(case["forbidden_kinds"])
            ok = ok and not any(e["kind"] in forbidden for e in events)
        print(f"[{'PASS' if ok else 'FAIL'}] {case['case']} — {[(e['kind'], e['start_frame'], e['end_frame']) for e in events]}")
        passed += int(ok)
    print(f"\nCV-2 reference temporal checks: {passed}/{total} passed")
    return 0 if passed == total else 1

if __name__ == "__main__":
    raise SystemExit(main())
