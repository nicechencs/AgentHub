---
title: Kiro HTTP / 本机转发
type: proposal
status: proposed
owner: maintainers
updated: 2026-09-07
audience: contributor
---

# Kiro HTTP / 本机转发

> 提案，不是现行实现契约。与 [接入 Kiro（kiro-cli）](agent-kiro.md) 互补：那边管半面 CLI；这边探「用户自己的 Kiro 登录 / API Key → HTTP，供 Chat 与本机路由用」。

接线纪律见 [添加 Agent](../guides/adding-an-agent.md)、[Connections 与路由](../concepts/connections-and-routing.md)、[产品边界](../decisions/product-boundaries.md)。用户文案用 **登录 / 本机路由 / 直连**，不写票、桥、PKCE。

## Goal

用用户自己的 Kiro 登录（或 `KIRO_API_KEY`）经 HTTP 列出模型并对话，使 Chat / 兼容客户端不必只靠每次拉起 `kiro-cli`；可选把同一上游接到本机路由，供其他工具走 loopback。CLI headless 仍作合法回退。

## Facts（社区与公开材料；≠ 本仓库已验证）

- **官方无公开 REST**：无稳定对外 Chat Completions 文档；社区网关逆向 / 旁路调用是当前可行路径（[jwadow/kiro-gateway](https://github.com/jwadow/kiro-gateway)、[chasedputnam/go-kiro-gateway](https://github.com/chasedputnam/go-kiro-gateway)）。只引思路，不整仓搬代码、不捆绑第三方二进制。
- **凭据来源（常见）：**
  - `kiro-cli` SQLite `auth_kv`（token / device-registration 键；Builder ID 与企业 SSO 均可）
  - `~/.aws/sso/cache/*.json`（含 `accessToken` / `refreshToken`；有无 `clientId`+`clientSecret` 区分 Desktop vs OIDC）
  - 环境变量 `KIRO_API_KEY`（`ksk_` 前缀；官方 CLI headless；API Key 路径通常不带 `profileArn`）
  - 刷新：Desktop → `prod.{region}.auth.desktop.kiro.dev/refreshToken`；OIDC → `oidc.{region}.amazonaws.com/token`
- **上游主机：** 推理/流式从旧 `q.{region}.amazonaws.com` 迁到 `runtime.{region}.kiro.dev`（社区称约 2026-05 起切换）；管理类仍可能走旧口或 `management.*`。请求侧常见 `Content-Type: application/x-amz-json-1.0` 与 `x-amz-target`（如 `AmazonCodeWhispererStreamingService.GenerateAssistantResponse`）；模型列表与聊天可能用不同 target 前缀。
- **`profileArn`：** Desktop / 部分 SSO 需要；Builder ID / 部分 OIDC 与 API Key 路径行为不一致（SSO 时有时在 kiro-cli `state` 表，不在 token JSON）。错配是常见 4xx 源。
- **客户端表面（社区常见）：** OpenAI `/v1/chat/completions`（及部分 `/v1/responses`）、Anthropic `/v1/messages`、`/v1/models`。
- **失败模式：** 主机与协议 churn；refresh 失效需重新登录；Builder ID vs 企业 `profileArn` / region 错位；区域连通性（VPN/代理）。

## Open decisions

1. **嵌入 vs 外挂：** 上游 HTTP 适配做进 AgentHub core（本机路由池一员），还是按需 spawn 受控 helper（进程边界清晰、升级独立）？
2. **先接哪张表面：** Chat 原生通道，还是先 OpenAI 兼容 loopback（本机路由 / 第三方客户端）？
3. **账号优先级：** 是否 **Builder ID / 个人登录与 `ksk_` API Key 先做**，企业 IdC / `profileArn` 后置？
4. **与 CLI 半面的关系：** HTTP 成功后 headless 是否降为探测/安装回退，还是长期双轨？

## Non-goals

- 不宣称官方支持或官方 REST。
- 不打包、不自动下载社区网关二进制；不整仓 vendor AGPL 网关。
- 不把本页写成验收禁令清单；实现切片与能力矩阵按添加 Agent / Route 指南另开。

## Pointers

- CLI 半面：[agent-kiro.md](agent-kiro.md)
- 本机路由语义：[connections-and-routing.md](../concepts/connections-and-routing.md)、[local-route-api.md](../reference/local-route-api.md)、[adding-an-adapter.md](../guides/adding-an-adapter.md)
- 产品词与不做项：[product-boundaries.md](../decisions/product-boundaries.md)
