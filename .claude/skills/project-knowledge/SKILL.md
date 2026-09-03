---
name: project-knowledge
description: Use when working on this repository as a dispatched subagent, before reading or changing any file
user-invocable: false
---
Generated from knowledge/ by tools/kb.sh render. Do not edit.

# Project knowledge

## Standing rules

- [houserules.pnpm-only] Use pnpm for every package operation in this repository (`pnpm install`, `pnpm add --save-exact`, `pnpm test`, `pnpm dlx`); never `npm` or `npx`.
- [houserules.tag-pilot-is-read-only] Treat `~/projects/tag-pilot` as a read-only reference: never modify it, commit there, run its agents, or run `init` or `update` against it.
- [houserules.template-is-the-source] Edit the kit in `template/`, then run `node bin/houserules.mjs update --dir .`; never hand-edit the root copies or the generated rules and skill.
- [process.ask-when-missing] Ask the user when information is missing. Do not assume.
- [process.backlog-drives-work] The backlog drives all work. Select backlog items before you start; every requirement traces to an item.
- [process.brainstorm-first] Start each batch with a brainstorming session or a written spec; get the user's approval before implementing.
- [process.code-health-scan] Every plan carries a code-health scan of the files the batch touches: name the smells and antipatterns found, and fold targeted fixes into the tasks.
- [process.conventional-commits] Commits use Conventional Commits (feat, fix, chore, test, ci, docs, refactor). Header at most 100 characters, body lines at most 100.
- [process.deliverables-json] Task reports, reviews, and branch reviews are JSON files that pass `tools/kb.sh validate` against `.claude/schemas/deliverables.json`.
- [process.ff-only-merges] Merge fast-forward only, from the CLI, after aggregating the branch into clean logical commits. No merge commits, no GitHub squash merges.
- [process.knowledge-first] Before you change a file, read its knowledge: the ids in your task and `tools/kb.sh for <path>`. Cite the ids you relied on in your report.
- [process.live-run-before-ci] Verify a change live (run the app, service, or tool for real; capture evidence) before any PR, merge, or deploy spend.
- [process.model-policy] Every review runs on a mightier model than the implementer it reviews. Implementers use the cheapest model that fits the task.
- [process.no-tech-debt] Fix every review finding, Minor included. Defer a fix only for a stated reason, as a backlog item; never as a TODO in code.
- [process.rulings-to-file] Every ruling goes to its home file in the same turn — a deferral’s backlog item included. A ledger and the chat are not home files; neither survives compaction.
- [process.sequential-agents] Run all agent work strictly sequentially. Never dispatch two implementers or two reviewers at the same time.
- [process.tdd] Test-driven development for every executable change: the failing test first, or a disclosed-mutation proof for already-correct behavior; verbatim RED and GREEN.
- [quality.no-compat-softening] Never soften a design for backward compatibility. Make the correct change and migrate everything it breaks; the codebase is not built on compromises.
- [quality.principles] YAGNI, KISS, DRY; prefer a well-maintained library to custom code; one responsibility per unit; tests assert behavior; a11y and i18n are part of done.
- [security-hygiene.dependency-vetting] Verify every new dependency is well-maintained before adding it. Record the check in the task report's `dependency_vetting` field.
- [security-hygiene.exact-pins] Pin exact versions. Install only with a CLI (`pnpm add --save-exact`, `cargo add <crate>@=<version>`). Never write a version number by hand.
- [security-hygiene.no-coauthor] Never add a co-author or session trailer line to any commit.
- [security-hygiene.no-focused-tests] Never commit a focused test (`.only(`).
- [security-hygiene.verify-current-docs] Verify every library, tool, and framework API against current docs before use. Internal knowledge is stale.
- [writing-style.code-comments] Write a code comment only for a constraint the code cannot show.
- [writing-style.doc-comments] Document every exported symbol with the language's doc-comment convention. Name things so the code reads without comments.
- [writing-style.principles] Write docs, comments, commit messages, and reports in ASD-STE100 style: short sentences, active voice, one instruction per sentence, concise.

## Retrieval protocol

1. Resolve every id under `Knowledge:` in your task: `tools/kb.sh get <ids>` (JSON).
2. Before editing, run `tools/kb.sh for <every file you will change>` and `get` any rule you are unsure about.
3. Write `REPORT_FILE` as a `task-report` (schema `.claude/schemas/deliverables.json`, `self_audit: null`), run `tools/kb.sh validate <REPORT_FILE>`, then `tools/kb.sh audit --base <BASE> --head HEAD --ids <ids, comma-separated> --report <REPORT_FILE>`. The `--ids` value is the task's `Knowledge:` list, generated from it, never typed separately. Copy the audit `summary` and its `deterministic` rows into `self_audit` — never hand-written rows; the judged rows are the reviewer's. Fix every `fail`, re-run until clean, validate again. List the ids you relied on in `knowledge_used`.

## Topics

houserules  10  Working in the houserules kit repository
knowledge-base  4  Authoring knowledge entries
process  21  How work runs: batches, dispatch, reviews, rulings
quality  3  Quality principles
security-hygiene  5  Dependency, commit, and test hygiene
writing-style  3  Writing style for docs, comments, commits, reports
