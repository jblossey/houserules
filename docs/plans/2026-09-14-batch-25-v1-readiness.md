# Batch 25 plan: the v1-readiness batch

Spec: docs/specs/2026-09-14-batch-25-v1-readiness.md (approved as
amended 2026-09-14; rulings 5.79).
Items: HR-113, HR-116, HR-107, HR-109, HR-110, HR-111, HR-112,
HR-115, HR-117, HR-120, HR-100, HR-104, HR-119, HR-098, HR-081,
HR-101, HR-102.
Workspace: .superpowers/sdd/2026-09-14-batch-25/.
Order: T1 (the review rows, seeded proofs) → the controller
publishes the private review page → the owner ticks ASYNC → T3a →
T3b → T4 → T2 (the ruled template edits + HR-116 + evals rerun) →
T5 (freeze, HR-098 owner-attended, the Release-As 1.0.0 footer) →
branch review → finish → the watched 1.0.0 cut.
Implementers on sonnet (effort xhigh), task reviews on opus (high),
branch review on fable (high), strictly sequential.

## Code-health scan (process.code-health-scan)

- `crates/houserules/src/rules/audit.rs` (2845 lines) and
  `rules/check.rs` (2220) are the two largest files and the natural
  landing sites for HR-109/115/120 — god-file risk. Direction: each
  new deterministic check lands as its own focused module under
  `rules/` (or `src/bin/` for a standalone gate), wired from the
  host file with a one-line registration; no new 300-line block in
  either file.
- `crates/houserules/src/report_claims.rs` (1794 lines) hosts
  HR-107/110/111. The three checks all resolve claim text against
  cited captures: extract ONE shared capture-resolution helper
  (locate the cited file, load it, expose matched lines) and build
  the three checks on it. The file's Limits-bullet convention
  (self-documented at its head) binds any new false-positive class.
  HR-122 lands here too: the foreign-repository commit-citation
  class T1 hit (a Limits bullet or a recognized citation form),
  whether or not T3a's own corpus run re-triggers it.
- `crates/houserules/src/get.rs` (172 lines) is clean; HR-100 is a
  localized fix — `parse_id` hardcodes `HR-`/`A-`/`PP-` at lines
  48-54 while `.houserules.json` carries `idPrefix`. Fix reads the
  project's prefix; the `A-`/`PP-` shapes are kit-fixed forms to
  keep (verify against the item body and schema before narrowing).
- `crates/houserules/src/install.rs` (1172 lines) hosts HR-104; its
  module doc (lines 6-81) already states the restamp contract —
  the fix must keep that doc true (state-only-the-source).
- `crates/houserules/src/bin/find-shell-tool-refs.rs` (481 lines)
  carries HR-101's two dead exceptions; HR-115's vacuous-exception
  gate should CATCH them before the fix removes them (the live
  corpus proof, spec §5).
- `template/**` edits (T2) each pair with `payload.stamp`
  (per_commit co-change); the evals rerun triggers on any agent
  template or scenario change (process.evals-rerun).
- `mise.toml` + `.github/workflows/ci.yml` host HR-119; the 3-OS
  blast radius is the recorded constraint — the gate lands in the
  ubuntu-only checks job, with cargo-dist installed via mise's aqua
  registry entry (feasibility verified at HR-119's filing; re-verify
  against current docs at implementation).

## T1 — the review rows (implementer, sonnet)

Derive `t1-rows.json` in the workspace: one row per shipped default
plus the gap rows, exactly the spec §2 row classes, each row
`{id, class, source (rerunnable provenance), title, current (the
default's text or shape, compact), proposed_notes (empty)}`.
Enumerations: `git ls-files template`; per-entry reads of the seeded
knowledge files; the agent templates/skills split into instruction-
block rows; schema constraint rows from both schema copies; gap (a)
per-entry diff repo-vs-template on shared topics + repo-only classes;
gap (b) tag-pilot shared-topic diffs + generic-topic scan —
tag-pilot STRICTLY read-only. The row set's completeness claim names
its enumerations (closure-claims-carry-enumeration). No template
edit in this task. Review (opus) verifies row completeness by
re-running the enumerations.

## The review page (controller, after T1's review)

The controller builds and publishes the PRIVATE artifact page from
t1-rows.json (ruling 5.79(1): private by default, link never
shared): rows grouped by class, filterable; per row the three
affordances (appropriate yes/no, requested changes free text, notes)
plus a global add-missing-items box; decisions in the artifact's
shared database. Seeded read-back proof before the owner starts: one
test row ticked, read back via the database API, then cleared
(spec §5). The owner reviews ASYNC while T3/T4 build.

## T3a — the report-claims family (HR-107, HR-110, HR-111)

The shared capture-resolution helper, then: HR-107 deterministic
citation-line check; HR-110 numeric/locative cross-check; HR-111
vacuous-zero (a cited sweep covers the file it certifies). TDD each;
then the corpus run: the checker over every retained deliverable in
the tracked tree and the batch workspaces, false positives triaged
into Limits bullets or fixes (the recorded corpus is the proof).

## T3b — engine and audit checks (HR-109, HR-115, HR-117, HR-120)

- HR-109: the schema engine REJECTS unsupported keywords (a schema
  that names a keyword the engine ignores is a gate narrowing
  itself); migration: both schema copies re-validated, any silently
  ignored keyword either implemented or removed with the constraint
  restated in supported form (quality.no-compat-softening).
- HR-115: vacuous-exception check — every gate exception must match
  at least one live occurrence; proof: it catches HR-101's two dead
  entries on the current tree BEFORE T4 removes them.
- HR-117: the CI-mirror test parsing ci.yml's run lines against the
  task-gates entry.
- HR-120: audit flags a 7-40 hex string in a backlog/ or knowledge/
  diff hunk resolving to a commit not reachable from the default
  branch.
Each its own module (scan direction above).

## T4 — adopter-facing fixes and riders (HR-100, HR-104, HR-119, HR-101, HR-102)

HR-100 idPrefix resolution in get.rs; HR-104 restamp no-op when
content equals the payload (install.rs, module doc kept true);
HR-119 the regen-and-compare CI gate (checks job only, cargo-dist
via aqua, the allow-dirty interplay from HR-119's body); HR-101
remove the two dead exceptions (AFTER T3b's gate catches them);
HR-102 the .gitattributes narration sweep.

## T2 — the ruled template edits (after the owner's ticks land)

Read every decision back from the page's database; home each ruling
(design.md 5.x for policy, the entry edits template-first, the spec
addendum for the row-level record); apply the edits with stamp
pairing; HR-116's three riders in the same task (orchestrating skill
commit discipline; implementer range-wide check-commit step;
deliverables schema optional rulings array — both schema copies,
parity-pinned). Then the FULL evals rerun (template + scenario
changes), record.json appended - the CONTROLLER runs it after this
task's review closes (implementers cannot dispatch agents); the
task's own audit sanctions the interim evals-rerun fail against
this section.

## T5 — freeze, governance, the cut

- The surface freeze: the CLI command set (from main.rs's clap tree)
  and both schema copies' constraint surface recorded in design.md
  as the 1.0 contract.
- HR-098 (owner-attended): the code-owner approval requirement for
  non-owner pushes; the Dependabot auto-merge proof on the owner's
  token.
- The unsigned-macOS Gatekeeper paragraph in README/runbook
  (ruling 5.79(4), HR-121 named).
- The EMPTY commit carrying `Release-As: 1.0.0` (exactly one
  `^Release-As:` hit verified at aggregation).
- HR-081's close-gate observation: after the batch merges, the
  release PR must show the version bump attributed through the
  stamp pairing; the observation closes HR-081 and HR-113.

## Close

Branch review (fable), aggregation to 1-5 commits with tree
identity (the footer commit preserved), finishing-a-feature
(fetch/rebase; checks on an OPEN PR; the main push before any
branch deletion), the watched 1.0.0 cut (release-please on the PAT
→ tag v1.0.0 → dist uploads into the release,
houserules.release-please-owns-the-release-object), the batch
report with acceptance.
