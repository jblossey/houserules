# Batch 25 spec: the v1-readiness batch

Status: approved by the owner as amended, 2026-09-14 (design.md
5.79 gate rulings; directed 2026-09-12, design.md 5.68/5.69; HR-113
the umbrella).
Items: HR-113, HR-116, HR-107, HR-109, HR-110, HR-111, HR-112,
HR-115, HR-117, HR-120, HR-100, HR-104, HR-119, HR-098, HR-081,
HR-101, HR-102 (selected 2026-09-14; the riders ruled in at the
gate). Deferred at the gate: HR-072, HR-088. Parked owner items stay parked; HR-053 (modular
installs) and HR-055 (starter rule set) are review-adjacent — the
review's free-addition affordance may pull either back.

## 1. Goal

The repository leaves this batch v1-ready: every template default is
owner-ratified through the complete review, the deterministic gate
family guards the kit's own process claims, the adopter-facing
defects are fixed, the CLI and schema surface is frozen and recorded,
and the 1.0.0 cut rides the proven release chain — as ruled at this
gate.

## 2. Half one — the template-defaults review (HR-113(2))

One row per shipped default, derived from the tree, never memory
(measured 2026-09-14):

- `git ls-files template` = 30 files. Row classes: the 43 seeded
  knowledge entries (process 29, knowledge-base 3, quality 3,
  security-hygiene 5, writing-style 3) one row each; the three agent
  templates' instruction blocks; the three skills; the schema
  constraints (id shapes, required fields, textList floors, the
  task-id shape); the seeded workflow (knowledge.yml); CLAUDE.md's
  seeded text; settings.json hooks; areas.json defaults; the backlog
  seed files; docs/README.md; the 4 eval scenarios;
  tools/claude-session-start.sh.
- Gap rows (a), this repository vs template: 83 running entries vs
  43 seeded — the per-entry diff on the shared topics plus the
  repo-only machinery classes, each delta a row (used-here,
  never-shipped).
- Gap rows (b), tag-pilot vs template, STRICTLY READ-ONLY
  (houserules.tag-pilot-is-read-only — enumerate by reading, never
  modify, commit, or run against it): the shared-topic diffs
  (process 28, security-hygiene 10, knowledge-base 3, writing-style
  3; tag-pilot ships NO quality.json — itself a row) plus a scan of
  its generic-topic files (toolchain, live-run, deploy, coverage,
  batch-analysis, settings, infra) for template-worthy process
  candidates.
- Estimated 150-200 rows total.
- Per-row affordances (owner-directed): tick template-appropriate or
  not; request changes and specify which; free additions anywhere.
- Every row names its rerunnable provenance (the file, entry id, or
  diff that produced it) — closure-claims-carry-enumeration applies
  to the row set itself.
- Deliverable form: RULED AT THIS GATE (§6 Q1). The controller
  recommends an interactive review page (a published artifact with
  shared persistent state): the owner ticks and annotates rows in
  place, the decisions read back programmatically, no manual
  transcription of ~200 rows; a CSV spreadsheet or a markdown
  checklist file in the workspace are the alternatives.
- Every owner decision homes per process.rulings-to-file (design.md
  5.x for template-policy rulings, entry edits in template/**, spec
  addendum for the row-level record); the edits ride this batch's T2.

## 3. Half two — v1 readiness (HR-113(1))

- **The ruled template edits (T2)**: apply every change the review
  produces, template-first (houserules.template-is-the-source), each
  template commit stamp-paired (per_commit co-change). HR-116's three
  riders land in the same template-touching task: the orchestrating
  skill's controller mid-flight commit discipline; the implementer
  retrieval protocol's range-wide check-commit invocation; the
  deliverables schema's optional rulings array. Template/eval changes
  trigger the full evals rerun (process.evals-rerun).
- **The deterministic gate family (T3)**: HR-107 (citation-line check
  in check-report-claims), HR-109 (the schema engine rejects
  unsupported keywords instead of silently narrowing — a gate defect
  class), HR-110 (numeric/locative claims cross-checked against their
  cited captures), HR-111 (vacuous-zero: a cited sweep covers the
  file it certifies), HR-112 (the no-provenance comment sweep becomes
  a deterministic gate), HR-115 (vacuous-exception: a gate exception
  matches something real), HR-117 (the CI-mirror test deriving the
  task-gates list from ci.yml), HR-120 (in-branch SHA cited from a
  permanent file). Each lands TDD with its own false-positive corpus
  run over the existing tree.
- **Adopter-facing fixes (T4)**: HR-100 (`get` resolves a project's
  own --id-prefix — an adopter-breaking defect), HR-104 (update skips
  the restamp when content equals the payload), HR-119 (a real CI
  gate regenerates release.yml from the config and compares bytes —
  the feasibility notes in the item: mise registry carries cargo-dist
  via aqua; the 3-OS blast radius is the constraint to design
  around).
- **Governance (owner-attended)**: HR-098 — main requires the code
  owner's approval for everyone but the owner; the Dependabot
  auto-merge proof on the owner's token completes.
- **The surface freeze**: the CLI command set and the two schema
  copies' constraint surface recorded as the 1.0 contract in
  design.md; post-1.0 changes to either become breaking-change
  rulings.
- **macOS signing**: the revisit-at-1.0 ruling (design.md 5.37 left
  binaries unsigned) — ruled at this gate (§6 Q4).
- **HR-081's close gate** rides this batch's first template-touching
  merge: the stamp pairing makes the release PR bump; the observation
  closes the item.
- **The 1.0.0 cut** (§6 Q3): if ruled in-batch, an empty commit
  carrying `Release-As: 1.0.0` rides the final branch (exactly one
  `^Release-As:` hit verified at aggregation per
  houserules.release-footer-survives-aggregation); the proven chain
  does the rest, dist uploading into release-please's release
  (houserules.release-please-owns-the-release-object).

## 4. Task shape (provisional; the plan settles it)

T1 (build the review) → the owner reviews ASYNC while T3 → T4 build
→ T2 (apply the ruled edits + HR-116 + evals rerun) → T5 (freeze
docs, HR-098 owner-attended, the cut as ruled) → branch review →
finish. Strictly sequential agents; the owner's review runs beside
the agent work, not inside it.

## 5. Live-proof discipline

Gate work proves itself on corpus runs over this repository's own
tree (the richest violation corpus available); the review page's
read-back is itself verified (a seeded test row ticked and read back
before the owner starts); the 1.0.0 cut, if ruled, is watched
end-to-end like 0.3.0 was.

## 6. Ruling questions for the gate

1. **Deliverable form — RULED (owner, 2026-09-14)**: the interactive
   review page, as a claude.ai artifact that is NOT published
   publicly. Artifacts are private by default (owner-account access
   only); the batch never shares the link or changes its sharing
   state, and the decision read-back plus the homed rulings land in
   the repository as the durable record — the page is the input
   device, not the evidence store.
2. **Scope — RULED (owner, 2026-09-14)**: HR-101 and HR-102 ride
   (HR-101 doubles as HR-115's first live corpus hit); HR-072 and
   HR-088 stay deferred, no v1 bearing. Seventeen items total.
3. **The 1.0.0 cut — RULED (owner, 2026-09-14)**: in-batch at
   close. The Release-As: 1.0.0 footer rides the final branch as an
   empty commit (exactly one ^Release-As: hit verified at
   aggregation), and the cut is watched end to end like 0.3.0.
4. **macOS signing at 1.0 — RULED (owner, 2026-09-14)**:
   defer-documented. 1.0 ships unsigned; the docs state the
   current Gatekeeper bypass (amended 2026-09-15, owner ruling: run
   once and let macOS block it, then System Settings > Privacy &
   Security > Open Anyway - Apple removed right-click-Open in macOS
   Sequoia); HR-121 books signing, triggered
   by real adopter friction, not a date. The 5.37 revisit-at-1.0
   obligation is discharged by this ruling.
