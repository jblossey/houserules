# Batch 22 spec: kit stewardship — archive, comment prune, downstream update

Status: approved by the owner, 2026-09-11 (design.md 5.61),
as drafted — the three §6 rulings included.
Items: HR-105 (archive sweep), HR-106 (comment prune + the
in-code docs rule), HR-099 (downstream update model), HR-089
(rider: the update backfill question HR-099's design settles).
NOT in scope: the template cluster HR-078/080/090/094 + riders
HR-095/096 (the recorded candidate for batch 23, one shared
evals rerun); the step-two release set (owner-attended, HR-068
first); HR-042/053/055/057/058 (parked owner items); HR-072
(lint narrowing); HR-081/082 (release-adjacent); HR-088
(--fix arm).

## 1. Goal

Three owner requests from 2026-09-11, one theme: the kit stays
clean as it ages. The active backlog and knowledge base carry
only what future development needs (HR-105). Code comments
describe what is current, never history (HR-106, with the
standing rule that keeps it true). The kit updates template
content downstream while an adopter's deliberate changes stay
untouched (HR-099, settling HR-089's backfill question on the
way).

## 2. HR-105 — the archive sweep

After every batch, retired records leave the active set for an
archive that history analysis can still reach. Archiving moves
data; it never deletes it.

What qualifies:
- Backlog items with status `done` or `dropped`.
- Batch entries in `backlog/batches.json` whose state is `done`
  and whose acceptance ruling is homed.
- Knowledge entries marked retired. Knowledge has no status
  field today — retirement lives in prose ("RETIRED at batch 20
  T3..."). The schema gains an optional `status` field
  (`active` default | `superseded` | `retired`); the first
  sweep stamps the field on the entries whose summaries already
  say it.
- NOT `backlog/decisions.json`: rulings are the permanent
  record design.md §5.x cross-references; they stay active.

Where it lives: `backlog/archive/` (mirroring `items/` files
plus `batches.json`) and `knowledge/archive/` (mirroring the
topic files). Both stay schema-valid.

The sweep: a `houserules archive` subcommand. It moves every
qualifying record, prints one line per move and a summary
count, and is idempotent (a second run moves nothing). No
flags in v1 (YAGNI).

Readers after the split:
- `get` resolves an archived id and labels it archived —
  `knowledge-base.ids-are-permanent` holds; `see` links from
  active to archived entries keep resolving.
- `list`, `index`, `topics`, `for`, `standing`, `render`, and
  both check gates read the active set only. check-knowledge's
  liveness rules (dead globs, loading paths) do not judge
  archived entries; both checks still schema-validate the
  archive files so corruption is loud.
- The dead-glob gate note: archiving an entry never changes
  `areas.json`; an area emptied by archiving fails the gate as
  it should — the sweep reports it, the operator rules.

Lifecycle hook: `finishing-a-feature` gains a close-out step —
run `houserules archive` after acceptance rulings land; the
sweep commit rides the close (or next) branch. The template
copy of the skill carries the same step; no agent-template or
eval change, so no evals rerun (`process.evals-rerun` does not
trigger).

## 3. HR-106 — the comment prune and the in-code docs rule

The owner's standard: a comment describes what is current —
concisely, precisely, without unnecessary detail. No comment
documents history. History lives in git, the specs, and the
knowledge base; the code carries only the current contract.

Survey evidence (repo-wide, 2026-09-11; full report at
`.superpowers/sdd/2026-09-11-batch-22/comment-survey.md`):
24,878 Rust lines carry 6,676 comment lines (26.8%) — 6,250
doc-comment lines, 426 non-doc. Of 993 comment blocks, 293
(29.5%) carry a strong history marker; those blocks hold
4,038 comment lines, 60.5% of all comment lines. Five
comments cite commit SHAs, two cite CI run ids, and nearly
every module doc opens as a batch-by-batch changelog — the
worst are `report_claims.rs:1-255` and `install.rs:1-218`.
An estimated 15-20% of comment lines already meet the
standard; the four shell files are fully compliant. 41
blocks carry only an HR-/WI- id and need case-by-case
judgement.

The rule iteration (a standing-entry change — the owner rules
the wording at this gate):
- `writing-style.code-comments` (standing) revised: "Write a
  code comment only for a current constraint the code cannot
  show. Never narrate history in code; git, the specs, and the
  knowledge base hold it."
- `writing-style.doc-comments` (standing) revised: "Document
  every exported symbol with the language's doc-comment
  convention: the current contract, concise and complete.
  Name things so the code reads without comments."
- Both template knowledge copies ship the same wording.

The sweep itself:
- Scope: every code file — `crates/houserules/src/**` (bins
  included), `crates/houserules/tests/**`, `tools/*.sh`,
  `.githooks/*`, and the template copies of the shell files.
  Goldens and fixtures are out (generated or deliberate).
- Method: one or more implementer subagents, dispatched
  sequentially, each with a bounded file list from the survey.
- Keep/drop line: a mechanism explanation rewrites to present
  tense and stays; provenance (batch/task/round references,
  HR-/WI- ids, dates, spec and archive paths, commit SHAs, CI
  run ids) drops; a comment that states a constraint plus its
  history keeps the constraint only. A knowledge id naming a
  rule the code enforces may stay — it is a current contract,
  not history. Fixture data (a WI- id inside test data) is
  data, not narration.
- Doc comments are claims (`writing-style.doc-comments`
  bullet): every rewritten doc comment is re-verified against
  the code it describes.
- TDD note: comment-only edits are not executable changes; the
  gate is the full suite plus the cross-target check green,
  and the reviewer confirms the diff touches comments only.

## 4. HR-099 — the downstream update model (settles HR-089)

Today `update` overwrites every KIT_OWNED file unconditionally
and never touches a SEED_ONCE file. Both halves are wrong for
adopters: a deliberate local change to a kit file is silently
destroyed, and improved kit content never arrives.

Recommended design — a stamped baseline plus an explicit
override list, both in `.houserules.json`:

- `init` and `update` stamp `baselines`: for each KIT_OWNED
  file and each kit-shipped knowledge entry, the SHA-256 of
  the content the kit last wrote (computed after any
  `--id-prefix` rewrite).
- `update`, KIT_OWNED: current content equals the baseline →
  overwrite and restamp, as today. Differs → adopter-modified:
  keep the file, print one `kept <file> (locally modified)`
  line. A path in `overrides` is kept silently — the adopter
  declared ownership.
- `update`, SEED_ONCE knowledge topics: entry-level upsert by
  id. A kit entry the adopter left at baseline → replaced with
  the new kit content and restamped. Modified → kept and
  reported. Deleted by the adopter → respected and reported
  once; an override silences it. Adopter-authored entries are
  never touched.
- `update`, SEED_ONCE non-knowledge files: absent → written
  (this is HR-089's backfill, now safe: absence with no
  override means "missing", absence with an override means
  "deleted on purpose"). Present → untouched in v1; whole-file
  baseline upsert can follow if wanted.
- The marker the owner asked for: `overrides` is a hand-edited
  JSON list of paths and entry ids in `.houserules.json`,
  documented in the template README. No new subcommand in v1
  (YAGNI); `update` preserves the field when it restamps.
- Migration for installs with no `baselines`: compare against
  the current payload; equal → stamp and proceed; different →
  report as modified (the safe default) with the override
  instruction. Nothing is overwritten on the first run after
  this change.
- Dependency: SHA-256 needs a digest crate; candidate `sha2`
  (RustCrypto: maintained, MIT OR Apache-2.0, inside the
  cargo-deny allow-list; exact-pinned via `cargo add`). The
  owner rules the adoption (`quality.well-maintained-libraries`).

HR-089 closes with this design: `update` backfills a missing
SEED_ONCE file, the override list carries the
deleted-on-purpose case, and install.rs's "seeds once and
never touches again" contract is rewritten to the new one.

## 5. Task shape and order

- T1: HR-105 — schema field, archive subcommand, reader split,
  the finishing-skill step (template + update --dir .), first
  live sweep.
- T2: HR-099 + HR-089 — baselines, override list, the three
  update passes, migration path, template README documentation.
- T3: HR-106 — the standing-rule revision (owner-ruled wording
  from this gate), then the sweep over the whole tree, T1/T2's
  new code included. Last on purpose: nothing lands history
  narration after the sweep. The survey sizes it as three
  sequential implementer dispatches: src core (the seven
  changelog module docs and the audit/check essays), bins plus
  the remaining src modules, tests. Each dispatch carries its
  file list and per-file hit counts from the survey.
- Implementers on sonnet, task reviews on opus, branch review
  on fable, strictly sequential. No agent-template or eval
  change anywhere in the batch: no evals rerun.
- Every executable change under `process.tdd`; live run per
  `houserules.live-run-recipe` (scratch `git init`, init/update
  both paths) before any CI spend.

## 6. Gate rulings (owner, 2026-09-11, design.md 5.61)

1. HR-105: the knowledge `status` field is the retirement
   signal — `active` (default) | `superseded` | `retired`;
   the first sweep stamps the entries whose summaries already
   say RETIRED.
2. HR-106: both standing-rule wordings adopted as drafted in
   §3.
3. HR-099: `sha2` adopted (exact-pinned via `cargo add`,
   vetting recorded in the task report); entry-level upsert
   for SEED_ONCE knowledge topics.
