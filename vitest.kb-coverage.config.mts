import { defineConfig } from 'vitest/config';
import { sharedTestConfig } from './vitest.shared.mts';

// HR-054 task 4 fix round 1, finding 4 (task-4-review.json): the review's
// fix asked for one config with a global `branches: 99` plus a glob-keyed
// entry ratcheting template/tools/kb.mjs at its own lower number. Verified
// live that this does not hold: vitest counts every included file,
// including one carrying its own glob-keyed threshold, into the SAME
// global aggregate (docs vitest.dev/config/coverage: "Vitest counts all
// files, including those covered by glob-patterns, into the global
// coverage thresholds. This is different from Jest behavior") -- with
// kb.mjs still in vitest.config.mts's `coverage.include`, the combined
// branches percentage stayed 81%, failing a global 99% floor regardless
// of kb.mjs's own override. A single config cannot both protect the other
// four files at 99% and ratchet kb.mjs at 67%/72%.
//
// This second, kb.mjs-only coverage run is the config-level equivalent of
// excluding kb.mjs from the global aggregate the way Jest would: it
// measures only kb.mjs (`tests/kb.test.mjs` is the one spec file that
// exercises it), gated at the measured post-removal numbers so it cannot
// slide further, while vitest.config.mts's own run keeps the other four
// files at the pre-port 99% branches floor, unweakened by kb.mjs's drop.
//
// Batch 17 T3 (houserules.vitest-coverage-floor-tracks-the-rust-port): the
// `audit`/`validate`/`stats` vitest cases ported to cargo and left this
// file (`tests/kb.test.mjs`'s own `audit`/`stats`/`main (audit, stats)`/
// `validate`/`main (validate)` describe blocks), so kb.mjs's own measured
// coverage fell again -- 67.08/72.85 (post-T2, backlog-command-adjacent
// surfaces already gone) to 18.98/26.01 (post-T3, only `loadBase`, `byId`,
// `list`, and the read commands -- `topics`/`index`/`get`/`for`/`standing`
// plus their `main` dispatch -- are still exercised in-process; `render`/
// `check`/`audit`/`validate`/`stats` all left). The floor moves down to the
// newly measured numbers, per the ratchet pattern; it is not minted at 0
// because a real, still-JS-owned surface (the read commands) remains
// covered in-process. (Fix round 1, issue 2, task-3-review.json: the
// figure here and the branches/lines pair below both originally read
// 18.98/26.67 -- 26.67 is the coverage table's Statements column, not
// Lines, which reads 26.01; the floor of 26 already held against the
// correct number, so nothing was actually broken.)
//
// Batch 17 T4: the knowledge read commands (`topics`/`index`/`get`/`for`/
// `standing`) and `main`'s own dispatch switch ported to Rust and left this
// file too (`tests/kb.test.mjs`'s `read commands`/`main (read commands)`
// describe blocks), so coverage fell again -- 18.98/26.01 to 5.06/10.18
// (measured; floored to 5/10 below). Only `loadBase`, `byId`, and `list`
// are still exercised in-process (`describe('loadBase')`, `describe('byId')`,
// `describe('list')`, and `describe('the repository knowledge base')`'s own
// `loadBase` call); `main` itself is reached only through the live gate's
// subprocess spawns now, which v8's coverage instrumentation cannot see
// (the same reason T2's backlog-coverage ratchet measured 0/0 once its
// whole command surface left in one task -- this floor is not 0 only
// because `loadBase` remains a real, still-JS-owned surface exercised
// in-process). Not yet 0/0, so this ratchet stays (not deleted, per the
// gotcha's T2 precedent for a genuine 0-floor case) with its threshold
// moved down to the newly measured numbers.
export default defineConfig(() => ({
  root: import.meta.dirname,
  test: {
    ...sharedTestConfig,
    include: ['tests/kb.test.mjs'],
    coverage: {
      reportsDirectory: './test-output/vitest/coverage-kb',
      provider: 'v8' as const,
      include: ['template/tools/kb.mjs'],
      thresholds: { branches: 5, lines: 10 },
    },
  },
}));
