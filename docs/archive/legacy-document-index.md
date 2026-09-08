# Legacy Document Index

> Status: archive migration record
> 
> This index covers every legacy flat document that was present directly under `docs/` at the start of the documentation reorganization, plus the retained archive records. It records where the content belongs after the reorganization. It is not a second current navigation index.

## Migration rules

- Stable product/domain content moves to `docs/concepts/` and `docs/decisions/`.
- System and roadmap content moves to `docs/architecture/` and `docs/STATUS.md`.
- Contribution contracts move to `docs/guides/`; shared reference facts move to `docs/reference/`.
- Wire and runtime facts move to `docs/reference/`.
- Current UI contracts move to `docs/ui/`.
- Future candidates move to `docs/proposals/` and must remain `Status: proposed`.
- Completed plans, one-time audits, comparisons, and superseded redesigns move to `docs/archive/` with the Archived warning.

The destination paths below are the intended organized names. A destination may consolidate several legacy documents; the old file name still appears here so links, searches, and review comments remain traceable.

## Legacy flat documents (31)

| Legacy file | Destination | Treatment |
|---|---|---|
| `docs/README.md` | `docs/README.md` | Rewrite as the organized entrypoint and current-doc index |
| `docs/account-authorization-pool.md` | `docs/concepts/accounts-and-authorization.md` | Move stable account/authorization rules |
| `docs/adapter-design.md` | `docs/concepts/adapters-and-bridges.md` + `docs/reference/local-route-api.md` | Consolidate current runtime facts; remove plan prose |
| `docs/adapter-kimi-codex-dogfood.md` | `docs/guides/adapter-dogfood.md` | Move the current-safe dogfood procedure; do not retain secrets or prompts |
| `docs/adapter-sidecar-design.md` | `docs/proposals/adapter-sidecar.md` | Rewrite as a future candidate with `Status: proposed` |
| `docs/adding-an-agent.md` | `docs/guides/adding-an-agent.md` | Move contribution checklist and registry contract |
| `docs/agenthub-plan.md` | `docs/STATUS.md` + `docs/architecture/overview.md` | Keep current status facts; archive completed phase scheduling |
| `docs/architecture.md` | `docs/architecture/overview.md`, `core-runtime.md`, and `frontend-backend.md` | Move system and frontend/backend boundary contract |
| `docs/bridges-page-redesign.md` | `docs/ui/page-patterns.md` | Extract current Routes page behavior; old terminology remains only in archive context |
| `docs/capability-matrix.md` | `docs/reference/capabilities.md` | Move current capability and availability rules |
| `docs/chat-page-redesign.md` | `docs/ui/page-patterns.md` + `docs/concepts/chat-and-agents.md` | Extract current Chat workflow and states |
| `docs/chat-process-streaming.md` | `docs/concepts/chat-and-agents.md` | Move protocol/display boundary and current event contract |
| `docs/chat-ui-agent-mechanism-comparison.md` | `docs/concepts/chat-and-agents.md` | One-time comparison body removed from the active tree; stable current behavior is documented in the destination, and the full historical body is recoverable only from Git history |
| `docs/cli-and-config.md` | `docs/reference/cli-and-config.md` | Move CLI command and configuration contract |
| `docs/connection-binding-model.md` | `docs/concepts/connections-and-routing.md` | Move domain model and product write semantics |
| `docs/deepseek-harness-integration.md` | `docs/guides/adding-an-agent.md` + `docs/reference/capabilities.md` | Dated integration body removed from the active tree; only stable contribution and capability rules remain current, while the full historical body is recoverable only from Git history |
| `docs/hardcoding-governance.md` | `docs/STYLE.md` + current architecture/reference pages | One-time audit body removed from the active tree; stable source-of-truth rules are current, while the full historical body is recoverable only from Git history |
| `docs/local-route-endpoints.md` | `docs/reference/local-route-api.md` | Move current loopback endpoint and protocol contract |
| `docs/logging.md` | `docs/reference/logging.md` | Move logging, redaction, and retention contract |
| `docs/modularity-improvement.md` | `docs/proposals/modularity.md` | Rewrite as a future candidate with current baseline |
| `docs/platform-capability-refactor.md` | `docs/architecture/core-runtime.md` + `docs/reference/capabilities.md` | Completed refactor body removed from the active tree; stable architecture and capability facts are current, while the full historical body is recoverable only from Git history |
| `docs/platform-capability-remediation.md` | `docs/architecture/core-runtime.md` | Completed remediation body removed from the active tree; stable runtime facts are current, while the full historical body is recoverable only from Git history |
| `docs/privacy.md` | `docs/reference/privacy-and-release.md` | Move current privacy, release, screenshot, and secret-handling rules |
| `docs/product-decisions.md` | `docs/concepts/connections-and-routing.md` + `docs/decisions/product-boundaries.md` | Move the three user-facing connection methods |
| `docs/provider-api-oauth-adaptation.md` | `docs/concepts/adapters-and-bridges.md` + `docs/reference/capabilities.md` + `docs/reference/route-compatibility.md` | Move provider/API capability and writeability rules; the compatibility matrix is owned by `docs/reference/route-compatibility.md` |
| `docs/route-detail-redesign.md` | `docs/ui/page-patterns.md` | Extract current Routes list/detail behavior |
| `docs/testing.md` | `docs/reference/testing.md` + `docs/guides/testing-and-validation.md` | Move test boundaries and validation contract |
| `docs/tray-background-modes.md` | `docs/proposals/tray-background-modes.md` | Rewrite as a future candidate with `Status: proposed` |
| `docs/ui-component-standard.md` | `docs/ui/design-system.md` | Extract stable component, token, and state rules |
| `docs/ui-design.md` | `docs/ui/page-patterns.md` | Extract current shells, pages, and interactions |
| `docs/ui-experience-alignment.md` | `docs/ui/design-system.md` + `docs/archive/README.md` | Keep stable UI constraints current; record the dated comparison only as history |

## Retained archive records (28)

| Archive file | Current destination or explanation |
|---|---|
| `docs/archive/a4-unified-loopback-gateway.md` | Historical gateway design; current facts belong in `docs/reference/local-route-api.md` and `docs/concepts/adapters-and-bridges.md` |
| `docs/archive/2026-09-07-project-review-06b893c4.md` | 2026-09-07 project review first round, moved from `docs/reviews/` |
| `docs/archive/2026-09-07-project-review-c726bd2a.md` | 2026-09-07 project review continuation, moved from `docs/reviews/` |
| `docs/archive/2026-09-07-project-review-c8d792b5.md` | 2026-09-07 incremental review, moved from `docs/reviews/` |
| `docs/archive/2026-09-07-review-fix-progress.md` | Fix tracking for the 2026-09-07 reviews, moved from `docs/reviews/` |
| `docs/archive/2026-09-project-review.md` | 2026-09-04 project review, moved from `docs/reviews/` |
| `docs/archive/chat-b2-handoff.md` | Chat B2 development launch prompt, moved from `docs/guides/chat-b2-handoff.md`; current facts belong in `docs/STATUS.md` and `docs/archive/chat-codex-b2.md` |
| `docs/archive/chat-claude-b3.md` | Claude ChatRuntime probe, moved from `docs/status/chat-claude-b3.md`; current facts belong in `docs/STATUS.md` |
| `docs/archive/chat-codex-b1.md` | Codex Chat B1 record, moved from `docs/status/chat-codex-b1.md`; current facts belong in `docs/STATUS.md` |
| `docs/archive/chat-codex-b2.md` | Codex Chat B2 record, moved from `docs/status/chat-codex-b2.md`; current facts belong in `docs/STATUS.md` |
| `docs/archive/chat-codex-s0.md` | Codex Chat S0 probe, moved from `docs/status/chat-codex-s0.md` |
| `docs/archive/chat-review-handoff.md` | Chat B1 independent-review launch prompt, moved from `docs/guides/chat-review-handoff.md`; evidence belongs in `docs/archive/chat-codex-b1.md` |
| `docs/archive/codex-install-modularity-review.md` | 2026-08-27 Codex install review, moved from `docs/status/codex-install-modularity-review.md` |
| `docs/archive/hub-redesign-plan.md` | Historical Hub Phase 1 record; current page behavior belongs in `docs/ui/page-patterns.md` and current product docs |
| `docs/archive/multi-account-routing-rfc.md` | Historical same-surface multi-account RFC; current runtime facts belong in product/reference docs; the shipped default pool is documented in `docs/concepts/connections-and-routing.md` |
| `docs/archive/naming-convention-audit.md` | 2026-09-02 naming audit, moved from `docs/reviews/naming-convention-audit.md`; current terms belong in `docs/reference/terminology.md` |
| `docs/archive/objectization-encapsulation-audit.md` | Objectization audit, moved from `docs/architecture/`; five volumes share this prefix; follow-on designs remain `proposed` in `docs/architecture/*-owners.md` |
| `docs/archive/objectization-encapsulation-audit-cli-tauri.md` | Objectization audit volume, moved from `docs/architecture/` |
| `docs/archive/objectization-encapsulation-audit-core.md` | Objectization audit volume, moved from `docs/architecture/` |
| `docs/archive/objectization-encapsulation-audit-frontend.md` | Objectization audit volume, moved from `docs/architecture/` |
| `docs/archive/objectization-encapsulation-audit-oauth.md` | Objectization audit volume, moved from `docs/architecture/` |
| `docs/archive/objectization-encapsulation-audit-tests-fixtures.md` | Objectization audit volume, moved from `docs/architecture/` |
| `docs/archive/project-review-handoff.md` | 2026-09-07 full-project review launch prompt, moved from `docs/guides/project-review-handoff.md`; current procedure belongs in `docs/guides/agent-workflow.md` |
| `docs/archive/recent-fix-review.md` | 2026-08-26 fix recap, moved from `docs/architecture/recent-fix-review.md` |
| `docs/archive/route-endpoint-audit-2026-08.md` | Dated audit snapshot; current endpoint facts belong in `docs/reference/local-route-api.md` |
| `docs/archive/routing-connection-refactor-plan.md` | Historical implementation plan; future process options belong in `docs/proposals/adapter-sidecar.md` and `docs/proposals/modularity.md` |
| `docs/archive/single-kernel-projections.md` | Completed single-kernel proposal, implementation slices, and dated E/F evaluation; current contract belongs in `docs/architecture/adapter-route-kernel.md` |
| `docs/archive/unified-loopback-pool.md` | Design record for the same-port authorization pool; current contract belongs in `docs/concepts/connections-and-routing.md` and `docs/reference/local-route-api.md` |

## Coverage check

The flat set is intentionally listed as 31 rows, including the legacy `README.md`. The retained archive set is intentionally listed as 28 rows. When a root file is removed, its row must remain here and the destination document must contain the stable content or an explicit archived explanation. No row authorizes credential encryption work or domestic OAuth/API conversion.
