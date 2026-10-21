# macOS .dmg (CI, unsigned)
Built by `.github/workflows/mac-dmg.yml` on macos-14 arm64, attached to prerelease `wd-mac-<sha>`.
- Open unsigned app: drag to /Applications, then `xattr -cr /Applications/OpenLess.app` (or right-click > Open).
- MediaPipe is NOT bundled (no public prebuilt libmediapipe). Gesture needs:
  - `hand_landmarker.task` (release asset) at `~/Library/Application Support/workdance/models/hand_landmarker.task`, or `WORKDANCE_HAND_MODEL=/path/hand_landmarker.task`
  - `MEDIAPIPE_LIB=/path/libmediapipe.dylib`
  Without these the app runs; gesture falls back/disabled.

## Gesture-capable build (commit after the first no-gesture build)
- `scripts/fetch-mac-bundle-deps.sh` puts `libmediapipe.dylib` (official MediaPipe 0.10.35, from Google's PyPI wheel `mediapipe-0.10.35-py3-none-macosx_11_0_arm64.whl`, sha256-pinned) and `hand_landmarker.task` into `vendor/openless/src-tauri/mac-deps/`. Run it before a local mac `tauri build`. `tauri.macos.conf.json` requires these files.
- Bundle layout: `Contents/Frameworks/libmediapipe.dylib` and `Contents/Resources/models/hand_landmarker.task`.
- Lookup at runtime: lib `MEDIAPIPE_LIB` → `<exe>/../Frameworks` → `<exe>/../Resources` → cache/system; model `WORKDANCE_HAND_MODEL` → `WORKDANCE_MODEL_DIR` → bundle Resources → `~/Library/Application Support/workdance/models`.
- If either is missing the tray shows 「手势没装好」, not 「安静」.
- CI fallback: run workflow_dispatch with `mediapipe_from_source=true` to build the dylib with bazel from v0.10.35.
