# AdSlicer Release Build

## Apple Silicon macOS

First-time sidecars:

```bash
npm run setup:bins
```

Build the native release:

```bash
npm run release
```

Equivalent explicit command:

```bash
./build.sh mac-arm
```

Expected Tauri output is under:

```text
src-tauri/target/aarch64-apple-darwin/release/bundle/
```

Do not use `mac-universal` on the current Apple Silicon/Homebrew OpenCV setup unless a separate x86_64 OpenCV toolchain has also been installed and configured.
