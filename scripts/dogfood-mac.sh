#!/usr/bin/env bash
# Print Mac dogfood checklist + exact commands for wake→click→G07.
# Does not download models or start the app (safe to run anytime).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
CHECKLIST="$ROOT/docs/dogfood/2026-09-04-dogfood-checklist.md"

echo "=== WorkDance Dogfood (macOS) ==="
echo "repo: $ROOT"
echo

if [[ -f "$CHECKLIST" ]]; then
  echo "--- checklist: docs/dogfood/2026-09-04-dogfood-checklist.md ---"
  cat "$CHECKLIST"
  echo
fi

DATA_LOCAL="${XDG_DATA_HOME:-$HOME/Library/Application Support}"
HAND="$DATA_LOCAL/workdance/models/hand_landmarker.task"
ASR="$DATA_LOCAL/workdance/models/asr/paraformer-zh-small"

echo "--- exact commands ---"
cat <<EOF
# 1) Models (SHA256 pinned in scripts)
./scripts/download-hand-landmarker.sh
./scripts/download-asr-model.sh

# Expected paths (override with WORKDANCE_HAND_MODEL / WORKDANCE_MODEL_DIR / WORKDANCE_ASR_MODEL_DIR):
#   hand: $HAND
#   asr:  $ASR

# 2) Native MediaPipe lib (dlopen — not linked at compile time)
#    Extract from mediapipe PyPI wheel or self-build, then:
export MEDIAPIPE_LIB=/path/to/libmediapipe.dylib

# 3) Run desktop with both features (do NOT set *_STUB=1)
cd apps/desktop
npm install
npm run tauri -- dev --features dogfood
# equivalent: --features mediapipe-hands,sherpa-asr

# 4) Permissions: Camera + Microphone + Accessibility (System Settings)
# 5) Confirm non-stub: Settings → vision backend mediapipe-hands; ASR sherpa-asr
#    G07 text must NOT be「实验记录已追加。」
EOF

echo
echo "hand model present? $([[ -f "$HAND" ]] && echo YES || echo NO) → $HAND"
echo "asr model ready?   $([[ -f "$ASR/model.int8.onnx" && -f "$ASR/tokens.txt" ]] && echo YES || echo NO) → $ASR"
