# Batch 33: the 2.0.0 major release

**Status: SUPERSEDED on 2026-10-15** by the owner's re-scope to discussion 59 (design.md 5.97). No task of this draft runs. **Status: DRAFT, not approved.** Prepared for the owner on 2026-10-10. The owner ruled decisions D1 to D11 the same day (section 8, design.md 5.96). The owner ruled the scope question Q1 the same day. The spec review is next; it takes the D10 rulings.

Owner rulings: design.md 5.94 (batch 33 is 2.0.0: HR-171 with HR-057 and HR-158, HR-173), 5.95 (one 2.0.0 after 1.4.0 collects every backlog item and GitHub issue that needs a major version, with a proper upgrade path), 5.96 (decisions D1 to D11 of section 8). Owner direction, 2026-10-10: prepare all changes, the decisions, and the ruling-free spec and plan parts; check assumptions and work from evidence.

Workspace: `.superpowers/sdd/2026-10-10-batch-33/`. Ledger `progress.md` holds every verification run cited here (V-ids).

## 1. Method

- Enumerations (rerunnable): `houserules list --open` (42 items), `gh issue list --state open` (issues 35, 38, 41, 50), `gh pr list --state open` (PR #49 batch 30, PR #53 Dependabot). Rendered to `inventory-items.md` and `inventory-issues.md` in the workspace.
- Each item claim that the classification relies on was re-checked against main at `v1.4.0`, with the tree binary. The ledger lists each check and its result.
- Measurement files in the workspace: `m1-report-output-share.txt` (inline output in task reports), `m2-code-health.txt` (the files this batch touches), `m3-hr175-corpus.txt` (HR-175's checks over the reports). The ledger holds the single-command checks (V-ids), V-lint included.

## 2. What needs a major version

`houserules.1-0-surface-is-frozen` (design.md 5.82) defines the frozen surface: the 20 subcommands and their flags, and the constraints of six schema files (`knowledge/schema.json`, `backlog/schema.json`, `.claude/schemas/deliverables.json`, and their `template/` copies). Adding, removing, or renaming a subcommand or flag, or changing a constraint, needs a major version. Additions count: a new subcommand, flag, optional field, or enum value is a major change under 5.82.

### 2.1 Why the schemas make a major release hard (verified)

- The three schema files are seed-once (`houserules files`; `install.rs` `SEED_ONCE`). `update` never rewrites a seed-once file that exists.
- The binary validates against the copy on disk (`rules/model.rs:222` reads `knowledge/schema.json`; `validate_deliverable.rs:26` reads `.claude/schemas/deliverables.json`).
- Result: a kit entry that uses a new check type fails `check-knowledge` in every existing install. Measured on a fresh 1.4.0 install: `knowledge/writing-style.json.entries[0].check.type: must be one of "grep-absent", "commits", "co-change", "diff-append-only", "report-field", "durable-sha"` (V-schema).
- `update` has a baseline mechanism for the 8 `KIT_OWNED` files and for each kit knowledge entry (`baseline.rs`). The schemas, the eval scenarios, `AGENTS.md`, and `CLAUDE.md` have none.

### 2.2 How adopters reach a new release (verified)

- `houserules update` first self-updates the binary to the latest release, across major versions. Only `HOUSERULES_SKIP_SELF_UPDATE` or `CI` stops it (`selfupdate.rs:77`, `:362-364`). Then the new binary runs the repository phase. On the day 2.0.0 ships, every adopter on the shell-installer channel who runs `update` gets the 2.0.0 binary and its migration. A mise or direct-download install has no receipt; it moves when the adopter upgrades it.
- The seeded CI workflow is seed-once. Installs seeded by 1.2.0 or later install the version `.houserules.json` records. Installs seeded by 0.3.0, 1.0.0, or 1.1.0 install the latest release (`git show <tag>:template/.github/workflows/knowledge.yml`; ledger V-ci). Their CI runs the 2.0.0 binary over a 1.x tree before the adopter updates.

## 3. Inventory and classification

Classes:

- **M**: needs a major version under 5.82 as written.
- **U**: upgrade-path work. The major release creates it, or it lowers the friction of every update.
- **C**: co-located. It needs no major on its own, but it changes files that an M item rewrites: the agent templates under eval, the checker, the report-field arm, `stats`. Separate delivery pays the same review and eval cost twice.
- **G**: blocked on an owner ruling that exists independent of 2.0.0.
- **S**: superseded by an M item's design.
- **N**: compatible and independent. Any minor release can carry it.
- **X**: out: proof pending, trigger-gated, calibration-gated, or internal to this repository.

| Item | Class | Evidence and note |
|---|---|---|
| HR-171 capture, inline caps, header checker | M | Ruled into 33 (5.94). New subcommand and a deliverables constraint. |
| HR-057 capture-file citations | M | Ruled into 33. Absorbed by HR-171. |
| HR-158 generated report facts | M | Ruled into 33. Absorbed by HR-171 and 4.4. |
| HR-173 check type that runs a linter | M | Ruled into 33. Adds an enum value to `$defs/check/properties/type`. |
| HR-175 report-field content checks | M | Ruled into 33. Adds check properties. Confirmed: the arm tests presence only (`audit.rs` `run_report_field_check`). |
| HR-139 required command patterns in a report list field | M | Same arm and schema change as HR-175. Recommended into 33. |
| HR-129 (2) `fix_rounds[].gates_run` | M | Adds a deliverables property. Batch 29 item (D5). |
| HR-144 / issue 38 design-phase review gate | M, out (D5: batch 29, a minor after 2.0) | Adds a `spec-review` deliverable kind. Batch 29 item (D5). |
| HR-138 (2) sanctions for judged rows | M or C | The item asks for a structural mark. A property on `auditRow` is M (`additionalProperties: false`); the `(sanctioned: <ref>)` evidence text that deterministic rows carry today is C. The spec gate picks the form. (1) the self_audit byte-compare is S: 4.4 has the audit write `self_audit`. |
| HR-088 `check-backlog --fix` | M, out (Q1: a minor after 2.0) | New flag. Under D2 it ships in a minor after 2.0. |
| HR-053 modular installs | M, out (Q1: a minor after 2.0) | New `init` selection. Under D2 it ships in a minor after 2.0. |
| HR-147 / issue 41 subdirectory install | M, out (D7: batch 34) | Still refuses at 1.4.0 (ledger, issue 41). Root resolution, hooks, CI, and `.claude/` all assume the top level (`root.rs`). D7. |
| HR-152 `stats` lists sanctioned rows apart | M (per its triage) | Confirmed: `stats.rs` never reads `sanctioned`. An output-shape change; 5.82 does not name output. In scope as C if D1 takes it. |
| HR-131 durable-sha default branch | G, in (D6) | Confirmed: `durable_sha.rs:245` fails when it finds no `origin/HEAD`, no `origin/main` or `origin/master`, and no local `main` or `master`. Any repair is a flag or a config key. D6. |
| Issue 50 seed `cite-durable-refs` (HR-178) | G, in (D6) | Confirmed unseeded. Contradicts design.md 5.88 ("the entry is not seeded"). A seeded fail-level `durable-sha` check exposes every adopter to HR-131. D6. |
| HR-162 seeded topic files in emit form | U | Confirmed and grown: the first `update` after `init` rewrites `quality.json` (77 lines), `security-hygiene.json` (96), and now `knowledge-base.json` (5). |
| HR-177 the CI gate on GitLab | dropped (D11) | Owner direction, 2026-10-10, then dropped the same day (D11): a host other than GitHub keeps the skip note of `init` and the CI instructions of `migrating-knowledge` ("Generate your own CI gate"). |
| HR-151 citation-remedy side effects | U | `update` on an unstamped install: prefix reset and entry restore. Same code as the migration (`install.rs` `update`). |
| HR-129 (1, 3, 4) | C | (1) partly addressed: `stats` reads findings into `cost` (`stats.rs:182`); violations still count adherence rows only. (3, 4) not in the templates. |
| HR-153 (1-4) template riders | C | None of the four is in the templates (grep). The standing-entry clauses are G (owner only). |
| HR-165 (4-7) template riders | C | Not in the templates. (1-3) are N. |
| HR-166 | C | Point 1 confirmed: the template pattern still reads `[^.;:]{0,40}`. Point 3: the orchestrating skill's CLAUDE.md sentence is unchanged. |
| HR-150 closing-act capture | S | The capture command replaces the redirect. The second gap (re-run the citation step after a report fix) moves into the rewritten closing act. |
| HR-072 paste-run lint narrowing | S | 4.2 retires the paste-run lint for captured runs. |
| HR-174 scratchpad paths in evidence | C | Correction: `check_ephemeral_paths` already scans the narrative fields (`report_claims.rs:1191`). The item widens it to `docs_verified[].source` and the run fields, which the checker rewrite touches. |
| HR-148 comma-list citations | C | Confirmed (Limits). The checker split (4.2) touches the same function. |
| HR-176 report-field evidence counts | C | Confirmed (`audit.rs:693`). Same arm as HR-175. |
| HR-164 stats texts | C | Confirmed (`proposals.rs:409`, `README.md:360`, `cost.rs:155`). Same files as HR-152. |
| HR-163 mechanize six rules | C + G | HR-173 is the mechanism. Per entry: a check, or the owner tags `ruled-keep`. |
| HR-058 empty-range audit | N | `empty_range` already in the summary (`audit.rs:1089`). No flag needed. |
| HR-127 verify-drift warn | N | An audit row; no surface change. |
| HR-128 checker over reviews | N | (a) runs the same checker on more files. (b) is a new narrative lint. |
| HR-130 loading-path lint | N | No check exists. A new `check-knowledge` lint reddens installs only after `update` (pinned CI). |
| HR-055 starter rule set | N | Content. Pairs with HR-053. |
| HR-042 workflow actions trigger vetting | G | Confirmed: the trigger lists manifests only. A standing-entry change: owner ruling pending since batch 11. |
| HR-172 narrow rulings-to-file | G | Standing, owner-seeded. |
| HR-141 / issue 35 part 2 | X | No trigger met the owner's zero-false-positive bar (5.87). |
| HR-123, HR-124 | X | Calibration-gated: no corpus instance. The checker split (4.2) is their rider. |
| HR-098, HR-154 | X | Proof pending; HR-154 closes on the batch-30 branch (D8). |
| HR-121 | X | Trigger: real Gatekeeper friction. |
| HR-126, HR-149, HR-155 | X | Internal gates of this repository. HR-149 confirmed (`vacuous-exception-gate.rs:195`). |

## 4. Design (ruling-free)

These parts hold under every option of section 8.

### 4.1 `houserules capture` (HR-171)

`houserules capture --out <file> -- <command> [args...]`:

- Runs the command directly, without a shell. A pipeline or a `cd` runs through a shell the caller names: `-- sh -c '<script>'`, or `-- pwsh -Command '<script>'` on Windows; the header then records that script verbatim.
- Writes a header, one blank line, then the merged stdout and stderr in the order they arrive. Header keys, one per line: `command` (the argv, shell-quoted), `cwd` (relative to the repository root), `started` and `ended` (ISO 8601, UTC), `exit`, `head` (the full sha), `dirty` (`true` when `git status --porcelain` is not empty), `encoding` (`utf-8`, or `lossy: <n> invalid bytes replaced`).
- Exits with the command's exit code, so `&&` chains behave as without the wrapper. A command that cannot start exits 127 and writes a header with `exit: 127` and the error.
- Prints one line to stdout after the run: the report entry for this capture as JSON (`command`, `exit`, `capture`, `sha256`, `excerpt`). The implementer pastes the tool's line; the checker compares it with the file.
- The adopter audit's gaps all disappear with this tool: a working directory other than the root, an output filter such as `| tail`, and a capture in a Windows code page (adopter audit, root cause "Evidence is hand-crafted").

### 4.2 Report evidence by reference (HR-171, HR-057)

- The deliverables `run` definition becomes: `command`, `exit` (required), `capture` (workspace-relative path, required), `sha256` (required), `excerpt` (optional, at most 4,000 characters). `output` leaves the schema.
- Cap: 4,000 characters. The adopter proposed 4,000 (audit Q3). Corpus run m1 (105 task reports, 5,111 run entries): 79.5% of report bytes are inline output; a 4,000-byte cap moves 68.3% of the output bytes out; the median entry is 377 bytes, so most excerpts fit whole.
- `check-report-claims`, per run entry: the capture exists, its hash matches, and the header's `command` and `exit` equal the entry's; the excerpt is a substring of the body. A `natural` TDD entry passes when the RED header's `head` is BASE or a commit of the task's range, and its `ended` is earlier than the commit time of the first listed commit that descends from that `head`. A task with several commits runs a later RED at an intermediate commit, so BASE alone is too narrow.
- These checks replace four that parse shell strings: `check_redirected_captures`, `check_truncation_markers`, `check_paste_run_lint`, and the capture-time arm of `natural_red`. The narrative checks stay: sha resolution, self-audit narrative, ephemeral paths (widened by HR-174), citation lines (with HR-148's comma lists), number claims, sweep coverage.
- `report_claims.rs` (2,999 lines, 1,804 production) splits into submodules: one per check family and the shared resolver. This is the rider HR-123 and HR-124 ask for.

### 4.3 A check type that runs a project command (HR-173)

- New check type `command`: `{"type": "command", "level": "fail", "files": [<globs>], "run": ["<argv>", ...], "append_files": <bool>}`.
- The audit runs the command once when a changed file matches `files`. With `append_files: true`, it appends the matching files to the argv. Exit 0 passes; any other exit is the check's level; the row's evidence names the exit code and the first 20 output lines.
- The audit reads every other check's input from git at `--head`: `git show` for file text, the range's diff and log for the rest (`audit.rs` `run_check`). A command runs in the working tree. So the row is `skipped` with a named reason when the working tree's HEAD is not `--head`, or when a matching file has uncommitted changes.
- Trust: the command comes from the repository's own knowledge files, the same trust as its build scripts. The seeded workflow runs on `pull_request`, not `pull_request_target`, so a fork's PR gets no secrets (V-ci).
- The migrating-knowledge skill gets recipes: Rust, `ruff check --select D` (Python docstrings), `eslint` with the jsdoc plugin. The adopter's five doc-comment criticals are what these catch (audit Q10).
- Measured on this repository (a binary crate, no library target): `cargo clippy -- -W missing_docs` reports 0 items; `-W clippy::missing_docs_in_private_items` reports 206 (V-lint). The Rust recipe therefore names the clippy lint for a crate with no public API, and rustdoc's `missing_docs` for a library.
- A linter sees the whole crate or package, not the diff. A `command` check passes only on a tree that is already clean, so adopting one starts with the cleanup. For this repository that is HR-165 (2): 206 items. The recipe says so, and `append_files` serves only a linter that takes file arguments (ruff, eslint).

### 4.4 Report fields that tests can check (HR-175, HR-139, HR-176, HR-158)

- `report-field` gains `non_empty: true`: `null`, `""`, `[]`, and `{}` fail.
- `report-field` gains `covers_ids: true` for `knowledge_used`: with `--report`, every `--ids` id must appear.
- Corpus run m3 (105 reports): `covers_ids` would fail 1 of 104 reports that have a round-0 audit (batch 8 task 1, two ids missing). `knowledge_used` is never empty (0 of 104) and `dependency_vetting` never empty when present (0 of 18), so `non_empty` goes on `process.knowledge-first` and `security-hygiene.dependency-vetting`. It does not go on `security-hygiene.verify-current-docs`: 30 of the 87 reports that its trigger fires on hold `docs_verified: []`, and the three samples read are legitimate empties (a schema edit, Node built-ins, a knowledge port). An empty `docs_verified` stays judged; HR-175's text names it, so the spec gate confirms this narrowing.
- `report-field` gains `requires: [<regex>...]` for a list field (HR-139): each pattern must match the `command` of at least one entry.
- The row evidence reads `set in <triggered> of <read> reports` (HR-176). The branch-reviewer template compares the second number.
- Generated facts (HR-158): `audit --report <file>` writes `commits`, `files_changed`, and `self_audit` into the report from the range it audits. The implementer stops copying them. The rows cannot drift from a fresh run, which retires HR-138 (1).

### 4.5 Template and eval changes

- The implementer template uses `capture` for every command it cites and pastes the tool's JSON line. The closing act gets HR-150's second sentence and HR-165 (4, 7).
- The task-reviewer template reads captures by path and gets HR-129 (4) and HR-165 (5). The branch-reviewer template gets HR-153 (3) and HR-176. The orchestrating skill gets HR-153 (1, 2), HR-129 (3), and HR-166 (3). The finishing skill gets HR-153 (4) and HR-165 (6).
- The eval scenarios change with the report shape. `dependency-add` expects "tdd holds the verbatim RED and GREEN output" today; it becomes "each tdd run cites a capture whose header shows the RED exit".
- One eval run covers every template change (`process.evals-rerun`).

### 4.6 The upgrade path, parts every option keeps

- A migration runs inside `update`. A second `update` changes nothing.
- `update` prints one migration summary: each file it rewrote, each file it kept because the adopter changed it, and the hand step that remains for each kept file.
- Old workspaces stay readable. `stats` reads raw JSON (`stats.rs` reader), so it must read a 1.x report and a 2.0 report in one run: the five-workspace window crosses the release. Test: one fixture workspace of each shape.
- `validate` on a 1.x report and `check-report-claims` on one print one named line ("this report predates the 2.0 evidence format; finish the batch on 1.x or rewrite its run entries") instead of a list of schema errors.
- The release note and a `docs/upgrading-to-2.md` guide state the order: finish a running batch, commit, run `houserules update`, review the summary, commit the migration on its own.
- Live proof matrix: installs seeded at 1.0.0, 1.2.0, and 1.4.0; each with an unchanged kit, a modified kit entry, an overridden entry, and a modified schema; plus one unstamped install. Each is updated by the 2.0.0 binary, then runs `check-knowledge`, `check-backlog`, `validate` and `check-report-claims` on a new-format report, and `audit`. A second `update` changes nothing.
- HR-162: every seeded topic file ships in emit form, so the migration diff holds only real changes. HR-151 (2, 4): `update` on an unstamped install names `--id-prefix` and records the deletion of an overridden entry.

## 5. Design set by a ruling

### 5.1 Schema delivery (D3: baselines)

Ruled: the three schema files, the four eval files, and the seeded CI workflow get baselines like `KIT_OWNED`. The baseline is the hash of the payload after the id-prefix rewrite (`PREFIXED` in `install.rs`). At baseline, `update` rewrites the file. Modified: `update` keeps it and prints the hand step. The 2.0.0 binary's own models need the 2.0 constraints, so a kept schema then fails the gates with a line that names the file.

### 5.2 A 2.0.0 binary on a 1.x tree (D4: a named red)

Ruled: every gate checks the stamp first. With a stamp below 2.0.0, it prints one line, "this install is at kit <v>; run `houserules update` to migrate to 2.0.0", and exits 2. The seeded CI of 0.3.0 to 1.1.0 installs goes red with that line until the adopter updates. 5.88 set the precedent: a red that the release note states first.

### 5.3 The default branch and the seeded durable-sha rule (D6)

Ruled: the kit seeds `knowledge-base.cite-durable-refs` (issue 50, HR-178), which supersedes design.md 5.88. It ships with HR-131's repair: `.houserules.json` gains a `defaultBranch` key, and `durable_sha.rs` reads it before its fallbacks (`origin/HEAD`, `origin/main`, `origin/master`, local `main` or `master`). With no key and no fallback, the row is `skipped` with a line that names the key, never a `fail`. `install.rs` `new_marker` keeps keys it does not own, so a restamp keeps the key. Today a missing default branch is an error (`durable_sha.rs:245`). The seeded entry keeps this repository's check (`durable-sha`, level `fail`), with `files` reduced to the paths the kit itself names: `backlog/**`, `knowledge/**`, `docs/specs/**` (`docs/design.md` is this repository's own). The live proof (T10) runs it on an install whose default branch is `trunk`, with and without the key.

### 5.4 Hosts other than GitHub (D11)

Ruled: no change. `init` prints the skip note, and `migrating-knowledge` ("Generate your own CI gate") says what to set up in the CI pipeline and how. T7 checks that section against the 2.0 commands; `docs/upgrading-to-2.md` tells an adopter with a hand-written gate to raise the pinned version in it after `update`.

## 6. Out of scope

Every X row of section 3. Every N row. HR-042 and HR-172 unless the spec review rules them in (D10). HR-144 (batch 29) and HR-147 (batch 34), each a minor after 2.0 (D5, D7). HR-177 (dropped, D11). HR-088 and HR-053, each a minor after 2.0 (Q1).

## 7. Release

- 2.0.0 through release-please: the breaking commits carry `!` and a `BREAKING CHANGE:` footer.
- The release note starts with the migration order and the red that 5.2 describes.
- The frozen-surface entry and design.md 5.82 get the 2.0 surface: the subcommand list with `capture`, and the new blob ids of the six schema files.

## 8. Decisions for the owner

Ruled by the owner on 2026-10-10 (design.md 5.96). Each line keeps the evidence it was ruled on.

- **D1 Scope: (b).** The M items, the upgrade path, and the C items, which share their files and one eval run.
- **D2 The frozen surface after 2.0: adopted.** From 2.0.0, an addition (a subcommand, a flag, an optional field, an enum value) is a minor change, because `update` delivers the schemas (D3). Removing, renaming, and tightening stay major. T9 writes the rule into `houserules.1-0-surface-is-frozen` and design.md.
- **D3 Schema delivery: baselines** (5.1). The mechanism exists for 8 files and every kit entry, and D2 needs it.
- **D4 A 2.0.0 binary on a 1.x tree: the named red** (5.2). Installs seeded at 0.3.0 to 1.1.0 run the latest binary in CI.
- **D5 Batch 29: HR-129 and HR-153 join batch 33.** HR-144 stays in batch 29, a minor after 2.0.
- **D6 Issue 50 and HR-131: seed with the repair** (5.3). Supersedes 5.88's "not seeded". Filed as HR-178.
- **D7 HR-147, issue 41: its own batch after 2.0** (batch 34, a minor under D2).
- **D8 Batch 30 (PR #49): resolved before batch 33 starts.** The owner stores `DEPENDABOT_AUTOMERGE_TOKEN` in the Actions store first. A trial rebase of `batch-30` onto main at `v1.4.0` stops at its first commit with conflicts in `backlog/archive/kit.json`, `backlog/items/kit.json`, and `docs/design.md` (ledger V-b30-rebase).
- **D9 Capture timestamps: `jiff`.** Read on crates.io and docs.rs on 2026-10-10: 0.2.38 (2026-10-06; 0.2.36 is yanked); `Timestamp::now()`; the `Display` form is RFC 3339 in UTC with `Z`; `str::parse` reads it back; `round(Unit::Second)` gives whole seconds. T1 pins the version it vets with `cargo add`.
- **D10 Standing-rule rulings: at the spec review.** HR-042, HR-172, HR-153's clauses, HR-163's per-entry choice (a check or `ruled-keep`), and the batch-31/32 loop proposals (`.superpowers/sdd/2026-10-10-batch-32/owner-gated-proposals.md`). HR-138 (2)'s form (an `auditRow` property or the evidence text) is picked there too.
- **D11 Hosts other than GitHub: no change** (5.4). HR-177 is dropped.

- **Q1 HR-088 and HR-053: (a), both after 2.0, each as a minor.** Both are M only because they add a flag or an `init` selection; D2 makes that a minor change. HR-053 needs its own design, and HR-055 pairs with it.
