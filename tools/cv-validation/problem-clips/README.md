# 1995 WCPX/CBS Real-World Boundary Regression

These checks target real misses discovered after the CV-7 production plateau.
The media itself is intentionally **not bundled** in the source package.

Failure classes represented:

- show tail -> short station/program bumper -> promo/ad block
- strong black separators rejected because surrounding average statistics look similar
- 30-second ad -> 30-second ad boundaries
- commercial -> dark credits -> news/promo material
- mostly-black credits/title material masking the hard reset at its entry/exit

Run with the six supplied clips in one folder:

```bash
./test-problem-clips.sh "/path/to/WCPX-problem-clips"
```

The evaluator uses a ±1.25 second tolerance because the structural requirement
is to recover the correct top-level break; exact ownership of the separator
frames can still vary within the black/fade interval.
