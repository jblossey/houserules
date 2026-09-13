# Batch 24 spec: the repo-and-setup batch

Status: approved by the owner as amended, 2026-09-12
(design.md 5.66 selection, 5.67 rulings):
the fine-grained-PAT trigger; version 0.3.0 with the -alpha
suffix dropped (the latest alias requires non-prerelease and
0.x already signals pre-1.0); the 0.2.0-alpha remains stay as
history; and the owner's added requirement - THE LATEST
STRUCTURE (§2a).
Items: HR-108 (the umbrella), HR-048, HR-068, HR-081, HR-082,
HR-049, HR-063, HR-064, HR-069, HR-070 (selected 2026-09-12,
design.md 5.66).
NOT in scope: HR-100/101/102/104/107/109/110/111/112 (code and
process items with no release surface); HR-072, HR-088; the
parked owner items.

## 1. What is actually broken (measured 2026-09-12)

- One release exists: `houserules: v0.2.0-alpha` (prerelease,
  tag `houserules-v0.2.0-alpha`, created 2026-09-03 by
  release-please) with ZERO uploaded assets — the tag was
  pushed with the default GITHUB_TOKEN, so release.yml (dist)
  never ran (`houserules.default-token-tags-start-no-workflows`,
  HR-068's subject).
- The README's direct-download table and shell-installer URL
  point at tag `v0.2.0-alpha` — the WRONG tag name (the release
  tag carries the `houserules-` component prefix), so every
  documented URL 404s twice over: no assets, wrong tag. A stray
  duplicate tag `v0.2.0-alpha` (pre-rust-config) also exists.
- All three documented install channels fail today: the shell
  installer (404), mise `ubi:` (no asset to resolve, and the
  backend is deprecated upstream — HR-070), direct download
  (404).
- Release PR #18 (`chore(main): release 1.0.0-alpha`) is open
  and its `plan` check sits at `action_required` — workflow
  runs on the release-please branch await manual approval, so
  the PR cannot even go green unattended.
- release-please attributes only `crates/**` commits while
  `template/**` ships inside the binary (HR-081): a
  template-only batch would cut no release although the
  shipped artifact changed.
- Cargo.lock version residue (HR-082) rides any version-bump
  commit.

## 2. Goal

One complete, provable release cycle, then the docs that match
it: the release PR merges green unattended; the tag triggers
dist; the release page carries the source archives, all five
platform binaries with checksums, and the installer script;
all three documented install channels verified live from a
clean environment; the runbook's release section rewritten to
the proven reality.

## 2a. The latest structure (owner-required at the gate)

Every documented install path pins to GitHub's
`releases/latest/download/<asset>` alias instead of a
versioned URL, so a release never forces a README edit:

- The shell installer:
  `.../releases/latest/download/houserules-installer.sh`.
- The direct-download table: one `latest/download` URL per
  platform archive (checksums via the same alias +
  `.sha256`).
- mise's `github:jblossey/houserules` backend resolves latest
  natively (HR-070's migration aligns).
- The alias resolves only to non-prerelease releases — the
  0.3.0 ruling (no -alpha) makes every release a full release,
  so the alias always points at the newest one.
- HR-049 ("the pinned install tag follows each release")
  closes by design change: the x-release-please URL blocks in
  install paths retire; nothing version-bearing remains to
  follow. An exact-version download stays possible through the
  release page itself; the README documents the alias only.

## 3. The mechanics (per item)

- **HR-068 — the trigger**: ruled at this gate (§6 Q1). The
  chosen mechanism lands in the release-please workflow (token)
  or release.yml (dispatch hop), with the seeded live proof
  HR-063 demands: the whole chain exercised once for real.
- **HR-069**: dist's pr-run-mode set to `plan` so PRs run the
  cheap plan only; paired with whatever setting clears the
  action_required approval gate for release-please's own
  branch runs (investigate: repository Actions approval policy
  for workflow runs from Actions-created branches).
- **HR-081**: extend release-please's path attribution so
  `template/**` commits count toward the houserules component
  (the payload ships in the binary; a template change IS a
  product change). Config change + a seeded proof.
- **HR-082**: the Cargo.lock version-bump residue folded into
  the release flow so a release PR leaves no stale lock line.
- **HR-048**: closed by the first release whose page shows the
  five dist archives + checksums + installer script (and
  GitHub's own source zip/tar.gz, which appear once users look
  at the right tag's page).
- **HR-049 + HR-064**: the README re-walk after the release —
  every install path moves to the latest alias (§2a; HR-049
  closes by that design change), every channel's PENDING
  paragraph is replaced by its live-verified state, each
  verification captured.
- **HR-070**: `mise use ubi:` → the `github:` backend per
  mise's current docs, verified live.
- **HR-063**: the seeded CI proof — the full chain (release PR
  merge → tag → dist → assets → installs) exercised once and
  its evidence retained in the batch workspace and cited from
  the runbook.
- Runbook: docs/runbook.md's release section rewritten to the
  proven procedure (it still narrates the retired Node
  strategy per HR-096's adjacent findings — the batch's docs
  task sweeps it to current reality).

## 4. Task shape (provisional; the plan settles it)

- T1: the machinery — trigger (as ruled), pr-run-mode,
  approval policy, release-please path attribution, Cargo.lock
  flow. Live-proven on seeded runs where possible WITHOUT
  cutting the real release.
- T2 (owner-attended checkpoint): cut the real release — merge
  the release PR (version as ruled, §6 Q2), watch the chain,
  verify the page.
- T3: the docs — README re-walk with live captures per
  channel, the runbook rewrite, the mise backend migration.
- The evals set is untouched (no agent-template change): no
  evals rerun.

## 5. Live-proof discipline

Every claim of "works" in this batch is a capture: the curl
installer run, the mise install, a direct download + checksum
verification + `houserules --version`, on a clean environment
(a container or a scrubbed PATH). The README cites classes,
the workspace holds the literals (cite-into-artifact).

## 6. Ruling questions for the gate

1. **HR-068, the trigger — RULED**: a fine-grained PAT stored
   as a repository Actions secret, used by the release-please
   workflow so its tag push triggers release.yml (the
   DEPENDABOT_AUTOMERGE_TOKEN precedent). The owner mints it
   once (contents:write on this repository) when T1 asks.
2. **The version — RULED**: 0.3.0, the -alpha suffix dropped
   (release-please re-aimed; prerelease suffixes would exclude
   releases from the latest alias, and 0.x already carries the
   pre-1.0 signal). Release PR #18's 1.0.0-alpha is
   superseded.
3. **The 0.2.0-alpha remains — RULED**: left as history; the
   next release becomes the page's face and the README stops
   pointing at them.
