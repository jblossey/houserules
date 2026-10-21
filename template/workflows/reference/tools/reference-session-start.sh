#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# Copyright (c) 2026 Jannis Blossey
# Claude Code SessionStart hook of the reference workflow. Prints the
# recovery ritual. Usage: reference-session-start.sh
set -euo pipefail
cat <<'TXT'
Session ritual: run `git status --short && git log --oneline -15`, run the start command of the Session section in the houserules block of `AGENTS.md`, read the plan ledger when a plan is in flight, then read `.claude/skills/orchestrating/SKILL.md` before you act.
TXT
