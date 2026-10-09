# Batch 31 plan — process cost

> **For agentic workers:** dispatch through `.claude/agents/` (`implementer`,
> `task-reviewer`, `branch-reviewer`) per `.claude/skills/orchestrating/SKILL.md`.

**Goal:** tiers, severity by impact, a bounded review loop, parallel
agents on disjoint files, and a self-optimizing knowledge base fed by an
extended `houserules stats`, shipped to adopters through `update`.

**Spec:** docs/specs/2026-10-09-batch-31-process-cost.md (approved
2026-10-09). Branch: batch-31. Workspace:
`.superpowers/sdd/2026-10-09-batch-31/` (absolute path in every dispatch).

**Tier: full** (criterion: the batch changes `crates/houserules/src/**`
and `template/**`, a public contract every adopter runs). Re-review cap: 2
per task.

**Parallelism (design.md 5.92, applied from this batch on):** T1 and T2
are parallel. Their file sets are disjoint (below); neither needs the
other's output except the `stats` key names, which the Interfaces blocks
fix. Each runs in its own worktree from the same `BASE`. The controller
integrates T1, then T2, and runs the full gates after each. T3 and T4 run
after both close.

## Global constraints

- No subcommand, no flag, no schema file change. `stats` widens its
  positional to `<WORKSPACE>...` only (design.md 5.91). Release: 1.3.0
  (a `feat` commit; release-please computes it).
- No gate becomes stricter: `check-knowledge` budgets, `audit`, and
  `validate` keep their behavior (spec 2.4, 3).
- No standing entry's `summary` changes in `template/knowledge/`.
- New kit entries are not standing.
- Edit the kit in `template/`, then `houserules update --dir .` with the
  tree binary (`houserules.template-is-the-source`,
  `houserules.controller-gates-use-the-tree-binary`); regenerate
  `crates/houserules/payload.stamp` (`houserules.payload-stamp-gate`).
- Seeded text stays harness-neutral (`houserules.agents-md-is-canonical`).
- Writing style: ASD-STE100 (`writing-style.principles`).

## Review focus

1. A workspace path with a trailing slash (`.superpowers/sdd/x/`, as a
   shell glob yields): the task label prefix is `x`, never empty. Test in
   T1.
2. The same workspace given twice (`a a`, or `a ./a`): read once. Test in
   T1.
3. A missing path or a file instead of a directory: one named error that
   names the path, exit 2 (`houserules.crash-paths-are-named`). Test in
   T1.
4. A directory with no deliverables among the workspaces: a zero row,
   no error. Test in T1.
5. Old review shapes: an issue with no `rule`, no `severity`, or no
   `file`: counted under `other`, never a crash. Test in T1.

## Code-health scan (`process.code-health-scan`)

- `crates/houserules/src/rules/stats.rs` (395 lines, one function
  `stats` of about 70 lines): it mixes file discovery, parsing, and
  aggregation in one function. The new keys must not grow it: the cost
  profile goes into a new `rules/cost.rs`, the per-rule rows and
  proposals into a new `rules/proposals.rs`; `stats.rs` keeps the
  existing keys and composes the three.
- `stats` takes a `dir` and `cmd_stats` loads the knowledge base only to
  discard it. The proposals need the base: pass `&Base` into the new
  code instead of loading it twice.
- `template/.claude/skills/orchestrating/SKILL.md` "Handling reviews"
  holds 13 bullets with no order; the new loop bullets go next to the
  severity bullet they change, and the batch-close bullets stay
  together.
- `template/.claude/agents/task-reviewer.md` states severity twice (step
  4 and Calibration) with two different rules. T2 states it once, in
  Calibration, and step 4 points to it.
- `template/.claude/agents/branch-reviewer.md` restates the severity rule
  a third time; it points to the same entry.

---

### Task 1: `stats` reads several workspaces and measures cost (HR-161)

**Files:**
- Modify: `crates/houserules/src/main.rs` (`Command::Stats`, the dispatch
  arm)
- Modify: `crates/houserules/src/rules/stats.rs`,
  `crates/houserules/src/rules/mod.rs`
- Create: `crates/houserules/src/rules/cost.rs`,
  `crates/houserules/src/rules/proposals.rs`
- Modify: `tests/goldens/stats/*.json` (regenerated with
  `cargo run --bin gen-goldens`; see step 6)
- Create: fixture workspaces under `tests/fixtures/stats-cost/` if an
  integration test needs files on disk; unit tests build theirs in a
  `TempDir`
- Modify: `README.md` (the `stats` row and a short "Measure process cost"
  section)
- Not touched: `template/**`, `knowledge/**`, `.claude/**`, `AGENTS.md`,
  `crates/houserules/payload.stamp`, `backlog/**` (T2 and the controller
  own them)

**Interfaces:**
- Produces (CLI): `houserules stats <WORKSPACE>... [--dir <DIR>]`.
  Clap: `workspaces: Vec<PathBuf>` with `required = true`.
- Produces (JSON, consumed by T2's prose): top-level keys `violations`,
  `unused_ids`, `audits`, `reviews` (as today), then `workspaces`,
  `cost`, `rules`, `proposals`. A proposal row:
  `{"action", "id", "owner_gate", "reason", "evidence"}`. Actions:
  `demote`, `retire`, `mechanize`, `narrow`, `budget`.
- Produces (Rust): `pub(super) fn stats(dirs: &[PathBuf], base: &Base)
  -> Result<Value, String>`; `cost::profile`, `cost::target`,
  `proposals::rule_rows`, `proposals::proposals` (names are the
  implementer's to refine; keep one responsibility per module).

**Semantics (spec 2.4, exact):**
- Task label: the existing `stats_task` value for one workspace; for two
  or more, `<last path component>/<task>`. Normalize each path first
  (strip a trailing separator; read a path that canonicalizes to one
  already read only once).
- `stats` also reads `branch-review.json` in each workspace: its `issues`
  and `rule_adherence`. A `re-review` deliverable's findings are its
  `new_breakage` rows. `violations` keeps today's inputs only, so its
  values do not change for one workspace.
- Row of `workspaces`: `path` (as given), `tasks` (distinct task labels
  over `task-*-report.json`), `reviews` (task review files plus
  `branch-review.json`), `re_reviews` (review files whose `kind` is
  `re-review`), `fix_rounds` (sum of the reports' `fix_rounds` lengths),
  `report_bytes` (sum of the report file sizes), `findings`
  (`critical`, `important`, `minor`, `other`), `targets` (`code`,
  `deliverable`, `prose`, `other`).
- Target of a finding, from `file` (strip a `:<line>` suffix first):
  absent or not a string: `other`; contains the workspace's last path
  component, or its file name matches `*-report.json`, `*-review*.json`,
  or `*-audit*.json`: `deliverable`; ends in `.md`, or starts with
  `docs/`, `knowledge/`, or `backlog/`, or contains `commit` (any case):
  `prose`; otherwise `code`.
- `cost`: `totals` (the sums of the `workspaces` rows) and `per_task`
  (each total divided by the total `tasks`, rounded to two decimals; the
  token `"none"` when `tasks` is 0, `quality.absence-is-designed`).
- `rules`: one row per entry of the knowledge base whose raw `status` is
  absent or `active`, sorted by id: `id`, `standing`, `mode`
  (`deterministic` when the entry's `check` is `CheckField::Valid`, else
  `judged`), `workspaces` (count where an audit's `rules` or a review's
  `rule_adherence` holds the id), `injected` (count of workspaces where an
  audit's `ids` holds it), `rows`, `fails` (rows with `result: fail`),
  `findings` (by severity, as above), `targets`, `cited` (reports whose
  `knowledge_used` names it).
- `proposals`, sorted by action in the table order, then id; an entry may
  get more than one:

  | action | condition |
  |---|---|
  | `demote` | `standing`; `workspaces >= 3`; `fails == 0`; no findings |
  | `retire` | not `standing`; `injected >= 3`; `fails == 0`; no findings; `cited == 0` |
  | `mechanize` | `mode == judged`; fails or findings in 2 or more workspaces |
  | `narrow` | 3 or more findings; at least two thirds of them `minor` or with target `deliverable` or `prose` |
  | `budget` | more than 25 standing entries in the base; `id` is `"standing"`; `evidence` holds `standing`, `budget` (25), and `candidates` (standing ids by ascending catch rate `(fails + findings) / max(rows, 1)`, then id) |

- `owner_gate`: true when the entry is standing or its raw
  `source.by` is `user`; always true for `budget`.
- `evidence`: the numbers the condition read (`workspaces`, `injected`,
  `rows`, `fails`, `findings`, `cited`, as applicable).
- An id in a deliverable that the base does not hold gets no `rules` row
  and no proposal; it still counts in `violations` and `unused_ids`.
- The thresholds (3, 3, 2, 3, two thirds, 25) are named constants with a
  doc comment each.

- [ ] **Step 1: Failing tests first.** In `stats.rs`'s test module (and
  the new modules' own), using the existing pattern of
  `aggregates_violations_unused_ids_and_file_counts` (deliverables
  written into a `TempDir`, `serde_json::json!` assertions) and a minimal
  knowledge base built in a `TempDir` the way other `rules::` tests build
  one (find a `load_base` call in a test of `audit.rs` or `check.rs` and
  follow it). One test per behavior:
  - one workspace: the four old keys equal their values from today's
    `stats(dir)` for the same files;
  - two workspaces: task labels carry the prefix; review focus 1 and 2;
  - each `workspaces` field; each target class including `other`; review
    focus 5;
  - `cost.totals`, `cost.per_task`, and `"none"` at zero tasks;
  - every `rules` field; a retired entry has no row; an unknown id has no
    row;
  - each proposal action at its threshold and one below it; `owner_gate`
    for standing, for `source.by: user`, and false otherwise;
  - `branch-review.json` and `new_breakage` counted;
  - review focus 3 and 4 at the command level (`cmd_stats` or an
    integration test in `crates/houserules/tests/`).
- [ ] **Step 2: Run them and capture RED** into the workspace
  (`cargo test -p houserules --bin houserules stats > <ws>/t1-red.txt
  2>&1`); expected: compile errors or assertion failures naming the new
  keys.
- [ ] **Step 3: Implement** `cost.rs`, `proposals.rs`, the `stats`
  composition, the variadic positional, and `cmd_stats` passing `&Base`.
- [ ] **Step 4: GREEN** for the focused tests, then the full suite
  (`cargo test --workspace`).
- [ ] **Step 5: CLI help.** The `Stats` doc comment and the positional's
  doc name one or more workspaces. Run the help tests
  (`cargo test --bin houserules no_subcommand_carries_a_long_about_beyond_its_short_one`).
- [ ] **Step 6: Goldens.** `tests/goldens/stats/*.json` change by the new
  keys only. Prove it before regenerating: a scratch check (kept in the
  workspace, not the tree) that the old golden's four keys equal the new
  output's four keys byte for byte. Then regenerate with the existing
  generator (`cargo run --bin gen-goldens -- --help` names its use) and
  run `cargo test --test validate_stats_audit_parity`.
- [ ] **Step 7: README.** Update the `stats` row in the command table and
  add a short section that shows
  `houserules stats .superpowers/sdd/*/` and names the `proposals` key
  and its five actions. No process history
  (`writing-style.readme-is-an-adopter-funnel`).
- [ ] **Step 8: Gates.** `cargo fmt --check`, `cargo clippy --workspace
  --all-targets -- -D warnings`, `cargo test --workspace`,
  `mise run lint`. Live run per `houserules.live-run-recipe`: a scratch
  `git init`, `houserules init --dir <scratch>`, `check-knowledge` and
  `check-backlog` inside it, then `houserules stats` over the scratch
  workspace and over `.superpowers/sdd/2026-10-01-batch-28
  .superpowers/sdd/2026-10-02-batch-30` of this repository (read only),
  each captured `> <file> 2>&1` with its exit.
- [ ] **Step 9: Commit** (`feat(cli): stats reads several workspaces and
  proposes knowledge changes from measured cost`), RED and GREEN in the
  body.

### Task 2: the kit's tiers, severity, loop, parallelism, and batch-close loop (HR-159, HR-140, HR-160)

**Files:**
- Modify: `template/.claude/skills/orchestrating/SKILL.md`,
  `template/.claude/agents/task-reviewer.md`,
  `template/.claude/agents/branch-reviewer.md`,
  `template/.claude/skills/migrating-knowledge/SKILL.md`,
  `template/AGENTS.md`, `template/docs/README.md`,
  `template/knowledge/process.json`
- Modify (this repository's adoption): `knowledge/houserules.json`,
  `knowledge/process.json` (via `update` plus the supersession),
  root `AGENTS.md`
- Generated: root copies through `./target/debug/houserules update --dir .`,
  `.claude/rules/*.md` and the generated skill through `render`,
  `crates/houserules/payload.stamp` through
  `cargo run --bin payload-stamp-gate -- --write`
- Not touched: `crates/houserules/src/**`, `tests/**`, `README.md`
  (T1 owns them)

**Interfaces:**
- Consumes: the `stats` CLI and keys of Task 1 (above). Prose names
  `houserules stats <WORKSPACE>...`, `proposals`, `owner_gate`, and the
  five actions exactly.
- Produces: kit entry ids `process.risk-tiers`,
  `process.severity-by-impact`, `process.bounded-review-loop`,
  `process.knowledge-earns-its-place`,
  `process.parallel-on-disjoint-files`; the tag `full-tier`.

- [ ] **Step 1: New kit entries** in `template/knowledge/process.json`,
  each `kind: rule`, `area: process`, `standing: false`, `source`
  `{"date": "2026-10-09", "by": "user", "ref": "owner ruling, batch 31"}`,
  summary one sentence (`knowledge-base.summary-is-the-rule`), body the
  spec's text for its section:
  - `process.risk-tiers` — spec 2.1 (the table as sentences; the
    `full-tier` tag; the plan records tier and criterion; the reviewer
    checks it; up only).
  - `process.severity-by-impact` — spec 2.2 (the three definitions;
    the rule's class does not set severity; the finding keeps `rule`).
  - `process.bounded-review-loop` — spec 2.3.
  - `process.knowledge-earns-its-place` — spec 2.5 steps 1 to 6 and the
    stated limit.
  - `process.parallel-on-disjoint-files` — spec 2.6 bullets 1 to 5.
- [ ] **Step 2: Amended kit entries** (body only; summaries of standing
  entries unchanged): `process.no-tech-debt` (a sentence: severity follows
  `process.severity-by-impact`; a Minor fix closes without a re-review;
  at the cap of `process.bounded-review-loop` the open non-critical
  findings become one backlog item with the reason);
  `process.brainstorm-first` (light tier: the backlog item's body is the
  written spec); `process.code-health-scan` (light tier: the scan is a
  line in the dispatch brief); `process.fix-round-verification-record`
  (a Minor-only round closes with a controller note).
- [ ] **Step 3: Orchestrating skill.**
  - After the lifecycle table, a `## Tiers` section: the spec 2.1 table
    with "the project's owner or decider" for the owner, and "Mark a
    high-risk area with an entry in that area tagged `full-tier`."
  - Replace the paragraph "Dispatch agent work sequentially by default.
    ..." with the spec 2.6 procedure, ending: "An `AGENTS.md` line from
    a kit before 1.3.0 that says to hand off tasks sequentially by
    default states the old kit default, not a project ruling."
  - Dispatch protocol: an implementer dispatch adds
    `Tier: <light|standard|full> (<criterion>)`.
  - Handling reviews: replace "Severity of an adherence failure: standing
    rule is Critical; area rule is Important; `warn` is Minor." with the
    impact rule citing `process.severity-by-impact`; add the bounded
    loop bullet citing `process.bounded-review-loop`; the fix-round
    bullet names the Minor-only form.
  - Batch close: replace the "Retrospective proposals from the branch
    review: ..." bullet with spec 2.5 (the `stats` run over the five most
    recent workspaces saved as `<workspace>/stats.json`; apply
    `owner_gate: false` rows and non-standing retrospective proposals in
    one `docs(knowledge): ...` commit, with the four actions as spec 2.5
    step 3 states them; list `owner_gate: true` rows and standing
    proposals in the batch report; `overrides` for a kit-shipped id; the
    default for new entries).
  - Template evaluation: replace "a `houserules update` that brings new
    templates included" with: commit a `houserules update` on its own,
    outside a batch branch; the kit release carries its own eval run.
- [ ] **Step 4: task-reviewer.** Step 4 of Rule adherence: file every
  `fail` at the severity its impact earns (Calibration); the rule's class
  does not set it. Calibration: the three definitions of spec 2.2, then
  the existing sentences from "Polish is Minor — file it anyway" on. Add
  to the re-review fields paragraph: read the fix diff and the findings
  under verification only; file a new finding at its impact; only a new
  Critical or Important finding reopens the task. Add one line: check the
  `Tier:` line against the orchestrating skill's criteria; a wrong tier is
  an Important finding.
- [ ] **Step 5: branch-reviewer.** "with the standard severities
  (standing rule critical, area rule important, warn minor)" becomes "at
  the severity their impact earns (`process.severity-by-impact`)". The
  retrospective reads `<WORKSPACE>/stats.json` (the controller's run over
  the five most recent workspaces) and runs `houserules stats
  <WORKSPACE>` only when that file is absent; it carries each
  `proposals` row into `recommendations` as one line (action, id,
  owner_gate, reason); a proposed new entry is non-standing with an area
  by default, and a proposed standing entry names the standing entry it
  replaces.
- [ ] **Step 6: migrating-knowledge and template AGENTS.md.** The
  agent-parallelism question names the new default (parallel on disjoint
  file sets, each in its own worktree; sequential otherwise) and the
  closing sentence's default list follows. Template `AGENTS.md` line:
  "If your harness can hand a task to another agent, run handoffs in
  parallel only when the plan marks their file sets disjoint, each in its
  own worktree; run every other handoff sequentially. The orchestrating
  skill holds the procedure." `template/docs/README.md`: one paragraph on
  `houserules stats` at batch close and on `overrides` for a kit entry
  the project retires.
- [ ] **Step 7: This repository.** `knowledge/houserules.json` gains two
  entries, `kind: rule`, not standing, tag `full-tier`:
  `houserules.cli-changes-run-full-tier` (area `cli`) and
  `houserules.template-changes-run-full-tier` (area `template`), each
  stating why (the binary every adopter runs; the payload every adopter
  receives). `process.sequential-agents`: `status: superseded`, `see`
  `process.parallel-on-disjoint-files`, then `houserules archive`. Root
  `AGENTS.md`: the sequential sentence becomes the parallel default with
  the new id. `knowledge/houserules.json`'s entry that names
  `process.sequential-agents` in a body (grep for it) names the new id.
- [ ] **Step 8: Propagate and gates.** `./target/debug/houserules update
  --dir .` (after `cargo build`), `render`, the payload stamp, then
  `check-knowledge`, `check-backlog`, `render --check`,
  `cargo test --workspace`, `mise run lint`. An unfiltered grep for
  `standing rule is` and `sequential` over the tree
  (`process.pointer-sweep-unfiltered`); each remaining hit is either
  correct or fixed. Live run, the adopter's update path of spec 3: build
  `main`'s binary from a detached `git worktree add --detach <dir> main` into a scratch target
  directory; `init` a scratch `git init` repository with it (a 1.2.0
  install) and commit; run the tree binary's `update --dir <scratch>`;
  then the tree binary's `check-knowledge` and `check-backlog` inside it
  (green), `git -C <scratch> diff --stat` (the kit files and the new
  entries change, nothing else), and `update` a second time (no change).
  Each command its own `live_run` entry with `exit`.
- [ ] **Step 9: Commit** (`feat(template): risk tiers, severity by
  impact, a bounded review loop, parallel agents, and a self-optimizing
  knowledge base`).

### Task 3: eval run (`process.evals-rerun`), controller-dispatched

After T1 and T2 integrate. Every scenario of `.claude/evals/` at the
final template state, per the orchestrating skill's Template evaluation
section: `dependency-add` and `docs-edit` through `implementer` (sonnet),
`seeded-violations` through `task-reviewer` (opus), each in a detached
scratch worktree; one run set appended to `.claude/evals/record.json`
with the blob ids of both templates. The three scenario dispatches are
independent and read-only toward the tree: they run in parallel.

### Task 4: live run of the loop on this repository (controller)

1. `./target/debug/houserules stats` over the five most recent
   workspaces (`2026-09-14-batch-25` to `2026-10-02-batch-30`) into
   `<ws>/stats.json`, and over all 26 into `<ws>/stats-all.json`.
2. Compare `cost` with the session analysis of 2026-10-09 (spec 1): the
   numbers agree or the difference is explained by a stated classifier
   difference.
3. Apply the `owner_gate: false` proposals in one `docs(knowledge)`
   commit (template entries through `template/` and `update`).
4. Put every `owner_gate: true` proposal to the owner at the merge
   checkpoint.

### Branch review

`branch-reviewer` (fable) over `main..HEAD`, `WORKSPACE` as above. Fix
waves capped at 2 (full tier).

## Self-review against the spec

- 2.1 → T2 steps 1, 3, 4, 7. 2.2 → T2 steps 1, 2, 3, 4, 5. 2.3 → T2
  steps 1-4. 2.4 → T1. 2.5 → T2 steps 1, 3, 5, 6; T4. 2.6 → T2 steps 1,
  3, 6, 7; this plan's own parallel T1/T2. 3 → T2 step 3 (update
  guidance), Global constraints, T1 step 6 (old keys). 5 → T1 steps 1-8,
  T2 step 8, T3, T4.
