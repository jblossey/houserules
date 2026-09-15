# Batch 25 spec addendum: the template-defaults review decisions

Recorded 2026-09-14 from the owner's private review page read-back
(design.md 5.80; the page and its database are purged after T2
consumes them - this file is the durable row-level record). Verdicts:
Keep = template-appropriate as shipped (on a GAP row: ADOPT into the
template, owner-confirmed); Change = appropriate with the quoted
changes; Drop = not template-appropriate (on a GAP row: do not ship).
150 keep / 86 drop / 15 change; 253 of 258 rows ticked; the 6
workflow rows fold under the whole-workflow ruling and the migrate
skill's When-to-use row keeps its text (owner-confirmed). This file
quotes the owner's own words only; no tag-pilot content.

## Cross-cutting directives (owner, verbatim where quoted)

1. The ask-the-user pattern becomes ask-the-owner/decider EVERYWHERE:
   "in completely autonomous projects, users expect that decisions
   are taken over by an orchestrator or manager agent. Our houserules
   must respect this diversity. ... The intention is that clarifying
   questions must be asked when things are unclear. Who answers these
   questions depends on the project and environment setup. I have not
   flagged every following item where this needs adjustment. I expect
   you to change this pattern throughout all applicable items."
2. "never add concrete references to the houserules repo to any
   template file!" - every batch/controller/repo-history reference is
   swept from every template file.
3. De-Rust the templates: "crate is rust-specific. consuming repos
   might not be written in rust. change this throughout all templates
   to be generic."
4. The seeded CI workflow leaves the payload: "projects might not be
   on github. Therefore, the ci workflow setup must be a part of init
   or migrate instead of a concrete config." (Covers all workflow
   rows.)
5. Template rules that are houserules-repo opinion, not universal,
   leave the fixed rule set: ff-only-merges, no-coauthor,
   sequential-agents, the effortLevel pin; model-policy becomes a
   recommendation. The init/migrate flow elicits each project's own
   merge/attribution/parallelism discipline instead.
6. Standing flips: contract-refresh-sweep, gate-shell-chains,
   skills-for-procedures, keep-knowledge-current (the last also in
   this repository's own copy).

## Change rows (verdict: change; owner text verbatim)

- `row-areas-defaults`
  - changes: remove github
- `row-claude-md-workflow-section`
  - changes: change user gate to the more generic form, remove the limitation of running only sequential agents.
- `row-eval-dependency-add`
  - changes: crate is rust-specific. consuming repos might not be written in rust. change this throughout all templates to be generic.
- `row-seeded-entry-process-ask-when-missing`
  - changes: ask the owner/decider
  - notes: in completely autonomous projects, users expect that decisions are taken over by an orchestrator or manager agent. Our houserules must respect this diversity. This formulation must be adjusted across all items. The intention is that clarifying questions must be asked when things are unclear. Who answers these questions depends on the project and environment setup. I have not flagged every following item where this needs adjustment. I expect you to change this pattern throughout all applicable items.
- `row-seeded-entry-process-contract-refresh-sweep`
  - changes: standing: true
  - notes: there is a reference to some batch and controller which shouldn't make it to the template because all template items must not contain references to the houserules repo itself.
- `row-seeded-entry-process-gate-shell-chains`
  - changes: standing: true
- `row-seeded-entry-process-model-policy`
  - changes: this must be a recommendation, not a fixed rule because different projects might have different preferences
- `row-seeded-entry-process-skills-for-procedures`
  - changes: make standing
- `row-seeded-entry-writing-style-code-comments`
  - changes: Add that references to these docs are allowed to be added to the comments
- `row-skill-finishing-a-feature-common-mistakes`
  - changes: change appropriately
- `row-skill-finishing-a-feature-overview`
  - changes: drop ff-only and no co-author lines. They are true for houserules but not necessarily for consuming repos.
- `row-skill-finishing-a-feature-procedure`
  - changes: make sure to change this appropriately to the changes demanded
- `row-skill-migrating-knowledge-intro`
  - changes: it is not only standing rules we seed, is it? We also seed rules that are case-dependent. also add the notion that knowledge that is only present in memory should be made explicit
- `row-skill-migrating-knowledge-inventory`
  - changes: knowledge that must be migrated also might live in past conversations and in memory
- `row-workflow-job-defaults`
  - changes: This change request applies to the whole ci workflow: projects might not be on github. Therefore, the ci workflow setup must be a part of init or migrate instead of a concrete config.

## Keep rows carrying owner text

- `row-gap-repo-drift-knowledge-base-state-only-the-source`
  - changes: never add concrete references to the houserules repo to any template file!
- `row-seeded-entry-process-keep-knowledge-current`
  - notes: Why is this not standing? I need more context here.

## Drop rows (verdict: drop)

- 46 of the 49 gap-tagpilot rows: dropped. Their ids stay in the
  gitignored workspace record only - this repository is public and
  even tag-pilot's entry names stay out of tracked files (owner
  directive on tag-pilot sensitivity). THREE keep-ticks stand and
  are ADOPTED, ruled 2026-09-15 (design.md 5.81) after a controller
  mis-aggregation first recorded them as drops:
  row-gap-tagpilot-generic-toolchain-tests-clean-temp-dirs,
  row-gap-tagpilot-only-process-pointer-sweep-unfiltered,
  row-gap-tagpilot-only-process-report-evidence-verbatim (their
  generic content enters the template de-referenced; publication of
  these three is the owner's 5.81 ruling).
- `row-agent-template-implementer-working-commits` - applied as the
  no-coauthor half only ("Never add a co-author line."); the
  Conventional-Commits half of the same bullet stays, because
  `process.conventional-commits` remains a template standing rule with
  a live deterministic check and dropping the whole bullet would leave
  that seeded rule unenforced in the implementer template itself
  (T2 review, finding 7).
- `row-agent-template-implementer-working-no-subagents`
- `row-gap-repo-drift-process-brainstorm-first`
- `row-gap-repo-drift-security-hygiene-exact-pins`
- `row-gap-repo-houserules-houserules-actions-default-shell-lacks-pipefail`
- `row-gap-repo-houserules-houserules-actions-pinned-by-sha`
- `row-gap-repo-houserules-houserules-corpus-batch14-fixtures-are-committed`
- `row-gap-repo-houserules-houserules-crash-paths-are-named`
- `row-gap-repo-houserules-houserules-default-token-tags-start-no-workflows`
- `row-gap-repo-houserules-houserules-dev-tools-are-rust-native`
- `row-gap-repo-houserules-houserules-live-run-recipe`
- `row-gap-repo-houserules-houserules-mise-cooldown-on-release-day`
- `row-gap-repo-houserules-houserules-path-pins-mirror-the-code`
- `row-gap-repo-houserules-houserules-payload-embeds-checkout-bytes`
- `row-gap-repo-houserules-houserules-payload-runs-on-builtins`
- `row-gap-repo-houserules-houserules-payload-stamp-gate`
- `row-gap-repo-houserules-houserules-pinned-shas-live-on-mains-ancestry`
- `row-gap-repo-houserules-houserules-platform-gated-tests`
- `row-gap-repo-houserules-houserules-pnpm-only`
- `row-gap-repo-houserules-houserules-post-release-restamp`
- `row-gap-repo-houserules-houserules-readme-mirrors-kit-owned`
- `row-gap-repo-houserules-houserules-release-footer-survives-aggregation`
- `row-gap-repo-houserules-houserules-release-please-owns-the-release-object`
- `row-gap-repo-houserules-houserules-release-please-schema-gap`
- `row-gap-repo-houserules-houserules-release-workflow-is-generated`
- `row-gap-repo-houserules-houserules-rename-map`
- `row-gap-repo-houserules-houserules-rust-toolchain-bumps-use-stable`
- `row-gap-repo-houserules-houserules-seeded-repo-live-proof`
- `row-gap-repo-houserules-houserules-tag-pilot-is-read-only`
- `row-gap-repo-houserules-houserules-task-gates-mirror-ci`
- `row-gap-repo-houserules-houserules-template-is-the-source`
- `row-gap-repo-houserules-houserules-tests-clean-scratch-dirs`
- `row-gap-repo-new-knowledge-base-cite-durable-refs`
- `row-gap-repo-new-process-every-main-commit-is-ci-green`
- `row-gap-repo-new-process-gates-cover-generated-commits`
- `row-gap-repo-new-quality-absence-is-designed`
- `row-seeded-entry-process-ff-only-merges` - this is very opinionated. during an init process, the orchestrator that performs the init should name the process explicitly but many codebases operate differently and we can't keep this as a template rule.
- `row-seeded-entry-process-sequential-agents`
- `row-seeded-entry-security-hygiene-no-coauthor`
- `row-settings-effort-level`

## Keep rows (verdict: keep, no text) and unticked

148 rows kept as shipped; ids in the batch workspace's
t2-decisions/summary.json (workspace record). Unticked/blank: the 6
workflow rows (folded under directive 4) and
row-skill-migrating-knowledge-when (keeps its text).
