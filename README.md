# workdance

依托普通笔记本 RGB 摄像头 + 麦克风，采用**手势空间操作 + 语音语义录入**的多模态融合交互范式。

- 锁定规格：[docs/specs/2026-09-04-workdance-locked.md](docs/specs/2026-09-04-workdance-locked.md)
- 集成计划：[docs/specs/2026-09-04-mediapipe-asr-plan.md](docs/specs/2026-09-04-mediapipe-asr-plan.md)
- 当前实现：**WP0–WP5（v1 壳）+ WP-M1（MediaPipe Hands）+ WP-M2（landmark→G02–G05）**

## 架构

选用 **Tauri 2 + Rust backend**：检测 / 分类 / ASR / 备忘 / 注入均在 **Rust 线程**，不进 JS。

| Crate | 职责 |
| --- | --- |
| `workdance-core` | 配置（含 `voice_only` / `asr_language`）、托盘三态、DualTier、memo |
| `workdance-vision` | 摄像头、掌存在、双档调度 |
| `workdance-input` | 手势 / 焦点 / G07–G08 / 光标门控 / InjectQueue |
| `apps/desktop` | 托盘三态、仅语音听写、设置、首启权限门禁 |

## 端到端 Demo

### Linux stub（CI / 无硬件）

```bash
cargo check --workspace
cargo test -p workdance-core
cargo test -p workdance-input
WORKDANCE_VISION_STUB=1 cargo test -p workdance-vision
WORKDANCE_VISION_STUB=1 WORKDANCE_INPUT_STUB=1 cd apps/desktop && npm run tauri -- dev
```

### Windows / macOS（真机主路径）

```bash
cd apps/desktop && npm install && npm run tauri -- dev
# 首次：权限向导 → 允许摄像头后启动视觉
# 手势：掌入镜唤醒 → G02–G05 / G07 录音 / G08 双短拳备忘
# 仅语音：设置打开「仅语音」或托盘「仅语音 · 开始听写」（无需握拳；松听写 → stub/本地 ASR 追加）
```

| 能力 | Win/Mac | Linux stub |
| --- | --- | --- |
| 双档视觉 | 真摄像头（nokhwa）或 stub | stub |
| 光标/滚轮/键 | SendInput / CGEvent | NullInjector |
| G07 听写 | stub ASR 默认；whisper 可选 | stub |
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
- G07 握拳在无视觉 Active 时不可用；改用托盘 **「仅语音 · 开始听写」** 软件武装麦克风听写（内存 → stub ASR → AppendText + 可选 memo），**无需握拳、不移光标**
- 配置持久化：`voice_only`、`asr_language=zh`

### 设置接线

灵敏度 / 死区 / 备忘路径 / 摄像头模式 / ASR 模型 / **听写语言（中文）** / 手势开关 / 仅语音 → `AppConfig`（WP1–WP4 实际读取）

### 首启

权限不完整时打开权限窗；**视觉在摄像头 Granted，或向导完成后 Unknown，或 `WORKDANCE_VISION_STUB=1` 时才启动**（Missing 不启动）。

## 验收清单（锁定规格）

| 项 | 状态 |
| --- | --- |
| 开局无误触光标 | **真**：Sleep / voice-only 不注入 Move |
| 指哪说哪（浏览器） | Win/Mac 坐标注入 **真**；焦点锁 Linux stub；听写 stub/可选 whisper |
| 离线 | **真**：无云上传；ASR/视觉本机 |
| 双端主路径 | Win SendInput / Mac CGEvent **真路径**；Linux null |

## WP-M1：MediaPipe Hands（视觉真路径骨架）

- 观测扩展为 `HandFrame { present, confidence, landmarks: Option<[HandLandmark; 21]> }`；`DualTierMachine` 仍消费 `PalmObservation`（0.5s / 1.2s / conf≥0.6 **未改**）
- Feature `mediapipe-hands`（**默认关**）：`MediaPipeHandLandmarker` 经 MediaPipe Tasks C API（`libloading` 运行时加载，不硬链）
- Sleep 档：`PresenceOnly`（无 landmarks）；Active 档：`FullLandmarks`（有则填 21 点）。前置摄像头推理前水平镜像
- 模型：`hand_landmarker.task` → `dirs::data_local_dir()/workdance/models/`（**不进 git**）
- `create_default_detector()` 优先级：mediapipe 模型在盘 → `ort-hands` → stub；缺模型 / 打开失败 → stub + **明确** `VisionBackendStatus`（设置页 banner / `get_vision_status`），禁止静默假检测
- 手势 landmarks **已**接入 G02–G05（见下方 WP-M2）；ASR / sherpa 不在本切片（WP-A1）

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

```bash
# 1) 模型
./scripts/download-hand-landmarker.sh

# 2) 提供 libmediapipe（从官方 mediapipe PyPI wheel 解出，或自建）
#    export MEDIAPIPE_LIB=/path/to/libmediapipe.so   # Linux
#    export MEDIAPIPE_LIB=/path/to/libmediapipe.dylib # macOS
#    set MEDIAPIPE_LIB=C:\path\mediapipe.dll          # Windows

# 3) 带 feature 编译桌面
cargo check -p workdance-vision --features mediapipe-hands
# apps/desktop：在 Cargo.toml 为 workdance-vision 打开 mediapipe-hands 后 tauri build/dev
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

## OUT OF SCOPE

- 云同步、术语热词、Word/仪器适配、合并本 PR 之外的发行流程
- **WP-A1** sherpa ASR（本切片不做）

## Linux CI 依赖

```bash
sudo apt-get install -y \
  libwebkit2gtk-4.1-dev libayatana-appindicator3-dev \
  librsvg2-dev patchelf build-essential libssl-dev libgtk-3-dev libv4l-dev
```
