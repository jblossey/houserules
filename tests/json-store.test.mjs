import { mkdtempSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { execFileSync } from 'node:child_process';
import { describe, expect, it } from 'vitest';
import {
  Errors,
  emit,
  listJsonFiles,
  readJson,
  repoRoot,
  validate,
} from '../template/tools/lib/json-store.mjs';

function tmp() {
  return mkdtempSync(join(tmpdir(), 'json-store-'));
}

describe('emit', () => {
  it('serializes a value as indented JSON with a trailing newline', () => {
    expect(emit({ a: 1 })).toBe('{\n  "a": 1\n}\n');
  });
});

describe('readJson', () => {
  it('parses a file', () => {
    const dir = tmp();
    writeFileSync(join(dir, 'a.json'), '{"a":1}');
    expect(readJson(join(dir, 'a.json'))).toEqual({ a: 1 });
  });
  it('names the file on a missing path', () => {
    expect(() => readJson('/nonexistent/x.json')).toThrow(/x\.json/);
  });
  it('names the file on invalid JSON', () => {
    const dir = tmp();
    writeFileSync(join(dir, 'bad.json'), '{');
    expect(() => readJson(join(dir, 'bad.json'))).toThrow(
      /bad\.json: invalid JSON/,
    );
  });
});

describe('listJsonFiles', () => {
  it('lists json files sorted, ignoring others', () => {
    const dir = tmp();
    writeFileSync(join(dir, 'b.json'), '{}');
    writeFileSync(join(dir, 'a.json'), '{}');
    writeFileSync(join(dir, 'c.md'), '');
    expect(listJsonFiles(dir)).toEqual([
      join(dir, 'a.json'),
      join(dir, 'b.json'),
    ]);
  });
});

describe('repoRoot', () => {
  it('returns the git toplevel of a directory', () => {
    const dir = tmp();
    execFileSync('git', ['init', '-q', dir]);
    expect(repoRoot(dir)).toBe(
      execFileSync('git', ['-C', dir, 'rev-parse', '--show-toplevel'], {
        encoding: 'utf8',
      }).trim(),
    );
  });
});

describe('Errors', () => {
  it('collects messages', () => {
    const errors = new Errors();
    expect(errors.any).toBe(false);
    errors.add('x');
    expect(errors.any).toBe(true);
    expect(errors.list).toEqual(['x']);
  });
});

function run(value, schema, root) {
  const errors = new Errors();
  validate(value, schema, '$', errors, root);
  return errors.list;
}

describe('validate', () => {
  it('accepts a matching object and reports every violation once', () => {
    const schema = {
      type: 'object',
      required: ['id', 'n'],
      additionalProperties: false,
      properties: {
        id: { type: 'string', pattern: '^[a-z]+$', minLength: 2, maxLength: 3 },
        n: { type: 'integer', minimum: 1 },
        tags: { type: 'array', items: { type: 'string' }, uniqueItems: true },
        kind: { enum: ['a', 'b'] },
      },
    };
    expect(run({ id: 'ab', n: 1, tags: ['x'], kind: 'a' }, schema)).toEqual([]);
    expect(
      run({ id: 'A', n: 0, tags: ['x', 'x'], kind: 'c', extra: 1 }, schema),
    ).toEqual([
      '$.id: must match ^[a-z]+$',
      '$.id: shorter than 2',
      '$.n: below 1',
      '$.tags: items must be unique',
      '$.kind: must be one of "a", "b"',
      '$: unknown field "extra"',
    ]);
    expect(run({ id: 'abcd' }, schema)).toEqual([
      '$: missing "n"',
      '$.id: longer than 3 characters',
    ]);
  });
  it('checks types including lists and null', () => {
    expect(run(null, { type: ['string', 'null'] })).toEqual([]);
    expect(run(1.5, { type: 'integer' })).toEqual(['$: must be integer']);
    expect(run([], { type: 'object' })).toEqual(['$: must be object']);
    expect(run({}, { type: 'array' })).toEqual(['$: must be array']);
    expect(run('x', { type: ['integer', 'null'] })).toEqual([
      '$: must be integer or null',
    ]);
    expect(run(true, { type: 'boolean' })).toEqual([]);
  });
  it('validates array items and additionalProperties schemas', () => {
    expect(
      run([1, 'x'], { type: 'array', items: { type: 'integer' } }),
    ).toEqual(['$[1]: must be integer']);
    expect(run([1, 2, 3], { type: 'array' })).toEqual([]);
    expect(
      run(
        { a: { paths: 1 } },
        {
          type: 'object',
          additionalProperties: {
            type: 'object',
            properties: { paths: { type: 'array' } },
          },
        },
      ),
    ).toEqual(['$.a.paths: must be array']);
    expect(run({ a: 1 }, { type: 'object' })).toEqual([]);
  });
  it('resolves local $ref and rejects other refs', () => {
    const root = {
      $defs: { id: { type: 'string', pattern: '^x$' } },
      type: 'object',
      properties: { id: { $ref: '#/$defs/id' } },
    };
    expect(run({ id: 'x' }, root, root)).toEqual([]);
    expect(run({ id: 'y' }, root, root)).toEqual(['$.id: must match ^x$']);
    expect(() => run('x', { $ref: 'http://x' }, root)).toThrow(
      /unsupported \$ref/,
    );
    expect(() => run('x', { $ref: '#/$defs/nope' }, root)).toThrow(
      /unresolved \$ref/,
    );
  });
});
