---
paths:
  - "tests/**"
  - "vitest.config.mts"
---
Generated from knowledge/ by tools/kb.sh render. Do not edit.

# Tests rules

## Gotchas

- [houserules.vitest-restore-mocks-scope] Vitest `restoreMocks` restores `vi.spyOn` spies only; reset a `vi.mock` factory’s `vi.fn` in an explicit `afterEach` with `mockReset()`.

Detail: tools/kb.sh get <id>
