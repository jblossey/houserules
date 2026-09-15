# Batch 27 — the install-and-update UX cluster and the agent-agnostic kit

Items: HR-133 (update self-updates the binary), HR-134 (update-available
header), HR-135 (install path moves to `~/.local/bin`, ruled 5.86),
HR-136 (the kit works with Codex and other non-Claude harnesses).
Kickoff: the owner's direction of 2026-09-15 (design.md 5.86).

## 1. Constraints that shape every task

- **The 1.0 surface freeze holds** (`houserules.1-0-surface-is-frozen`,
  design.md 5.82). This batch adds **no subcommand and no flag**: HR-133
  extends `update`'s behavior under its existing name; HR-134 writes to
  stderr; every opt-out is an environment variable. The 20-name pin test
  stays untouched, and the release stays a minor (1.x).
- **Structured output stays clean.** The update-available header goes to
  stderr only, and only when stderr is a TTY; stdout JSON is never
  polluted.
- **Network calls are bounded.** No command ever blocks on the network
  for the header: a cached check with a TTL, silent on any failure, and
  skipped outright in CI (the seeded knowledge.yml gate must not get
  slower or flakier).

## 2. Research digest (verified 2026-09-15)

- **axoupdater** (axodotdev/axoupdater, v0.10.x, last release within a
  month of writing): cargo-dist's companion updater, usable as a
  library. `AxoUpdater::load_receipt()` reads the install receipt the
  dist shell/PowerShell installers write to `~/.config/houserules/`
  (`%LOCALAPPDATA%\houserules\` on Windows);
  `is_update_needed_sync()` compares against the latest GitHub
  Release; `run_sync()` downloads and self-replaces. **No receipt
  exists for mise or direct-download installs** — the design must
  branch on receipt presence.
- **dist `install-path`**: accepts `"~/.local/bin"` verbatim (also
  `CARGO_HOME`, `$ENV_VAR/subdir`, and since 0.14 an array of
  fallbacks). What a stale receipt does after the configured path
  changes is undocumented — T1 pins it by live run.
- **AGENTS.md**: the cross-tool instruction standard (originated by
  OpenAI, stewarded by the Linux Foundation's Agentic AI Foundation;
  read natively by Codex, Copilot, Cursor, Windsurf, Amp, Zed, Jules
  and more). **Claude Code reads only CLAUDE.md**, and its own docs
  endorse exactly one bridge: a CLAUDE.md that imports AGENTS.md
  (`@AGENTS.md`, expanded at launch, four-hop import depth).
- **`.claude/rules/*.md`** is Claude-Code-native (unconditional load,
  or conditional via `paths:` frontmatter). **Skills** follow the Agent
  Skills open standard (agentskills.io) — plain markdown any harness
  can read as a procedure document. **`.claude/agents`** subagent
  templates are Claude-specific with no cross-tool standard.
- Sources: github.com/axodotdev/axoupdater (README),
  docs.rs/axoupdater, axodotdev.github.io/cargo-dist/book (config
  reference), code.claude.com/docs/en/memory.md, skills.md,
  sub-agents.md; agents.md ecosystem posts, re-checked 2026-09-15.

## 3. Design

### T1 — the install-path move (HR-135)

`dist-workspace.toml` sets `install-path = "~/.local/bin"`; `dist
generate` regenerates release.yml; dist-generate-check keeps them
paired. README's install section and docs/runbook.md drop the
CARGO_HOME wording. The comment block in dist-workspace.toml (which
currently explains staying at CARGO_HOME) flips to record ruling 5.86.

Migration: nothing moves existing binaries. The next release's
installer writes to `~/.local/bin`; a stale `~/.cargo/bin/houserules`
would then shadow or be shadowed by PATH order. T2's `update` detects
that: after a successful self-update (or when skipping one), it scans
PATH for other `houserules` executables at different paths and prints
one warning naming the stale copy to delete. The receipt-vs-new-path
behavior (does axoupdater honor the old receipt path or the new
installer default?) is pinned by T2's live run; the runbook records
the observed behavior.

### T2 — `update` self-updates the binary (HR-133)

`houserules update` gains a self-update phase that runs BEFORE the
repo phase:

1. Receipt exists (shell/PowerShell installer channel): axoupdater
   checks; if newer, `run_sync()` self-replaces, then `update`
   re-executes the NEW binary with the same arguments plus
   `HOUSERULES_SELF_UPDATED=1` in the environment (loop guard), so the
   repo phase runs on the new payload. Output: one line per step.
2. No receipt: print one guidance line naming the channel-appropriate
   command (`mise up houserules`, or re-download) and continue the repo
   phase on the current binary. Never an error.
3. `HOUSERULES_SKIP_SELF_UPDATE=1`, or `CI` set: skip the phase
   silently.

Failure of the self-update phase (network down, GitHub unreachable,
no permissions) degrades to a warning; the repo phase always runs.

Dependency: `axoupdater` as a library, `github` backend only, blocking
API (no tokio); added by `cargo add axoupdater@=<version>` with the
dependency-vetting record in the task report
(`security-hygiene.exact-pins`, `security-hygiene.dependency-vetting`).

### T3 — the update-available header (HR-134)

Every command MAY print, as its first stderr line:
`houserules <current> -> <latest> available; run houserules update`.

Gating, all of which must hold: stderr is a TTY; `CI` unset;
`HOUSERULES_NO_UPDATE_CHECK` unset; the cache says a newer version
exists. The cache (one JSON file under the XDG cache dir) records the
last check time and the latest known version; a check runs at most
once per 24h, in-line but with a short timeout, and any failure just
refreshes the timestamp. Version comparison is semver against
`CARGO_PKG_VERSION` — receipt-free, so it works for mise and
direct-download installs too. The `update` command itself never
prints the header (it acts instead).

Implementation detail: reuse axoupdater's release query if it works
without a receipt; otherwise a minimal GitHub `releases/latest` GET
via a small pinned HTTP client. The task report records which path
was taken and why.

### T4 — the agent-agnostic kit (HR-136)

The kit's core is already harness-neutral: knowledge and backlog are
JSON, the gates are one binary, the commit hook is a plain git hook.
What is Claude-bound is the LOADING PATH: CLAUDE.md,
`.claude/rules/*`, skills, agents, and the SessionStart hook in
`.claude/settings.json`. The design makes AGENTS.md the canonical
instruction surface and keeps the `.claude/*` files as Claude-native
acceleration of the same content:

1. **`template/AGENTS.md` (new, SEED_ONCE)**: the canonical project
   instructions — today's template CLAUDE.md content, rephrased
   harness-neutrally ("your agent", not "Claude"), plus a section any
   harness can follow: run `houserules standing` at session start,
   `houserules for <paths>` before editing, the check gates, and where
   the procedure docs live (`.claude/skills/*/SKILL.md` are plain
   markdown — read `finishing-a-feature` before merging). Seed-once,
   so an adopter's existing AGENTS.md is never overwritten; the
   migrating-knowledge skill gains a step for merging the houserules
   section into an existing AGENTS.md by hand.
2. **`template/CLAUDE.md` becomes the one-line pointer** `@AGENTS.md`
   (the pattern Claude Code's own docs endorse). Claude Code loads the
   same canonical text; `.claude/rules/*` still gives Claude its
   conditional area loading on top. Existing adopters keep their
   CLAUDE.md (seed-once); nothing breaks.
3. **Sweep the seeded texts for Claude-only phrasing**: skills,
   knowledge entries the template ships, and docs/README wording that
   says "Claude" where "your agent" is meant. `.claude/agents`
   templates and the settings.json hook stay as documented
   Claude-specific extras — AGENTS.md names them as optional.
4. **This repository adopts its own medicine**: root AGENTS.md +
   pointer CLAUDE.md via `houserules update --dir .` (the repo's
   CLAUDE.md carries no custom content beyond the template's, verified
   at implementation; if drift exists, fold it into AGENTS.md).

Template payload changes throughout T4 → payload.stamp regeneration
in the same commits (`houserules.payload-stamp-gate`); the template
edit makes this a `feat(template)` batch, so release-please proposes
a minor — consistent with section 1's no-breaking constraint.

## 4. Task order and pipeline

Standard pipeline resumes (5.84 was batch-26-only): implementer
(sonnet/xhigh) → task-reviewer (opus/high) per task, branch-reviewer
(fable/high) at the end, strictly sequential. Order: T1 → T2 → T3 →
T4. T2 depends on T1 (the migration warning names the new path); T3
reuses T2's dependency decision; T4 is independent but last, as the
largest template edit. TDD per task; T1/T4 live-run via a scratch
`init` and this repository's own `update`; T2/T3 live-run against the
real latest release (read-only checks; the self-replace run uses a
scratch install of the previous release).

## 5. Open points folded into the gate

- The spec recommends axoupdater-as-library for both HR-133 and
  HR-134's network path; the owner rules the dependency choice at this
  gate (`quality.well-maintained-libraries`).
- No new flags or subcommands anywhere (section 1); if a task finds it
  cannot deliver without one, it stops and the question comes back as
  a ruling request rather than a 2.0 surprise.
- HR-134's header wording above is the proposed contract; amendments
  at the gate are free, later ones cost a task round.

## 6. Approval

Approved by the owner as written, 2026-09-15, including the three
embedded recommendations (axoupdater-as-library; the CLAUDE.md
pointer; no new flags or subcommands).
