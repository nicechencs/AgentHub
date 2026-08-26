---
title: AgentHub 文档索引
type: navigation
status: current
owner: maintainers
updated: 2026-08-26
---

# AgentHub 文档

这里是开发、架构、产品决策和运维参考的统一入口。当前事实与未来提案分开维护；`archive/` 只保留历史上下文，不作为实现依据或待办来源。

## 第一次阅读

1. [开发环境与第一次运行](getting-started/development.md)
2. [当前实现状态](STATUS.md)
3. [架构总览](architecture/overview.md)
4. [贡献指南](../CONTRIBUTING.md)

## 按任务查找

### 开发操作

- [添加 Agent](guides/adding-an-agent.md)
- [添加 Adapter](guides/adding-an-adapter.md)
- [Adapter dogfood](guides/adapter-dogfood.md)
- [测试与验证](guides/testing-and-validation.md)
- [故障排查](guides/troubleshooting.md)

### 理解系统

- [架构总览](architecture/overview.md)
- [前端与 Backend 边界](architecture/frontend-backend.md)
- [Core 与运行时](architecture/core-runtime.md)
- [Adapter 路线内核](architecture/adapter-route-kernel.md)
- [Connections、Routes 与绑定](concepts/connections-and-routing.md)
- [Adapter 与本机路由](concepts/adapters-and-bridges.md)
- [账号与授权](concepts/accounts-and-authorization.md)
- [Chat 与 Agent](concepts/chat-and-agents.md)
- [插件、MCP 与技能](concepts/plugins-and-mcp.md)
- [对象化与封装审查](architecture/objectization-encapsulation-audit.md)
- [对象化与封装审查：前端与运行时](architecture/objectization-encapsulation-audit-frontend.md)
- [对象化与封装审查：Core 剩余分区](architecture/objectization-encapsulation-audit-core.md)
- [对象化与封装审查：OAuth](architecture/objectization-encapsulation-audit-oauth.md)
- [对象化与封装审查：CLI、Tauri 与工具链](architecture/objectization-encapsulation-audit-cli-tauri.md)
- [对象化与封装审查：测试、Mock 与 Fixture](architecture/objectization-encapsulation-audit-tests-fixtures.md)
- [读模型 owner 与兼容策略（提案）](architecture/read-model-owners.md)

### 查稳定契约

- [CLI 与配置](reference/cli-and-config.md)
- [能力矩阵](reference/capabilities.md)
- [本机路由 API](reference/local-route-api.md)
- [路由兼容性](reference/route-compatibility.md)
- [隐私与发布](reference/privacy-and-release.md)
- [日志](reference/logging.md)
- [测试](reference/testing.md)
- [术语表](reference/terminology.md)
- [MCP inventory](reference/mcp-inventory.md)
- [Agent 插件表面](reference/agent-plugin-surfaces.md)

### 产品与界面

- [产品边界与范围外决定](decisions/product-boundaries.md)
- [决策记录索引](decisions/README.md)
- [UI 设计系统](ui/design-system.md)
- [页面模式](ui/page-patterns.md)

## 未来与历史

- [提案索引](proposals/README.md)：尚未承诺实施的候选方向。
- [Adapter sidecar](proposals/adapter-sidecar.md)
- [本机同口授权池](proposals/unified-loopback-pool.md)
- [托盘后台模式](proposals/tray-background-modes.md)
- [模块化改进](proposals/modularity.md)
- [插件（extension / plugin）管理](proposals/plugin-management.md)
- [归档索引](archive/README.md)：不可作为当前契约。
- [单一内核与查表投影（历史）](archive/single-kernel-projections.md)
- [旧文档迁移索引](archive/legacy-document-index.md)：旧路径到新真源的完整映射。

## 文档治理

- [文档风格与维护规则](STYLE.md)
- [项目工程红线](../AGENTS.md)
- [安全策略](../SECURITY.md)

新增页面前先判断它是教程/指南、参考、解释、决策、提案还是归档。一个事实只保留一个现行真源，其他页面只写短摘要并链接过去。提交前运行 `pnpm check:docs`。
