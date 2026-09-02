---
title: 本机 Routes API
description: AgentHub 进程内 Gateway 的 loopback HTTP endpoint、鉴权和错误码。
type: reference
audience: integrator
status: current
updated: 2026-09-02
---

# 本机 Routes API

本机 Routes 是 AgentHub 进程内 Gateway，监听 `127.0.0.1:<port>`。它面向本机 Agent 客户端，不是公网 API；上游凭据不会通过下游响应返回。UI 称为 Routes/路由，`bridge` 是内部实现名。

## 鉴权

所有 endpoint 都要求该 Route 的本机令牌：

```text
Authorization: Bearer <local-token>
```

缺失或错误返回 `401`：

```json
{"error":{"code":"invalid_api_key","message":"Invalid local bearer token.","type":"invalid_request_error"}}
```

本机令牌是 AgentHub 生成的 Route 凭据，不等于上游 provider/API key。默认池按 route/surface 持有一把令牌：往池里增删登录不会改客户端要写的口和令牌。不要把它提交到日志、Issue 或 fixture。

## Endpoint

| 方法 | 路径 | 说明 |
|---|---|---|
| `GET` | `/health` | 返回 listener 状态和最近观察到的上游状态；不会发起新的 provider 探测 |
| `GET` | `/v1/models`、`/models` | 返回当前默认池可服务的模型并集；由本机 resolver 合成，不代理上游目录 |
| `POST` | `/v1/responses` | Responses surface；是否可用取决于该 Route 的 downstream surface |
| `POST` | `/v1/messages` | Anthropic Messages surface |
| `POST` | `/v1/chat/completions`、`/chat/completions` | OpenAI Chat Completions surface |

`/models` 与 `/chat/completions` 是兼容别名。其余对话路径使用 `/v1/messages`、`/v1/responses`、`/v1/chat/completions`。对这些对话路径发 `GET`/`PUT` 等非 POST 方法返回 `405` `method_not_allowed`（双语 JSON + `Allow: POST`），不会返回空 body。

目标客户端：Claude 用 `/v1/messages`；Codex 和 Grok 用 `/v1/responses`（配置里写本机令牌，按 API Key 方式）；Kimi / DSH 用 `/v1/chat/completions`。

## Models 响应

```json
{
  "object": "list",
  "data": [
    {"id": "model-id", "object": "model"}
  ]
}
```

名单由该 Route 的共享 resolver 决定：静态映射、成员能力和运行时证据合成一份索引，`GET /models` 与实际 dispatch 读同一代。默认池列出当前成员可服务模型的并集。名单为空时 fail-closed，不会把未知模型发给任意上游。不要把 `GET /models` 当官方模型目录。

## Route surface 和上游协议

每个 Route 绑定一个 downstream surface。Gateway 可按目标协议直通或转换：

| downstream | 常见 upstream | 行为 |
|---|---|---|
| Messages | Anthropic Messages | 同协议 `/messages` |
| Messages | OpenAI Chat Completions | 转换请求和响应 |
| Responses | Codex/Grok Responses | 同协议 `/responses` |
| Responses | OpenAI Chat Completions | 转换请求和响应 |
| Responses | Anthropic Messages | 转换（Anthropic Key → Codex） |
| Messages | xAI Responses | 转换（Grok 订阅 → Claude） |
| Chat Completions | OpenAI Chat Completions | 同协议 `chat/completions` |
| Chat Completions | Codex Responses | 转换（Codex 订阅 → Kimi / DSH） |
| 任一支持 surface | 其它已注册协议 | 按 Route profile 的转换器处理 |

Codex 与 Grok 都使用 `POST /v1/responses`。具体 Responses 格式（Codex 或 Grok）跟这条路由一起保存，由本机令牌选中，**不**根据请求正文或 URL 猜测。Messages 与 Chat Completions 不会继承这份 Responses 格式。接到 Codex 时写入 `wire_api = "responses"` 和 `preferred_auth_method = "apikey"`，本机令牌进 `auth.json` 的 `OPENAI_API_KEY`；接到 Grok 时写入 `api_backend = "responses"`，本机令牌进 `config.toml` 的 `api_key`。这与 Codex↔Grok 双向转换开关无关。

请求路径和 Route surface 不匹配时，认证成功后返回 `404`，code 为 `surface_mismatch`，响应为双语 JSON（说明本机只提供的路径）。`feature.codex_ingress_grok_upstream` / `feature.grok_ingress_codex_upstream` 默认关闭；关闭时 Responses 按身份转发。打开且当前来源/目标边未授权时，返回 `503`，日志 code 为 `route_unavailable`，不会退回直通。已保存的池 surface 或 Responses 格式与当前端点不一致时，准备启动失败，不会带着错误格式启动。

## 错误码

| HTTP | code | 含义 |
|---|---|---|
| `400` | `invalid_request` / `ProtocolError.code` | JSON、字段或协议不合法 |
| `400` | `listed_models_reject` | model 不在用户提供的名单 |
| `400` | `model_unavailable` | 没有正在运行的 Route 可提供该 model |
| `400` | `continuation_unavailable` | 需要同一登录才能继续的对话，原成员已不可用 |
| `401` | `invalid_api_key` | 本机令牌无效 |
| `404` | `surface_mismatch` | 请求 surface 与 Route 不匹配；JSON 双语说明应改打的路径 |
| `405` | `method_not_allowed` | 对话路径只接受 POST；JSON 双语说明 + `Allow: POST` |
| `408` | `request_timeout` | 请求体读取超时 |
| `429` | `bridge_overloaded` | 本机并发门限已满，并带 `Retry-After: 1` |
| `503` | `bridge_stopping` | listener 正在停止 |
| `503` | `pool_exhausted` | 默认池当前没有可服务该请求的成员；可能带 `Retry-After` |
| `503` | `route_unavailable` | 已打开 Codex↔Grok Responses 转换，但当前边未授权；不会退回直通 |
| `502` | `upstream_error` | 上游不可用、认证失败或返回无效响应 |
| `504` | `upstream_timeout` | 上游非流式请求超时 |

上游短错误只写入 AgentHub 脱敏日志，不原样转发给下游客户端。流式错误以 SSE error frame 返回，具体转换能力由协议测试覆盖。

## 运行约束

- 只绑定 loopback，不提供公网监听或 CORS 网关。
- Route 的启动、停止、apply 和恢复由 backend/Tauri 控制面完成；不要手写第二个 listener。
- 已提交下游第一个字节后不再换成员或重放；提交前可按健康与模型切合格成员。
- `local_bridge` 只是三种 adapter route 之一；能用 `native_endpoint` 或 `config_sync` 时优先原生路径，官方直连不会自动改成本机转发。

## 相关页面

- [Connections、Routes 与绑定](../concepts/connections-and-routing.md)
- [Adapters 与本机 Bridge](../concepts/adapters-and-bridges.md)
- [Route 兼容性](route-compatibility.md)
- [本机同口授权池（归档）](../archive/unified-loopback-pool.md)：默认同口池的设计记录；现行契约以本页为准。

