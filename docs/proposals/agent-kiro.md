---
title: 接入 Kiro（kiro-cli）Agent
type: proposal
status: proposed
owner: maintainers
updated: 2026-09-29
audience: contributor
---

# 接入 Kiro（kiro-cli）Agent

> 提案，不是现行契约。主体已落地，只剩下文列出的边界。现行行为见 [STATUS](../STATUS.md) 与 [capabilities](../reference/capabilities.md)。接线纪律见 [添加 Agent](../guides/adding-an-agent.md)。

## 当前基线

| 状态 | 内容 | 证据 |
| --- | --- | --- |
| 已落地 | Agent id `kiro`，只认 `kiro-cli`（不把 Kiro IDE 当成已安装）；检测、安装、登录指引、API Key | `crates/agenthub-core/src/integrations/agents/kiro/`（`install.rs`、`paths.rs`）；`adapters/kiro.rs` |
| 已落地 | 新对话走 `kiro-cli acp`：同进程续聊、允许/拒绝、停止；生成时不能中途补充 | [STATUS](../STATUS.md) Chat 段 |
| 已落地 | 旧打印对话保留 headless / HTTP 多轮 | 见 [Kiro HTTP](agent-kiro-http.md) |
| 已落地 | 项目历史、用量、列模型、对话标题 | `integrations/agents/kiro/{project,usage,session_title}.rs`；capabilities：ProjectHistory / Usage Partial，ModelSelect Full |
| 已落地 | Kiro 登录经本机路由接到 Claude / Codex / Grok | 见 [Kiro HTTP](agent-kiro-http.md) |
| 未做 | Skills、MCP | capabilities 为 Planned；`adapters/kiro.rs` 中 `Mcp => planned("待路径核实")` |

早期方案里的「一轮一发」「不接持续通道」「本机路由后置」已过期，不是待办。

## 剩余目标

- Skills / MCP：确认 `.kiro/` 与 `~/.kiro/` 的真实布局后再定能力等级。项目内 `.kiro/steering/`、`.kiro/agents/*.yaml`（Kiro 自己的 agent，不是 AgentHub 的 Agent）、`.kiro/hooks/` 最多只读。
- 企业 IdC / `profileArn` 实机验收、打印路径流式：见 [Kiro HTTP](agent-kiro-http.md)。

## 门槛

- Skills / MCP 有真实路径证据和 round-trip 测试后，才从 Planned 升级。
- `ConfigWrite` 保持 Unsupported，直到有稳定 round-trip。

## 非目标

- 嵌入 Kiro IDE、Web、Mobile 或 Crew。
- 对话页宣称支持 `kiro-cli` 的终端补全（Autocomplete 下拉、Inline 灰色提示）。那是终端能力，未嵌终端就不写。
- 假的 `/model`、`/agent` 选择器。
- 凭据落盘加密、国产 OAuth、官方登录转 API Key。

## 未决问题

- CLI 2.x 与 3.0（`--v3`）会话格式不兼容，AL2 不支持 3.0。是否按版本降级能力，需实测后定。
- 官方文档提到 `~/.kiro/sessions/`（v3），写入前要本机核实。

## 参考

- 官方：[CLI](https://kiro.dev/docs/cli/)、[Headless](https://kiro.dev/docs/cli/headless/)、[安装](https://kiro.dev/docs/getting-started/installation/)、[登录](https://kiro.dev/docs/getting-started/authentication/)
- [Kiro HTTP / 本机转发](agent-kiro-http.md)
- [Claude Chat B3](../archive/chat-claude-b3.md)（「没有真实通道就不做假按钮」先例）
