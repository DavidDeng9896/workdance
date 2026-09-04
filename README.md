# workdance

依托普通笔记本 RGB 摄像头 + 麦克风，采用**手势空间操作 + 语音语义录入**的多模态融合交互范式。

- 锁定规格：[docs/specs/2026-09-04-workdance-locked.md](docs/specs/2026-09-04-workdance-locked.md)
- 当前实现：**WP0 壳** + **WP1 双档视觉** + **WP2 手势注入（G02–G05）** + **WP3 焦点锁 + G07 离线听写**

## 架构

选用 **Tauri 2 + Rust backend**：检测 / 分类 / ASR / 注入均在 **Rust 线程**，不进 JS。

| Crate | 职责 |
| --- | --- |
| `workdance-core` | 配置、托盘态、DualTierMachine |
| `workdance-vision` | 摄像头、掌存在 detector、双档调度 |
| `workdance-input` | 手势分类、焦点 dwell、G07、离线 ASR trait、串行 InjectQueue |
| `apps/desktop` | Tauri 壳；vision → tray；G07 → Recording 托盘态 |

## WP0 / WP1（摘要）

```bash
cargo check --workspace
cargo test -p workdance-core
WORKDANCE_VISION_STUB=1 cargo test -p workdance-vision
cd apps/desktop && npm install && npm run build
```

`WORKDANCE_VISION_STUB=1`：无摄像头时脚本化进掌/离掌。托盘休眠↔手势开由状态机驱动。

## WP2：光标 / 点击 / 滚轮 / 返回

| ID | 动作 | 注入 |
| --- | --- | --- |
| G02 | 张掌平移 | `MoveAbs`（自拍镜像 X） |
| G03 | 短促握拳 &lt;300ms | `ClickLeft`；**&gt;300ms 不当点击** |
| G04 | 长握后握拳平移 | `Scroll`（G07 录音中抑制） |
| G05 | 张掌下挥 | `KeyEscape` |

仅在 `VisionTier::Active` 时产生注入；**Sleep 档绝不移动光标**。

## WP3：焦点锁 + G07 离线听写

### 焦点

- 悬停输入框/控件 **0.4s** → 锁焦点（`FocusLock` + `FocusProbe`）
- 握拳可 **reconfirm** 待定目标
- **手势移动不解除**已锁焦点、不中断录音
- Linux：`StubFocusProbe`；Win/Mac：UIA/AX **cfg 占位**（当前回落 stub）

### G07

| 条件 | 行为 |
| --- | --- |
| 握拳 &lt;1s | 不进入录音（短拳仍走 G03） |
| 握拳 ≥1s | 开始录音（最长 60s）；托盘 → **录音** |
| 手离镜 | **立刻中止**，不跑 ASR |
| 松拳 | 内存缓冲 → 离线 ASR → `AppendText` **追加**写入当前焦点 |
| 结束后 | 托盘回到视觉档对应的 手势开 / 休眠 |

### ASR

- Trait `AsrBackend`；默认 `StubAsr` 返回固定中文「实验记录已追加。」（CI / 无模型）
- Feature `whisper`：可选本地模型路径（**模型不入库**）
- 下载：`scripts/download-whisper-tiny.sh`
- **不上传音频**；缓冲仅内存，测试断言不落盘

### 文本注入

- `InjectCommand::AppendText` 进入现有串行 `InjectQueue`
- Win/Mac：Unicode 键盘注入（辅助功能 SetValue 不可用时的回落）
- Linux：`NullInjector` 接受并丢弃（CI 无副作用）

### Stub demo（无 mic / 无模型）

```bash
cargo test -p workdance-input
WORKDANCE_VISION_STUB=1 WORKDANCE_INPUT_STUB=1 cd apps/desktop && npm run tauri -- dev
# stub 编排含 ≥1s 握拳 → Recording 托盘 → 松拳 AppendText（stub 中文）
```

```bash
# 可选本地 whisper 模型（不提交）
./scripts/download-whisper-tiny.sh
cargo check -p workdance-input --features whisper
```

### 测试

```bash
cargo test -p workdance-input
# dwell 0.4s 锁焦点 / G07 <1s 不录 / ≥1s 开录 / 离镜中止 / 松拳 AppendText / 无音频落盘
cargo check --workspace
```

## OUT OF SCOPE（截至 WP3）

- G08 备忘录 MD 落盘（仅后续 WP4；本切片无 memo 文件保存）
- 托盘「仅语音」产品化 / 设置页完整打磨 — WP5
- 云 API、遥测、Word/仪器适配
- 精细框选 / 拖拽 / 空中键盘
- 将 MediaPipe / whisper 权重提交进仓

## Linux CI 依赖

```bash
sudo apt-get install -y \
  libwebkit2gtk-4.1-dev libayatana-appindicator3-dev \
  librsvg2-dev patchelf build-essential libssl-dev libgtk-3-dev libv4l-dev
```
