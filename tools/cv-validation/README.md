# AdSlicer OpenCV Validation Corpus — CV-1

This folder is the first reproducible test harness for the OpenCV development track.
It validates **measurements**, not commercial-cut decisions. The existing AdSlicer
planner remains untouched in CV-1.

## Synthetic controls

| File | What it tests |
|---|---|
| `01_clean_black.mkv` | Existing duration-style black behavior |
| `02_single_black_frame.mkv` | One-frame black separator — core AdSlicer target |
| `03_two_frame_near_black.mkv` | Raised/analog-style near-black evidence |
| `04_temporal_luma_valley.mkv` | Fall → minimum → rise pattern for temporal analysis |
| `05_dark_textured_content.mkv` | False-positive control: dark content != separator |
| `06_hard_cut_no_black.mkv` | Frame-difference evidence without black |
| `07_uniform_blue_slate.mkv` | Color evidence: low luma does not necessarily mean black |

`ground_truth.json` records expected frame ranges/events. Frame indices are zero-based.

## Generate the corpus

```bash
./generate_synthetic_corpus.sh
```

Requires FFmpeg in PATH. The generated media is lossless FFV1 at 320×240 / 30 fps
so exact frame behavior is easy to inspect.

## Run the Rust/OpenCV evidence probe

From this folder:

```bash
./run_probe_suite.sh
```

This invokes:

```bash
cargo run --example adslicer_cv_probe --features opencv-analysis -- <media> <result-dir>
```

Each result directory contains:

- `opencv_analysis_manifest.json` — engine/version/config/source metadata
- `opencv_frame_metrics.jsonl` — one complete JSON object per analyzed frame
- `opencv_frame_metrics.csv` — easy plotting/troubleshooting data

## Evaluate the evidence

```bash
python3 ./evaluate_probe_results.py
```

The evaluator currently checks:

- exact one-frame black visibility,
- strict black vs raised near-black,
- preservation of a temporal luminance valley,
- dark textured content not collapsing into near-black,
- hard-cut frame-difference response,
- uniform blue slate showing why saturation/color evidence must complement luma.

## Add real VHS controls

Create `corpus-real/` locally (it is intentionally not populated in the repository)
and add short 10–30 second excerpts around known VHS edge cases. Recommended first set:

1. clean commercial break,
2. one-frame black separator,
3. noisy/raised VHS black,
4. fast fade or fade valley,
5. tracking disturbance near a real boundary,
6. legitimately dark program content,
7. interlaced/field-weird transition,
8. solid-color station slate,
9. commercial transition with no black,
10. one difficult ambiguous example.

For every real clip, note the expected boundary frame/time and why it is interesting.
That corpus becomes the permanent regression set for CV-3/CV-4 and later.

## Compare with the current FFmpeg blackdetect baseline

The current UI defaults are `d=0.10`, `pix_th=0.08`, `pic_th=0.98`. Run:

```bash
./run_legacy_blackdetect_baseline.sh
```

On the included synthetic corpus, the expected structural weakness is visible:
the duration-first baseline does not report the exact one-frame black control or
the two-frame raised-black control. This is not a criticism of FFmpeg; it is the
specific behavior the frame-first OpenCV track is intended to complement.
