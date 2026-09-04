# workdance

依托普通笔记本 RGB 摄像头 + 麦克风，采用**手势空间操作 + 语音语义录入**的多模态融合交互范式。

- 锁定规格：[docs/specs/2026-09-04-workdance-locked.md](docs/specs/2026-09-04-workdance-locked.md)
- 集成计划：[docs/specs/2026-09-04-mediapipe-asr-plan.md](docs/specs/2026-09-04-mediapipe-asr-plan.md)
- WP-A1 边界：[docs/specs/2026-09-04-a1-sherpa-boundary.md](docs/specs/2026-09-04-a1-sherpa-boundary.md)
- 当前实现：**WP0–WP5（v1 壳）+ WP-M1（MediaPipe Hands）+ WP-M2（landmark→G02–G05）+ WP-A1（Sherpa ASR，feature 默认 OFF）**

## 架构

选用 **Tauri 2 + Rust backend**：检测 / 分类 / ASR / 备忘 / 注入均在 **Rust 线程**，不进 JS。

| Crate | 职责 |
| --- | --- |
| `workdance-core` | 配置（含 `voice_only` / `asr_language`）、托盘三态、DualTier、memo |
| `workdance-vision` | 摄像头、掌存在、双档调度 |
| `workdance-input` | 手势 / 焦点 / G07–G08 / 光标门控 / InjectQueue |
| `apps/desktop` | 托盘三态、仅语音听写、设置、首启权限门禁 |

## Dogfood（真模型）

Mac / Win 真机路径：打开 **`mediapipe-hands` + `sherpa-asr`**，下载并校验模型，跑通 **唤醒 → 单击 → G07**。清单：[docs/dogfood/2026-09-04-dogfood-checklist.md](docs/dogfood/2026-09-04-dogfood-checklist.md)。辅助打印：`./scripts/dogfood-mac.sh` / `pwsh ./scripts/dogfood-win.ps1`。

### Feature flags

| Feature | Crate / 桌面转发 | 默认 |
| --- | --- | --- |
| `mediapipe-hands` | `workdance-vision`；桌面 `workdance-desktop/mediapipe-hands` | **OFF** |
| `sherpa-asr` | `workdance-input`；桌面 `workdance-desktop/sherpa-asr` | **OFF** |
| `dogfood` | 桌面便捷开关 = 上述两者 | **OFF** |

```bash
# 编译检查（Linux CI 可编译；无需本机摄像头 / 真跑推理）
cargo check -p workdance-vision --features mediapipe-hands
cargo check -p workdance-input --features sherpa-asr
cargo check -p workdance-desktop --features dogfood
```

### 下载模型（SHA256 钉死；失败非零退出）

```bash
./scripts/download-hand-landmarker.sh
./scripts/download-asr-model.sh
```

| 模型 | 默认路径（`dirs::data_local_dir()`） | SHA256 |
| --- | --- | --- |
| Hand Landmarker `.task` | `{data_local}/workdance/models/hand_landmarker.task` | `fbc2a30080c3c557093b5ddfc334698132eb341044ccee322ccf8bcf3607cde1` |
| Paraformer-zh-small | `{data_local}/workdance/models/asr/paraformer-zh-small/` | 归档 `da92b3db5218c5be53aad53e57d1b6e63e7fc98a0e054fbdd6dbe18e9c6b1450` |

`data_local`：Linux `~/.local/share`；macOS `~/Library/Application Support`；Windows `%LOCALAPPDATA%`。模型 **不进 git**（`.gitignore`：`**/models/**`、`*.task`、`*.onnx`）。

### 路径 / 环境变量覆盖

| 变量 | 作用 |
| --- | --- |
| `WORKDANCE_HAND_MODEL` | 手模型 `.task` 完整路径 |
| `WORKDANCE_MODEL_DIR` | 模型目录（手模型为 `$DIR/hand_landmarker.task`） |
| `WORKDANCE_ASR_MODEL_DIR` | ASR 解压根目录（需含 `model.int8.onnx` + `tokens.txt`） |
| `MEDIAPIPE_LIB` | `libmediapipe.dylib` / `.so` / `mediapipe.dll` 路径（运行时 dlopen） |

**不要**设置 `WORKDANCE_VISION_STUB=1` / `WORKDANCE_ASR_STUB=1` / `WORKDANCE_INPUT_STUB=1`（那些是 CI / stub 演示）。

### Mac / Win 启动桌面

```bash
# 1) 模型
./scripts/download-hand-landmarker.sh
./scripts/download-asr-model.sh

# 2) MediaPipe 动态库（从 mediapipe PyPI wheel 解出或自建）
export MEDIAPIPE_LIB=/path/to/libmediapipe.dylib   # macOS
# export MEDIAPIPE_LIB=/path/to/libmediapipe.so     # Linux
# set MEDIAPIPE_LIB=C:\path\mediapipe.dll            # Windows

# 3) 带 features 跑 Tauri（apps/desktop）
cd apps/desktop && npm install
npm run tauri -- dev --features dogfood
# 等价：--features mediapipe-hands,sherpa-asr
```

Windows 可用 Git Bash 跑下载脚本，再用 PowerShell：`pwsh ./scripts/dogfood-win.ps1` 打印清单。

### 权限

| 权限 | 用途 |
| --- | --- |
| 摄像头 | 掌检测 / 校准 / Continuity |
| 麦克风 | G07 / 仅语音听写 |
| 辅助功能（Mac Accessibility）/ 输入注入 | 光标、点击、滚轮、追加文本 |

首启权限窗不完整时会提示；视觉仅在摄像头 Granted（或向导后 Unknown）时启动。

### 如何确认非 stub

| 检查 | 真路径 | stub / 假路径（失败） |
| --- | --- | --- |
| 视觉 | 设置 / `get_vision_status` → `backend=mediapipe-hands`，`ok=true` | `backend=stub` / `ScriptedStubDetector`（缺模型或未开 feature） |
| ASR | `get_asr_status` → `backend=sherpa-asr`，模型已安装；G07 为真实转写 | 固定句 **`实验记录已追加。`**（仅 `WORKDANCE_ASR_STUB=1` / 测试）；或缺模型时 `UnavailableAsr`（空文本，禁止假成功） |

端到端：掌入镜唤醒 → 短拳单击浏览器 → 长握 G07 听写追加。

### Linux CI 说明

- 默认 **不**启用 features、**不**下载模型；`WORKDANCE_VISION_STUB=1` 测 stub。
- CI 对 `mediapipe-hands` / `sherpa-asr` 做 **compile-only** `cargo check`（无需摄像头；MediaPipe 为 dlopen，缺原生库仍可编译）。
- 真机 dogfood 仅 Win/Mac；Linux 无辅助功能注入主路径（`NullInjector`）。

## 端到端 Demo

### Linux stub（CI / 无硬件）

```bash
cargo check --workspace
cargo test -p workdance-core
cargo test -p workdance-input
WORKDANCE_VISION_STUB=1 cargo test -p workdance-vision
WORKDANCE_VISION_STUB=1 WORKDANCE_INPUT_STUB=1 WORKDANCE_ASR_STUB=1 cd apps/desktop && npm run tauri -- dev
```

### Windows / macOS（真机主路径）

真模型 dogfood（MediaPipe + Sherpa）：见上方 **[Dogfood（真模型）](#dogfood真模型)**。

```bash
cd apps/desktop && npm install && npm run tauri -- dev --features dogfood
# 首次：权限向导 → 允许摄像头后启动视觉
# 手势：掌入镜唤醒 → G02–G05 / G07 录音 / G08 双短拳备忘
# 仅语音：设置打开「仅语音」或托盘「仅语音 · 开始听写」（无需握拳；松听写 → 真 ASR / Unavailable）
```

| 能力 | Win/Mac | Linux stub |
| --- | --- | --- |
| 双档视觉 | 真摄像头（nokhwa）或 stub | stub |
| 光标/滚轮/键 | SendInput / CGEvent | NullInjector |
| G07 听写 | `sherpa-asr`+模型 → 真转写；否则 Unavailable / `WORKDANCE_ASR_STUB=1` | stub（测试） |
| G08 备忘 MD | 真写入 `notes_path` | 真写入 |
| 仅语音听写 | 托盘武装听写 | 同左 |

## WP5：托盘三态 · 仅语音 · 设置

### 托盘

- **休眠**（灰）/ **手势开**（蓝）/ **录音·听写中**（红）
- 视觉驱动 Sleep↔GestureActive；G07 / 仅语音听写 → Recording
- 文案：`WorkDance · 休眠|手势开|录音`；仅语音时 `· 仅语音` / `· 听写中`
- 菜单：设置 / 校准 / 权限 / **仅语音 · 开始|结束听写** / 三态 / 退出（无工程演示口令）

### 仅语音

- `voice_only=true` → `gesture_enabled=false`，**光标注入关闭**（`GestureEngine::set_cursor_enabled(false)`）
- G07 握拳在无视觉 Active 时不可用；改用托盘 **「仅语音 · 开始听写」** 软件武装麦克风听写（内存 PCM → ASR → AppendText + 可选 memo），**无需握拳、不移光标**
- 配置持久化：`voice_only`、`asr_language=zh`

### 设置接线

灵敏度 / 死区 / 备忘路径 / 摄像头模式 / ASR 模型 / **听写语言（中文）** / 手势开关 / 仅语音 → `AppConfig`（WP1–WP4 实际读取）

### 首启

权限不完整时打开权限窗；**视觉在摄像头 Granted，或向导完成后 Unknown，或 `WORKDANCE_VISION_STUB=1` 时才启动**（Missing 不启动）。

## 验收清单（锁定规格）

| 项 | 状态 |
| --- | --- |
| 开局无误触光标 | **真**：Sleep / voice-only 不注入 Move |
| 指哪说哪（浏览器） | Win/Mac 坐标注入 **真**；焦点锁 Linux stub；听写 sherpa / Unavailable（禁 Stub 假句） |
| 离线 | **真**：无云上传；ASR/视觉本机 |
| 双端主路径 | Win SendInput / Mac CGEvent **真路径**；Linux null |

## WP-M1：MediaPipe Hands（视觉真路径骨架）

- 观测扩展为 `HandFrame { present, confidence, landmarks: Option<[HandLandmark; 21]> }`；`DualTierMachine` 仍消费 `PalmObservation`（0.5s / 1.2s / conf≥0.6 **未改**）
- Feature `mediapipe-hands`（**默认关**）：`MediaPipeHandLandmarker` 经 MediaPipe Tasks C API（`libloading` 运行时加载，不硬链）
- Sleep 档：`PresenceOnly`（无 landmarks）；Active 档：`FullLandmarks`（有则填 21 点）。前置摄像头推理前水平镜像
- 模型：`hand_landmarker.task` → `dirs::data_local_dir()/workdance/models/`（**不进 git**）
- `create_default_detector()` 优先级：mediapipe 模型在盘 → `ort-hands` → stub；缺模型 / 打开失败 → stub + **明确** `VisionBackendStatus`（设置页 banner / `get_vision_status`），禁止静默假检测
- 手势 landmarks **已**接入 G02–G05（见下方 WP-M2）；ASR 见 **WP-A1**

### 下载模型

```bash
./scripts/download-hand-landmarker.sh
# 锁定：URL + SHA256 写在脚本内；校验失败非零退出
```

| 字段 | 值 |
| --- | --- |
| URL | `https://storage.googleapis.com/mediapipe-models/hand_landmarker/hand_landmarker/float16/1/hand_landmarker.task` |
| SHA256 | `fbc2a30080c3c557093b5ddfc334698132eb341044ccee322ccf8bcf3607cde1` |

（CDN 无稳定 lite `.task`；生产钉 float16 full ≈7.5 MB。）

### Win / Mac 启用 MediaPipe

见 **[Dogfood（真模型）](#dogfood真模型)**。摘要：

```bash
./scripts/download-hand-landmarker.sh
export MEDIAPIPE_LIB=/path/to/libmediapipe.dylib   # 或 .so / mediapipe.dll
cargo check -p workdance-vision --features mediapipe-hands
# 桌面：npm run tauri -- dev --features mediapipe-hands   # 或 --features dogfood
```

Linux CI 默认 **stub 绿**（`WORKDANCE_VISION_STUB=1`）；另有 `cargo check --features mediapipe-hands`（无需本机 lib）。

## WP-M2：landmark → G02–G05

有 21 点 landmarks（MediaPipe Active / 测试 fixture）时，替换纯 stub 轨迹：

| 手势 | 映射 |
| --- | --- |
| **G02** | 食指尖（landmark 8）→ 既有平滑 / 死区 / 校准 → 屏幕光标（引擎内镜像 X） |
| **G03 / G04** | 四指 tip–MCP / PIP–MCP 曲率 → `openness` → 短拳单击 / 长拳滚动 |
| **G05** | 掌心（腕 + 四 MCP 均值）下挥 → Esc |

- **Sleep**：仍不注入光标（开局无误触）；`DualTierMachine` 阈值未改
- **无 landmarks**（stub presence-only）：保留原先脚本化 `HandSample` 路径，CI 绿
- `InjectQueue` 串行不变；Win/Mac 真注入，Linux `NullInjector`
- 单测：`workdance-input` `landmarks` 合成序列覆盖 move / click / scroll / swipe + Sleep 抑制

```bash
cargo test -p workdance-input --lib landmarks
WORKDANCE_VISION_STUB=1 cargo test -p workdance-vision
```

## WP-A1：sherpa-onnx 离线中文听写

边界：[docs/specs/2026-09-04-a1-sherpa-boundary.md](docs/specs/2026-09-04-a1-sherpa-boundary.md)

- **只换** `AsrBackend`：`SherpaAsr`（feature `sherpa-asr`，**默认 OFF**）；**不改** DualTier / G02–G05 / G07–G08 契约
- PCM：**16 kHz mono i16 LE**，≤60 s；**永不**写 wav/mp3/raw；`audio_discarded: true`
- G07 / 仅语音：整段离线 `transcribe_zh`（松拳或满 60 s / 结束听写）
- 工厂：`sherpa-asr`+模型 → `SherpaAsr`；否则仅 `cfg(test)` 或 `WORKDANCE_ASR_STUB=1` → `StubAsr`；生产缺模型 → **`UnavailableAsr`**（空文本，**禁止**注入 `实验记录已追加。`）
- 空 whisper 占位已从 `create_default_asr` 隔离（可选 `whisper` feature 仍编译，不进生产默认）
- 设置页：`get_asr_status` 显示模型已安装 / 缺失，并提示下载脚本

### 下载模型

```bash
./scripts/download-asr-model.sh
# 锁定 URL + SHA256；解压到 {data_local}/workdance/models/asr/paraformer-zh-small/
# 覆盖：WORKDANCE_ASR_MODEL_DIR=/path/to/model-root
```

| 字段 | 值 |
| --- | --- |
| 包名 | `sherpa-onnx-paraformer-zh-small-2024-03-09.tar.bz2` |
| URL | `https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/sherpa-onnx-paraformer-zh-small-2024-03-09.tar.bz2` |
| SHA256 | `da92b3db5218c5be53aad53e57d1b6e63e7fc98a0e054fbdd6dbe18e9c6b1450` |

### 启用真转写

见 **[Dogfood（真模型）](#dogfood真模型)**。摘要：

```bash
./scripts/download-asr-model.sh
cargo check -p workdance-input --features sherpa-asr
# 桌面：npm run tauri -- dev --features sherpa-asr   # 或 --features dogfood
```

CI 默认 **不**启用 `sherpa-asr`、**不**下载模型；另有 compile-only `cargo check -p workdance-input --features sherpa-asr`。默认 `cargo test` / `cargo check --workspace` 保持绿。

## 下一步（next steps）

| 切片 | 文档 | 状态 |
| --- | --- | --- |
| **WP-A1** | [sherpa-onnx 集成边界](docs/specs/2026-09-04-a1-sherpa-boundary.md) | **已实现**（本切片；feature 默认 OFF） |
| **WP-A2** | 见 [集成计划](docs/specs/2026-09-04-mediapipe-asr-plan.md) §7 | 缺模型 UI 打磨；whisper 备份产品化 |
| **A1.1** | 见 A1 边界 §1 / §4 | VAD / 流式切段（非 A1） |

## OUT OF SCOPE

- 云同步、术语热词、Word/仪器适配、合并本 PR 之外的发行流程
- VAD / 流式 ASR（**A1.1**）；DualTier / G02–G05 / G07–G08 契约变更

## Linux CI 依赖

```bash
sudo apt-get install -y \
  libwebkit2gtk-4.1-dev libayatana-appindicator3-dev \
  librsvg2-dev patchelf build-essential libssl-dev libgtk-3-dev libv4l-dev
```
