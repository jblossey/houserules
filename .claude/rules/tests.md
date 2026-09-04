---
paths:
  - "tests/**"
  - "vitest.config.mts"
  - "vitest.shared.mts"
  - "vitest.kb-coverage.config.mts"
---
Generated from knowledge/ by tools/kb.sh render. Do not edit.

# Tests rules

## Rules

- [houserules.tests-clean-scratch-dirs] Every test that mints a scratch directory registers its removal at the mkdtemp site: the shared helper, or onTestFinished/afterEach with recursive rmSync.
- [quality.pin-copies-byte-exact] Pin a hand-synced copy byte-exact modulo its one designed difference, expressed as the production transform; never normalize by deleting the differing field.

## Gotchas

- [houserules.corpus-batch14-fixtures-are-committed] tests/corpus/fixtures/batch14-workspace/ commits gitignored .superpowers/ batch-14 deliverables verbatim, host paths included.
- [houserules.pinned-shas-live-on-mains-ancestry] A committed test or fixture that pins a git sha must pin one on main's ancestry, proven reachable in a fresh clone.
- [houserules.vitest-coverage-floor-tracks-the-rust-port] A ported file's coverage floor moves to its own excluded, separately-ratcheted vitest run; the still-JS-owned files' global floor never drops.
- [houserules.vitest-restore-mocks-scope] Vitest `restoreMocks` restores `vi.spyOn` spies only; reset a `vi.mock` factory’s `vi.fn` in an explicit `afterEach` with `mockReset()`.

Detail: tools/kb.sh get <id>
