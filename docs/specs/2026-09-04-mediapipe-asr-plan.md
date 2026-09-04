# MediaPipe Hands + 离线中文 ASR 集成计划

- 日期：2026-09-04
- 状态：规划定稿（文档-only；不改 Rust/应用代码）
- 仓库：https://github.com/DavidDeng9896/workdance
- 依据：[锁定规格](./2026-09-04-workdance-locked.md)；现有 `DualTierMachine` / `AsrBackend` / `HandPresenceDetector` 契约；WP0–WP5 壳与 stub 路径

## 0. 一句话

在**不推翻现有契约**的前提下，把视觉 stub / `ort-hands` 与 ASR stub / whisper 占位，替换为**本机 MediaPipe Hands**与**离线中文 ASR**，使真机听写与掌检测可用；CI 仍走 stub。

## 1. 非协商约束（ verbatim 精神）

| 约束 | 定案 |
| --- | --- |
| 契约 | **保留** `DualTierMachine`、`AsrBackend`、`HandPresenceDetector`（及 `PalmObservation` / `AsrResult`）；**只替换 stub 实现**，不重写调度 / 注入 / 托盘状态机 |
| 视觉主路径 | 优先 **MediaPipe Tasks Hand Landmarker**，Cargo feature：`mediapipe-hands` |
| 视觉回退 | 保留现有 **`ort-hands`** 为可选回退；无模型 / 无 feature 时用 **stub**（CI / `WORKDANCE_VISION_STUB=1`） |
| 模型 | **不进 git**；提供下载脚本 + **sha256** 校验，落到本机 data 目录 |
| ASR 主路径 | 优先 **sherpa-onnx**：**SenseVoice** 或 **Paraformer-zh INT8** |
| ASR 回退 | 现有 **whisper** feature 作备份；生产听写**不得**默认 `StubAsr` |
| 隐私 | **永不上传**音频；**永不写 wav**（内存 PCM → 文本；备忘只落 MD） |
| 线程 | 视觉线程与 ASR 线程**分离**；系统注入（光标/键/追加文本）**串行**（沿用 `InjectQueue`） |
| 失败 | 缺模型 → **显式 UI 提示** + **仅语音 / 手势降级**兜底（缺视觉模型可 voice-only；缺 ASR 模型禁止假装听写成功） |
| StubAsr | **仅 CI / 无硬件演示**；不是生产听写后端 |

## 2. 现状（基线）

- `workdance-vision`：`HandPresenceDetector` + `ScriptedStubDetector`；可选 `OrtHandLandmarker`（`ort-hands`）；worker 内驱动 `DualTierMachine`
- `workdance-input`：`AsrBackend` + `StubAsr`；可选 `WhisperAsr`（占位）；G07 / 仅语音听写走内存缓冲
- `workdance-core`：`DualTierMachine`、托盘三态、`voice_only` / `asr_language`
- 模型路径约定已有雏形（如 `WORKDANCE_WHISPER_MODEL`、本地 `workdance/models/`）；本计划统一下载与校验

## 3. 视觉：MediaPipe Hands

### 3.1 选型

1. **首选**：MediaPipe Tasks **Hand Landmarker**（本机），feature `mediapipe-hands`
2. **回退**：现有 `ort-hands` ONNX 路径
3. **CI / 无模型**：`ScriptedStubDetector` / `WORKDANCE_VISION_STUB=1`

### 3.2 集成要点

- 新实现实现 `HandPresenceDetector`：帧 → `PalmObservation { present, confidence }`；置信度阈值仍由 `DualTierMachine` / 锁定规格（&lt; 0.6 无效）消费
- 不改变双档 FPS、入镜 0.5s、离镜 1.2s 逻辑
- 绑定：Win/Mac 真机主路径；Linux CI 不编 MediaPipe 或强制 stub
- 模型文件（如 `.task`）经脚本下载到 data 目录；缺失时 worker 打日志 + 前端/托盘可见错误，并允许切 `voice_only`

### 3.3 Feature 矩阵（建议）

| Feature | 用途 |
| --- | --- |
| （默认） | stub，CI 绿 |
| `mediapipe-hands` | 生产视觉首选 |
| `ort-hands` | 无 MediaPipe 时的 ONNX 回退 |

`create_default_detector()` 优先级建议：`mediapipe-hands`（模型在盘）→ `ort-hands`（模型在盘）→ stub（并上报「视觉模型缺失」）。

## 4. ASR：离线中文

### 4.1 选型

1. **首选**：sherpa-onnx — **SenseVoice** 或 **Paraformer-zh INT8**（体积与延迟适合笔记本）
2. **备份**：现有 `whisper` feature（tiny/小模型）
3. **禁止**：生产默认 `StubAsr`；Stub 文案不得伪装成真听写结果进入用户文档（CI 测试除外）

### 4.2 契约与隐私

- 继续实现 `AsrBackend::transcribe_zh(&[u8]) -> AsrResult`
- `audio_discarded` 必须为 true（策略不变）
- 禁止写临时 wav / 上传；PCM 仅内存
- `asr_language=zh` 已在配置；新后端尊重之

### 4.3 缺模型行为

- 启动或首次听写前检查模型路径 + sha256（若已记录）
- 缺失 → **设置 / 托盘 / 权限相关 UI 明确提示**（下载指引或脚本命令）
- 不静默回落 StubAsr；可选：禁用 G07 / 仅语音听写按钮直至模型就绪
- 视觉仍可用时：手势操作照常，听写不可用直至 ASR 就绪

## 5. 线程与注入

```
[Camera] → Vision thread (HandPresenceDetector + DualTierMachine)
                ↓ PalmObservation / tier events
         GestureEngine (input crate)
                ↓
         InjectQueue  ——串行——→ SendInput / CGEvent / NullInjector

[Mic PCM] → ASR thread (AsrBackend)
                ↓ AsrResult.text
         AppendText via InjectQueue（同一串行队列）
```

- 视觉与 ASR **不共享**阻塞式推理线程
- 所有系统注入（Move / Click / Scroll / AppendText）经现有串行队列，避免竞态

## 6. 模型下载（脚本，不进 git）

- 位置建议：`scripts/download-models.sh`（或拆 `download-hands.sh` / `download-asr.sh`）
- 内容：URL、目标路径、`sha256sum` 校验、失败非零退出
- README / 设置页提示运行脚本；CI **不**下载大模型
- `.gitignore` 已忽略或继续忽略 `**/models/**` 与下载产物

## 7. 工作切片

| 切片 | 内容 | 完成判据 |
| --- | --- | --- |
| **WP-M1** | `mediapipe-hands` feature + `HandPresenceDetector` 实现；缺模型显式降级 | 真机掌入镜唤醒 DualTier；CI 仍 stub 绿 |
| **WP-M2** | 下载脚本 + sha256；设置/托盘「视觉模型」状态；`ort-hands` 回退文档化 | 无模型有 UI；有模型可一键就绪 |
| **WP-A1** | sherpa-onnx SenseVoice **或** Paraformer-zh INT8 实现 `AsrBackend` | 内存 PCM → 中文文本；无 wav、无上传 |
| **WP-A2** | 缺 ASR 模型 UI；禁用假听写；whisper 作可选备份 feature | 生产不默认 StubAsr；CI 仍可用 Stub |
| **WP-P** | 端到端打磨：线程边界、失败文案、Win/Mac 验收、README | 拔网线可手势 + 听写；与锁定规格验收对齐 |

建议顺序：**WP-M1 → WP-A1 → WP-M2 / WP-A2（可并行）→ WP-P**。

## 8. 验收（相对锁定规格）

- [ ] 开局无误触：缺模型 / Sleep / voice-only 不注入 Move
- [ ] 掌检测真路径：MediaPipe（或文档声明的 ort 回退）驱动双档
- [ ] 指哪说哪：G07 / 仅语音 → 离线中文 ASR → 追加焦点
- [ ] 离线：无云；无音频落盘
- [ ] CI：Linux stub，无大模型下载，`cargo test` / check 绿
- [ ] StubAsr 仅测试与 stub 演示，不作为发布默认听写

## 9. OUT OF SCOPE

- 术语**热词** / 自定义词表
- **云 VLM** 看屏或云端 ASR
- Word / 仪器桌面**专项适配**
- 重写 `DualTierMachine` / 托盘状态机 / 注入抽象
- 把模型 blob 提交进 git
- 本计划文档之外的发行 / 签名流程

## 10. 明确不改（本 PR）

本文件与 README 链接为**文档-only**。实现落在后续 WP-M* / WP-A* / WP-P PR，不在此变更 Rust 或应用代码。
