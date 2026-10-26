---
name: readme-rework
description: Use before a houserules release, or when the README needs rework: an independent discovery of what houserules is, first-time-reader personas, and a best-practice rewrite, scaled to a major, minor, or patch release
---

# The README rework at a release

Before each release, the README gets the rework that the release calls for. The steps come from the 2.0.0 rework (design.md 285 to 288). The briefs and the persona catalog are beside this file.

## Rules for every run

- The README follows the practice of open-source READMEs: a short description, install, usage with real output, the features in brief, concepts, documentation links, contributing, and the license last. Read the current guidance at each major run: [GitHub Docs: About the repository README file](https://docs.github.com/en/repositories/managing-your-repositorys-settings-and-features/customizing-your-repository/about-readmes), the [standard-readme spec](https://github.com/RichardLitt/standard-readme/blob/main/spec.md), and [Make a README](https://www.makeareadme.com/).
- Every houserules term the README uses is explained in plain words where it first appears and links to its entry in the README's short glossary. A picture replaces a wall of text where it shows the mechanism better: the core loop, what `init` writes, the optional layers, the review flow (design.md 290).
- The persona reports are evidence of where a first-time reader gets lost. Fix those places inside the structure above. Do not add a section, box, or heading for a persona, and do not address a reader type (design.md 286).
- Readers and the discovery agent are general-purpose agents, not the implementer or reviewer templates of this repository: those load houserules knowledge. A persona reads only a frozen copy of the README and researches nothing.
- The discovery agent never reads the README or a review of it. It learns houserules from the code, the files, and the binary (design.md 287).
- Every command the README shows runs at the release head with its real output, through the README tests.
- The owner reviews the README before the release.
- Keep every brief, report, and design note in the batch workspace, with the prefix `readme-`.

## The steps

1. **Discovery.** Make a detached worktree of the release head. Remove `README.md` and every test that quotes it from its working tree. Build the binary at that head. Dispatch one agent with [`discovery-brief.md`](discovery-brief.md). It writes what houserules is, by capability, with commands and output.
2. **Persona round 1.** Freeze a copy of the current README outside the repository. Dispatch one agent per chosen persona of [`personas.md`](personas.md), with [`persona-brief.md`](persona-brief.md) and its persona text. The personas run in parallel.
3. **Findings and design.** Merge the reports into one list: the findings that many personas share, then the findings of each. Write the target structure from the practice above. The frame (the description, usage, feature overview, and glossary) comes from the discovery report and the tree, not from the old README.
4. **Rewrite.** Dispatch an implementer with the findings and design. Reference material moves whole to pages under `docs/`, with its runnable blocks and tests. The short description equals the `description` of `crates/houserules/Cargo.toml`. Review the task as any other.
5. **Persona round 2.** Run the same personas on the new README. Compare each decision and each top change with round 1. Fix what is still unclear, then take the README to the owner.

## How much to run

| Release | Discovery | Personas | Rewrite |
|---|---|---|---|
| Major | Yes. | Every persona of the catalog, in both rounds. | The whole README, reframed from the discovery report. |
| Minor | Only when the release adds or removes a capability that changes what houserules is, or the first screen no longer states what the release does. | The personas whose capabilities the release changes, and persona 13 when install, `init`, or the quick start changed. Both rounds. | The sections of the changed capabilities, the first screen if it changed, and the links to new docs pages. |
| Patch | No. | Persona 13 only, when an install step or a quick-start command changed. One round. | Only the facts the release made stale. |

To find the capabilities that a release changes, read its commits since the last tag (`git log v<last>..HEAD --format=%s`) and its `CHANGELOG.md` section. Match each change to the capabilities column of the catalog. Add a persona to the catalog when a release adds a capability that no persona exercises.
