# CV-5 — Watchable Edit Validation

## Goal

Turn CV-4 shadow removal intervals into short media files that can be judged by eye and ear without promoting OpenCV into AdSlicer's production cut plan.

CV-5 is the first stage that renders media from the OpenCV shadow plan, but these renders are disposable QC artifacts only. The production AdSlicer plan and production renderer remain unchanged.

## Stable command

```bash
./test-opencv.sh --input "/path/to/video.mp4"
```

No intermediate JSON/JSONL file needs to be located or passed manually.

## New output

```text
cv-test-output/<source>/
└── 05-edit-validation/
    ├── EDIT_VALIDATION_REPORT.txt
    ├── edit_validation_manifest.json
    ├── remove-001_content.mp4
    ├── remove-001_result-preview.mp4
    └── ...
```

For each accepted CV-4 shadow removal:

- `remove-NNN_content.mp4` shows exactly what OpenCV proposes removing.
- `remove-NNN_result-preview.mp4` joins a short window immediately before the proposed removal to a short window immediately after it. This shows what the resulting edit/splice would feel like.

The default splice context is 2 seconds on each side.

## Options

```text
--preview-context S   Seconds before/after the proposed removal (default 2)
--preview-limit N     Maximum proposed edits rendered (default 20; 0 = all)
--skip-edit-previews  Run CV-1 through CV-4 only; do not render QC media
```

The preview limit is a safety guard for long recordings. It does not change the shadow plan; it only limits how many QC media pairs are rendered.

## Rendering

CV-5 uses the same bundled FFmpeg/FFprobe sidecars already shipped with AdSlicer.

Validation clips are re-encoded to portable H.264/AAC MP4 so proposed boundaries can be judged independently from stream-copy keyframe constraints. Audio is included when the source contains an audio stream.

## Reproducibility

`edit_validation_manifest.json` records:

- source interval start/end/duration,
- CV-4 interval score and tier,
- before/after context windows,
- source audio presence,
- output file paths,
- actual rendered durations,
- per-preview render success/failure,
- preview codec/settings,
- preview limit and context configuration.

## Acceptance criteria

The synthetic CV-5 regression requires:

1. one expected synthetic commercial-block shadow interval,
2. a rendered removed-content clip whose duration matches the proposed interval,
3. a rendered before→after splice preview of approximately the requested context duration,
4. no production cut-plan mutation,
5. no production render.

Run all current regression stages with:

```bash
./test-opencv.sh --synthetic
```

## Product boundary

CV-5 does **not** activate OpenCV edits inside the normal AdSlicer UI. It makes the proposed edits tangible enough to evaluate before CV-6 considers promotion into an optional active planner.
