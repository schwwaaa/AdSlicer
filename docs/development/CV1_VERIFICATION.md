# CV-1 Development Verification — Current AdSlicer Baseline

Verification performed after rebuilding CV-1 from the user-supplied current `AdSlicer.zip` on 2026-08-16.

## Baseline integrity

- Authoritative source: user-supplied `AdSlicer.zip`
- Baseline Git HEAD: `12ed5ea` (`updated ui of website`)
- Existing uncommitted `src/main.js` changes preserved
- `job.rs`: unchanged by CV-1
- `detect.rs`: unchanged by CV-1
- `cut.rs`: unchanged by CV-1
- `models.rs`: unchanged by CV-1
- frontend JS/HTML/CSS: unchanged by CV-1

See `CV1_BASELINE.md` for the exact scope.

## Completed in the artifact environment

- Synthetic corpus generation: **PASS**
- Exact generated frame counts verified with FFprobe: **PASS**
- Shell syntax (`bash -n`): **PASS**
- Python evaluator syntax: **PASS**
- JSON ground-truth/template parsing: **PASS**
- Independent OpenCV 4.13 measurement check against generated corpus: **15/15 evidence checks PASS**
- Legacy FFmpeg `blackdetect` comparison recorded using current AdSlicer UI defaults.

Generated frame counts:

```text
01_clean_black.mkv            210
02_single_black_frame.mkv     181
03_two_frame_near_black.mkv   182
04_temporal_luma_valley.mkv   188
05_dark_textured_content.mkv  240
06_hard_cut_no_black.mkv      180
07_uniform_blue_slate.mkv     195
```

Legacy FFmpeg baseline:

```text
01 clean black                detected
02 one-frame black            missed
03 two-frame near-black       missed
04 temporal luma valley       detected as 0.10 s black event
05 dark textured content      no black event
06 hard cut / no black        no black event
07 blue slate                 no black event
```

See `tools/cv-validation/legacy_blackdetect_baseline.txt` for captured output.

## Still requires target-Mac validation

The artifact environment does not provide a Rust/Cargo toolchain, so the native Rust OpenCV binding itself remains deliberately unclaimed until tested on the development Mac:

- Cargo resolves `opencv = 0.100.1` with feature `opencv-analysis`
- OpenCV discovery/linking succeeds
- `adslicer_cv_probe` compiles and runs
- Rust-generated CSV/JSONL passes `evaluate_probe_results.py` at 15/15
- ordinary AdSlicer build/run remains unchanged

Run:

```bash
cd tools/cv-validation
./run_probe_suite.sh
python3 ./evaluate_probe_results.py
```

CV-1 should only be accepted after those target-machine checks pass.
