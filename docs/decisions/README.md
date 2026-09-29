---
title: 决策索引
type: navigation
status: current
owner: maintainers
audience: all contributors
source-of-truth: project AGENTS.md, current code/contracts, and linked decision pages
updated: 2026-09-29
---

# 决策索引

本目录只放仍然有效的产品边界和架构决策。实施记录、一次性排期和已完成的迁移在 `docs/archive/`，不能从它们派生新任务。

## 当前决策

| 决策 | 结论 | 详见 |
| --- | --- | --- |
| 用户对象与接法 | 界面说登录、连接、路由；三种接法由 planner 给出 | [连接与路由](../concepts/connections-and-routing.md) |
| 登录生命周期 | 登录只有一套；路由页直接添加的官方登录 / API Key（`home=route_pool`）由路由页管理，可不出现在连接页 | [产品边界](product-boundaries.md) |
| 前端与后端 | 页面 → runtime → `#backend` → Tauri / mock；生产不静默回退 mock；只有 Tauri adapter 调 `invoke` | [Frontend/backend boundary](../architecture/frontend-backend.md) |
| 路线内核 | `AdapterRouteService::plan()` 是唯一决策出口；golden 只读；mock 只查表；未命中 fail-closed | [Adapter 路线内核](../architecture/adapter-route-kernel.md) |
| Core 形态 | 模块化单体；GUI / CLI 共享 core；平台能力按端口分区 | [Core and runtime](../architecture/core-runtime.md) |
| 本机转发（`local_bridge`） | 运行在 Tauri 进程内；默认每个 Agent / 端点一个同口授权池；sidecar 只是提案，未部署 | [Adapter 与本机路由](../concepts/adapters-and-bridges.md) |
| 写入入口 | 产品写入走 `plan` → `bind` / `unbind`；生成的配置不能再当登录 | [连接与路由](../concepts/connections-and-routing.md) |
| 账号池 | 官方登录按 Agent + 身份覆盖；Key 按指纹分行；每个 Agent 同时只有一份当前登录 | [Accounts and authorization](../concepts/accounts-and-authorization.md) |
| 凭据落盘加密 | 无必要，项目范围外；沿用现有存储 | [产品边界](product-boundaries.md) |
| 国产官方登录 | 不开 adapter 边，不转 API，不可分享；国产路由只认官方 API Key | [产品边界](product-boundaries.md) |
| API Key 分享 | 所有 API Key 都可入连接池并接到其他工具，不按所属 Agent 挡掉 | [产品边界](product-boundaries.md) |
| 插件与 MCP | 插件是各家 plugin / extension 包；MCP 是 `/mcp` 的 server 清单，部分 Agent 可写入；二者不混名 | [插件、MCP 与技能](../concepts/plugins-and-mcp.md) |

## 阅读规则

1. 先读 [架构总览](../architecture/overview.md)，再读领域概念；实现细节以代码和 contract 为准。
2. 写着「目标」「未来」「提案」的内容不是当前能力，尤其是 `agenthub-adapterd` sidecar。
3. `Ticket`、`Binding`、`Wallet` 是实现术语；界面统一说「登录」「连接」。
4. 分支与发布红线在根 [AGENTS.md](../../AGENTS.md)，本目录不复制。
