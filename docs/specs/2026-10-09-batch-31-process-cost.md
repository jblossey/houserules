# Batch 31 — process cost: tiers, a bounded review loop, a self-optimizing knowledge base

Items: HR-159, HR-140, HR-160, HR-161. HR-158 is filed, not built.
Kickoff: the owner's direction of 2026-10-09 (design.md 5.90 to 5.92).
The owner ruled sections 2.1 to 2.5 in the brainstorm of 2026-10-09
and directed section 2.6 at the spec gate.
Section 8 is the gate for the written spec as a whole.

## 1. The problem (measured 2026-10-09)

Users report that houserules prolongs feature work. The analysis read
every batch workspace under `.superpowers/sdd/*/` (26 workspaces,
batches 2 to 30), the 633 `issues` of their review files, and the
history of `knowledge/`.

- Review effort lands on the process's own deliverables. About half of
  the 633 findings target reports, evidence files, or prose. The three
  rules cited most in findings are claim rules:
  `knowledge-base.state-only-the-source` (50),
  `process.claims-match-artifacts` (35),
  `process.closure-claims-carry-enumeration` (21). A standing-rule fail
  is Critical by its class, so a report sentence forces a fix round and
  a re-review, and the edited report gives the next review new text.
- The knowledge base only grows: 41 entries and 26 standing on
  2026-09-02, 102 and 39 on 2026-10-09. Audit rows per review grew from
  30 (batch 2) to 62 (batch 30); report size per task from about 55 KB
  to 316-635 KB; reviews per task from 2.3 to 3-5.
- One pipeline serves every change. Batch 30 shipped a 116-line
  workflow through 7 agent dispatches, a 353-line spec, and a 142-line
  plan.

## 2. Design (ruled in the brainstorm)

### 2.1 Risk tiers (HR-159)

| Tier | Selected when | Pipeline |
|---|---|---|
| light | one task; docs, comments, config values, or a dependency bump; no executable behavior change; no `full-tier` entry applies | the backlog item's body is the spec (the owner still approves); implementer; one task review; controller live run. No plan document, no branch review. |
| standard | every change that is neither light nor full | short spec and owner gate; plan; implementer; one task review and at most one re-review; branch review only for a batch of two or more tasks |
| full | `houserules for <planned files>` returns an entry tagged `full-tier`, or the change alters a public contract, a schema, authentication or security code, or a data migration | today's pipeline; at most two re-reviews per task |

- A project marks its high-risk areas with an area entry tagged
  `full-tier`. Tags are free strings of the knowledge schema: no schema
  change, no new command.
- The plan (light: the dispatch brief) records the tier and the
  criterion that selected it. The task reviewer checks the tier; a
  wrong tier is a finding.
- A tier moves up mid-batch, never down (2.3).
- This repository tags two area entries `full-tier`: one for the `cli`
  area (`crates/houserules/src/**`) and one for the `template` area
  (`template/**`). A schema file outside them selects the full tier by
  the criterion "alters a schema".

### 2.2 Severity by impact (HR-140)

A finding's severity follows its impact. The rule's class (standing,
area, warn) no longer sets it. The finding keeps its `rule` id.

| Severity | Impact |
|---|---|
| critical | wrong behavior on a shipped contract (CLI, schema, user-facing text, a gate); a security exposure; data loss; a gate that passes when it must fail; fabricated evidence (a run that did not happen, output that is not real) |
| important | a missed requirement; incorrect or fragile behavior; maintainability damage you would block a merge over; a misstated claim the merge decision relies on (a test result, a gate outcome, a behavior) |
| minor | everything else: narrative report prose, comment and doc wording, report fields that no decision relies on, polish |

The core of `process.no-tech-debt` stays: every finding is fixed, or
deferred as a backlog item with a reason.

### 2.3 The bounded review loop (HR-140)

- A fix round that holds only Minor fixes closes with a controller
  verification note (`process.fix-round-verification-record`), never
  with a re-review.
- A re-review reads the fix diff and the original findings only. It
  files new findings at their impact; only a new Critical or Important
  finding opens a further round.
- Re-reviews per task: light 0, standard 1, full 2. A branch-review fix
  wave has the same cap as the batch's tier.
- At the cap: the open non-critical findings become one backlog item
  with the reason (a deferral under `process.no-tech-debt`); an open
  Critical finding goes to the owner.
- A light task with an Important or Critical finding moves up to the
  standard tier and gets its one re-review.

### 2.4 The cost analysis in the binary (HR-161, design.md 5.91)

`houserules stats <WORKSPACE>...` takes one or more workspaces.

- The existing keys `violations`, `unused_ids`, `audits`, and
  `reviews` keep their meaning. With one workspace their values do not
  change. With several, a task label is `<workspace dir name>/<task>`.
- `stats` also reads `branch-review.json` when a workspace holds it.
  The re-review's findings are its `new_breakage` rows.
- New key `workspaces`: one row per workspace: `path`, `tasks`,
  `reviews`, `re_reviews`, `fix_rounds` (the sum of the reports'
  `fix_rounds` lengths), `report_bytes`, `findings` by severity, and
  `targets` (findings by target).
- A finding's target, from its `file` field: `deliverable` when the
  path is inside the workspace or names a `*-report.json`,
  `*-review*.json`, or `*-audit*.json` file; `prose` when it ends in
  `.md` or starts with `docs/`, `knowledge/`, or `backlog/`, or names a
  commit message; `code` otherwise.
- New key `cost`: the sums of the `workspaces` rows and the means per
  task.
- New key `rules`: one row per entry of the knowledge base: `id`,
  `standing`, `mode` (`deterministic` when the entry has a `check`,
  else `judged`), `workspaces` (where an audit carried it), `rows`,
  `fails`, `findings` by severity, `targets`, `cited` (reports whose
  `knowledge_used` names it).
- New key `proposals`, from fixed thresholds; each row carries
  `action`, `id`, `reason`, its evidence numbers, and `owner_gate`
  (true for a standing entry and for an entry whose `source.by` is
  `user`):

| Action | Threshold |
|---|---|
| `demote` (standing entry to a non-standing area entry) | standing; audited in 3 or more workspaces; 0 fails; 0 findings |
| `retire` (`status: retired`, then `houserules archive`) | non-standing; injected in 3 or more workspaces; 0 fails; 0 findings; never cited |
| `mechanize` (a deterministic `check`) | no `check`; fails or findings in 2 or more workspaces |
| `narrow` (rescope the rule to contract surfaces) | 3 or more findings; at least two thirds Minor or with target `deliverable` (design.md 5.93) |
| `budget` | more than 25 standing entries; the row lists the standing entries by ascending catch rate |

- An entry tagged `ruled-keep` (the owner ruled to keep it as it is) gets no
  `demote`, `retire`, `narrow`, or `mechanize` proposal; it still counts toward
  the budget (design.md 5.93).
- The budget of 25 is advisory. `check-knowledge` keeps its hard
  60-line budget on the generated standing file. No gate changes.
- An id in a deliverable that the knowledge base does not hold (an
  archived or unknown id) counts in `violations` as today and gets no
  `rules` row and no proposal.

### 2.5 The self-optimizing loop (HR-160)

A new kit entry `process.knowledge-earns-its-place` (process area,
not standing) and the orchestrating skill carry the loop. At every
batch close:

1. The controller runs `houserules stats` over the five most recent
   batch workspaces and saves the output in the batch workspace.
2. The branch reviewer (standard and full batches with a branch review)
   or the controller (otherwise) reads `proposals` beside the
   retrospective.
3. A proposal with `owner_gate: false` is applied in the batch's
   `docs(knowledge)` commit: `retire` sets `status: retired` and runs
   `houserules archive`; `demote` drops `standing` and gives the entry
   an area; `narrow` rewrites the entry's scope; `mechanize` files a
   backlog item for the check.
4. A proposal with `owner_gate: true` goes to the owner in the batch
   report.
5. A kit-shipped entry the project retires, demotes, or rewrites goes
   into `.houserules.json` `overrides`, so `update` stays silent about
   it.
6. A new retrospective entry defaults to a non-standing area entry. A
   new standing entry names the standing entry it replaces, or the
   budget headroom from the latest `stats` run.

Limit, stated: a rule that prevents violations by its presence also
shows zero fails. That is why every standing demotion is owner-gated.

### 2.6 Parallel agents on disjoint files (design.md 5.92)

- The plan marks tasks parallel when their declared file sets are
  disjoint and no task needs another's output. Files that many tasks
  regenerate belong to the controller: the generated rule files and
  skill, `backlog/`, the payload stamp, lockfiles, and the root copies
  of the kit. A parallel task does not touch them; the controller
  regenerates them at integration.
- Each parallel implementer works in its own git worktree, on its own
  branch from the same `BASE`.
- The controller integrates a finished task onto the batch branch one
  at a time (cherry-pick), runs the gates after each, and writes the
  old-to-new commit map to the ledger. A conflict or a red gate at
  integration is a fix round for the later task.
- Reviews of different tasks may run at the same time: reviewers do not
  write to the tree. The review of a task starts after its implementer
  closes.
- At most three agents run at once unless the plan sets a lower limit.
  The ledger records each dispatch's start and close.
- The kit ships the default in the orchestrating skill and a new
  non-standing entry `process.parallel-on-disjoint-files`. An install
  seeded before 1.3.0 keeps the seed-once `AGENTS.md` line "sequentially
  by default". The orchestrating skill is the carrier of the migration:
  `update` rewrites it, and it names that line as the old kit default,
  not a project ruling, and gives the one-line replacement verbatim. A
  test pins the skill's copy to `template/AGENTS.md`. An install seeded
  by the 1.0.0 kit holds the old line in `CLAUDE.md` instead, and
  `update` backfills an `AGENTS.md` with the new line; the skill names
  both files and says to delete the old `CLAUDE.md` line. The release
  note stays a summary.
- This repository: `process.sequential-agents` gets `status:
  superseded` and a `see` link to the new entry; the root `AGENTS.md`
  line follows. The standing set loses one entry.

## 3. The adopter's update path (`process.gates-cover-generated-commits`)

The whole migration is `houserules self-update` and
`houserules update`. The walk:

- Binary 1.3.0: every existing `stats` call behaves the same; the new
  keys are additive. No other command changes.
- `update` replaces an unmodified kit-owned file (both skills, the
  agent templates) and an unmodified kit-shipped entry; it adds the new
  entries to the existing topic files; it keeps and reports a locally
  modified one, as today.
- The seeded `knowledge.yml` installs the version `.houserules.json`
  records (HR-154): CI moves to 1.3.0 in the same commit as the update.
- `check-knowledge`'s kit-citation lint: the new skill text cites the
  new entries; the same `update` run writes them. Green.
- `process.evals-rerun`: the update changes `task-reviewer.md` in the
  adopter's tree. The co-change check fails an audit whose range holds
  that commit. The skill and the release note tell the adopter to
  commit the update on its own, outside a batch branch; the kit's own
  eval run in this batch covers the new templates. Accepted red: an
  adopter who commits the update inside a batch branch gets the evals
  fail row in that batch's branch-range audit, as with every template
  update before.
- An adopter with a locally modified orchestrating skill or
  task-reviewer template keeps the old severity and loop rules until
  they merge by hand. `update` names each such file.

## 4. Changes

- `crates/houserules/src/main.rs`, `crates/houserules/src/rules/stats.rs`
  (and a sibling module if `stats.rs` grows past one responsibility):
  the variadic positional and the keys of 2.4.
- `template/.claude/skills/orchestrating/SKILL.md`: the lifecycle by
  tier (2.1), the severity line (2.2), the loop and its caps (2.3), the
  batch-close loop (2.5), the update guidance (3).
- `template/.claude/agents/task-reviewer.md`: severity by impact; the
  re-review scope; the tier check.
- `template/.claude/agents/branch-reviewer.md`: reads `proposals`;
  retrospective entries default to non-standing.
- `template/AGENTS.md` (new installs) and root `AGENTS.md`: the
  parallel default (2.6).
- `template/.claude/skills/migrating-knowledge/SKILL.md`: the
  parallelism question names the new default.
- `template/.claude/skills/finishing-a-feature/SKILL.md`: only where it
  names a review step the tiers change.
- `template/knowledge/process.json` and `knowledge/process.json`: new
  `process.risk-tiers`, `process.severity-by-impact`,
  `process.bounded-review-loop`, `process.knowledge-earns-its-place`,
  `process.parallel-on-disjoint-files` (none standing);
  `process.sequential-agents` superseded (this repository only); amended bodies of `process.no-tech-debt`,
  `process.brainstorm-first` (light: the item body is the spec),
  `process.code-health-scan` (light: the scan is a brief line),
  `process.fix-round-verification-record` (Minor-only rounds).
  No summary of a standing entry changes.
- `knowledge/houserules.json`: the two `full-tier` area entries (2.1).
- `README.md` and `docs/README.md`: `stats` usage and the loop.
- `crates/houserules/payload.stamp`, the root copies via
  `houserules update --dir .`, `.claude/evals/record.json` (one eval
  run of every scenario at the final template state).

## 5. Tests and proof

- `stats`: TDD over fixture workspaces in a `TempDir`: the old keys
  unchanged for one workspace (a golden of today's output); task labels
  across two workspaces; every `workspaces`, `cost`, and `rules` field;
  each proposal action at its threshold and one below it; `owner_gate`
  for standing and `source.by: user`; an unknown id; an unreadable
  deliverable keeps its named error; `branch-review.json` and
  `new_breakage` counted.
- CLI: `stats` with zero workspaces is a usage error; with one, the
  same output as 1.2.0 for the old keys.
- Live run (`houserules.live-run-recipe`): a scratch `git init`, `init`,
  `update`, `check-knowledge` green; `stats` over this repository's five
  most recent workspaces and over all 26; the loop applied to this
  repository (2.5): the non-gated proposals in the batch's knowledge
  commit, the gated ones to the owner.
- Evals: every scenario of `.claude/evals/` at the final template
  state, one run set in `.claude/evals/record.json`.

## 6. Limits, stated

- The target classifier reads paths; a finding with a free-text `file`
  field (a commit, a range) is `prose` when it names a commit message
  and `code` otherwise.
- A commit citation in a form the section 9 rule does not name (a
  commit id followed by a comma, or a commit named after file paths in
  one `file` field) counts as `code`: 2 of 845 findings over the 26
  closed workspaces (stats-all.json, `cost.totals.findings`).
- Thresholds are constants in the binary; a project changes them only
  through a kit release.
- Workspaces are git-ignored; the window is what the machine holds.

## 7. Out of scope

- HR-158 (generated report facts; a schema change).
- HR-144 (the design-phase review gate) stays in batch 29.
- The model policy is unchanged.

## 8. Approval

Approved by the owner 2026-10-09: "Approved; plan and build". The next
owner stop is the merge question.

## 9. Corrections after approval (controller record)

- 2.4 and 2.5, owner ruling 5.93 at the live run: `narrow` reads `prose`
  as a contract surface, not as soft; the `ruled-keep` tag records an
  owner's keep decision so the loop stops re-proposing it.
- 2.4 `budget` (controller, Task 1 round 2): a `ruled-keep` entry counts in
  the row's `standing` and stays out of `candidates`, since the ruling denies it
  a demotion; the row's evidence adds `ruled_keep`, the count of tagged
  standing entries.

- 2.4, the target classifier (controller, 2026-10-09, before the Task 1
  review): a `:<line>-<line>` tail is stripped like `:<line>`; a
  deterministic adherence row counts from the audit files only and a
  judged row from the review files only, so no row counts twice.
- 2.4, "names a commit message" (controller, 2026-10-09, Task 1 review
  issue 1): the plan read it as "contains `commit`", which classes
  `check_commit.rs` and `.githooks/commit-msg` as prose. The rule is: a
  `file` field names a commit message when, compared in lower case, it
  starts with a commit id (7 to 40 hexadecimal characters followed by
  the end, a space, or `..`), or starts with `commit ` followed by a
  commit id, or starts with `git log` or `git history`, or contains
  `commit message`, `commit body`, or `commit header`. A path that holds
  the word as part of a file or directory name is code.

## 10. Outcome (controller record, 2026-10-09)

- Shipped: sections 2.1 to 2.6 as corrected in section 9. Task 1 (`stats`)
  and Task 2 (the kit text) ran in parallel worktrees, the first use of
  2.6; Task 3 ran every eval scenario (6/6, 3/3, 5/5); Task 4 ran the
  loop on this repository, which led to ruling 5.93.
- The bounded loop held: each task and the branch fix wave stopped at
  its cap. HR-164 and HR-166 hold the Minor and one Important finding
  left at the caps; HR-162, HR-163, and HR-165 hold the other filed
  follow-ups; HR-158 stays filed.
- For the owner at the merge: the owner-gated proposals of the loop
  (`stats.json` in the batch workspace: standing demotions, mechanize
  and narrow rows for standing rules, and the budget row), the kit's 33
  standing entries against the budget of 25, the width of
  `houserules.cli-changes-run-full-tier`, a `sanctioned` field on audit
  rows (a schema change), and the shape of the branch reviewer's
  `recommendations`.
