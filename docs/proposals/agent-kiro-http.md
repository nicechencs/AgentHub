---
title: Kiro HTTP / 本机转发
type: proposal
status: proposed
owner: maintainers
updated: 2026-09-09
audience: contributor
---

# Kiro HTTP / 本机转发

> 提案，不是现行实现契约。YAML 保持 `status: proposed`（STYLE 要求提案必须 proposed）。若干切片已落地，**不要把本页当成未开工**。现行行为见 [STATUS](../STATUS.md)。

接线纪律见 [添加 Agent](../guides/adding-an-agent.md)、[Connections 与路由](../concepts/connections-and-routing.md)、[产品边界](../decisions/product-boundaries.md)。用户文案用 **登录 / 本机路由 / 直连**，不写票、桥、PKCE。

## 进度（已落地 / 剩余边界）

对照 [STATUS](../STATUS.md)。本表只防止把提案当成未开工，不是现行契约。

| 状态 | 内容 |
| --- | --- |
| **已落地** | 列模型可走 HTTP |
| **已落地** | Chat 打印路径 HTTP 多轮，经 `kiro-http:<conversationId>` 续场；已有 HTTP 会话失败不回退 CLI |
| **已落地** | Kiro 登录经本机路由接到 Claude / Codex / Grok |
| **已落地** | 检测 / 安装 / 登录、ACP 新对话见 [CLI 半面](agent-kiro.md) |
| **已落地** | 本机路由 `stream=true`：上游 AWS event-stream 帧完成即转发文本块（单测分块 reader；真窗 TTFT 未验） |
| **剩余边界** | 企业 IdC / `profileArn` 实机验收（带上参数 ≠ 已验收） |
| **剩余边界** | Chat 打印路径与本机路由 JSON 仍收齐再返回；真窗 TTFT；客户端断开不停上游读 |
| **剩余边界** | 官方 REST（不宣称、不接入） |
| **历史约束（不是现行待办）** | CLI 早期方案里的「一轮一发」「不接持续通道」「本机路由后置」——见 [agent-kiro.md](agent-kiro.md) §3 |

## Goal

候选目标（切片已部分落地）：用用户自己的 Kiro 登录（或 `KIRO_API_KEY`）经 HTTP 列出模型并对话，使 Chat / 兼容客户端不必只靠每次拉起 `kiro-cli`；同一上游可接到本机路由。仅未开始的 HTTP 新会话可回退 CLI；已有 HTTP 会话失败时保留会话并报错，不能静默换成新对话。已接线部分与剩余边界见进度表。

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

## 当前范围

1. **嵌入 core**（不外挂第三方网关二进制）。
2. **Chat 与本机路由均已接入**；本机路由 SSE 按上游 event-stream 帧转发文本块（单测；真窗 TTFT 未验）。Chat 打印路径与 JSON 仍收齐再返回。
3. **Builder ID + `ksk_` API Key 先**；企业 IdC / `profileArn` / `runtime.*.kiro.dev` 仍是**剩余边界**（带上参数 ≠ 已验收）。
4. **当前双轨：** 新交互对话走 ACP；旧打印路径保留 HTTP / CLI。HTTP 新会话失败可回退 CLI，已有 HTTP 会话失败不回退；CLI 会话按原 `--resume-id` 继续。

## First slice（工作区；模块已落地）

- 模块：`crates/agenthub-core/src/adapters/kiro/http/`（creds / client / eventstream）已在工作区。
- 本机核实（Builder ID / OIDC DeviceCode）：`q.{region}.amazonaws.com` 上 ListAvailableModels + GenerateAssistantResponse；OIDC refresh 写回 sqlite。企业 IdC 不在本切片验收范围内。
- 不宣称官方 REST；不打包社区网关。


## Chat HTTP multi-turn（已接线）

- Chat 持久化的 `native_session_id`：HTTP 用 `kiro-http:<conversationId>`；CLI `--resume-id` 不加此前缀。
- `try_http_run_result`：有 HTTP 前缀则带 `conversationId` 续聊；有 CLI id 则跳过 HTTP；无 id 则新开 HTTP 对话。
- 已有 HTTP 会话在登录或上游失败时明确报错；仅无会话 id 的新请求可以回退 `kiro-cli`。HTTP id 不传给 `--resume-id`，也不用于 ACP 恢复。
- 本机路由按调用方的 `stream` 返回 JSON 或对应接口的 SSE。`stream=true` 时在上游 `assistantResponseEvent` 帧完成时转发文本块，不把收齐后的整段再切成假流。`stream=false` 与 Chat 打印路径仍收齐再返回。真窗 TTFT 未验。
- 本机路由使用共享库中所选登录的当前访问令牌，并带上该登录的区域、`profileArn` 与请求来源；不会借用或刷新其他本机登录，也不把刷新信息放进本机路由。登录过期后需同步共享库。Chat 打印路径直接读取本机 sqlite / SSO / `KIRO_API_KEY`（可从 sqlite `state` 补 `profileArn`），并继续沿用原有刷新逻辑。ACP 新对话只拉起 `kiro-cli acp`，不向进程注入 `profileArn`。带上这些参数不代表企业 IdC 场景已完成实机验收。

## Non-goals

- 不宣称官方支持或官方 REST。
- 不打包、不自动下载社区网关二进制；不整仓 vendor AGPL 网关。
- 不把本页写成验收禁令清单；实现切片与能力矩阵按添加 Agent / Route 指南另开。

## Pointers

- CLI 半面：[agent-kiro.md](agent-kiro.md)
- 本机路由语义：[connections-and-routing.md](../concepts/connections-and-routing.md)、[local-route-api.md](../reference/local-route-api.md)、[adding-an-adapter.md](../guides/adding-an-adapter.md)
- 产品词与不做项：[product-boundaries.md](../decisions/product-boundaries.md)
