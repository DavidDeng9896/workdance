# WorkDance Dogfood Checklist（Aegis）

- 日期：2026-09-04
- 目标：Mac/Win 真模型路径 — **唤醒 → 单击 → G07 听写**
- 前置：`main` @ e271435+；feature `mediapipe-hands` + `sherpa-asr`；模型经下载脚本 + SHA256

## A. 构建与模型

- [ ] 已运行 `./scripts/download-hand-landmarker.sh`（SHA256 通过）
- [ ] 已运行 `./scripts/download-asr-model.sh`（SHA256 通过）
- [ ] 手模型路径存在：`{data_local}/workdance/models/hand_landmarker.task`
- [ ] ASR 目录存在：`{data_local}/workdance/models/asr/paraformer-zh-small/`（含 `model.int8.onnx` + `tokens.txt`）
- [ ] 已设置 `MEDIAPIPE_LIB`（或系统能找到 `libmediapipe` / `mediapipe.dll`）
- [ ] 桌面以 features 启动：`--features dogfood`（或 `mediapipe-hands,sherpa-asr`）
- [ ] **未**设置 `WORKDANCE_VISION_STUB=1` / `WORKDANCE_ASR_STUB=1` / `WORKDANCE_INPUT_STUB=1`

## B. 权限（Mac / Win）

- [ ] **摄像头**：允许（掌检测 / 校准）
- [ ] **麦克风**：允许（G07 / 仅语音听写）
- [ ] **辅助功能 / 输入注入**（Mac Accessibility；Win 以管理员或正常输入权限运行）

## C. 端到端：wake → click → G07

- [ ] 开局 **无误触光标**（休眠 / 未唤醒时鼠标不动）
- [ ] **G01 唤醒**：掌对镜稳定 ≥0.5s → 托盘「手势开」
- [ ] **G02**：张掌平移 → 光标跟随
- [ ] **G03**：短促握拳 → 浏览器单击
- [ ] **G07**：长握 ≥1s 录音 → 松拳 → **中文听写追加**到焦点（非固定 stub 句）
- [ ] 离镜 / 无操作约 1.2s → 回落休眠

## D. 确认非 stub（必须）

### 视觉

- [ ] 设置页 / `get_vision_status`：`backend` = `mediapipe-hands`，`ok` = true
- [ ] **不是** `ScriptedStubDetector` / `backend=stub`（缺模型时会明确降级文案）

### ASR

- [ ] 设置页 / `get_asr_status`：`backend` = `sherpa-asr`，模型已安装
- [ ] G07 输出为真实转写，**不是**固定句 `实验记录已追加。`
- [ ] 转写路径无 wav/mp3 落盘；可不联网完成

## E. Linux CI（本仓）

- [ ] 默认 `cargo check --workspace` / stub tests 绿（**不**下载模型）
- [ ] `cargo check -p workdance-vision --features mediapipe-hands` 编译通过（无需本机 libmediapipe）
- [ ] `cargo check -p workdance-input --features sherpa-asr` 编译通过（compile-only；不跑摄像头）

## OUT OF SCOPE（本清单）

- CI 真开摄像头 / 真录音
- WP-A2 UI 打磨
- 新手势 / VAD 流式 ASR
