---
title: 本机路由真机验收
description: 在桌面应用和真实登录上验收本机转发发布前的七类风险。
type: how-to
status: current
owner: maintainers
updated: 2026-09-29
---

# 本机路由真机验收（dogfood）

本文是本机转发发布前的真机清单。自动化测试只覆盖能机器化的部分，不能代替桌面应用、真实登录和真实客户端；七类验收全部完成前，实验性路由不算发布完成。

## 范围和入口

用 `pnpm tauri:dev` 和真实登录验证。接到某个 Agent 的入口是总览页点 Agent 卡片打开的连接弹窗；把连接页的登录加入连接池，用路由 → 连接池页的「从连接同步」。路由页只管已接好的本机转发的运行。

典型 smoke flow：

1. Kimi Code 会员 API Key → Claude：直接改配置，不启动本机转发。
2. Kimi Code 会员 API Key → Codex：`local_bridge`，验证 loopback、端口和长流；Codex 配置应为 `wire_api = "responses"`、`preferred_auth_method = "apikey"`，`auth.json` 的 `OPENAI_API_KEY` 为入口 Key。
3. Anthropic API Key → Pi：写入 Pi 认的登录位置，不启动本机转发。
4. 任意已支持来源 → Grok 本机路由：`config.toml` 为 `api_backend = "responses"`，`api_key` 为入口 Key，客户端请求 `POST /v1/responses`。

本清单只记结构化证据，不记真实密钥。Kimi 会员 OAuth 不在本清单，产品不做 OAuth 反代、写入其他 Agent 或 OAuth 转 API。

## 前置条件

1. 跑自动测试（等价于 core 的 `bridge` 过滤加 GUI 的 `adapter_bridge_controller` 过滤）：

   ```text
   pnpm test:bridge
   ```

2. 启动桌面应用：`pnpm tauri:dev`。
3. 准备一份可撤销的真实登录和一个可释放的本机端口，按下表记录：

| 项 | 记录 |
|---|---|
| 日期 / 操作者 | |
| OS / 构建提交 | |
| AgentHub 数据目录 | 只记路径形态，不贴真实用户名；推荐 `AGENTHUB_HOME` 占位 |
| 来源登录 | 只记产品和供应商 id 后缀 |
| `profile_id` | |
| 首选端口 / 实际端口 | |
| `auto_start` | 默认 true；第 7 项确认自动恢复；如本次测试前已关闭，先恢复为 true |

## 证据规则

每项记录：环境、动作、结果（通过/失败/跳过）、`profile_id`、`request_id`、`code`、`elapsed_ms`、`op` 和问题。禁止记录 URL query、Authorization、API key、OAuth token、prompt、工具参数、响应正文或完整配置。

## 七类真机验收

### 1. 密钥轮转

1. 创建并启动一条 Kimi 会员 Key → Codex 的本机转发，记下 `profile_id` 和端口。
2. 在连接页更换该来源的 API Key，新旧 Key 都不写进记录。
3. 对运行中的路由发一次最小 Codex 请求，或停止后再启动。
4. 核对 listener 随来源 Key 变化正确替换，入口 Key 不变。

通过标准：上游请求用新 Key，入口 Key 未变，DTO 和日志里没有 bearer 或上游密钥。

### 2. 端口冲突与重绑定

1. 用 `TcpListener` 或其他进程占用 preferred port。
2. 启动或重启该路由；也可在 `auto_start=true` 时重启 AgentHub。
3. 在路由页、连接页或总览同时核对实际端口和目标 Agent 的 `base_url`。

通过标准：自动换到新端口，profile 与生成的供应商同步，旧端口不再留在本机正在用的配置里，旧 listener 不被引用。

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

通过标准：401、429、5xx 和取消都不回传密钥或上游正文；取消会终止上游请求；路由可标为 degraded，恢复后继续服务。

### 6. 托盘退出 drain

1. 保持至少一条本机转发在运行。
2. 从托盘退出，确认弹出「隐藏到托盘 / 停止服务并退出 / 取消」。
3. 选隐藏后确认端口仍可请求；选停止服务并退出后确认端口已释放。

通过标准：有路由在运行时必须确认；退出由 `ExitCoordinator` 排空并幂等停止 listener；日志有 `op=exit` 和活动路由数量。没有路由时的日志不能当通过证据。

### 7. 自动恢复和失败回滚

1. 确认该路由的 `auto_start` 为 `true`（默认值），退出再启动 AgentHub。
2. 核对只恢复 `Active + auto_start + local_bridge`。
3. 让 preferred port 被占用，验证恢复时重绑定并写回新端口。
4. 人为触发恢复失败，检查旧 `local_port`/provider `base_url` 保持一致且新 listener 已停止。

通过标准：失败标为 retryable，不影响其他 profile；`NeedsAttention` 不会被成功的 restore 误清；回滚后本机正在用的配置不指向失效 listener。

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

七项全部通过，且没有未关闭的密钥泄露、错误路由、残留 listener、重复执行或回滚问题，才算真机验收完成。自动测试通过**不能**单独作为放行依据。失败要保留结构化证据，回到对应规则的限制或开关处理，不得用删日志、放宽断言或改用 mock 来「通过」。

