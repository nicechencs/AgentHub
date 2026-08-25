---
title: Route Adapter 真机 Dogfood
description: 在桌面应用和真实连接上验收本机 Routes 的发布前七类风险。
type: how-to
status: current
owner: maintainers
updated: 2026-08-25
---

# Route Adapter 真机 Dogfood

本文是本机 Routes 发布前的真机清单。自动化测试只覆盖可机器化子集，不能代替桌面应用、真实连接和真实客户端的验证；七类验收全部完成前，不得把实验性 Route 当作发布完成。

## 范围和入口

使用 `pnpm tauri:dev` 和真实连接验证。产品入口是 Dashboard 的连接/切换和 Connections 的分享/路由；本机 Routes 页面（中文“路由”，英文 `Routes`）只管理已绑定 Route 的运行时。不要把内部 `bridge` 名称写成用户功能名。

典型 smoke flow：

1. Kimi Code 会员 API Key → Claude：直接改配置，不启动本机 Route。
2. Kimi Code 会员 API Key → Codex：`local_bridge`，验证 loopback、端口和长流。
3. Anthropic API Key → Pi：写入 Pi 认的登录位置，不启动本机 Route。

本清单只记录结构化证据，不记录真实秘密。Kimi 会员 OAuth 不在本清单，产品不做 OAuth 反代、写入其他 Agent 或 OAuth 转 API。

## 前置条件

```text
pnpm tauri:dev
cargo test -p agenthub-core --locked bridge
cargo test -p agenthub-gui --locked adapter_bridge_controller
```

自动测试通过后，使用一份可撤销的真实连接和一个可被释放的本机端口。记录模板：

| 项 | 记录 |
|---|---|
| 日期 / 操作者 | |
| OS / 构建提交 | |
| AgentHub 数据目录 | 只记路径形态，不贴真实用户名；推荐 `AGENTHUB_HOME` 占位 |
| 来源连接 | 只记产品和 provider id 后缀 |
| `profile_id` | |
| 首选端口 / 实际端口 | |
| `auto_start` | 默认 false；第 7 项再打开 |

## 证据规则

每项记录：环境、动作、结果（通过/失败/跳过）、`profile_id`、`request_id`、`code`、`elapsed_ms`、`op` 和问题。禁止记录 URL query、Authorization、API key、OAuth token、prompt、工具参数、响应正文或完整配置。

## 七类真机验收

### 1. 密钥轮转

1. 创建并启动一个 Kimi membership → Codex Route，记录 `profile_id` 和端口。
2. 在 Connections 轮转该来源的 API key，不把新旧 key 写入记录。
3. 对运行中的 Route 发起一次最小 Codex 请求，或停止后再启动。
4. 核对 listener 因来源凭据变化正确替换；local bearer 保持不变。

通过标准：上游请求使用新凭据、loopback token 未变化、DTO/日志不含 bearer 或上游 secret。

### 2. 端口冲突与重绑定

1. 用 `TcpListener` 或其他进程占用 preferred port。
2. 启动或重启 Route；也可在 `auto_start=true` 时重启 AgentHub。
3. 在 Routes、Connections 或 Dashboard 同时核对实际端口和目标 Agent 的 `base_url`。

通过标准：自动选择新端口，profile 与生成的 provider 同步，旧端口不再留在 live 配置，旧 listener 不被错误引用。

### 3. 长时间 SSE

1. 通过 Codex 发送持续数分钟的流式请求，不记录 prompt 或正文。
2. 观察正常结束、取消和空闲超时路径。
3. 核对日志只有关联 id、状态、错误码和耗时等安全字段。

通过标准：流能完成或给出通用错误；分片边界和 Unicode 不损坏；无 payload 泄漏；idle timeout 后 listener 仍能接受下一次请求。

### 4. 文本及工具调用闭环

1. 完成一次短文本流。
2. 完成一次“模型调用工具 → 客户端回填结果 → 模型终答”的闭环。
3. 只记录是否完成、结构化的 `call_id`/名称和耗时，不记录参数值或结果正文。

通过标准：Responses 事件和 SSE 顺序完整；工具只执行一次；未支持的 thinking/signature 字段明确降级或失败，不伪造签名块。

### 5. 上游失败与中途取消

1. 临时使上游认证失败或断开连接，确认客户端得到通用错误。
2. 取消一个进行中的流式请求。
3. 可选地触发 429/5xx，核对本机状态和日志，不复制上游原文。

通过标准：401、429、5xx、取消都不回传秘密或上游正文；取消会终止上游请求；Route 可标为 degraded 并在恢复后继续服务。

### 6. 托盘退出 drain

1. 保持至少一个 Route 运行。
2. 发起托盘退出，确认出现“隐藏到托盘 / 停止 Routes 并退出 / 取消”等确认路径。
3. 选择隐藏后确认端口仍可请求；选择停止并退出后确认端口释放。

通过标准：有 Route 时必须确认；退出由 `ExitCoordinator` 排空并幂等停止 listener；日志记录 `op=exit` 和活动 Route 数量，但不把没有 Route 的日志当成通过证据。

### 7. 自动恢复和失败回滚

1. 打开该 Route 的 `auto_start`，退出再启动 AgentHub。
2. 核对只恢复 `Active + auto_start + local_bridge`。
3. 让 preferred port 被占用，验证恢复时重绑定并写回新端口。
4. 人为触发恢复失败，检查旧 `local_port`/provider `base_url` 保持一致且新 listener 已停止。

通过标准：失败标记为 retryable，不污染其他 profile；`NeedsAttention` 不被成功 restore 错误清掉；回滚后不会留下指向失效 listener 的 live 配置。

## 自动化对照

这些测试只作为真机前置，不是发布放行：

| 类别 | 自动化覆盖 | 过滤入口 | 仍需真机 |
|---|---|---|---|
| 密钥轮转 | listener 替换、local bearer 不变、restore 读取新 key | `ensure_listener_replaces_upstream_auth_while_keeping_local_bearer`、`restore_uses_a_rotated_source_key_without_changing_the_local_bearer` | 真实上游接受新 key、长连接行为 |
| 端口冲突 | preferred 占用后的 rebind、projection realign、失败恢复 | `ensure_listener_rebinds_when_preferred_port_is_busy`、`busy_preferred_port_rebind_then_realign_updates_projection`、`realign_restored_bridge_port_*` | 真实 Codex 读取新 `base_url` |
| 长 SSE | mock 分片和空闲超时 | `cargo test -p agenthub-core --locked bridge` | 真实模型数分钟流 |
| 文本/工具 | Responses↔Chat 协议 fixtures | `cargo test -p agenthub-core --locked bridge::protocol` | 真实 Codex 工具执行闭环 |
| 失败/取消 | health/auth、脱敏 debug、degraded 状态 | `bound_health_rejects_upstream_auth_before_a_provider_switch` 及 bridge host tests | 真实 401/429/5xx 和客户端取消 |
| 退出 drain | `ExitCoordinator` 和幂等 stop | `exit_coordinator`、`stop_is_idempotent_*` | 真实托盘三选一 UI |
| 自动恢复/回滚 | restore filter、retryable、port realign | `restore_filter_*`、`retryable_restore_*`、`realign_restored_bridge_port_*` | 冷启动 GUI 和真实端口竞争 |

## 发布门槛

七项全部勾选，且没有未关闭的密钥泄露、错误路由、残留 listener、双执行或回滚问题，才算 Phase 1 真机 dogfood 完成。工作区自动测试通过**不能**单独作为放行依据。所有失败应保留结构化证据并回到对应 rule 的限制或 gate，不得用删日志、放宽断言或改成 mock 来“通过”。

