# WP-A1：sherpa-onnx 集成边界

- 日期：2026-09-04
- 状态：边界定稿（文档-only；本 PR **不改** Rust / 应用代码）
- 仓库：https://github.com/DavidDeng9896/workdance
- 依据：[锁定规格](./2026-09-04-workdance-locked.md)；[MediaPipe + ASR 计划](./2026-09-04-mediapipe-asr-plan.md) §4 / §7 **WP-A1**；现有 `AsrBackend` / `create_default_asr` / G07 契约

## 0. 一句话

用 **sherpa-onnx Paraformer-zh-small** 实现真实离线中文听写，**只替换** `AsrBackend` 实现；DualTier、G07/G08、landmarks 契约不动。Feature 默认关，CI 无 feature 仍绿。

## 1. 硬边界（本切片可改 / 不可改）

| 区域 | WP-A1 |
| --- | --- |
| `AsrBackend` / `StubAsr` / `create_default_asr` | **可改**：新增 `SherpaAsr`（或等价），接好默认工厂 |
| Cargo feature `sherpa-asr` | **可加**；**默认 OFF** |
| 模型下载脚本（`scripts/download-*.sh`） | **可加**；须钉 URL + **SHA256** |
| `DualTierMachine` / 双档 FPS / 掌阈值 | **禁止改** |
| G07 / G08 状态机契约与事件语义 | **禁止改**（仍整段离线转写） |
| landmarks / G02–G05 映射 | **禁止改** |
| 托盘状态机 / InjectQueue 串行抽象 | **禁止改** |
| VAD / 流式切段 | **不做**（属 **A1.1**，非 A1） |

## 2. Feature 与默认工厂

### 2.1 Feature

- 名称：`sherpa-asr`
- 默认：**OFF**
- CI / 默认 `cargo test` / `cargo check --workspace`：**不**启用该 feature，须继续绿

### 2.2 `create_default_asr` 选择规则

优先级（实现时须遵守）：

1. **`sherpa-asr` feature 已开** 且 模型目录可用 → `SherpaAsr`（真转写）
2. 否则，仅当 **测试** 或显式 `WORKDANCE_ASR_STUB=1` → `StubAsr`
3. **生产路径**（feature 开但缺模型，或未开 feature 的发布配置）：**不得**静默用 Stub 把固定句写入用户焦点

生产听写**禁止**向用户文档注入固定短语 `实验记录已追加。`（该句仅允许 Stub / CI）。缺模型时显式失败或禁用听写（UI 细节可落 WP-A2；A1 至少不得假装成功）。

## 3. 默认模型钉死

| 项 | 值 |
| --- | --- |
| 发布包名 | `sherpa-onnx-paraformer-zh-small-2024-03-09.tar.bz2` |
| 来源 | [k2-fsa/sherpa-onnx `asr-models` release](https://github.com/k2-fsa/sherpa-onnx/releases/tag/asr-models) |
| URL | `https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/sherpa-onnx-paraformer-zh-small-2024-03-09.tar.bz2` |
| 体积 | ≈78 MB（归档约 74.3 MiB / 77 920 048 bytes） |
| 归档 SHA256 | `da92b3db5218c5be53aad53e57d1b6e63e7fc98a0e054fbdd6dbe18e9c6b1450` |
| 安装目录 | `{dirs::data_local_dir()}/workdance/models/asr/paraformer-zh-small/` |
| 覆盖变量 | `WORKDANCE_ASR_MODEL_DIR`（指向已解压模型根目录，内含 `model.int8.onnx`、`tokens.txt` 等） |

解压后期望文件（以官方包为准）：至少 `model.int8.onnx`、`tokens.txt`（及包内附带配置）。**模型不进 git**。

### 3.1 下载脚本要求

- 新增脚本（建议名：`scripts/download-paraformer-zh-small.sh`）
- 必须：**钉死 URL + SHA256**；校验失败非零退出
- 解压到上述默认目录；支持 `WORKDANCE_ASR_MODEL_DIR` 覆盖目标
- CI **不**下载该模型

## 4. PCM 契约

| 项 | 定案 |
| --- | --- |
| 采样 | **16 kHz**、**mono**、**i16 little-endian** |
| 上限 | **最长 60 s**（与 G07 `G07_MAX_MS` 对齐） |
| 落盘 | **永不**写音频到磁盘（无 wav / 无临时录音文件） |
| 上传 | **永不**上传 |
| 输出 | 仅内存 → `AsrResult { text, audio_discarded: true }` |

G07：**整段离线**（松拳或满 60 s 后一次 `transcribe_zh`）。流式 / VAD 切段 → **A1.1**，不在 A1 范围。

## 5. 与现有代码的接缝

- 入口：`workdance-input` 的 `AsrBackend::transcribe_zh(&[u8])`
- 调用方：G07 松拳 / 超时；WP5 仅语音听写松臂 — **不改**其状态机，只换后端实现
- 可选保留 `whisper` feature 作备份（计划 WP-A2）；A1 不要求删除 whisper 占位

## 6. 验收（WP-A1）

- [ ] **离线**：无网络亦可完成转写（模型已在本地）
- [ ] **无 wav**：转写路径不写音频文件
- [ ] **非 stub 文本**：启用 `sherpa-asr` + 模型时，输出为模型转写结果，**不是** `实验记录已追加。`
- [ ] **CI**：未开 `sherpa-asr` 时 `cargo check` / `cargo test` 仍绿（stub 路径不变）

## 7. OUT OF SCOPE（本边界 / 本 PR）

- 实现 Rust `SherpaAsr` 或改任何应用代码（**本 PR 文档-only**）
- VAD / 流式 ASR（**A1.1**）
- 缺模型 UI 打磨与 whisper 备份产品化（**WP-A2**）
- 改 DualTier、G07/G08、landmarks、注入队列
- 术语热词、云端 ASR、模型 blob 进仓

## 8. 建议实现顺序（后续 PR，非本 PR）

1. 下载脚本（URL + SHA256 钉死）→ 本地模型目录
2. `sherpa-asr` feature + `SherpaAsr: AsrBackend`（16 kHz mono i16 LE，整段）
3. 调整 `create_default_asr`（§2.2）；生产禁固定 stub 文案
4. 带 feature 的集成/手工验收；默认 CI 无 feature 回归

---

本文件为 **WP-A1 边界规格**。实现落在后续代码 PR；本 PR 仅文档与 README 链接。
