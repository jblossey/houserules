#!/usr/bin/env bash
# Claude Code SessionStart hook. Prints the recovery ritual; after a
# compaction also the standing rules, so they sit at the recency end of
# the context. Usage: claude-session-start.sh start|compact
set -euo pipefail
mode=${1:-start}
dir=$(cd "$(dirname "$0")" && pwd)
cat <<'TXT'
Session ritual: run `git status --short && git log --oneline -15`, read the in-progress batch (`tools/backlog.sh list --batch <n>`) and the plan ledger when one is in flight, then invoke the `orchestrating` skill before you act.
TXT
if [ "$mode" = "compact" ]; then
  echo "Context was compacted. Re-read the spec and plan in flight. Standing rules:"
  "$dir/kb.sh" standing
fi
