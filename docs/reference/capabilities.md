---
title: 能力参考
description: AgentHub 能力键、四级状态和当前 Agent 能力快照。
type: reference
audience: contributor
status: current
updated: 2026-08-25
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

| 能力 | claude | codex | kimi | grok | pi | workbuddy | cursor | dsh |
|---|---|---|---|---|---|---|---|---|
| ConfigWrite | Full | Full | Full | Full | Full | Full | Unsupported | Partial |
| AccountSwitch | Full | Full | Full | Full | Full | Unsupported | Unsupported | Partial |
| ApiKeyAccount | Full | Partial | Full | Full | Partial | Unsupported | Partial | Full |
| Skills | Full | Full | Unsupported | Full | Full | Full | Full | Full |
| LiveBackup | Full | Full | Full | Full | Full | Full | Unsupported | Full |
| StructuredStream | Full | Full | Full | Full | Full | Unsupported | Unsupported | Planned |
| DangerousMode | Full | Full | Partial | Full | Partial | Full | Full | Partial |
| ProjectHistory | Full | Full | Full | Full | Full | Full | Partial | Full |
| ProjectDelete | Full | Full | Full | Full | Full | Full | Unsupported | Partial |
| ProviderPresets | Full | Full | Full | Full | Unsupported | Unsupported | Unsupported | Partial |
| Usage | Full | Full | Full | Full | Full | Full | Unsupported | Full |
| Mcp | Planned | Planned | Planned | Planned | Planned | Planned | Planned | Planned |
| ModelSelect | Planned | Planned | Planned | Planned | Planned | Planned | Planned | Planned |
| SessionResume | Partial | Partial | Planned | Planned | Planned | Planned | Planned | Planned |

能力矩阵不承载 npm 包名、安装 URL、home 路径或账号识别算法；这些是 adapter/port 数据。只读 MCP inventory 也不等于 `Mcp` 管理能力，本机 Routes 的 models endpoint 也不改变 `ModelSelect` 状态。

