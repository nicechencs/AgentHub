---
title: Local Route Sidecar
type: proposal
status: proposed
owner: maintainers
updated: 2026-08-25
---

# Local Route Sidecar

> Status: proposed
> 
> This is a future architecture candidate. It is not an implementation commitment and must not be read as evidence that a sidecar binary exists.

## 1. Current baseline

`local_bridge` is currently hosted inside the Tauri process. `adapter_control` and `DesktopAdapterControl` provide a Tauri-neutral control contract and an in-process host, but there is no `agenthub-adapterd`, IPC client, schema lease implementation, or sidecar lifecycle in the product.

The current Routes page must continue to represent the in-process host accurately. If the host is unavailable, the UI reports `host_unavailable`; it does not silently invent a running status or fall back to browser mock data.

## 2. Candidate goal

Evaluate a same-package, current-user process that can own the long-lived `local_bridge` runtime while the desktop UI and CLI act as control clients. The candidate would address GUI reload, crash, update, and window-close behavior without making the route listener a system service or a remote API.

## 3. Proposed boundary

Only long-lived route runtime responsibilities are candidates for the process boundary:

- loopback listener and protocol data plane;
- route lifecycle, health, drain, recovery, and process admission;
- durable operation journal for start/stop/apply/remove recovery;
- a versioned control IPC for status and mutations.

These remain outside the candidate process:

- accounts, providers, Connections, Tickets, Bindings, and credential ownership;
- live configuration writes and generated-provider lifecycle;
- SQLite migration and arbitrary table writes;
- general configuration routes and native endpoints that do not need a long-lived runtime;
- public network listeners, LAN service behavior, or a multi-user daemon.

The sidecar must call a Tauri-neutral core application contract for domain mutations. It must not write target Agent files, credential files, or domain tables directly.

## 4. Compatibility invariants

1. The data plane stays loopback-only (`127.0.0.1` and `::1`).
2. GUI exit does not implicitly stop a healthy candidate runtime; explicit stop-and-exit remains a separate command.
3. A runtime mutation is idempotent by request ID and canonical payload hash.
4. Status is observed from the runtime process. A durable profile without a reachable process is `host_unavailable`, never `running`.
5. Handshake checks protocol, application-contract, and schema compatibility before any mutation.
6. Instance identity and epoch protect clients from stale responses after a restart.
7. Update and rollback require a drain/prepare protocol. A process with an incompatible schema must fail closed.
8. Secrets do not travel in argv or ordinary control messages. The candidate does not create a new credential store.

## 5. Candidate slices

These are evaluation slices, not a committed schedule:

### Slice A: contract hardening

Keep the in-process host as the only runtime and prove the Tauri-neutral control contract with status, lifecycle, idempotency, stale-instance, and failure tests.

### Slice B: read-only process prototype

Spawn a same-package process for a read-only status handshake. Compare lifecycle, data directory, logging, and update behavior without moving writes or the listener.

### Slice C: controlled runtime ownership

Only after Slice B is accepted, move listener ownership and journal recovery behind the IPC. Preserve the same control contract and add fault injection for process crash, update, stale lock, and schema mismatch.

### Slice D: client parity

Make GUI and CLI use the same control client. Keep browser mock behavior in the mock adapter and make non-Tauri production pages explicitly unavailable.

## 6. Decision gates

Do not promote the candidate unless all of the following are demonstrated:

- deterministic start/stop/restart behavior under crash and update;
- no direct domain-table or live-file writes from the runtime process;
- schema lease and migration compatibility with a recoverable failure path;
- GUI and CLI observe identical status and error contracts;
- Routes list/detail remains usable when the runtime is unavailable;
- loopback exposure, instance locking, and secret handling have security review;
- focused Rust, frontend contract, and end-to-end smoke tests are green.

## 7. Explicit exclusions

This proposal does not include credential-at-rest encryption, domestic OAuth adapters, OAuth-to-API conversion, a public service, or moving Connections/Accounts/Providers into another process. Those topics are outside the product scope and must not be added as sidecar milestones.
