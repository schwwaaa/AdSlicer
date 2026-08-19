#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")" && pwd)"
OUT="$ROOT/corpus"
mkdir -p "$OUT"

FFMPEG="${FFMPEG:-ffmpeg}"
SIZE="320x240"
RATE="30"
ENC=( -c:v ffv1 -level 3 -pix_fmt yuv444p )

say() { printf '\n== %s ==\n' "$1"; }

# 01: clean one-second black slug between two obviously different pictures.
say "01 clean black (30 frames)"
"$FFMPEG" -hide_banner -loglevel error -y \
  -f lavfi -i "testsrc2=size=${SIZE}:rate=${RATE}:duration=3" \
  -f lavfi -i "color=c=black:size=${SIZE}:rate=${RATE}:duration=1" \
  -f lavfi -i "smptebars=size=${SIZE}:rate=${RATE}:duration=3" \
  -filter_complex "[0:v][1:v][2:v]concat=n=3:v=1:a=0[v]" -map "[v]" "${ENC[@]}" \
  "$OUT/01_clean_black.mkv"

# 02: exactly one black frame. This is the key regression case for the new engine.
say "02 single black frame"
"$FFMPEG" -hide_banner -loglevel error -y \
  -f lavfi -i "testsrc2=size=${SIZE}:rate=${RATE}:duration=3" \
  -f lavfi -i "color=c=black:size=${SIZE}:rate=${RATE}:duration=0.033333333333" \
  -f lavfi -i "smptebars=size=${SIZE}:rate=${RATE}:duration=3" \
  -filter_complex "[0:v][1:v][2:v]concat=n=3:v=1:a=0[v]" -map "[v]" "${ENC[@]}" \
  "$OUT/02_single_black_frame.mkv"

# 03: two raised-black frames. With the default probe settings these should be
# near-black evidence but should not behave like digital 0-level black.
say "03 two-frame near-black"
"$FFMPEG" -hide_banner -loglevel error -y \
  -f lavfi -i "testsrc2=size=${SIZE}:rate=${RATE}:duration=3" \
  -f lavfi -i "color=c=0x181818:size=${SIZE}:rate=${RATE}:duration=0.066666666667" \
  -f lavfi -i "smptebars=size=${SIZE}:rate=${RATE}:duration=3" \
  -filter_complex "[0:v][1:v][2:v]concat=n=3:v=1:a=0[v]" -map "[v]" "${ENC[@]}" \
  "$OUT/03_two_frame_near_black.mkv"

# 04: controlled 8-frame luminance valley. It approaches dark gray and rises
# again, providing a deterministic temporal-pattern test independent of fades.
say "04 temporal luminance valley"
VALLEY_TMP="$OUT/.04_valley_only.mkv"
"$FFMPEG" -hide_banner -loglevel error -y \
  -f lavfi -i "nullsrc=size=${SIZE}:rate=${RATE},geq=lum='if(eq(N,0),80,if(eq(N,1),60,if(eq(N,2),40,if(eq(N,3),24,if(eq(N,4),18,if(eq(N,5),28,if(eq(N,6),50,70)))))))':cb=128:cr=128" \
  -frames:v 8 "${ENC[@]}" "$VALLEY_TMP"
"$FFMPEG" -hide_banner -loglevel error -y \
  -f lavfi -i "testsrc2=size=${SIZE}:rate=${RATE}:duration=3" \
  -i "$VALLEY_TMP" \
  -f lavfi -i "smptebars=size=${SIZE}:rate=${RATE}:duration=3" \
  -filter_complex "[0:v][1:v][2:v]concat=n=3:v=1:a=0[v]" \
  -map "[v]" "${ENC[@]}" "$OUT/04_temporal_luma_valley.mkv"
rm -f "$VALLEY_TMP"

# 05: a deliberately dark but textured/moving program section. Useful for
# preventing a low-luma threshold from becoming a false boundary detector.
say "05 dark textured content"
"$FFMPEG" -hide_banner -loglevel error -y \
  -f lavfi -i "testsrc2=size=${SIZE}:rate=${RATE}:duration=8" \
  -vf "trim=duration=8,eq=brightness=-0.28:contrast=0.75" "${ENC[@]}" \
  "$OUT/05_dark_textured_content.mkv"

# 06: direct picture-to-picture hard cut with no dark separator.
say "06 hard cut no black"
"$FFMPEG" -hide_banner -loglevel error -y \
  -f lavfi -i "testsrc2=size=${SIZE}:rate=${RATE}:duration=3" \
  -f lavfi -i "smptebars=size=${SIZE}:rate=${RATE}:duration=3" \
  -filter_complex "[0:v][1:v]concat=n=2:v=1:a=0[v]" -map "[v]" "${ENC[@]}" \
  "$OUT/06_hard_cut_no_black.mkv"

# 07: uniform saturated color card. It should have very low luma variance but
# its channel means make clear that it is not a black frame.
say "07 uniform blue color card"
"$FFMPEG" -hide_banner -loglevel error -y \
  -f lavfi -i "testsrc2=size=${SIZE}:rate=${RATE}:duration=3" \
  -f lavfi -i "color=c=blue:size=${SIZE}:rate=${RATE}:duration=0.5" \
  -f lavfi -i "smptebars=size=${SIZE}:rate=${RATE}:duration=3" \
  -filter_complex "[0:v][1:v][2:v]concat=n=3:v=1:a=0[v]" -map "[v]" "${ENC[@]}" \
  "$OUT/07_uniform_blue_slate.mkv"

# 08: two black separators around a 12-second synthetic "commercial" block.
# CV-4 should pair the EXIT of the first dark interval with the ENTRY of the
# second interval, rather than proposing the black slugs themselves as edits.
say "08 synthetic commercial block"
"$FFMPEG" -hide_banner -loglevel error -y \
  -f lavfi -i "testsrc2=size=${SIZE}:rate=${RATE}:duration=5" \
  -f lavfi -i "color=c=black:size=${SIZE}:rate=${RATE}:duration=0.2" \
  -f lavfi -i "smptebars=size=${SIZE}:rate=${RATE}:duration=12" \
  -f lavfi -i "color=c=black:size=${SIZE}:rate=${RATE}:duration=0.2" \
  -f lavfi -i "testsrc=size=${SIZE}:rate=${RATE}:duration=5" \
  -filter_complex "[0:v][1:v][2:v][3:v][4:v]concat=n=5:v=1:a=0[v]" -map "[v]" "${ENC[@]}" \
  "$OUT/08_commercial_block.mkv"

# 09: structural segmentation control. Three true top-level black separators
# create four complete source segments. A fourth *internal* uniform black fade
# sits in the middle of the 30-second third segment with the same visual source
# on both sides. Complete Segments should preserve that 30-second block while
# Every Separator is allowed to expose the internal fade.
say "09 structural segmentation with internal black fade"
"$FFMPEG" -hide_banner -loglevel error -y \
  -f lavfi -i "testsrc2=size=${SIZE}:rate=${RATE}:duration=3" \
  -f lavfi -i "color=c=black:size=${SIZE}:rate=${RATE}:duration=0.2" \
  -f lavfi -i "smptebars=size=${SIZE}:rate=${RATE}:duration=15" \
  -f lavfi -i "color=c=black:size=${SIZE}:rate=${RATE}:duration=0.2" \
  -f lavfi -i "testsrc=size=${SIZE}:rate=${RATE}:duration=15" \
  -f lavfi -i "color=c=black:size=${SIZE}:rate=${RATE}:duration=0.1" \
  -f lavfi -i "testsrc=size=${SIZE}:rate=${RATE}:duration=15" \
  -f lavfi -i "color=c=black:size=${SIZE}:rate=${RATE}:duration=0.2" \
  -f lavfi -i "color=c=red:size=${SIZE}:rate=${RATE}:duration=3" \
  -filter_complex "[0:v][1:v][2:v][3:v][4:v][5:v][6:v][7:v][8:v]concat=n=9:v=1:a=0[v]" \
  -map "[v]" "${ENC[@]}" "$OUT/09_structural_full_timeline.mkv"

# 10: raised-uniform VHS-black regression. The first separator is deliberately
# above the normal dark_mean_luma/near-black thresholds (RGB gray ~36) but is
# spatially flat. This reproduces the real missed AFHV -> migraine boundary:
# Complete Segments must recover it without globally raising black sensitivity.
say "10 raised uniform VHS-black separator"
"$FFMPEG" -hide_banner -loglevel error -y \
  -f lavfi -i "testsrc2=size=${SIZE}:rate=${RATE}:duration=3" \
  -f lavfi -i "color=c=0x242424:size=${SIZE}:rate=${RATE}:duration=0.5" \
  -f lavfi -i "smptebars=size=${SIZE}:rate=${RATE}:duration=15" \
  -f lavfi -i "color=c=black:size=${SIZE}:rate=${RATE}:duration=0.2" \
  -f lavfi -i "color=c=red:size=${SIZE}:rate=${RATE}:duration=3" \
  -filter_complex "[0:v][1:v][2:v][3:v][4:v]concat=n=5:v=1:a=0[v]" \
  -map "[v]" "${ENC[@]}" "$OUT/10_raised_uniform_separator.mkv"

printf '\nSynthetic OpenCV validation corpus generated in:\n  %s\n' "$OUT"
