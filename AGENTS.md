# houserules — the kit repository

This repository develops the kit in `template/` and runs its own installed copy of it (entry `houserules.template-is-the-source`).

## Knowledge base

Project knowledge — rules, invariants, gotchas, procedures, decisions, history — lives in `knowledge/*.json`. Every entry has an id.

- `houserules topics` lists the topics. `houserules index --topic <t>` lists a topic's entries.
- `houserules get <id>...` prints full entries. `houserules for <path>...` prints the rules for the files you are about to change.
- `houserules standing` prints the non-negotiables.
- `houserules check-knowledge` and `houserules check-backlog` are lint gates. `houserules audit --base <ref>` checks a change against its rule package.
- Write every ruling to its home file in the same turn it is made: scope or product goes to `backlog/` (`houserules set ...` or a direct edit); process goes to `knowledge/process.json` (`standing: true` when non-negotiable); design goes to the batch spec. Then run `houserules render` and commit the generated files with the change.
- Do not add knowledge to this file. Add an entry.

## Workflow

- Run `houserules standing` at the start of a session, at every batch start or resume, and before any dispatch.
- Before you change a file, run `houserules for <path>` and read what comes back.
- The backlog (`backlog/`) drives all work. Read it with `houserules list --open`, `get <id>`, `batch <n>`; tick with `set <id> status=done batch=<n>`.
- A batch runs: brainstorm or spec, a user gate, plan, development, live run, a merge, rollout, acceptance.
- Procedure docs live at `.claude/skills/*/SKILL.md` — plain markdown any agent can read; read `.claude/skills/finishing-a-feature/SKILL.md` before a merge.
- Dispatch subagents only through the templates in `.claude/agents/`: `implementer`, `task-reviewer`, `branch-reviewer`, `spec-reviewer`. Every dispatch carries `Knowledge:` ids and `BASE:`.

## Working with other harnesses

Every instruction above holds for any agent or harness reading this file, the dispatch mandate included: no instruction here depends on subagents existing, but none of them becomes optional for a harness that has them.

One directory gives Claude Code a genuinely optional acceleration of the same rules, never a requirement:

- `.claude/rules/*.md` loads into Claude Code's context when you read a matching file: the rules of one area. The standing rules reach every harness through the houserules block of this file; Claude Code reads it through `CLAUDE.md`. Other harnesses get the rules of an area from the `AGENTS.md` of its directory or from `houserules for <path>`.

<!-- houserules:begin -->
Generated from knowledge/ by houserules render. Do not edit.

## Standing rules

- [houserules.1-0-surface-is-frozen] Past 1.0, a CLI command/flag change or a schema constraint change is a breaking change: get an owner ruling and bump the major version.
- [houserules.tag-pilot-is-read-only] Treat the tag-pilot checkout outside this repository as read-only: never modify it, commit there, run its agents, or run `init` or `update` against it.
- [houserules.template-is-the-source] Edit the kit in `template/`, then run `houserules update --dir .`; never hand-edit the root copies or the generated rules and skill.
- [process.ask-when-missing] Ask the user when information is missing. Do not assume.
- [process.claims-match-artifacts] Before submitting a report, re-open every artifact a claim cites and confirm the artifact shows what the sentence says.
- [process.closure-claims-carry-enumeration] A claim that a set is closed ('every', 'all', 'no remaining') names the bounded, rerunnable enumeration that produced it; without the artifact, no claim.
- [process.conventional-commits] Commits use Conventional Commits (feat, fix, chore, test, ci, docs, refactor). Header at most 100 characters, body lines at most 100.
- [process.every-commit-names-a-work-item] Every commit names the work item it delivers in a `Work-Item:` trailer.
- [process.evidence-outlives-the-session] Cite evidence only at paths that outlive the session: the project's deliverables folder or the tracked tree, never a session scratchpad.
- [process.fix-proofs-extend-the-reviewers-run] A fix round's proof re-runs the review's own reproduction, extended to cover the fix -- never a fresh, narrower proof.
- [process.gates-cover-generated-commits] Before a new gate or pipeline step ships, walk every commit author class and every generated resource it touches through it on paper; state each accepted red.
- [process.no-tech-debt] Fix every review finding, Minor included. Defer a fix only for a stated reason, as a work item in the tracker; never as a TODO in code.
- [process.review-findings-are-claims-too] A review finding stating a measurement, a reproduction, or a proposed fix's behavior is verified by running it before filing; restated claims are re-derived.
- [process.rulings-to-file] Every ruling goes to its home file in the same turn; one that changes what the spec states amends the spec; one inside its intent goes to the brief or report.
- [process.suggestions-are-unverified] Another agent's suggested wording, pin, or count is a draft, not evidence: re-derive every number, quote, and reference from the tree before transcribing it.
- [process.tdd] Test-driven development for every executable change: the failing test first, or a disclosed-mutation proof for already-correct behavior; verbatim RED and GREEN.
- [quality.no-compat-softening] Never soften a design for backward compatibility. Make the correct change and migrate everything it breaks; the codebase is not built on compromises.
- [quality.principles] YAGNI, KISS, DRY; prefer a well-maintained library to custom code; one responsibility per unit; tests assert behavior; a11y and i18n are part of done.
- [quality.well-maintained-libraries] Use well-maintained libraries over custom code. Research candidates at spec time; the owner rules the choice; refactors fold into specs.
- [security-hygiene.dependency-vetting] Verify every new dependency is well-maintained before adding it. Record the check in the task report's `dependency_vetting` field.
- [security-hygiene.exact-pins] Pin exact versions. Install only with a CLI (`cargo add <crate>@=<version>`, `pnpm add --save-exact`). Never write a version number by hand.
- [security-hygiene.no-coauthor] Never add a co-author, session, or tool-attribution line to any commit or pull request description.
- [security-hygiene.verify-current-docs] Verify every library, tool, and framework API against current docs before use. Internal knowledge is stale.
- [writing-style.code-comments] Write a code comment only for a current constraint the code cannot show. Never narrate history in code; git, the specs, and the knowledge base hold it.
- [writing-style.doc-comments] Document every exported symbol with the language's doc-comment convention: the current contract. Name things so the code reads without comments.
- [writing-style.principles] Write docs, comments, commit messages, and reports in ASD-STE100 style: short sentences, active voice, one instruction per sentence, concise.

## Session

- Start: run `houserules list --open`.
- End: run `houserules check-backlog`.
- Write houserules output to a file and read the file; treat a `<<` marker or a "more items" note as a lost read.

## Reference workflow

This install runs the reference workflow. Read `.claude/skills/orchestrating/SKILL.md` at the start of a session and before any handoff. If your harness can hand a task to another agent the way `.claude/agents/*.md` describes, run handoffs in parallel only when the plan marks their file sets disjoint, each in its own worktree; a harness with no subagents does the work directly, in its own session.

## Area rules

### docs

Globs: `docs/**`, `README.md`, `AGENTS.md`, `knowledge/**`, `backlog/**`, `.claude/**`
Run `houserules for <path>` before you change a file that matches.

### tools

Globs: `tools/**`, `.github/**`, `mise.toml`
Run `houserules for <path>` before you change a file that matches.

### cli

Globs: `crates/houserules/src/**`
Run `houserules for <path>` before you change a file that matches.

### template

Globs: `template/**`
Run `houserules for <path>` before you change a file that matches.

### tests

Globs: `tests/**`
Run `houserules for <path>` before you change a file that matches.

### rust

Globs: `crates/**`, `Cargo.toml`, `Cargo.lock`
Run `houserules for <path>` before you change a file that matches.
<!-- houserules:end -->
