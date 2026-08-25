---
title: 前端与 Backend Adapter 边界
type: architecture
status: current
owner: maintainers
audience: frontend and integration contributors
source-of-truth: src/app/runtime, src/lib/backend/contracts, src/lib/backend/tauri, src/dev/mocks, and vite.config.ts
updated: 2026-08-25
---

# 前端与 Backend Adapter 边界

## 调用路径

所有页面能力都遵循这条路径：

```text
页面
  → lib/api（兼容 façade，可渐进迁移）或 backend port
  → app/runtime.getBackend()
  → #backend（构建时 alias）
  → Tauri adapter 或 browser mock
```

Tauri 路径继续是：

```text
lib/backend/tauri/<port>.ts
  → lib/backend/tauri/invoke.ts
  → Tauri command
  → src-tauri command
  → agenthub-core service
```

`lib/backend/tauri/invoke.ts` 是唯一允许导入 `@tauri-apps/api` 并调用 `invoke` 的位置。`contracts`、`lib/api`、页面和 mock 不得直接调用 `invoke`。

## 构建与测试选择

| 命令/场景 | `#backend` 实现 | 语义 |
| --- | --- | --- |
| `pnpm dev`、`tauri dev`、`pnpm tauri:dev` | `src/lib/backend/tauri/create-backend.ts` | 选择 Tauri adapter；`tauri dev` 启动桌面壳，单独运行 `pnpm dev` 时若没有 Tauri host 则明确 unavailable |
| `pnpm dev:mock` | `src/dev/mocks/create-backend.ts` | 纯浏览器交互，使用 fixtures 和 mock 状态 |
| `pnpm test` / Vitest | `src/dev/mocks/create-backend.ts` | 固定 mock backend；测试不得依赖 Tauri |
| `pnpm build` | Tauri adapter | 生产构建不得把 `src/dev`、测试文件或 mock 实现打入产物 |

生产页面在非 Tauri 环境中必须报告 unavailable 或明确错误，不能为了“让页面能用”静默切到 mock。mock 只服务 `dev:mock` 和测试。

## Contracts 与 port

`src/lib/backend/contracts` 定义 Backend 聚合接口和按领域拆分的 port，例如 `account`、`provider`、`ticket`、`adapter`、`chat`、`config`、`install`、`catalog`、`project`、`skill` 和 `usage`。DTO 使用前后端稳定的 wire 形状；secret 字段只能以脱敏值或不可序列化的“引用动作”表达。

页面不应根据 Agent 名称或 API URL 自己推导路线。连接流程应调用 backend 的 `ticket.plan`，展示 `route`、成熟度、`canApply` 和原因，再由唯一写入口执行 `bind`/`unbind`。具体概念见 [connections-and-routing](../concepts/connections-and-routing.md)。

## Runtime store

`app/runtime` 负责组合和可观察的共享状态，不拥有领域真相：

- `getBackend()` 保持一个当前 backend 实例；测试可替换并 reset store。
- connection pool store 缓存全量 accounts/providers，支持并发去重、partial/error 和强制刷新；它不是 `ConnectionService` 的 current 指针。
- catalog store 从 backend 加载 Agent 描述；加载失败时清空/标记 error，不恢复静态列表冒充成功。
- 页面通过 hook 或 façade 订阅这些 store，写操作完成后刷新或等待 backend 返回的领域结果。

## 错误与降级

Tauri adapter 对非桌面运行时抛出结构化 unavailable 错误；mock 和 Tauri 应保持相同的 port 契约。配置 projector、Agent capability、local bridge control 等后端不可用时，页面显示对应状态并允许重试，不解析旧格式后继续写入，也不把“加载失败”当成空列表。

## 相关页面

- [Architecture overview](overview.md)
- [Core and runtime](core-runtime.md)
- [Adapters and bridges](../concepts/adapters-and-bridges.md)
- [Testing reference](../reference/testing.md)
