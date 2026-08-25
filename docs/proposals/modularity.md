---
title: Modularity and Boundary Tightening
type: proposal
status: proposed
owner: maintainers
updated: 2026-08-25
---

# Modularity and Boundary Tightening

> Status: proposed
> 
> This proposal records possible next boundaries for the existing modular monolith. It is not a backlog and does not authorize a broad directory rewrite.

## 1. Current baseline

The project already has a useful core/gui/cli split, Agent integration registry, backend contract layer, product `plan`/`bind`/`unbind` writes, and an `adapter_control` contract with an in-process Tauri host. Chat is split into a page orchestrator, model functions, hook, and components. These are current boundaries and must be preserved.

The remaining pressure is concentrated in single sources of truth and cross-domain orchestration: install channels, configuration writers, capability matrix versus write gates, and services that combine CRUD with live-write compensation. `local_bridge` remains an in-process runtime; process migration is covered by [adapter-sidecar.md](adapter-sidecar.md), not this document.

## 2. Candidate principles

1. Keep the modular monolith. Do not introduce microservices, DDD/CQRS ceremony, an event bus, or a dynamic plugin ABI for the sake of naming.
2. Make each product rule have one authoritative source plus contract tests.
3. Keep domain ownership in core; Tauri and CLI are shells and clients.
4. Keep page files as orchestration and move pure decisions to existing model/lib modules.
5. Preserve public compatibility façades while shrinking their discoverable write surface.
6. Prefer a small, independently testable seam over a physical move.

## 3. Candidate work areas

### P0: contract and source-of-truth tightening

- Derive install channels from the registered Agent contribution instead of maintaining parallel literals.
- Use shared fixtures to assert capability matrix, plan/write gate, and apply behavior agree for each rule ID.
- Keep product writes on `plan`/`bind`/`unbind`; retain deprecated compatibility calls only behind internal boundaries.
- Keep the `TicketBinding` versus `ActiveBinding` distinction explicit in code and docs.
- Maintain the backend direction `contracts -> Tauri adapter/mocks -> compatibility API -> pages`; pages and layout must not call `invoke`.

### P1: use-case boundaries

- Separate provider CRUD, provider switching, account compensation, route planning, runtime lifecycle, and backup/restore coordination by ports/use cases while preserving public service entry points.
- Introduce stable wire DTO mappings for commands that currently expose core models directly.
- Add a shared Backend contract suite that runs against browser mock and an injectable Tauri transport.
- Continue the existing page pattern for Skills, Projects, and Agent lifecycle: pure model functions, a hook for effects, and a thin index orchestrator.

### P2: process and façade follow-through

- Evaluate the sidecar candidate only after control-contract, schema, and update gates pass.
- Reduce the production `AgentAdapter` façade to sparse Agent contributions without changing Agent-facing behavior.
- Give GUI and CLI one control client for local route lifecycle if the sidecar is promoted.
- Remove compatibility ambiguity only after callers and tests have migrated.

## 4. Ownership table

| Concern | Candidate owner | Must not become |
|---|---|---|
| Agent-specific differences | `integrations/agents/<key>/` contribution | Page-specific Agent switches |
| Capability and route decision | `domain/protocol_graph/` | A second rule table in mock or UI |
| Product write | `plan` / `bind` / `unbind` use cases | A page calling an old compatibility apply directly |
| Live local route runtime | Current in-process control host; future sidecar only if promoted | A credential store or direct SQL writer |
| UI transport | backend contracts and adapters | Page-level `invoke` or runtime mock selection |
| Page pure logic | Existing `*-model`/`*-format` modules | A new framework or global state layer |

## 5. Verification before each slice

- Identify callers and the public compatibility surface before moving a symbol.
- Add or update contract tests before deleting a duplicate path.
- Keep production and test code in separate files.
- Run the focused frontend or Rust domain suite through a separate test step.
- Update current docs only after code and tests establish the new owner.
- Leave unrelated historical design records in the archive; do not turn them into active work.

## 6. Non-goals

No directory-wide rewrite, microservice split, Connections/Accounts/Providers process split, credential-at-rest encryption, domestic OAuth adapter, or OAuth-to-API conversion is part of this proposal. These exclusions are deliberate and must not appear as implementation milestones.
