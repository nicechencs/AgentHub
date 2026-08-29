---
title: 添加 Route Adapter
description: 为一个登录来源和目标 Agent 增加可验证的协议适配或本机 Route。
type: guide
audience: contributor
status: current
updated: 2026-08-29
---

# 添加 Route Adapter

这里的 adapter 指“登录来源到目标 Agent 的连接方式”，不是新增 Agent 的 `AgentAdapter`。用户看到的是三种做法：直接改配置、写进对方认的登录、或使用本机 Route；代码中的路线枚举是 `native_endpoint`、`config_sync`、`local_bridge`。

## 1. 先分类

| 路线 | 何时使用 | 结果 |
|---|---|---|
| `native_endpoint` | 目标 Agent 能直接读取来源协议/端点 | 写入目标原生配置，不启动本机 listener |
| `config_sync` | 目标 Agent 能识别该登录，但需要字段投影或配置合并 | 通过目标 Agent 的配置/登录契约写入 |
| `local_bridge` | 只能由本机协议转换连接 | 启动绑定到 `127.0.0.1` 的 Routes listener |

优先证明原生路径；只有存在稳定协议边界且有测试时才增加转换。不要把任意 API key 伪装成 OAuth，也不要把国产 OAuth 转成 API。

## 2. 找到真源和写入口

1. 在 `crates/agenthub-core/src/domain/protocol_graph/` 查来源、目标协议和可写登录面。
2. 在 `crates/agenthub-core/src/models/adapter.rs` 使用已有 route 类型，不创建平行字符串枚举。
3. 在对应的 adapter/integration 模块实现分析、计划和投影；共享校验、备份、锁和 apply 顺序留在 core service。
4. 通过 `src/lib/api/tickets` 的 `plan`、`bind`、`unbind` 完成产品写入。
5. `src/lib/api/adapter` 只用于预览和本机 Routes 的控制面；页面不得把它当产品写入入口。

如果目标没有可靠 writer，返回 unsupported，不把它列为可绑定目标。配置写入必须明确受管字段、备份路径和失败补偿。

## 3. 本机 Route 的额外要求

`local_bridge` 需要同时定义：

- downstream surface：Messages、Responses 或 Chat Completions；
- 若 surface 是 Responses：显式保存 Codex 或 Grok 格式，绑到本机令牌选中的路由，不从请求正文推断；
- upstream protocol：Anthropic Messages、OpenAI Chat Completions、Codex Responses 或 Grok Responses；
- endpoint/base URL 选择和模型名单；
- 本地随机 bearer、端口偏好和运行状态；
- 请求、流式响应、协议转换、超时、取消和上游认证的行为。

接到 Codex 的本机转发时，写入对方的是本机地址、本机令牌和 Responses（`preferred_auth_method = "apikey"`），不要改成官方登录文件。接到 Grok 时同样写 `api_backend = "responses"` 和本机令牌，不要再用 Chat Completions 当默认本机接口。Codex↔Grok 双向 Responses 仍是实验开关、默认关闭，不能当成身份转发已经做了格式转换。

listener 是进程内 Gateway；`agenthub-adapterd` 仍是目标架构，不要在本任务中额外创建 sidecar。完整 HTTP 面见 [local-route-api.md](../reference/local-route-api.md)。

## 4. 前端和命名

- UI 导航统一使用 `Routes` / `路由`；旧 `/adapter`、`/router`、`/bridges` 只作为兼容跳转。
- `bridge` 是内部实现词，适合代码、日志和开发者文档，不作为普通用户功能名。
- 页面通过 backend contract；仅 `src/lib/backend/tauri/` 可直接 `invoke`。
- mock 只在 `pnpm dev:mock` 和 Vitest 注入，生产 build 必须使用 Tauri adapter。

## 5. 测试门槛

至少加入：

- 协议分析和 route 选择的纯函数测试；
- plan/bind/unbind 的能力、writer、备份和失败补偿测试；
- local bridge 的 auth、health、models、surface mismatch、Responses 格式不匹配（`route_unavailable`）、模型拒绝、上游错误和 SSE 转换测试；
- 真实 HTTP fixture，禁止把完整 token、prompt 或上游原始错误写入 fixture；
- UI contract test，验证 unavailable/unsupported 和 Routes 状态。

本机 Route 的测试使用 loopback 临时端口和内存 token，不连接真实供应商。测试文件放在相邻 `tests.rs`/`*.test.ts`，不要把测试模块塞回生产实现文件。

## 6. 完成标准

- 选择的路线有本地实现证据和稳定 rule id；
- 写入经过 Tickets plan/bind/unbind，切换前有 live backup；
- 不支持的来源/目标明确返回 unsupported；
- 日志包含 profile/request 关联字段且已脱敏；
- 相关 core、Tauri contract、Vitest 和 `pnpm build` 通过。

