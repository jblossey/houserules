# Project instructions for Claude

<!-- Replace this heading and line with your project's one-line identity. -->

## Knowledge base

Project knowledge — rules, invariants, gotchas, procedures, decisions, history — lives in `knowledge/*.json`. Every entry has an id.

- `tools/kb.sh topics` lists the topics. `tools/kb.sh index --topic <t>` lists a topic's entries.
- `tools/kb.sh get <id>...` prints full entries. `tools/kb.sh for <path>...` prints the rules for the files you are about to change.
- `tools/kb.sh standing` prints the non-negotiables. They also load from `.claude/rules/standing-rules.md`; area rules load when you read matching files.
- `tools/kb.sh check` and `tools/backlog.sh check` are lint gates. `tools/kb.sh audit --base <ref>` checks a change against its rule package.
- Write every ruling to its home file in the same turn it is made: scope or product goes to `backlog/` (`tools/backlog.sh set ...` or a direct edit); process goes to `knowledge/process.json` (`standing: true` when non-negotiable); design goes to the batch spec. Then run `tools/kb.sh render` and commit the generated files with the change.
- Do not add knowledge to this file. Add an entry.

## Workflow

- Invoke the `orchestrating` skill at session start, at every batch start or resume, and before any dispatch.
- The backlog (`backlog/`) drives all work. Read it with `tools/backlog.sh list --open`, `get <id>`, `batch <n>`; tick with `set <id> status=done batch=<n>`.
- A batch runs: brainstorm or spec, user gate, plan, sequential subagent development, live run, `finishing-a-feature`, rollout, acceptance.
- Dispatch subagents only through the templates in `.claude/agents/`: `implementer`, `task-reviewer`, `branch-reviewer`. Every dispatch carries `Knowledge:` ids and `BASE:`. All agent work runs strictly sequentially.
