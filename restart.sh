#!/usr/bin/env bash
# Stop leftover AgentHub desktop/dev processes, then start ./run.sh.
set -euo pipefail
SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
exec "$SCRIPT_DIR/run.sh" --restart "$@"
