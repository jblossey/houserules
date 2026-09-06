# Project instructions for Claude

<!-- Replace this heading and line with your project's one-line identity. -->

## Knowledge base

Project knowledge — rules, invariants, gotchas, procedures, decisions, history — lives in `knowledge/*.json`. Every entry has an id.

- `houserules topics` lists the topics. `houserules index --topic <t>` lists a topic's entries.
- `houserules get <id>...` prints full entries. `houserules for <path>...` prints the rules for the files you are about to change.
- `houserules standing` prints the non-negotiables. They also load from `.claude/rules/standing-rules.md`; area rules load when you read matching files.
- `houserules check-knowledge` and `houserules check-backlog` are lint gates. `houserules audit --base <ref>` checks a change against its rule package.
- Write every ruling to its home file in the same turn it is made: scope or product goes to `backlog/` (`houserules set ...` or a direct edit); process goes to `knowledge/process.json` (`standing: true` when non-negotiable); design goes to the batch spec. Then run `houserules render` and commit the generated files with the change.
- Do not add knowledge to this file. Add an entry.

## Workflow

- Invoke the `orchestrating` skill at session start, at every batch start or resume, and before any dispatch.
- The backlog (`backlog/`) drives all work. Read it with `houserules list --open`, `get <id>`, `batch <n>`; tick with `set <id> status=done batch=<n>`.
- A batch runs: brainstorm or spec, user gate, plan, sequential subagent development, live run, `finishing-a-feature`, rollout, acceptance.
- Dispatch subagents only through the templates in `.claude/agents/`: `implementer`, `task-reviewer`, `branch-reviewer`. Every dispatch carries `Knowledge:` ids and `BASE:`. All agent work runs strictly sequentially.
