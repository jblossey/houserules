# The review layer

A reviewer needs three facts about a change: what it did, what no check read,
and who must look at it. The review layer writes them down. Each pull or merge
request gets one report, and each merge gets one record. houserules computes
both from git, the rules of the install, and the files that your CI hands it.
No command talks to the host beyond the tracker that the install names: the GitHub tracker runs gh, as audit does.

The commands below start from a fresh install: a repository where `houserules
init` ran and one commit holds its files. The block adds a file and a commit,
then asks for the route of the change:

```sh
mkdir -p src
echo 'fn parse_header() {}' > src/parser.rs
git add src && git commit -q -m 'feat: add the parser'
houserules route --base HEAD~1
```

```text
route: owner-review
no area: `src/parser.rs` -> owner-review
```

An install with no `review` setting gets `owner-review` for each change, and
`senior-review` for a change to a rule source. The `review` key sets the rest.

| Command | What it writes |
|---|---|
| `houserules evidence --base <ref>` | The reviewer packet, with the uncovered surface first |
| `houserules route --base <ref>` | The route of the change |
| `houserules bundle write` and `bundle check` | The evidence bundle for the description of the request, and the check of it |
| `houserules record write` and `record show` | The change record of a merge, kept as a git note |
| `houserules ci print` and `ci write` | The lines or the file that wire all of this into GitHub or GitLab |

`init` writes no CI file. To run the gates and the review layer in your
pipeline, run `houserules ci print --host github` or `--host gitlab` (see
[CI](../README.md#ci)). The output calls the unit that the release of your binary
publishes: a reusable workflow for GitHub and a file for `include: remote` for
GitLab. A word in angle brackets stands for a value of your release:

```sh
houserules ci print --host github
```

```text
# Resolve <release commit sha> with: git ls-remote https://github.com/jblossey/houserules refs/tags/v<version>
name: houserules review
on:
  pull_request:
    types: [opened, synchronize, reopened, edited]
  push:
    branches: ["main"]
jobs:
  review:
    permissions:
      contents: write
      pull-requests: write
      statuses: write
      issues: write
    uses: jblossey/houserules/.github/workflows/houserules-review.yml@<release commit sha> # v<version>
    with:
      version: <version>
      dir: "."
      key: "root"
ci print: add the file that holds these lines to review.ci.files in .houserules.json, so that a change to it routes to senior-review
```

The `permissions` of the `review` job hold the write scopes of the unit. Inside
the unit, only the `report` and `record-push` jobs hold a write scope, and the
`gates` job reads the code with a read-only token. `--mode inline` prints the jobs themselves,
and `houserules ci write` creates a file of its own. The `migrating-knowledge`
skill and [docs/upgrading-to-2.md](upgrading-to-2.md) give the three ways, the
GitLab token, and the limits of the layer.

The keys and ids that the layer adds:

| Name | What it is |
|---|---|
| `review` | The key of `.houserules.json` that configures the layer: `diffExclude`, `route`, `bundle`, `intent`, `sessions`, `ci`, and `record`. `check-knowledge` fails a fault in it with one line that names the key |
| `review.route` | The route of an area or a path, thresholds for an external score, and `sampleRate`. An entry with `auto-merge-eligible` names `maxDiffLines` and `maxUncoveredShare` |
| `review.bundle` | `budgets` (the diff budget of each risk tier), `exemptAuthors`, and `exemptBranches` |
| `review.intent` | `sources` (`work-item`, `file`, or `openspec`) and `files` (globs of the files that the `file` source reads) |
| `review.sessions.trailers` | The trailer keys whose value refers to a session of an agent tool: an array of keys of letters, digits, and `-`. The default is `["Entire-Checkpoint"]`, and the packet prints each such trailer of the range in its `Sessions:` line |
| `review.ci.files` | The CI files of your own that call houserules; a change to one routes to `senior-review` |
| `review.record.maxBytes` | The size budget of a change record, at least 8,192 |
| `covers` | A field of the `check` of a knowledge entry. `covers: true` makes the check count as coverage of the files it reads. No entry of the kit sets it |
| `houserules/pack@1.3.0` | The schema id of a pack copy whose check sets `covers` |
| `houserules/overrides@1.2.0` | The schema id of a `knowledge/overrides.json` whose override sets `covers` |
| `intent-required`, `intent-resolves` | Two check types of the knowledge schema that judge the intent links of a change. No pack of the kit uses either one (see [Two checks of intent](openspec.md#two-checks-of-intent)) |
| `houserules/pack@1.4.0` | The schema id of a pack copy whose check has one of the two types or sets `sources` |
| `houserules/overrides@1.3.0` | The schema id of a `knowledge/overrides.json` whose override sets one of them |
