import { accessSync, chmodSync, constants, writeFileSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
import { delimiter, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';
import { scratchDir } from './scratch-dir.mjs';

const HOOK = fileURLToPath(new URL('../template/.githooks/commit-msg', import.meta.url));

/**
 * This process's own `PATH`, with every directory that holds a `houserules`
 * executable removed -- the hermetic base every `runHook` case builds its
 * own `PATH` from, so a real binary the test runner's own machine happens
 * to have installed (a local `cargo install`, say) can never leak into a
 * case that expects the probe to find nothing, while `grep`/`head`/`sh`
 * itself still resolve normally.
 */
function pathWithoutHouserules() {
  return (process.env.PATH ?? '')
    .split(delimiter)
    .filter((dir) => {
      try {
        accessSync(join(dir, 'houserules'), constants.X_OK);
        return false;
      } catch {
        return true;
      }
    })
    .join(delimiter);
}

/**
 * Runs the shipped commit-msg hook against `message` from a fresh
 * directory, with `extraPathDir` (when given) prepended to
 * `pathWithoutHouserules()`'s hermetic base as the child's `PATH`. Returns
 * the exit status and stderr regardless of whether the hook succeeds --
 * `spawnSync`, not `execFileSync`, because a successfully-exiting probed
 * command's own stderr (this file's argv-echoing fake, for instance) is
 * otherwise unobservable: `execFileSync` returns only stdout on success and
 * discards a non-throwing run's stderr entirely.
 */
function runHook(message, extraPathDir) {
  const dir = scratchDir('commit-msg-hook-');
  const msgFile = join(dir, 'MSG');
  writeFileSync(msgFile, message);
  const path = extraPathDir
    ? [extraPathDir, pathWithoutHouserules()].join(delimiter)
    : pathWithoutHouserules();
  const result = spawnSync('sh', [HOOK, msgFile], {
    cwd: dir,
    encoding: 'utf8',
    env: { ...process.env, PATH: path },
  });
  return { status: result.status, stderr: result.stderr };
}

/**
 * Writes an executable fake `houserules` script into a fresh directory: a
 * `check-commit --help` probe call exits 0 (the fake understands the
 * subcommand), and a real `check-commit <file>` call exits `code`, writing
 * `output` to stderr first if given. Returns that directory's path,
 * suitable as `runHook`'s lone `PATH` entry.
 */
function fakeHouserules(code, output) {
  const dir = scratchDir('fake-houserules-');
  const script = join(dir, 'houserules');
  const body =
    'if [ "$1" = "check-commit" ] && [ "$2" = "--help" ]; then\n' +
    '  exit 0\n' +
    'fi\n' +
    (output ? `echo '${output}' >&2\n` : '') +
    `exit ${code}\n`;
  writeFileSync(script, `#!/usr/bin/env sh\n${body}`);
  chmodSync(script, 0o755);
  return dir;
}

/**
 * Writes an executable fake `houserules` script that simulates an older
 * binary with no `check-commit` subcommand: a `check-commit --help` probe
 * call exits 2 (clap's own "unrecognized subcommand" exit, verified live
 * against a real bogus subcommand), matching spec section 6's capability-
 * probe ruling. A real `check-commit <file>` call -- one the hook must
 * never make once the probe has already failed -- is caught rather than
 * silently tolerated: it prints a distinctive line and exits 1, so a hook
 * that calls it anyway fails this file's own test loudly instead of
 * coincidentally passing. Returns the script's directory, suitable as
 * `runHook`'s lone `PATH` entry.
 */
function fakeHouserulesRejectingCheckCommit() {
  const dir = scratchDir('fake-houserules-no-check-commit-');
  const script = join(dir, 'houserules');
  writeFileSync(
    script,
    '#!/usr/bin/env sh\n' +
      'if [ "$1" = "check-commit" ] && [ "$2" = "--help" ]; then\n' +
      '  exit 2\n' +
      'fi\n' +
      'echo "should not have been invoked for real" >&2\n' +
      'exit 1\n',
  );
  chmodSync(script, 0o755);
  return dir;
}

describe('commit-msg hook', () => {
  it('accepts a trailer-free message with no output when houserules is absent from PATH', () => {
    const { status, stderr } = runHook('feat: a clean subject\n');
    expect(status).toBe(0);
    expect(stderr).toBe('');
  });

  it('rejects a Co-Authored-By trailer with one line naming it, before any probe', () => {
    const { status, stderr } = runHook('feat: x\n\nCo-Authored-By: Someone <a@b.com>\n');
    expect(status).toBe(1);
    expect(stderr.trim().split('\n')).toHaveLength(1);
    expect(stderr).toContain('Co-Authored-By');
  });

  it('rejects a Claude-Session trailer with one line naming it, before any probe', () => {
    const { status, stderr } = runHook('feat: x\n\nClaude-Session: https://example.test/s\n');
    expect(status).toBe(1);
    expect(stderr.trim().split('\n')).toHaveLength(1);
    expect(stderr).toContain('Claude-Session');
  });

  it('execs houserules check-commit when the binary is on PATH, inheriting its exit and stderr', () => {
    const path = fakeHouserules(1, 'process.conventional-commits: bad subject');
    const { status, stderr } = runHook('bad subject\n', path);
    expect(status).toBe(1);
    expect(stderr.trim()).toBe('process.conventional-commits: bad subject');
  });

  it("passes 'check-commit' and the message file's own path as argv", () => {
    const dir = scratchDir('fake-houserules-argv-');
    const script = join(dir, 'houserules');
    writeFileSync(script, '#!/usr/bin/env sh\necho "$1 $2" >&2\nexit 0\n');
    chmodSync(script, 0o755);
    const { stderr } = runHook('feat: x\n', dir);
    const [subcommand, messagePath] = stderr.trim().split(' ');
    expect(subcommand).toBe('check-commit');
    expect(messagePath.endsWith('MSG')).toBe(true);
  });

  it('degrades to the trailer gate alone when houserules is absent from PATH', () => {
    const { status, stderr } = runHook('bad subject\n');
    expect(status).toBe(0);
    expect(stderr).toBe('');
  });

  it('degrades to the trailer gate alone when an on-PATH houserules rejects the check-commit subcommand', () => {
    const path = fakeHouserulesRejectingCheckCommit();
    const { status, stderr } = runHook('bad subject\n', path);
    expect(status).toBe(0);
    expect(stderr).toBe('');
  });
});
