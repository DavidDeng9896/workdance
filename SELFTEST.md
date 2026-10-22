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
| Tray id `main-tray` | Tooltip `WorkDance · 安静 / Idle` (正在听手 / 正在录音 / 刚误触停一下) |
| Capsule event channel | `workdance:tray` listened to by `Capsule.tsx` when the capsule is idle |
| Continuity PiP | Always-on-top `workdance-pip` (180×120, bottom-right): mirrored camera + blue 21-point skeleton + corner hand count; listens to `workdance:hand` |
| Accessibility denied | Tray / PiP show `需要辅助功能` (never silent Enigo failure) |

ASR is still OpenLess: local sherpa/foundry **and** the existing cloud/API providers. Gesture code does not pick a provider.

## Gate fields (debug commands)

Tauri commands (desktop):

- `workdance_gesture_set_enabled` `{ enabled: bool } -> bool`
- `workdance_gesture_gate_state` → `GateSnapshot`
- `workdance_gesture_last_decision` → `DecisionKind`
- `workdance_gesture_event_counts` → `EventCounts`

`GateSnapshot` (snake_case JSON; Aegis asserts the four bold fields):

| Field | Meaning |
| --- | --- |
| **`gesture_enabled`** | Gesture loop allowed |
| **`gate_state`** | `idle` / `tracking` / `recording` / `reject_cooldown` (= Lumen 安静 / 正在听手 / 正在录音 / 刚误触停一下) |
| `pose` | `absent` / `open_palm` / `fist` / `other` |
| `recording` | This gate owns a long-fist dictation |
| `landmark_count` | `0` or `21` |
| `fist_held_ms` | Current fist age |
| `mis_touch_remaining_ms` | Pause left after an ignored fist |
| **`last_decision`** | `none` / `move` / `click` / `dictation_pressed` / `dictation_released` |
| **`session_counts.move`** | Open-palm pointer moves |
| **`session_counts.click`** | Short-fist releases |
| **`session_counts.scroll`** | Always `0` (not implemented; G-10～12 later) |
| **`session_counts.dictation_pressed`** | Long-fist dictation start edges |
| **`session_counts.dictation_released`** | Dictation stop edges (open hand or leave frame) |
| `short_fist_max_ms` | Provisional `400` |
| `long_fist_min_ms` | Provisional `800` |
| `mis_touch_pause_ms` | Provisional `350` |
| `thresholds_provisional` | Always `true` until a tuning pass |
| `scroll_implemented` | Always `false` |

Commands still named `workdance_gesture_*`; returned JSON uses the fields above.

Host status (in-process, not a separate command) also tracks `cameraOpen`, `trackerError`, and `frontCameraMirrorX`.

## Provisional behavior (not claimed as tuned)

- Short fist: release **before 400 ms** → left click.
- **400–799 ms** → no click, no dictation, tray `reject_cooldown` for 350 ms.
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
2. Open five fingers in frame: tray tooltip becomes `WorkDance · 正在听手 / Tracking`. Cursor follows the palm. No still-hold wake.
3. Fist shorter than ~0.4 s, then open: one left click. `session_counts.click` increments. No dictation.
4. Fist held in the 0.4–0.8 s band, then open: no click. Tray `刚误触停一下 / Reject cooldown`. Pointer stays put until the pause ends.
5. Fist held ≥ ~0.8 s: dictation starts, pointer freezes. Open the hand or leave the frame: dictation stops and text inserts. `session_counts.dictation_pressed` and `dictation_released` each +1.
6. Hand leaves before a short click completes: no click.
7. `session_counts.scroll` stays 0.
