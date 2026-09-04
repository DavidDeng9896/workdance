#!/usr/bin/env bash
# Optional: download a local hand-landmarker ONNX for feature `ort-hands`.
# Models are NOT committed. Typical size ~5–15 MB depending on the asset.
set -euo pipefail

DEST_DIR="${WORKDANCE_MODEL_DIR:-${XDG_DATA_HOME:-$HOME/.local/share}/workdance/models}"
mkdir -p "$DEST_DIR"
DEST="$DEST_DIR/hand_landmarker.onnx"

if [[ -f "$DEST" ]]; then
  echo "already present: $DEST"
  exit 0
fi

URL="${WORKDANCE_HAND_ONNX_URL:-}"

if [[ -z "$URL" ]]; then
  cat <<EOF
No WORKDANCE_HAND_ONNX_URL set.

Place a hand-landmarker ONNX at:
  $DEST

Then build with:
  cargo check -p workdance-vision --features ort-hands

WP1 CI uses the deterministic stub backend (no download required).
EOF
  exit 0
fi

echo "downloading → $DEST"
curl -fL --retry 3 -o "$DEST.partial" "$URL"
mv "$DEST.partial" "$DEST"
ls -lh "$DEST"
