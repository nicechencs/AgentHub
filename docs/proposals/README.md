---
title: Future Proposals
type: navigation
status: current
owner: maintainers
updated: 2026-08-26
---

# Proposals

> These documents describe future candidates, not current implementation contracts.

Every proposal in this directory has `Status: proposed`. A proposal may be researched, prototyped, rejected, or superseded without changing the current product. It must not be copied into a current implementation checklist until its owner, scope, compatibility plan, and acceptance tests are approved.

## Current baseline

- The user-facing runtime page is **Routes / 路由** at `/routes`.
- `local_bridge` currently runs in the Tauri process through the in-process control host. The current control contract is useful independently of any process move.
- The current tray behavior and module boundaries are the baseline. A proposal must preserve them until a replacement is implemented and verified.
- Product writes remain `plan` / `bind` / `unbind`; credentials, accounts, providers, and live configuration are not moved into a speculative runtime process.
- A local-bridge profile still authenticates with its own local bearer. Sharing one loopback port does not currently mean one Hub token or a cross-product authorization pool.

## Candidates

| Proposal | Status | Question it explores |
|---|---|---|
| [adapter-sidecar.md](adapter-sidecar.md) | proposed | Could a user-level process own the long-lived local route runtime while GUI and CLI remain clients? |
| [unified-loopback-pool.md](unified-loopback-pool.md) | proposed | Could every routed authorization share one loopback port, one Hub token per Agent, and model/health scheduling? |
| [tray-background-modes.md](tray-background-modes.md) | proposed | Could closing the window reduce WebView memory without changing route ownership or exit semantics? |
| [modularity.md](modularity.md) | proposed | Which single-source and use-case boundaries should be tightened before any larger process change? |
| [Service 内部 owner 拆分](../architecture/service-internal-owners.md) | proposed | Concrete internal owner split for O-11 ProviderService, O-12 AccountService, O-13 BackupService, and O-14/O-66 local-route persist — façades and switch semantics stay |
| [plugin-management.md](plugin-management.md) | proposed | Could AgentHub list and manage per-agent plugin/extension packs (not MCP servers) via official CLIs, with a Routes-like workbench? |
| [read-model-owners.md](../architecture/read-model-owners.md) | proposed | Could O-15–O-19 be narrowed with unique mapper owners without changing wire DTO or splitting public types? |
| [runtime-context-owners.md](../architecture/runtime-context-owners.md) | proposed | Could store reset/invalidation live in one runtime context (O-07) without changing plan/bind/switch? |
| [form-sidebar-owners.md](../architecture/form-sidebar-owners.md) | proposed | Could GenericConfigForm and Sidebar split schema/nav/stats owners (O-23) without restyling chrome? |
| [startup-gateway-owners.md](../architecture/startup-gateway-owners.md) | proposed | Concrete owners for AgentHub bootstrap, registry vs catalog, transport, Gateway, and protocol vs vendor policy (O-26–O-30) |
| [usage-owners.md](../architecture/usage-owners.md) | proposed | Usage filter/normalizer/model-switch owners (O-31–O-34) without changing totals or reasoning_tokens |
| [test-fixture-owners.md](../architecture/test-fixture-owners.md) | proposed | Connect-flow fixtures, mock ticket resolver width, and OAuth device test store scope (O-41/O-42/O-44) |

## Proposal rules

1. State the current behavior before describing a future shape.
2. Keep account, provider, credential, and live-write ownership explicit.
3. Prefer a reversible slice with contract tests over a directory move or a new framework.
4. Do not turn an architectural option into a feature flag, UI label, or capability claim before implementation.
5. Keep credentials-at-rest encryption and domestic OAuth/API conversion outside these proposals. They are not project work.

## Promotion to current contract

A proposal can be promoted only after the implementation has a named owner, an explicit compatibility/migration plan, focused tests, failure behavior, and an update to the current docs under `docs/concepts/`, `docs/decisions/`, `docs/architecture/`, `docs/reference/`, `docs/guides/`, or `docs/ui/`. Until then, links from current pages should describe the baseline, not the candidate.
