---
title: 测试参考
description: AgentHub 前端、Tauri contract、Rust core 和 fixture 的测试约定。
type: reference
audience: contributor
status: current
updated: 2026-08-31
---

# 测试参考

风险分级以 [AGENTS.md](../../AGENTS.md) 为准。日常选命令时优先看 [测试与验证](../guides/testing-and-validation.md)；本页是完整命令表和文件约定。

## 命令

| 命令 | 用途 |
|---|---|
| `pnpm typecheck` | 应用 TypeScript |
| `pnpm typecheck:test` | 测试 TypeScript |
| `pnpm test` | Vitest 全量 |
| `pnpm test:contracts` | backend、边界和 feature contract 集 |
| `pnpm test:e2e:browser` | Playwright 浏览器冒烟，只打 `pnpm dev:mock` 的真实 DOM |
| `pnpm build` | Tauri adapter 生产 build 和 module graph guard |
| `cargo test -p agenthub-core --locked` | Rust core |
| `cargo test -p agenthub-cli --locked` | CLI |
| `pnpm test:pr` | 提交前组合门禁 |

Vitest 由配置固定使用 mock backend；`pnpm dev:mock` 是浏览器演示入口。`pnpm test:e2e:browser` 用 Playwright 在独立端口启动 `vite --mode mock`，只覆盖路由、表单、弹层和焦点，不代表真实 Tauri。不要复用本机 `pnpm dev`（5173，Tauri adapter）。`pnpm build` 永远选择 Tauri adapter，禁止把 mock 或 `e2e/` 打进生产 bundle。

## 验证范围

按 [AGENTS.md](../../AGENTS.md) 的风险级别选择命令，不要把提交前或 CI 的全量门禁当成每次本地改动的默认步骤。

## 文件约定

- Rust：生产文件只声明 `#[cfg(test)] mod tests;`，实现放 `tests.rs` 或 `*_tests.rs`。
- 前端：测试与生产文件并列，使用 `*.test.ts` 或 `*.test.tsx`。
- Playwright：独立目录 `e2e/browser/`，不进入 Vitest 与生产 bundle。
- fixture：使用脱敏、最小、可读的输入；真实网络、真实路径和真实凭据不进仓库。
- mock reset：放在 `src/dev/mocks`；生产 façade 不暴露测试 reset API。

## 测试层

1. 纯函数和 mapper：边界值、错误码、序列化、路径解析。
2. backend contract：Tauri command、DTO、unsupported/unavailable、adapter 选择。
3. core service：SQLite、lock、backup、配置写入、能力闸门、安装和解析器。
4. Routes/协议：loopback HTTP、认证、surface、模型名单、非流式和 SSE 转换。
5. build boundary：生产模块图不得包含 `src/dev`、`src/test`、`e2e/`、测试/规格文件。
6. 浏览器冒烟：Playwright + Chromium 走 `pnpm dev:mock` 的关键用户旅程；不覆盖 Tauri IPC、系统对话框或真实账号。

## Fixture 规则

fixture README 位于 `src/lib/provider-detect/__tests__/fixtures/README.md`。每个样例都应说明来源形态和预期识别结果；URL、Key、路径使用占位符。测试只断言结构和脱敏行为，不断言用户的真实配置值。

## 失败分类

- typecheck 失败：类型契约或 import 边界错误；
- Vitest 失败：mock/domain 行为或 mapper 回归；
- Playwright 失败：`dev:mock` 下的路由、表单、弹层或焦点回归；失败时保留 trace 与 HTML report；
- Cargo 失败：core 逻辑、路径、锁、数据库或 parser 回归；
- build guard 失败：生产依赖了 dev/mock/test/e2e 模块；
- Tauri smoke 失败：真实 runtime、系统命令或 IPC 映射问题。

测试结果必须保留失败用例和原始错误；不要为让套件变绿而放宽安全断言或把真实网络塞进单元测试。

