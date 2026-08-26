# Archive

> **Archived documentation.** Everything in this directory is a historical record, completed implementation note, superseded plan, or dated audit. It is not a current implementation contract and must not be converted into a new task list without checking the current docs and code.

## How to read the archive

- Read current behavior from the organized docs under `docs/concepts/`, `docs/decisions/`, `docs/architecture/`, `docs/guides/`, `docs/reference/`, and `docs/ui/`.
- Read future candidates from `docs/proposals/`; every proposal is explicitly marked `Status: proposed`.
- Historical wording, old route names, old file paths, and “not implemented” statements may be preserved here because they describe the document's authoring context. They do not override the current `/routes`, Routes, 路由 contract.
- The records below retain their historical正文. Each begins with a uniform Archived warning and points readers toward the current destination.

## Retained records

| File | Historical role | Current reading path |
|---|---|---|
| [a4-unified-loopback-gateway.md](a4-unified-loopback-gateway.md) | Unified loopback gateway design and A4 compatibility decisions | Current route endpoint and runtime reference docs; same-port pool candidate is `docs/proposals/unified-loopback-pool.md` |
| [routing-connection-refactor-plan.md](routing-connection-refactor-plan.md) | Dated multi-lane implementation plan for route and connection refactoring | Current concepts/architecture docs plus `docs/proposals/adapter-sidecar.md` and `docs/proposals/unified-loopback-pool.md` |
| [multi-account-routing-rfc.md](multi-account-routing-rfc.md) | RFC for same-surface multi-account route runtime and member health | Current routing concept plus the same-port pool candidate `docs/proposals/unified-loopback-pool.md` |
| [hub-redesign-plan.md](hub-redesign-plan.md) | Completed Hub Phase 1 implementation record | `docs/ui/page-patterns.md` and `docs/concepts/connections-and-routing.md` |
| [route-endpoint-audit-2026-08.md](route-endpoint-audit-2026-08.md) | Dated endpoint audit snapshot | `docs/reference/local-route-api.md` |
| [single-kernel-projections.md](single-kernel-projections.md) | Completed Adapter single-kernel proposal plus E/F evaluation | Current contract: `docs/architecture/adapter-route-kernel.md` |

## Removed historical bodies

The following one-time notes were removed from the current worktree after their stable conclusions were rewritten into current docs. Their original bodies remain recoverable from Git history; they are not copied here because doing so would preserve stale commands, paths, and task lists beside the current contracts.

| Legacy file | Why the body is no longer current | Current reading path |
|---|---|---|
| `agenthub-plan.md` | Mixed product plan, completed phases, and status snapshot | `docs/STATUS.md` and `docs/architecture/overview.md` |
| `bridges-page-redesign.md` | Superseded page name and route | `docs/ui/page-patterns.md` |
| `chat-page-redesign.md` | Completed page redesign plan | `docs/ui/page-patterns.md` and `docs/concepts/chat-and-agents.md` |
| `chat-ui-agent-mechanism-comparison.md` | Dated comparison research | `docs/concepts/chat-and-agents.md` for current behavior |
| `deepseek-harness-integration.md` | Dated integration plan mixed with implementation status | `docs/guides/adding-an-agent.md` and `docs/reference/capabilities.md` |
| `hardcoding-governance.md` | One-time source-of-truth audit | `docs/STYLE.md` and the current architecture/reference pages |
| `platform-capability-refactor.md` | Completed refactor plan | `docs/architecture/core-runtime.md` and `docs/reference/capabilities.md` |
| `platform-capability-remediation.md` | Completed remediation record | `docs/architecture/core-runtime.md` |
| `route-detail-redesign.md` | Superseded implementation paths | `docs/ui/page-patterns.md` |
| `ui-experience-alignment.md` | Dated visual comparison and completed phases | `docs/ui/design-system.md` and `docs/ui/page-patterns.md` |

## Retention policy

For a retained archive document, use the archive when its implementation phase is complete, its decisions have been copied into a current contract, or it is useful only as audit evidence. Preserve that retained body and date. Add the standard warning at the top, update links to the current destination, and add the file to [legacy-document-index.md](legacy-document-index.md).

One-time records removed from the active tree are recoverable only from Git history. Their stable conclusions must be migrated to the corresponding current source-of-truth document and recorded in [legacy-document-index.md](legacy-document-index.md); the deleted body is not an active contract, implementation plan, or TODO list.

An archived document may explain why a decision changed. It cannot reopen a product decision, add a current route, enable a capability, or create a security/credential task by itself.
