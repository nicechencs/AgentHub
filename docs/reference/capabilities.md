---
title: 能力参考
description: AgentHub 能力键、四级状态和当前 Agent 能力快照。
type: reference
audience: contributor
status: current
updated: 2026-09-29
---

# 能力参考

能力回答“这个 Agent 能不能安全执行某类操作”，不是安装状态、配置参数或模型目录。来源是各 adapter 的 `capability()`（`crates/agenthub-core/src/adapters/*.rs`）；CLI 可生成当前快照：

```text
agenthub agent capabilities --markdown
```

## 状态

| 状态 | 含义 | 调用方行为 |
|---|---|---|
| `Full` | 已接入且契约完整 | 正常开放 |
| `Partial` | 可用但有已知降级 | 开放并显示原因 |
| `Unsupported` | 对方契约不存在或明确不支持 | fail closed |
| `Planned` | AgentHub 尚未接入 | fail closed，并标明是路线图 |

非 `Full` 必须有 `reason`。静态能力与 detect 得到的安装/版本状态不能合并。

## 能力键

`ConfigWrite`、`AccountSwitch`、`ApiKeyAccount`、`Skills`、`LiveBackup`、`StructuredStream`、`DangerousMode`、`ProjectHistory`、`ProjectDelete`、`ProviderPresets`、`Usage`、`Mcp`、`ModelSelect`、`SessionResume`（共 14 个）。

## 当前快照

2026-09-29 用 `agenthub agent capabilities --markdown` 核对。变更后以 CLI 输出和 Rust 源码为准。

| 能力 | claude | codex | kimi | grok | pi | workbuddy | cursor | dsh | zcode | kiro |
|---|---|---|---|---|---|---|---|---|---|---|
| ConfigWrite | Full | Full | Full | Full | Full | Partial | Unsupported | Partial | Partial | Unsupported |
| AccountSwitch | Full | Full | Full | Full | Full | Partial | Unsupported | Partial | Partial | Partial |
| ApiKeyAccount | Full | Partial | Full | Full | Partial | Full | Unsupported | Full | Full | Partial |
| Skills | Full | Full | Partial | Full | Full | Full | Full | Full | Full | Planned |
| LiveBackup | Full | Full | Full | Full | Full | Full | Unsupported | Full | Full | Full |
| StructuredStream | Full | Full | Full | Full | Full | Unsupported | Unsupported | Planned | Unsupported | Partial |
| DangerousMode | Full | Full | Partial | Full | Partial | Full | Full | Partial | Unsupported | Partial |
| ProjectHistory | Full | Full | Full | Full | Full | Full | Full | Full | Partial | Partial |
| ProjectDelete | Full | Full | Full | Full | Full | Full | Unsupported | Partial | Unsupported | Unsupported |
| ProviderPresets | Full | Full | Full | Full | Unsupported | Unsupported | Unsupported | Partial | Unsupported | Unsupported |
| Usage | Full | Full | Full | Full | Full | Full | Unsupported | Full | Full | Partial |
| Mcp | Partial | Partial | Planned | Partial | Planned | Partial | Partial | Planned | Planned | Planned |
| ModelSelect | Planned | Planned | Planned | Partial | Planned | Planned | Planned | Planned | Planned | Full |
| SessionResume | Partial | Partial | Planned | Partial | Planned | Planned | Planned | Planned | Planned | Partial |

## 各家说明

- **Cursor**：`ConfigWrite` / `AccountSwitch` / `ApiKeyAccount` 为 Unsupported——可以导入本机已有登录，但不能配置 API Key，也不能写回 Cursor；切换失败给中文说明。store-stamp 默认软隐藏 Cursor Agent（见 [STATUS](../STATUS.md)）。
- **Kiro**：`AccountSwitch` Partial（可在连接里切换并写回 Kiro）；`ProjectHistory` Partial（列出命令行和编辑器里的对话，删除要到 Kiro 里做）；`Usage` Partial（读 kiro-cli 会话 token，日志没写时总览为 0）；`ModelSelect` Full（HTTP 或 CLI 列模型）。
- **ZCode**：API Key 追加写入 `~/.zcode/v2/config.json` 的一条供应商（官方槽或自定义行），不替换其他条目；套餐登录不导入。「历史」可列出任务并预览对话，删除要到 ZCode 里做。
- **WorkBuddy**：自定义模型按 `models.json` 一行一份登录追加，只写 `/v1/chat/completions`；DeepSeek 官方的 `/chat/completions` 写入时改成 `/v1/chat/completions`。桌面套餐登录不导入。
- WorkBuddy / ZCode 的“安装”只打开官网，不算安装失败。
- **占用方式**：WorkBuddy / ZCode 是追加（只动对应那一行），Pi / DSH 是具名槽，其余默认独占。
- **DSH 用量与历史**：会话日志是 `~/.dsh/sessions/--<cwd>--/<会话 id>/session.vN.jsonl.zstd`（多个 zstd 帧拼接），token 在每步的 `data.usage`。会话 id 取目录名；平铺的单文件日志回退用文件名。尾帧不完整或损坏时，已解出的行照常计入，但这次读取记为失败（`usage health` 与 collect 输出会体现），文件继续追加后从头重扫。读取代码在 `crates/agenthub-core/src/usage/session_jsonl.rs`。

## 容易混淆的地方

- `Skills: Full` 指共享库与各工具的启用矩阵，**不等于** Chat 里能为本轮指定「用于本次」技能。只有 Codex 持续通道接了「用于本次」；Grok `Skills` 是 Full，但 Chat「用于本次」不支持。见 [Chat 与 Agent 运行](../concepts/chat-and-agents.md)。
- `Mcp: Partial` 指 Claude / Codex / Grok / Cursor / WorkBuddy 已有无 OAuth 的写入/启用；只读 MCP 盘点不等于完整管理，也不等于厂商插件包。见 [MCP inventory](mcp-inventory.md) 与 [Agent 插件表面](agent-plugin-surfaces.md)。
- 没有 `Capability::Plugins`：`/plugins` 列出 Claude / Grok / Pi 的包，Claude / Grok 可安装/卸载/启用/停用，更新仍是[提案](../proposals/plugin-management.md)。
- 本机 Routes 的 `/v1/models` 不改变 `ModelSelect`。
- 能力矩阵不放 npm 包名、安装 URL、home 路径或账号识别算法；这些属于 adapter 数据。
- Chat 持续通道深度另见 [Chat 支持深度矩阵](chat-support-depth.md)。
