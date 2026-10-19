# WorkDance

单机手势 + 语音输入。桌面宿主是 vendored 的 [OpenLess](https://github.com/Open-Less/openless) `beta`（`vendor/openless`），不是旁边再挂一个进程。

手势逻辑在 `crates/workdance-gesture`（分类 + 时间门控），不放进 `openless-core`。

- 怎么跑、门控字段、接上的 OpenLess 符号：[SELFTEST.md](SELFTEST.md)
- 上游许可：AGPL-3.0-only，见 [NOTICE](NOTICE) 与 `vendor/openless/UPSTREAM_LICENSE`

## 这一刀做了什么

- 张掌（五指张开）在画面里就跟踪，光标跟着掌心，不要求停住唤醒。
- 短握拳再松开：左键单击。
- 长握拳（暂定 ≥ 800 ms）：走 OpenLess `DictationHotkeyEdge` 开始听写；保持握拳说话；松拳或手离开画面则停止并插入文本。
- 听写进行中不移动、不点击、不滚动。
- 没有手：手势不产生光标、点击或听写。
- 键盘热键听写仍是 OpenLess 原路径。ASR 仍由用户在 OpenLess 里选本地或云端/API。
- 托盘四态：安静 / 跟踪 / 听写 / 误触暂停（胶囊在空闲时也听 `workdance:tray`）。
- 指针注入和 `TextInserter` 共用一把串行锁。

掌心甩动滚动没有做。预留方向是甲：向上甩则向上滚；前置摄像头只镜像 X。握拳时长是暂定值（400 / 800 / 暂停 350 ms），`thresholdsProvisional = true`。

## 调试命令

`workdance_gesture_set_enabled`、`workdance_gesture_gate_state`、`workdance_gesture_last_decision`、`workdance_gesture_event_counts`。字段说明在 SELFTEST。

## 构建

Linux CI 能测门控和 `openless-core`。OpenLess 的 Tauri 壳在 Linux 上会故意编译失败，桌面壳在 Windows / macOS 上检查：

```bash
cargo test -p workdance-gesture
cargo check -p openless-core --manifest-path vendor/openless/Cargo.toml
cargo check --manifest-path vendor/openless/src-tauri/Cargo.toml   # Win / macOS
```

## 旧壳

`crates/workdance-core`、`workdance-vision`、`workdance-input` 和 `apps/desktop` 是上一版 DualTier 壳，只留作对照（Hands FFI、镜像、注入队列）。不要把它当发行宿主。
