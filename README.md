# workdance

依托普通笔记本 RGB 摄像头 + 麦克风，采用**手势空间操作 + 语音语义录入**的多模态融合交互范式。

- 锁定规格：[docs/specs/2026-09-04-workdance-locked.md](docs/specs/2026-09-04-workdance-locked.md)
- 当前实现：**WP0 工程壳** + **WP1 双档视觉**（掌存在 / 休眠↔手势激活）

## 架构

选用 **Tauri 2 + Rust backend**：

1. 设置 / 权限 / 校准多窗口 UI（Lumen dark lab-desk）。
2. 托盘三态；WP1 起 **休眠 / 手势开** 由视觉状态机驱动（录音仍为手动 stub，待 WP3）。
3. **检测在 Rust 线程**（`workdance-vision`），不进 JS 主循环。

| Crate / 路径 | 职责 |
| --- | --- |
| `crates/workdance-core` | 配置 TOML、托盘态、双档状态机（0.5s 唤醒 / 1.2s 回落 / conf≥0.6） |
| `crates/workdance-vision` | 摄像头（nokhwa）、`HandPresenceDetector` trait、stub / 可选 ORT、调度线程 |
| `apps/desktop` | Tauri 壳；把 vision tier 接到托盘 |

配置：`~/.config/workdance/config.toml`（macOS / Windows 见 `dirs::config_dir`）。

## WP0：如何跑壳

```bash
cargo check
cargo test -p workdance-core
cd apps/desktop && npm install && npm run build
cd apps/desktop && npm run tauri -- dev   # 需图形会话
```

仅 UI 预览：`cd apps/desktop && npm run dev` → http://localhost:1420

## WP1：双档视觉

### 行为（锁定规格）

| 档 | FPS | 条件 |
| --- | --- | --- |
| Sleep（默认） | 3–5（实现取 4） | 只做掌存在 + 置信度 |
| Active | 25–30（实现取 27.5） | 掌稳定 ≥0.5s 且 conf ≥0.6 |
| 回落 Sleep | — | 无有效掌 1.2s |

### Stub 模式（CI / 无摄像头）

```bash
# 强制脚本化进掌/离掌（默认脚本会触发唤醒与回落）
WORKDANCE_VISION_STUB=1 cargo test -p workdance-vision
WORKDANCE_VISION_STUB=1 cd apps/desktop && npm run tauri -- dev
```

无摄像头时，即使未设环境变量，worker 也会自动回退 stub，并在 stderr 打印原因。

### 真机摄像头

```bash
# 默认 feature `camera`（nokhwa → Linux V4L2 / macOS AVFoundation / Win MF）
cargo check -p workdance-vision
cd apps/desktop && npm run tauri -- dev
```

前置内置摄像头优先（index 0）；sleep 档打开低分辨率（约 320×240）。

Linux 可能需要：`sudo apt-get install -y v4l-utils libv4l-dev`

### 可选 ORT 手部后端（非 CI 默认）

模型**不进仓库**。下载/放置脚本：

```bash
./scripts/download-hand-landmarker.sh
# 或手动放到：
#   Linux: ~/.local/share/workdance/models/hand_landmarker.onnx
#   macOS: ~/Library/Application Support/workdance/models/hand_landmarker.onnx
# 体积通常约 5–15 MB（视具体导出而定）

cargo check -p workdance-vision --features ort-hands
```

Win/Mac 启用步骤：安装摄像头权限 → 放置 ONNX → 带 `ort-hands` 编译桌面壳。无模型时自动回退 stub。

### 托盘与手动覆盖

- 默认：vision 线程驱动 **休眠 ↔ 手势开**
- 托盘/设置里手动切状态仍可用（debug）；会设 `manual_override`，直到调用「恢复自动」/`clear_manual_override`
- **录音** 仍为手动 stub（WP3）

### 测试

```bash
cargo test -p workdance-core          # 含 dual-tier 状态机（无硬件）
WORKDANCE_VISION_STUB=1 cargo test -p workdance-vision
cargo check --workspace               # CI；无需摄像头
```

## OUT OF SCOPE（截至 WP1）

- G02–G05 / 光标注入（SendInput / CGEvent）— WP2
- ASR / 录音落盘 — WP3–WP4
- 云 API、遥测、Word/仪器适配

## Linux CI 系统依赖

```bash
sudo apt-get install -y \
  libwebkit2gtk-4.1-dev libayatana-appindicator3-dev \
  librsvg2-dev patchelf build-essential libssl-dev libgtk-3-dev \
  libv4l-dev
```
