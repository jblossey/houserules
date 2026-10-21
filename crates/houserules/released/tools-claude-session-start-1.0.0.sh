#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# Copyright (c) 2026 Jannis Blossey
# Claude Code SessionStart hook. Prints the recovery ritual; after a
# compaction also the standing rules, so they sit at the recency end of
# the context. Usage: claude-session-start.sh start|compact
set -euo pipefail
mode=${1:-start}
cat <<'TXT'
Session ritual: run `git status --short && git log --oneline -15`, read the in-progress batch (`houserules list --batch <n>`) and the plan ledger when one is in flight, then invoke the `orchestrating` skill before you act.
TXT
if [ "$mode" = "compact" ]; then
  echo "Context was compacted. Re-read the spec and plan in flight. Standing rules:"
  houserules standing
fi
