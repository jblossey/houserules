// This repository runs its own kit (docs/design.md §5.5). The root copy of every
// kit-owned file is installed by `node bin/houserules.mjs update --dir .` and must
// stay byte-identical to its source in template/.
import { existsSync, readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';
import { KIT_OWNED } from '../bin/houserules.mjs';

const ROOT = fileURLToPath(new URL('../', import.meta.url));
const read = (path) => readFileSync(`${ROOT}${path}`, 'utf8');

describe('dogfood', () => {
  it.each(KIT_OWNED)('%s at the root equals its template source', (file) => {
    expect(existsSync(`${ROOT}${file}`)).toBe(true);
    expect(read(file)).toBe(read(`template/${file}`));
  });
  it('stamps the installed version and the HR id prefix', () => {
    const stamp = JSON.parse(read('.houserules.json'));
    expect(stamp).toEqual({
      version: JSON.parse(read('package.json')).version,
      idPrefix: 'HR',
    });
  });
});
