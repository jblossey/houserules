# Batch 22 plan: kit stewardship

Spec: docs/specs/2026-09-11-batch-22-stewardship.md (approved
2026-09-11, design.md 5.61 — the three gate rulings included).
Items: HR-105, HR-106, HR-099, HR-089.
Workspace: .superpowers/sdd/2026-09-11-batch-22/.
Order: T1 archive → T2 update model → T3 comment sweep (last,
so the sweep covers T1/T2's new code). Implementers on sonnet,
task reviews on opus, branch review on fable, strictly
sequential. No agent-template or eval change: no evals rerun.

## Code-health scan (process.code-health-scan)

Files the batch touches, smells found:

- `src/install.rs` (875 lines): one module carries the payload
  tables, init, update, files, and their CLI arms — already at
  the strain point, and T2 adds baselines and override logic.
  Fold: T2 puts the baseline/override machinery in a new
  `src/baseline.rs` with one responsibility (stamp, compare,
  report); install.rs calls it.
- `src/rules/model.rs` `load_base` and `src/backlog/load.rs`:
  clean flat reads; the archive split needs no change here —
  only `get`'s resolution path grows. No smell.
- `src/main.rs` module doc and most module docs repo-wide are
  batch-by-batch changelogs; `rules/audit.rs:844-863` is a
  20-line inline essay duplicated at :2631; five comments cite
  commit SHAs, two cite CI run ids (the survey's full list:
  the workspace's comment-survey.md). Fold: this is T3's whole
  scope.
- `tests/install.rs` pins install.rs's KIT_OWNED/SEED_ONCE
  lists by copy — T2 touches update semantics, so the twin
  pins must move with it; the task brief names them.
- knowledge JSON topic files are hand-ordered; the T1 sweep
  must preserve entry order and `emit` formatting so diffs
  stay reviewable (reuse `crate::emit::emit`, the HR-076
  lesson).

## T1 — HR-105: the archive

Deliverables:
1. `knowledge/schema.json` (+ template copy): optional entry
   field `status`: `active` | `superseded` | `retired`
   (absent = active).
2. New `src/archive.rs` + `houserules archive` in main.rs:
   moves backlog items with status `done`/`dropped` to
   `backlog/archive/<same-file-name>`, done batch entries to
   `backlog/archive/batches.json`, knowledge entries with
   non-active status to `knowledge/archive/<topic>.json`.
   Idempotent; one line per move + a summary count; writes
   through `crate::emit::emit`; `decisions.json` untouched.
3. Reader split: `get` resolves archived ids and labels them
   archived (`see` links keep resolving); `list`, `index`,
   `topics`, `for`, `standing`, render, and both check gates
   read the active set only; both checks schema-validate
   archive files so corruption is loud.
4. `finishing-a-feature` close-out step (edit
   `template/.claude/skills/finishing-a-feature/SKILL.md`,
   then `houserules update --dir .`).
5. First live sweep of this repository ON A SEEDED SCRATCH
   COPY as the live run; the real repo's first sweep runs at
   this batch's close, after merge.

TDD: failing test first per behavior — archive moves, archive
idempotence, get-resolves-archived, checks-skip-archived-
liveness, checks-validate-archive-schema, emptied-area
dead-glob interplay (the sweep reports, the gate still fires).
Gates: full suite, cross-target check, both checks, render
--check, residue gate — all green at task end.

## T2 — HR-099 + HR-089: the downstream update model

Deliverables (spec §4, as ruled):
1. `sha2` exact-pinned via `cargo add sha2@=<current>`;
   vetting recorded in `dependency_vetting`; cargo deny green.
2. New `src/baseline.rs`: stamp/compare/report for the
   `baselines` map in `.houserules.json` (KIT_OWNED files +
   kit-shipped knowledge entry ids; hashes computed after any
   `--id-prefix` rewrite).
3. `update` passes: KIT_OWNED at-baseline → overwrite +
   restamp; modified → kept + one report line; `overrides`
   (hand-edited list in `.houserules.json`, preserved across
   restamps) → kept silently. SEED_ONCE knowledge topics →
   entry-level upsert by id (at-baseline replaced, modified/
   adopter-authored kept, adopter-deleted respected +
   reported once). SEED_ONCE other files: absent → written
   (HR-089's backfill) unless overridden; present → untouched.
4. Migration: no `baselines` stamped → compare against the
   current payload; equal stamps, different reports; nothing
   overwritten on the first run.
5. install.rs's "seeds once and never touches again" contract
   prose rewritten to the new contract; `tests/install.rs`
   twin pins updated; template README documents `overrides`.

TDD: failing test per pass and per migration arm; live run
per houserules.live-run-recipe covering: fresh init → update
(no-op), modified KIT_OWNED kept, override silence, knowledge
entry upsert, HR-089 backfill, migration first-run. HR-089
ticks done with this task.

## T3 — HR-106: the rule revision + the sweep

1. First commit: revise `writing-style.code-comments` and
   `writing-style.doc-comments` (both knowledge copies) to the
   §3 ruled wordings; `houserules render`; commit generated
   files with it.
2. Then three sequential implementer dispatches, file lists
   and hit counts from the workspace's comment-survey.md:
   - T3a: src core — the seven changelog module docs
     (report_claims, install, schema_pin, residue-gate is a
     bin but rides T3b, check_commit, gen-goldens rides T3b,
     audit) plus rules/check.rs, rules/audit.rs,
     rules/validate_deliverable.rs, rules/glob.rs,
     rules/read.rs, main.rs, report_claims.rs, install.rs.
   - T3b: bins (residue-gate, gen-goldens,
     find-shell-tool-refs) + remaining src modules (backlog/*,
     rules/*, get, emit, root, node_path, schema_pin).
   - T3c: tests/** (the five "reconstructed" paragraphs in
     backlog_parity.rs included) + tests/common.
3. Keep/drop line: spec §3 verbatim in every dispatch. Doc
   comments are claims: every rewritten doc comment
   re-verified against the code it describes.
4. Gate per dispatch: full suite + clippy + cross-target check
   green; the reviewer confirms the diff touches comments
   only (allowing whitespace the comment removal frees).

## Dispatch mechanics

- Every dispatch: BASE, Backlog, Knowledge (5-10 ids from
  `houserules for` + procedure ids), REPORT_FILE in the
  workspace; reviews add HEAD, REVIEW_FILE, AUDIT_JSON
  (`--report`; branch review `--workspace`).
- Controller gates run through the tree binary once render
  output changes in-branch; reinstall after merge.
- Live run before any PR/CI spend; finish via
  finishing-a-feature: rebase onto main first, aggregate to
  1-5 clean commits with tree-identity verification, five
  checks green on the PR head, ff-only merge from the CLI.
