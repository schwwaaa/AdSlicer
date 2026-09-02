# Real WCPX/CBS Boundary Failure Study — 1995 corpus

Six real output clips were supplied after Productization Pass 05 to study missed top-level boundaries. The clips are not bundled in this source package.

## Observed failure classes

### 1. Strong separator + weak aggregate context

The existing structural classifier requires both a strong separator score and a 3-second before/after context-change score. Real ads can have very different imagery while still sharing similar *average* luminance/color/texture statistics.

Example: `segment_0010` contains two ~30-second ads with a clear black separator at ~30.0s. Measured surrounding-context score is only ~0.155, below the existing 0.20 structural threshold. The separator is therefore likely seen but left ambiguous.

Similar candidate behavior exists around ~29.85s in `segment_0020` and ~29.98s in `segment_0023`.

### 2. Broadcast cadence at show tails

`segment_0005` transitions from program material into a short show/station bumper, promo/news material, and commercials. Strong dark transitions occur around approximately:

- 566.7–568.8s
- 572.7–573.2s
- 588.1–590.1s
- ~618.1s

The spacing between candidate boundaries contains characteristic ~5/15/30-second broadcast timing. Candidate-to-candidate cadence is useful evidence that the separators are top-level pieces rather than unrelated internal fades.

### 3. Mostly-black credits/title material masks real edges

`segment_0009` contains show/credit material on a black field followed by a hard reset into a promo/news piece around ~483.2s. The dark-credit interval itself is grouped as dark temporal evidence, but the true *exit edge* has a very strong measured frame reset (~67 mean luma-difference units).

`segment_0014` contains roughly:

- 0–30s: commercial
- ~30–42s: dark credits / station material
- ~42–63s: news/promo material

The entry into the credit field has a measured reset of ~16.7; the exit into the next piece reaches ~21.7. Treating the whole mostly-black credit run as one separator-like event can hide both meaningful edges.

## Fix strategy

Do **not** globally loosen black thresholds. CV-6.1a raised-VHS-black behavior remains unchanged.

Instead, Complete Segments gains two structural recovery paths:

1. **Broadcast cadence promotion** — ambiguous but strong separator events may be promoted when a neighboring candidate/source boundary produces a normal 5/10/15/20/30/45/60/90-second broadcast interval. A small context-change floor (0.08) remains mandatory so exact timing alone cannot split the existing same-scene synthetic internal-fade regression.
2. **Structured-dark edge recovery** — dark events with visible spatial structure (credits/title cards) can contribute their entry/exit edges when the edge has a meaningful visual reset and either a strong reset or broadcast-duration support.

The original Complete Segments structural rule remains first priority. These are fallback recovery paths only.

## Approximate regression targets

| Clip | Expected recovered boundary/boundaries |
|---|---|
| `segment_0005` | ~573.0, ~588.2, ~618.2s (the earlier ~566.8s show-tail fade is observed but not required as a separate top-level piece) |
| `segment_0009` | ~483.2s |
| `segment_0010` | ~30.0s |
| `segment_0014` | ~30.0 and ~42.3s |
| `segment_0020` | ~29.85s |
| `segment_0023` | ~29.98s |

A dedicated `test-problem-clips.sh` runner is included. It requires the six external media files and checks Complete Segments boundaries with a ±1.25s tolerance.
