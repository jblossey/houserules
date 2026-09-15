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

- Run `houserules standing` and read `.claude/skills/orchestrating/SKILL.md` at the start of a session, at every batch start or resume, and before any dispatch.
- Before you change a file, run `houserules for <path>` and read what comes back.
- The backlog (`backlog/`) drives all work. Read it with `houserules list --open`, `get <id>`, `batch <n>`; tick with `set <id> status=done batch=<n>`.
- A batch runs: brainstorm or spec, a user gate, plan, development, live run, a merge, rollout, acceptance.
- Procedure docs live at `.claude/skills/*/SKILL.md` — plain markdown any agent can read; read `.claude/skills/finishing-a-feature/SKILL.md` before a merge.
- Dispatch subagents only through the templates in `.claude/agents/`: `implementer`, `task-reviewer`, `branch-reviewer`. Every dispatch carries `Knowledge:` ids and `BASE:`. All agent work in this repository runs strictly sequentially (`process.sequential-agents`): never start a second one before the last one closes.

## Working with other harnesses

Every instruction above holds for any agent or harness reading this file, the dispatch mandate included: a harness that can hand a task to another agent the way `.claude/agents/*.md` describes follows that mandate exactly; one with no subagent concept does the work directly, in its own session, instead — no instruction here depends on subagents existing, but none of them becomes optional for a harness that has them.

One directory gives Claude Code a genuinely optional acceleration of the same rules, never a requirement:

- `.claude/rules/*.md` loads automatically into Claude Code's context: standing rules every session, area rules when you read a matching file. Other harnesses get the identical rules by reading this file and running the `houserules` commands above.
