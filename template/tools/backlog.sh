#!/usr/bin/env bash
# Backlog CLI wrapper. Uses node from PATH; falls back to mise.
set -euo pipefail
dir=$(cd "$(dirname "$0")" && pwd)
if command -v node >/dev/null 2>&1; then exec node "$dir/backlog.mjs" "$@"; fi
exec mise exec node -- node "$dir/backlog.mjs" "$@"
