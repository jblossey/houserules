---
paths:
  - "tests/**"
---
Generated from knowledge/ by houserules render. Do not edit.

# Tests rules

## Rules

- [houserules.tests-clean-scratch-dirs] Every test that mints a scratch directory registers its removal at the mkdtemp site: `tempfile::TempDir`'s own Drop, or an equivalent Drop-owning guard.
- [quality.pin-copies-byte-exact] Pin a hand-synced copy byte-exact modulo its one designed difference, expressed as the production transform; never normalize by deleting the differing field.

## Gotchas

- [houserules.corpus-batch14-fixtures-are-committed] tests/fixtures/batch14-workspace/ commits gitignored .superpowers/ batch-14 deliverables verbatim, host paths neutralized.
- [houserules.pinned-shas-live-on-mains-ancestry] A committed test or fixture that pins a git sha must pin one on main's ancestry, proven reachable in a fresh clone.
- [houserules.vitest-coverage-floor-tracks-the-rust-port] RETIRED at batch 20 T3: the vitest ratchet ended with vitest itself; cargo's suites are the ongoing gate, with no coverage threshold of their own.
- [houserules.vitest-restore-mocks-scope] RETIRED at batch 20 T3 (HR-047): vitest left the repository entirely, and with it every `vi.mock`/`vi.spyOn` call site this gotcha governed.

Detail: houserules get <id>
