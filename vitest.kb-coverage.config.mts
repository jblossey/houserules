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
export default defineConfig(() => ({
  root: import.meta.dirname,
  test: {
    ...sharedTestConfig,
    include: ['tests/kb.test.mjs'],
    coverage: {
      reportsDirectory: './test-output/vitest/coverage-kb',
      provider: 'v8' as const,
      include: ['template/tools/kb.mjs'],
      thresholds: { branches: 67, lines: 72 },
    },
  },
}));
