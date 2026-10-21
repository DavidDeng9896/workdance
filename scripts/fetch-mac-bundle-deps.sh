#!/usr/bin/env bash
# Fetch macOS bundle deps into vendor/openless/src-tauri/mac-deps/ (required by
# tauri.macos.conf.json): official MediaPipe 0.10.35 libmediapipe.dylib (from the
# Google-published PyPI wheel; C API Mp*) + pinned hand_landmarker.task.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUT="$ROOT/vendor/openless/src-tauri/mac-deps"
mkdir -p "$OUT"; TMP="$(mktemp -d)"
WHL_URL="https://files.pythonhosted.org/packages/aa/c2/439e948d2a9a542498aee5d8fa3fb91ac6f4478be5d502f0c78ca3fb2333/mediapipe-0.10.35-py3-none-macosx_11_0_arm64.whl"
WHL_SHA="3b31376f34ca3665e34b834565996464cd66c9c91316e914fa7f149c891ce7ac"
TASK_URL="https://storage.googleapis.com/mediapipe-models/hand_landmarker/hand_landmarker/float16/1/hand_landmarker.task"
TASK_SHA="fbc2a30080c3c557093b5ddfc334698132eb341044ccee322ccf8bcf3607cde1"
sha() { if command -v sha256sum >/dev/null; then sha256sum "$1"; else shasum -a 256 "$1"; fi | awk '{print $1}'; }
if [[ ! -f "$OUT/libmediapipe.dylib" ]]; then
  curl -fsSL -o "$TMP/mp.whl" "$WHL_URL"
  [[ "$(sha "$TMP/mp.whl")" == "$WHL_SHA" ]] || { echo "wheel sha mismatch" >&2; exit 1; }
  unzip -q -o "$TMP/mp.whl" 'mediapipe/tasks/c/libmediapipe.dylib' -d "$TMP"
  cp "$TMP/mediapipe/tasks/c/libmediapipe.dylib" "$OUT/"
fi
if [[ ! -f "$OUT/hand_landmarker.task" ]]; then
  curl -fsSL -o "$OUT/hand_landmarker.task" "$TASK_URL"
fi
[[ "$(sha "$OUT/hand_landmarker.task")" == "$TASK_SHA" ]] || { echo "model sha mismatch" >&2; exit 1; }
if command -v install_name_tool >/dev/null; then
  install_name_tool -id @rpath/libmediapipe.dylib "$OUT/libmediapipe.dylib"
  codesign --force -s - "$OUT/libmediapipe.dylib"
fi
ls -la "$OUT"
