# The reference workflow

The reference workflow is an optional set of lifecycle files for a project
that hands tasks to agents. It needs a tracker and the rules that its files
cite:

```sh
houserules init --workflow reference --tracker json --add-required-packs
```

Without a tracker, `init` refuses: `the reference workflow needs a tracker;
run houserules init --workflow reference --tracker json|github|beads`.
Without `--add-required-packs`, `init` names the packs that hold the rules
the workflow cites and adds none. `check-knowledge` fails while a rule that
the workflow requires is missing or excluded.

Four agent templates, each on a configured model, carry the rules into
every change:

- **implementer** (default model `sonnet`): implements one task from a
  brief, test-driven, and writes a JSON report.
- **task-reviewer** (default `opus`): reviews one task's diff for spec
  compliance, code quality, and rule adherence.
- **branch-reviewer** (default `opus`): reviews the whole branch before
  merge and proposes knowledge-base improvements drawn from the batch's
  reviews.
- **spec-reviewer** (default `opus`): attacks a spec before a human reads
  it and files its findings in a `spec-review` deliverable.

The model of each role sits in `workflow.models` in `.houserules.json`.
The templates carry no `model:` line. The `orchestrating` skill names the
configured model in every dispatch, and each report and review records it
in its `model` field.

Four skills drive the lifecycle. `designing` drafts a spec from what the
project already holds, has it reviewed, and takes it to the project's owner
or decider. `planning` turns the approved spec into tasks with declared
file sets. `orchestrating` dispatches the agents, runs the live run, and
drives the batch to its finish. `finishing-a-feature` merges the branch.
The risk of the change picks one of two tiers, light or standard. A task is
critical when it changes the commands, flags, or output of a command-line
interface, a schema, authentication or security code, or the data that an
install keeps on disk: its tests come first, and it gets one review. Every
dispatched agent runs `houserules audit` against its own diff and records
the result in its JSON report, so rule adherence is checked, not just
asserted.

The workflow also installs a commit-msg hook that gates Conventional
Commits, three eval scenarios with their record, and
`tools/reference-session-start.sh`, which prints the session ritual. The
block that `render` writes into `AGENTS.md` points every harness at the
`orchestrating` skill. A harness with no subagents does the work directly,
in its own session.

The workflow keeps the deliverables of a batch in a workspace, a folder of the
`workspaces` location. The default location is `.houserules/work/`, and git
ignores it through the `.houserules/` line of `.gitignore`. Set the key
`workspaces` in `.houserules.json` to another relative folder inside the
install root to keep the workspaces elsewhere. A fresh clone lacks an ignored
file, so `check-knowledge` names a `verify` path into an ignored folder: commit
a copy under a tracked path and verify that.

## The task loop

The skills and templates of the workflow carry these rules for a task:

- An implementer runs the focused test while it iterates. Before it commits, it
  runs the tests of the files that its task changes and every other gate of the
  brief, and it names the targets in its report. The person who runs the work runs
  the full suite, and every suite that the plan marks as integration-only, once
  per integration.
- A reading that the spec or the brief leaves open, and that a test would pin, is
  a question before the implementer writes that test. The implementer stops, names
  the reading and its choices, and returns NEEDS_CONTEXT. The person who runs the
  work rules it and resumes the task.
- The ledger of a batch records the start and the close of each dispatch with its
  UTC time.
- The person who runs the work asks the project's owner or decider in a plain
  message, never in a question tool that holds its turn. It keeps dispatching every
  task and fix round that needs no ruling.
- A wave orders the tasks; it is no barrier. A task starts as soon as every task it
  needs is integrated.
- A plan may name a file that two tasks share, such as a command-line entry point.
  The two tasks may run at once, and the person who runs the work resolves the
  later cherry-pick by keeping both sides. Any other resolution is a fix round for
  the later task.
- While a fix-diff review runs, the person who runs the work may integrate the fix
  and run the integration gates. A new Critical or Important finding undoes the
  integration and opens the next round.
