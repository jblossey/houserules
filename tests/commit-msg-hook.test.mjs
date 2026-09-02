import { mkdtempSync, writeFileSync } from 'node:fs';
import { execFileSync } from 'node:child_process';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

const HOOK = fileURLToPath(new URL('../template/.githooks/commit-msg', import.meta.url));

/**
 * Runs the shipped commit-msg hook against `message` from a fresh directory
 * that holds no node_modules, so the commitlint branch never fires and only
 * the trailer gate is under test. Returns the exit status and stderr.
 */
function runHook(message) {
  const dir = mkdtempSync(join(tmpdir(), 'commit-msg-hook-'));
  const msgFile = join(dir, 'MSG');
  writeFileSync(msgFile, message);
  try {
    execFileSync('sh', [HOOK, msgFile], {
      cwd: dir,
      encoding: 'utf8',
      stdio: ['ignore', 'pipe', 'pipe'],
    });
    return { status: 0, stderr: '' };
  } catch (error) {
    return { status: error.status, stderr: error.stderr };
  }
}

describe('commit-msg hook', () => {
  it('accepts a trailer-free message with no output', () => {
    const { status, stderr } = runHook('feat: a clean subject\n');
    expect(status).toBe(0);
    expect(stderr).toBe('');
  });

  it('rejects a Co-Authored-By trailer with one line naming it', () => {
    const { status, stderr } = runHook('feat: x\n\nCo-Authored-By: Someone <a@b.com>\n');
    expect(status).toBe(1);
    expect(stderr.trim().split('\n')).toHaveLength(1);
    expect(stderr).toContain('Co-Authored-By');
  });

  it('rejects a Claude-Session trailer with one line naming it', () => {
    const { status, stderr } = runHook('feat: x\n\nClaude-Session: https://example.test/s\n');
    expect(status).toBe(1);
    expect(stderr.trim().split('\n')).toHaveLength(1);
    expect(stderr).toContain('Claude-Session');
  });
});
