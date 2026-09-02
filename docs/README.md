---
title: AgentHub 文档索引
type: navigation
status: current
owner: maintainers
updated: 2026-08-31
---

# AgentHub 文档

按任务打开一页。不要把本页当必读清单，也不要把审查稿、提案或 `archive/` 当成当前实现。

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
- [release 分支保护](guides/release-branch-protection.md)
- [故障排查](guides/troubleshooting.md)

### 理解系统

现行说明，不要和下面的审查稿、提案混读。

- [架构总览](architecture/overview.md)
- [前端与 Backend 边界](architecture/frontend-backend.md)
- [Core 与运行时](architecture/core-runtime.md)
- [Adapter 路线内核](architecture/adapter-route-kernel.md)
- [Connections、Routes 与绑定](concepts/connections-and-routing.md)
- [Adapter 与本机路由](concepts/adapters-and-bridges.md)
- [账号与授权](concepts/accounts-and-authorization.md)
- [Chat 与 Agent](concepts/chat-and-agents.md)
- [插件、MCP 与技能](concepts/plugins-and-mcp.md)

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

## 审查与提案

不是现行契约。实现对应条目之前不要当架构说明阅读；完整列表见 [提案索引](proposals/README.md)。

- [对象化与封装审查](architecture/objectization-encapsulation-audit.md)（审查记录；分册从该页链出）
- [Codex 安装与连接/路由模块化审查](status/codex-install-modularity-review.md)

## 未来与历史

- [提案索引](proposals/README.md)：尚未承诺实施的候选方向。
- [归档索引](archive/README.md)：不可作为当前契约。已落地的同口授权池设计稿在 [本机同口授权池（归档）](archive/unified-loopback-pool.md)。
- [旧文档迁移索引](archive/legacy-document-index.md)：旧路径到新真源的完整映射。

## 文档治理

- [文档风格与维护规则](STYLE.md)
- [项目工程红线](../AGENTS.md)
- [安全策略](../SECURITY.md)

新增页面前先判断它是教程/指南、参考、解释、决策、提案还是归档。一个事实只保留一个现行真源，其他页面只写短摘要并链接过去。提交前运行 `pnpm check:docs`。
