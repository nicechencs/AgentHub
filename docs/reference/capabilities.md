---
title: 能力参考
description: AgentHub 能力键、四级状态和当前 Agent 能力快照。
type: reference
audience: contributor
status: current
updated: 2026-08-29
---

# 能力参考

能力回答“该 Agent 能否安全执行某类操作”，不是安装状态、配置参数或模型目录。真源是 adapter 的 `capability()`；CLI 可生成快照：

```text
agenthub agent capabilities --markdown
```

## 状态

| 状态 | 含义 | 调用方行为 |
|---|---|---|
| `Full` | 已接入且契约完整 | 正常开放 |
| `Partial` | 可用但有已知降级 | 开放并显示原因 |
| `Unsupported` | 对方契约不存在或明确不支持 | fail-closed |
| `Planned` | AgentHub 尚未接入 | fail-closed，并标明是路线图 |

非 `Full` 必须有 `reason`。静态能力与 detect 的安装/版本状态不可合并。

## 能力键

`ConfigWrite`、`AccountSwitch`、`ApiKeyAccount`、`Skills`、`LiveBackup`、`StructuredStream`、`DangerousMode`、`ProjectHistory`、`ProjectDelete`、`ProviderPresets`、`Usage`、`Mcp`、`ModelSelect`、`SessionResume`。

## 当前快照

下表是仓库当前 adapter 声明的摘要；变更后以 CLI 输出和 Rust 源码为准。

| 能力 | claude | codex | kimi | grok | pi | workbuddy | cursor | dsh | zcode |
|---|---|---|---|---|---|---|---|---|---|
| ConfigWrite | Full | Full | Full | Full | Full | Partial | Unsupported | Partial | Partial |
| AccountSwitch | Full | Full | Full | Full | Full | Partial | Unsupported | Partial | Partial |
| ApiKeyAccount | Full | Partial | Full | Full | Partial | Full | Partial | Full | Full |
| Skills | Full | Full | Unsupported | Full | Full | Full | Full | Full | Full |
| LiveBackup | Full | Full | Full | Full | Full | Full | Unsupported | Full | Full |
| StructuredStream | Full | Full | Full | Full | Full | Unsupported | Unsupported | Planned | Unsupported |
| DangerousMode | Full | Full | Partial | Full | Partial | Full | Full | Partial | Unsupported |
| ProjectHistory | Full | Full | Full | Full | Full | Full | Partial | Full | Partial |
| ProjectDelete | Full | Full | Full | Full | Full | Full | Unsupported | Partial | Planned |
| ProviderPresets | Full | Full | Full | Full | Unsupported | Unsupported | Unsupported | Partial | Unsupported |
| Usage | Full | Full | Full | Full | Full | Full | Unsupported | Full | Full |
| Mcp | Planned | Planned | Planned | Planned | Planned | Planned | Planned | Planned | Planned |
| ModelSelect | Planned | Planned | Planned | Planned | Planned | Planned | Planned | Planned | Planned |
| SessionResume | Partial | Partial | Planned | Planned | Planned | Planned | Planned | Planned | Planned |

Cursor 的 `ConfigWrite` / `AccountSwitch` 为 Unsupported：没有稳定的本机登录文件可写。界面切换失败时给出中文说明，不静默。**store-stamp 默认软隐藏 Cursor Agent**（见 [STATUS](../STATUS.md)）。WorkBuddy / ZCode 本机安装只打开官网，不当成脚本安装失败。占用方式：WorkBuddy / ZCode 是目录追加（只动对应那一行），Pi / DSH 是具名槽，其余默认独占。ZCode API Key 按目录追加写入 `~/.zcode/v2/config.json` 的一条供应商（官方槽或自定义行），不替换其它条目；套餐登录不导入。ZCode Projects 可列出任务并预览对话，不删除。WorkBuddy 自定义模型按 `models.json` 一行一份登录追加，只写 `/v1/chat/completions`；若地址是 DeepSeek 官方 `/chat/completions`，写入时会改成 `/v1/chat/completions`。桌面套餐登录不导入。

能力矩阵不承载 npm 包名、安装 URL、home 路径或账号识别算法；这些是 adapter/port 数据。只读 MCP inventory 也不等于 `Mcp` 管理能力，更不等于厂商 plugin/extension 包。本机 Routes 的 models endpoint 也不改变 `ModelSelect` 状态。MCP 扫描见 [MCP inventory](mcp-inventory.md)；各家插件包与 MCP 表面见 [Agent 插件表面](agent-plugin-surfaces.md)；插件页仍是 [提案](../proposals/plugin-management.md)。

