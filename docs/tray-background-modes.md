# 托盘后台模式（低内存后台 vs 隐藏界面）

> **状态：未来优化点，未实施。** 本页只记录设计结论与实施要点，不派生当前任务。
> 实现前不得把本页能力表抄进 CLI 矩阵或 `Backend.features` 快照。

相关真源：

| 主题 | 真源 |
|---|---|
| 实现状态 / 未实现清单 | [agenthub-plan.md §8](agenthub-plan.md#8-当前实现状态以代码与测试为准) |
| 目录与分层 | [architecture.md](architecture.md) |
| 退出协调 / bridge drain | `src-tauri/src/exit_coordinator.rs`（代码即真源） |
| 关闭行为决策 | `src-tauri/src/window_policy.rs`（代码即真源） |

---

## 1. 背景与结论

当前「隐藏到托盘」实现（`tray.rs` 的 `apply_exit_impact_choice` 与 `lib.rs` 的 `CloseRequested` 分支）调用的是 **`window.hide()`**：

- WebView2 进程继续存活，DOM、JS 堆、React 组件树全部驻留；
- 前端定时器照跑（`main.tsx` 全局 tick、dashboard bridge 轮询、agent card lifecycle、usage sync），后台持续 IPC 与重渲染；
- Rust 侧 hub / bridge host 本就常驻，是托盘存在的意义，不属于可压缩范围。

**结论：hide 后进程内存基本不降。** WebView2 + JS 堆通常占总内存 60–80%，要降内存必须动 WebView2 本身。产品上做成设置项，让用户在「隐藏界面」（现状）与「低内存后台」之间选择。

## 2. 设置项：三档设计

| 档位 | 行为 | 内存效果 | 代价 |
|---|---|---|---|
| 标准后台（默认，现状） | `window.hide()`，前端照常 | 无变化 | 无 |
| 省电后台 | hide + 前端暂停轮询/定时器（可选叠加 WebView2 `TrySuspend`，Windows 专属） | 小幅下降 + 后台 CPU 归零 | 唤醒后需刷新一次数据 |
| 深度低内存 | 销毁 webview/窗口，只留 Rust 进程 + 托盘；点亮时重建 | 最大（WebView2 完全退出） | 冷启动延迟、UI 状态丢失、重建时序复杂 |

若不想暴露三档，可简化为二元选择：「隐藏界面」（现状）/「低内存后台」（= 深度档），省电档作为隐藏分支的内部优化顺带做。

## 3. 实施要点

### 3.1 设置键

- 新增 `background_mode` 设置键，走现有 `set_setting` 流程；值归一化参考 `window_policy.rs::is_close_to_tray_enabled` 的写法（未知值回退默认档）。
- 决策逻辑写成纯函数（如 `decide_background_action(...)`），单测与生产分文件（Rust 放 `*/tests.rs`，前端并列 `*.test.ts`）。

### 3.2 Rust 分流点

- 现有两处 hide 调用（`lib.rs` `CloseRequested`、`tray.rs` `apply_exit_impact_choice`）收敛到一个公共函数 `hide_main_window(app)`，内部读设置决定 hide 还是 destroy。
- destroy 只销毁窗口/webview；`AppState` / hub / bridge host 在 Rust 侧，天然不受影响，且不走 ExitCoordinator 的退出路径。

### 3.3 重建时序（深度档的主要工作量）

- 托盘点亮 → 先 `WebviewWindowBuilder::create()`，等 webview ready 再 emit 导航事件。现有 `TRAY_NAVIGATE_EVENT` 在重建场景会丢消息，需要 pending-navigation 缓存：webview 未就绪先存路径，ready 后补发。
- 前端启动时恢复上次路由（localStorage/sessionStorage 存最后路径即可）。
- Windows 上避免在事件回调里同步 destroy，用 defer/spawn。

### 3.4 前端配合（省电档 + 深度档通用）

- Rust 在 hide 时 emit `app:hidden` / 显示时 emit `app:visible`；前端据此暂停全局 tick、bridge 轮询、usage sync 等，visible 时强制刷新一次。
- 页面层不直接 invoke，事件订阅经 `lib/api/` 或 backend 层封装。

### 3.5 边界情况

- `request_app_restart`（更新重启）：窗口已销毁时 relaunch 是进程级操作，理论无影响，需实测。
- `decide_close_action`（`close_to_tray || bridge_active` 驻留判断）与本设置正交：先决定是否驻留，再决定驻留方式（hide / destroy）。
- WebView2 `TrySuspend` 有弹窗/媒体时失败，需要降级为普通 hide。

## 4. 建议拆分

两个独立 PR：

1. **设置项 + 省电档**（低风险，前端为主 + 一个 Rust emit）；
2. **深度低内存模式**（重建时序、pending navigation、状态恢复，回归风险集中区）。
