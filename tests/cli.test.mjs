import { describe, expect, it } from 'vitest';
import { UsageError, parseArgs } from '../template/tools/lib/cli.mjs';

describe('UsageError', () => {
  it('is an Error subclass', () => {
    expect(new UsageError('x')).toBeInstanceOf(Error);
  });
});

describe('parseArgs', () => {
  it('splits positionals and --key value / --flag options, honoring booleanOpts', () => {
    expect(
      parseArgs(
        ['a', '--base', 'x', '--full', 'b', '--ids', 'p.q,r.s'],
        new Set(['full', 'check']),
      ),
    ).toEqual({
      positional: ['a', 'b'],
      opts: { base: 'x', full: true, ids: 'p.q,r.s' },
    });
    expect(parseArgs(['--check'], new Set(['full', 'check']))).toEqual({
      positional: [],
      opts: { check: true },
    });
  });
  it('treats a value-style flag with nothing after it as a bare true', () => {
    // Covers the branch where a non-boolean flag (per
    // booleanOpts) is the last token, so there is no value to consume —
    // e.g. a user typing `--area` and forgetting the value.
    expect(parseArgs(['--area'], new Set(['full', 'check']))).toEqual({
      positional: [],
      opts: { area: true },
    });
  });
  it('treats every option as valued when booleanOpts is omitted', () => {
    expect(parseArgs(['--batch', '34'])).toEqual({
      positional: [],
      opts: { batch: '34' },
    });
    expect(parseArgs(['--open'])).toEqual({
      positional: [],
      opts: { open: true },
    });
  });
});
