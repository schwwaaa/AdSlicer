# CV-2 Verification Record

## Baseline

CV-2 is built on the accepted CV-1 package that was itself rebuilt from the user's
most up-to-date `AdSlicer.zip`. The FFmpeg/Tauri sidecars remain present.

## Protected production files

The following files were SHA-256 checked against CV-1 and are unchanged:

- `src-tauri/src/adslicer/job.rs`
- `src-tauri/src/adslicer/detect.rs`
- `src-tauri/src/adslicer/cut.rs`
- `src-tauri/src/adslicer/models.rs`
- `src/main.js`
- `src/index.html`
- `src/style.css`

Therefore CV-2 cannot alter production segmentation or rendering behavior.

## Static checks performed in the packaging environment

- all JSON files parsed successfully,
- all Python validation utilities compiled successfully,
- all shell scripts passed `bash -n`,
- all four macOS FFmpeg/FFprobe sidecars are present.

## Temporal rule reference validation

The independent Python mirror was run against the seven synthetic CV-1 controls:

```text
[PASS] 01_clean_black          → black_interval 90–119
[PASS] 02_single_black_frame   → black_separator 90
[PASS] 03_two_frame_near_black → near_black_separator 90–91
[PASS] 04_temporal_luma_valley → fade_valley 92–95
[PASS] 05_dark_textured_content → no temporal-dark event
[PASS] 06_hard_cut_no_black    → scene_discontinuity 90
[PASS] 07_uniform_blue_slate   → chromatic_dark 90–104

CV-2 reference temporal checks: 7/7 passed
```

## Real evidence calibration

The accepted 99.2-second, 2,974-frame CV-1 metrics were processed with the same
reference rules. They produced 41 observations:

- 7 near-black intervals,
- 1 fade valley,
- 33 scene discontinuities.

These counts are intentionally observations, not proposed edits. Ground-truth boundary
annotations are still required before CV-3 can score candidate boundaries.

## Remaining runtime gate

This packaging environment does not contain Rust/Cargo. The Rust CV-2 implementation
must therefore be compiled and exercised on the development Mac:

```bash
cd tools/cv-validation
./run_temporal_suite.sh
```

Acceptance target:

```text
CV-2 temporal checks: 7/7 passed
```

Then run `adslicer_cv_temporal` against the existing real
`opencv_frame_metrics.jsonl` and inspect the generated event CSV/JSON before moving to
CV-3.
