#!/usr/bin/env bash
# ============================================================
#  AdSlicer — Universal Build Script
#  Supports: macOS Universal (arm64 + x86_64), Windows (x86_64)
#
#  Usage:
#    ./build.sh                    # Build for current OS (auto-detect)
#    ./build.sh setup-bins         # Download static ffmpeg/ffprobe sidecars
#    ./build.sh mac-universal      # macOS arm64 + x86_64 → universal .app + .dmg
#    ./build.sh mac-arm            # macOS arm64 only
#    ./build.sh mac-x86            # macOS x86_64 only
#    ./build.sh windows            # Windows MSVC x86_64 (via cross or native)
#    ./build.sh all                # All targets (requires cross-compile toolchain)
# ============================================================

set -euo pipefail

APP_NAME="AdSlicer"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TAURI_DIR="${SCRIPT_DIR}/src-tauri"
BINS_DIR="${TAURI_DIR}/binaries"

# ── Helpers ──────────────────────────────────────────────────

log()  { echo -e "\033[1;36m==> $*\033[0m"; }
ok()   { echo -e "\033[1;32m ✔  $*\033[0m"; }
warn() { echo -e "\033[1;33m ⚠  $*\033[0m"; }
fail() { echo -e "\033[1;31m ✖  $*\033[0m"; exit 1; }

require_cmd() {
  command -v "$1" >/dev/null 2>&1 || fail "Required command not found: $1"
}

require_rust_target() {
  if ! rustup target list --installed 2>/dev/null | grep -q "$1"; then
    log "Installing Rust target: $1"
    rustup target add "$1" || fail "Failed to install Rust target: $1"
  fi
}

detect_os() {
  case "$(uname -s)" in
    Darwin*) echo "mac" ;;
    Linux*)  echo "linux" ;;
    MINGW*|MSYS*|CYGWIN*) echo "windows" ;;
    *)       echo "unknown" ;;
  esac
}

# ── setup-bins: download static ffmpeg/ffprobe sidecars ──────
#
#  Fetches pre-built static binaries from BtbN's GitHub releases
#  (https://github.com/BtbN/FFmpeg-Builds) and places them in
#  src-tauri/binaries/ with the Rust target-triple naming that
#  Tauri's externalBin bundler requires.
#
#  After running this, `cargo tauri build` will automatically
#  bundle the correct binary for each target platform.

setup_bins() {
  log "Setting up ffmpeg/ffprobe sidecars in ${BINS_DIR}/"
  mkdir -p "${BINS_DIR}"

  local OS
  OS="$(detect_os)"

  # ── macOS: download arm64 and x86_64 static builds ──────────────────────────
  if [[ "${OS}" == "mac" ]]; then
    setup_bins_mac_arm64
    setup_bins_mac_x86_64

  # ── Windows: download x86_64 static build ────────────────────────────────────
  elif [[ "${OS}" == "windows" ]]; then
    setup_bins_windows_x86_64

  # ── Linux: download x86_64 static build ──────────────────────────────────────
  elif [[ "${OS}" == "linux" ]]; then
    setup_bins_linux_x86_64

  else
    warn "Unknown OS — downloading all three sets of binaries."
    setup_bins_mac_arm64
    setup_bins_mac_x86_64
    setup_bins_windows_x86_64
    setup_bins_linux_x86_64
  fi

  ok "Sidecar binaries are ready in ${BINS_DIR}/"
  echo ""
  echo "  Files present:"
  ls -lh "${BINS_DIR}" | grep -v '^total' | grep -v '.gitkeep' || true
}

# Download a file, trying curl then wget.
_download() {
  local url="$1"
  local dest="$2"
  if command -v curl >/dev/null 2>&1; then
    curl -fsSL --retry 3 -o "${dest}" "${url}"
  elif command -v wget >/dev/null 2>&1; then
    wget -q --tries=3 -O "${dest}" "${url}"
  else
    fail "Neither curl nor wget found. Please install one and retry."
  fi
}

# BtbN: Linux + Windows static builds only (no macOS).
# macOS: ffmpeg.martin-riedl.de — provides arm64 + x86_64 ZIPs with stable redirect URLs.
BTBN_BASE="https://github.com/BtbN/FFmpeg-Builds/releases/latest/download"
MARTIN_BASE="https://ffmpeg.martin-riedl.de/redirect/latest/macos"

_unzip_binary() {
  local archive="$1"
  local dest="$2"
  local bin_name="$3"
  local tmp_dir="${BINS_DIR}/_unzip_tmp"
  rm -rf "${tmp_dir}"; mkdir -p "${tmp_dir}"
  unzip -q "${archive}" -d "${tmp_dir}"
  find "${tmp_dir}" -name "${bin_name}" -exec cp {} "${dest}" \; 2>/dev/null || true
  rm -rf "${tmp_dir}"
}

setup_bins_mac_arm64() {
  local triple="aarch64-apple-darwin"
  local ff_out="${BINS_DIR}/ffmpeg-${triple}"
  local fp_out="${BINS_DIR}/ffprobe-${triple}"

  if [[ -f "${ff_out}" && -f "${fp_out}" ]]; then
    ok "macOS arm64 binaries already present — skipping."
    return
  fi

  log "Downloading macOS arm64 static ffmpeg (ffmpeg.martin-riedl.de)…"
  local ff_archive="${BINS_DIR}/ffmpeg-macos-arm64.zip"
  local fp_archive="${BINS_DIR}/ffprobe-macos-arm64.zip"

  _download "${MARTIN_BASE}/arm64/snapshot/ffmpeg.zip"  "${ff_archive}"
  _download "${MARTIN_BASE}/arm64/snapshot/ffprobe.zip" "${fp_archive}"

  _unzip_binary "${ff_archive}" "${ff_out}" "ffmpeg"
  _unzip_binary "${fp_archive}" "${fp_out}" "ffprobe"

  chmod +x "${ff_out}" "${fp_out}" 2>/dev/null || true
  rm -f "${ff_archive}" "${fp_archive}"

  [[ -f "${ff_out}" ]]  || fail "ffmpeg  macOS arm64 not found after extraction"
  [[ -f "${fp_out}" ]]  || fail "ffprobe macOS arm64 not found after extraction"
  ok "macOS arm64 binaries ready."
}

setup_bins_mac_x86_64() {
  local triple="x86_64-apple-darwin"
  local ff_out="${BINS_DIR}/ffmpeg-${triple}"
  local fp_out="${BINS_DIR}/ffprobe-${triple}"

  if [[ -f "${ff_out}" && -f "${fp_out}" ]]; then
    ok "macOS x86_64 binaries already present — skipping."
    return
  fi

  log "Downloading macOS x86_64 static ffmpeg (ffmpeg.martin-riedl.de)…"
  local ff_archive="${BINS_DIR}/ffmpeg-macos-x86_64.zip"
  local fp_archive="${BINS_DIR}/ffprobe-macos-x86_64.zip"

  _download "${MARTIN_BASE}/amd64/snapshot/ffmpeg.zip"  "${ff_archive}"
  _download "${MARTIN_BASE}/amd64/snapshot/ffprobe.zip" "${fp_archive}"

  _unzip_binary "${ff_archive}" "${ff_out}" "ffmpeg"
  _unzip_binary "${fp_archive}" "${fp_out}" "ffprobe"

  chmod +x "${ff_out}" "${fp_out}" 2>/dev/null || true
  rm -f "${ff_archive}" "${fp_archive}"

  [[ -f "${ff_out}" ]]  || fail "ffmpeg  macOS x86_64 not found after extraction"
  [[ -f "${fp_out}" ]]  || fail "ffprobe macOS x86_64 not found after extraction"
  ok "macOS x86_64 binaries ready."
}

setup_bins_windows_x86_64() {
  local triple="x86_64-pc-windows-msvc"
  local ff_out="${BINS_DIR}/ffmpeg-${triple}.exe"
  local fp_out="${BINS_DIR}/ffprobe-${triple}.exe"

  if [[ -f "${ff_out}" && -f "${fp_out}" ]]; then
    ok "Windows x86_64 binaries already present — skipping."
    return
  fi

  log "Downloading Windows x86_64 static ffmpeg…"
  local archive="${BINS_DIR}/ffmpeg-win64.zip"
  _download "${BTBN_BASE}/ffmpeg-master-latest-win64-lgpl.zip" "${archive}"

  local extract_dir="${BINS_DIR}/_extract_win64"
  rm -rf "${extract_dir}"; mkdir -p "${extract_dir}"
  unzip -q "${archive}" -d "${extract_dir}"

  find "${extract_dir}" -name "ffmpeg.exe"  -exec cp {} "${ff_out}" \; 2>/dev/null || true
  find "${extract_dir}" -name "ffprobe.exe" -exec cp {} "${fp_out}" \; 2>/dev/null || true

  rm -rf "${extract_dir}" "${archive}"

  [[ -f "${ff_out}" ]]  || fail "ffmpeg.exe  Windows x86_64 not found after extraction"
  [[ -f "${fp_out}" ]]  || fail "ffprobe.exe Windows x86_64 not found after extraction"
  ok "Windows x86_64 binaries ready."
}

setup_bins_linux_x86_64() {
  local triple="x86_64-unknown-linux-gnu"
  local ff_out="${BINS_DIR}/ffmpeg-${triple}"
  local fp_out="${BINS_DIR}/ffprobe-${triple}"

  if [[ -f "${ff_out}" && -f "${fp_out}" ]]; then
    ok "Linux x86_64 binaries already present — skipping."
    return
  fi

  log "Downloading Linux x86_64 static ffmpeg…"
  local archive="${BINS_DIR}/ffmpeg-linux-x86_64.tar.xz"
  _download "${BTBN_BASE}/ffmpeg-master-latest-linux64-lgpl.tar.xz" "${archive}"

  local extract_dir="${BINS_DIR}/_extract_linux64"
  rm -rf "${extract_dir}"; mkdir -p "${extract_dir}"
  tar -xf "${archive}" -C "${extract_dir}"

  find "${extract_dir}" -name "ffmpeg"  -exec cp {} "${ff_out}" \; 2>/dev/null || true
  find "${extract_dir}" -name "ffprobe" -exec cp {} "${fp_out}" \; 2>/dev/null || true

  chmod +x "${ff_out}" "${fp_out}" 2>/dev/null || true
  rm -rf "${extract_dir}" "${archive}"

  [[ -f "${ff_out}" ]]  || fail "ffmpeg  Linux x86_64 not found after extraction"
  [[ -f "${fp_out}" ]]  || fail "ffprobe Linux x86_64 not found after extraction"
  ok "Linux x86_64 binaries ready."
}

# ── Build functions ───────────────────────────────────────────

build_mac_arm() {
  log "Building macOS arm64 (Apple Silicon)…"
  require_rust_target "aarch64-apple-darwin"
  cd "${TAURI_DIR}"
  cargo tauri build --target aarch64-apple-darwin
  ok "macOS arm64 build complete"
}

build_mac_x86() {
  log "Building macOS x86_64 (Intel)…"
  require_rust_target "x86_64-apple-darwin"
  cd "${TAURI_DIR}"
  cargo tauri build --target x86_64-apple-darwin
  ok "macOS x86_64 build complete"
}

build_mac_universal() {
  log "Building macOS Universal Binary (arm64 + x86_64)…"
  require_cmd lipo

  build_mac_arm
  build_mac_x86

  ARM_APP="${TAURI_DIR}/target/aarch64-apple-darwin/release/bundle/macos/${APP_NAME}.app"
  INTEL_APP="${TAURI_DIR}/target/x86_64-apple-darwin/release/bundle/macos/${APP_NAME}.app"

  [[ -d "${ARM_APP}" ]]   || fail "ARM .app not found: ${ARM_APP}"
  [[ -d "${INTEL_APP}" ]] || fail "Intel .app not found: ${INTEL_APP}"

  UNIVERSAL_ROOT="${TAURI_DIR}/target/universal/release/bundle/macos"
  UNIVERSAL_APP="${UNIVERSAL_ROOT}/${APP_NAME}.app"

  log "Assembling universal .app bundle…"
  rm -rf "${UNIVERSAL_APP}"
  mkdir -p "${UNIVERSAL_ROOT}"
  cp -R "${ARM_APP}" "${UNIVERSAL_APP}"

  while IFS= read -r -d '' arm_bin; do
    rel="${arm_bin#${ARM_APP}/}"
    intel_bin="${INTEL_APP}/${rel}"
    universal_bin="${UNIVERSAL_APP}/${rel}"

    if [[ -f "${intel_bin}" ]] && file "${arm_bin}" | grep -q "Mach-O"; then
      log "  lipo: ${rel}"
      lipo -create -output "${universal_bin}" "${arm_bin}" "${intel_bin}"
    fi
  done < <(find "${ARM_APP}" -type f -print0)

  ok "Universal .app → ${UNIVERSAL_APP}"

  if command -v create-dmg >/dev/null 2>&1; then
    log "Creating DMG…"
    DMG_OUT="${TAURI_DIR}/target/universal/release/bundle/${APP_NAME}-universal.dmg"
    create-dmg \
      --volname "${APP_NAME}" \
      --window-size 540 380 \
      --icon-size 128 \
      --icon "${APP_NAME}.app" 150 185 \
      --hide-extension "${APP_NAME}.app" \
      --app-drop-link 390 185 \
      "${DMG_OUT}" \
      "${UNIVERSAL_APP}" \
    && ok "DMG → ${DMG_OUT}" \
    || warn "create-dmg failed; skipping DMG creation"
  else
    warn "create-dmg not found — skipping DMG (install with: brew install create-dmg)"
  fi
}

build_windows() {
  log "Building Windows x86_64…"

  if [[ "$(detect_os)" != "windows" ]]; then
    if command -v cargo-xwin >/dev/null 2>&1; then
      log "Cross-compiling with cargo-xwin…"
      require_rust_target "x86_64-pc-windows-msvc"
      cd "${TAURI_DIR}"
      cargo xwin build --release --target x86_64-pc-windows-msvc
    elif command -v cross >/dev/null 2>&1; then
      log "Cross-compiling with 'cross'…"
      require_rust_target "x86_64-pc-windows-gnu"
      cd "${TAURI_DIR}"
      cross build --release --target x86_64-pc-windows-gnu
    else
      warn "Neither cargo-xwin nor cross found."
      warn "To build Windows from macOS/Linux, install one:"
      warn "  cargo install cargo-xwin"
      warn "  cargo install cross && cross build …"
      warn "Or run this script natively on a Windows machine."
      exit 1
    fi
  else
    require_rust_target "x86_64-pc-windows-msvc"
    cd "${TAURI_DIR}"
    cargo tauri build --target x86_64-pc-windows-msvc
  fi

  ok "Windows build complete"
}

build_all() {
  local os
  os="$(detect_os)"
  if [[ "${os}" == "mac" ]]; then
    build_mac_universal
    build_windows
  elif [[ "${os}" == "windows" ]]; then
    build_windows
  else
    warn "Unsupported OS for 'all' target: ${os}"
    exit 1
  fi
}

build_current_os() {
  local os
  os="$(detect_os)"
  case "${os}" in
    mac)     build_mac_universal ;;
    windows) build_windows ;;
    *)       fail "Unsupported OS: ${os}. Use an explicit target argument." ;;
  esac
}

# ── Dependency checks ─────────────────────────────────────────
require_cmd cargo
require_cmd rustup

if ! cargo tauri --version >/dev/null 2>&1; then
  fail "tauri-cli not found. Install with: cargo install tauri-cli"
fi

# ── Dispatch ─────────────────────────────────────────────────
TARGET="${1:-auto}"

case "${TARGET}" in
  auto)           build_current_os ;;
  setup-bins)     setup_bins ;;
  mac-universal)  build_mac_universal ;;
  mac-arm)        build_mac_arm ;;
  mac-x86)        build_mac_x86 ;;
  windows)        build_windows ;;
  all)            build_all ;;
  *)
    echo "Usage: $0 [auto|setup-bins|mac-universal|mac-arm|mac-x86|windows|all]"
    exit 1
    ;;
esac

echo ""
ok "Done."