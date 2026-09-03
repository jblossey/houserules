import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { onTestFinished } from 'vitest';

/**
 * Mints a fresh directory under the OS temp root and registers its removal
 * for when the running test finishes, so the suite leaves the temp root as
 * it found it.
 *
 * @param {string} prefix - `mkdtemp` prefix, e.g. `'kb-'`.
 * @returns {string} the new directory's absolute path.
 */
export function scratchDir(prefix) {
  const dir = mkdtempSync(join(tmpdir(), prefix));
  onTestFinished(() => rmSync(dir, { recursive: true, force: true }));
  return dir;
}
