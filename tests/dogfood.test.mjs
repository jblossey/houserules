// This repository runs its own kit (docs/design.md §5.5). The root copy of every
// kit-owned file is installed by `node bin/houserules.mjs update --dir .` and must
// stay byte-identical to its source in template/.
import { existsSync, readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';
import { KIT_OWNED, SEED_ONCE } from '../bin/houserules.mjs';

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

describe('the deliverables schema copies', () => {
  // .claude/schemas/deliverables.json is SEED_ONCE: update never writes it, so its
  // root and template copies are hand-synced. `init` seeds the root copy once,
  // rewriting every WI- occurrence to the project's id prefix (bin/houserules.mjs's
  // PREFIXED rewrite); `update` skips it. That same rewrite is the exact pin: any
  // other drift, a dropped or reformatted backlogId pattern included, fails the
  // suite.
  const SCHEMA_PATH = '.claude/schemas/deliverables.json';

  it('equals its template source with the id prefix rewrite applied', () => {
    const { idPrefix } = JSON.parse(read('.houserules.json'));
    expect(read(SCHEMA_PATH)).toBe(
      read(`template/${SCHEMA_PATH}`).replaceAll('WI-', `${idPrefix}-`),
    );
  });
});

describe('the seeded eval scenario copies', () => {
  // .claude/evals/*.json entries in SEED_ONCE are not PREFIXED: init copies them
  // unchanged, so the pin is plain byte equality, no transform. record.json is
  // excluded by design: the root copy accumulates run sets across evaluations
  // while the template ships only the seed record.
  const SCENARIOS = SEED_ONCE.filter(
    (file) => file.startsWith('.claude/evals/') && file !== '.claude/evals/record.json',
  );

  it('derives at least one seeded scenario from SEED_ONCE', () => {
    expect(SCENARIOS).not.toHaveLength(0);
  });

  it.each(SCENARIOS)('%s at the root equals its template source', (file) => {
    expect(read(file)).toBe(read(`template/${file}`));
  });
});
