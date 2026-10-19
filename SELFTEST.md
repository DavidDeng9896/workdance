# WorkDance × OpenLess self-test

WorkDance is one desktop app: the vendored OpenLess `beta` host
(`vendor/openless`) plus `crates/workdance-gesture`. Keyboard dictation is
unchanged. Gesture dictation reuses the same hotkey edge as mouse buttons.

Upstream OpenLess refuses to compile the Tauri shell on Linux (`compile_error!`
in `src-tauri/src/lib.rs`). This VM verifies the gate and `openless-core`.
Windows and macOS verify the shell with the command below.

## What was wired

| OpenLess symbol | WorkDance use |
| --- | --- |
| `DictationHotkeyEdge::Pressed` / `Released` | `Coordinator::dispatch_workdance_dictation_edge` → `dictation::handle_pressed_edge` / `handle_released_edge` → `OpenLessBackend::dispatch_dictation_hotkey_edge` |
| `TextInserter::insert` (and clipboard restore / unicode keystrokes / `copy_fallback`) | `workdance_gesture::IoGuard` so pointer injection cannot overlap an insert |
| Tray id `main-tray` | Tooltip `WorkDance · 安静 / Quiet` (and the other three states) |
| Capsule event channel | `workdance:tray` listened to by `Capsule.tsx` when the capsule is idle |

ASR is still OpenLess: local sherpa/foundry **and** the existing cloud/API providers. Gesture code does not pick a provider.

## Gate fields (debug commands)

Tauri commands (desktop):

- `workdance_gesture_set_enabled` `{ enabled: bool } -> bool`
- `workdance_gesture_gate_state` → `GateSnapshot`
- `workdance_gesture_last_decision` → `DecisionKind`
- `workdance_gesture_event_counts` → `EventCounts`

`GateSnapshot` (camelCase JSON):

| Field | Meaning |
| --- | --- |
| `enabled` | Gesture loop allowed |
| `tray` | `quiet` / `tracking` / `recording` / `mis_touch_pause` |
| `pose` | `absent` / `open_palm` / `fist` / `other` |
| `recording` | This gate owns a long-fist dictation |
| `landmarkCount` | `0` or `21` |
| `fistHeldMs` | Current fist age |
| `misTouchRemainingMs` | Pause left after an ignored fist |
| `lastDecision` | `none` / `move` / `click` / `dictation_pressed` / `dictation_released` |
| `counts.moveEvents` | Open-palm pointer moves |
| `counts.click` | Short-fist releases |
| `counts.scroll` | Always `0` (not implemented) |
| `counts.dictationEdge` | Pressed + released edges |
| `shortFistMaxMs` | Provisional `400` |
| `longFistMinMs` | Provisional `800` |
| `misTouchPauseMs` | Provisional `350` |
| `thresholdsProvisional` | Always `true` until a tuning pass |
| `scrollImplemented` | Always `false` |

`DecisionKind` and `EventCounts` are the same names as the commands.

Host status (in-process, not a separate command) also tracks `cameraOpen`, `trackerError`, and `frontCameraMirrorX`.

## Provisional behavior (not claimed as tuned)

- Short fist: release **before 400 ms** → left click.
- **400–799 ms** → no click, no dictation, tray `mis_touch_pause` for 350 ms.
- **≥ 800 ms** still closed → `DictationHotkeyEdge::Pressed`. Keep the fist while speaking.
- Open the hand **or** leave the frame → `Released`. If the keyboard mode ignores key-up (Toggle), the host then calls `stop_dictation` so text still inserts.
- While that session is starting, recording, transcribing, polishing, or inserting: no move, no click, no scroll.
- No hand, or confidence &lt; 0.5: no pointer, no click, no gesture dictation.
- Open palm moves the cursor on the **palm center** at any speed (deadzone `0.004` of the frame only).
- Front camera mirrors **X only** (`WORKDANCE_FRONT_CAMERA=0` disables it).
- Palm-flick scroll is **not** implemented. Reserved direction is 甲: flick up scrolls up; only X is mirrored on a front camera.

## Run the gate tests (Linux VM / CI)

```bash
cargo test -p workdance-gesture
cargo check -p workdance-gesture --features mediapipe
cargo check -p openless-core --manifest-path vendor/openless/Cargo.toml
```

`mediapipe` only type-checks the dlopen FFI. It does not need `libmediapipe` or a camera.

## Run the desktop host (Windows or macOS)

OpenLess Tauri dependencies (WebView2 on Windows, Xcode CLT on macOS) plus a C toolchain. Then:

```bash
# from the repo root
cargo check --manifest-path vendor/openless/src-tauri/Cargo.toml
```

Runtime (after `npm ci` inside `vendor/openless` if you need the webview assets):

```bash
./scripts/download-hand-landmarker.sh
export MEDIAPIPE_LIB=/path/to/libmediapipe.dylib   # macOS
# export MEDIAPIPE_LIB=/path/to/mediapipe.dll      # Windows
# model default: {data_local}/workdance/models/hand_landmarker.task
# overrides: WORKDANCE_HAND_MODEL, WORKDANCE_MODEL_DIR

cd vendor/openless
npm ci
npm run tauri dev
```

Disable gestures without removing the hotkey: `WORKDANCE_GESTURE=0`.

Keyboard hold-to-talk is the fallback and does not go through the gesture gate.

## Manual checks

1. Hotkey dictation still inserts the transcript (OpenLess provider unchanged).
2. Open five fingers in frame: tray tooltip becomes `WorkDance · 跟踪 / Tracking`. Cursor follows the palm. No still-hold wake.
3. Fist shorter than ~0.4 s, then open: one left click. `counts.click` increments. No dictation.
4. Fist held in the 0.4–0.8 s band, then open: no click. Tray `误触暂停 / Mis-touch pause`. Pointer stays put until the pause ends.
5. Fist held ≥ ~0.8 s: dictation starts, pointer freezes. Open the hand or leave the frame: dictation stops and text inserts. `counts.dictationEdge` increases by 2.
6. Hand leaves before a short click completes: no click.
7. `counts.scroll` stays 0.
