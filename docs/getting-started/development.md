---
title: 开发环境
description: 在本地启动 AgentHub、选择运行方式，并按改动风险运行验证。
type: getting-started
audience: contributor
status: current
updated: 2026-09-29
---

# 开发环境

本文面向第一次在仓库里开发的人。仓库是 Cargo 工作区：业务代码在 `crates/agenthub-core`，CLI 在 `crates/agenthub-cli`，桌面应用在 `src-tauri`（crate 名 `agenthub-gui`）；React 页面在 `src/`，只通过 backend contract 访问后端。

## 前置条件

- Node.js 与 pnpm。
- Rust stable、Cargo，以及 Tauri 在当前操作系统上的构建依赖。
- Git。

## 安装并启动

1. 在仓库根目录安装依赖：

   ```text
   pnpm install
   ```

2. 按需要选一种运行方式：

   | 命令 | 运行形态 | backend |
   |---|---|---|
   | `pnpm tauri:dev` | 桌面开发窗口 | 真实 Tauri 后端 |
   | `pnpm dev:mock` | 浏览器演示 | 浏览器 mock，仅用于演示和页面开发 |
   | `pnpm dev` | 只起 Vite（`http://127.0.0.1:5173`，端口被占用会直接失败） | Tauri adapter；在普通浏览器里调用后端会显示 unavailable |

   macOS / Linux 也可以用 `pnpm dev:macos` / `pnpm dev:linux`（即 `./run.sh`）。

结果：`pnpm tauri:dev` 打开桌面窗口，页面能读到本机 Agent；`pnpm dev:mock` 在浏览器里显示演示数据。

## 最小工作流

1. 用 `pnpm dev:mock` 先调页面状态和交互。
2. 用 `pnpm tauri:dev` 验证真实 command、文件读写和系统环境。
3. 跨边界行为补 contract test；Rust service 补相邻测试文件。
4. 跑与改动匹配的过滤测试，命令见 [测试与验证](../guides/testing-and-validation.md)。
5. 提交前跑 `pnpm test:pr`；改了生产边界、依赖或发布再跑 `pnpm build`。

`pnpm build` 先做 TypeScript 检查再 `vite build`。Vite 在任何 build 里都固定解析 `src/lib/backend/tauri/create-backend.ts`，并拒绝 `src/dev`、`src/test` 和测试文件进入生产模块图，所以不能用 mock 代替生产 build 验证。

## 编辑边界

前端分层、mock 边界、测试分文件和产品写入入口以根 [AGENTS.md](../../AGENTS.md) 的「前端 backend 分层」「测试」为准。最常碰到的三条：

- 只有 `src/lib/backend/tauri/` 可以调用 Tauri `invoke`。
- `src/dev/mocks/` 只服务 `pnpm dev:mock`、Vitest 和 Playwright；页面不能自己判断环境后切到 mock。
- 产品写入走 `src/lib/api/tickets.ts` 的 plan / bind / unbind；`src/lib/api/adapter.ts` 只用于预览和本机路由运行时。
