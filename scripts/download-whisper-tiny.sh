#!/usr/bin/env bash
# Download ggml-tiny for optional `whisper` feature. Models are NOT committed.
set -euo pipefail
DEST_DIR="${WORKDANCE_MODEL_DIR:-${XDG_DATA_HOME:-$HOME/.local/share}/workdance/models}"
mkdir -p "$DEST_DIR"
OUT="$DEST_DIR/ggml-tiny.bin"
if [[ -f "$OUT" ]]; then
  echo "already present: $OUT"
  exit 0
fi
URL="${WHISPER_TINY_URL:-https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-tiny.bin}"
echo "downloading whisper tiny → $OUT"
curl -L --fail -o "$OUT" "$URL"
echo "done: $OUT"
echo "enable with: cargo check -p workdance-input --features whisper"
