# agenthub-adapterd (isolated Messages slice)

This directory is an independent Go program. It is **not** the live/default
local gateway. The in-process Rust forwarder is unchanged. Vendor plugin pages
are unchanged. There is no plugin store.

This slice only proves, in an isolated scratch directory:

- control `Handshake`, `Status`, `AcquireOrRenewOwner` (`acquire` / `renew`), `Start`, and `Stop`
- process up / down
- `POST /v1/messages` with a **synthetic** entry key, JSON and SSE, forwarded
  to a controlled loopback upstream mock
- connection-pool scheduling: `priority_failover` / `round_robin`, member
  health and cooldown, model union, client cancel; no member switch after
  output has started
- same-protocol `POST /v1/responses`, `POST /v1/chat/completions`, and the
  alias `POST /chat/completions` (JSON, SSE, tool calls, cancel, upstream
  errors). The runner does not guess Codex versus Grok from the request body.

`Start` is the product control name for this isolated slice. It does not write
real agent config, refuses the product default port `43121` and real
`~/.agenthub`, and only starts Messages listening from
`$AGENTHUB_HOME/config/probe.json` when `AGENTHUB_HOME` is a scratch directory.

`ActivateProbeListen` remains a probe-only shortcut with the same listen start.
It is not the default gateway.

## Build

```bash
cd go/agenthub-adapterd
go test ./...
go build -o bin/agenthub-adapterd .
```

## Isolated probe

From the repository root (creates an absolute scratch tree under `/tmp`, never
`~/.agenthub`):

```bash
scripts/route-runtime-probe/messages-isolated.sh
scripts/route-runtime-probe/pool-isolated.sh
scripts/route-runtime-probe/protocols-isolated.sh
```

The scripts list every data/config/log path before start, check they stay
under scratch, then verify handshake, status, process up, Messages JSON/SSE,
pool scheduling, Responses and Chat Completions, cancel, Stop, and process down.

## Run the daemon yourself

`AGENTHUB_HOME` must be an **absolute** directory under `/tmp`, `/var/tmp`, or
`.tmp/route-runtime-probe/`. The listen port must not be the product default
`43121`.

Control channel: Unix domain socket at `$AGENTHUB_HOME/run/adapterd.sock`
(Linux). POST JSON envelopes to `http://localhost/control` with
`curl --unix-socket`.

```bash
export AGENTHUB_HOME=/tmp/agenthub-route-runtime-probe/manual/home
mkdir -p "$AGENTHUB_HOME"/{config,run,logs}
# write config/probe.json (synthetic key + loopback upstream) before Start
./bin/agenthub-adapterd run --home "$AGENTHUB_HOME" --listen-port 18765
```

Status replies must not include the entry key or upstream credentials.

## Out of scope

Responses, Chat Completions, official login, live gateway cutover, real
`~/.agenthub` reads/writes, plugin SDK/ABI, and stages E/F.
