# The houserules step and evidence

## The houserules step

Every install holds the `houserules-step` skill and the script
`tools/houserules-step.sh`. Any workflow, or none, calls the script around
a change. The script runs the `houserules` on your `PATH`:

```sh
tools/houserules-step.sh before src/parser.rs     # the rules for the files you will change
tools/houserules-step.sh after main report.json   # the audit since main, the report's facts, and the report check
```

`before` runs `houserules for` and writes the rule package to
`.houserules/step/rules.json`. `after <base>` runs `houserules audit --base
<base>` and writes the result to `.houserules/step/audit.json`. With a
report as the second argument, the audit runs with `--report`, and the script
then runs `houserules report-facts` and `houserules validate` and writes
`.houserules/step/report-facts.txt` and `.houserules/step/validate.json`. The
script prints the path of each output file, and `after` prints each exit
status, so an agent reads the output with its file-read tool. Paths and refs
are relative to the install root. `.houserules/` holds the scratch output of
the step and, by default, the batch workspaces, and `init` adds it to
`.gitignore`.

## Evidence

A task report cites its evidence instead of holding it. `houserules capture`
runs a command and writes its output to a file under a header. The report names
the file and its hash. `houserules report-facts` fills the facts of the report
from git and from those files, and `houserules check-report-claims` checks the
report against them.

The commands below start from a fresh install: a repository where `houserules
init` ran and one commit holds its files. The first block adds a file and a
commit, then captures a command:

```sh
mkdir -p src .houserules/work/demo
echo 'fn parse_header() {}' > src/parser.rs
git add src && git commit -q -m 'feat: add the parser'
houserules capture --out .houserules/work/demo/files.txt -- ls src
```

```text
.houserules/work/demo/files.txt: exit 0
```

`capture` runs the command with no shell, in the current directory, with stdin
closed. It exits with the exit code of the command, so a failing test run still
fails a gate chain. The directory of `--out` must exist. Line 1 of the file is
the header, one JSON object: the command, the directory, the start and end
times, the exit status, the signal, `HEAD`, a dirty flag, and the count of byte
sequences that were not UTF-8. The output of the command follows. A command with
a variable prefix runs through `env` (`-- env CI=1 cargo test`), and a pipeline
runs through `sh -c`.

A report cites each run by the path of its capture, and each line that a claim
rests on by a citation. The next block writes a fragment of a report (a task
report holds the other fields of its schema too), fills its facts, and checks
it:

```sh
cat > report.json <<'EOF'
{
  "kind": "task-report",
  "tests": [
    { "capture": ".houserules/work/demo/files.txt", "excerpt": "parser.rs" }
  ],
  "citations": [
    { "path": "src/parser.rs", "line": 1, "anchor": "fn parse_header" }
  ]
}
EOF
houserules report-facts report.json --base HEAD~1
houserules check-report-claims report.json
```

```text
report.json: facts filled
report.json: no claim mismatches found
```

- `report-facts` adds the `sha256`, the `command`, and the `exit` of each run
  from its capture, the `sha256` of each citation from the file at `HEAD`, the
  `commits` of `<base>..HEAD`, and the `files_changed`. With `--audit <file>`,
  where the file is the result of `audit --json`, it fills `self_audit`. It
  never replaces a recorded hash: a capture or a cited line that differs from
  its hash is a finding.
- An `excerpt` is optional, holds at most 1000 UTF-16 code units, and must be a
  byte-exact part of the output of its capture.
- A citation names a file that git tracks at `HEAD`, the cited lines, and an
  anchor: text of those lines that occurs once in the file.
- `check-report-claims` fails a report when a capture is missing or changed,
  when a `command` or an `exit` differs from its header, when an `excerpt` is
  not part of the output of its capture, and when a cited line differs from
  `HEAD`.

A capture that changes after `report-facts` recorded its hash is a finding. A
word in angle brackets stands for a value of your run:

```sh
echo changed >> .houserules/work/demo/files.txt
houserules check-report-claims report.json
```

```text
tests[0]: capture .houserules/work/demo/files.txt has sha256 <hash of the file>, but the report records <hash in the report>; re-read the capture, then remove sha256 and run report-facts
```

`tools/houserules-step.sh after <base> <report>` runs the audit with `--report`,
then `report-facts`, then `validate`. The reference workflow teaches the
implementer to run every test, gate, and live run through `capture`.
