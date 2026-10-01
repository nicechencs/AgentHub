---
title: 添加路由接法
description: 为一个登录来源和目标 Agent 增加可验证的接法或本机转发。
type: guide
audience: contributor
status: current
updated: 2026-09-29
---

# 添加路由接法

这里的 adapter 指「一份登录接到目标 Agent 的方式」，不是新增 Agent 用的 `AgentAdapter`（那个见 [添加 Agent](adding-an-agent.md)）。界面上是三种接法：直接配置、写进对方认的登录、本机转发；代码里对应路线枚举 `native_endpoint`、`config_sync`、`local_bridge`。

## 1. 先分类

| 路线 | 何时使用 | 结果 |
|---|---|---|
| `native_endpoint` | 目标 Agent 能直接读取来源协议/端点 | 写入目标原生配置，不启动本机 listener |
| `config_sync` | 目标 Agent 能识别该登录，但需要字段投影或配置合并 | 通过目标 Agent 的配置/登录契约写入 |
| `local_bridge` | 只能靠本机协议转换连接 | 启动只监听 `127.0.0.1` 的本机转发 listener |

优先证明原生路径；只有协议边界稳定且有测试时才加转换。不要把 API Key 伪装成 OAuth，也不要把国产 OAuth 转成 API。

## 2. 找到规则来源和写入口

1. 在 `crates/agenthub-core/src/domain/protocol_graph/` 查来源、目标协议和可写登录面。
2. 在 `crates/agenthub-core/src/models/adapter.rs` 使用已有 route 类型，不创建平行字符串枚举。
3. 在对应的 adapter/integration 模块实现分析、计划和投影；共享校验、备份、锁和 apply 顺序留在 core service。
4. 产品写入走 `src/lib/api/tickets.ts` 的 `plan`、`bind`、`unbind`。
5. `src/lib/api/adapter.ts` 只用于预览和本机路由的控制面，页面不得把它当写入入口。

如果目标没有可靠 writer，返回 unsupported，不把它列为可绑定目标。配置写入必须明确受管字段、备份路径和失败补偿。

## 3. 本机转发的额外要求

`local_bridge` 需要同时定义：

- 下游端点（downstream surface）：Messages、Responses 或 Chat Completions；
- 端点是 Responses 时：显式保存 Codex 或 Grok 格式，跟入口 Key 选中的路由一起保存，不从请求正文推断；
- upstream protocol：Anthropic Messages、OpenAI Chat Completions、Codex Responses 或 Grok Responses；
- endpoint/base URL 选择和模型名单；
- 入口 Key（本地随机 bearer）、端口偏好和运行状态；
- 请求、流式响应、协议转换、超时、取消和上游认证的行为。

接到 Codex 时，写进对方配置的是本机地址、入口 Key 和 Responses（`wire_api = "responses"`、`preferred_auth_method = "apikey"`），不改官方登录文件。接到 Grok 时写 `api_backend = "responses"` 和入口 Key，不再默认用 Chat Completions。Codex↔Grok 互转 Responses 仍是实验开关（`feature.codex_ingress_grok_upstream` / `feature.grok_ingress_codex_upstream`），默认关闭。

listener 运行在 Tauri 进程内；`agenthub-adapterd` sidecar 只是提案，不要在本任务里创建。完整 HTTP 接口见 [本机路由 API](../reference/local-route-api.md)。

## 4. 前端和命名

- 导航用 `Routes` / 「路由」，现行路径是 `/routes`；旧 `/adapter`、`/router`、`/bridges` 只是兼容跳转。
- `bridge` 是内部词，只用于代码、日志和开发文档，界面说「本机转发」。
- 前端边界（`invoke`、mock）按 [AGENTS.md「前端 backend 分层」](../../AGENTS.md#前端-backend-分层)。

## 5. 测试门槛

至少加入：

- 协议分析和 route 选择的纯函数测试；
- plan/bind/unbind 的能力、writer、备份和失败补偿测试；
- 本机转发的鉴权、health、models、端点不匹配（`surface_mismatch`）、Responses 格式不匹配（`route_unavailable`）、模型拒绝、上游错误和 SSE 转换测试（`pnpm test:bridge`）；
- 真实 HTTP fixture，禁止把完整 token、prompt 或上游原始错误写入 fixture；
- UI contract test，验证 unavailable / unsupported 和路由状态。

本机转发测试用 loopback 临时端口和内存 token，不连真实供应商。测试放相邻 `tests.rs` / `*.test.ts`。发布前的真机验收见 [本机路由真机验收](adapter-dogfood.md)。

## 6. 完成标准

- 选择的路线有本地实现证据和稳定 rule id；
- 写入经过 plan / bind / unbind，切换前备份本机正在用的配置；
- 不支持的来源/目标明确返回 unsupported；
- 日志包含 profile/request 关联字段且已脱敏；
- 相关 core、Tauri contract、Vitest 和 `pnpm build` 通过。

