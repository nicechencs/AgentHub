---
title: Kiro HTTP / 本机转发
type: proposal
status: proposed
owner: maintainers
updated: 2026-09-29
audience: contributor
---

# Kiro HTTP / 本机转发

> 提案，不是现行契约。主体切片已落地，只剩下表的边界。现行行为见 [STATUS](../STATUS.md)。用户文案用「登录 / 本机路由 / 直连」。

## 当前基线

| 状态 | 内容 | 证据 |
| --- | --- | --- |
| 已落地 | 列模型、Chat 打印路径走 HTTP（Builder ID / OIDC / `KIRO_API_KEY`），主机 `q.{region}.amazonaws.com` | `crates/agenthub-core/src/adapters/kiro/http/`（`creds`、`client`、`eventstream`） |
| 已落地 | 多轮用 `kiro-http:<conversationId>` 续同一对话；已有 HTTP 会话失败直接报错，不回退 CLI；只有无会话 id 的新请求可回退 `kiro-cli` | `http/client.rs` 的 `HTTP_NATIVE_SESSION_PREFIX`；`adapters/kiro.rs` |
| 已落地 | Kiro 登录经本机路由接到 Claude / Codex / Grok；`stream=true` 时上游 `assistantResponseEvent` 帧完成即转发 | `http/client.rs`（单测覆盖分块读取） |
| 已落地 | 本机路由用共享库里所选登录的当前令牌、区域、`profileArn`；不在路由里刷新令牌 | [STATUS](../STATUS.md) Kiro 与本机路由段 |
| 已落地 | 检测 / 安装 / 登录、ACP 新对话 | 见 [接入 Kiro](agent-kiro.md) |

## 剩余边界

- 企业 IdC / `profileArn` / `runtime.{region}.kiro.dev` 实机验收。请求里带上参数不等于已验收。
- Chat 打印路径和本机路由 `stream=false` 仍收齐再返回；客户端断开时不停上游读取。
- 本机路由流式的真窗首字延迟未测。

## 门槛

- 企业 IdC 账号实机通过列模型、多轮、本机路由。
- 流式首字延迟在真窗有记录。

## 非目标

- 不宣称官方支持或官方 REST。
- 不打包或自动下载社区网关，不整仓引入 AGPL 网关代码（社区实现如 [jwadow/kiro-gateway](https://github.com/jwadow/kiro-gateway) 只作参考）。

## 未决问题

- 社区称推理主机约 2026-05 起迁到 `runtime.{region}.kiro.dev`，当前代码仍用 `q.{region}.amazonaws.com`。何时切换、是否双主机回退，需实测后定。
- `profileArn` 在 Builder ID、OIDC、API Key 三种登录下行为不一致，是常见 4xx 来源。

## 相关

- [接入 Kiro](agent-kiro.md)
- [连接与路由](../concepts/connections-and-routing.md)、[本机路由 API](../reference/local-route-api.md)
- [产品边界](../decisions/product-boundaries.md)
