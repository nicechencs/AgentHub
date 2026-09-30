#!/usr/bin/env bash
# Scriptable /v1/messages preflight: models + two-round chat.
# Helps Test verify non-stream (default) or stream multi-round continuity.
#
# Usage:
#   AGENTHUB_BASE=http://127.0.0.1:44227 \
#   AGENTHUB_TOKEN=ahb_... \
#   AGENTHUB_MODEL=grok-4.6 \
#     scripts/route-messages-preflight.sh
#
# Optional:
#   AGENTHUB_STREAM=1          SSE rounds instead of JSON
#   AGENTHUB_TIMEOUT_S=60
#
# Prints JSON to stdout. Token is never echoed; last4 only.

set -euo pipefail

BASE="${AGENTHUB_BASE:-}"
TOKEN="${AGENTHUB_TOKEN:-}"
MODEL="${AGENTHUB_MODEL:-}"
STREAM="${AGENTHUB_STREAM:-0}"
TIMEOUT_S="${AGENTHUB_TIMEOUT_S:-60}"

usage() {
  cat <<'EOF'
Usage: AGENTHUB_BASE=http://127.0.0.1:PORT AGENTHUB_TOKEN=... AGENTHUB_MODEL=... \
  scripts/route-messages-preflight.sh

Optional: AGENTHUB_STREAM=1 AGENTHUB_TIMEOUT_S=60
EOF
}

if [[ -z "$BASE" || -z "$TOKEN" || -z "$MODEL" ]]; then
  usage >&2
  exit 2
fi

last4() {
  local value="$1"
  local len=${#value}
  if (( len < 4 )); then
    printf ''
    return
  fi
  printf '%s' "${value:len-4}"
}

TOKEN_LAST4="$(last4 "$TOKEN")"
WORKDIR="$(mktemp -d)"
trap 'rm -rf "$WORKDIR"' EXIT

assistant_from_json() {
  python3 - "$1" <<'PY'
import json, sys
path = sys.argv[1]
with open(path, encoding="utf-8") as fh:
    raw = fh.read()
try:
    data = json.loads(raw)
except json.JSONDecodeError:
    print("")
    raise SystemExit(0)
content = data.get("content")
if isinstance(content, list):
    texts = []
    for part in content:
        if isinstance(part, dict) and part.get("type") == "text":
            texts.append(str(part.get("text") or ""))
        elif isinstance(part, str):
            texts.append(part)
    print("".join(texts))
    raise SystemExit(0)
if isinstance(content, str):
    print(content)
    raise SystemExit(0)
print("")
PY
}

assistant_from_sse() {
  python3 - "$1" <<'PY'
import json, sys
path = sys.argv[1]
text = []
with open(path, encoding="utf-8") as fh:
    for line in fh:
        line = line.strip()
        if not line.startswith("data:"):
            continue
        payload = line[5:].strip()
        if not payload or payload == "[DONE]":
            continue
        try:
            data = json.loads(payload)
        except json.JSONDecodeError:
            continue
        delta = data.get("delta")
        if isinstance(delta, dict) and isinstance(delta.get("text"), str):
            text.append(delta["text"])
            continue
        if data.get("type") == "content_block_delta":
            inner = data.get("delta") or {}
            if isinstance(inner, dict) and isinstance(inner.get("text"), str):
                text.append(inner["text"])
print("".join(text))
PY
}

http_code_of() {
  local file="$1"
  if [[ -f "$file" ]]; then
    tr -d '\r' < "$file" | head -n 1 | awk '{print $2}'
  else
    printf ''
  fi
}

auth_header="Authorization: Bearer ${TOKEN}"
models_body="$WORKDIR/models.json"
models_hdr="$WORKDIR/models.hdr"
curl -sS --max-time "$TIMEOUT_S" \
  -D "$models_hdr" \
  -o "$models_body" \
  -H "$auth_header" \
  "$BASE/v1/models" || true
models_http="$(http_code_of "$models_hdr")"

round() {
  local n="$1"
  local payload="$2"
  local body="$WORKDIR/r${n}.body"
  local hdr="$WORKDIR/r${n}.hdr"
  local extra=()
  if [[ "$STREAM" == "1" ]]; then
    extra+=(-H "Accept: text/event-stream")
  fi
  curl -sS --max-time "$TIMEOUT_S" \
    -D "$hdr" \
    -o "$body" \
    -H "$auth_header" \
    -H "Content-Type: application/json" \
    -H "anthropic-version: 2023-06-01" \
    "${extra[@]}" \
    -d "$payload" \
    "$BASE/v1/messages" || true
}

if [[ "$STREAM" == "1" ]]; then
  stream_json=true
else
  stream_json=false
fi

round 1 "$(python3 - "$MODEL" "$stream_json" <<'PY'
import json, sys
model, stream = sys.argv[1], sys.argv[2] == "true"
print(json.dumps({
    "model": model,
    "max_tokens": 64,
    "stream": stream,
    "messages": [{"role": "user", "content": "Reply with the exact token OK1 and nothing else."}],
}))
PY
)"

r1_http="$(http_code_of "$WORKDIR/r1.hdr")"
if [[ "$STREAM" == "1" ]]; then
  r1_text="$(assistant_from_sse "$WORKDIR/r1.body")"
else
  r1_text="$(assistant_from_json "$WORKDIR/r1.body")"
fi

round 2 "$(python3 - "$MODEL" "$stream_json" "$r1_text" <<'PY'
import json, sys
model, stream, prior = sys.argv[1], sys.argv[2] == "true", sys.argv[3]
print(json.dumps({
    "model": model,
    "max_tokens": 64,
    "stream": stream,
    "messages": [
        {"role": "user", "content": "Reply with the exact token OK1 and nothing else."},
        {"role": "assistant", "content": prior or "OK1"},
        {"role": "user", "content": "Reply with OK2 then the previous token."},
    ],
}))
PY
)"

r2_http="$(http_code_of "$WORKDIR/r2.hdr")"
if [[ "$STREAM" == "1" ]]; then
  r2_text="$(assistant_from_sse "$WORKDIR/r2.body")"
else
  r2_text="$(assistant_from_json "$WORKDIR/r2.body")"
fi

python3 - "$models_http" "$r1_http" "$r2_http" "$TOKEN_LAST4" "$MODEL" "$STREAM" "$r1_text" "$r2_text" <<'PY'
import json, sys
models_http, r1_http, r2_http, last4, model, stream, r1, r2 = sys.argv[1:]
def code(value):
    try:
        return int(value)
    except ValueError:
        return 0
out = {
    "models_http": code(models_http),
    "round1_http": code(r1_http),
    "round2_http": code(r2_http),
    "model": model,
    "stream": stream == "1",
    "key_last4": last4,
    "round1_text": r1,
    "round2_text": r2,
    "ok": code(models_http) == 200 and code(r1_http) == 200 and code(r2_http) == 200
        and "OK1" in r1 and "OK2" in r2,
}
print(json.dumps(out, ensure_ascii=False))
raise SystemExit(0 if out["ok"] else 1)
PY
