---
title: AgentHub review 修复进展（2026-09-07）
type: status
status: current
owner: maintainers
updated: 2026-09-07
scope: dest @ 5ee06a4f；对照三份 2026-09-07 全项目 review 的确认问题
---

# AgentHub review 修复进展

本页是审查问题的修复跟踪，不是新的审查报告。问题证据仍以原报告为准：

- [c726bd2a 全项目 review](./2026-09-07-project-review-c726bd2a.md)
- [c8d792b5 增量 review](./2026-09-07-project-review-c8d792b5.md)
- [06b893c4 首轮，已被 c726bd2a 覆盖](./2026-09-07-project-review-06b893c4.md)

主 Agent 在 2026-09-07 于 HEAD `5ee06a4f` 回读源码后，确认下列问题仍可达。未把旧结论当证据。修复前已写好最小方案；实现 Agent 不得改方案范围外的文件。

## 批次

文件不重叠的任务才并行。日常合入仍走 `dev`/`dest`，每个批次一个 worktree 和分支。

| 批次 | 问题 | 分支 / worktree | 状态 |
| --- | --- | --- | --- |
| 1a | P1-004、P2-002 | `fix/review-p1-restore` | 核实完成，待实现 |
| 1b | P1-003 | `fix/review-p1-backup` | 核实完成，待实现 |
| 1c | P1-006、P1-007、P1-008 | `fix/review-p1-bridge-auth` | 核实完成，待实现 |
| 2a | P1-002 | `fix/review-p1-process-idle` | 核实完成，待实现 |
| 2b | P1-001 | `fix/review-p1-kiro-http-cancel` | 核实完成，待实现 |
| 2c | P1-005 | `fix/review-p1-sse-trailer` | 核实完成，待实现 |
| 3a | P1-009、P2-004、P2-005 | `fix/review-p1-chat-runtime` | 核实完成，待实现 |
| 3b | C8-P1-001/002/003、C8-P2-001/002 | `fix/review-open-chat` | 核实完成，待实现 |
| 4 | P2-003 | `fix/review-p2-setup-guide` | 核实完成，待实现 |
| 5 | P2-001 | 不在本轮并行 | 机械迁移，单独排队 |

## 确认问题与方案

### 批次 1a — 路由所属登录恢复

**AHREV-P1-004**（confirmed，当前 HEAD）

- 源码：`restore_connection_trash` 先 `restore_trash`（同一事务里插入源行并删除回收记录），再 `reattach_restored_pool_owned` → `attach_pool_owned_authorization` → `require_enabled()`。
- 路由关闭或 attach 失败后：回收记录已消失；源行带 `home=route_pool`，被 `ticket_read_service` 滤掉，也不在路由成员里；再次恢复因源行已存在失败。
- 方案：先恢复源行、**成功 reattach 后再删除回收记录**。reattach 失败则补偿删除刚插入的源行（若原本不存在），保留回收记录，让用户可重试。路由功能关闭时直接失败并保留回收记录，不要把登录留在不可见状态。
- 验证：关闭路由功能后恢复 `home=route_pool` 登录，断言回收记录仍在、Connections 无残留、可重试；注入 attach 失败同样可重试。

**AHREV-P2-002**（confirmed，同一文件）

- 源码：`recycle_route_membership` 先单独插入回收记录，再 `remove_route_authorization`。失败无整体补偿，重试可再插入记录。
- 方案：同一 Immediate 事务写入回收记录并删除成员；或使用稳定来源键避免重复插入。后续投影失败要能补偿。
- 验证：在删除成员/投影失败点注入，断言无重复回收记录、成员状态可重试。

允许改动：`crates/agenthub-core/src/lib.rs`、`services/connection_service/trash.rs`、`services/route_pool_service.rs` 及其 `tests.rs`。不要顺带重构。

### 批次 1b — 备份恢复误报成功

**AHREV-P1-003**（confirmed）

- 源码：`delete_failed` 进入 `RestoreResult.skipped_deletions`，该字段 `serde(skip)`。`BackupPort.restoreBackup` 返回 `void`，页面无条件成功提示。`edited`/`unknown` 是明确安全保留，不算本问题。
- 方案：公开 `skippedDeletions`（至少包含 `delete_failed`）。端口返回恢复结果。仅当存在 `delete_failed` 时页面用警告而不是成功；不要把整次恢复改成硬失败（快照文件已经写回）。
- 验证：注入只读/删除失败，断言 Tauri/页面能区分完整恢复与部分恢复。更新 mock 与 contract。

允许改动：backup_service、backup DTO/port/tauri/mock、`BackupsPanel` 与相关 i18n、对应测试。

### 批次 1c — 本机路由重放与登录刷新

**AHREV-P1-006**（confirmed）

- 源码：`post_upstream_attempt` 把 `builder.send()` 除 timeout/stopping 外的所有错误标成 `Unavailable`；failover 注释写「响应前失败可安全换成员」。header timeout 已禁止重放，因为请求可能已计费。连接建立后、响应头前断连与 timeout 同类，不能换成员。
- 方案：只有明确「连接尚未建立」的错误（如 `reqwest::Error::is_connect()`）才 `Unavailable`。其余 send 错误按不可重放失败返回，不换成员。
- 验证：本地双上游，A 接受 POST 后断连，断言 B 收不到同一请求。

**AHREV-P1-007**（confirmed）

- 源码：两个池可持有同一登录的独立 token cell。`reload_oauth_upstream_access` 在数据库未再变化时返回 `None`；回调把 `None` 当成失败。B 的 cell 仍是 T0，随后按指纹跨池隔离，把有效 T1 也踢掉。
- 方案：刷新后始终把当前可用 token 交给调用方，供过期 cell 追平。隔离前核对失败请求用的 token/revision 是否仍是当前版本；过期请求不得隔离已更新登录。
- 验证：双 cell 先后 401；第一个已转到 T1 后，第二个不得 isolate T1。

**AHREV-P1-008**（confirmed）

- 源码：Hub-owned 刷新走同步 `ureq`（最长约 30s），从 async `reload_member` 直接调用，没有 blocking 边界。
- 方案：同步刷新放入 `spawn_blocking`/`block_in_place`，保留 fingerprint singleflight。
- 验证：固定少量 worker，多指纹慢刷新时其他请求和停止仍推进。

允许改动：`bridge/host/upstream.rs`、`bridge/host/transport/failover.rs`、`bridge/auth_reload.rs`、`account_service/oauth_owner.rs`、`oauth/providers.rs`（仅当必须把 ureq 移出 async 路径）及对应测试。不要改 SSE 转换。

### 批次 2a — 截断后活动时钟

**AHREV-P1-002**（confirmed）

- 源码：`read_pipe_capped` 在 cap 后 `accepted == 0` 直接 continue，不再 `on_text`；主循环只在 `rx.try_recv()` 刷新 `last_activity`。
- 方案：成功读到字节就刷新独立活动时间，即使不纳入保存缓冲。保留 2 MiB 展示上限和绝对墙钟期限。
- 验证：小上限 + 短空闲，持续输出超过上限应完成；真正停止输出后才超时。

### 批次 2b — Kiro HTTP 取消

**AHREV-P1-001**（confirmed）

- 源码：`try_http_run_result(prompt, opts)` 不接收 `CancelToken`。`resolve_job` 在登记取消前同步跑完 HTTP。`run_each_parallel` 把 Early 成功直接当作完成。
- 方案：HTTP 执行接收取消与期限，本地等待可中断。取消后统一 cancelled 出口，保留 HTTP 会话标识。不要只在请求开始前检查一次。
- 验证：可控 transport 覆盖等待中取消、取消后迟到成功、短期限。

### 批次 2c — SSE 正常结束误报失败

**AHREV-P1-005**（confirmed）

- 源码：`stream.rs` 见到完成事件就 `break`，随后 `if !saw_done || !buffer.is_empty()` 记失败。`response.completed` 后的合法注释/空帧若与终止帧同一网络块，会留下 buffer。
- 方案：终止后按合法 SSE 语义忽略尾部空白/注释；无终止帧仍要失败。
- 验证：同块与跨块 fixture，覆盖 Messages 与 Chat。

### 批次 3a — Chat 启动停止与排序

**AHREV-P1-009**（confirmed）：catalog/`turn/start` 同步阻塞时，Cancel/Shutdown 进不了 actor；`shutdown` 忽略 5 秒超时仍删库。

**AHREV-P2-004**（confirmed）：`poll_events` 后续 63 次 `Duration::ZERO`，`recv_timeout` 在 `remaining.is_zero()` 时直接 `Ok(None)`，不能 `try_recv`。

**AHREV-P2-005**（confirmed）：`begin_turn` 只在标题为空时更新 `updated_at`。

方案：目录与 start 可取消；Shutdown 能抢占；未确认退出不删库。后续读取改为真正 `try_recv`。`begin_turn` 无条件更新 `updated_at`，标题仍只在空时赋值。

### 批次 3b — 系统文件夹打开 Chat

**AHREV-C8-P1-001**（confirmed）：事件处理写 pending 且不消费；HashRouter `useNavigate` 随 pathname 换身份，effect 重建后再 `takePending`。

**AHREV-C8-P1-002**（confirmed）：空列表 `ensureDefaultConversation` 与 shell bootstrap 并发建会话。

**AHREV-C8-P1-003**（confirmed）：菜单登记无条件 `current_exe()`，AppImage 临时挂载路径会失效。

**AHREV-C8-P2-001**（confirmed）：Windows Drive 菜单 `"C:\"` 被 `CommandLineToArgvW` 吃掉末引号。

**AHREV-C8-P2-002**（confirmed）：旧 bootstrap cleanup 在 `applied=false` 时写回，可覆盖第二次请求。

方案：事件只通知前端取 pending，消费点唯一。shell bootstrap 完成前不要自动建默认会话。AppImage 用经校验的 `APPIMAGE`。根目录避免反斜杠紧邻闭合引号。写回前比较 payload/代次。

### 批次 4 — 安装页失败提示

**AHREV-P2-003**（confirmed）：`open_in_browser` 失败只写日志；`setup_guide_open_failed` 只看 `spawn_error`/`timed_out`；用户仍看到「已打开官网安装页」。

方案：打开失败记入 `ExecResult`，走失败提示，不要说已经打开。

### 批次 5 — 测试文件分离

**AHREV-P2-001**：53 个生产 Rust 文件内嵌测试。机械迁移，禁止顺带重构。本轮功能修复完成后再分批做。

## 关闭规则

一个问题只有在同时满足时才标为已关闭：

1. 当前 dest 含修复提交；
2. 约定测试通过；
3. 主 Agent 回读源码确认原触发路径不再成立。

关闭后在本页和原 review 对应条目写提交哈希，不删除原证据。
