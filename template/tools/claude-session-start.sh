#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# Copyright (c) 2026 Jannis Blossey
# Claude Code SessionStart hook. After a compaction it prints the standing
# rules, so they sit at the recency end of the context. Any other argument
# prints nothing. Usage: claude-session-start.sh compact
set -euo pipefail
if [ "${1:-}" = "compact" ]; then
  echo "Context was compacted. Standing rules:"
  houserules standing
fi
