---
title: 测试与验证
description: 按前端、Tauri contract、Rust core 和生产 build 分层选择验证命令。
type: guide
audience: contributor
status: current
updated: 2026-09-29
---

# 测试与验证

本页告诉你改完代码该跑哪条命令。风险分级和最小验证以 [AGENTS.md「协作」](../../AGENTS.md#协作) 为准；完整命令表和 CI 矩阵见 [测试参考](../reference/testing.md)。

测试分三层，和 backend 边界一致：浏览器 mock 测页面和交互，Tauri contract 测 IPC 映射，Rust core 测业务和文件系统。不要用一个端到端测试代替这三层。

## 按改动选命令

日常先跑与风险匹配的过滤测试；全量组合留到提交前或 CI。

| 改动 | 最小验证 |
|---|---|
| 页面样式 / 交互 | 相关 Vitest + `pnpm typecheck`；涉及路由、弹层或焦点时加 `pnpm test:e2e:browser` |
| backend contract / façade | `pnpm test:contracts` + `pnpm typecheck:test` |
| Rust service / adapter | `cargo test -p agenthub-core --locked <filter>` |
| Adapter 能力契约 JSON | `cargo test -p agenthub-core --locked shared_capability_contract`；内核输出变了用 `UPDATE_ADAPTER_CAPABILITY_CONTRACT=1` 重新生成，不手改期望文件 |
| 运行时安装或配置写入 | core service 测试 + Tauri contract + mock 流程 |
| 本机路由 / 协议转换 | `pnpm test:bridge`（core `bridge` + GUI `adapter_bridge_controller`），并断言错误码和日志 |
| 纯文档 | `pnpm check:docs` |
| 生产边界、依赖或发布 | `pnpm build` + `pnpm test:pr` |

测试失败时保留原始用例和日志，不靠放宽断言藏回归。

## 前端

1. 跑单个文件：

   ```text
   pnpm test src/lib/backend/boundary-imports.test.ts
   ```

   结果：只运行该文件，末尾显示 `Test Files 1 passed`。

2. 跑 backend 边界与 contract 集：`pnpm test:contracts`。
3. 需要真实 DOM 时跑 `pnpm test:e2e:browser`。它在独立端口启动 `vite --mode mock`，只用 Chromium，覆盖 `e2e/browser/` 下的启动、导航、连接、对话、历史等旅程，不覆盖 Tauri、真实网络或用户目录。

规则：

- Vitest 固定使用 mock backend，由 `src/test/setup.ts` 初始化。
- 测试文件与生产文件并列：`feature.test.ts` / `feature.test.tsx`。
- 领域 reset 放 `src/dev/mocks`，不要把 `__reset*ForTests` 加回生产 façade。
- mapper、backend contract、错误映射和 feature flag 优先写纯函数测试。

## Tauri contract

改 command 参数、DTO 或 adapter 选择时，至少确认：

1. Tauri adapter 用了正确的 command 名和参数形状；
2. 不支持或非 Tauri 环境返回明确的 unavailable；
3. 页面没有直接 import `@tauri-apps/api`；
4. mock backend 与生产 contract 的行为边界一致。

`pnpm build` 的模块图检查是额外门槛，不能代替 contract test。

## Rust core

生产模块只声明测试模块，实现放相邻的 `tests.rs`（或 `*_tests.rs`）：

```rust
#[cfg(test)]
mod tests;
```

按风险选测试内容：service 状态机、SQLite migration、路径安全、per-agent 锁、备份与写入补偿、adapter registry、协议转换、解析器 fixture。测试里不写真实用户 home、真实账号或真实远程 token。

```text
cargo test -p agenthub-core --locked <filter>
cargo test -p agenthub-cli --locked
cargo test -p agenthub-gui --locked <filter>
```

## 提交前

```text
pnpm test:pr
```

它依次跑 `pnpm typecheck`、`pnpm typecheck:test`、`pnpm test` 和 `cargo test -p agenthub-core --locked`。改了生产边界、依赖或发布相关文件时再加 `pnpm build`：它固定使用 Tauri adapter，并在打包时拒绝 `src/dev`、`src/test` 和测试文件进入生产模块图。页面或纯函数改动不要默认跑 `pnpm build`，`pnpm dev:mock` 也不能代替它。

## 测试与生产代码分离

- Rust 测试不与业务实现写在同一文件。
- 前端测试不放进生产模块。
- mock fixture 不从生产入口导出；Vite build 不解析 `src/dev`。
- fixture 里的 URL、Key、路径都用明显的占位值。
