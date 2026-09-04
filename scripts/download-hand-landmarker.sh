#!/usr/bin/env bash
# Download MediaPipe Hand Landmarker `.task` for feature `mediapipe-hands`.
# Models are NOT committed. Default destination:
#   ${XDG_DATA_HOME:-$HOME/.local/share}/workdance/models/hand_landmarker.task
#
# Locked asset (WP-M1):
#   URL:    https://storage.googleapis.com/mediapipe-models/hand_landmarker/hand_landmarker/float16/1/hand_landmarker.task
#   SHA256: fbc2a30080c3c557093b5ddfc334698132eb341044ccee322ccf8bcf3607cde1
#
# Note: no stable lite `.task` bundle is published on the MediaPipe CDN; float16
# full (~7.5 MB) is the pinned production asset. Optional ORT `.onnx` remains a
# separate fallback (set WORKDANCE_HAND_ONNX_URL).
set -euo pipefail

DEST_DIR="${WORKDANCE_MODEL_DIR:-${XDG_DATA_HOME:-$HOME/.local/share}/workdance/models}"
mkdir -p "$DEST_DIR"
DEST="$DEST_DIR/hand_landmarker.task"

URL="${WORKDANCE_HAND_TASK_URL:-https://storage.googleapis.com/mediapipe-models/hand_landmarker/hand_landmarker/float16/1/hand_landmarker.task}"
SHA256_EXPECTED="${WORKDANCE_HAND_TASK_SHA256:-fbc2a30080c3c557093b5ddfc334698132eb341044ccee322ccf8bcf3607cde1}"

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

if [[ -f "$DEST" ]]; then
  GOT="$(sha256_file "$DEST")"
  if [[ "$GOT" == "$SHA256_EXPECTED" ]]; then
    echo "already present + sha256 ok: $DEST"
    exit 0
  fi
  echo "checksum mismatch for existing file; re-downloading" >&2
  rm -f "$DEST"
fi

echo "downloading → $DEST"
echo "  url: $URL"
curl -fL --retry 3 -o "$DEST.partial" "$URL"
GOT="$(sha256_file "$DEST.partial")"
if [[ "$GOT" != "$SHA256_EXPECTED" ]]; then
  rm -f "$DEST.partial"
  echo "SHA256 mismatch:" >&2
  echo "  expected: $SHA256_EXPECTED" >&2
  echo "  got:      $GOT" >&2
  exit 1
fi
mv "$DEST.partial" "$DEST"
ls -lh "$DEST"
echo "sha256 ok: $GOT"

# Optional ORT fallback asset (not required for mediapipe-hands).
ONNX_DEST="$DEST_DIR/hand_landmarker.onnx"
ONNX_URL="${WORKDANCE_HAND_ONNX_URL:-}"
if [[ -n "$ONNX_URL" && ! -f "$ONNX_DEST" ]]; then
  echo "downloading ORT fallback → $ONNX_DEST"
  curl -fL --retry 3 -o "$ONNX_DEST.partial" "$ONNX_URL"
  mv "$ONNX_DEST.partial" "$ONNX_DEST"
  ls -lh "$ONNX_DEST"
fi
