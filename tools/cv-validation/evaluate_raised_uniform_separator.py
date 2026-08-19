#!/usr/bin/env python3
import json, pathlib, sys

if len(sys.argv) != 2:
    raise SystemExit("usage: evaluate_raised_uniform_separator.py OPENCV_TEST_REPORT.json")
report=json.loads(pathlib.Path(sys.argv[1]).read_text())
struct=(report.get("structural_segmentation") or {})
summary=(struct.get("summary") or {})
complete=(struct.get("complete_segments") or {})
events=(struct.get("events") or [])

checks=[]
def check(name, ok, detail=""):
    checks.append(bool(ok))
    print(f"[{'PASS' if ok else 'FAIL'}] {name}" + (f" — {detail}" if detail else ""))

check("Raised-uniform source becomes three Complete Segments",
      summary.get("complete_segments") == 3,
      f"got {summary.get('complete_segments')}")
check("Two Complete-Segment structural boundaries are selected",
      summary.get("complete_boundaries") == 2,
      f"got {summary.get('complete_boundaries')}")
check("Complete Segments coverage remains exact",
      bool((complete.get("coverage") or {}).get("coverage_pass")))

bounds=complete.get("boundaries") or []
first_t=float(bounds[0].get("pts_s",999)) if bounds else 999
check("Raised-gray separator near 3 seconds is recovered",
      2.8 <= first_t <= 3.6,
      f"first boundary {first_t:.3f}s")

segments=complete.get("segments") or []
if len(segments)==3:
    d=[float(x.get("duration_s",0)) for x in segments]
    check("Middle 15-second content block remains complete",
          14.5 <= d[1] <= 16.2,
          f"middle duration {d[1]:.3f}s")
else:
    check("Middle 15-second content block remains complete", False, "segment count mismatch")

raised=[]
for e in events:
    min_l=float(e.get("min_mean_luma",0))
    sd=float(e.get("mean_stddev_luma",999))
    if 32.0 < min_l <= 40.0 and sd <= 6.0:
        raised.append(e)
check("Raised-uniform event is preserved as explicit evidence", bool(raised),
      f"events {len(raised)}")

passed=sum(checks)
print(f"\nCV-6.1 raised-uniform checks: {passed}/{len(checks)} passed")
if passed != len(checks):
    raise SystemExit(1)
