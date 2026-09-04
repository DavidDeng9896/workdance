# workdance

依托普通笔记本 RGB 摄像头 + 麦克风，采用**手势空间操作 + 语音语义录入**的多模态融合交互范式。

- 锁定规格：[docs/specs/2026-09-04-workdance-locked.md](docs/specs/2026-09-04-workdance-locked.md)
- 当前实现：**WP0 壳** + **WP1 双档视觉** + **WP2 手势注入（G02–G05）**

## 架构

选用 **Tauri 2 + Rust backend**：检测 / 分类 / 注入均在 **Rust 线程**，不进 JS。

| Crate | 职责 |
| --- | --- |
| `workdance-core` | 配置、托盘态、DualTierMachine |
| `workdance-vision` | 摄像头、掌存在 detector、双档调度 |
| `workdance-input` | 手势分类、指数平滑、串行 InjectQueue；Win SendInput / Mac CGEvent / Linux null |
| `apps/desktop` | Tauri 壳；vision → tray；tier → input |

## WP0 / WP1（摘要）

```bash
cargo check --workspace
cargo test -p workdance-core
WORKDANCE_VISION_STUB=1 cargo test -p workdance-vision
cd apps/desktop && npm install && npm run build
```

`WORKDANCE_VISION_STUB=1`：无摄像头时脚本化进掌/离掌。详见上文历史提交；托盘休眠↔手势开由状态机驱动。

## WP2：光标 / 点击 / 滚轮 / 返回

### 手势

| ID | 动作 | 注入 |
| --- | --- | --- |
| G02 | 张掌平移 | `MoveAbs`（自拍镜像 X） |
| G03 | 短促握拳 &lt;300ms | `ClickLeft`；**&gt;300ms 不当点击** |
| G04 | 长握后握拳平移 | `Scroll` |
| G05 | 张掌下挥 | `KeyEscape`（浏览器返回/关弹窗的 best-effort） |

仅在 `VisionTier::Active` 时产生注入；**Sleep 档绝不移动光标**（开局无误触）。

平滑：指数平滑 + 配置死区（`dead_zone` / `sensitivity`）。串行队列：`workdance-inject` 单线程。

### Stub demo（Linux CI / 无 landmarks）

```bash
cargo test -p workdance-input
# 桌面：视觉 stub 唤醒后，手势线程用脚本化 HandSample 走 G02–G05
WORKDANCE_VISION_STUB=1 WORKDANCE_INPUT_STUB=1 cd apps/desktop && npm run tauri -- dev
```

Linux 默认 **null** 注入后端（接受命令、不碰系统指针）。CI 不需要摄像头或辅助功能权限。

### 真机 OS 注入

| 平台 | 后端 | 权限 |
| --- | --- | --- |
| Windows | `SendInput`（`cfg(windows)`） | 一般用户态即可 |
| macOS | `CGEvent`（`cfg(target_os = "macos")`） | 辅助功能 / 输入监控 |
| Linux | `NullInjector` | — |

```bash
# Win / Mac
cd apps/desktop && npm run tauri -- dev
# 需 Active 档 + 后续 landmark 源；无模型时可用 INPUT_STUB 脚本演示注入路径
```

真实手部 landmarks→`HandSample` 依赖 WP1 检测后端输出；当前 stub/CI 用脚本扩展。ORT/MediaPipe 全 landmarks 仍可选、非 WP2 阻断项。

### 测试

```bash
cargo test -p workdance-input   # 短拳点击 / 长握不点 / sleep 抑制 / 滚 vs 移 / 下挥 Esc
cargo check --workspace
```

## OUT OF SCOPE（截至 WP2）

- ASR / 录音 / 备忘录 MD — WP3–WP4
- 托盘「仅语音」产品化 — WP5
- 云 API、遥测、Word/仪器适配
- 精细框选 / 拖拽 / 空中键盘

## Linux CI 依赖

```bash
sudo apt-get install -y \
  libwebkit2gtk-4.1-dev libayatana-appindicator3-dev \
  librsvg2-dev patchelf build-essential libssl-dev libgtk-3-dev libv4l-dev
```
