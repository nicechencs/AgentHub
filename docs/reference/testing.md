---
title: 测试参考
description: AgentHub 前端、Tauri contract、Rust core 和 fixture 的测试约定。
type: reference
audience: contributor
status: current
updated: 2026-09-29
---

# 测试参考

本页是完整命令表和文件约定。按改动选哪条命令，看 [测试与验证](../guides/testing-and-validation.md)；风险分级以 [AGENTS.md](../../AGENTS.md) 为准。不要把提交前或 CI 的全量门禁当成每次本地改动的默认步骤。

## 命令

| 命令 | 用途 |
|---|---|
| `pnpm typecheck` | 应用 TypeScript |
| `pnpm typecheck:test` | 测试 TypeScript |
| `pnpm test` | Vitest 全量 |
| `pnpm test:contracts` | backend、边界和 feature contract 集 |
| `pnpm test:bridge` | 本机路由：`cargo test -p agenthub-core bridge` + `cargo test -p agenthub-gui adapter_bridge_controller` |
| `pnpm test:e2e:browser` | Playwright 浏览器冒烟，只打 `dev:mock` 的真实 DOM |
| `pnpm check:docs` / `pnpm test:docs` | 文档检查 / 文档检查脚本自身的测试 |
| `pnpm build` | 生产构建（固定 Tauri adapter）和模块图检查 |
| `cargo test -p agenthub-core --locked` | Rust core |
| `cargo test -p agenthub-cli --locked` | CLI |
| `cargo test -p agenthub-gui --locked` | Tauri 桌面端 |
| `pnpm test:pr` | 提交前组合门禁：`typecheck` + `typecheck:test` + `test` + core cargo test |

说明：

- Vitest 由配置固定使用 mock backend；`pnpm dev:mock` 是浏览器演示入口。
- `pnpm test:e2e:browser` 在独立端口（`scripts/dev-runtime.json` 的 `e2ePort`，当前 5174）启动 `vite --mode mock`，只覆盖路由、表单、弹层和焦点，不代表真实 Tauri。不要复用 `pnpm dev` 的 5173。
- `pnpm build` 永远选 Tauri adapter，不允许把 mock 或 `e2e/` 打进生产 bundle。

## 文件约定

- Rust：生产文件只声明 `#[cfg(test)] mod tests;`，实现放 `tests.rs` 或 `*_tests.rs`。
- 前端：测试与生产文件并列，用 `*.test.ts` / `*.test.tsx`。
- Playwright：独立目录 `e2e/browser/`，不进 Vitest 和生产 bundle。
- mock reset：放 `src/dev/mocks`；生产 façade 不暴露测试 reset API。

## 测试层

1. 纯函数和 mapper：边界值、错误码、序列化、路径解析。
2. backend contract：Tauri command、DTO、unsupported/unavailable、adapter 选择。
3. core service：SQLite、锁、备份、配置写入、能力门禁、安装和解析器。
4. 本机路由 / 协议：本机 HTTP、鉴权、surface、模型名单、非流式和 SSE 转换。
5. 构建边界：生产模块图不能包含 `src/dev`、`src/test`、`e2e/`、测试/规格文件。
6. 浏览器冒烟：Playwright + Chromium 走 `dev:mock` 的关键用户旅程；不覆盖 Tauri IPC、系统对话框或真实账号。

## Fixture

- 说明文件：`src/lib/provider-detect/__tests__/fixtures/README.md`。每个样例写明来源形态和预期识别结果。
- URL、Key、路径用占位符；真实网络、真实路径和真实登录信息不进仓库。写法细则见 [隐私与发布边界](privacy-and-release.md#测试数据和-fixture)。
- 测试只断言结构和打码行为，不断言用户的真实配置值。

## 失败分类

| 失败 | 通常说明 |
|---|---|
| typecheck | 类型契约或 import 边界错误 |
| Vitest | mock/领域行为或 mapper 回归 |
| Playwright | `dev:mock` 下的路由、表单、弹层或焦点回归；保留 trace 与 HTML report |
| Cargo | core 逻辑、路径、锁、数据库或解析器回归 |
| build 检查 | 生产代码依赖了 dev/mock/test/e2e 模块 |
| Tauri 实机 | 真实 runtime、系统命令或 IPC 映射问题 |

保留失败用例和原始错误；不要为了让套件变绿放宽安全断言，或把真实网络塞进单元测试。
