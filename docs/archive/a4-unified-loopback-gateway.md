# A4 unified loopback gateway — design (archived)

> **归档（2026-08-24）**。进程内 Gateway 已落地（`crates/agenthub-core/src/bridge/host/gateway.rs`）。现行契约见 [../reference/route-compatibility.md](../reference/route-compatibility.md) 与 [../reference/local-route-api.md](../reference/local-route-api.md)。下文是当时的签核稿，不要按「not implemented」派工。

> **Archived / 已归档**: Historical design and approval record. Preserve the body for context; do not use it as the current implementation contract, capability switch, or TODO list.
> **Status**: archived historical record
> Current behavior is defined by the organized `docs/concepts/`, `docs/decisions/`, `docs/reference/`, and `docs/ui/` documents.
>
Status at authoring time: **design only**. A1–A3 had landed on `refactor/bridge-gateway`. Do not treat the freeze notes below as current engineering gates.

Today (after A1–A3): **one profile → one loopback TCP listener → one `BridgeLocalSurface` → one `ResolvedAuth`**. The router already registers all four paths; unmatched conversation endpoints 404 *before* local auth.

Target (§5.4): **one in-process gateway** (still `BridgeRuntimeHost`; no sidecar) with `/v1/messages`, `/v1/responses`, `/v1/chat/completions`, `/v1/models`. Local bearer identifies the edge. Path selects `DownstreamSurface`.

---

## 1. Port compatibility — pick dual-listen, not forced rewrite

### What is already written into agents

Generated live config stores a **per-profile** loopback URL, not a gateway id:

| Target | Written URL | Token |
|---|---|---|
| Codex | `http://127.0.0.1:{port}/v1` (`wire_api = responses`) | `OPENAI_API_KEY` = local bearer |
| Grok / Kimi / DSH | `http://127.0.0.1:{port}/v1` | `api_key` = local bearer |
| Claude Code | `ANTHROPIC_BASE_URL=http://127.0.0.1:{port}` (no `/v1`) | `ANTHROPIC_AUTH_TOKEN` = local bearer |

`AdapterProfile.local_port` is the port those URLs used at last successful bind. Restore rebinds `preferred_port = local_port`. Agents, extra workspaces, and copied TOML all pin that tuple.

Forced rewrite on first upgrade would drop any process still pointing at the old port (Claude/Codex/Grok running across a Hub restart, a second checkout, a manual copy). That violates “存量绑定不得因升级失联”.

### Decision: compatibility dual-listen, converge on apply/restore

**One router, many sockets.**

- All bound sockets are `127.0.0.1` / `::1` only. Same Axum app, same bearer table.
- Running set binds the **union of `local_port` values** of started profiles. Two profiles on 43121 and 43122 → two TCP listeners, one dispatcher.
- **New** `start`/`apply`: if the gateway already has a socket, project **that** port into the new profile (convergence). If none, keep today’s ephemeral/preferred bind and that port becomes the first gateway socket.
- **Restore of an existing profile**: bind its historical `local_port` (alias if the gateway already owns another port). Register the edge. Do **not** rewrite live agent config on boot.
- **Convergence write**: only the existing apply/restore saga (`needs_reprojection` / `switch_generated_provider`) may change the URL in the target Agent. After a successful realign, `local_port` equals the shared port and the orphan alias is unbound when no remaining profile cites it.
- Last edge stop unbinds remaining sockets. No always-on gateway without a bound profile.

Rejected:

| Option | Why not |
|---|---|
| Forced migration on upgrade | Breaks pinned loopback URLs outside the saga; no lock/backup/verify |
| Well-known fixed port (e.g. 43100) | Occupancy vs other apps; still needs a rewrite of every live config |
| Keep independent routers forever | Misses §5.4; new edges would keep growing match arms / admission islands |

### Schema

**No `adapter_profiles` column this round.** `local_port` stays “port the target config should use”. Adding `gateway_port` would be a later migration and needs a separate sign-off. Tauri commands unchanged.

---

## 2. Bearer identifies the edge

Replace `ListenerState` (one token, one surface, one upstream) with:

```text
Gateway
  sockets: set of loopback TcpListener (union of cited ports)
  edges: HashMap<profile_id, EdgeState>
  tokens: local bearer → profile_id   // unique per profile, as today
```

`EdgeState` holds what `ListenerState` holds today: upstream config, admission semaphore, observed upstream status, grok replay, listed models, reload callback.

Auth is the **only** middleware:

1. Extract `Authorization: Bearer` or `x-api-key` (same as today).
2. Constant-time compare against every live local bearer; if none match → **401 `invalid_api_key`** (same body as `/health` today). Do not reveal whether the path would have been 404.
3. Bound edge = the match. Path → `DownstreamSurface`. If that edge’s `local_surface` does not serve it → **404** (empty, as today).

This **changes 404-vs-401 order** relative to A1–A3. Today a Messages-only listener 404s `/v1/responses` even with a bad token. After A4, a bad token is always 401; 404 means “this bearer/edge does not serve that endpoint”. That is the A4 card’s contract rewrite. Tests in `bridge/tests.rs` that assert 404 without distinguishing auth must be updated.

Refresh of **upstream** auth still must not rotate the local bearer (`ensure_listener_replaces_upstream_auth_while_keeping_local_bearer`).

---

## 3. `/v1/models` per bearer

Keep §5.1.3: local synthesis, never upstream proxy.

- Requires a valid local bearer (401 otherwise). The endpoint itself must exist (404 here is a regression).
- Body is `{ object: "list", data: [{ id, object: "model" }] }` from **that edge’s** `listed_models`.
- Empty mapping → `200` + `data: []` + `empty_models` log. Do not invent `gpt-*` / `grok-*`.
- Aliases `/v1/models` and `/models` unchanged.

---

## 4. Health

`GET /health` stays a **non-billable liveness** read. It never probes the provider.

| Request | Response |
|---|---|
| Missing/wrong bearer | 401 `invalid_api_key`, log `op=health code=unauthorized` (warn) |
| Valid bearer, edge registered | 200 `{ ok, service: "agenthub-bridge", listener_status: "running", upstream_status }` where `upstream_status` is **that edge’s** last stored outcome |
| Valid bearer, edge stopping | 503 `bridge_stopping` (same as conversation) |

No anonymous “is anything listening” probe. Success stays **debug**. Control-plane `verify_bound_health` uses the profile’s own bearer against any of the gateway sockets that profile cites (`local_port`).

`degraded` remains per-edge (listen up, last upstream failed). Process-wide `host_unavailable` is unchanged (GUI derived).

---

## 5. Admission: per-edge, not per-socket

Today: `Semaphore::new(MAX_IN_FLIGHT_REQUESTS_PER_PROFILE)` on each listener (256 prod, 4 in tests). Two sockets would incorrectly double (or split) capacity if the cap stayed per socket.

After A4: **one semaphore per `EdgeState`**. Alias ports share it. Cap stays 256 — desktop safety, not a conversation quota. Overload still **429 `bridge_overloaded` + `Retry-After: 1`**, log `code=overloaded` with that `profile_id`.

A future AccountPicker (C2) hangs off `EdgeState`, not off the TCP listener. If A4 is deferred, C2 can still put the picker on today’s per-profile `ListenerState` (same cardinality: one edge per listener).

---

## 6. Start / stop / conflict

- `BridgeStartSpec` stays the control-plane input (profile_id, port, local_token, upstream, listed_models, reload). Internals may ignore “one spec one socket” and instead `register_edge` + `ensure_socket(port)`.
- Idempotent exact live start: same profile, same spec → `Ok(status)` as today.
- Upstream-only drift still `ConflictingStart` until `ensure_bridge_listener` replaces the edge in place; **local bearer unchanged**.
- `same_spec` must compare edge identity (token, upstream, surface, models), not “is this the only process on this port”.
- Stop(profile): unregister edge, drop its semaphore/replay; unbind a socket only when no remaining edge cites that port. Gateway process lives in `BridgeRuntimeHost` as today.

Host remains Tauri-neutral. Sidecar IPC is out of this round.

---

## 7. Test rewrite (after sign-off)

Must add/adjust:

- Same process, two profiles, two bearers, two surfaces: each path served only for the matching bearer; the other 404s.
- Shared port: two edges on one socket, tokens do not cross.
- Alias port: historical port still reaches the same edge after a second profile has taken the shared port.
- Unauth is 401 on every path including formerly 404-only surfaces.
- `/v1/models` body depends on bearer.
- `/health` `upstream_status` depends on bearer.
- Overload 429 is per-edge (fill A, B still accepts).
- Existing 401/429/502, RetryGate, grok recovery, passthrough fixtures, `ensure_listener_replaces_upstream_auth_while_keeping_local_bearer`.
- Frontend: `pnpm test -- bridges` (status page still reads per-profile port/running from control plane).

---

## 8. Out of scope (unchanged)

Load balancing; multi-account picker (C2); sidecar; domestic OAuth; credential-at-rest encryption; `/v1/embeddings` `/v1/images/*` `/v1/realtime`; flipping any `canApply`; public bind.
