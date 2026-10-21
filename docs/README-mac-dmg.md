# macOS .dmg (CI, unsigned)

Built by `.github/workflows/mac-dmg.yml` on macos-14 arm64, attached to prerelease `wd-mac-<sha>`.

## Install
Drag to `/Applications`, then `xattr -cr /Applications/OpenLess.app` (or right-click → Open). No env vars needed for the gesture-capable builds (`f98de30` and later).

## What's inside (gesture-capable)
- `Contents/Frameworks/libmediapipe.dylib` (MediaPipe 0.10.35 C API, from Google's official arm64 wheel)
- `Contents/Resources/models/hand_landmarker.task`

Do **not** set `MEDIAPIPE_LIB` / `WORKDANCE_HAND_MODEL` for a normal install — those are optional overrides for developers. The app looks in the bundle first (after any explicit override). If the lib or model is missing, the tray shows 「手势没装好」, not 「安静」.

## Local rebuild
Run `scripts/fetch-mac-bundle-deps.sh` before `npx tauri build` on a Mac. Optional CI `workflow_dispatch` input `mediapipe_from_source=true` builds the dylib with bazel from v0.10.35 instead of the wheel.
