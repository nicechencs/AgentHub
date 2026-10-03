---
title: 模块化与边界收紧
type: proposal
status: proposed
owner: maintainers
updated: 2026-10-03
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

Go 路由替换：只按[Go 路由替换方案](adapter-sidecar.md)逐个纵向切片实施，不从本文派生通用扩展框架或重复任务。

## 6. 任务验收

- 移动 symbol 前确认调用方和公开兼容面。
- 跨文件或并行任务明确负责人和范围；局部改动不额外增加流程。
- 拆 service 或 hook 前写清要保持的行为、事务和锁语义。
- 删除兼容路径前补足与风险匹配的检查。
- 先跑针对性测试，边界变化大再跑 typecheck/build。

## 7. 非目标

全目录重写、微服务、把 Connections/Accounts/Providers 拆成独立进程、凭据落盘加密、国产 OAuth Adapter、OAuth 转 API。

## 8. 功能模块与 Go 路由程序

`/plugins` 页面继续只管理各 Agent 的厂商 plugin / extension 包。Go 路由程序是随 AgentHub 打包的内部运行组件，不建立另一套插件概念；厂商插件见[插件管理提案](plugin-management.md)。

当前本机转发仍是 Tauri 进程内 `local_bridge`。树中已有隔离目录下的 Go Messages 切片（`go/agenthub-adapterd`、`scripts/route-runtime-probe/messages-isolated.sh`），但它还不是默认网关。

路由只解决自身的进程替换。登录、连接、会话、历史、用量、技能、MCP、各 Agent 对接、数据迁移和更新继续留在现有模块中，不为它们设计可下载扩展或独立进程。

Go 路由只依赖现有 core 给出的运行配置和登录信息；它不拥有数据库、Agent 配置或产品支持范围。具体替换顺序见[Go 路由替换方案](adapter-sidecar.md)。

如果以后真的出现第二个需要独立进程的组件，再根据两者已经验证的共同需求决定是否抽取复用接口；本方案不提前定义注册中心、插件基类或独立更新机制。

本节非目标：动态插件 ABI、插件商店 / SDK、第二套领域数据库、事件总线。不把 `/plugins` 管理的厂商包接到 Go 路由程序。

是否改善以运行结果判断：Go 路由故障时其他页面仍可用，替换运行程序不要求重写登录和用户数据。当前仍是提案，不宣称默认网关已经切换。
