# CLI reference

Working with knowledge:

| Command | What it does |
|---|---|
| `topics` | Lists every knowledge topic with its entry count and title |
| `index` | Lists knowledge-entry index rows, optionally filtered |
| `get <id>` | Prints items by id: a knowledge entry, or a work item of the installed tracker |
| `for <path>` | Prints the rule package a set of changed paths pulls in |
| `standing` | Lists the standing rules |
| `render` | Rewrites stale generated harness files (`--check` lists them instead) |
| `pack <list\|add\|upgrade\|remove>` | Lists, adds, upgrades, and removes the knowledge packs of an install |

Working with work items:

| Command | What it does |
|---|---|
| `list` | Lists the work items of the installed tracker, optionally filtered; `--deferred` lists the deferrals with their reasons |
| `batch <n>` | Prints one development batch's summary and item rows |
| `set <id> k=v` | Applies field assignments (`status`, `batch`, `milestone`) to a work item |
| `defer` | Files a deferral: `--title` files a new item, and `--item <id>` marks an existing one; both name a reason and an origin |
| `archive` | Sweeps done/dropped items and done batches of the JSON tracker, and retired or superseded entries of your topic files, into `archive/` mirrors; a pack entry stays in its pack copy |

Gates:

| Command | What it does |
|---|---|
| `check-knowledge` | Validates the knowledge base, its packs against their pins, its overrides and exclusions, the fields of each check, the `runners` and `workspaces` keys of `.houserules.json` and the tool of each `runner` check, generated-file freshness, and that kit-owned files cite only entries it holds; it names a `verify` path that git ignores |
| `check-backlog` | Validates the work items of the installed tracker |
| `check-commit` | Runs commit-message checks against a message file or git range |
| `audit` | Builds a git range's rule package and runs every deterministic check, the tool of each `runner` check included, and resolves each `Work-Item:` and `Deferred:` trailer of the range through the installed tracker; a task audit (`--report`) leaves out an entry tagged `controller` unless `--ids` names it, and compares the `self_audit` of the report with its own run |
| `validate` | Validates deliverable JSON files against the schema, which requires a `model` field; a finished task report fails when its self-audit, or the audit capture of a fix round, skipped checks |
| `check-report-claims` | Cross-checks a report's claims against the captures, files, and commits it cites; with `--doc <path>` or `--item <id>` it checks a spec or a backlog item for cited workspace paths that do not exist, and skips a cited path under `.houserules/step/` |
| `stats <workspace>...` | Aggregates rule violations, sanctioned fails, process cost, and knowledge-base proposals across one or more workspaces; `--window <n>` reads the `n` most recent |

Working with evidence:

| Command | What it does |
|---|---|
| `capture --out <file> -- <command>...` | Runs a command and writes its output to the file under a header (the command, the directory, the times, the exit status, `HEAD`, and a dirty flag); it exits with the exit code of the command |
| `report-facts <report> --base <ref>` | Fills the fact fields of a task report from git and from captures: the commits, the changed files, the `sha256`, `command`, and `exit` of each run, the `sha256` of each recorded citation, and, with `--audit <file>`, the self-audit |

Reviewing a change:

| Command | What it does |
|---|---|
| `evidence` | Writes the reviewer packet of a range as Markdown, and with `--json` as a document: the uncovered surface first, then the intent, the failed, warned, proven, and judged rules, the diff size, and the route; `--validation <file>` adds the result of an `openspec validate` capture to the intent of an OpenSpec change |
| `route` | Decides the route of a range, `auto-merge-eligible`, `owner-review`, or `senior-review`; `--no-audit` computes the floor, which runs no check |
| `bundle <write\|check>` | `write` writes the evidence bundle section for the description of a pull or merge request from captures; `check` fails a description with a missing part, a stale run, or a diff over its budget |
| `record <write\|show\|list\|audit>` | Keeps a change record for each merge as a git note: `write` computes it, `show` prints it and checks its rule hashes against git, `list` lists the records, and `audit` adds a post-merge verdict; `stats --records <range>` reports the review coverage of the records in a range |
| `ci <print\|write>` | Prints the lines that wire the review layer into GitHub or GitLab, or creates a file of its own for them; neither edits a CI file that it did not create |

Installing:

| Command | What it does |
|---|---|
| `init` | Seeds the minimal default into a git repository from the embedded payload; `--tracker json\|github\|beads` (with `--repo` for GitHub) installs a tracker, `--workflow reference` installs the reference workflow (`--add-required-packs` adds the packs it needs), `--target claude` adds the Claude Code files, and `--dir <subdir>` installs in a subdirectory; it adds `.houserules/` to `.gitignore`; it writes no CI file |
| `update` | Syncs kit-owned files, moves the kit packs and the workflow files, converts an install of houserules 1.x, removes retired files, brings `.gitignore` to hold `.houserules/`, reports version drift; it refuses a directory that holds no install |
| `files` | Prints the file classes of the kit as JSON: kit-owned, seed-once, pack copies, kept current through a baseline, tracker files, workflow files, and workflow data |

Run any command with `--help` for its flags.
