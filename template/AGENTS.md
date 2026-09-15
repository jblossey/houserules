# Project instructions

<!-- Replace this heading and line with your project's one-line identity. -->

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
- A batch runs: brainstorm or spec, a decision gate from the project's owner or decider, plan, development, live run, a merge, rollout, acceptance.
- Procedure docs live at `.claude/skills/*/SKILL.md` — plain markdown any agent can read; read `.claude/skills/finishing-a-feature/SKILL.md` before a merge.
- If your harness can hand a task to another agent, do that sequentially by default — never start a second one before the last one closes — until the project rules its own parallelism discipline.

## Working with other harnesses

Every instruction above holds for any agent or harness reading this file. Two directories give Claude Code an extra, optional acceleration of the same rules — treat them as convenience, never as a requirement:

- `.claude/rules/*.md` loads automatically into Claude Code's context: standing rules every session, area rules when you read a matching file. Other harnesses get the identical rules by reading this file and running the `houserules` commands above.
- `.claude/agents/*.md` are Claude Code subagent templates (`implementer`, `task-reviewer`, `branch-reviewer`) for handing a task to a fresh context; every such handoff carries `Knowledge:` ids and `BASE:`, and reads `.claude/skills/orchestrating/SKILL.md` again first. A harness with no subagent concept does the same work directly, in its own session, instead — no instruction here depends on subagents existing.
