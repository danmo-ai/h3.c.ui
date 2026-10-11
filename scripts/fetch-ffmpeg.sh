#!/usr/bin/env bash
# Download static FFmpeg + FFprobe into vendor/ffmpeg/<os>-<arch>/.
# Used by `make ffmpeg` / `make pack` so the desktop app does not need Homebrew.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OS="$(uname -s | tr '[:upper:]' '[:lower:]')"
ARCH="$(uname -m)"

case "$OS-$ARCH" in
  darwin-arm64) PLATFORM="macos-arm64"; MR_OS=macos; MR_ARCH=arm64 ;;
  darwin-x86_64) PLATFORM="macos-amd64"; MR_OS=macos; MR_ARCH=amd64 ;;
  linux-aarch64|linux-arm64) PLATFORM="linux-arm64"; MR_OS=linux; MR_ARCH=arm64 ;;
  linux-x86_64) PLATFORM="linux-amd64"; MR_OS=linux; MR_ARCH=amd64 ;;
  *)
    echo "unsupported platform: $OS-$ARCH" >&2
    exit 1
    ;;
esac

OUT="$ROOT/vendor/ffmpeg/$PLATFORM"
MARK="$OUT/.version"
VERSION_TAG="${H3_FFMPEG_VERSION:-release}"

mirror_resources() {
  local res="$ROOT/desktop/src-tauri/resources"
  mkdir -p "$res"
  cp -f "$OUT/ffmpeg" "$res/ffmpeg"
  cp -f "$OUT/ffprobe" "$res/ffprobe"
  chmod +x "$res/ffmpeg" "$res/ffprobe"
  if [[ "$OS" == "darwin" ]]; then
    codesign --force --sign - "$res/ffmpeg" "$res/ffprobe" 2>/dev/null || true
    xattr -dr com.apple.quarantine "$res/ffmpeg" "$res/ffprobe" 2>/dev/null || true
  fi
  echo "mirrored  → $res/{ffmpeg,ffprobe}"
}

if [[ -x "$OUT/ffmpeg" && -x "$OUT/ffprobe" && -f "$MARK" ]]; then
  echo "ffmpeg already present: $OUT ($(cat "$MARK"))"
  mirror_resources
  exit 0
fi

TMP="$(mktemp -d)"
cleanup() { rm -rf "$TMP"; }
trap cleanup EXIT

mkdir -p "$OUT"
echo "fetching static FFmpeg/FFprobe ($PLATFORM, $VERSION_TAG)…"

fetch_one() {
  local name="$1"
  local url="https://ffmpeg.martin-riedl.de/redirect/latest/${MR_OS}/${MR_ARCH}/${VERSION_TAG}/${name}.zip"
  local zip="$TMP/${name}.zip"
  echo "  $url"
  curl -fL --retry 3 --retry-delay 1 -o "$zip" "$url"
  unzip -qo "$zip" -d "$TMP/${name}"
  local bin
  bin="$(find "$TMP/${name}" -type f -name "$name" | head -n1)"
  if [[ -z "$bin" ]]; then
    echo "missing $name inside zip" >&2
    exit 1
  fi
  install -m 755 "$bin" "$OUT/$name"
}

fetch_one ffmpeg
fetch_one ffprobe

# Ad-hoc sign so Gatekeeper / spawn works on fresh downloads.
if [[ "$OS" == "darwin" ]]; then
  codesign --force --sign - "$OUT/ffmpeg" "$OUT/ffprobe" 2>/dev/null || true
  xattr -dr com.apple.quarantine "$OUT/ffmpeg" "$OUT/ffprobe" 2>/dev/null || true
fi

echo "$VERSION_TAG $(date -u +%Y-%m-%dT%H:%MZ)" >"$MARK"
"$OUT/ffmpeg" -version | head -n1
"$OUT/ffprobe" -version | head -n1

echo "installed → $OUT"
mirror_resources
