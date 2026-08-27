---
title: Codex 安装与连接/路由模块化审查
type: status
status: current
owner: maintainers
audience: product, frontend, and core contributors
source-of-truth: dev branch review on 2026-08-27; fixes on branch cursor/codex-modularity-fixes-85e8
updated: 2026-08-27
---

# Codex 安装与连接/路由模块化审查

本页记录 2026-08-27 对 `dev` 分支的深度审查结论与修复状态。每项可在源码、测试或文档中核实。

## A. Codex 安装

| ID | 问题 | 优先级 | 状态 | 修复说明 |
| --- | --- | --- | --- | --- |
| A1 | macOS/Linux 仅暴露 npm 渠道（OpenAI 无 Unix native 脚本） | P1 | **已记录** | 产品/上游限制；见 [排障指南](../guides/troubleshooting.md#codex-安装) 与 [Chat 与 Agent](../concepts/chat-and-agents.md#codex-外部安装) |
| A2 | npm 渠道依赖 Node.js + npm，环境未就绪时安装失败 | P0 | **已缓解** | 排障文档区分 `env.not_ready`；Agents 页已有环境面板与一键装环境 |
| A3 | 安装命令成功但重检找不到二进制（PATH / npm prefix） | P0 | **已修复** | `detect_binary` 增加 `npm prefix -g` 动态探测；排障文档补充重启说明 |
| A4 | 全局 npm 权限不足（EACCES） | P2 | **已记录** | 安装服务已分类；排障文档补充权限 remediation |
| A5 | Agent 软隐藏被误判为「未安装」 | P2 | **已缓解** | Agents 卡已有「已隐藏」Badge；排障文档说明 `agent_visibility.json` |
| A6 | Codex 并非 Adapter/安装计划缺失（常见误解） | — | **已澄清** | 文档明确已注册；排查从 `doctor` 开始 |

## B. 外部渠道 Codex → Chat

| ID | 问题 | 优先级 | 状态 | 修复说明 |
| --- | --- | --- | --- | --- |
| B1 | IDE/桌面安装不在 PATH，依赖 extra copy 扫描 | — | **能力** | `codex_copies.rs` + `promote_spawnable_extra_copy`；非缺陷 |
| B2 | Chat 只 spawn CLI，不调 VS Code 扩展 API | P1 | **已记录** | [Chat 与 Agent](../concepts/chat-and-agents.md#codex-外部安装) 写明架构决策 |
| B3 | `~/.codex/auth.json` 无效时 Chat 不可选 | P1 | **已记录** | 排障文档：先完成登录再 Chat |
| B4 | 隐藏 Agent 不进 Chat 选器 | P2 | **预期** | 同 A5 |
| B5 | 未选工作目录阻断发送 | P2 | **预期** | Chat 产品规则；文档已提及 cwd 要求 |
| B6 | 非常见安装路径可能扫描不到 | P3 | **已记录** | 建议 `doctor` 诊断；无手动路径配置（范围外） |

## C. 连接 / 路由页面模块化

| ID | 问题 | 优先级 | 状态 | 修复说明 |
| --- | --- | --- | --- | --- |
| C1 | Dashboard/Agents 反向依赖 `pages/connections/ticket-wallet-model` | P1 | **已修复** | 共享标签/菜单 helper 迁至 `src/lib/ticket-wallet-labels.ts`、`src/lib/menu-dialog-arm.ts` |
| C2 | 连接页嵌入 `pages/accounts`、`pages/providers` Dialog | P1 | **已修复** | Dialog 迁至 `src/components/connections/`；旧路径保留 re-export |
| C3 | `isLeftoverLocalRouteProvider` 定义在 Chat 页 | P1 | **已修复** | 迁至 `src/lib/leftover-local-route.ts`；Chat 页 re-export |
| C4 | plan fan-out 在 Connections 与 ConnectFlow 重复 | P2 | **已缓解** | 两处均通过 `createPlanFanout`；共享 deps 在 `lib/connect-flow/default-deps.ts` |
| C5 | 路由页 `index.tsx` 编排过重 | P2 | **部分** | wallet 读模型并入 `useAdapterResources`；其余 dialog 状态仍 inline |
| C6 | `ticket-wallet-model.ts` 过大（~1024 行） | P2 | **部分** | 抽出跨页共享模块；Connections 专用逻辑仍留页内 |
| C7 | 路由页注释与创建 bind 行为不一致 | P2 | **已修复** | 注释更新为「运行时为主，创建/导入为产品例外」 |
| C8 | 路由页单独 `listTicketWallet` 做孤儿分区 | P2 | **已修复** | 改用 `useTicketWallet` 共享 store（经 `useAdapterResources`） |
| C9 | 路由页 `planAdapter` 与 ConnectFlow `planTicket` 不统一 | P2 | **已修复** | native enroll 预览改用 `planTicket` + `ticketIdFor` |

## 验证

修复合并前至少运行：

```bash
pnpm typecheck
pnpm exec vitest run src/lib/leftover-local-route.test.ts src/pages/chat/chat-model.test.ts src/pages/connections/ticket-wallet-model.test.ts src/pages/bridges/create-route-flow.test.ts src/pages/dashboard/dashboard-layout.test.ts
cargo test -p agenthub-core detect_binary --locked
pnpm check:docs
```

## 相关文档

- [排障指南](../guides/troubleshooting.md)
- [Chat 与 Agent](../concepts/chat-and-agents.md)
- [Connections、Routes 与绑定](../concepts/connections-and-routing.md)
- [当前实现状态](../STATUS.md)
