# AdSlicer Productization Pass 07 — Native Release Packaging

## Purpose

Make the current AdSlicer plateau reliably buildable as a macOS release on the validated Apple Silicon development machine without changing detection behavior.

## Failure fixed

The previous default `npm run build` called `build.sh` with no argument. On macOS that defaulted to `mac-universal`, which attempted both:

- `aarch64-apple-darwin`
- `x86_64-apple-darwin`

The installed Homebrew OpenCV 4.14 libraries are arm64. Rust can compile AdSlicer for x86_64, but the final linker cannot link arm64 OpenCV dylibs into an x86_64 executable. This produced `found architecture 'arm64', required architecture 'x86_64'` and undefined OpenCV symbols.

## New release contract

```bash
npm run build
# or
npm run release
```

On an Apple Silicon Mac this now builds only:

```text
aarch64-apple-darwin
```

Intel and universal macOS builds remain explicit advanced targets and require matching x86_64 OpenCV libraries.

## Explicitly unchanged

- OpenCV frame evidence
- temporal detection
- raised-VHS-black recovery
- boundary scoring
- structural segmentation
- broadcast-edge recovery from Pass 06
- FFmpeg rendering
- progress / ETA
- Stop behavior
- UI layout
- Legacy compatibility behavior

## Release status

This is a packaging/productization fix, not a new detector pass. It is appropriate for preserving the current working plateau as a release snapshot while broader 1.0 work (older-source validation, documentation, licensing, signing/notarization, and installer validation) continues.
