# Runners

A runner check runs an external tool over a change and fails the audit on what
the tool reports. Your project defines the tool; the kit ships none.

Define the tool in the `runners` key of `.houserules.json`:

```json
{
  "runners": {
    "shellcheck": {
      "command": ["shellcheck"],
      "parser": "exit-code",
      "install": "install shellcheck from https://www.shellcheck.net"
    }
  }
}
```

`command` is the program and its arguments. No shell starts, and a program
with a path separator resolves against the install root. `parser` is
`exit-code` or `sarif`. `install` is one line that tells a person how to
install the tool. `init` and `update` keep the values of the key.

Name the tool in the `check` of a knowledge entry:

```json
{ "type": "runner", "level": "fail", "tool": "shellcheck", "files": ["**/*.sh"], "scope": "changed" }
```

- `tool` is a key of `runners`.
- `files` lists the globs that trigger the check; the default is every file.
  A check with no matching changed file passes with `not triggered` and starts
  no process.
- `scope: "changed"` (the default) appends the matching changed files to the
  command, one argument each and at most 200 for each call. `scope: "tree"`
  appends nothing and makes one call; the tool reads the install root.
- The tool runs with the install root as its working directory and the
  environment of `audit`. No timeout applies.

The row of the entry shows the result:

- An `exit-code` tool passes when every call exits 0 (`exited 0 in 1 call`).
  Otherwise the row fails with the evidence of the failing call: the first
  non-blank line of its stderr, trimmed; `exited <n>` when stderr holds no text;
  or `ended by a signal` when a signal ended the tool.
- A `sarif` tool prints a SARIF 2.1.0 document on stdout, and its exit status
  does not matter then. Each result is a finding at the level that SARIF gives
  it: `error` and `warning` count, `note` and `none` do not. The row fails with
  the first finding as `<path>:<line> <rule>: <message>` and `and <n> more`,
  and it holds a `findings` array. A clean run passes with
  `no findings in 1 call`. Semgrep (`semgrep scan --sarif`) and ast-grep
  (`ast-grep scan --format sarif`) print SARIF.

The tool reads the working tree, so a triggered runner check needs the tree at
`--head` checked out: `--head` is the checked-out commit, and no file that the
tool reads differs from its content there. The jobs that `houserules ci`
wires check out the head of the pull or merge request for this reason, and so
does a workflow that a 1.x release seeded. The jobs install no tool: pass
the install step of your tools to the `setup` input of the unit, or add it to
your own CI.

`audit` stops with exit 2, one line on stderr, and no rows when a triggered
check cannot run:

```text
runner tool "shellcheck" is not defined in .houserules.json runners
runner tool "shellcheck" (shellcheck) is not installed: install shellcheck from https://www.shellcheck.net
runner tool "shellcheck" needs the tree at --head checked out: HEAD is 7d1e99b, --head is cfef028
```

The same exit covers a malformed `runners` entry, a program that cannot start,
and SARIF output that is missing, invalid, or reports a failed run.
`check-knowledge` names a `runner` check whose tool `runners` does not define,
and each malformed entry of `runners`. The `migrating-knowledge` skill covers
how to run such a tool without network access.
