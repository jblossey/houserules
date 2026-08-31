import { defineConfig } from 'vitest/config';

export default defineConfig(() => ({
  root: import.meta.dirname,
  test: {
    name: 'houserules',
    watch: false,
    globals: true,
    environment: 'node',
    include: ['tests/*.test.mjs'],
    reporters: ['default'],
    restoreMocks: true,
    coverage: {
      reportsDirectory: './test-output/vitest/coverage',
      provider: 'v8' as const,
      include: [
        'bin/houserules.mjs',
        'template/tools/kb.mjs',
        'template/tools/backlog.mjs',
        'template/tools/lib/json-store.mjs',
        'template/tools/lib/cli.mjs',
      ],
      thresholds: { lines: 80, branches: 99 },
    },
  },
}));
