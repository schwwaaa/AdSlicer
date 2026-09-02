#!/usr/bin/env python3
import json, math, pathlib, subprocess, sys

if len(sys.argv) != 2:
    raise SystemExit("usage: evaluate_structural_segmentation.py OPENCV_TEST_REPORT.json")
path = pathlib.Path(sys.argv[1])
report = json.loads(path.read_text())
struct = report.get("structural_segmentation") or {}
summary = struct.get("summary") or {}
complete = struct.get("complete_segments") or {}
every = struct.get("every_separator") or {}
render = report.get("full_segment_validation") or {}

checks=[]
def check(name, ok, detail=""):
    checks.append(bool(ok))
    print(f"[{'PASS' if ok else 'FAIL'}] {name}" + (f" — {detail}" if detail else ""))

check("Complete Segments has four full timeline clips", summary.get("complete_segments") == 4,
      f"got {summary.get('complete_segments')}")
check("Complete Segments uses three structural boundaries", summary.get("complete_boundaries") == 3,
      f"got {summary.get('complete_boundaries')}")
check("Every Separator exposes the internal fade", summary.get("every_separator_segments") == 5,
      f"got {summary.get('every_separator_segments')}")
check("Complete Segments coverage is exact", bool((complete.get("coverage") or {}).get("coverage_pass")))
check("Every Separator coverage is exact", bool((every.get("coverage") or {}).get("coverage_pass")))
check("No Complete Segments residual disappears",
      float((complete.get("coverage") or {}).get("uncovered_duration_s", 999)) <= 0.002)
check("No Complete Segments overlap",
      float((complete.get("coverage") or {}).get("overlap_duration_s", 999)) <= 0.002)

items = (render.get("items") or []) if isinstance(render, dict) else []
complete_items = [i for i in items if i.get("mode") == "complete_segments"]
check("All four Complete Segments validation clips rendered",
      len(complete_items) == 4 and all(i.get("status") == "rendered" for i in complete_items),
      f"rendered {sum(i.get('status')=='rendered' for i in complete_items)}/{len(complete_items)}")

# The two middle complete segments should be near 15s and 30s despite a true
# uniform black fade inside the 30-second synthetic content block.
segments = complete.get("segments") or []
if len(segments) == 4:
    durations=[float(s.get("duration_s",0)) for s in segments]
    check("15-second block remains intact", abs(durations[1]-15.2) < 0.75, f"{durations[1]:.3f}s")
    check("30-second block remains intact across internal black fade", abs(durations[2]-30.3) < 0.9, f"{durations[2]:.3f}s")
else:
    check("15-second block remains intact", False, "segment count mismatch")
    check("30-second block remains intact across internal black fade", False, "segment count mismatch")

passed=sum(checks)
print(f"\nCV-6 structural-segmentation checks: {passed}/{len(checks)} passed")
if passed != len(checks):
    raise SystemExit(1)
