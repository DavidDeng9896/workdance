# workdance

依托普通笔记本 RGB 摄像头 + 麦克风，采用**手势空间操作 + 语音语义录入**的多模态融合交互范式。

- 锁定规格：[docs/specs/2026-09-04-workdance-locked.md](docs/specs/2026-09-04-workdance-locked.md)
- 当前实现：**WP0 工程壳**（托盘 / 权限 / 校准 / 设置）

## 架构选择（WP0）

选用 **Tauri 2 + Rust backend**，而不是纯 Rust tray crate：

1. 设置 / 权限 / 校准需要多窗口中文 UI，贴近 Lumen dark lab-desk 稿；HTML/CSS 更合适。
2. 系统托盘与窗口管理是 Tauri 2 一等能力。
3. **检测与注入钩子留在 Rust**（`workdance-core` stubs）：后续视觉 / ASR / 串行注入不进 JS 主循环。

Workspace：

| Crate / 路径 | 职责 |
| --- | --- |
| `crates/workdance-core` | 配置 TOML、三态、权限占位、vision/ASR/inject 空钩子 |
| `apps/desktop` | Tauri 2 壳 + `ui/` 静态页面 |

配置文件：`~/.config/workdance/config.toml`（macOS: `~/Library/Application Support/workdance/config.toml`；Windows: `%APPDATA%\workdance\config.toml`）。

## 如何运行 WP0

### 前置

- Rust **1.88+**（推荐 `rustup default stable`）
- Node.js 20+ / npm
- Linux 额外系统库见下方 CI；macOS / Windows 按 [Tauri 前提](https://v2.tauri.app/start/prerequisites/) 安装

### 开发（桌面壳）

```bash
# 校验 Rust
cargo check
cargo test -p workdance-core

# UI 依赖 + 静态校验
cd apps/desktop && npm install && npm run build

# 启动 Tauri（托盘 + 窗口；需图形会话）
cd apps/desktop && npm run tauri -- dev
```

托盘菜单：打开设置 / 校准 / 权限；手动切换 **休眠 · 手势开 · 录音**；退出。

### 仅预览 UI（无托盘）

```bash
cd apps/desktop && npm install && npm run dev
# http://localhost:1420 → settings.html
```

### 生产构建命令

```bash
# Linux / macOS / Windows（在各自平台上）
cd apps/desktop && npm install && npm run tauri -- build
```

CI **不要求摄像头硬件**；Linux CI 只跑 `cargo check` + UI `npm run build`。

## OUT OF SCOPE（WP0）

- MediaPipe / ONNX / 任何视觉推理
- 离线 ASR 模型与录音管线
- SendInput / CGEvent / 真实点击或滚轮
- 云 API、遥测、上传音视频
- Word / 仪器适配器
- WP1–WP5 手势识别与焦点逻辑

## Linux CI 系统依赖（参考）

```bash
sudo apt-get install -y \
  libwebkit2gtk-4.1-dev libayatana-appindicator3-dev \
  librsvg2-dev patchelf build-essential libssl-dev libgtk-3-dev
```
