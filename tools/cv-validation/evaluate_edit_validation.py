#!/usr/bin/env python3
"""CV-5 synthetic acceptance check for watchable edit validation."""
from __future__ import annotations
import json
import sys
from pathlib import Path


def fail(msg: str) -> int:
    print(f"[FAIL] {msg}")
    return 1


def main() -> int:
    if len(sys.argv) != 2:
        print("usage: evaluate_edit_validation.py edit_validation_manifest.json", file=sys.stderr)
        return 2
    path = Path(sys.argv[1])
    if not path.exists():
        return fail(f"missing manifest: {path}")
    data = json.loads(path.read_text())
    if data.get("production_cut_plan_modified") is not False:
        return fail("production_cut_plan_modified must remain false")
    if data.get("production_render_performed") is not False:
        return fail("production_render_performed must remain false")

    summary = data.get("summary") or {}
    if int(summary.get("previews_rendered", 0)) != 1:
        return fail(f"expected 1 rendered preview, got {summary.get('previews_rendered')}")
    if int(summary.get("previews_failed", 0)) != 0:
        return fail(f"expected 0 failed previews, got {summary.get('previews_failed')}")

    items = data.get("items") or []
    if len(items) != 1:
        return fail(f"expected exactly 1 validation item, got {len(items)}")
    item = items[0]
    if item.get("status") != "rendered":
        return fail(f"validation item status is {item.get('status')!r}")

    content = Path(item["removed_content_file"])
    result = Path(item["result_preview_file"])
    for label, media in [("removed content", content), ("result preview", result)]:
        if not media.exists():
            return fail(f"missing {label}: {media}")
        if media.stat().st_size <= 1024:
            return fail(f"{label} is unexpectedly tiny: {media.stat().st_size} bytes")

    planned = float(item["duration_s"])
    actual_content = float(item.get("removed_content_duration_s") or 0.0)
    actual_result = float(item.get("result_preview_duration_s") or 0.0)
    if abs(actual_content - planned) > 0.75:
        return fail(f"removed-content duration {actual_content:.3f}s differs too much from planned {planned:.3f}s")
    # Synthetic test requests 1s of context on both sides.
    if not (1.5 <= actual_result <= 2.5):
        return fail(f"result-preview duration should be about 2s, got {actual_result:.3f}s")

    print(f"[PASS] removed-content render: {actual_content:.3f}s for planned {planned:.3f}s")
    print(f"[PASS] resulting-edit splice preview: {actual_result:.3f}s")
    print("[PASS] production cut plan and production render remain untouched")
    print("\nCV-5 edit-validation checks: 3/3 passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
