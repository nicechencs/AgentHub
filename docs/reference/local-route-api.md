---
title: 本机 Routes API
description: AgentHub 进程内 Gateway 的 loopback HTTP endpoint、鉴权和错误码。
type: reference
audience: integrator
status: current
updated: 2026-08-27
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

`/models` 是兼容别名。对话请求使用带 `/v1` 的路径。

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
| Chat Completions | OpenAI Chat Completions | 同协议 `chat/completions` |
| 任一支持 surface | 其它已注册协议 | 按 Route profile 的转换器处理 |

请求路径和 Route surface 不匹配时，认证成功后返回 `404`，日志 code 为 `surface_mismatch`。

## 错误码

| HTTP | code | 含义 |
|---|---|---|
| `400` | `invalid_request` / `ProtocolError.code` | JSON、字段或协议不合法 |
| `400` | `listed_models_reject` | model 不在用户提供的名单 |
| `400` | `model_unavailable` | 没有正在运行的 Route 可提供该 model |
| `401` | `invalid_api_key` | 本机令牌无效 |
| `404` | `surface_mismatch`（日志） | 请求 surface 与 Route 不匹配；响应 body 为空 404 |
| `429` | `bridge_overloaded` | 本机并发门限已满，并带 `Retry-After: 1` |
| `503` | `bridge_stopping` | listener 正在停止 |
| `503` | `pool_exhausted` | 默认池当前没有可服务该请求的成员；可能带 `Retry-After` |
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

