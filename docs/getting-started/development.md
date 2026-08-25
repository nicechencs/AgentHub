---
title: 开发环境
description: 在本地启动 AgentHub、选择 backend adapter 并运行最小验证。
type: getting-started
audience: contributor
status: current
updated: 2026-08-25
---

# 开发环境

本文面向第一次在仓库中开发的人。业务代码分在 `agenthub-core`，GUI 是 `src-tauri`，CLI 是 `crates/agenthub-cli`；React 页面只通过 backend contract 或兼容 façade 访问后端。

## 前置条件

- Node.js 与 pnpm（用于 Vite、Vitest 和前端依赖）。
- Rust stable、Cargo，以及 Tauri 对当前操作系统的构建依赖。
- Git。

安装依赖后，从仓库根目录执行：

```text
pnpm install
```

## 启动方式

| 命令 | 运行形态 | backend adapter |
|---|---|---|
| `pnpm dev` | Vite 开发服务器（`http://127.0.0.1:5173`） | Tauri adapter；在普通浏览器中调用 Tauri 能力会显示 unavailable |
| `pnpm tauri:dev` | Tauri 桌面开发窗口 | Tauri adapter |
| `pnpm dev:mock` | 浏览器 mock 演示 | browser mock adapter |

`pnpm dev:mock` 只用于浏览器演示和页面开发，不代表生产后端。`pnpm dev` 与 `pnpm tauri:dev` 使用同一套生产 Tauri backend；只有桌面运行时提供真实 `invoke`。

## 构建和检查

```text
pnpm typecheck
pnpm typecheck:test
pnpm test
pnpm build
```

`pnpm build` 先运行应用 TypeScript 检查，再运行 `vite build`。Vite 配置在任何 build 中固定解析 `src/lib/backend/tauri/create-backend.ts`，并在生成 bundle 时拒绝 `src/dev`、`src/test` 以及测试文件进入生产模块图；因此不能用 mock 代替生产 build 的验证。

Rust 核心和 CLI 的局部检查：

```text
cargo test -p agenthub-core --locked
cargo test -p agenthub-cli --locked
```

提交前使用仓库脚本跑完整门禁：

```text
pnpm test:pr
```

它包含应用和测试 typecheck、Vitest，以及 `agenthub-core` 的 Cargo 测试。测试策略和分域命令见 [testing-and-validation.md](../guides/testing-and-validation.md) 与 [testing.md](../reference/testing.md)。

## 编辑边界

- 只有 `src/lib/backend/tauri/` 可以直接调用 Tauri `invoke`。
- `src/dev/mocks/` 只服务 `pnpm dev:mock` 和 Vitest；页面不能自行判断环境后静默切换 mock。
- 生产代码和测试代码分文件。Rust 生产模块只声明 `#[cfg(test)] mod tests;`，测试实现放在相邻 `tests.rs` 或 `*_tests.rs`；前端测试使用并列 `*.test.ts` / `*.test.tsx`。
- 产品写入使用 `src/lib/api/tickets` 的 plan/bind/unbind 流程；`src/lib/api/adapter` 只用于预览和本机 Routes 运行时。

## 最小工作流

1. 用 `pnpm dev:mock` 先验证页面状态和交互。
2. 用 `pnpm tauri:dev` 验证真实 Tauri command、文件读写和系统环境。
3. 为跨边界行为补 contract test；为 Rust service 补相邻测试文件。
4. 运行 `pnpm typecheck`、`pnpm typecheck:test` 和相关 Vitest/Cargo 过滤测试。
5. 最后运行 `pnpm build`，确认 mock 没有进入生产模块图。

