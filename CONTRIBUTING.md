# Contributing to houserules

Thank you for looking at houserules. This file covers the development
workflow for this repository.

## Setup

houserules itself runs on the compiled `houserules` binary, built from
`crates/houserules` -- no other toolchain is needed. Rust is pinned in
[mise](https://mise.jdx.dev).

```sh
git clone https://github.com/jblossey/houserules.git
cd houserules
mise run setup   # the commit-msg hook, houserules on PATH
```

`mise run setup` points `core.hooksPath` at `.githooks` and runs `cargo
install --path crates/houserules --bin houserules` so the binary is on
`PATH`. The commit-msg hook's
conventional-commit arm (`houserules check-commit`) runs only when that
probe succeeds; without it, the hook degrades to the trailer check alone
and the `check-commit` CI job is the backstop that still catches a bad
subject or body before merge.

## The batch process

houserules develops itself with its own kit (backlog id prefix `HR`). Work
starts from the backlog, never from an ad hoc idea:

1. A change gets a backlog item (`backlog/`) and, for anything beyond a
   trivial fix, a written spec (`docs/specs/`) that the owner approves
   before implementation starts.
2. An implementer agent does the work, one task at a time. Agents never
   run two at a time; every step in a batch is strictly sequential.
3. A task-reviewer agent reviews every task's diff on a stronger model
   than the one that implemented it. Every finding gets fixed, or gets a
   reasoned deferral filed as its own backlog item — never a `TODO` left
   in code.
4. A branch-reviewer agent reviews the finished branch before it merges.
   Merges are fast-forward only, from the command line — no merge
   commits, no GitHub squash or merge buttons.

`.claude/agents/`, `.claude/skills/orchestrating/SKILL.md`, and
`knowledge/process.json` carry the full rules; this section is the short
version for outside contributors.

## Commits

Commits follow [Conventional Commits](https://www.conventionalcommits.org/):
a type, an optional scope, and a lowercase subject (`fix(cli): reject an
empty id prefix`). The header stays at most 100 characters; body lines
stay at most 100 characters too.

Never add a `Co-Authored-By:` or `Claude-Session:` trailer to a commit, no
matter what tool wrote the message. `.githooks/commit-msg` rejects both
trailers, then probes `houserules check-commit --help` and, when it
succeeds, runs `houserules check-commit` against the rest of the message.

## Before you open a pull request

```sh
mise run lint    # shellcheck, cargo deny, check-knowledge, check-backlog, render --check, the residue gate
cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test
```

Both must pass locally. CI (`.github/workflows/ci.yml`) runs the same
gates, plus a `houserules check-commit` pass over your commit range and a
knowledge-base audit of your diff, on every pull request.
