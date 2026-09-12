# Batch 23 plan: the template cluster

Spec: docs/specs/2026-09-12-batch-23-template-cluster.md
(approved 2026-09-12, design.md 5.65 — every template sentence
verbatim).
Items: HR-078, HR-080, HR-090, HR-094, HR-095, HR-096, HR-103.
Workspace: .superpowers/sdd/2026-09-12-batch-23/.
Order: T1 (code + records) → T2 (templates + skill + sweep) →
the controller-run evals rerun → batch live run → fable branch
review → aggregation → finish. Implementers on sonnet, task
reviews on opus, strictly sequential.

## Code-health scan (process.code-health-scan)

- `crates/houserules/src/rules/deliverable.rs`: three filename
  patterns as separate literals is the smell HR-103(1) removes;
  the batch-22 branch fix aligned their id part by hand — the
  declared-shape unification with a parity test is the fix, not
  a fourth literal. No other smell; the file was reviewed twice
  in batch 22.
- `crates/houserules/src/report_claims.rs`: the Limits section
  is a maintained enumeration (15 bullets, counted at the
  batch-22 sweep); HR-095 appends one bullet — keep the bullet
  form, no prose paragraph.
- The nine HR-096 markers live in backlog item and archive bodies
  (four) and in old plan and spec header prose (five, the
  batch-6/7/8/9 plans and the batch-2 spec); each restatement
  edits body or header text only — where entries are involved,
  ids, summaries, and areas stay untouched
  (knowledge-base.ids-are-permanent).
- The three agent templates and the orchestrating skill are
  long instruction files; edits are surgical insertions at the
  sections the spec names — no restructuring, no renumbering
  beyond what an inserted step forces (and every cross-reference
  to a renumbered step updates in the same commit, the batch-22
  doc-comments lesson).
- `.claude/schemas/deliverables.json` is PREFIXED (id-prefix
  rewrite) and both copies differ only at the id-prefix line —
  the task-id-shape declaration must land identically in both
  and survive the rewrite (T1 proves with a --id-prefix scratch
  init).

## T1 — the code and records (HR-103(1), HR-095, HR-096)

1. The task-id shape declared once in
   `.claude/schemas/deliverables.json` (both copies; measured
   from the corpus: the shipped shape is `\d+[a-z]?`), the three
   deliverable.rs patterns derived from or parity-pinned to it.
   TDD: the parity test fails under a disclosed one-pattern
   mutation; a --id-prefix FOO scratch init proves the rewrite
   leaves the declaration intact.
2. HR-095: the one Limits bullet in report_claims.rs
   (comment-only; the retained t3a scripts stay zero).
3. HR-096: rerun the retained batch-21 enumeration
   (sweep-unreachable-shas.sh) at BASE — expect the nine rows;
   restate each to its event locator preserving the claim;
   rerun to zero unreachable; retain both runs in the batch
   workspace.

Gates: the full CI-mirroring set (houserules.task-gates-mirror-ci):
fmt --check, cargo test --locked, the cross-target check,
check-knowledge, check-backlog, render --check, residue-gate,
clippy, cargo deny.

## T2 — the templates, the skill, the sweep (HR-078/080/090/094/103)

1. Every §2 sentence lands verbatim in template/.claude/...;
   `houserules update --dir .` (tree binary if T1 changed render
   output — it does not; the installed binary is current at
   885d5a3) regenerates the KIT_OWNED root copies; the dogfood
   byte-parity suite pins the sync.
2. HR-090's sweep widening: extend the retained pattern set so
   flag-only audit teaching is caught; rerun over the tracked
   tree to zero unexplained; retain the widened script + run in
   the workspace as the closure enumeration.
3. The T2 dispatch carries every changed template instruction
   verbatim (the running session's copies are stale until
   restart) — and the T2 review verifies each landed sentence
   against the spec's drafted text word for word.

Gates: as T1. No TDD cycles for prose; the sweep rerun and the
dogfood parity suite are the deterministic proofs.

## The evals rerun (controller-run, after T2, before the branch review)

Per the orchestrating skill's Template evaluation section:
implementer scenarios through `implementer` (sonnet) in detached
scratch worktrees at the branch head; `seeded-violations`
through `task-reviewer` (opus) on the fixture its setup builds;
judge every expected_behavior line from the report or review;
append ONE run set to `.claude/evals/record.json` (date,
template blob ids at HEAD, per-scenario pass/of/notes);
`eval-`-prefixed workspace artifacts; keep nothing from the
worktrees. The audit stays red until record.json changes — the
designed forcing function.

## Close

Batch live run (fresh scratch init: regenerated templates
present, byte-identical to template/, both checks green), the
fable branch review (BASE = the merge base with main), every
finding fixed or filed, aggregation to 1-5 commits from reviewed
boundary trees with tree-identity proof, then finishing-a-feature
— fetch/rebase first, checks on an OPEN PR, the main push before
any branch deletion.
