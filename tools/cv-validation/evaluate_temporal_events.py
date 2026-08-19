#!/usr/bin/env python3
"""Validate CV-2 Rust temporal event outputs against synthetic ground truth."""
from __future__ import annotations

import json
from pathlib import Path

ROOT = Path(__file__).resolve().parent
RESULTS = ROOT / "results"
TRUTH = json.loads((ROOT / "temporal_ground_truth.json").read_text())


def load_events(case: str):
    path = RESULTS / case / "temporal" / "opencv_temporal_events.json"
    if not path.exists():
        raise FileNotFoundError(path)
    return json.loads(path.read_text())["events"]


def event_matches(event, expected):
    for key, value in expected.items():
        if event.get(key) != value:
            return False
    return True


def main() -> int:
    passed = 0
    total = 0
    for case in TRUTH["cases"]:
        total += 1
        name = case["case"]
        try:
            events = load_events(name)
        except FileNotFoundError as exc:
            print(f"[FAIL] {name}: missing {exc}")
            continue

        ok = True
        detail = ""
        if "required_event" in case:
            expected = case["required_event"]
            matches = [e for e in events if event_matches(e, expected)]
            ok = bool(matches)
            detail = f"expected {expected}, found {[(e['kind'], e['start_frame'], e['end_frame']) for e in events]}"
        if "forbidden_kinds" in case:
            forbidden = set(case["forbidden_kinds"])
            offenders = [e for e in events if e["kind"] in forbidden]
            ok = ok and not offenders
            detail = f"forbidden temporal-dark events={[(e['kind'], e['start_frame'], e['end_frame']) for e in offenders]}"

        status = "PASS" if ok else "FAIL"
        print(f"[{status}] {name} — {detail}")
        passed += int(ok)

    print(f"\nCV-2 temporal checks: {passed}/{total} passed")
    return 0 if passed == total else 1


if __name__ == "__main__":
    raise SystemExit(main())
