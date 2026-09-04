# workdance

依托普通笔记本 RGB 摄像头 + 麦克风，采用**手势空间操作 + 语音语义录入**的多模态融合交互范式。

- 锁定规格：[docs/specs/2026-09-04-workdance-locked.md](docs/specs/2026-09-04-workdance-locked.md)
- 当前实现：**WP0–WP4**（壳 / 双档视觉 / G02–G05 / 焦点+G07 / G08 备忘）

## 架构

选用 **Tauri 2 + Rust backend**：检测 / 分类 / ASR / 备忘 / 注入均在 **Rust 线程**，不进 JS。

| Crate | 职责 |
| --- | --- |
| `workdance-core` | 配置、托盘态、DualTierMachine、**MD memo 写入/搜索** |
| `workdance-vision` | 摄像头、掌存在 detector、双档调度 |
| `workdance-input` | 手势分类、焦点、G07 ASR、**G08 双短拳**、InjectQueue |
| `apps/desktop` | Tauri 壳；G07→Recording；G08→写 MD；`search_notes` 命令 |

## WP0–WP3（摘要）

```bash
cargo check --workspace
cargo test -p workdance-core
WORKDANCE_VISION_STUB=1 cargo test -p workdance-vision
cargo test -p workdance-input
```

详见历史：双档视觉、G02–G05 注入、0.4s 焦点锁、G07 ≥1s 录音 + stub 离线 ASR 追加。

## WP4：G08 时间戳 Markdown 备忘

### 手势启发式（G08）

**双短拳（second short-fist）**：两次握拳均 **&lt;300ms**，且第二次松拳落在第一次松拳后的 **500ms** 窗口内（`G08_WINDOW_MS`）。

| 对比 | 区别 |
| --- | --- |
| G03 | 单次短拳 → 单击；第一次短拳仍单击并武装 G08 窗口 |
| G05 | 张掌**下挥** → Esc |
| G07 | 握拳 **≥1s** → 录音 |
| G08 | 第二次短拳 → 若正在录音则先停录；写入 MD 备忘（**不存音频**） |

正文来自最近一次 G07 `DictationReady` 听写（或强制停录得到的 stub/ASR 文本）；无听写时写 `(无听写内容)`。

### 备忘文件

- 目录：设置里的 `notes_path`（启动 / 保存设置时 `ensure_notes_dir`，缺失则创建）
- 文件名：`YYYY-MM-DD-HHMMSS.md`
- 内容：YAML frontmatter（title/created/kind）+ 标题 + 听写正文
- **从不**写入 wav/mp3 等音频

### 搜索

- Rust：`workdance_core::search_memos(notes_path, query)`
- Tauri：`search_notes` / `ensure_notes_directory`
- 设置页「备忘搜索」调用上述命令（进程内子串过滤）

### Demo

```bash
cargo test -p workdance-core   # memo 写入 / 搜索 / 建目录 / 无音频
cargo test -p workdance-input  # G08 双短拳 → SaveRequested
# 桌面 stub：G07 后接双短拳 → 日志打印 memo 路径
WORKDANCE_VISION_STUB=1 WORKDANCE_INPUT_STUB=1 cd apps/desktop && npm run tauri -- dev
```

```bash
# 纯库演示写备忘
cargo test -p workdance-core write_memo_creates_timestamped_md -- --nocapture
```

## OUT OF SCOPE（截至 WP4）

- 云同步、完整 ELN
- 托盘「仅语音」产品化 / 设置页全面打磨 — WP5
- 云 API、遥测、Word/仪器适配
- 精细框选 / 拖拽 / 空中键盘
- MediaPipe / whisper 权重入库

## Linux CI 依赖

```bash
sudo apt-get install -y \
  libwebkit2gtk-4.1-dev libayatana-appindicator3-dev \
  librsvg2-dev patchelf build-essential libssl-dev libgtk-3-dev libv4l-dev
```
