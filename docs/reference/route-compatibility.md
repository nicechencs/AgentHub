---
title: Route 兼容性参考
description: 当前来源、目标、路由、规则和适用门禁的可核对快照。
type: reference
status: current
owner: maintainers
updated: 2026-08-29
---

# Route 兼容性参考

本文记录协议图中已经声明的 Adapter capability cells。这里的 Route 是产品界面的“路由”；源码内部的 `LocalBridge` 表示 Hub 在本机提供 loopback 转发，并不表示目标 Agent 获得了来源凭据。

## 真源与判定规则

- 生成真源是 `crates/agenthub-core/src/domain/protocol_graph/adapter_capability_matrix.rs`；LocalBridge 的 `rule_id`、来源、目标、传输、协议和默认模型由 `adapter_capability_matrix/local_bridge_edges.rs` 单一声明派生。
- 本页快照更新时间为 **2026-08-29**。矩阵常量为 `MATRIX_VERSION = "1"`、默认 `VERIFIED_AT = "2026-08-12"`；表中单元格的 `verified_at` 以各行源码为准。2026-08-29 的 Responses 本机令牌改动没有新增或删除矩阵 cell，只把 Codex/Grok Responses 格式绑到路由本身。动态状态变更后，以源码、`agenthub agent capabilities` 以及下列测试重新核对，不以本页文字作为第二份真源。
- 建议核对命令：`cargo test -p agenthub-core --locked adapter_capability_matrix`。该测试覆盖规则 ID、路线、`can_apply`、成熟度、门禁和 LocalBridge 清单。
- `can_apply` 是矩阵层的写入/启动标志。实际 plan 还要满足来源凭据、目标 writer 和 plan 的 private `write_gate`；源码定义为“矩阵 `can_apply` 与 plan `write_gate` 的交集”。因此表中的 `true` 不承诺当前每个账户都能直接应用。
- 七项 gate 为 `official_contract`、`terms_reviewed`、`endpoint_stable`、`auth_refresh`、`protocol_conversion`、`isolation_verified`、`e2e_verified`。`all_open` 表示七项为 `true`；`all_closed` 表示七项均为 `false`。
- 成熟度使用 planner 的 `adapter_maturity_from_decision`：开放 stable/experimental cell 分别显示 `Stable`/`Experimental`；有 rule 但不允许应用的记录显示 `Preview`。订阅候选在公开 analyze/plan surface 上仍会 fail-closed 为 `Unsupported`。

## 当前兼容矩阵

来源单元格包含 credential class：`ApiKey`、`OauthAuthJson`（Codex `auth_json` 形态）或 `OauthOther`。`route` 是源码中的 `AdapterRoute`；`limits` 是下方“限制组”中的源码常量名。

| source → target | rule id | route | maturity | can_apply / gate | verified_at | limits |
|---|---|---|---|---|---|---|
| `KimiCodeMembership / ApiKey → Claude` | `kimi-membership-to-claude-v1` | `native_endpoint` | Stable | `true / all_open` | 2026-08-12 | `KIMI_CLAUDE_LIMITS` |
| `GlmCodingPlan / ApiKey → Codex` | `glm-coding-plan-to-codex-v1` | `native_endpoint` | Experimental | `true / all_open` | 2026-08-15 | `CODEX_NATIVE_API_LIMITS` |
| `DeepseekApi / ApiKey → Codex` | `deepseek-api-to-codex-v1` | `native_endpoint` | Experimental | `true / all_open` | 2026-08-15 | `CODEX_NATIVE_API_LIMITS` |
| `KimiCodeMembership / ApiKey → Codex` | `kimi-membership-to-codex-v1` | `local_bridge` | Experimental | `true / all_open` | 2026-08-12 | `KIMI_CODEX_LIMITS` |
| `KimiCodeMembership / ApiKey → Pi` | `kimi-membership-to-pi-v1` | `config_sync` | Stable | `true / all_open` | 2026-08-12 | `KIMI_PI_LIMITS` |
| `AnthropicApi / ApiKey → Pi` | `anthropic-api-to-pi-v1` | `config_sync` | Stable | `true / all_open` | 2026-08-12 | `ANTHROPIC_PI_LIMITS` |
| `AnthropicApi / ApiKey → Codex` | `anthropic-api-to-codex-v1` | `local_bridge` | Experimental | `true / all_open` | 2026-08-12 | `ANTHROPIC_CODEX_LIMITS` |
| `OpenaiApi / ApiKey → Pi` | `openai-api-to-pi-v1` | `config_sync` | Stable | `true / all_open` | 2026-08-12 | `OPENAI_PI_LIMITS` |
| `OpenaiApi / ApiKey → Codex` | `openai-api-to-codex-v1` | `local_bridge` | Experimental | `true / all_open` | 2026-08-21 | `OPENAI_CODEX_LIMITS` |
| `OpenaiApi / ApiKey → Claude` | `openai-api-to-claude-v1` | `local_bridge` | Experimental | `true / all_open` | 2026-08-23 | `OPENAI_CLAUDE_LIMITS` |
| `OpenaiApi / ApiKey → Grok` | `openai-api-to-grok-bridge-v1` | `local_bridge` | Experimental | `true / all_open` | 2026-08-23 | `OPENAI_GROK_BRIDGE_LIMITS` |
| `XaiApi / ApiKey → Pi` | `xai-api-to-pi-v1` | `config_sync` | Stable | `true / all_open` | 2026-08-12 | `XAI_PI_LIMITS` |
| `GlmCodingPlan / ApiKey → Pi` | `glm-coding-plan-to-pi-v1` | `config_sync` | Experimental | `true / all_open` | 2026-08-15 | `GLM_PI_LIMITS` |
| `DeepseekApi / ApiKey → Pi` | `deepseek-api-to-pi-v1` | `config_sync` | Experimental | `true / all_open` | 2026-08-15 | `DEEPSEEK_PI_LIMITS` |
| `GlmCodingPlan / ApiKey → Claude` | `glm-coding-plan-to-claude-v1` | `native_endpoint` | Experimental | `true / all_open` | 2026-08-12 | `GLM_CLAUDE_LIMITS` |
| `DeepseekApi / ApiKey → Claude` | `deepseek-api-to-claude-v1` | `native_endpoint` | Experimental | `true / all_open` | 2026-08-12 | `DEEPSEEK_CLAUDE_LIMITS` |
| `DeepseekApi / ApiKey → Dsh` | `deepseek-api-to-dsh-v1` | `config_sync` | Stable | `true / all_open` | 2026-08-12 | `DEEPSEEK_DSH_LIMITS` |
| `ClaudeSubscription / OauthOther → Pi` | `claude-subscription-to-pi-v1` | `config_sync` | Experimental | `true / all_open` | 2026-08-15 | `SUBSCRIPTION_PI_APPLY_LIMITS` |
| `CodexChatGptSubscription / OauthAuthJson → Pi` | `codex-subscription-to-pi-v1` | `config_sync` | Experimental | `true / all_open` | 2026-08-15 | `SUBSCRIPTION_PI_APPLY_LIMITS` |
| `CodexChatGptSubscription / OauthOther → Pi` | `codex-subscription-to-pi-v1` | `config_sync` | Experimental | `true / all_open` | 2026-08-15 | `SUBSCRIPTION_PI_APPLY_LIMITS` |
| `XaiGrokSubscription / OauthOther → Pi` | `grok-subscription-to-pi-v1` | `config_sync` | Experimental | `true / all_open` | 2026-08-15 | `SUBSCRIPTION_PI_APPLY_LIMITS` |
| `KimiCodeMembership / ApiKey → Grok` | `kimi-membership-to-grok-v1` | `native_endpoint` | Experimental | `true / all_open` | 2026-08-15 | `GROK_NATIVE_LIMITS` |
| `OpenaiApi / ApiKey → Grok` | `openai-api-to-grok-v1` | `native_endpoint` | Experimental | `true / all_open` | 2026-08-15 | `GROK_NATIVE_LIMITS` |
| `XaiGrokSubscription / OauthOther → Claude` | `grok-subscription-to-claude-v1` | `local_bridge` | Experimental | `true / all_open` | 2026-08-15 | `GROK_CLAUDE_LIMITS` |
| `XaiGrokSubscription / OauthOther → Codex` | `grok-subscription-to-codex-v1` | `local_bridge` | Experimental | `true / all_open` | 2026-08-20 | `GROK_CODEX_LIMITS` |
| `CodexChatGptSubscription / OauthAuthJson → Claude` | `codex-subscription-to-claude-app-server-v0` | `local_bridge` (public `Unsupported`) | Preview | `false / all_closed` | 2026-08-12 | `CODEX_CLAUDE_LIMITS` |
| `CodexChatGptSubscription / OauthAuthJson → Claude` | `codex-subscription-to-claude-responses-v1` | `local_bridge` | Experimental | `true / all_open` | 2026-08-15 | `CODEX_CLAUDE_LIMITS` |
| `CodexChatGptSubscription / OauthOther → Claude` | `codex-subscription-to-claude-responses-v1` | `local_bridge` | Experimental | `true / all_open` | 2026-08-15 | `CODEX_CLAUDE_LIMITS` |
| `CodexChatGptSubscription / OauthAuthJson → Codex` | `codex-subscription-to-codex-v1` | `native_endpoint` | Stable | `true / all_open` | 2026-08-20 | `CODEX_OFFICIAL_SELF_LIMITS` |
| `CodexChatGptSubscription / OauthOther → Codex` | `codex-subscription-to-codex-v1` | `native_endpoint` | Stable | `true / all_open` | 2026-08-20 | `CODEX_OFFICIAL_SELF_LIMITS` |
| `CodexChatGptSubscription / OauthAuthJson → Grok` | `codex-subscription-to-grok-v1` | `local_bridge` | Experimental | `true / all_open` | 2026-08-20 | `CODEX_CHAT_LIMITS` |
| `CodexChatGptSubscription / OauthOther → Grok` | `codex-subscription-to-grok-v1` | `local_bridge` | Experimental | `true / all_open` | 2026-08-20 | `CODEX_CHAT_LIMITS` |
| `CodexChatGptSubscription / OauthAuthJson → Kimi` | `codex-subscription-to-kimi-v1` | `local_bridge` | Experimental | `true / all_open` | 2026-08-20 | `CODEX_CHAT_LIMITS` |
| `CodexChatGptSubscription / OauthOther → Kimi` | `codex-subscription-to-kimi-v1` | `local_bridge` | Experimental | `true / all_open` | 2026-08-20 | `CODEX_CHAT_LIMITS` |
| `CodexChatGptSubscription / OauthAuthJson → Dsh` | `codex-subscription-to-dsh-v1` | `local_bridge` | Experimental | `true / all_open` | 2026-08-20 | `CODEX_CHAT_LIMITS` |
| `CodexChatGptSubscription / OauthOther → Dsh` | `codex-subscription-to-dsh-v1` | `local_bridge` | Experimental | `true / all_open` | 2026-08-20 | `CODEX_CHAT_LIMITS` |
| `ClaudeSubscription / OauthOther → Codex` | `claude-subscription-to-codex-v1` | `local_bridge` | Preview | `false / all_closed` | 2026-08-22 | `CLAUDE_CODEX_LIMITS` |

## 限制组

限制组名称直接对应 `adapter_capability_matrix.rs` 或 `adapter_capability_matrix/local_bridge_edges.rs` 中的常量；下列摘要保留会影响用户操作的事实：

| 限制组 | 当前限制 |
|---|---|
| `KIMI_CLAUDE_LIMITS` | 写入 Claude 的 base URL 和凭据引用标记；应用后切换当前 Claude Connection，需避免并行配置写入。 |
| `CODEX_NATIVE_API_LIMITS` | 使用官方 Responses 端点，不开本机转发；自动生成配置只保存凭据引用，当前不写官方 `~/.codex/models.json`。 |
| `KIMI_CODEX_LIMITS` | 本机转发、Codex 指向 loopback、Hub 保持托盘运行；长流/工具仍属实验性；固定端口占用会尝试重分配并回写。 |
| `KIMI_PI_LIMITS` / `ANTHROPIC_PI_LIMITS` / `OPENAI_PI_LIMITS` / `XAI_PI_LIMITS` | 写入 Pi `models.json` 对应 provider 与凭据引用标记，配置会成为 Pi 当前连接；预览不传输明文 Key。 |
| `ANTHROPIC_CODEX_LIMITS` | 本机转发，下游 Responses、上游 Anthropic Messages；Hub 需在托盘运行，端口冲突会重分配并回写。 |
| `OPENAI_CODEX_LIMITS` | 本机转发，下游 Responses、上游 OpenAI Chat Completions；Hub 需在托盘运行并处理端口重分配。接到 Codex 时写入 `wire_api = "responses"` 和 `preferred_auth_method = "apikey"`，本机令牌进 `auth.json` 的 `OPENAI_API_KEY`。 |
| `OPENAI_CLAUDE_LIMITS` | 本机转发，下游 Messages、上游 OpenAI Chat Completions；目标 Claude 指向本机端点并保留托盘运行。 |
| `OPENAI_GROK_BRIDGE_LIMITS` | 本机转发，下游 Responses、上游 OpenAI Chat Completions；目标 Grok 指向本机端点并保留托盘运行。接到 Grok 时写入 `api_backend = "responses"`，本机令牌进 `config.toml` 的 `api_key`。 |
| `GLM_PI_LIMITS` / `DEEPSEEK_PI_LIMITS` | 写入 Pi 自定义 provider 的 `baseUrl`、`api`、`models` 和凭据引用；live 写入才 materialize，回填前 scrub 明文。 |
| `GLM_CLAUDE_LIMITS` / `DEEPSEEK_CLAUDE_LIMITS` | 写入 Claude 的 Anthropic 兼容 Base URL 和凭据引用；实验性入口，部分扩展字段可能忽略或不支持。 |
| `DEEPSEEK_DSH_LIMITS` | 写入 DeepSeek Harness 的 home 级 provider 引用和凭据文件，不把 API Key 写入 `cordis.patch.yml`。 |
| `SUBSCRIPTION_PI_APPLY_LIMITS` | 官方登录写入 Pi 认的位置；由 Pi 自己续期，AgentHub 不重复刷新；原工具和 Pi 一起续期可能互相踢下线。 |
| `GROK_NATIVE_LIMITS` | 写入 Grok `config.toml` 的 OpenAI Chat Completions 模型位置，不开本机转发；只接受官方 Kimi Code/OpenAI API 标记。 |
| `GROK_CLAUDE_LIMITS` | Claude 指向本机，Claude 不接收上游 xAI OAuth token；本机 Messages→xAI Responses（`cli-chat-proxy`），Grok 过期需重新同步。 |
| `GROK_CODEX_LIMITS` | Codex 指向本机，Grok 登录不会写入 Codex；Hub 不自动刷新过期登录，需重新同步。接到 Codex 时同样写 Responses + 本机 API Key，不写官方登录文件。 |
| `CODEX_CLAUDE_LIMITS` | Claude 的 `ANTHROPIC_BASE_URL`/`ANTHROPIC_AUTH_TOKEN` 指向本机，上游 Codex token 不进 Claude；Responses 适配实验性，过期需重新同步。App Server v0 记录仍关闭。 |
| `CODEX_OFFICIAL_SELF_LIMITS` | 官方登录写进 Codex，不改本机路由；应用后成为 Codex 当前登录。 |
| `CODEX_CHAT_LIMITS` | 目标 Agent 指向本机，上游 Codex 官方登录不写入对方；过期需重新同步，Hub 不自动刷新。接到 Grok 时写 `api_backend = "responses"` 和本机令牌。 |
| `CLAUDE_CODEX_LIMITS` | Codex 指向本机，上游 Claude 订阅 token 不写入 Codex；规则未完成、thinking 无签名时降级关闭，过期需重新同步。 |

所有 LocalBridge cell 当前 `multi_account = false`；不能据此推导多账号轮询或后台自动刷新已经可用。

## Fail-closed 情况

矩阵没有声明的完整 key（来源、credential、transport、目标、协议或版本任一不同）不会被名称猜测升级，返回 `Unsupported` 且 `can_apply = false`。另外，源码对以下情况保留明确关闭原因：

- `XaiGrokSubscription → Kimi`：Kimi 只接受自己的官方 Key。
- `XaiGrokSubscription → Dsh`：DSH 只接受 DeepSeek 官方 Key。
- `XaiApi → Grok` / `XaiApi → Codex`：协议能对上，但没有格子，返回 `Unsupported`。
- 没有 writer 的目标（例如 Cursor）直接 fail-closed。
- Codex/ChatGPT 订阅到 Claude 的 App Server candidate，以及 Claude 订阅到 Codex 的 preview 规则，虽然保留 rule id 供审计，当前 gate 全闭，不得应用。App Server 在公开 analyze/plan 上显示 `Unsupported`；Claude→Codex preview 仍显示 `local_bridge`，只是 `can_apply = false`。

`plan()` 对 `OpenaiApi / ApiKey → Grok` 选用先出现的开放格子 `openai-api-to-grok-bridge-v1`（本机转发）；`openai-api-to-grok-v1` 仍是官方 Chat Completions 直连格子，不是默认 plan 赢家。接到 Codex / Grok 的本机转发与 Codex↔Grok 双向 Responses 实验开关无关。

`route-compatibility` 只描述适配决策；本机端点、启动、健康检查、SSE、工具调用和退出排空的操作证据见 [Adapter 真机 Dogfood](../guides/adapter-dogfood.md)，端点和生命周期参考见 [Local Route API](local-route-api.md)。
