---
title: 术语表
description: AgentHub 用户界面、领域模型和内部实现术语的对应关系。
type: reference
audience: all
status: current
updated: 2026-08-26
---

# 术语表

用户文案和代码名有意分开。新增文档、页面和错误消息优先使用“用户界面术语”。

| 用户界面术语 | 内部/代码术语 | 含义 |
|---|---|---|
| Agent | `AgentId` / `AgentKey` / `AgentAdapter` | 被 AgentHub 检测、安装、配置或运行的第三方 CLI/runtime |
| 登录 | account / credential / Ticket（内部） | 用户可选择的授权或 API key 记录；实现中仍可能出现 Ticket，但 UI 不说“票” |
| 供应商 | provider | 某 Agent 的 API 端点、模型和相关配置记录 |
| 路由 / Routes | adapter profile / route | Connections 之外用于管理本机转发 listener 的产品表面 |
| bridge | `local_bridge` / in-process Gateway | Routes 的内部协议转换实现；不作为普通用户功能名 |
| 直接配置 | `native_endpoint` | 将来源端点写入目标 Agent 自己的配置 |
| 写进对方认的登录 | `config_sync` | 将登录投影到目标 Agent 能识别的配置/登录契约 |
| 本机转发 | `local_bridge` | 通过 loopback listener 转发或转换协议 |
| 能力 | `Capability` | adapter 对某操作的静态声明，不等于安装状态 |
| 完整 | `Full` | 能力已经接入且契约完整 |
| 部分支持 | `Partial` | 可用但有降级，必须提示 |
| 不支持 | `Unsupported` | 对方契约不存在或明确不支持 |
| 计划中 | `Planned` | AgentHub 尚未接入，不得假装可用 |
| live 配置 | live files | 第三方 Agent 实际读取的配置/登录文件 |
| 数据目录 | `data_dir` / `AGENTHUB_HOME` | AgentHub 自己的 SQLite、备份、日志和缓存根目录 |
| 共享技能真源 | `~/.agents/skills` | 跨 Agent 的技能源；各 Agent 可有自己的投影目录 |
| 插件 / MCP | MCP server 条目（当前只读 inventory）；厂商 Plugin 包未接线 | 页面可称插件；不要与 Skills 或某家 `/plugin` 市场混成同一模块 |
| mock backend | `src/dev/mocks` | 仅供 `pnpm dev:mock` 和 Vitest 使用的浏览器实现 |
| Tauri adapter | `src/lib/backend/tauri` | 生产桌面 backend；唯一允许直接 `invoke` 的前端边界 |
| fixture | test fixture | 脱敏、固定、最小的测试输入，不是用户数据备份 |

## 书写规则

- 页面、菜单和用户提示用“登录”“Routes/路由”“供应商”。
- 架构和代码说明可以写 `Ticket`、`Binding`、`bridge`，但首次出现应解释它们与用户术语的关系。
- `local_bridge` 不等于所有 adapter；它只是三种 route 之一。
- `Capability::ModelSelect` 不等于 Routes 的 `/v1/models`；后者是本机 Route 的只读映射列表。
- 凭据落盘沿用当前实现方案；不要在文档中引入 keyring、AES、主密码或国产 OAuth 转 API 计划。

