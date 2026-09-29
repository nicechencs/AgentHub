---
title: Local Route Sidecar
type: proposal
status: proposed
owner: maintainers
updated: 2026-09-29
---

# Local Route Sidecar

> Status: proposed. A future architecture candidate, not a commitment. No sidecar binary exists.

## 1. Current baseline

- `local_bridge` runs inside the Tauri process. `DesktopAdapterControl` (`src-tauri/src/adapter_control_host.rs`) is the in-process control host.
- There is no `agenthub-adapterd`, IPC client, schema lease, or sidecar lifecycle.
- When the host is unavailable, status is `host_unavailable` (`crates/agenthub-core/src/models/adapter_state_model.rs`). The UI never invents a running state or falls back to mock data.

## 2. Candidate goal

A same-package, current-user process owns the long-lived route runtime; the desktop UI and CLI become control clients. It would survive GUI reload, crash, update, and window close. It is not a system service or remote API.

Moves into the process (candidate):

- loopback listener and protocol data plane;
- route lifecycle, health, drain, recovery;
- operation journal for start/stop/apply/remove;
- versioned control IPC.

Stays outside:

- accounts, providers, Connections, Tickets, Bindings, credentials;
- live configuration writes and generated providers;
- SQLite migrations and table writes;
- public or LAN listeners, multi-user daemons.

The sidecar calls a Tauri-neutral core contract for domain changes. It never writes Agent files, credential files, or domain tables itself.

## 3. Invariants

1. Loopback only (`127.0.0.1`, `::1`).
2. GUI exit does not stop a healthy runtime; stop-and-exit is a separate command.
3. Mutations are idempotent by request ID and payload hash.
4. Status comes from the live process. A stored profile with no reachable process is `host_unavailable`.
5. Handshake checks protocol, contract, and schema versions before any mutation.
6. Instance identity and epoch reject stale responses after restart.
7. Update and rollback use drain/prepare; schema mismatch fails closed.
8. No secrets in argv or ordinary control messages; no new credential store.

## 4. Evaluation slices

- **A. Contract hardening:** keep the in-process host; test status, lifecycle, idempotency, stale instance, failure.
- **B. Read-only prototype:** spawn a process for a status handshake only.
- **C. Runtime ownership:** after B is accepted, move the listener and journal behind IPC, with fault injection (crash, update, stale lock, schema mismatch).
- **D. Client parity:** GUI and CLI share one control client.

## 5. Gates

- Deterministic start/stop/restart under crash and update.
- No domain-table or live-file writes from the runtime process.
- Schema lease with a recoverable failure path.
- GUI and CLI see identical status and errors.
- Routes list/detail stays usable when the runtime is down.
- Security review of loopback exposure, instance lock, secret handling.
- Focused Rust, frontend contract, and end-to-end smoke tests green.

## 6. Non-goals

Credential encryption, domestic OAuth adapters, OAuth-to-API conversion, a public service, or moving Connections/Accounts/Providers out of the app.
