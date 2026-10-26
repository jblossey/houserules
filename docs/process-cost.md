# Measure process cost

`houserules stats` reads the deliverables that agents write into a batch
workspace and prints one JSON document. Give it one or more workspace
directories:

```sh
houserules stats .houserules/work/*/
```

`--window <n>` reads only the `n` most recent workspaces. A workspace is more
recent by the `YYYY-MM-DD` prefix of its folder name, then by name; a folder
with no date prefix is the oldest. `n` above the number of workspaces reads all
of them, and `n` must be 1 or more:

```sh
houserules stats --window 5 .houserules/work/*/
```

With several workspaces, a task label reads `<workspace>/<task>`; a window of
one workspace drops the prefix, as a single named workspace does. The
output keeps the keys `violations`, `unused_ids`, `audits`, and
`reviews`. It adds these keys:

- `workspaces`: one row per workspace with its tasks, reviews, fix
  rounds, report bytes, findings by severity and by target, and `reversals`.
  `reversals` counts the `Reversal:` lines of the `progress.md` of the
  workspace: a line counts when it starts with `Reversal:`, after optional
  spaces and an optional list marker with its space (`- `, `* `, or `+ `).
- `sanctioned`: one row for each fail that a sanction accepts, with the rule
  `id`, the `workspace`, the `task`, and the `ref` of the sanction.
- `models`: one row per model that wrote a deliverable, sorted by row
  name. A `model` value that names one family (`fable`, `haiku`, `opus`,
  or `sonnet`) as a whole word counts in the row of that family: the
  alias `opus` and the id `claude-opus-5-5` share the row `opus`. A value
  that names no family keeps its own row, as `human` and `unknown` do. A
  row has `model` in place of `path` and the other fields of a
  `workspaces` row, except `reversals`. A deliverable with no `model` field
  counts in the row `pre-2.0`.
- `cost`: the sums of those rows and the mean per task.
- `rules`: one row per active knowledge entry. `source` names where the
  entry comes from: `local`, or the name of the pack that ships it.
  `workspaces` counts the workspaces whose audit or review rated it, and
  `injected` the workspaces whose audit injected it. `rows` counts the
  deterministic audit rows and the judged review rows that rated it, and
  `fails` those that failed. `findings` and `targets` count the findings
  that cite it, by severity and by target. `cited` counts the reports
  that cited it. `verdicts` counts the judged rows with a `pass` or `fail`
  result, `overrides` those that a person overruled, and `override_rate` is
  `overrides` over `verdicts` (`null` at zero verdicts). `cost` is the sum,
  over the reviews that judged the rule, of each review's share of its bytes
  (`null` for a rule with no judged rows). `sanctioned` counts the fails that a
  sanction accepts, which `violations` and `fails` leave out.
- `packs`: one row for `local` and one for each installed pack, with
  the sums of its source. `entries` counts its active entries and
  `standing` the standing ones. `judged_rows` counts the review rows that
  rated its judged entries. `violations` counts the tasks that
  `violations` lists for its entries.
- `proposals`: the changes the numbers support. Each row names an
  `action`, the entry `id`, the `reason`, and the `evidence`. The five
  actions are `demote`, `retire`, `mechanize`, `narrow`, and `budget`. A
  row with `"owner_gate": true` is for the project's owner to decide.
  `narrow` counts a finding as soft only when it is minor or hits a
  deliverable, so a finding on Markdown, `docs/`, `knowledge/`, or
  `backlog/` text counts as a contract finding. Tag an entry
  `ruled-keep` when you decide to keep it, and `stats` stops proposing
  `demote`, `retire`, `narrow`, or `mechanize` for it; the entry still
  counts toward the standing budget. `mechanize` also needs an
  `override_rate` of at most 0.2, or `null`; its `evidence` adds
  `override_rate` and `samples`, up to three `issues` of task reviews and
  branch reviews that name the rule. The `evidence` of `retire` adds `cost`.

One `models` row, for a workspace whose only deliverable is a task review
that `claude-opus-5-5` wrote, with one important finding on code and one
minor finding on prose. The row is named after the family, `opus`:

```json
{
  "model": "opus",
  "tasks": 0,
  "reviews": 1,
  "re_reviews": 0,
  "fix_rounds": 0,
  "report_bytes": 0,
  "findings": {
    "critical": 0,
    "important": 1,
    "minor": 1,
    "other": 0
  },
  "targets": {
    "code": 1,
    "deliverable": 0,
    "prose": 1,
    "other": 0
  }
}
```

One `proposals` row, for a workspace whose only deliverable is a task review
with four minor findings on prose that cite `process.claims-match-artifacts`:

```json
{
  "action": "narrow",
  "id": "process.claims-match-artifacts",
  "owner_gate": true,
  "reason": "4 of 4 findings are minor or hit a deliverable. Narrow the rule to contract surfaces.",
  "evidence": {
    "findings": 4,
    "soft_findings": 4
  }
}
```
