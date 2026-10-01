---
title: 模块化与边界收紧
type: proposal
status: proposed
owner: maintainers
updated: 2026-09-29
---

# 模块化与边界收紧

> 状态：提案。只记录已建立的边界和下一步可做的最小改造，不授权大范围目录重写。

## 1. 当前基线

AgentHub 是模块化单体：GUI 和 CLI 共用 `agenthub-core`。已建立的边界：

- Agent 注册表和运行时 Agent Catalog 是 Agent 顺序、能力、安装渠道的唯一来源；前端没有第二份列表。
- 产品写入走 `plan` / `bind` / `unbind`；旧 adapter apply 只作兼容，页面不调用。
- `ConnectionService` 负责 active/current；`is_current` 只是兼容镜像。`TicketBinding` 与 `ActiveBinding` 含义不同。
- `local_bridge` 由进程内 `DesktopAdapterControl` 托管，控制契约与 Tauri 解耦。
- 只有 `src/lib/backend/tauri/invoke.ts` 调用 `invoke`；生产构建不带 mock，非 Tauri 页面显示 unavailable。
- Rust 路由测试与 browser mock 共用 `src/dev/mocks/fixtures/adapter-capability-contract.json`，其 `expect` 由 `AdapterRouteService::plan()` 生成，漂移即失败。现行契约见 [Adapter 路线内核](../architecture/adapter-route-kernel.md)。
- Provider / Account / Backup 服务已按内部子模块拆开（`services/provider_service/`、`account_service/`、`backup_service/`），公开方法不变。设计见 [Service 内部 owner 拆分](../archive/service-internal-owners.md)。
- 前端 `src/lib/backend/contracts/` 已有按领域的 port、mapper 和 wire 类型（如 `adapter-wire.ts`、`provider-map.ts`），各自带测试。

仍存在的问题：各 port 缺统一的契约测试；wire 边界只在部分领域明确；部分页面和 Chat 大 hook 职责偏多；`src/lib/api` 兼容 façade 偏宽。

## 2. 持续约束

1. 保持模块化单体，不引入微服务、DDD/CQRS 套件、事件总线或动态插件 ABI。
2. 每条产品规则只有一处来源，由契约测试保护；mock 和页面不维护第二份规则表。
3. 领域逻辑在 core；Tauri 和 CLI 只做入口、传输、展示。
4. 删除兼容入口前，先迁移调用方和测试。
5. 前端调用路径以 [前后端边界](../architecture/frontend-backend.md) 为准。
6. 页面只做编排；纯逻辑放本功能的 `*-model` / `*-format` / hook。
7. 文件大只是调查信号，不是拆分理由。

## 3. 已落地

| 项 | 证据 |
|---|---|
| C1 路由规则契约 | `adapter_route_service/tests.rs` 中 `shared_capability_contract_rule_ids_match_matrix` 等；`src/dev/mocks/adapter.test.ts` |
| F1 Skills 页面局部逻辑 | `src/pages/skills/skills-library-model.ts` |
| F2 Projects 页面局部逻辑 | `src/pages/projects/projects-list-model.ts` |
| D3 服务内部拆分 | 见基线；门面方法名未变 |
| D6 单一内核 | 已关闭；历史见 [单一内核提案归档](../archive/single-kernel-projections.md)。不拆 crate、不上 sccache |

不要从这些项再派生新的抽取任务。

## 4. 待设计（只能派调查，不能直接派开发）

- **D1 共享 port 契约测试：** 同一组用例同时跑 mock 与 Tauri 实现（成功映射、结构化错误、unavailable、写后刷新、事件清理）。测试设施不进生产模块图。进入开发的条件：选定试点 port，写清文件、测试和回滚边界。
- **D2 稳定 wire DTO：** 列出仍直接序列化 core model 的 command，每次只做一个领域（`adapter-wire.ts` 可作样板）。Tauri 不维护第二套领域模型。
- **D4 `src/lib/api` 分类：** 把导出分为纯转发、DTO 映射、允许的 runtime/cache 协调、废弃四类。分类阶段不删导出。
- **D5 收窄 `AgentAdapter`：** 找出只为兼容保留的方法，逐组迁到已有 port。不宣称已有动态插件 ABI，不改 Agent 行为。

## 5. 延期

Sidecar 与统一控制客户端：只按 [Local Route Sidecar](adapter-sidecar.md) 推进，不从本文派生任务。

## 6. 任务验收

- 移动 symbol 前确认调用方和公开兼容面。
- 每个任务一个负责人、明确文件范围；并行任务不改同一文件。
- 拆 service 或 hook 前写清要保持的行为、事务和锁语义。
- 删除兼容路径前先补契约测试。
- 先跑针对性测试，边界变化大再跑 typecheck/build。

## 7. 非目标

全目录重写、微服务、把 Connections/Accounts/Providers 拆成独立进程、凭据落盘加密、国产 OAuth Adapter、OAuth 转 API。
