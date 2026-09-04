# Live camera chrome (Lumen QA)

Pixel / structure pass for Continuity PiP + calibration stage. Screenshots from static UI preview (`npm run dev` → headless Chrome).

| File | Scene |
| --- | --- |
| `calibration_frame.png` | P0 calibration with demo front frame (full-bleed + vignette, rings, fingertip, glass HUD) |
| `calibration_empty.png` | P0 without frame — dark placeholder + `打开摄像头…` |
| `gesture_pip.png` | P1 gesture-active Continuity PiP (~180×120) |
| `recording_pip.png` | P1 recording PiP (REC + waveform) + tray `录音 · Ns` |

Preview URLs:

```text
http://127.0.0.1:1420/calibration.html?demo=frame
http://127.0.0.1:1420/calibration.html
http://127.0.0.1:1420/live.html?scene=gesture
http://127.0.0.1:1420/live.html?scene=recording
http://127.0.0.1:1420/pip.html?mode=gesture&demo=frame
```
