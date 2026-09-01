import { mkdtempSync, symlinkSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { afterEach, describe, expect, it } from 'vitest';
import { UsageError, isMainModule, parseArgs } from '../template/tools/lib/cli.mjs';

const CLI_URL = new URL('../template/tools/lib/cli.mjs', import.meta.url).href;
const CLI_PATH = fileURLToPath(CLI_URL);
const OTHER_FILE = fileURLToPath(
  new URL('../template/tools/lib/json-store.mjs', import.meta.url),
);

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

describe('isMainModule', () => {
  const savedArgv1 = process.argv[1];
  afterEach(() => {
    process.argv[1] = savedArgv1;
  });

  it('is true when argv[1] is a symlink to the module', () => {
    const dir = mkdtempSync(join(tmpdir(), 'cli-entry-'));
    const link = join(dir, 'cli-link.mjs');
    symlinkSync(CLI_PATH, link);
    process.argv[1] = link;
    expect(isMainModule(CLI_URL)).toBe(true);
  });

  it('is true when argv[1] is the module\'s direct real path', () => {
    process.argv[1] = CLI_PATH;
    expect(isMainModule(CLI_URL)).toBe(true);
  });

  it('is false when argv[1] names another existing file', () => {
    process.argv[1] = OTHER_FILE;
    expect(isMainModule(CLI_URL)).toBe(false);
  });

  it('is false when argv[1] is unset', () => {
    delete process.argv[1];
    expect(isMainModule(CLI_URL)).toBe(false);
  });

  it('is false when argv[1] names a path that does not exist', () => {
    process.argv[1] = join(tmpdir(), 'cli-entry-missing', 'no-such-file.mjs');
    expect(isMainModule(CLI_URL)).toBe(false);
  });
});
