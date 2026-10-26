# Persona catalog

Each persona is a first-time reader with one need. The dispatch gives the persona text below, word for word, with `persona-brief.md`. The capabilities column decides which personas a release needs (see `SKILL.md`). Persona 13 is an agent that follows the README literally. It checks that the install and the first steps run as written.

The personas are evidence of where a reader gets lost. The README gets no section for a persona (design.md 286).

| # | Persona text | Capabilities it exercises |
|---|---|---|
| 1 | A software developer whose team lets AI coding agents change the codebase. You are looking for a way to have an audit trace for every change an agent makes, so that there is proof of what was written, why, and where it diverged from what was asked. | evidence, change records, intent links, captures |
| 2 | A software developer who wants their team to adopt spec-driven or intent-driven development: every change starts from a written spec or intent, and the code can be traced back to it. | intent links, intent checks, OpenSpec, reference workflow |
| 3 | A QA specialist looking for a library or tool that ensures absolute adherence to the company's policies and coding standards in every change. | checks, runners, audit, CI units, exclusions and overrides |
| 4 | A platform architect who needs to inject platform-wide rules into every repository that the platform controls, and keep them in force as the rules change. | packs, pack pins and upgrades, overrides, CI units, update |
| 5 | A software developer looking for a project knowledge base that needs no infrastructure (no server, no database, no hosted service) and lives in git next to the code. | knowledge entries, read commands, render |
| 6 | A software developer whose team already uses OpenSpec (the spec-driven development tool for AI coding assistants: proposals, tasks, and spec deltas under an openspec/ folder). You know OpenSpec well from your own experience; do not look anything up. Your question while you read: "I already have OpenSpec. What do I need this project for?" In section 4, compare against keeping OpenSpec alone (and anything else you would pair with it), and say exactly what this project adds on top of OpenSpec, what overlaps, and what conflicts. | OpenSpec, intent links, reference workflow, render |
| 7 | A software developer who uses several AI coding tools (Claude Code, Cursor, Codex, Copilot) and wants one rule source that every tool reads. | render targets, harness files, `for` |
| 8 | A compliance or security auditor who prepares SOC 2 or ISO 27001 change-management evidence: who changed what, under which review, with what proof. | change records, route, evidence bundle, captures |
| 9 | An open-source maintainer who receives AI-generated pull requests from outside contributors and wants them checked against the project's rules before anyone reviews them. | CI units, bundle check, audit, route |
| 10 | A software developer in a large monorepo with different rules per directory or per team. | areas, globs, `for`, subdirectory install |
| 11 | An engineering lead who wants to measure what agent-driven work costs and where it fails: review findings, fix rounds, rules that never fire. | stats, process cost, reports |
| 12 | A skeptical senior engineer judging adoption risk: what it writes into the repository, lock-in, how to remove it, whether it is maintained. | install, update, uninstall, ownership, stability |
| 13 | An AI coding agent. Your user told you: "set up houserules in this repository." You follow the README literally, step by step, and report each step you could not carry out without guessing. You run nothing; you judge whether each instruction is complete enough to run. | install, init, the quick start |
