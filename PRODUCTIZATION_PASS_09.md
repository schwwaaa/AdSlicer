# AdSlicer Productization Pass 09 — Restore npm-first developer workflow

## Purpose

Correct the Pass 08 regression where `npm run dev` required release sidecars or an extra setup command.

## Contract

- `npm run dev` is the normal development command.
- Development does not require bundled FFmpeg/ffprobe sidecars.
- If matching local gitignored sidecars are present, AdSlicer may use them.
- Otherwise development uses `ffmpeg` and `ffprobe` from PATH.
- Release builds require the correct target-specific local sidecars and fail early with a clear path if they are absent.
- No detector, segmentation, UI, progress, ETA, Stop, or broadcast-edge behavior is changed.

## Commands

```bash
npm run dev
npm run test:opencv
npm run release
npm run release:mac-arm
npm run release:mac-intel
npm run release:mac-universal
```
