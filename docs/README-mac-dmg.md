# macOS .dmg (CI, unsigned)
Built by `.github/workflows/mac-dmg.yml` on macos-14 arm64, attached to prerelease `wd-mac-<sha>`.
- Open unsigned app: drag to /Applications, then `xattr -cr /Applications/OpenLess.app` (or right-click > Open).
- MediaPipe is NOT bundled (no public prebuilt libmediapipe). Gesture needs:
  - `hand_landmarker.task` (release asset) at `~/Library/Application Support/workdance/models/hand_landmarker.task`, or `WORKDANCE_HAND_MODEL=/path/hand_landmarker.task`
  - `MEDIAPIPE_LIB=/path/libmediapipe.dylib`
  Without these the app runs; gesture falls back/disabled.
