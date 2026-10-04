# agenthub-adapterd (isolated route runtime)

This directory is an independent Go program. It is **not** the live/default
local gateway. The in-process Rust forwarder is unchanged. Vendor plugin pages
are unchanged. There is no plugin store.

These slices prove, in an isolated scratch directory:

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
`~/.agenthub`. The application supervisor resolves eligible saved loopback
routes and official Anthropic API Key routes through core, sends the complete
runtime configuration once over the child process's stdin, and closes stdin.
The configuration is retained only in memory. It can contain multiple
Messages, Responses, and Chat Completions entries selected by their entry key
and surface.

The older `$AGENTHUB_HOME/config/probe.json` input remains only for the
standalone probes and `ActivateProbeListen`; application start does not use it.

Runtime validation accepts external upstreams only for Anthropic Messages
routes that use an API Key and the exact official
`https://api.anthropic.com` endpoint (optionally `/v1`). It rejects user info,
query/fragment data, other ports, encoded paths, and redirects. The isolated
probes do not call the real Anthropic service.

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
scripts/route-runtime-probe/existing-flow-isolated.sh
```

The scripts list every data/config/log path before start, check they stay
under scratch, then verify handshake, status, process up, Messages JSON/SSE,
pool scheduling, Responses and Chat Completions, entry isolation, both upstream
authentication modes, cancel, graceful drain, Stop, and process down. The
existing-flow probe passes its runtime configuration through stdin rather than
writing API keys to disk.

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
# write config/probe.json (synthetic key + loopback upstream) before probe Start
./bin/agenthub-adapterd run --home "$AGENTHUB_HOME" --listen-port 18765
```

Status replies must not include the entry key or upstream credentials.
Application-managed runs add `--runtime-config-stdin` and provide the strict
`route-config.v0-isolated` document on stdin. This interface remains internal
to the isolated supervisor and is not a public configuration format.

## Out of scope

Official-login refresh, non-allowlisted external upstreams, live/default
gateway cutover, real `~/.agenthub` reads/writes, combined desktop-to-Agent
configuration write/recovery, Windows control transport, plugin SDK/ABI, and
stages E/F. No probe calls the real Anthropic service or uses a real API Key.
