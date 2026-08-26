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

## Candidates

| Proposal | Status | Question it explores |
|---|---|---|
| [adapter-sidecar.md](adapter-sidecar.md) | proposed | Could a user-level process own the long-lived local route runtime while GUI and CLI remain clients? |
| [tray-background-modes.md](tray-background-modes.md) | proposed | Could closing the window reduce WebView memory without changing route ownership or exit semantics? |
| [modularity.md](modularity.md) | proposed | Which single-source and use-case boundaries should be tightened before any larger process change? |
| [plugin-management.md](plugin-management.md) | proposed | Could AgentHub list and manage per-agent plugin/extension packs (not MCP servers) via official CLIs, with a Routes-like workbench? |

## Proposal rules

1. State the current behavior before describing a future shape.
2. Keep account, provider, credential, and live-write ownership explicit.
3. Prefer a reversible slice with contract tests over a directory move or a new framework.
4. Do not turn an architectural option into a feature flag, UI label, or capability claim before implementation.
5. Keep credentials-at-rest encryption and domestic OAuth/API conversion outside these proposals. They are not project work.

## Promotion to current contract

A proposal can be promoted only after the implementation has a named owner, an explicit compatibility/migration plan, focused tests, failure behavior, and an update to the current docs under `docs/concepts/`, `docs/decisions/`, `docs/architecture/`, `docs/reference/`, `docs/guides/`, or `docs/ui/`. Until then, links from current pages should describe the baseline, not the candidate.
