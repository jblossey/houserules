# The Tier-2 rewrite: houserules as one static binary

Date: 2026-09-04. Status: approved by the owner, 2026-09-04.
Driver: HR-052 (this spec is batch 15's deliverable); HR-047 and
HR-048 are the build items it governs. Rulings already filed: plain
`v<version>` tags (design.md §5.20), Rust (§5.21), npm retired —
binary-only distribution (§5.22), the surface and runtime rulings —
no shims, flat commands, modularity, dev tooling (§5.23), and the
post-port repository sweep (§5.24).

## 1. Goal

One `houserules` binary, written in Rust, replaces
`bin/houserules.mjs`, `template/tools/kb.mjs`, and
`template/tools/backlog.mjs`. Every adopter-facing behavior carries
over under the current suite's contracts. Distribution is per-platform
release binaries. The end state is Node-free on BOTH sides (owner
ruling, 2026-09-04): adopters need no runtime install, and this
repository's own dev tooling migrates too — cargo test replaces
vitest, a built-in `check-commit` replaces commitlint in the hook,
and pnpm, package.json, and node_modules leave the tree at
retirement.

## 2. The payload contract, redefined (owner ruling at approval)

Current invariant (`houserules.payload-runs-on-builtins`): the payload
runs on Node built-ins and POSIX shell only. Proposed replacement
(RULED at the shims question, 2026-09-04: no shims):

> The vendored payload runs on the `houserules` binary and POSIX shell
> only: every shipped reference invokes the flat `houserules` commands
> directly, the commit hook stays POSIX shell, and no vendored file
> needs Node, npm, or any runtime install.

`tools/kb.sh` and `tools/backlog.sh` are DELETED, not wrapped: the
wrapper layer would exist only to avoid a one-time mechanical
reference rewrite, and the near-zero-adopter moment makes that rewrite
cheap. `update` gains vendored-file deletion so existing installs
migrate cleanly — a real new capability, not a compromise.

Approving this spec rules the redefinition; the design.md §3 ownership
prose and the invariant entry amend in PHASE 3 (re-timed at the batch
16 branch review: the invariant stays true until the reference
rewrite actually flips the payload contract, and amending earlier
would state what is not yet so).

## 3. Architecture

- One cargo workspace, one binary crate `houserules`, FLAT commands
  (RULED 2026-09-04: no kb/backlog namespaces): `init`, `update`,
  `files`, `render`, `check-knowledge`, `check-backlog`,
  `check-commit` (the commit-msg hook's gate — replaces the hook's
  commitlint probe with a built-in conventional-commit check, reusing
  the audit's existing subject/body rules; adopters lose their last
  soft Node dependency), `audit`, `validate`, `stats`, `get`
  (resolves by id shape: `HR-031` is a backlog item, `process.tdd` a
  knowledge entry; ordering ruled at the batch 17 T4 review — a
  fixed-domain command loads its domain before its arity check, JS
  parity, while the unified `get` checks arity first because its
  domain depends on the ids: loading a default domain would wrongly
  fail in a repository holding only the other one; the divergence
  is pinned by a counterexample test), `for`, `index`, `topics`,
  `standing`, `list`, `set`, `batch`. The checks stay per-module by the owner's ruling;
  exact check-command names settle in phase 1 against the module
  names below.
- The crate layout separates the feature modules the owner intends to
  make optional later: a `rules` module (knowledge, render, audit,
  validate, the generated files) and a `backlog` module (items,
  batches, checks), with `install` (init/update/files) above them.
  The parity port ships everything; the boundaries exist so a future
  modular install (backlog-only, or rules without backlog) is a
  feature flag away, never a rewrite. Modular installs themselves are
  out of scope here (HR-053).
- serde models the knowledge, backlog, and deliverables schemas
  exactly; the JSON Schema files stay the vendored source of truth and
  a build test pins the serde models against them.
- Data-layer rule (controller-accepted at the batch 17 T2 review,
  pending owner confirmation at the batch report): typed serde
  models serve only paths where the data is never re-serialized
  back to its source file and a parse failure is an acceptable
  outcome (aggregating readers, the schema-pin build tests). Every
  path that must preserve an adopter's on-disk key order or
  diagnose malformed input reads raw serde_json::Value through
  tolerant loaders — a typed round-trip reorders user-owned files,
  and a strict loader cannot report the malformed data the checks
  exist to diagnose (both proven at the T2 review). Model types
  with no consumer under this rule are deleted with their pin
  tests, not kept dormant.
- No shims: every shipped file that says `tools/kb.sh ...` or
  `tools/backlog.sh ...` today — CLAUDE.md, the generated rules files,
  the three skills, the agent templates, entries' verify paths —
  rewrites to the flat `houserules <command>` form in one mechanical
  phase-3 task; the two shell tools are deleted from the payload, and
  `update` learns KIT_OWNED deletions. A missing binary fails at the
  shell with command-not-found; the README and CLAUDE.md name the
  install channels.
- Glob matching runs on the globset crate as the single engine
  (RULED 2026-09-04 mid batch 16, design.md §5.25 — amends this
  spec's original union-parity line): every divergence from the JS
  union is pinned by a counterexample test asserting the chosen
  answer, malformed globs are named errors, never panics, and
  extglob leaves the vocabulary. The
  `houserules.glob-union-matcher` gotcha stays the parity test's
  first target; markdown render output is byte-pinned against the
  JS renderer's output.

## 4. Parity gates (the heart of the migration)

- A frozen corpus of fixtures generated by the JS implementation at
  the migration base: rendered rules files, generated skill, audit
  JSON over recorded ranges, validate verdicts over the workspace
  archive, backlog command outputs.
- Gate: the Rust binary reproduces the corpus byte-identically
  (render, files manifest, check/validate messages) and
  field-identically (audit JSON, allowing no differences). The JS path
  retires only after the corpus gate holds on all platforms in CI.
- The existing vitest suite stays the behavioral contract only while
  a surface is unported: each phase ports its surface's tests to
  cargo test alongside the code (owner ruling: the dev tooling
  migrates too), the vitest suite shrinks phase by phase, and a
  phase's JS surface retires only when its cargo tests run green and
  its corpus slice holds. At full retirement no vitest, commitlint,
  pnpm, or node_modules remains.

## 5. Phasing (build batches, each owner-gated as usual)

1. Corpus + scaffold: the cargo workspace, CI matrix, the frozen
   fixture corpus, `render` and `check-knowledge` in Rust behind the
   parity gate; their tests land as cargo tests from day one.
2. `audit`, `validate`, `stats`, and the backlog commands (`list`,
   `get`, `set`, `batch`, `check-backlog`) — the deterministic check
   runners and serde schema models, with their vitest cases ported to
   cargo test. Also the knowledge read commands (`get`, `for`,
   `index`, `topics`, `standing`), assigned here with the batch 17
   plan (owner-approved 2026-09-04): they share this phase's models,
   `for` is a dormant glob call site, and phase 3's reference
   rewrite needs every command shipped.
3. `init`/`update`/`files` — KIT_OWNED sync with the new deletion
   capability, the stamp and drift line, settings merge; the
   reference rewrite across every shipped file to the flat commands;
   `tools/kb.sh` and `tools/backlog.sh` deleted from the payload and
   from installs at the next `update`; `check-commit` lands and the
   hook's commitlint probe becomes `houserules check-commit`.
4. Delivery (HR-048): cargo-dist (or cross/zig — settled in phase 1
   by a spike) wires per-platform binaries with checksums into the
   release-please release; `include-component-in-tag` flips false in
   the same batch; README/runbook/skills update; HR-049's pin
   mechanism lands here; macOS signing decision presented to the
   owner with costs.
5. Channels and full retirement: mise ubi (works from release assets
   with the `v*` tags), the asdf plugin, tap/winget — owner-attended
   external acts. The JS path retires completely: the remaining
   vitest suite, commitlint, pnpm, package.json, the lockfile, and
   node_modules leave the tree; mise pins the Rust toolchain instead
   of node/pnpm; CI runs cargo; the invariant amendment and the
   repository sweep below complete the arc.

Post-processing obligation (owner ruling, 2026-09-04; the closing
task of phase 5): scan the full repository and correct every rule,
knowledge entry, backlog item, and doc to the new setup — flat
`houserules` commands, Rust/cargo tooling, binary distribution. The
sweep covers the README, CLAUDE.md, design.md's live sections, the
runbook, the skills, the agent templates, knowledge entries' bodies
and verify commands, and open backlog item bodies. Standing entries
that encode the old world (`houserules.pnpm-only`,
`houserules.template-is-the-source`'s Node update command, the
payload invariant) amend or retire, each recorded in design.md §5.
Historical records keep their wording: the CHANGELOG, decision rows,
past specs and plans, eval records, and closed items' histories state
what was true when written. The gate is mechanical: `git grep` over
the live files finds no reference to `kb.sh`, `backlog.sh`,
`houserules.mjs`, pnpm, vitest, or commitlint when the sweep is done,
with the historical files above as the only allowed matches.

## 6. What does not change

- The knowledge/backlog JSON formats, ids, and schemas; the agent
  templates and skills in kind; the batch process itself.
- The commit-msg hook stays POSIX shell — only its commitlint probe
  becomes `houserules check-commit` (phase 3).
- What DOES change beyond the port (owner rulings, 2026-09-04): the
  dev tooling migrates with it — vitest to cargo test phase by phase,
  commitlint to the built-in check, pnpm/package.json/node_modules
  gone at retirement — and the phase-5 repository sweep (§5) corrects
  every rule, knowledge entry, backlog item, and doc to the new
  setup. Nothing JS survives phase 5.
- CLI failure paths (RULED by the owner at the batch 16 report, 2026-09-04; raised batch 16 T3): where the JS
  re-throws and node prints a stack trace with exit 1, the binary
  prints one named error line to stderr and exits 2. Success and
  stale/check paths keep byte parity; every ported command pins its
  error arms with tests.
- Eager glob validation (RULED by the owner at the batch 16 report, 2026-09-04; raised batch 16 T3 re-review, reviewer-endorsed): the binary validates every areas.json glob at load
  time and fails with a named error where the JS tolerated a bad
  glob silently until match time — the exact failure mode the
  glob-union-matcher gotcha was born from. A new failure only on a
  base whose areas.json declares a glob globset rejects; no such
  base exists here or in the corpus.
- Regex reason text (RULED by the owner at the batch 16 report, 2026-09-04; raised batch 16 T4 review):
  check-pattern VALIDITY verdicts match the JS exactly — an
  ECMAScript-regex engine decides them, so exit codes and finding
  lists agree for every pattern and flags value. The parenthesised
  V8 reason text is reproduced for the recognised categories; other
  categories carry the Rust engine's reason wording, because full
  V8 reason-text parity is unreachable without embedding a JS
  engine.
- Coverage floors during the shrink (RULED by the owner at the batch 16 report, 2026-09-04; raised batch 16 T4 review, mechanism corrected at the r1 re-review): the global vitest thresholds
  keep their pre-port values over the still-JS-owned files; a file
  whose surface has ported to cargo leaves the main coverage run
  and gets its own separately-ratcheted vitest coverage run pinned
  at the measured post-removal numbers — a per-file ratchet, never
  a global drop. (A glob-keyed threshold beside the global in one
  config cannot express this: vitest applies the global threshold
  to every included file, verified in the installed runtime.)
  Every still-shipped JS surface keeps at least one behavioral
  vitest gate; for the check path that gate drives the live
  template/tools/kb.mjs over the frozen corpus check slices.
- Crash paths (RULED by the owner at the batch 16 report,
  2026-09-04; raised batch 16 T4 r1 re-review): where
  the frozen JS dies with an uncaught runtime error on malformed
  data — a non-string `verify` entry (TypeError from path.join), a
  schema pattern its engine cannot compile, an unsupported or
  unresolved schema `$ref` — the binary instead
  reports named check findings with exit 1. Reproducing a crash
  would be wrong; the deviation is that the binary is reachable
  where the JS was not.
- CLI argument parsing (controller-accepted deviation, batch 17 T2
  review, pending owner confirmation at the batch report): the
  binary parses argv with clap, so a flag the JS's parseArgs
  silently ignored, coerced to a bare true, or let a duplicate
  override becomes a named usage error at exit 2, and usage lines
  name the flat surface. Reproducing parseArgs's ambiguity would
  silently swallow typo'd flags. Every observable instance —
  including the four JS success paths and the error-text
  differences the T2 review enumerated — is pinned by a
  counterexample test asserting the clap answer.

## 7. Out of scope

- Any behavior change to the kit's semantics during the port (parity
  first; improvements are separate backlog items after retirement).
  The flat command surface is the one sanctioned exception: the
  reference rewrite maps old invocations to the flat names.
- Modular installs (backlog-only, rules-only): the crate boundaries
  prepare for them; the feature itself is HR-053, gated after the
  port.
- The wrapper npm package (additive later, only on demand).
- tag-pilot adoption (decision 4 re-raises at the first binary
  release).
