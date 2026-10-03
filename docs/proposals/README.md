---
title: Future Proposals
type: navigation
status: current
owner: maintainers
updated: 2026-10-03
---

# Proposals

> These documents describe future candidates, not current implementation contracts.

Every proposal in this directory has `Status: proposed`. A proposal may be researched, prototyped, rejected, or superseded without changing the current product. It must not be copied into a current implementation checklist until its owner, scope, compatibility plan, and acceptance tests are approved. Rows marked 半面已落地 / 切片已落地 still use YAML `proposed` because remaining bounds are not current contracts; do not treat those pages as unstarted.

## Current baseline

- The user-facing runtime page is **Routes / 路由** at `/routes`.
- `local_bridge` currently runs in the Tauri process through the in-process control host. The current control contract is useful independently of any process move.
- The current tray behavior (window policy in `src-tauri/src/window_policy.rs`) and module boundaries are the baseline. A proposal must preserve them until a replacement is implemented and verified.
- Product writes remain `plan` / `bind` / `unbind`; credentials, accounts, providers, and live configuration are not moved into a speculative runtime process.
- Default local-bridge pools share one loopback port and one Hub token per Agent/surface. Mixed-provider composite routes and Codex↔Grok pair adapters remain experimental flags, default off. The shipped pool contract lives in [Connections and routing](../concepts/connections-and-routing.md) and [the local route API](../reference/local-route-api.md); the design record is archived at [unified-loopback-pool.md](../archive/unified-loopback-pool.md).

## Candidates

| Proposal | 落地情况 | 要回答的问题 |
|---|---|---|
| [Chat 统一体验与跨环境开发](chat-unified-experience.md) | 部分落地 | 如何共用聊天交互、逐家接入 Agent，并在换电脑或换开发 Agent 后复现和验收？用户标准见 [Chat 体验标杆](../ui/chat-experience-bar.md)。 |
| [可选本机命令护栏](chat-tool-guard.md) | 未落地 | 占位，暂不实现。 |
| [Go 路由替换](adapter-sidecar.md) | 隔离切片已落地 | 如何停止扩建旧 Rust 路由运行模块，按 Messages、连接池、其他协议、产品接入和默认网关切换逐条交付 Go 替代实现？不派生插件平台、复杂租约或两阶段提交。 |
| [tray-background-modes.md](tray-background-modes.md) | 部分落地 | 关窗后能否释放 WebView 内存，而不改变路由归属和退出语义？关窗策略已抽到 `src-tauri/src/window_policy.rs`。 |
| [模块化与边界收紧](modularity.md) | 部分落地 | 如何先收紧模块接口，再选择有明确收益的官方扩展，而不把所有功能一次变成插件平台？ |
| [接入 Kiro（kiro-cli）](agent-kiro.md) | 大部分落地 | 剩 Skills / MCP。现行以 [STATUS](../STATUS.md) 为准。 |
| [Kiro HTTP / 本机转发](agent-kiro-http.md) | 大部分落地 | 剩企业 IdC / `profileArn` 验收、打印路径流式、主机迁移。 |
| [国内自动更新镜像（R2）](update-mirror-r2.md) | 代码已落地 | 剩云端运维配置和国内真机验收。 |
| [plugin-management.md](plugin-management.md) | 部分落地 | 插件更新与 Codex / Pi 写入仍是提案。现行以 [STATUS](../STATUS.md) 为准。 |

已全部落地的提案（Chat 宿主加深、7 份 `*-owners` 拆分）和 2026-09-07 UI 审查已移到 [归档](../archive/README.md)。

## Proposal rules

1. State the current behavior before describing a future shape.
2. Keep account, provider, credential, and live-write ownership explicit.
3. Prefer a reversible slice with contract tests over a directory move or a new framework.
4. Do not turn an architectural option into a feature flag, UI label, or capability claim before implementation.
5. Keep credentials-at-rest encryption and domestic OAuth/API conversion outside these proposals. They are not project work.

## Promotion to current contract

A proposal can be promoted only after the implementation has a named owner, an explicit compatibility/migration plan, focused tests, failure behavior, and an update to the current docs under `docs/concepts/`, `docs/decisions/`, `docs/architecture/`, `docs/reference/`, `docs/guides/`, or `docs/ui/`. Until then, links from current pages should describe the baseline, not the candidate.
