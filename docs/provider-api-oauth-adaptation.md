# 模型厂商、API 与 OAuth 适配规则

> 状态：**当前工作区规则**，不代表已发布版本。
> 最近核对：2026-08-12。
> 本文是厂商入口、凭据类型和跨 Agent 适配规则的单一事实源；Adapter 的页面、运行时与协议桥架构见 [adapter-design.md](adapter-design.md)。

## 1. 先看结论

是否能够适配，由下面四项共同决定：

```text
来源产品 + 凭据类型 + 上游协议 + 目标客户端协议
```

厂商名称相同、都使用 OAuth，或都宣传“OpenAI 兼容”，都不足以证明可以互换。

| 概念 | 含义 | 例子 |
|---|---|---|
| 来源产品 | Key 或 Token 实际属于哪个产品 | Kimi Code 会员、Kimi 开放平台 |
| 凭据类型 | 凭据的签发与刷新方式 | API Key、PKCE OAuth、device code |
| 上游协议 | 服务实际接受的请求格式 | Anthropic Messages、Chat Completions、Responses |
| 目标客户端 | 最终读取配置并发起请求的 Agent | Claude Code、Codex、Pi |

硬规则：

1. **API Key 与 OAuth 分开判断**：支持某厂商 API Key，不等于支持其订阅 OAuth。
2. **协议必须写全名**：`OpenAI-compatible` 必须进一步区分 Chat Completions 与 Responses。
3. **同厂商不同产品不得混用**：Base URL、Key、额度和授权范围都可能不同。
4. **只认显式来源标记**：不能根据名称、标签或 URL 猜测凭据属于哪个产品。
5. **默认拒绝**：没有代码规则和测试的组合一律为 `unsupported`。
6. **不复制凭据**：Adapter 保存来源引用；真实凭据只在写入 live 配置或请求上游时短暂解析。

## 2. 厂商与产品入口

下表描述协议事实，不等于 AgentHub 已实现对应的跨 Agent 路由。实现状态以 [§4](#4-当前实现矩阵) 为准。

| 厂商 / 产品 | 常见凭据 | 协议或客户端约束 | AgentHub 当前结论 |
|---|---|---|---|
| Anthropic API / Claude Code | Anthropic API Key；Claude 官方登录 | Claude Code 可连接 Anthropic Messages 兼容网关 | 仅 Anthropic API Key → Pi 有预览规则；Claude OAuth 不跨 Agent 复用 |
| OpenAI API / ChatGPT | OpenAI API Key；ChatGPT 登录 | Codex 自定义 Provider 当前要求 Responses | 普通 OpenAI API Key 尚无 Adapter 规则；ChatGPT OAuth 只用于其明确支持的登录路径 |
| Kimi Code 会员平台 | 会员 API Key，**不是 OAuth** | 同一产品提供 Anthropic Messages 与 OpenAI Chat Completions 兼容入口 | 已有 Claude 直连、Codex 实验 Bridge、Pi 预览规则 |
| Kimi 开放平台 | 开放平台 API Key | 使用独立 Base URL、额度和产品契约 | 不与 Kimi Code 会员 Key 混用；当前无 Adapter 路由 |
| 智谱 GLM Coding Plan | Coding Plan API Key，**不是 OAuth** | 提供 Anthropic Messages 与 OpenAI Chat Completions 入口；套餐仅限官方支持的工具环境 | 当前无 Adapter 路由；不能由双协议入口推导 Codex Responses 直连 |
| DeepSeek API | DeepSeek API Key，**不是 OAuth** | 提供 Anthropic Messages 与 OpenAI Chat Completions 兼容入口；部分 Anthropic 字段会被忽略或不支持 | 当前无 Adapter 路由；官方支持 Claude Code 配置，但不能推导 Codex Responses 直连 |
| xAI / Grok | xAI API Key；xAI 登录 | API 与账号授权是不同入口 | 当前无跨 Agent Adapter 路由 |
| Google Gemini | Gemini API Key 或 Google 授权 | 原生 API 与 OpenAI 兼容入口需分别声明 | 仅作为候选来源；当前无 Adapter 路由 |

### 2.1 Kimi Code 会员双协议入口

| 协议 | Base URL | 常用 Endpoint 示例 |
|---|---|---|
| OpenAI Chat Completions 兼容 | `https://api.kimi.com/coding/v1` | `https://api.kimi.com/coding/v1/chat/completions` |
| Anthropic Messages 兼容 | `https://api.kimi.com/coding/` | `https://api.kimi.com/coding/v1/messages` |

`OpenAI 兼容`在这里特指 **Chat Completions**，不包含 Codex 所需的 Responses。因此：

- Kimi Code 会员 → Claude Code 可以使用 Anthropic 兼容入口直连。
- Kimi Code 会员 → Codex 需要本地协议转换，不能只改 Base URL。

### 2.2 Kimi Code 与 Kimi 开放平台

| 对比项 | Kimi Code 会员平台 | Kimi 开放平台 |
|---|---|---|
| Base URL | OpenAI Chat：`https://api.kimi.com/coding/v1`<br>Anthropic：`https://api.kimi.com/coding/` | `https://api.moonshot.cn/v1` |
| 凭据 | Kimi Code Console 创建的会员 API Key | Kimi 开放平台 API Key |
| 额度 | Kimi Code 会员编程权益 | 开放平台计费与额度 |
| 是否可混用 | 否 | 否 |

Kimi CLI `/login` 生成的 managed OAuth 又是第三种来源，不能伪装成上述任一 API Key。

### 2.3 GLM Coding Plan

| 协议 | Base URL | 适配边界 |
|---|---|---|
| Anthropic Messages | `https://open.bigmodel.cn/api/anthropic` | 可作为 Claude Code 等官方支持工具的原生端点候选 |
| OpenAI Chat Completions | `https://open.bigmodel.cn/api/coding/paas/v4` | 只代表 Chat Completions，不代表 Codex Responses |

GLM Coding Plan 的凭据和使用范围必须单独识别：

- 个人版和团队版均使用 Coding Plan API Key；官方明确说明团队套餐 Key 与平台其他 API Key 不通用。
- 套餐额度仅限官方列出的工具与产品环境，新增规则前必须确认目标工具仍在支持列表中。
- 官方 Coding Tool Helper 当前可管理 Claude Code、OpenCode、Crush 和 Factory Droid；它只能证明这些工具存在官方配置路径，不代表 AgentHub 已实现适配。
- 当前没有 GLM Adapter 规则。即使目标客户端支持相同协议，也必须补齐显式来源标记、规则代码和测试后才能开放。

### 2.4 DeepSeek API

| 协议 | Base URL | 适配边界 |
|---|---|---|
| OpenAI Chat Completions | `https://api.deepseek.com` | Base URL 不追加 `/v1`；不代表 Codex Responses |
| Anthropic Messages | `https://api.deepseek.com/anthropic` | 官方支持 Anthropic SDK 与 Claude Code，但不保证所有 Anthropic 扩展字段无损 |

DeepSeek 使用平台签发的 API Key，不是 OAuth。官方 Anthropic 兼容表中存在“忽略”或“不支持”的字段，因此新增规则时必须按目标 Agent 实测文本、流式输出、thinking、工具调用、停止原因和用量，不能只验证请求成功。当前 AgentHub 尚无 DeepSeek Adapter 规则。

## 3. 路由类型

| 路由 | 使用条件 | 是否运行本地服务 |
|---|---|---|
| `config_sync` | 目标 Agent 原生支持同一凭据和协议，仅需转换配置结构 | 否 |
| `native_endpoint` | 上游原生提供目标协议，只需写 Base URL、模型与凭据引用 | 否 |
| `local_bridge` | 授权允许，但上下游协议不同，且已有经过测试的转换器 | 是，仅 loopback |
| `unsupported` | 产品、凭据、协议、版本或授权边界未验证 | 否 |

Bridge 转换的是请求、流式事件、工具调用、停止原因和用量字段，不会把 OAuth Token “转换”为另一家 API Key。

## 4. 当前实现矩阵

| 显式来源 | 目标 | 分析结果 | 当前可执行状态 |
|---|---|---|---|
| Kimi Provider，`agent_id=kimi` 且 `meta.preset=kimi-code-membership` | Claude Code | stable `native_endpoint` | **可应用**；普通 Apply 服务当前唯一白名单 |
| 同上 | Codex | experimental `local_bridge` | **可实验应用**；`plan.canApply=true`，由 Tauri 专用 Bridge 路径执行，尚未完成端到端验收 |
| 同上 | Pi | stable `config_sync` | **仅预览**；不可写入 |
| Anthropic Provider，或显式 Anthropic API-key Account | Pi | stable `config_sync` | **仅预览**；不可写入 |
| 其他来源、目标或未标记记录 | 任意 | `unsupported` | 不产生写操作 |

补充边界：

- Kimi managed OAuth 不会被识别为 Kimi Code 会员 API Key。
- 普通 OpenAI、xAI、Gemini、Kimi 开放平台、GLM Coding Plan、DeepSeek API 或任意“兼容 API”目前都不会自动升级为 Adapter 规则。
- `stable` 表示规则结论稳定，不等于已经开放写入；是否可写还要看 Apply 白名单。
- Kimi → Codex 目前是唯一 Bridge 白名单，不代表已经提供通用协议网关。

## 5. OAuth 边界

AgentHub 当前可发起的登录与跨 Agent 适配是两套能力：

| 登录目标 | AgentHub 当前入口 | 能否据此跨 Agent 复用 |
|---|---|---|
| Claude | PKCE | 否；当前没有 OAuth Adapter 规则 |
| Codex / ChatGPT | PKCE | 否；仅用于明确支持该授权的客户端 |
| Grok / xAI | PKCE | 否；当前没有 OAuth Adapter 规则 |
| Pi | Anthropic PKCE、OpenAI Codex PKCE、xAI device code | 仅写入 Pi 对应的 provider 槽位；不能推导其他 Agent 可用 |
| Kimi | 当前没有 AgentHub OAuth 登录入口 | 会员 API Key 与 Kimi CLI managed OAuth 必须分开 |

OAuth access/refresh token 带有客户端、受众、范围和刷新语义。只有目标客户端公开支持相同契约，并且 AgentHub 增加显式规则与测试后，才允许 `config_sync`；否则应引导用户使用目标客户端自己的登录流程。

## 6. 判定顺序

```text
选择来源 + 目标 Agent
  → 确认来源产品、凭据类型与区域
  → 确认上游协议和目标客户端版本/协议
  → 目标是否原生支持同一配置？           是：config_sync
  → 上游是否原生提供目标协议？           是：native_endpoint
  → 是否有已测试且授权允许的转换器？     是：local_bridge
  → unsupported，并给出原因和替代路径
```

规则分析、计划与执行必须使用同一规则版本。`local_bridge` 由专用 Bridge 服务执行，不进入普通 `AdapterApplyService`；新增规则不得只在 UI 或命令层绕过计划门禁。

## 7. 新增或更新规则

每条规则至少记录：

- 来源产品、区域、凭据类型和显式识别字段；
- 上游协议、Base URL、必要 Endpoint 与官方资料；
- 目标 Agent、目标协议和适用版本；
- 路由、支持级别、限制与 `verified_at`；
- 分析、计划、写入和失败回滚测试；Bridge 还需协议 fixtures 与端到端测试。

出现以下变化时必须重新核对：厂商端点或认证方式变更、目标客户端协议升级、OAuth 刷新语义变化、规则代码或测试变化。未完成验证前保留原状态或降级为 `unsupported`，不能只更新文案日期。

## 8. 官方资料

- [OpenAI Codex Authentication](https://developers.openai.com/codex/auth)
- [OpenAI Codex Configuration Reference](https://developers.openai.com/codex/config-reference)
- [Anthropic Claude Code LLM gateway](https://docs.anthropic.com/en/docs/claude-code/llm-gateway)
- [Anthropic Claude Code getting started](https://docs.anthropic.com/en/docs/claude-code/getting-started)
- [Kimi Code 概览与 API Access](https://www.kimi.com/code/docs/en/)
- [Kimi Code 接入 Claude Code](https://www.kimi.com/code/docs/en/third-party-tools/claude-code.html)
- [Kimi Code 接入 Codex](https://www.kimi.com/code/docs/en/third-party-tools/codex.html)
- [Kimi Code CLI Providers](https://www.kimi.com/code/docs/en/kimi-code-cli/configuration/providers.html)
- [Kimi 开放平台文档](https://platform.kimi.com/docs/overview)
- [GLM Coding Plan 一键安装助手](https://docs.bigmodel.cn/cn/coding-plan/extension/coding-tool-helper)
- [GLM Coding Plan 接入工具与双协议端点](https://docs.bigmodel.cn/cn/coding-plan/tool/others)
- [GLM Coding Plan 快速开始](https://docs.bigmodel.cn/cn/coding-plan/quick-start)
- [DeepSeek Anthropic API compatibility](https://api-docs.deepseek.com/guides/anthropic_api)
- [DeepSeek 接入 Claude Code](https://api-docs.deepseek.com/quick_start/agent_integrations/claude_code/)
- [DeepSeek Models & Pricing（双协议 Base URL）](https://api-docs.deepseek.com/quick_start/pricing/)
- [Pi AI providers 与 OAuth](https://github.com/earendil-works/pi/blob/main/packages/ai/README.md)
- [Gemini API OpenAI compatibility](https://ai.google.dev/gemini-api/docs/openai)
