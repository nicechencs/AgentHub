# agenthub-adapterd (isolated Messages slice)

This directory is an independent Go program. It is **not** the live/default
local gateway. The in-process Rust forwarder is unchanged. Vendor plugin pages
are unchanged. There is no plugin store.

This slice only proves, in an isolated scratch directory:

- control `Handshake`, `Status`, and `AcquireOrRenewOwner` (`acquire` / `renew`)
- process up / down
- `POST /v1/messages` with a **synthetic** entry key, JSON and SSE, forwarded
  to a controlled loopback upstream mock

`ActivateProbeListen` is a **probe-only** control message. It is not product
`CommitDesired`, does not write real agent config, and only starts Messages
listening from `$AGENTHUB_HOME/config/probe.json` when `AGENTHUB_HOME` is a
scratch directory.

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
```

The script lists every data/config/log path before start, checks they stay
under scratch, then verifies handshake, status, process up, one Messages JSON
request (and SSE), and process down.

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
# write config/probe.json (synthetic key + loopback upstream) before ActivateProbeListen
./bin/agenthub-adapterd run --home "$AGENTHUB_HOME" --listen-port 18765
```

Status replies must not include the entry key or upstream credentials.

## Out of scope

Responses, Chat Completions, official login, live gateway cutover, real
`~/.agenthub` reads/writes, plugin SDK/ABI, and stages E/F.
