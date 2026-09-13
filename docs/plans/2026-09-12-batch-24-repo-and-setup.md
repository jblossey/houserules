# Batch 24 plan: the repo-and-setup batch

Spec: docs/specs/2026-09-12-batch-24-repo-and-setup.md
(approved as amended 2026-09-12; rulings 5.67).
Items: HR-108, HR-048, HR-068, HR-081, HR-082, HR-049, HR-063,
HR-064, HR-069, HR-070.
Workspace: .superpowers/sdd/2026-09-12-batch-24/.
Order: T1 (machinery, seeded proofs) → T3a (the docs rewrite,
pre-merge) → the owner mints the PAT → the batch merges → T2
(the real 0.3.0 release, owner-attended) → T3b (the
post-release live verification, close branch).
Implementers on sonnet, task reviews on opus, branch review on
fable, strictly sequential.

## Code-health scan (process.code-health-scan)

- `.github/workflows/release.yml` is GENERATED
  (`houserules.release-workflow-is-generated`): every change
  goes through dist-workspace.toml + `dist generate`, with
  `dist generate --check` as the gate. Never hand-edit it.
- `dist-workspace.toml:63` carries `pr-run-mode = "upload"` —
  HR-069's target (→ `plan`); its comment block around :54
  documents the dispatch-releases interplay — re-verify those
  claims against dist's current docs before changing either.
- `.github/workflows/release-please.yml` runs on the default
  token today; the PAT wiring lands there. Keep the workflow's
  permissions minimal: the token comes from the secret, not
  from widened workflow permissions.
- `release-please-config.json` (or its equivalent) holds the
  rust release-type at crates/houserules; both the 0.3.0
  re-aim and the template/** attribution land there —
  everything verified against release-please's CURRENT docs
  (security-hygiene.verify-current-docs; internal knowledge of
  its config surface is stale, proven twice in this repo).
- Every workflow `uses:` stays SHA-pinned
  (houserules.actions-pinned-by-sha); release.yml's pins live
  in dist-workspace.toml's [dist.github-action-commits],
  regenerated — never hand-edited.
- Actions policy: sha_pinning_required is ON; the
  action_required approval gate for release-please-branch runs
  is a repository Actions setting — investigate via the API,
  change the narrowest setting that clears it, and record
  which.

## T1 — the machinery (seeded proofs, no real release)

1. The trigger: release-please.yml uses a fine-grained PAT
   from a repository Actions secret (name it
   RELEASE_PLEASE_TOKEN) so its tag push triggers release.yml.
   Wire the workflow to the secret NOW; the secret itself is
   minted by the owner (contents:write on this repository)
   when this task's wiring is reviewed — the batch pauses
   there for the seeded proof.
2. The version: re-aim release-please at 0.3.0 (the
   release-as/Release-As mechanism per current docs),
   superseding PR #18's 1.0.0-alpha; the -alpha suffix is
   gone from the next and all future versions unless the
   owner rules otherwise.
3. HR-069: pr-run-mode = "plan" via dist-workspace.toml +
   `dist generate`; the regenerated release.yml committed with
   the config in the same commit.
4. The approval gate: identify and change the repository
   Actions setting that leaves release-please-branch runs at
   action_required; capture before/after.
5. HR-081: template/** commits count toward the component's
   release attribution (per release-please's current config
   surface for extra paths/components); seeded proof: a
   template-only commit on a scratch branch produces a
   release-please diff/PR bump where BASE config produces
   none. NO real release PR churn beyond the 0.3.0 re-aim.
6. HR-082: the Cargo.lock version line updates inside the
   release flow (release-please's cargo-workspace plugin or a
   lock-refresh step) so a release PR carries a consistent
   lock; seeded proof on the scratch branch.
7. Gates: the full CI-mirroring set + `dist generate --check`
   + `shellcheck` for any script touched.

## The PAT checkpoint (owner, once)

After T1's review closes: the owner mints the fine-grained PAT
(this repository only; Contents: Read and write; nothing else)
and stores it as the RELEASE_PLEASE_TOKEN Actions secret. Then
the seeded chain proof runs: a scratch prerelease-style tag on
a throwaway branch is NOT possible with release-please
end-to-end without merging — so the chain proof IS T2's real
release, watched; HR-063's evidence is that watched chain.

## T2 — the real release (owner-attended)

Merge the re-aimed release PR (0.3.0); watch: tag push →
release.yml triggers (the PAT proof) → five archives +
checksums + installer on the release page, marked latest;
`releases/latest/download/houserules-installer.sh` resolves.
Capture every hop. HR-048 and HR-068 close here; the
post-merge restamp step (finishing skill step 8) runs:
`houserules update --dir .` reads the new version drift and
the stamp commits.

## T3 — the docs (split; amended after T1's fix round 3)

T3a (rides the batch, BEFORE the merge): README's install
paths move to the latest alias (§2a) and the versioned
x-release-please URL blocks retire — /README.md leaves
extra-files if nothing remains for it to update — so the
release PR's regeneration after the batch merges never writes
a wrong-shape tag into the README. The runbook's release
section rewritten to the proven procedure, its retired-Node
narration swept. The mise section moves to the github:
backend (HR-070).

T3b (after T2's release, on the close branch): each channel
exercised from a clean environment (scrubbed PATH or
container; curl installer, mise github: backend, direct
download + sha256 + --version) with captures in the workspace;
the PENDING paragraphs replaced by their live-verified state.
HR-049, HR-063, HR-064, HR-070 close across T3a+T3b; HR-108's
umbrella closes with the batch.

## Close

Branch review (fable), aggregation to 1-5 commits with tree
identity, finishing-a-feature (fetch/rebase; checks on an OPEN
PR; the main push before any branch deletion), the report with
acceptance.
