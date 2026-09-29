---
title: 前端与 Backend Adapter 边界
type: architecture
status: current
owner: maintainers
audience: frontend and integration contributors
source-of-truth: src/app/runtime, src/lib/backend/contracts, src/lib/backend/tauri, src/dev/mocks, vite.config.ts, and src/lib/backend/boundary-imports.test.ts
updated: 2026-09-29
---

# 前端与 Backend Adapter 边界

本页说明页面怎样拿到后端能力、构建时选哪套实现，以及前端共享状态由谁持有。

## 调用路径

```text
页面
  → src/lib/api（过渡层）或 backend port
  → app/runtime 的 getBackend()
  → #backend（构建时别名，经 src/lib/backend/current.ts 导出 createBackend）
  → Tauri 实现或浏览器 mock
```

Tauri 一侧：

```text
lib/backend/tauri/<port>.ts
  → lib/backend/tauri/invoke.ts
  → src-tauri command
  → agenthub-core service
```

边界规则由 `src/lib/backend/boundary-imports.test.ts` 锁住：

- 只有 `lib/backend/tauri/invoke.ts` 可以从 `@tauri-apps/api/core` 导入 `invoke`。
- 其他 `@tauri-apps/*` 导入（事件监听、版本、更新、自启动等）只能在 `lib/backend/tauri/` 下；唯一例外是 `lib/platform.ts` 用 `isTauri` 判断运行环境。
- 页面和 hook 不得导入 `lib/backend/tauri` 或 `src/dev`。

## 构建与测试选择

`#backend` 的指向在 `vite.config.ts`、`vitest.config.ts` 和 `tsconfig.*.json` 里配置。

| 命令 | `#backend` 实现 | 说明 |
| --- | --- | --- |
| `pnpm tauri:dev` | `src/lib/backend/tauri/create-backend.ts` | 启动真实桌面后端 |
| `pnpm dev` | 同上 | 只启动 Vite；浏览器里没有 Tauri，调用会报 unavailable |
| `pnpm build` | 同上 | 构建插件拒绝把 `src/dev`、`e2e/` 或测试文件打进产物 |
| `pnpm dev:mock` | `src/dev/mocks/create-backend.ts` | 纯浏览器演示，用 fixtures 和内存状态 |
| `pnpm test`（Vitest） | 同上 | 固定 mock；测试不得依赖 Tauri |
| `pnpm test:e2e:browser` | 同上 | Playwright 以 mock 模式启动 Vite，只测浏览器 DOM |

非 Tauri 环境下，`invoke.ts` 抛结构化 unavailable 错误。生产页面必须显示这个错误，不能静默切到 mock。mock 的路线结果只查 `plan()` 生成的快照，见 [Adapter 路线内核](adapter-route-kernel.md)。

## Contracts 与 port

`src/lib/backend/contracts/ports.ts` 定义 `Backend` 聚合接口，按领域分 port：`account`、`adapter`、`ticket`、`agent`、`catalog`、`config`、`backup`、`chat`、`env`、`project`、`provider`、`settings`、`skill`、`usage`、`dashboard`、`doctor`、`install`、`update`、`trash`、`mcp`、`plugins`。

- DTO 与 Rust 的 serde 形状一致。密钥字段只能是脱敏值，或不带明文的「引用动作」。
- 页面不按 Agent 名称或 API 地址自己推导路线。连接流程先调 `ticket.plan` 展示路线、成熟度、`canApply` 和原因，再由 `bind` / `unbind` 写入。前端入口是 `src/lib/api/tickets.ts` 的 `planTicket` / `bindTicket` / `unbindTicket`。概念见 [Connections、Routes 与绑定](../concepts/connections-and-routing.md)。

## 共享读模型

`src/app/runtime` 持有前端共享状态，但不拥有领域真相：

- `backend-runtime.ts`：`getBackend()` 惰性创建并保持一个 backend 实例；`setBackend` / `resetBackend` 换实例后调用 `runtime-context.ts` 的 `resetRuntimeContext()`，统一重置所有 store。
- `agent-catalog-store`：从 backend 加载 Agent 目录；失败时标记 error，不拿静态列表冒充成功。
- `agent-status-store`：Agent 安装状态，叠加登录与本机登录探测结果。
- `connection-inventory-store`：缓存全部已保存登录和供应商，去重并发请求，支持部分失败和强制刷新。它不是界面上的「连接池」，也不是 `ConnectionService` 的 current 指针。
- `ticket-wallet-store`：Ticket 钱包快照。Ticket 是代码里对「一份登录」的叫法，钱包列出每份登录及它已接到哪些 Agent；供总览、连接、路由和 Chat 共用。
- `app-update-store`：应用自身是否有更新。
- `mutation-coordinator.ts`：写入成功后调用 `refreshRuntimeReadModels` 刷新相关 store；刷新失败只记在各 store 的 error 上，不影响写入结果。

页面通过 hook 或 `lib/api` 订阅这些 store。

## 错误与降级

mock 与 Tauri 实现必须满足同一份 port 契约。配置写入、Agent 能力、本机转发控制等后端不可用时，页面显示对应状态并允许重试；不解析旧格式后继续写入，也不把「加载失败」当成空列表。

## 相关页面

- [架构总览](overview.md)
- [Core 与 Runtime](core-runtime.md)
- [Adapter 路线内核](adapter-route-kernel.md)
- [Adapters 与本机 Bridge](../concepts/adapters-and-bridges.md)
- [测试参考](../reference/testing.md)
