<p align="center">
  <img src="https://github.com/schwwaaa/AdSlicer/blob/main/docs/assets/adslicer-icon-256.png?raw=true" width="176" alt="AdSlicer film-eye logo">
</p>

<h1 align="center">AdSlicer</h1>

<p align="center">
  <strong>Archival boundary detection and segment slicing for VHS, analog transfers, and long-form broadcast recordings.</strong>
</p>

<p align="center">
  <img alt="Tauri 2" src="https://img.shields.io/badge/Desktop-Tauri%202-ffcc00?style=flat-square&labelColor=080808">
  <img alt="Rust" src="https://img.shields.io/badge/Core-Rust-ffcc00?style=flat-square&labelColor=080808">
  <img alt="FFmpeg" src="https://img.shields.io/badge/Media-FFmpeg-ffcc00?style=flat-square&labelColor=080808">
  <img alt="Platforms" src="https://img.shields.io/badge/Targets-macOS%20%7C%20Windows%20%7C%20Linux-ffcc00?style=flat-square&labelColor=080808">
</p>

<p align="center">
  <a href="#quick-start">Quick Start</a> ·
  <a href="#how-it-works">How It Works</a> ·
  <a href="#building-from-source">Build</a> ·
  <a href="docs/index.html">Help Documentation</a> ·
  <a href="https://github.com/schwwaaa/AdSlicer/issues">Issues</a>
</p>

> **Naming note:** The public application name is **AdSlicer**. The repository retains its original `AdSlicer` URL so existing links, clones, and project history continue to work.

AdSlicer turns a full-length recording into a structured, reproducible segmentation plan and exports the resulting clips automatically. **Adaptive analysis** is the normal product path for VHS/broadcast material. The original FFmpeg-based **Legacy** detector remains available only under Advanced / Compatibility as a fallback for unusual sources.

It is designed for difficult analog sources—not only clean digital broadcasts. VHS noise, unstable black levels, short separators, imperfect timing, and inconsistent audio floors are treated as expected input conditions rather than edge cases.

<p align="center">
  <img src="docs/preview.png" width="100%" alt="AdSlicer help interface with Windows 95 inspired styling">
</p>

## Project status

The adaptive detection, full-coverage segmentation, batch, chapter, test-export, diagnostic, and rendering pipeline is implemented. CV-7 is the current technical plateau; the immediate work is productization, real-media validation, packaging, documentation, and licensing rather than adding an editor.

## The workflow

```text
Choose a recording or folder
        ↓
Adaptive structural analysis
        ↓
Full-coverage segmentation plan
        ↓
Automatic export
        ↓
Use the resulting clips in the destination project
```

AdSlicer is intentionally automation-first. It aims to perform the segmentation job as completely as possible without requiring timeline editing inside AdSlicer. If a source still needs creative or frame-specific cleanup, that work belongs in the user's downstream editing environment.

## Core capabilities

| Capability | What it provides |
|---|---|
| Adaptive analysis | Frame-first luminance/color/variance/delta evidence, temporal transition analysis, and structural segmentation |
| Complete Segments | Default mode that preserves complete broadcast pieces across internal fades |
| Every Separator | Alternative mode that splits at every sufficiently strong separator-like event |
| Analyze Only | Runs full analysis and writes diagnostics without rendering media |
| Test Export | Caps rendered segment duration for a quick output check while still analyzing the complete recording |
| Separate Clips | Exports the full structural timeline as individual segment files |
| Chaptered Recording | Preserves the recording and adds navigation markers |
| Batch Folder | Processes a directory of compatible recordings using one configuration |
| Simple output profiles | Preserve Video for the proven fast path, or Frame Accurate H.264 when keyframe-independent cuts are required |
| Legacy Compatibility | Original detector and manual tuning controls retained under Advanced / Compatibility |
| Live job progress | Pre-run duration estimate, current stage, recording position, frame progress, elapsed time, measured ETA, active heartbeat, and clip export progress |
| Clear activity history | Simple milestone log by default, with Verbose troubleshooting/validation detail available when needed |
| Reproducible diagnostics | Structural manifests, evidence reports, and coverage validation for troubleshooting |
| Self-contained builds | FFmpeg and FFprobe are bundled as application sidecars |

## How it works

AdSlicer's normal analysis path runs the validated frame-first chain: per-frame evidence → temporal events → boundary diagnostics → structural roles → full-timeline segmentation. It includes raised-VHS-black recovery for uniform gray analog black floors and enforces a zero-loss coverage invariant before rendering.

The original detector remains available under **Advanced / Compatibility → Legacy Compatibility**. It is a fallback and regression reference, not a co-equal normal workflow.

### Output modes

**Segments / Cut** depends on the selected engine:

- **OpenCV Adaptive:** exports every structural segment from the selected Complete Segments or Every Separator policy into `segments/`. Every source frame belongs to exactly one planned segment.
- **Legacy:** removes planned commercial blocks and exports an assembled show master, individual commercial clips, and intermediate show parts.
- Both modes retain diagnostic and metadata output.

**Chapters** preserves the source recording and embeds `Content N` / `Advertisement N` chapter markers without removing footage.

## Quick start

1. Choose **Single Recording** or **Batch Folder**.
2. Select the recording/folder and a destination.
3. Leave **Split Behavior** on **Complete Segments** unless you explicitly want every strong separator.
4. Leave **Video Output** on **Preserve Video** for the proven fast path, or choose **Frame Accurate** when precise re-encoded cut points are required.
5. After selecting a single recording, AdSlicer performs a quick metadata-only duration probe and shows a broad initial analysis estimate. Press **Analyze & Export**; after processing begins, the estimate is replaced by measured live ETA, stage, elapsed time, and exact frame progress. Use **Analyze Only** or **Test Export** when validating unusual material.

Most users should not need Advanced / Compatibility. The Activity Log defaults to **Simple**, showing only understandable processing milestones. **Verbose** mode exposes additional technical detail for troubleshooting and validation, while the full analysis reports continue to be written automatically.

## Help documentation

The bundled `docs/` site contains deeper tuning guidance, diagnostic/dataset references, use cases, and build information for the smaller percentage of users who need it. It can be opened directly or published through GitHub Pages.

## Building from source

### Requirements

- Rust toolchain
- Tauri 2 platform prerequisites for the target operating system
- OpenCV 4 + libclang for the OpenCV Adaptive engine
- A supported macOS, Windows, or Linux build environment

The validated CV-7 development environment is macOS Apple Silicon with Homebrew OpenCV 4.14. `./build.sh dev` configures Homebrew OpenCV/LLVM discovery automatically. Cross-platform OpenCV runtime packaging still requires platform validation.

FFmpeg and FFprobe do not need to be installed globally. The build helper downloads the sidecars used by the application.

### First-time setup

```bash
./build.sh setup-bins
```

### Development

```bash
npm run dev
```

`npm run dev` is a convenience wrapper around `./build.sh dev`, which configures the same Homebrew OpenCV/libclang discovery used by `test-opencv.sh`. The shell helper is a development/build tool only; packaged users launch the normal AdSlicer application and do not run shell commands.

### Release builds

The default release command builds the **native architecture of the machine doing the build**. This is intentional because OpenCV is a native C++ dependency and must match the target architecture.

```bash
npm run build              # native release for the current machine
npm run release            # same as above
npm run release:mac        # native macOS release
npm run release:mac-arm    # explicit Apple Silicon release
./build.sh mac-x86         # explicit Intel build (requires x86_64 OpenCV)
./build.sh mac-universal   # advanced: requires both arm64 + x86_64 OpenCV toolchains
./build.sh windows         # Windows x86_64
```

On an Apple Silicon Mac with Homebrew OpenCV installed under `/opt/homebrew`, the normal release target is `aarch64-apple-darwin`. A universal build cannot link that arm64 OpenCV installation into its Intel half. AdSlicer now fails early with a clear explanation rather than producing a long linker failure.

The build script also contains Linux sidecar setup and platform detection support.

## Repository layout

```text
.
├── docs/                  Help website and visual assets
├── src/                   HTML/CSS/JavaScript application interface
├── src-tauri/
│   ├── presets/           Built-in detection presets
│   ├── src/adslicer/      Detection, planning, logging, and export engine
│   ├── icons/             Application bundle icons
│   └── tauri.conf.json    Tauri application configuration
├── build.sh               Sidecar setup and release build helper
└── README.md
```

## Diagnostics and dataset output

Every run records the parameters that produced it. This makes detection behavior auditable and allows separate runs to be compared without reconstructing the original application state.

`dataset.jsonl` contains timing, boundary context, silence coverage, uniform-frame coverage, scene-change statistics, signal flags, classification labels, confidence, run parameters, and run-level summary fields. It can be loaded directly into pandas:

```python
import pandas as pd

df = pd.read_json("logs/dataset.jsonl", lines=True)
low_confidence = df[
    (df["label"] == "commercial") &
    (df["confidence"] < 0.90)
]
```

## Current direction

### Productization direction

The current direction is deliberate reduction rather than adding an internal editor. AdSlicer should automatically get recordings as close to usable segmentation as possible, with a small normal interface and deeper compatibility/diagnostic controls hidden under Advanced.

Near-term work is limited to:

- Simplifying and refining the retro interface
- Testing a wider real-media corpus
- Fixing reproducible failures rather than speculative edge cases
- Polishing installation, packaging, and update behavior
- Clear documentation and licensing for a perpetual desktop product
- Maintaining Adaptive as the normal path while Legacy remains a hidden fallback during rollout

A transport/timeline editor is not part of the current 1.0 plan.

### Adaptive production integration

CV-7 promoted the validated frame-first OpenCV pipeline into the normal application path. The default **Complete Segments** policy preserves complete broadcast pieces across internal fades; **Every Separator** remains available for archival splitting at every strong separator-like event. Legacy remains hidden under Advanced / Compatibility during rollout.

## Reporting problems

Use the [issue tracker](https://github.com/schwwaaa/AdSlicer/issues) for reproducible bugs and focused feature requests. Useful reports include:

- Operating system and application build
- Source format and approximate recording duration
- Preset and modified parameters
- Relevant activity-log output
- A redacted `run_manifest.json` or `detect.json`
- A description of the expected and observed boundary behavior

Do not upload copyrighted source recordings unless you own them or have permission to share them.

## Acknowledgements

AdSlicer is built with [Tauri](https://tauri.app/), Rust, and [FFmpeg](https://ffmpeg.org/). Its multi-signal commercial-detection strategy is informed by established broadcast-detection techniques, including concepts used by Comskip, while maintaining its own planner, data model, interface, export workflow, and archival focus.

---

<p align="center">
  <strong>AdSlicer</strong><br>
  Preserve the broadcast. Automate the boundary. Export with intent.
</p>

---

## CV-3 — Shadow Boundary Candidates

CV-3 consumes the temporal JSON created by CV-2 and ranks potential boundary
points. It remains shadow-only and does not alter AdSlicer's production plan.

### Synthetic validation

From `tools/cv-validation`:

```bash
./run_probe_suite.sh
./run_temporal_suite.sh
./run_boundary_suite.sh
```

Expected final line:

```text
CV-3 boundary checks: 7/7 passed
```

### Real cached evidence

If CV-2 produced:

```text
cv-real-temporal/opencv_temporal_events.json
```

run from the project root:

```bash
cargo run \
  --manifest-path src-tauri/Cargo.toml \
  --example adslicer_cv_boundary \
  -- "cv-real-temporal/opencv_temporal_events.json" \
     "cv-real-boundary"
```

Outputs:

```text
cv-real-boundary/opencv_boundary_candidates.json
cv-real-boundary/opencv_boundary_summary.json
cv-real-boundary/opencv_boundary_candidates.csv
cv-real-boundary/opencv_boundary_evidence_only.csv
```

The `boundary_score` is a versioned development heuristic, not a calibrated probability.

## OpenCV development validation

For current OpenCV development passes, use the stable root-level test entry point:

```bash
./test-opencv.sh "/path/to/video.mp4"
```

Run the complete synthetic regression with:

```bash
./test-opencv.sh --synthetic
```

See `docs/development/CV3_UNIFIED_TESTING.md` for details.

---

## Unified OpenCV validation command

For CV-1 through CV-3 testing, use the source video directly:

```bash
./test-opencv.sh --input "/path/to/video.mp4"
```

Optional custom output:

```bash
./test-opencv.sh --input "/path/to/video.mp4" --output "/path/to/output"
```

Synthetic regression:

```bash
./test-opencv.sh --synthetic
```

Do not manually chain JSONL/JSON files between CV stages. Intermediate files are diagnostics only. The primary result is `OPENCV_TEST_REPORT.txt` in the generated output folder.

---

## CV-6 — Structural Segmentation validation

Current OpenCV development now runs CV-1 through CV-6 from the same command:

```bash
./test-opencv.sh --input "/path/to/video.mp4"
```

CV-6 separates **separator evidence** from **structural role** and produces two full-coverage
plans:

- **Complete Segments** — conservative top-level segmentation intended to keep full commercials/promos/program pieces intact.
- **Every Separator** — diagnostic mode that exposes every sufficiently strong separator-like event, including ambiguous internal fades.

Every frame remains accounted for. CV-6 reports uncovered and overlapping duration and
requires full timeline coverage.

Full validation media is written under:

```text
cv-test-output/<source>/06-structural-segmentation/renders/
```

Use `--segment-render-mode complete|every|both|none` to control those disposable validation
renders. See `docs/development/CV6_STRUCTURAL_SEGMENTATION.md`.

## CV-6.1 raised-black recovery

CV-6.1 adds a conservative VHS-specific recovery path for fades that settle at a
raised gray floor. A frame can now contribute dark evidence when it is both
`mean_luma <= 40` and exceptionally uniform (`stddev_luma <= 6`), even if it does
not meet the normal near-black pixel-ratio threshold. This was added from a real
missed boundary around 3.14 seconds in the validation source. Structural role
classification remains separate, so this does not mean every uniform dark fade
becomes a Complete Segments boundary.

The testing interface is unchanged:

```bash
./test-opencv.sh --synthetic
./test-opencv.sh --input "/path/to/video.mp4"
```

## CV-6.1a regression fix

Raised-gray VHS recovery now requires a sustained run (default: 3 analyzed frames)
before the secondary raised-uniform fallback is treated as dark evidence. This
prevents a single uniform shoulder frame from widening an ordinary fade valley,
while retaining strict/near-black 1–2 frame separator support.

## Productization Pass 06 — Real Broadcast Edge Recovery

Real 1995 WCPX/CBS validation exposed a class of Complete Segments misses around
show tails, 30-second ad pairs, black-background credits, and short station/promo
material. Pass 06 keeps the CV-6.1a detector thresholds intact and adds structural
fallback evidence from broadcast-duration cadence and hard entry/exit edges around
structured dark material. See `PROBLEM_CLIP_STUDY.md` and `PRODUCTIZATION_PASS_06.md`.
