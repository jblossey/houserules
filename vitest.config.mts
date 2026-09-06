import { defineConfig } from 'vitest/config';
import { sharedTestConfig } from './vitest.shared.mts';

export default defineConfig(() => ({
  root: import.meta.dirname,
  test: {
    ...sharedTestConfig,
    include: ['tests/*.test.mjs'],
    coverage: {
      reportsDirectory: './test-output/vitest/coverage',
      provider: 'v8' as const,
      // bin/houserules.mjs left this run at batch 18 T6
      // (houserules.vitest-coverage-floor-tracks-the-rust-port): its whole
      // command surface (init/update/files) ported to
      // crates/houserules/src/install.rs at T3/T4 of this same batch, and
      // tests/init.test.mjs now exercises the frozen file only through a
      // detached worktree's own dynamically-imported module instance (a
      // different absolute path, so v8 cannot attribute it back to this
      // one) or a real subprocess spawn -- neither counts here, the same
      // reason a fully-ported file's post-removal number floors at 0% and
      // mints no ratchet. tests/init.test.mjs and tests/dogfood.test.mjs
      // are this surface's live behavioral gate now.
      include: ['template/tools/lib/json-store.mjs', 'template/tools/lib/cli.mjs'],
      thresholds: { lines: 80, branches: 99 },
    },
  },
}));
