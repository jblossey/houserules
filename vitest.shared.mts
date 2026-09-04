// Test-runner settings every vitest config in this repository shares;
// only `test.include` and `test.coverage` differ between them (HR-054
// task 4 fix round 1, finding 4: vitest.config.mts and
// vitest.kb-coverage.config.mts both import this).
export const sharedTestConfig = {
  name: 'houserules',
  watch: false,
  globals: true,
  environment: 'node',
  reporters: ['default'],
  restoreMocks: true,
};
