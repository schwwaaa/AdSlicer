# Productization Pass 08 — Automatic Local Sidecar Bootstrap

This pass fixes a clean-clone / clean-ZIP development failure caused by the intentional `.gitignore` rule for `src-tauri/binaries/`.

## Problem

Tauri's `externalBin` configuration requires target-named ffmpeg/ffprobe sidecars to exist before the Rust application build begins. Because `binaries/` is intentionally ignored, a fresh copy of the source could fail with:

`resource path binaries/ffmpeg-aarch64-apple-darwin doesn't exist`

## Fix

`build.sh` now checks for the required sidecars before `dev` and release builds. If the correct target pair is absent, it restores only the target-specific local ffmpeg/ffprobe binaries using the existing bootstrap functions.

Examples:

- Apple Silicon dev/release → `aarch64-apple-darwin`
- Intel Mac dev/release → `x86_64-apple-darwin`
- Windows x64 release → `x86_64-pc-windows-msvc`
- Linux x64 dev → `x86_64-unknown-linux-gnu`

The binaries remain ignored by Git and are not committed to the repository.

## Result

On a fresh source copy, the normal command remains:

```bash
npm run dev
```

The first run may download the matching ffmpeg/ffprobe sidecars. Subsequent runs reuse the local ignored copies.

No detection, segmentation, rendering policy, UI, progress, or cancellation behavior changed in this pass.
