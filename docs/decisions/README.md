---
title: 决策索引
type: navigation
status: current
owner: maintainers
audience: all contributors
source-of-truth: project AGENTS.md, current code/contracts, and linked decision pages
updated: 2026-08-26
---

# 决策索引

本目录只放仍然有效的产品边界和架构决策。实施记录、一次性排期和已完成的迁移留在旧文档或 `docs/archive/`，不能从它们派生新任务。

## 当前决策

| 决策 | 结论 | 详见 |
| --- | --- | --- |
| 用户对象与路线 | UI 说登录、Connections、Routes；三种接法由 planner 派生 | [Connections and routing](../concepts/connections-and-routing.md) |
| Frontend backend | 页面 → runtime → `#backend` → Tauri/mock；生产不静默 mock；仅 Tauri adapter 调 `invoke` | [Frontend/backend boundary](../architecture/frontend-backend.md) |
| Adapter 路线内核 | `AdapterRouteService::plan()` 唯一决策；golden 只读投影；mock 只查表；未命中 fail-closed | [Adapter 路线内核](../architecture/adapter-route-kernel.md) |
| Core 形态 | 模块化单体；GUI/CLI 共享 core；平台能力按端口分区 | [Core and runtime](../architecture/core-runtime.md) |
| local_bridge | 当前 Tauri 进程内；sidecar 是迁移提案，未部署 | [Adapters and bridges](../concepts/adapters-and-bridges.md) |
| 写入入口 | 产品写入走 `plan` → `bind` / `unbind`；生成配置不能再当登录 | [Connections and routing](../concepts/connections-and-routing.md) |
| 账号池 | OAuth 按 Agent + identity 覆盖；Key 按指纹分行；每 Agent 只有一个 live current | [Accounts and authorization](../concepts/accounts-and-authorization.md) |
| 凭据落盘加密 | 无必要，项目范围外；沿用现有存储方案 | [Product boundaries](product-boundaries.md) |
| 国产 OAuth | 不开 adapter 边，不转 API；国产路由只认支持的 API Key | [Product boundaries](product-boundaries.md) |
| 插件 vs MCP | 插件是各家 extension/plugin 包；MCP 是 `/mcp` 只读 server 清单。二者不得混名 | [插件、MCP 与技能](../concepts/plugins-and-mcp.md) |

## 阅读规则

1. 先读架构总览，再读领域概念；实现细节以代码和 contract 为准。
2. 看到“目标”“未来”“提案”时，不要当作当前能力；尤其是 `agenthub-adapterd` sidecar。
3. 看到 `Ticket`、`Binding`、`Wallet` 时，按实现术语理解；产品表面统一用“登录”与“连接”。
4. `dev`/`release` 分支与发布红线属于项目根 [AGENTS.md](../../AGENTS.md)；本目录只链接，不复制发布流程。

## 相关架构与概念

- [Architecture overview](../architecture/overview.md)
- [Frontend/backend boundary](../architecture/frontend-backend.md)
- [Adapter 路线内核](../architecture/adapter-route-kernel.md)
- [Core/runtime](../architecture/core-runtime.md)
- [Connections/routing](../concepts/connections-and-routing.md)
- [Adapter 与本机路由](../concepts/adapters-and-bridges.md)
- [Accounts/authorization](../concepts/accounts-and-authorization.md)
- [Chat/agents](../concepts/chat-and-agents.md)
