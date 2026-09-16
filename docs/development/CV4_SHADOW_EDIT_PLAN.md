# CV-4 — Shadow Edit Planner

**Status:** development / validation

CV-4 is the first OpenCV stage that proposes *logical removal intervals*. It remains isolated from the production AdSlicer `Plan` and renderer.

## Pipeline

```text
source media
  ↓
CV-1 frame evidence
  ↓
CV-2 temporal events
  ↓
CV-3 boundary candidates
  ↓
CV-4 shadow edit plan
  ↓
comparison with current AdSlicer reset/default plan
```

The stable test interface remains:

```bash
./test-opencv.sh --input "/path/to/video.mp4"
```

No intermediate JSON/JSONL file needs to be supplied manually.

## Pairing semantics

CV-4 does not blindly pair any two high-scoring points. It respects boundary role semantics.

With the current `include_black = false` behavior:

- `interval_exit` can begin a proposed removal.
- `interval_entry` can end a proposed removal.
- a short `separator_anchor` can close one removal and begin another.
- a `fade_valley` can close one removal and begin another.
- `scene_only` and `chromatic_dark` observations cannot independently create an edit.

The final timestamp for an interval exit is corrected by one source-frame duration, because CV-3 records the PTS of the final dark frame while FFmpeg `black_end` refers to the boundary immediately after that frame.

## Planning guards

CV-4 currently mirrors the core reset/default AdSlicer planning values:

- edge pad before: `0.20 s`
- edge pad after: `0.06 s`
- minimum commercial: `5 s`
- maximum commercial: `240 s`
- minimum preserved show segment: `30 s`
- include black: `false`

These can be overridden through the unified test command for development without changing application settings.

## Legacy comparison

For a full stride-1 test, CV-4 runs one additional FFmpeg `blackdetect` pass using the current AdSlicer reset/default values:

- `d=0.10`
- `pix_th=0.08`
- `pic_th=0.98`
- merge gap `1.5 s`

Under the reset/default planner, silence, uniform-frame, and scene-change signals alter confidence but do not alter which intervals are removed. Therefore the black-detection pass plus the unchanged production `build_plan()` is sufficient to reproduce the current default removal intervals for A/B comparison.

Use `--skip-legacy` when a faster OpenCV-only run is wanted.

## Output

```text
cv-test-output/<source>/
├── OPENCV_TEST_REPORT.txt
├── OPENCV_TEST_REPORT.json
├── 01-frame-evidence/
├── 02-temporal-events/
├── 03-boundary-candidates/
└── 04-shadow-edit-plan/
    ├── opencv_shadow_plan.json
    ├── opencv_shadow_plan_summary.json
    ├── opencv_shadow_plan.csv
    ├── opencv_shadow_rejected.csv
    ├── legacy_default_plan.json
    ├── legacy_default_plan.csv
    ├── opencv_vs_legacy_comparison.json
    └── opencv_vs_legacy_matches.csv
```

The legacy/comparison files are present only when the comparison pass succeeds.

## Safety contract

CV-4 does **not**:

- modify `job.rs`, `detect.rs`, `cut.rs`, or `models.rs`;
- replace the production detector;
- replace the production planner;
- render media;
- write cuts into the application UI.

It only proposes and compares shadow intervals.

## Next gate

CV-4 should be accepted only after:

1. `./test-opencv.sh --synthetic` passes the CV-1 through CV-4 regression suite.
2. A real VHS/television sample produces a readable shadow plan.
3. The report makes OpenCV-only, legacy-only, and matched intervals obvious.
4. The proposed timestamps can be visually checked against the source media.

The next stage after acceptance is a controlled dry-run integration where an OpenCV plan can become the *selected* planning source without rendering, while the legacy planner remains available as fallback.
