---
title: 测试与验证
description: 按 frontend、Tauri contract、Rust core 和生产 build 分层验证改动。
type: guide
audience: contributor
status: current
updated: 2026-08-31
---

# 测试与验证

AgentHub 的测试边界与 backend adapter 边界一致：浏览器 mock 用于页面和交互，Tauri contract 验证 IPC 映射，Rust core 验证业务和文件系统。不要用一个端到端测试替代三层契约测试。

风险分级、Agent 流程和最小验证以 [AGENTS.md](../../AGENTS.md) 为准，本页不重复那张表。本页只列分层命令和边界。日常改动先跑过滤测试；下面的快速检查属于提交前或 CI。查完整命令表或 CI 矩阵时再打开 [测试参考](../reference/testing.md)。

## 快速检查

提交前或 CI 使用：

```text
pnpm typecheck
pnpm typecheck:test
pnpm test
pnpm build
```

`pnpm build` 是生产边界检查：它固定使用 Tauri adapter，并在 bundle 阶段拒绝 `src/dev`、`src/test` 和测试模块。`pnpm dev:mock` 不能作为 build 验证。不要为页面或纯函数改动默认运行 `pnpm build`。

## 前端测试

- Vitest 配置固定使用 mock backend；测试通过 `src/test/setup.ts` 初始化。
- 页面/领域测试文件与生产文件并列：`feature.test.ts` 或 `feature.test.tsx`。
- domain reset 放在 `src/dev/mocks`；不要把 `__reset*ForTests` 加回生产 façade。
- mapper、backend contract、error mapping 和 feature flags 优先做纯测试。
- Playwright（`pnpm test:e2e:browser`）只打 browser mock 的真实 DOM：启动、主导航、Connections / Chat / Projects 旅程，以及 Dialog 的 Escape / Tab / 关闭后焦点恢复。它不覆盖 Tauri、真实网络或用户目录。首批只跑 Chromium。

常用命令：

```text
pnpm test -- --run src/lib/backend/contracts/adapter-wire.test.ts
pnpm test -- --run src/lib/backend/boundary-imports.test.ts
pnpm test:contracts
```

## Tauri contract

涉及 command 参数、DTO 或 adapter 选择时，至少验证：

1. Tauri adapter 使用正确 command 名和参数形状；
2. 不支持或非 Tauri 环境返回明确 unavailable；
3. 页面不直接 import `@tauri-apps/api`；
4. mock backend 与生产 contract 的行为边界一致。

`pnpm build` 的 module graph guard 是额外门槛，但不能替代 contract test。

## Rust core

Rust 生产模块只声明测试模块，测试实现放相邻文件：

```rust
#[cfg(test)]
mod tests;
```

测试内容按风险选择：service 状态机、SQLite migration、路径安全、per-agent lock、backup/apply 补偿、adapter registry、协议转换和解析器 fixture。不要在测试中写真实用户 home、真实账号或真实远程 token。

```text
cargo test -p agenthub-core --locked
cargo test -p agenthub-core --locked bridge
cargo test -p agenthub-cli --locked
```

## 生产代码与测试代码的分离

- Rust 测试不得与业务实现写在同一文件；只保留 `#[cfg(test)] mod tests;` 声明。
- 前端测试不得放进生产模块；使用 `*.test.ts` / `*.test.tsx`。
- mock fixture 不得从生产入口导出；Vite build 不得解析 `src/dev`。
- 测试 fixture 内的 URL、Key、路径都使用明显占位值。

## 提交前矩阵

内环先用与风险匹配的过滤命令。提交前再按改动面补齐；没有边界变化时，不要把 CI 全量搬进每一次本地改动。

| 改动 | 最小验证 |
|---|---|
| 页面样式/交互 | 相关 Vitest + `pnpm typecheck`；涉及路由/弹层/焦点时加 `pnpm test:e2e:browser` |
| backend contract / façade | contract tests + `pnpm typecheck:test` |
| Rust service / adapter | 相关 `cargo test -p agenthub-core --locked <filter>` |
| Adapter capability 契约 JSON | `cargo test -p agenthub-core --locked shared_capability_contract`；内核输出变化后用 `UPDATE_ADAPTER_CAPABILITY_CONTRACT=1` 重新生成 golden，禁止手改 expect |
| 运行时安装或配置写入 | core service tests + Tauri contract + mock flow |
| Routes / 协议转换 | bridge HTTP/SSE tests + 错误码和日志断言 |
| 生产边界、依赖或发布 | `pnpm build` + `pnpm test:pr` |

完整参考见 [testing.md](../reference/testing.md)。测试失败时保留原始失败用例和日志，不通过放宽断言隐藏回归。

