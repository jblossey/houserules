# Project instructions

<!-- Replace this heading and line with your project's one-line identity. -->

## Knowledge base

Project knowledge — rules, invariants, gotchas, procedures, decisions, history — lives in `knowledge/*.json`. Every entry has an id.

- `houserules topics` lists the topics. `houserules index --topic <t>` lists a topic's entries.
- `houserules get <id>...` prints full entries. `houserules for <path>...` prints the rules for the files you are about to change.
- `houserules standing` prints the non-negotiables.
- `houserules check-knowledge` is the lint gate of the knowledge base. `houserules audit --base <ref>` checks a change against its rule package.
- Write every ruling to its home file in the same turn it is made: scope or product goes to a work item in the project's tracker, or to `docs/` when the project has none; process goes to a knowledge entry (`standing: true` when non-negotiable); design goes to the spec. Then run `houserules render` and commit the generated files with the change.
- Do not add knowledge to this file. Add an entry.

## Workflow

- Run `houserules standing` at the start of a session.
- Before you change a file, run `houserules for <path>` and read what comes back.
- Work items live in the project's tracker, once the project has one: `houserules init --tracker json|github|beads` installs it, and the houserules block below then names the commands that open and close a session on it.
- Procedure docs live at `.claude/skills/*/SKILL.md` — plain markdown any agent can read.
- `houserules init --workflow reference` installs the reference workflow, a set of lifecycle skills and agents for a project that wants them. Its install adds a `## Reference workflow` section to the houserules block below.

## Working with other harnesses

Every instruction above holds for any agent or harness reading this file. Two directories give Claude Code an extra, optional acceleration of the same rules — treat them as convenience, never as a requirement:

- `.claude/rules/*.md` loads into Claude Code's context when you read a matching file: the rules of one area. `houserules init --target claude` writes these files. The standing rules reach every harness through the houserules block of this file; Claude Code reads it through `CLAUDE.md`. Other harnesses get the rules of an area from the `AGENTS.md` of its directory or from `houserules for <path>`.
- `.claude/agents/*.md` are Claude Code subagent templates for handing a task to a fresh context; `houserules init --workflow reference` installs them. No instruction here depends on subagents existing.
