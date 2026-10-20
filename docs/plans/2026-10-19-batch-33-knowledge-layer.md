# Batch 33 plan — the knowledge layer

> **For agentic workers:** dispatch through `.claude/agents/` (`implementer`,
> `task-reviewer`, `branch-reviewer`) per `.claude/skills/orchestrating/SKILL.md`.

**Goal:** the kit's seeded rules become five versioned packs copied into
the tree; a project adds, upgrades, overrides, excludes, and removes
packs; `update` converts every 1.x install; `render` writes AGENTS.md
blocks for every harness and Claude Code files on request; houserules
installs into a subdirectory.

**Spec:** `docs/specs/2026-10-19-batch-33-knowledge-layer.md` (approved
2026-10-19, design.md 5.169, with controller calls C1 to C15; C16 to C25
approved on 2026-10-20, design.md 174). Every
task reads the spec sections it names; a brief quotes them by pointer
(`process.brief-carries-the-spec`). Branch: `batch-33`. Workspace:
`/home/jblossey/projects/houserules/.superpowers/sdd/2026-10-10-batch-33/`
(absolute path in every dispatch).

**Tier: full** (criteria: the batch changes `crates/houserules/src/**`,
`houserules.cli-changes-run-full-tier`, and `template/**`,
`houserules.template-changes-run-full-tier`, plus a schema and the data
of every install). Re-review cap: 2 per task. Branch review: yes. Eval
run: yes (T7 changes the implementer template, `process.evals-rerun`).
From Task 9 on: the standard tier (design.md 172, the full tier purged):
re-review cap 1, the branch review on opus.

**Order and parallelism (`process.parallel-on-disjoint-files`):**

| Wave | Tasks | Why |
|---|---|---|
| 1 | T1 ∥ T5 | disjoint file sets (below); neither reads the other's output |
| 2 | T2 | needs T1's pack module |
| 3 | T3 ∥ T4 | both need T2's merged base; disjoint files (T3 owns `check.rs`, `kit_citations.rs`, the marker gate; T4 owns the new `pack` modules and `main.rs`) |
| 4 | T6 | needs T4's L8 and area functions |
| 5 | T7 | then the controller converts this repository (C-step 2) |
| 6 | T8 | render port; needs T7's `renderers` and T3's `check.rs` |
| 7 | T9 | documents; needs every behavior |

Each parallel implementer works in its own worktree from the same
`BASE`, with the scratch root `<scratchpad>/task-<N>/`. The controller
integrates one task at a time (cherry-pick), runs the full gates after
each, and writes the commit map to the ledger. Up to four agents run at
once in this batch (owner, design.md 5.170): the implementers of a
parallel wave and the reviews of tasks already closed.

**Approved:** by the owner on 2026-10-19 (design.md 5.170).

## Cost measurement (design.md 5.170, `process.full-tier-measures-cost`)

- `efficiency-windows.json` holds one window per phase: spec (two
  windows, measured), plan (measured), each wave (its implementers,
  reviews, fix rounds, and integration), C-step 2, the eval run, the
  live run, the branch review and its fix wave.
- At each phase end the controller runs `efficiency.py` and saves
  `efficiency.json`; each ledger close row carries the dispatch's
  tokens, tool uses, and duration.
- Baseline: `efficiency-baseline.json` (batches 28 to 33 from the
  transcripts). At batch close, the batch report compares batch 33 per
  phase and per role with it.

## Global constraints

- The commands, flags, keys, file names, and printed lines are the
  spec's, verbatim. A printed line the spec quotes is pinned by a test
  that compares the whole line.
- No new dependency. git runs through `std::process::Command`, as the
  binary does today (spec F10, 5.164).
- Every pack copy, `overrides.json`, and `.houserules.json` is written
  in emit form (`crates/houserules/src/emit.rs`, HR-162).
- Edit the kit in `template/`; the controller runs `houserules update
  --dir .` with the tree binary and regenerates
  `crates/houserules/payload.stamp`
  (`houserules.template-is-the-source`,
  `houserules.controller-gates-use-the-tree-binary`,
  `houserules.payload-stamp-gate`). A task touches neither the root
  copies of kit files nor the generated files (`.claude/rules/`, the
  `project-knowledge` skill, AGENTS.md blocks), nor `backlog/`, nor
  `payload.stamp`.
- Until T7, this repository keeps its 1.x layout. Every task keeps the
  repository's own gates green on it: T1 adds the packs beside
  `template/knowledge/*.json` and removes nothing; T7 removes the 1.x
  topic files from `template/knowledge/`, and the controller converts
  this repository in the same integration (C-step 2).
- Path pins build paths component by component
  (`houserules.path-pins-mirror-the-code`); platform code is gated with
  `#[cfg]` (`houserules.platform-gated-tests`).
- Writing style: ASD-STE100 (`writing-style.principles`); doc comments
  state the current contract (`writing-style.doc-comments`), no history
  (`writing-style.code-comments`).
- Commit types: a behavior change under `crates/houserules/` is `feat`
  or `fix` (`houserules.commit-type-under-the-crate-path-decides-a-release`);
  tests-only and refactors use `test` and `refactor`.

## Corpus runs (`houserules.plan-semantics-run-on-the-corpus`)

- The conversion classification of spec section 5 (steps 1 to 4,
  "changed" as 1.x `classify` Modified, the 2.0 pack entry without the
  six cross-pack `see` links):
  `plan-corpus-conversion.py`, outputs `plan-corpus-conversion-repo.txt`
  (this repository: step 1 writes 34 overrides, step 2 makes 24 pack
  entries, 52 ids stay local, `writing-style.json` goes) and
  `plan-corpus-conversion-fresh-1-4-0.txt` (a fresh 1.4.0 install: 58
  pack entries, 0 overrides, 0 exclusions, all five topic files go). T7's
  tests pin both results.
- The directory-block rule of spec section 7:
  `plan-corpus-directory-blocks.py`, outputs
  `plan-corpus-directory-blocks-repo.txt` (blocks for `cli` in
  `crates/houserules/src/`, `template`, and `tests`; `docs`, `tools`,
  `rust` get root sections only; `global`, `process` get neither) and
  `plan-corpus-directory-blocks-template.txt` (a fresh install: a block
  for `tools` only). After C-step 2 sets `"agentsMd": false` for
  `template`, this repository gets blocks for `cli` and `tests` (spec
  section 11). T8's tests pin both.

## Review focus

1. A pack copy checked out with CRLF line endings, or with keys in
   another order after a hand reformat: the `sha256` pin still passes,
   because it hashes the parsed value (L3, F6). Test in T1.
2. A `git+` source whose URL holds `@` and `:` (`git+ssh://git@host:2222/o/r@v1.2.0#packs/p.json`),
   an annotated tag, and a tag that names a branch: the split, the
   peeled commit, and the refusal of L3a. Test in T4.
3. An `update` interrupted after the writer wrote some files: the next
   `update` finishes the conversion, because every state is tested again
   and the `schema` key is written last. Test in T7 (a writer that fails
   after step 5, then a clean run).
4. A directory AGENTS.md with CRLF line endings and project text before
   and after the block: removing the block keeps the project text byte
   for byte. Test in T8.
5. A subdirectory install whose folder name holds a space or a
   non-ASCII letter: the workflow file name, the `paths` filter, and the
   `:/` baseline key stay valid. Test in T6.

## Code-health scan (`process.code-health-scan`)

- `crates/houserules/src/rules/model.rs` `load_base` (95 lines) reads
  topic files with a hand-typed exclusion of `schema.json` and
  `areas.json`, repeated in `archive.rs:287` and `install.rs:270-276`
  (F1). T2 adds the one `RESERVED` constant and splits the loader:
  reading local topics, reading pack copies, and merging are three
  functions.
- `model.rs`'s `Base` doc comment narrates load history and other
  modules' needs at length. T2 rewrites it to the current contract of
  the merged base.
- `crates/houserules/src/rules/check.rs` `check_base` is 223 lines with
  one concern per block. T3 moves the new pack checks into
  `rules/check_packs.rs` and the marker gate into `rules/markers.rs`, and
  `check_base` calls them; T8 moves the budget block (`check.rs:955-1005`)
  into `rules/budget.rs`.
- `crates/houserules/src/install.rs` (1665 lines): `update` (98 lines)
  mixes the `KIT_OWNED` sync, the topic reconciliation, `SEED_ONCE`
  backfill, and the stamp. `upsert_topic_entries` (118 lines) is the
  1.x entry reconciliation that packs replace. T6 moves the marker
  read and write to `install/marker.rs` and the file sync to
  `install/sync.rs`; T7 moves the conversion to `install/conversion.rs`
  and deletes `upsert_topic_entries` and `stamp_topic_baselines` once
  the conversion owns their job. `install.rs` becomes `install/mod.rs`.
- `crates/houserules/src/rules/render.rs` `render_all` (88 lines) builds
  every output in one function. T8 splits it into a renderer port
  (`render/mod.rs`), `render/agents_md.rs`, and `render/claude.rs`.
- `crates/houserules/src/rules/audit.rs` `changed_files` and
  `tree_files` each parse git output their own way; T5 routes both
  through one install-root path mapping.
- `crates/houserules/src/rules/audit.rs` `audit` (206 lines) is not
  touched beyond T5's path mapping; noted, no change (no task needs it).

---

### Task 1: the five kit packs as data (HR-182, HR-162, HR-178, HR-195 entry, interaction 10)

**Files:**
- Create: `template/packs/knowledge-base.json`, `process.json`,
  `quality.json`, `security-hygiene.json`, `writing-style.json` (emit
  form; L1: `name`, `version` `1.0.0`, `schema`
  `houserules/pack@1.0.0`, `topics` with the 1.4.0 titles, `areas` with
  the globs of `template/knowledge/areas.json` for each area its entries
  use, `entries`)
- Create: `crates/houserules/schemas/pack-1.json`,
  `crates/houserules/schemas/overrides-1.json` (embedded with
  `include_str!`; the pack name pattern `^[a-z0-9-]+$` and `not` `local`,
  L1)
- Create: `crates/houserules/src/packs.rs` (the kit pack set, the pack
  copy type, the name check, the canonical `sha256`, validation against
  the embedded schema by the declared schema id, L6)
- Modify: `crates/houserules/src/main.rs` (`mod packs;` only)
- Modify: `template/.claude/skills/migrating-knowledge/SKILL.md` lines 17
  and 237 (interaction 10: name no id outside the `knowledge-base` pack)
- Not touched: `template/knowledge/*.json` (they stay until T7), every
  other file of T5's set

**Content rules (spec 1, 3 L1, 9, 10; F9):**
- Each 1.4.0 entry goes to the pack named by its id's first part, with
  its 1.4.0 text, except: the six cross-pack `see` targets of F9
  (`cross-pack-refs.txt`) leave the `see` arrays; an empty `see` is
  removed.
- `knowledge-base` gains `knowledge-base.cite-durable-refs` (HR-178): the
  text of this repository's `knowledge/knowledge-base.json` entry, with
  its `durable-sha` check at fail level, rewritten to name no id outside
  the `knowledge-base` pack and no path of this repository.
- `knowledge-base` gains one `gotcha` entry for HR-195 (spec section 10,
  last bullet): what was measured (a context proxy can shorten or drop
  houserules output with no visible sign; the source files and the gates
  are the truth); no proxy setting (5.137, the O7 constraint).
- A test pins each pack's checksum to its version (5.165(3)): a table
  `(name, version, sha256)` in `packs.rs`'s tests; changing a pack
  without changing the table fails.

**Interfaces (produced):**
- `pub(crate) struct PackCopy { name: String, version: String, schema: String, topics: Vec<(String, String)>, areas: Vec<(String, Vec<String>)>, entries: Vec<Value>, raw: Value }`
- `pub(crate) fn kit_packs() -> Vec<PackCopy>` (from the payload, sorted by name)
- `pub(crate) fn parse_pack(raw: Value, origin: &str) -> Result<PackCopy, String>` (validates the schema id, the embedded schema, the name, and that each id starts with a declared topic; the error names `origin` and the case)
- `pub(crate) fn pack_sha256(raw: &Value) -> String` (`sha256` of `serde_json::to_vec` of the parsed value, as `install::canonical_entry_bytes`)
- `pub(crate) fn valid_pack_name(name: &str) -> bool`
- `pub(crate) const PACK_SCHEMA_MAJOR: u64 = 1;`

- [ ] **Step 1: Failing tests first** (`packs.rs` tests): `kit_packs`
  returns five packs whose entry ids, together, equal the 1.4.0 kit ids
  plus the two new ids, each id once; no entry's `see` names an id of
  another pack; every pack copy is in emit form
  (`emit::to_string(&raw) == file bytes`); `pack_sha256` is equal for a
  CRLF copy and a reordered-whitespace copy and differs for a changed
  summary (Review focus 1); `parse_pack` refuses `../schema`, `a/b`,
  `local`, `Upper`, a newer major (`houserules/pack@2.0.0`), and an id
  outside the declared topics, each with its line; the version table
  matches. Run `cargo test --locked -p houserules packs::` and record
  RED.
- [ ] **Step 2:** write the five pack files and the two schemas; write
  `packs.rs`. GREEN. Run the full gate list (`mise run test`,
  `mise run lint`) and record it.
- [ ] **Step 3: Commit** `feat(packs): ship the kit's seeded rules as five versioned packs`.

### Task 2: the merged base (HR-182, HR-191; spec 3 L4, L7, L10, L11; 4.1; 4.3)

**Files:**
- Modify: `crates/houserules/src/rules/model.rs` (split loader; `RESERVED`; merged base)
- Modify: `crates/houserules/src/rules/read.rs` (`topics`, `index --topic` across sources, L11)
- Modify: `crates/houserules/src/get.rs` (C2: `excluded`, `overridden` fields)
- Modify: `crates/houserules/src/archive.rs` (`RESERVED`; local entries only, L10)
- Modify: `crates/houserules/src/install.rs:270-276` (`RESERVED` only)
- Modify: `crates/houserules/src/rules/stats.rs`, `rules/proposals.rs` (spec 4.3: `source` per `rules` row, a `packs` key)
- Not touched: `check.rs`, `audit.rs`, `render.rs` (they read `Base`;
  their outputs change only through it)

**Semantics (spec 4.1, exact):** the merged base is the local topic
files, every `knowledge/packs/*.json` copy, and `knowledge/overrides.json`.
An override replaces whole fields; `null` removes a field (F5). An
excluded entry is in no `entries`, `standing`, `for`, `index`, `audit`,
or `render` output; `get` prints it with `excluded` (the reason, or
`true`) and exits 0. `get` of an overridden id prints the effective
entry and `overridden` (the replaced field names, sorted). One id in two
active sources: the local entry wins over a pack entry, the first pack
in name order over another, and the pair is recorded for `check-knowledge`
(T3). A topic's title: the local file's `title`, else the first pack's
(name order). `topics` sums every source's entries. `stats` rows gain
`source` (`local` or the pack name); `packs` sums per source: `entries`,
`standing`, `judged_rows`, `violations`.

**Interfaces (produced):**
- `pub(crate) const RESERVED: [&str; 3] = ["schema.json", "areas.json", "overrides.json"];` in `model.rs`
- `pub(crate) enum Source { Local, Pack(String) }`; `Entry` gains `source: Source`
- `Base` gains `packs: Vec<PackCopy>`, `overrides_raw: Value`, `excluded: BTreeMap<String, Value>`, `overridden: BTreeMap<String, Vec<String>>`, `duplicates: Vec<(String, Source, Source)>`, `archived_ids: BTreeSet<String>`
- `load_base` keeps its signature; a missing `knowledge/packs/` or `overrides.json` is the 1.x shape and loads as empty

- [ ] **Step 1: Failing tests:** one test per Semantics sentence, built
  in a `TempDir` the way `read.rs` tests build a base; a `stats` test
  on a workspace fixture with one local and one pack rule; an `archive`
  test that a retired override leaves the pack copy untouched. RED.
- [ ] **Step 2:** implement; rewrite `Base`'s doc comment to the current
  contract. GREEN; full gates.
- [ ] **Step 3: Commit** `feat(knowledge): read one merged base of local entries, pack copies, and overrides`.

### Task 3: `check-knowledge` for packs, and the marker gate (HR-182, HR-151, HR-195; spec 3 L5–L7; 4.1; 6 first bullet; 7 CLAUDE.md bullet; 9 HR-151; 10)

**Files:**
- Create: `crates/houserules/src/rules/check_packs.rs` (pins, schema
  majors and minors, L5, duplicates, active-and-archived, `see` to an
  excluded id, the override and exclusion listing, area keys, the
  reserved topic, the `CLAUDE.md` import)
- Create: `crates/houserules/src/rules/markers.rs` (the pattern list and
  the string-value walk)
- Modify: `crates/houserules/src/rules/check.rs` (`check_base` calls the
  two modules; the area enum is no longer required)
- Modify: `crates/houserules/src/rules/kit_citations.rs` (the three
  batch-33 lines of spec section 9, `ownedPaths` in place of
  `overrides`)
- Modify: the backlog check (`crates/houserules/src/backlog/`) and
  `rules/validate_deliverable.rs` (call `markers`)
- Not touched: `main.rs`, the `pack` modules (T4), `render.rs`

**Semantics:** spec 3 L5 (four cases, one line each naming the id and
the case), L6, L7; every `check-knowledge` bullet of 4.1 (the
duplicate-id bullet and the newer-minor sentence, C13, included); 6
bullets 1 and 6; 7 (the `CLAUDE.md` line, C15); 9 HR-151 (the three
kit-citation lines, verbatim); 10 (the three headroom patterns as one
`regress` pattern each, the walk over parsed string values only, the
finding names the file and the field path). The headroom patterns are
the ones F8 quotes; the brief quotes them verbatim.

- [ ] **Step 1: Failing tests**, one per line the spec quotes, each
  comparing the whole line; a marker inside a JSON key is not found; a
  marker in `knowledge/archive/` and in `knowledge/packs/` is found
  (L12). RED.
- [ ] **Step 2:** implement. GREEN; full gates (this repository's own
  `check-knowledge` stays green: it has no packs yet, and its `CLAUDE.md`
  imports `AGENTS.md`).
- [ ] **Step 3: Commit** `feat(check): check pack copies, overrides, and compression markers`.

### Task 4: the `pack` commands, areas, and one-id-one-source (HR-182, HR-191; spec 3 L2, L3, L3a, L8, L9; 4.2; 6; C7, C11)

**Files:**
- Create: `crates/houserules/src/pack/mod.rs` (`list`, `add`, `remove`, `upgrade`)
- Create: `crates/houserules/src/pack/source.rs` (the three source forms; the git fetch)
- Create: `crates/houserules/src/pack/areas.rs` (spec 6: add, remove, `addedAreas` moves)
- Create: `crates/houserules/src/pack/one_source.rs` (L8, L9, the archived case)
- Modify: `crates/houserules/src/main.rs` (`Command::Pack` with `list`, `add <source>`, `remove <name>`, `upgrade <name> [<tag>]`, each with `--dir`)
- Create: `crates/houserules/tests/pack.rs` (integration: a local bare repository with tags as the git host)
- Not touched: `check.rs`, `kit_citations.rs`, `install.rs`

**Semantics:** spec 4.2 (every bullet, every refusal line, in order),
L2, L3, L3a (the split rule; `git ls-remote --tags <url> refs/tags/<tag>
refs/tags/<tag>^{}`, the peeled line wins; a ref with no tag line is
refused), L8 (with the archived case and the replace lines), L9, 6
(bullets 2 to 5), C7 (`backlog/` present is the tracker), C11. The git
fetch: `git init --bare` in a temp directory, `git fetch --depth 1 <url>
refs/tags/<tag>`, `git show FETCH_HEAD:<path>`; every git error is one
named line (`houserules.crash-paths-are-named`). Each write goes through
`emit`; nothing is written before every refusal check passes.

**Interfaces (produced, used by T7):**
- `pub(crate) fn one_source(root: &Path, base: &Base) -> Result<Vec<String>, String>` (applies L8 and L9; returns the printed lines)
- `pub(crate) fn add_pack_areas(root: &Path, pack: &PackCopy, record: &mut Vec<String>) -> Result<Vec<String>, String>`
- `pub(crate) fn remove_pack_areas(root: &Path, removed: &PackCopy, remaining: &[PackCopy], base: &Base) -> Result<Vec<String>, String>`
- `pub(crate) enum PackSource { Kit, File(String), Git { url: String, tag: String, path: String } }` with `parse` and `Display` (the recorded string)

- [ ] **Step 1: Failing tests** for every refusal and every printed line
  of 4.2, L8's three cases and its replace lines, the archived case,
  L9's empty-file case, section 6's four rules, Review focus 2, and the
  spec 16 cases: "fresh init … pack add of an upstream pack … pack
  remove", "git source … moved tag", "`pack upgrade` … adds one id and
  drops one", "`pack add` of a file whose pack name is `../schema`, or
  `local` … a branch refuses", and "after that conversion, `pack add` of
  the pack for the deleted `ownedPaths` topic" (its 2.0 half: an
  archived local entry becomes a retired override). RED.
- [ ] **Step 2:** implement. GREEN; full gates.
- [ ] **Step 3: Commit** `feat(pack): add, list, upgrade, and remove packs`.

### Task 5: the subdirectory root, install-root paths, and `defaultBranch` (HR-147, HR-131; spec 8 bullets 1, 3, 4, 5 (the path mapping); 9 HR-131)

**Files:**
- Modify: `crates/houserules/src/root.rs` (nearest `.houserules.json` at
  or above the cwd, no higher than the git top level, else the top
  level)
- Create: `crates/houserules/src/rules/git_paths.rs` (map git's
  top-level paths to install-root paths; drop outside paths)
- Modify: `crates/houserules/src/rules/audit.rs` (`changed_files`,
  `tree_files` through `git_paths`)
- Modify: `crates/houserules/src/rules/check_commit.rs` (skip commits
  that touch no install-root path; print the skipped count line)
- Modify: `template/.githooks/commit-msg` (pass `--dir` with the install
  root read from the hook's own path)
- Modify: `crates/houserules/src/rules/durable_sha.rs` (`defaultBranch`
  first, then F11's order; the not-found line names `defaultBranch`)
- Not touched: `install.rs`, `model.rs`, `template/knowledge/**`, `template/packs/**`

**Semantics:** spec 8 bullets 1, 3, 4, and the path-mapping bullet, verbatim;
9 HR-131. An install at the git top level maps every path to itself, so
every existing audit and check-commit test stays green unchanged.

- [ ] **Step 1: Failing tests:** root resolution from `sub/x/` finds
  `sub/.houserules.json`; from a folder with none, the top level; the
  spec 16 case "In a subdirectory install, a change inside `sub/` that
  breaks an area rule fails `audit`, and a change outside `sub/` is out
  of scope"; check-commit's skip line and count; the hook from
  `sub/.githooks/` passes `--dir sub`; `defaultBranch` wins over
  `origin/HEAD`. RED.
- [ ] **Step 2:** implement. GREEN; full gates.
- [ ] **Step 3: Commit** `feat(root): resolve a subdirectory install and match install-root paths`.

### Task 6: the 2.0 install marker, `init`, the file sync, and several installs (HR-147, HR-184 init half; spec 3 L13, L14; 5 sync paragraph and run order (4); 7 init bullets; 8 bullets 2, 5, 6, 7; C9, C12)

**Files:**
- Move: `crates/houserules/src/install.rs` → `install/mod.rs`
- Create: `crates/houserules/src/install/marker.rs` (`.houserules.json` 2.0: `schema`, `version`, `idPrefix`, `packs`, `ownedPaths`, `defaultBranch`, `renderers`, `baselines`; read and emit)
- Create: `crates/houserules/src/install/sync.rs` (the path-baseline sync: writes as `init` writes, restores a missing file, skips unchanged bytes silently, skips the Claude files while `renderers` lacks `claude`, `ownedPaths` never rewritten, `:/` keys for the workflow)
- Modify: `crates/houserules/src/main.rs` (`init --target claude`, `init --dir <subdir>`)
- Modify: `template/.github/workflows/knowledge.yml` (the subdirectory form: `working-directory`, `paths`, `name`) and the code that writes it
- Not touched: the conversion (T7), `render.rs` (T8)

**Semantics:** `init` writes install schema `houserules/install@2.0.0`,
the five kit packs with pins (`source: kit`), no topic files, `renderers`
`[]` or `["claude"]`; without `--target claude` it writes no
`.claude/agents/*.md`, no `tools/claude-session-start.sh`, merges no
hook entry into `.claude/settings.json`, and prints the enable line;
`init --target claude` on an existing install adds `claude`, seeds the
missing Claude files, keeps the rest, renders. Spec 8: the workflow file
name rule, `name: knowledge (<path>)`, the `:/` keys (top level too),
both refusals and the own-root case (C12). The sync paragraph of spec 5
and run-order point (4); a second `init` or `update` prints no `wrote`
line (R2-2).

- [ ] **Step 1: Failing tests:** each Semantics sentence; the spec 16
  cases "`init` then `update`: every knowledge file is byte-identical
  (HR-162)", "`init --dir sub` … every command from inside `sub` finds
  the install", and "Several installs (5.168)" (all five refusals and
  writes); Review focus 5. RED.
- [ ] **Step 2:** implement; move the marker and sync code out of
  `install.rs` with no behavior change first (a `refactor` commit), then
  the new behavior. GREEN; full gates.
- [ ] **Step 3: Commits** `refactor(install): split the marker and the file sync` and `feat(init): write the 2.0 install and support subdirectory installs`.

### Task 7: the 1.x conversion, pack moves on `update`, and D4 (HR-182, HR-151; spec 5 steps 0–9, the pure-function paragraph, D4, the pack moves; 9 HR-151 (2), (4); C3, C5, C8, C14)

**Files:**
- Create: `crates/houserules/src/install/conversion.rs` (a pure function from a 1.x tree to the 2.0 layout plus a hand-step list; a writer)
- Create: `crates/houserules/released/` (the released 1.x contents of `knowledge/schema.json` and `.github/workflows/knowledge.yml`, one file per distinct content, from `git show v1.0.0..v1.4.0`; an index file names the versions of each)
- Modify: `crates/houserules/src/install/mod.rs` (`update` runs the run order of spec 5; delete `upsert_topic_entries` and `stamp_topic_baselines`)
- Modify: `crates/houserules/src/main.rs` (D4 before every command but `update`, `init`, `files`; the line names `--id-prefix` when `.houserules.json` is absent)
- Delete: `template/knowledge/knowledge-base.json`, `process.json`, `quality.json`, `security-hygiene.json`, `writing-style.json`; modify `template/knowledge/schema.json` (no `$defs.area`)
- Modify: `crates/houserules/tests/update.rs`, `tests/dogfood.rs` (the pack layout)
- Not touched: this repository's own `knowledge/`, `.houserules.json` (C-step 2)

**Semantics:** spec 5 in full: every step's state and action, the
precedence, "changed", step 0's stop, the run order, D4's command list
and lines, the `--id-prefix` refusal (C14), the three pack-move bullets.
The function takes the binary's packs as input and reads no text to
assign a pack (spec 5, pure-function paragraph).

- [ ] **Step 1: Failing tests:** the corpus results (a fixture copy of
  this repository's `.houserules.json` and `knowledge/` at the plan's
  commit gives 34 overrides from step 1, 24 pack entries, 52 local ids,
  `writing-style.json` removed; a fresh 1.4.0 install gives 58 pack
  entries and nothing else); the spec 16 cases "A 1.4.0 install … with a
  modified kit entry …" (every listed state, the D4 half, the second
  `update`), "An unmodified 1.4.0 install converts with no override",
  "A converted install, then `pack remove process` … nothing changes",
  "A 1.4.0 install with a topic file `knowledge/overrides.json`",
  "A 1.4.0 install whose `CLAUDE.md` holds its own text", "A
  hand-written local entry with a pack id … the next `update` turns it
  into an override"; Review focus 3; the update collision and the
  hand-edited-copy bullets. The 1.4.0 fixtures are seeded by the
  `v1.4.0` binary built from the tag in a scratch worktree, once per
  test run, cached under the target directory. RED.
- [ ] **Step 2:** implement. GREEN; full gates on the task's own tests.
  This repository's own `check-knowledge` now prints D4's line; the
  controller converts it at integration (C-step 2), so the brief tells
  the implementer to run the full gates in a scratch copy of the
  repository converted with the tree binary, and to record that run.
- [ ] **Step 3: Commit** `feat(update): convert a 1.x install to packs`.

### Task 8: the render port (HR-184; spec 7 every bullet; 11 budget; C1, C9, C15 render half)

**Files:**
- Move: `crates/houserules/src/rules/render.rs` → `rules/render/mod.rs` (the port), plus `render/agents_md.rs`, `render/claude.rs`
- Create: `crates/houserules/src/rules/budget.rs` (moved from `check.rs:955-1005`, the budgets of spec 7)
- Modify: `crates/houserules/src/rules/check.rs` (call `budget`)
- Modify: `template/AGENTS.md`, `template/CLAUDE.md` (the seed holds the marked block)
- Modify: `template/.claude/agents/implementer.md` (the standing line of spec 7, last bullet; the brief quotes the new line verbatim)
- Modify: the seeded texts of spec 11 (the C9 note): `template/AGENTS.md:30`, `template/.claude/schemas/deliverables.json:4`, orchestrating `SKILL.md:61` and `:108`, migrating-knowledge `SKILL.md:99` (the brief re-runs the spec's grep and lists each line)
- Modify: `tests/render_parity.rs`, the render goldens

**Semantics:** spec 7, every bullet, verbatim; the search set and the
stale-block rule (5.167); budgets per output; the `project-knowledge`
skill without its standing section and with the file-read retrieval
protocol (5.141(1), (2)).

- [ ] **Step 1: Failing tests:** each bullet; the corpus pins (the
  template's areas: a `tools` block only; this repository's areas with
  `template` opted out: `cli` and `tests`); the spec 16 cases
  "Directory blocks (5.167)", "A submodule and a vendored directory …",
  "A Codex-style read … and a Claude Code read …"; Review focus 4. RED.
- [ ] **Step 2:** implement. GREEN; full gates.
- [ ] **Step 3: Commit** `feat(render): write AGENTS.md blocks for every harness and Claude files on request`.

### Task 9: the documents (spec 11)

**Files:**
- Create: `docs/upgrading-to-2.md` (the batch-33 section: every hand step `update` can print, listed from T7's code with a grep the brief names)
- Modify: `template/docs/README.md` lines 15 to 76 (packs, overrides, exclusions)
- Modify: `README.md` (`pack`, `--target claude`, the subdirectory install; `KIT_OWNED` mirror per `houserules.readme-mirrors-kit-owned`)

- [ ] **Step 1:** a test that every hand-step string in
  `install/conversion.rs` appears in `docs/upgrading-to-2.md` (RED
  first).
- [ ] **Step 2:** write the documents. GREEN; full gates.
- [ ] **Step 3: Commit** `docs: document packs, render targets, and the subdirectory install`.

---

## Controller steps

1. After each integration: `houserules update --dir .` and `houserules
   render` with the tree binary where the task changed `template/` or a
   renderer; regenerate `payload.stamp`; run `mise run test` and
   `mise run lint`; commit the generated files as `chore`.
2. At T7's integration: convert this repository with the tree binary
   (`./target/debug/houserules update --dir .`), set `"agentsMd": false`
   for `template` in `knowledge/areas.json`, run every gate, run a
   second `update` and confirm `git status --porcelain` is empty, and
   commit `chore: convert this repository to the 2.0 knowledge layout`
   (spec 11 bullet 4; spec 16 "This repository converts"). Record the
   override and pack counts against the corpus run in the ledger.
3. After T8: the eval run of every `.claude/evals/` scenario, appended
   to `.claude/evals/record.json` (`process.evals-rerun`).
4. The live run (spec 16, every case, with the tree binary in scratch
   repositories; each case's output saved under the workspace as
   `live-run-<case>.txt`), including the marker case and the
   `mise run lint` case.
5. Batch close: `stats` over the five latest workspaces into
   `stats.json`; the branch review (`branch-reviewer`, opus, design.md
   5.158); its fix wave under the standard tier's cap of 1 (design.md 172).

## Spec acceptance cases → tasks

| Spec 16 case | Task |
|---|---|
| Fresh `init`, `pack list`, `pack add` upstream, override, exclusion, `pack remove` | T4 (T6 for `init`) |
| Git source, moved tag | T4 |
| `pack upgrade` to a second tag | T4 |
| Directory blocks (5.167) | T8 |
| Several installs (5.168) | T6 |
| `init` then `update` byte-identical (HR-162) | T6 |
| Unmodified 1.4.0 converts with no override | T7 |
| Converted install, `pack remove process`, `update` changes nothing | T7 |
| 1.4.0 install with a topic `overrides` | T7 |
| 1.4.0 `CLAUDE.md` with no `@AGENTS.md` | T7 (T3 for the check line) |
| Subdirectory audit in and out of scope | T5 |
| Pack name `../schema`, `local`, branch ref | T4 |
| Archived entry after `pack add` of the `ownedPaths` topic's pack | T4 |
| Submodule and vendored install untouched by `render` | T8 |
| Hand-written local entry with a pack id | T3 (the line), T7 (the `update`) |
| `init --dir sub`, commands inside `sub`, `check-commit --from` count | T5, T6 |
| The 1.4.0 install with every state; D4 before `update` | T7 |
| Codex-style and Claude Code reads | T8 |
| This repository converts; `mise run lint` passes | C-step 2 |
| Markers in a knowledge file, a backlog file, a deliverable | T3 |
