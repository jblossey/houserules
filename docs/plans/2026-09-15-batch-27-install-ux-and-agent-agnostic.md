# Batch 27 plan — install-and-update UX and the agent-agnostic kit

Spec: docs/specs/2026-09-15-batch-27-install-ux-and-agent-agnostic.md
(approved 2026-09-15, section 6). Branch: batch-27. Pipeline: standard
(implementer sonnet → task-reviewer opus per task; branch-reviewer
fable at the end; strictly sequential). Workspace:
`.superpowers/sdd/2026-09-15-batch-27/`.

## Code-health scan (process.code-health-scan)

- `crates/houserules/src/install.rs` (1328 lines): already a
  multi-responsibility module (seed, update, baselines, retirement).
  Smell: size and mixed concerns. Targeted fix: T2 and T3 put the
  self-update phase and the version-check cache in their own modules
  (`selfupdate.rs`, `update_check.rs`), each with its own tests;
  `cmd_update` only orchestrates. No growth of install.rs beyond the
  SEED_ONCE list edit T4 needs.
- `crates/houserules/src/main.rs` (552 lines): dispatch is clean; the
  T3 header hook is one guarded call before the match, nothing more.
- `dist-workspace.toml`: comment block explains the CARGO_HOME choice
  that ruling 5.86 supersedes — flip it with the setting (T1), never
  leave a stale rationale.
- `template/CLAUDE.md` and the three template skills: prose is
  Claude-addressed throughout; T4's sweep is enumerable (grep -ri
  claude over template/ minus .claude paths that stay Claude-native).
- Seed-test fixture `make_seed_repo` (check.rs) pins init-written
  files; T4 adds AGENTS.md there in the same commit as the seed list
  change, or the dead-glob/seed tests drift (batch-26 lesson).

## Tasks

### T1 — install path to ~/.local/bin (HR-135)
Files: dist-workspace.toml, .github/workflows/release.yml (via `dist
generate`), README.md (install + uninstall wording), docs/runbook.md.
TDD: dist-generate-check is the standing proof for the workflow pair;
a config-shape pin follows release_please_config.rs's pattern if one
exists for dist-workspace.toml (add the install-path assertion to the
existing pin test file if present, else a small new test). Live run:
`mise run lint` (dist-generate-check green) — installer behavior
itself is release-gated and lands in T2's live run.

### T2 — update self-updates the binary (HR-133)
Files: new crates/houserules/src/selfupdate.rs, install.rs
(`cmd_update` orchestration only), Cargo.toml/lock (axoupdater,
`cargo add axoupdater@=<version>`, github backend only, blocking),
docs/runbook.md (observed receipt-vs-path behavior). Spec section 3
T2 is the contract: receipt→check→run_sync→re-exec with
HOUSERULES_SELF_UPDATED loop guard; no receipt→one guidance line;
CI/env skip; failures degrade to warnings; stale-copy PATH scan
warning. TDD: env/receipt branching and the PATH scan are unit-tested
with injected paths; the network path is behind a trait so tests
never touch GitHub. Live run: scratch install of the previous release
via the real installer, then `houserules update` self-updates it to
latest (captured); mise/no-receipt fallback captured on this machine.

### T3 — update-available header (HR-134)
Files: new crates/houserules/src/update_check.rs, main.rs (one call),
README (one sentence under Updating naming the header and
HOUSERULES_NO_UPDATE_CHECK). Contract: spec section 3 T3 (TTY + not
CI + env unset + cache; 24h TTL; silent failures; never on `update`;
stderr only). TDD: cache read/write, TTL, gating matrix unit-tested;
network behind the same trait as T2. Live run: real check against the
latest release captured once with cache file shown before/after.

### T4 — the agent-agnostic kit (HR-136)
Files: template/AGENTS.md (new), template/CLAUDE.md (pointer),
template skills sweep, install.rs SEED_ONCE (+AGENTS.md), check.rs
make_seed_repo fixture, payload.stamp, this repo's own AGENTS.md +
CLAUDE.md via tree-binary `update --dir .`, knowledge entry for the
canonical-instructions design, README "What it installs" row.
TDD: seed test proves a fresh seed carries AGENTS.md and passes
check_base; an update-backfill test proves an absent AGENTS.md is
backfilled and a present one kept. Live run: scratch `init` shows
AGENTS.md + pointer CLAUDE.md; this repository runs one session-start
sanity read (standing rules still load).

## Gates per task
fmt, clippy -D warnings, cargo test --locked, tree-binary
check-knowledge/check-backlog/render --check, check-commit --from
merge-base, mise run lint. Branch end: finishing-a-feature, 8 checks,
ff-only merge, main push before branch deletion.
