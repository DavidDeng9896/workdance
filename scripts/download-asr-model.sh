#!/usr/bin/env bash
# Download Paraformer-zh-small for Cargo feature `sherpa-asr` (WP-A1).
# Models are NOT committed. Default destination matches dirs::data_local_dir():
#   Linux:  ${XDG_DATA_HOME:-$HOME/.local/share}/workdance/models/asr/paraformer-zh-small/
#   macOS:  ~/Library/Application Support/workdance/models/asr/paraformer-zh-small/
#   Windows (Git Bash): %LOCALAPPDATA%/workdance/models/asr/paraformer-zh-small/
#
# Locked asset (WP-A1 boundary):
#   URL:    https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/sherpa-onnx-paraformer-zh-small-2024-03-09.tar.bz2
#   SHA256: da92b3db5218c5be53aad53e57d1b6e63e7fc98a0e054fbdd6dbe18e9c6b1450
#
# Override install root with WORKDANCE_ASR_MODEL_DIR (extracted model dir containing
# model.int8.onnx + tokens.txt). CI must not run this script.
set -euo pipefail

ARCHIVE_NAME="sherpa-onnx-paraformer-zh-small-2024-03-09.tar.bz2"
INNER_DIR="sherpa-onnx-paraformer-zh-small-2024-03-09"
URL="${WORKDANCE_ASR_ARCHIVE_URL:-https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/${ARCHIVE_NAME}}"
SHA256_EXPECTED="${WORKDANCE_ASR_ARCHIVE_SHA256:-da92b3db5218c5be53aad53e57d1b6e63e7fc98a0e054fbdd6dbe18e9c6b1450}"

if [[ -z "$SHA256_EXPECTED" ]]; then
  echo "SHA256_EXPECTED is empty — refuse to download without a pin" >&2
  exit 1
fi

default_data_local() {
  if [[ -n "${XDG_DATA_HOME:-}" ]]; then
    printf '%s' "$XDG_DATA_HOME"
    return
  fi
  case "$(uname -s 2>/dev/null || echo unknown)" in
    Darwin)
      printf '%s' "$HOME/Library/Application Support"
      ;;
    MINGW*|MSYS*|CYGWIN*)
      printf '%s' "${LOCALAPPDATA:-$HOME/AppData/Local}"
      ;;
    *)
      printf '%s' "$HOME/.local/share"
      ;;
  esac
}

if [[ -n "${WORKDANCE_ASR_MODEL_DIR:-}" ]]; then
  DEST="$WORKDANCE_ASR_MODEL_DIR"
else
  DEST="$(default_data_local)/workdance/models/asr/paraformer-zh-small"
fi

sha256_file() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | awk '{print $1}'
  elif command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$1" | awk '{print $1}'
  else
    echo "need sha256sum or shasum" >&2
    exit 1
  fi
}

model_ready() {
  [[ -f "$1/model.int8.onnx" && -f "$1/tokens.txt" ]]
}

if model_ready "$DEST"; then
  echo "already present: $DEST"
  ls -lh "$DEST/model.int8.onnx" "$DEST/tokens.txt"
  exit 0
fi

TMP="$(mktemp -d)"
cleanup() { rm -rf "$TMP"; }
trap cleanup EXIT

ARCHIVE="$TMP/$ARCHIVE_NAME"
echo "downloading → $ARCHIVE"
echo "  url: $URL"
curl -fL --retry 3 -o "$ARCHIVE.partial" "$URL"
mv "$ARCHIVE.partial" "$ARCHIVE"

GOT="$(sha256_file "$ARCHIVE")"
if [[ "$GOT" != "$SHA256_EXPECTED" ]]; then
  echo "SHA256 mismatch:" >&2
  echo "  expected: $SHA256_EXPECTED" >&2
  echo "  got:      $GOT" >&2
  exit 1
fi
echo "sha256 ok: $GOT"

echo "extracting…"
tar -xjf "$ARCHIVE" -C "$TMP"
SRC="$TMP/$INNER_DIR"
if ! model_ready "$SRC"; then
  echo "extracted archive missing model.int8.onnx or tokens.txt under $SRC" >&2
  ls -la "$TMP" >&2 || true
  exit 1
fi

mkdir -p "$DEST"
# Copy model assets (do not keep test_wavs — privacy / no audio retention).
cp -f "$SRC/model.int8.onnx" "$DEST/model.int8.onnx"
cp -f "$SRC/tokens.txt" "$DEST/tokens.txt"
# Optional sidecar files used by some runtimes / docs.
for f in config.yaml am.mvn README.md; do
  if [[ -f "$SRC/$f" ]]; then
    cp -f "$SRC/$f" "$DEST/$f"
  fi
done

if ! model_ready "$DEST"; then
  echo "install failed: $DEST incomplete" >&2
  exit 1
fi

ls -lh "$DEST/model.int8.onnx" "$DEST/tokens.txt"
echo "installed → $DEST"
echo "enable with: cargo check -p workdance-input --features sherpa-asr"
echo "override path: WORKDANCE_ASR_MODEL_DIR=$DEST"
