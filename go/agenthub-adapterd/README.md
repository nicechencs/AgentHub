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
- `POST /v1/responses`, `POST /v1/chat/completions`, and the alias
  `POST /chat/completions` (JSON, SSE, tool calls, cancel, upstream errors),
  including Responses ingress translated to a controlled OpenAI-compatible
  Chat upstream. The configured edge selects the protocol; the runner does
  not guess Codex versus Grok from the request body.
- bounded HTTP resources: strict 8 MiB request and 32 MiB response/SSE limits,
  a 16-request admission limit, upstream phase timeouts, SSE idle timeout, and
  safe response/error headers that never relay cookies, redirects, auth
  challenges, or raw upstream error bodies

`Start` is the product control name for this isolated slice. It does not write
real agent config, refuses the product default port `43121` and real
`~/.agenthub`. The application supervisor resolves eligible saved loopback
routes and the allowlisted official API Key routes through core, sends the
initial complete runtime snapshot as a length-framed document, and keeps stdin
open for later atomic replacements. The active configuration is retained only
in memory. It can contain multiple Messages, Responses, and Chat Completions
entries selected by their entry key and surface.

The older `$AGENTHUB_HOME/config/probe.json` input remains only for the
standalone probes and `ActivateProbeListen`; application start does not use it.

Runtime validation accepts only these exact official external routes:

| Target | Accepted base | Fixed final request URL | Route contract |
| --- | --- | --- | --- |
| Anthropic API Key | `https://api.anthropic.com/v1` | `https://api.anthropic.com/v1/messages` | Messages + `anthropic_messages` + `x_api_key` |
| OpenAI API Key | `https://api.openai.com/v1` | `https://api.openai.com/v1/chat/completions` | Responses or Chat Completions + `openai_chat_completions` + bearer |
| Kimi Code membership API Key | `https://api.kimi.com/coding/v1` | `https://api.kimi.com/coding/v1/chat/completions` | Responses or Chat Completions + `openai_chat_completions` + bearer |

Core and Go bind the target, login type, surface, transport, and authentication
as one row. Changing only a URL or one metadata field cannot turn another saved
login into an allowed route. Loopback fixtures remain compatible. Codex and
Grok official-login external routes remain closed until their vendor-specific
request contracts are implemented; Kimi OAuth, arbitrary relays, and other
external targets are also rejected.

External request URLs are built from the fixed target table rather than joined
from user-controlled paths. The outbound client ignores environment proxy
settings. Before each connection attempt it resolves the approved hostname,
rejects non-public or mixed DNS answers, then dials one of those checked
addresses while retaining the approved HTTPS hostname. It does not follow
redirects and shares a bounded connection pool. Successful JSON responses must
be JSON media types containing valid JSON;
successful streams must be `text/event-stream`. Errors, redirects, malformed
responses, and responses over the limit use a synthetic local error body.

`ActivateProbeListen` remains a probe-only shortcut with the same listen start.
It is not the default gateway.

## Build

```bash
cd go/agenthub-adapterd
go test ./...
go build -o bin/agenthub-adapterd .
```

Desktop release builds run `pnpm build:go-sidecar` and stage the target-specific
binary through Tauri `externalBin`. The build manifest, desktop version, and
SHA-256 are embedded in the GUI. On Unix, the supervisor opens that bundled
file without following symlinks, verifies it, copies it into the current 0700
scratch session, verifies the 0500 private copy again, and executes only that
copy. This makes the isolated supervisor usable from a packaged Unix build;
it still requires an explicit start and still refuses the default gateway
port. The Go process also has an authenticated `127.0.0.1` TCP control
transport that atomically selects its port and limits active connections. It
has run on Linux and cross-compiles for Windows amd64/arm64. The Windows
desktop supervisor remains unavailable until its process supervision and the
complete lifecycle pass on a Windows machine. The control token is not passed
through the child environment or command line: it is a framed prelude on the
same inherited stdin pipe that carries runtime configuration.

## Isolated probe

From the repository root (creates an absolute scratch tree under `/tmp`, never
`~/.agenthub`):

```bash
scripts/route-runtime-probe/messages-isolated.sh
scripts/route-runtime-probe/pool-isolated.sh
scripts/route-runtime-probe/protocols-isolated.sh
scripts/route-runtime-probe/existing-flow-isolated.sh
scripts/route-runtime-probe/config-stream-isolated.sh
scripts/route-runtime-probe/http-safety-isolated.sh
scripts/route-runtime-probe/packaged-sidecar-security-isolated.sh
scripts/route-runtime-probe/control-tcp-isolated.sh
scripts/route-runtime-probe/bind-go-e2e-isolated.sh
scripts/route-runtime-probe/external-policy-isolated.sh
```

The scripts list every data/config/log path before start, check they stay
under scratch, then verify handshake, status, process up, Messages JSON/SSE,
pool scheduling, Responses and Chat Completions, entry isolation, both upstream
authentication modes, cancel, graceful drain, Stop, and process down. The
existing-flow probe passes its runtime configuration through stdin rather than
writing API keys to disk. The HTTP-safety probe uses a malicious loopback
fixture to verify request/response/SSE limits, error and redirect sanitizing,
the production non-stream and SSE idle timeouts, the 16-request admission limit,
slot reuse after cancellation or rejection, and log redaction. Its two production
idle checks take about one minute. Set `AGENTHUB_HTTP_SAFETY_LONG_PROBE=1` to
also exercise the two-minute non-stream total timeout with a continuous slow
drip. The shortened timeout cases and downstream write deadline remain covered
by Go tests rather than pretending the real production durations elapsed.
The packaged-sidecar security probe runs a real non-root GUI against a
root-owned test copy, replaces the original path after it has been opened, and
confirms that only the verified private copy executes. It also checks that
stale-session cleanup removes only strictly owned and marked scratch roots.
The TCP-control probe checks authentication before request processing, a
child-selected port, the connection ceiling, hot reload, inherited-stdin token
delivery, command-line and file secret scans, graceful stop, and port release.
The bind probe makes the real desktop supervisor use that TCP control path and
exercises `plan` / `bind` / provider switch / delete / `unbind`, including two
required hot reloads, startup-secret scans, and backup restoration after the
original provider row has been deleted. The normal Unix supervisor still uses
its local socket.

The external-policy probe builds a real core example and the real Go process,
then pipes core-generated configurations into Go. It starts and stops the local
listener for five allowed cases and confirms ten denied cases, the accepted
configuration hash, secret scanning, and port release. It sends zero route
requests and makes zero external requests, so it does not validate any real
Anthropic, OpenAI, or Kimi service or API Key. Run it with
`pnpm probe:go-route-external-policy`.

## Run the daemon yourself

`AGENTHUB_HOME` must be an **absolute** directory under `/tmp`, `/var/tmp`, or
`.tmp/route-runtime-probe/`. The listen port must not be the product default
`43121`.

Control channel: Unix domain socket at `$AGENTHUB_HOME/run/adapterd.sock`
(Linux). POST JSON envelopes to `http://localhost/control` with
`curl --unix-socket`. The alternate TCP control starts only with
`--control-listen 127.0.0.1:0 --control-token-stdin`. Stdin must begin with a
four-byte big-endian length of exactly 43 followed by a canonical 256-bit raw
base64url token. Any one-shot JSON or length-framed runtime configuration
follows immediately on that same pipe. The child reports its selected endpoint
on stdout after binding. This transport is for the desktop supervisor and the
isolated probe, not a user-facing network API. The legacy
`AGENTHUB_ADAPTERD_CONTROL_TOKEN` environment source is rejected.

```bash
export AGENTHUB_HOME=/tmp/agenthub-route-runtime-probe/manual/home
mkdir -p "$AGENTHUB_HOME"/{config,run,logs}
# write config/probe.json (synthetic key + loopback upstream) before probe Start
./bin/agenthub-adapterd run --home "$AGENTHUB_HOME" --listen-port 18765
```

Status replies must not include the entry key or upstream credentials.
Application-managed runs use `--runtime-config-stdin-stream` and provide one or
more length-framed `route-config.v1-usage-spool` documents on stdin. Each frame is
a four-byte big-endian length followed by the exact JSON bytes. A valid update
atomically replaces the complete edge table without changing the process or
listen port; requests already in flight retain the table they selected. A
complete but invalid JSON/schema frame is rejected while the last good table
keeps serving. EOF, truncation, and oversize frames terminate the runtime
instead of leaving an unsupervised stale configuration. Status exposes only an
opaque SHA-256 acknowledgement and revision, never the configuration itself.
Each edge keeps its required primary entry Key in `ingress_key`; optional
additional entry Keys use `ingress_keys`. Repeated Keys on one edge are
deduplicated, while assigning any Key to multiple edges rejects the complete
configuration. Request authentication still requires the edge's exact surface.
The one-shot `--runtime-config-stdin` mode remains available to existing
isolated probes. Both interfaces are internal and are not public configuration
formats.

## Out of scope

Non-allowlisted external upstreams, Codex/Grok official-login external routes,
live/default gateway cutover, real
`~/.agenthub` reads/writes, combined desktop-to-Agent configuration
write/recovery, Windows desktop supervisor integration and real execution,
plugin SDK/ABI, and stages E/F.
Eligible Go members can request an official-login refresh from the desktop
controller and retry once, but the isolated probes use synthetic logins and do
not call a real external service or use a real API Key. The external-policy
probe validates only configuration acceptance and local process lifecycle.
