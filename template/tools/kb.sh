#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# Copyright (c) 2026 Jannis Blossey
# Knowledge base CLI wrapper. Uses node from PATH; falls back to mise.
set -euo pipefail
dir=$(cd "$(dirname "$0")" && pwd)
if command -v node >/dev/null 2>&1; then exec node "$dir/kb.mjs" "$@"; fi
exec mise exec node -- node "$dir/kb.mjs" "$@"
