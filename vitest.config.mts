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
      include: [
        'bin/houserules.mjs',
        'template/tools/lib/json-store.mjs',
        'template/tools/lib/cli.mjs',
      ],
      thresholds: { lines: 80, branches: 99 },
    },
  },
}));
