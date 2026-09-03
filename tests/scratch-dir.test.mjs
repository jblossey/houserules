import { existsSync } from 'node:fs';
import { describe, expect, it, onTestFinished } from 'vitest';
import { scratchDir } from './scratch-dir.mjs';

describe('scratchDir', () => {
  it('removes the directory when the test finishes', () => {
    let dir;
    // Vitest runs onTestFinished callbacks in reverse registration order, so
    // registering this observer first puts it after scratchDir's own
    // cleanup, letting one self-contained test assert both sides.
    onTestFinished(() => expect(existsSync(dir)).toBe(false));
    dir = scratchDir('scratch-dir-fixture-');
    expect(existsSync(dir)).toBe(true);
    expect(dir).toContain('scratch-dir-fixture-');
  });
});
