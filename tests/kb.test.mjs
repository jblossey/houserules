import {
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  readdirSync,
  unlinkSync,
  writeFileSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { execFileSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';
import {
  BUDGETS,
  GENERATED,
  SKILL_PATH,
  STANDING_COMMAND,
  areaFiles,
  areasFor,
  audit,
  byId,
  checkBase,
  cmdFor,
  cmdGet,
  cmdIndex,
  cmdStanding,
  cmdTopics,
  gitDiff,
  list,
  loadBase,
  main,
  render,
  renderAll,
  stats,
  topicLines,
  validateDeliverable,
} from '../template/tools/kb.mjs';
import { UsageError } from '../template/tools/lib/cli.mjs';

// The seed area enum is a starter; projects extend it. The fixtures run on a
// project-extended enum, which also proves the extension path works.
const seedSchema = JSON.parse(
  readFileSync(
    new URL('../template/knowledge/schema.json', import.meta.url),
    'utf8',
  ),
);
seedSchema.$defs.area.enum = [
  'global',
  'process',
  'rust',
  'webview',
  'api',
  'schemas',
  'infra',
  'docs',
];
/** The seed knowledge-entry schema with a project-extended area enum. */
export const SCHEMA = JSON.stringify(seedSchema);
/** The real deliverables schema, so validateDeliverable fixtures validate for real. */
export const DELIVERABLES_SCHEMA_CONTENT = readFileSync(
  new URL('../template/.claude/schemas/deliverables.json', import.meta.url),
  'utf8',
);
/** A minimal areas map covering every glob shape the tests route paths through. */
export const AREAS = {
  global: { paths: [] },
  process: { paths: [] },
  rust: { paths: ['crates/**', 'Cargo.toml'] },
  webview: { paths: ['apps/desktop/src/**'] },
  api: { paths: ['apps/api/**'] },
  schemas: { paths: ['packages/schemas/**'] },
  infra: { paths: ['tools/**', '.github/**'] },
  docs: { paths: ['docs/**', 'CLAUDE.md'] },
};

/** Runs a git command in `root`'s worktree and returns its stdout. */
export function git(root, ...args) {
  return execFileSync('git', args, { cwd: root, encoding: 'utf8' });
}
/** Writes `content` to `path` inside `root`, creating parent directories as needed. */
export function write(root, path, content) {
  mkdirSync(join(root, dirname(path)), { recursive: true });
  writeFileSync(join(root, path), content);
}
/**
 * Stages and commits everything in `root`'s worktree, returning the new commit's full SHA.
 * An optional `body` becomes the commit's second `-m`.
 */
export function commit(root, message, body) {
  git(root, 'add', '-A');
  const args = [
    '-c',
    'user.name=t',
    '-c',
    'user.email=t@t.t',
    '-c',
    'commit.gpgsign=false',
    'commit',
    '-q',
    '--no-verify',
    '--allow-empty',
    '-m',
    message,
  ];
  if (body != null) args.push('-m', body);
  git(root, ...args);
  return git(root, 'rev-parse', 'HEAD').trim();
}
/** A standing process rule, with every field a caller might need to override. */
export function entry(over = {}) {
  return {
    id: 'process.sequential',
    kind: 'rule',
    area: 'process',
    standing: true,
    summary: 'Run agents sequentially.',
    body: ['One at a time.'],
    tags: ['dispatch'],
    source: { date: '2026-08-29', by: 'user' },
    ...over,
  };
}
/** Groups entries by their id prefix and writes each group as its own topic file. */
export function writeTopics(root, entries) {
  const byTopic = new Map();
  for (const e of entries) {
    const topic = e.id.split('.')[0];
    if (!byTopic.has(topic)) byTopic.set(topic, []);
    byTopic.get(topic).push(e);
  }
  for (const [topic, topicEntries] of byTopic) {
    write(
      root,
      `knowledge/${topic}.json`,
      JSON.stringify({
        $schema: './schema.json',
        topic,
        title: `${topic} title`,
        entries: topicEntries,
      }),
    );
  }
}
/** A git repo with a schema, areas, the given entries, a CLAUDE.md, and one commit. */
export function makeRepo(entries = [entry()], files = {}) {
  const root = mkdtempSync(join(tmpdir(), 'kb-'));
  git(root, 'init', '-q', '-b', 'main');
  write(root, 'knowledge/schema.json', SCHEMA);
  write(root, 'knowledge/areas.json', JSON.stringify(AREAS));
  write(root, '.claude/schemas/deliverables.json', DELIVERABLES_SCHEMA_CONTENT);
  writeTopics(root, entries);
  write(root, 'CLAUDE.md', '# Test\n');
  for (const [path, content] of Object.entries(files))
    write(root, path, content);
  commit(root, 'chore: init');
  return root;
}

describe('loadBase', () => {
  it('loads topics and indexes entries by id, keeping the first duplicate', () => {
    const root = makeRepo([
      entry(),
      entry({ id: 'rust.a', area: 'rust', standing: false, summary: 'A' }),
    ]);
    const base = loadBase(root);
    expect(base.topics.map((t) => t.name)).toEqual(['process', 'rust']);
    expect(base.entries.get('rust.a').topic).toBe('rust');
    expect(base.entries.get('process.sequential').summary).toBe(
      'Run agents sequentially.',
    );
    expect(base.areas.rust.paths).toEqual(['crates/**', 'Cargo.toml']);
  });
  it('treats a topic with no entries array as having none', () => {
    const root = makeRepo();
    write(
      root,
      'knowledge/rust.json',
      JSON.stringify({ $schema: './schema.json', topic: 'rust', title: 't' }),
    );
    const base = loadBase(root);
    const rust = base.topics.find((t) => t.name === 'rust');
    expect(rust.entries).toBeUndefined();
    expect(base.entries.has('rust.a')).toBe(false);
  });
});

describe('areasFor', () => {
  it('maps paths to areas through the globs, always including global, sorted and deduplicated', () => {
    expect(
      areasFor(
        ['./crates/x/src/a.rs', 'Cargo.toml', 'docs/a.md', 'README.md'],
        AREAS,
      ),
    ).toEqual(['docs', 'global', 'rust']);
    expect(areasFor(['README.md'], AREAS)).toEqual(['global']);
    expect(areasFor(['docs/x.md'], AREAS)).toEqual(['docs', 'global']);
    expect(areasFor([], AREAS)).toEqual(['global']);
  });
  // HR-019: `template/**` must cross the `.claude` dot-segment, using this
  // repository's own real areas.json (the seed's AREAS fixture has no
  // `template` area to match against).
  it('includes template for a path under template/.claude, crossing the dot-segment', () => {
    const root = fileURLToPath(new URL('../', import.meta.url));
    const { areas } = loadBase(root);
    expect(
      areasFor(['template/.claude/agents/implementer.md'], areas),
    ).toContain('template');
  });
  // Review fix round 1 (task 1): the dot-segment matcher must not narrow the
  // rest of matchesGlob's vocabulary -- `?`, bracket classes, and brace
  // lists all still have to match, the same as before HR-019.
  it('still matches ?, bracket-class, and brace-list globs, not only ** and *', () => {
    const VOCAB_AREAS = {
      global: { paths: [] },
      question: { paths: ['crates/?.rs'] },
      bracket: { paths: ['src/*.[jt]s'] },
      brace: { paths: ['src/*.{js,ts}'] },
    };
    expect(areasFor(['crates/x.rs'], VOCAB_AREAS)).toContain('question');
    expect(areasFor(['src/a.ts'], VOCAB_AREAS)).toContain('bracket');
    expect(areasFor(['src/a.ts'], VOCAB_AREAS)).toContain('brace');
  });
});

describe('areaFiles', () => {
  it('groups changed files by every area their globs match, plus global always empty', () => {
    expect(areaFiles(['docs/x.md', 'tools/a.mjs'], AREAS)).toEqual({
      docs: ['docs/x.md'],
      global: [],
      infra: ['tools/a.mjs'],
    });
    expect(areaFiles([], AREAS)).toEqual({ global: [] });
  });
});

describe('checkBase', () => {
  it('passes a valid base', () => {
    const root = makeRepo();
    render(loadBase(root));
    expect(checkBase(loadBase(root))).toEqual([]);
  });
  it('reports schema, id, area, standing, see, verify, and check-shape errors', () => {
    const root = makeRepo([
      entry({ id: 'process.dup', see: ['nope.x'], verify: ['missing.txt'] }),
      entry({ id: 'process.dup', summary: 'x'.repeat(161) }),
      entry({ id: 'process.bad-standing', kind: 'gotcha' }),
      entry({
        id: 'process.bad-check',
        standing: false,
        check: { type: 'grep-absent', level: 'fail', pattern: '(' },
      }),
      entry({
        id: 'process.bad-commits',
        standing: false,
        check: { type: 'commits', level: 'warn' },
      }),
    ]);
    // `writeTopics` files an entry under its own id prefix, so `other.x`
    // would land in its own (matching) `other.json` and never violate the
    // "wrong topic" check below. Splice it into `process.json` directly so
    // the check has something to catch.
    const topic = JSON.parse(
      readFileSync(join(root, 'knowledge/process.json'), 'utf8'),
    );
    topic.entries.unshift(entry({ id: 'other.x' }));
    write(root, 'knowledge/process.json', JSON.stringify(topic));
    const areas = { ...AREAS };
    delete areas.docs; // simulate a missing area in the file only
    write(
      root,
      'knowledge/areas.json',
      JSON.stringify({ ...areas, extra: { paths: [] }, rust: { nope: 1 } }),
    );
    const errors = checkBase(loadBase(root));
    expect(errors).toEqual(
      expect.arrayContaining([
        'knowledge/areas.json: area "docs" is missing',
        'knowledge/areas.json: unknown area "extra"',
        'knowledge/areas.json.rust: missing "paths"',
        'knowledge/areas.json.rust: unknown field "nope"',
        'knowledge/process.json other.x: id must start with "process."',
        'knowledge/process.json process.dup: duplicate id (also in knowledge/process.json)',
        'knowledge/process.json.entries[2].summary: longer than 160 characters',
        'knowledge/process.json process.bad-standing: standing needs kind rule or invariant and area global or process',
        'knowledge/process.json process.dup: see "nope.x" does not exist',
        'knowledge/process.json process.dup: verify path "missing.txt" does not exist',
        'knowledge/process.json process.bad-check: check "grep-absent" needs "files"',
        'knowledge/process.json process.bad-check: check "grep-absent" needs "scope"',
        'knowledge/process.json process.bad-commits: check "commits" needs "subject", "body_absent", or "body_line_max"',
      ]),
    );
    expect(
      errors.some((e) =>
        /process\.bad-check: check pattern is not a valid regex/.test(e),
      ),
    ).toBe(true);
  });
  it('reports a topic whose name differs from its file name', () => {
    const root = makeRepo();
    write(
      root,
      'knowledge/process.json',
      JSON.stringify({
        $schema: './schema.json',
        topic: 'other',
        title: 't',
        entries: [],
      }),
    );
    expect(checkBase(loadBase(root))).toContain(
      'knowledge/process.json: topic "other" must equal the file name "process"',
    );
  });
  it('accepts a commits check that has only body_line_max', () => {
    const root = makeRepo([
      entry({
        check: { type: 'commits', level: 'warn', body_line_max: 80 },
      }),
    ]);
    render(loadBase(root));
    expect(checkBase(loadBase(root))).toEqual([]);
  });
  it('rejects a commits check with body_line_max below 1', () => {
    const root = makeRepo([
      entry({
        check: { type: 'commits', level: 'warn', body_line_max: 0 },
      }),
    ]);
    expect(
      checkBase(loadBase(root)).some((e) =>
        e.endsWith('check.body_line_max: below 1'),
      ),
    ).toBe(true);
  });
  it('exports the constants later tasks render with', () => {
    expect(GENERATED).toBe(
      'Generated from knowledge/ by tools/kb.sh render. Do not edit.',
    );
    expect(BUDGETS).toEqual({
      claudeMdLines: 200,
      claudeMdBytes: 12288,
      standingLines: 60,
      areaLines: 160,
      skillLines: 120,
    });
    expect(STANDING_COMMAND).toBe('tools/kb.sh standing');
    expect(new UsageError('x')).toBeInstanceOf(Error);
  });
  it('skips the unreadable entries the schema already reported', () => {
    const root = makeRepo();
    write(
      root,
      'knowledge/process.json',
      JSON.stringify({
        $schema: './schema.json',
        topic: 'process',
        title: 't',
        entries: [{ kind: 'rule' }, null],
      }),
    );
    const errors = checkBase(loadBase(root));
    expect(errors).toContain('knowledge/process.json.entries[0]: missing "id"');
    expect(errors).toContain(
      'knowledge/process.json.entries[1]: must be object',
    );
  });
  it('does not crash on a topic with no entries array', () => {
    const root = makeRepo();
    write(
      root,
      'knowledge/rust.json',
      JSON.stringify({ $schema: './schema.json', topic: 'rust', title: 't' }),
    );
    expect(checkBase(loadBase(root))).toContain(
      'knowledge/rust.json: missing "entries"',
    );
    // Covers topicLines' false branch: a topic with no entries array counts
    // as zero entries, independent of the schema error checkBase reports.
    expect(topicLines(loadBase(root))).toEqual([
      'process  1  process title',
      'rust  0  t',
    ]);
  });
  it('accepts a see reference to an existing entry and a verify path that exists', () => {
    const root = makeRepo([
      entry({ id: 'process.a' }),
      entry({
        id: 'process.b',
        standing: false,
        see: ['process.a'],
        verify: ['CLAUDE.md'],
      }),
    ]);
    render(loadBase(root));
    expect(checkBase(loadBase(root))).toEqual([]);
  });
  it('ignores a check whose type the schema already rejected, without crashing', () => {
    const root = makeRepo([
      entry({
        id: 'process.a',
        standing: false,
        check: { type: 'unknown-type', level: 'fail' },
      }),
    ]);
    const errors = checkBase(loadBase(root));
    expect(errors.some((e) => e.includes('check "unknown-type"'))).toBe(false);
  });
  it('flags a missing CLAUDE.md', () => {
    const root = makeRepo();
    unlinkSync(join(root, 'CLAUDE.md'));
    expect(checkBase(loadBase(root))).toContain('CLAUDE.md: missing');
  });
  it('accepts a CLAUDE.md with no trailing newline', () => {
    const root = makeRepo(undefined, { 'CLAUDE.md': '# Test' });
    render(loadBase(root));
    expect(checkBase(loadBase(root))).toEqual([]);
  });
  it('flags CLAUDE.md over the line budget', () => {
    const root = makeRepo(undefined, { 'CLAUDE.md': 'x\n'.repeat(201) });
    const errors = checkBase(loadBase(root));
    expect(
      errors.some((e) => /^CLAUDE\.md: \d+ lines, budget 200$/.test(e)),
    ).toBe(true);
  });
  it('flags CLAUDE.md over the byte budget', () => {
    const root = makeRepo(undefined, { 'CLAUDE.md': 'x'.repeat(12289) });
    const errors = checkBase(loadBase(root));
    expect(
      errors.some((e) => /^CLAUDE\.md: \d+ bytes, budget 12288$/.test(e)),
    ).toBe(true);
  });
  it('flags a stray file in .claude/rules and ignores non-markdown files there', () => {
    const root = makeRepo();
    write(root, '.claude/rules/extra.md', '# extra\n');
    write(root, '.claude/rules/notes.txt', 'ignore me\n');
    const errors = checkBase(loadBase(root));
    expect(errors).toContain(
      '.claude/rules/extra.md: not generated by kb; remove it',
    );
    expect(errors.some((e) => e.includes('notes.txt'))).toBe(false);
  });
});

describe('byId', () => {
  it('orders entries by id', () => {
    expect(byId({ id: 'a' }, { id: 'b' })).toBe(-1);
    expect(byId({ id: 'b' }, { id: 'a' })).toBe(1);
    expect(byId({ id: 'a' }, { id: 'a' })).toBe(0);
  });
});

describe('list', () => {
  it('wraps a scalar in an array and leaves an array as-is', () => {
    expect(list('x')).toEqual(['x']);
    expect(list(['x', 'y'])).toEqual(['x', 'y']);
  });
});

function capture() {
  const io = {
    stdout: '',
    stderr: '',
    out: (s) => (io.stdout += s),
    err: (s) => (io.stderr += s),
  };
  return io;
}

const indexRowFixture = (id, kind, area, standing, summary) => ({
  id,
  kind,
  area,
  standing,
  summary,
});

describe('read commands', () => {
  const entries = [
    entry(),
    entry({
      id: 'process.ask',
      kind: 'invariant',
      summary: 'Ask when unsure.',
      tags: ['dispatch', 'users'],
      see: ['process.sequential'],
      verify: ['CLAUDE.md'],
      check: { type: 'commits', level: 'warn', subject: '^x' },
    }),
    entry({
      id: 'rust.clean',
      area: 'rust',
      standing: false,
      kind: 'gotcha',
      summary: 'Clean before retry.',
      body: [],
      source: { date: '2026-08-01', by: 'review', ref: 'TP-226' },
    }),
    entry({
      id: 'rust.history',
      area: 'rust',
      standing: false,
      kind: 'history',
      summary: 'Batch 19 measured 96.01.',
    }),
  ];
  it('topics lists name, count, title', () => {
    expect(cmdTopics(loadBase(makeRepo(entries)))).toEqual([
      { topic: 'process', entries: 2, title: 'process title' },
      { topic: 'rust', entries: 2, title: 'rust title' },
    ]);
  });
  it('index filters by area, topic, tag, kind, standing and sorts by id', () => {
    const base = loadBase(makeRepo(entries));
    const row = indexRowFixture;
    expect(cmdIndex(base, {})).toEqual([
      row('process.ask', 'invariant', 'process', true, 'Ask when unsure.'),
      row(
        'process.sequential',
        'rule',
        'process',
        true,
        'Run agents sequentially.',
      ),
      row('rust.clean', 'gotcha', 'rust', false, 'Clean before retry.'),
      row('rust.history', 'history', 'rust', false, 'Batch 19 measured 96.01.'),
    ]);
    expect(cmdIndex(base, { area: 'rust' })).toEqual([
      row('rust.clean', 'gotcha', 'rust', false, 'Clean before retry.'),
      row('rust.history', 'history', 'rust', false, 'Batch 19 measured 96.01.'),
    ]);
    expect(cmdIndex(base, { topic: 'process', tag: 'users' })).toEqual([
      row('process.ask', 'invariant', 'process', true, 'Ask when unsure.'),
    ]);
    expect(cmdIndex(base, { kind: 'gotcha' })).toEqual([
      row('rust.clean', 'gotcha', 'rust', false, 'Clean before retry.'),
    ]);
    expect(cmdIndex(base, { standing: true })).toEqual([
      row('process.ask', 'invariant', 'process', true, 'Ask when unsure.'),
      row(
        'process.sequential',
        'rule',
        'process',
        true,
        'Run agents sequentially.',
      ),
    ]);
  });
  it('get returns the stored entries plus topic, in the order of the ids given, and rejects unknown ids', () => {
    const base = loadBase(makeRepo(entries));
    // Compares against the fixture entries as authored, not against
    // `base.entries.get(...)` — a test asserting on the same map the
    // command reads from cannot catch a change in what `loadBase` puts
    // into that map.
    expect(cmdGet(base, ['process.ask'])).toEqual([
      { ...entries[1], topic: 'process' },
    ]);
    expect(cmdGet(base, ['rust.clean', 'process.ask'])).toEqual([
      { ...entries[2], topic: 'rust' },
      { ...entries[1], topic: 'process' },
    ]);
    expect(() => cmdGet(base, ['nope.x'])).toThrow(UsageError);
  });
  it('for resolves areas and lists rule, invariant, gotcha entries only', () => {
    const base = loadBase(makeRepo(entries));
    expect(cmdFor(base, ['crates/a/src/x.rs'])).toEqual({
      paths: ['crates/a/src/x.rs'],
      areas: ['global', 'rust'],
      entries: [
        {
          id: 'rust.clean',
          kind: 'gotcha',
          area: 'rust',
          standing: false,
          summary: 'Clean before retry.',
        },
      ],
      standing: STANDING_COMMAND,
    });
    expect(cmdFor(base, ['README.md'])).toEqual({
      paths: ['README.md'],
      areas: ['global'],
      entries: [],
      standing: STANDING_COMMAND,
    });
    expect(cmdFor(base, ['crates/a/src/x.rs'], { full: true })).toEqual({
      paths: ['crates/a/src/x.rs'],
      areas: ['global', 'rust'],
      entries: [{ ...entries[2], topic: 'rust' }],
      standing: STANDING_COMMAND,
    });
  });
  it('for includes procedures and entries whose verify names a path', () => {
    const base = loadBase(
      makeRepo([
        entry({
          id: 'rust.procedure',
          area: 'rust',
          standing: false,
          kind: 'procedure',
          summary: 'Run the sidecar smoke.',
          body: [],
        }),
        entry({
          id: 'docs.verify-only',
          area: 'docs',
          standing: false,
          kind: 'decision',
          summary: 'Keep the crate layout.',
          body: [],
          verify: ['./crates/a/src/x.rs'],
        }),
      ]),
    );
    expect(cmdFor(base, ['crates/a/src/x.rs']).entries).toEqual([
      {
        id: 'docs.verify-only',
        kind: 'decision',
        area: 'docs',
        standing: false,
        summary: 'Keep the crate layout.',
      },
      {
        id: 'rust.procedure',
        kind: 'procedure',
        area: 'rust',
        standing: false,
        summary: 'Run the sidecar smoke.',
      },
    ]);
    expect(cmdFor(base, ['docs/other.md']).entries).toEqual([]);
  });
  it('includes a non-standing global rule in for, for any path', () => {
    const base = loadBase(
      makeRepo([
        entry({
          id: 'global.always',
          area: 'global',
          standing: false,
          summary: 'Applies everywhere.',
        }),
      ]),
    );
    const expected = [
      {
        id: 'global.always',
        kind: 'rule',
        area: 'global',
        standing: false,
        summary: 'Applies everywhere.',
      },
    ];
    expect(cmdFor(base, ['anything/at/all.rs']).entries).toEqual(expected);
    expect(cmdFor(base, ['unrelated/other.txt']).entries).toEqual(expected);
  });
  it('standing lists rules before invariants', () => {
    expect(cmdStanding(loadBase(makeRepo(entries)))).toEqual([
      { id: 'process.sequential', summary: 'Run agents sequentially.' },
      { id: 'process.ask', summary: 'Ask when unsure.' },
    ]);
  });
});

describe('main (read commands)', () => {
  it('dispatches topics, index, get, for, standing and reports usage errors with exit 2', () => {
    const root = makeRepo();
    const base = loadBase(root);
    let io = capture();
    expect(main(['topics'], io, root)).toBe(0);
    expect(JSON.parse(io.stdout)).toEqual(cmdTopics(base));
    io = capture();
    expect(main(['index', '--standing'], io, root)).toBe(0);
    expect(JSON.parse(io.stdout)).toEqual(cmdIndex(base, { standing: true }));
    io = capture();
    expect(main(['get', 'process.sequential'], io, root)).toBe(0);
    expect(JSON.parse(io.stdout)).toEqual(cmdGet(base, ['process.sequential']));
    io = capture();
    expect(main(['get'], io, root)).toBe(2);
    expect(io.stderr).toBe('get needs at least one id\n');
    io = capture();
    expect(main(['for'], io, root)).toBe(2);
    io = capture();
    expect(main(['for', 'docs/x.md', '--full'], io, root)).toBe(0);
    expect(JSON.parse(io.stdout)).toEqual(
      cmdFor(base, ['docs/x.md'], { full: true }),
    );
    io = capture();
    expect(main(['standing'], io, root)).toBe(0);
    expect(JSON.parse(io.stdout)).toEqual(cmdStanding(base));
    io = capture();
    expect(main(['bogus'], io, root)).toBe(2);
    expect(io.stderr).toMatch(/^usage: kb </);
    io = capture();
    expect(main([], io, root)).toBe(2);
  });
  it('reports invalid JSON in a knowledge file as a usage error, not a stack trace', () => {
    const root = makeRepo();
    write(root, 'knowledge/process.json', '{');
    const io = capture();
    expect(main(['topics'], io, root)).toBe(2);
    expect(io.stderr).toMatch(/knowledge\/process\.json: invalid JSON/);
    expect(io.stdout).toBe('');
    expect(io.stderr.split('\n')).toEqual([expect.stringMatching(/./), '']);
  });
  it('lets a missing knowledge file propagate as a real error, not a usage error', () => {
    const root = makeRepo();
    unlinkSync(join(root, 'knowledge/schema.json'));
    expect(() => main(['topics'], capture(), root)).toThrow(/schema\.json/);
    expect(() => main(['topics'], capture(), root)).not.toThrow(UsageError);
  });
});

describe('render', () => {
  const entries = [
    entry(),
    entry({
      id: 'process.ask',
      kind: 'invariant',
      summary: 'Ask when unsure.',
    }),
    entry({
      id: 'rust.clean',
      area: 'rust',
      standing: false,
      kind: 'gotcha',
      summary: 'Clean before retry.',
    }),
    entry({
      id: 'rust.floor',
      area: 'rust',
      standing: false,
      summary: 'Never lower a floor.',
    }),
    entry({
      id: 'rust.old',
      area: 'rust',
      standing: false,
      kind: 'history',
      summary: 'Old.',
    }),
  ];
  it('renders standing rules, one file per area with entries, and the knowledge skill', () => {
    const files = renderAll(loadBase(makeRepo(entries)));
    expect([...files.keys()]).toEqual([
      '.claude/rules/standing-rules.md',
      '.claude/rules/rust.md',
      SKILL_PATH,
    ]);
    expect(files.get('.claude/rules/standing-rules.md')).toBe(
      `${GENERATED}\n\n# Standing rules\n\n- [process.sequential] Run agents sequentially.\n- [process.ask] Ask when unsure.\n`,
    );
    expect(files.get('.claude/rules/rust.md')).toBe(
      `---\npaths:\n  - "crates/**"\n  - "Cargo.toml"\n---\n${GENERATED}\n\n# Rust rules\n\n## Rules\n\n- [rust.floor] Never lower a floor.\n\n## Gotchas\n\n- [rust.clean] Clean before retry.\n\nDetail: tools/kb.sh get <id>\n`,
    );
    const skill = files.get(SKILL_PATH);
    expect(
      skill.startsWith(
        `---\nname: project-knowledge\ndescription: Use when working on this repository as a dispatched subagent, before reading or changing any file\nuser-invocable: false\n---\n${GENERATED}\n\n# Project knowledge\n`,
      ),
    ).toBe(true);
    expect(skill).toContain(
      '## Standing rules\n\n- [process.sequential] Run agents sequentially.\n- [process.ask] Ask when unsure.\n\n## Retrieval protocol\n\n1. Resolve every id under `Knowledge:`',
    );
    expect(skill).toContain(
      "\n3. Write `REPORT_FILE` as a `task-report` (schema `.claude/schemas/deliverables.json`, `self_audit: null`), run `tools/kb.sh validate <REPORT_FILE>`, then `tools/kb.sh audit --base <BASE> --head HEAD --ids <ids, comma-separated> --report <REPORT_FILE>`. Copy the audit `summary` and its `deterministic` rows into `self_audit` — never hand-written rows; the judged rows are the reviewer's. Fix every `fail`, re-run until clean, validate again. List the ids you relied on in `knowledge_used`.\n",
    );
    expect(
      skill.endsWith(
        '## Topics\n\nprocess  2  process title\nrust  3  rust title\n',
      ),
    ).toBe(true);
  });
  it('render writes stale files; --check only lists them', () => {
    const root = makeRepo(entries);
    const base = loadBase(root);
    expect(render(base, { check: true }).toSorted()).toEqual(
      [
        SKILL_PATH,
        '.claude/rules/rust.md',
        '.claude/rules/standing-rules.md',
      ].toSorted(),
    );
    expect(existsSync(join(root, SKILL_PATH))).toBe(false);
    expect(render(base)).toHaveLength(3);
    expect(readFileSync(join(root, '.claude/rules/rust.md'), 'utf8')).toContain(
      '# Rust rules',
    );
    expect(render(base)).toEqual([]);
  });
  it('checkBase reports drift, stray rule files, and budget overruns', () => {
    const root = makeRepo(entries);
    const base = loadBase(root);
    expect(checkBase(base)).toContain(
      '.claude/rules/standing-rules.md: generated file is out of date (run tools/kb.sh render)',
    );
    render(base);
    write(root, '.claude/rules/stray.md', 'x');
    write(root, 'CLAUDE.md', 'x\n'.repeat(201));
    expect(checkBase(base)).toEqual([
      '.claude/rules/stray.md: not generated by kb; remove it',
      'CLAUDE.md: 201 lines, budget 200',
    ]);
    write(root, 'CLAUDE.md', `${'x'.repeat(12300)}\n`);
    expect(checkBase(base)).toContain('CLAUDE.md: 12301 bytes, budget 12288');
  });
  it('checkBase reports a generated file over its line budget', () => {
    const many = Array.from({ length: 61 }, (_, i) =>
      entry({ id: `process.r${String(i).padStart(2, '0')}` }),
    );
    const base = loadBase(makeRepo(many));
    render(base);
    expect(checkBase(base)).toContain(
      '.claude/rules/standing-rules.md: 65 lines, budget 60',
    );
  });
});

describe('main (render, check)', () => {
  it('render --check exits 1 while stale, render writes, check reports and passes', () => {
    const root = makeRepo();
    let io = capture();
    expect(main(['render', '--check'], io, root)).toBe(1);
    expect(io.stderr).toContain(
      '.claude/rules/standing-rules.md: would change\n',
    );
    io = capture();
    expect(main(['check'], io, root)).toBe(1);
    expect(io.stderr).toContain('generated file is out of date');
    io = capture();
    expect(main(['render'], io, root)).toBe(0);
    expect(io.stdout).toContain('.claude/rules/standing-rules.md: written\n');
    io = capture();
    expect(main(['render'], io, root)).toBe(0);
    expect(io.stdout).toBe('render: up to date\n');
    io = capture();
    expect(main(['render', '--check'], io, root)).toBe(0);
    expect(io.stdout).toBe('render: up to date\n');
    io = capture();
    expect(main(['check'], io, root)).toBe(0);
    expect(io.stdout).toBe('knowledge: ok\n');
  });
});

function auditEntries() {
  return [
    entry({
      id: 'process.commits',
      summary: 'Conventional commits, no co-author.',
      check: {
        type: 'commits',
        level: 'fail',
        subject: '^(feat|fix|chore|docs|test): .+',
        body_absent: 'co-authored-by',
        flags: 'i',
      },
    }),
    entry({
      id: 'infra.pins',
      area: 'infra',
      standing: false,
      summary: 'Exact pins.',
      check: {
        type: 'grep-absent',
        level: 'fail',
        files: '**/package.json',
        pattern: '"[\\^~]\\d',
        scope: 'changed',
      },
    }),
    entry({
      id: 'infra.tree',
      area: 'infra',
      standing: false,
      summary: 'No FORBIDDEN word in docs.',
      check: {
        type: 'grep-absent',
        level: 'warn',
        files: 'docs/**/*.md',
        pattern: 'FORBIDDEN',
        scope: 'tree',
      },
    }),
    entry({
      id: 'infra.a19',
      area: 'infra',
      standing: false,
      summary: 'A-19 in the report.',
      check: {
        type: 'report-field',
        level: 'fail',
        if: ['**/package.json', '**/Cargo.toml'],
        field: 'a19',
      },
    }),
    entry({
      id: 'rust.append',
      area: 'rust',
      standing: false,
      summary: 'Migrations append-only.',
      check: {
        type: 'diff-append-only',
        level: 'warn',
        files: 'crates/db/migrations.rs',
      },
    }),
    entry({
      id: 'rust.cochange',
      area: 'rust',
      standing: false,
      summary: 'lib.rs changes join the harness.',
      check: {
        type: 'co-change',
        level: 'fail',
        if: 'crates/lib.rs',
        // oxlint-disable-next-line unicorn/no-thenable -- `then` is the co-change check's schema field name (knowledge/schema.json), not a thenable
        then: 'crates/tests/harness.rs',
      },
    }),
    entry({
      id: 'rust.judged',
      area: 'rust',
      standing: false,
      summary: 'A judged rule.',
    }),
    entry({
      id: 'webview.unrelated',
      area: 'webview',
      standing: false,
      summary: 'Not in the package.',
    }),
    entry({
      id: 'process.proc',
      kind: 'procedure',
      standing: false,
      summary: 'A procedure.',
    }),
  ];
}

/** A report-field check on any package.json -> `dependency_vetting`, shared by the workspace tests. */
function reportFieldEntry() {
  return entry({
    id: 'process.reportws',
    summary: 'Every triggered report carries dependency_vetting.',
    check: {
      type: 'report-field',
      level: 'fail',
      if: '**/package.json',
      field: 'dependency_vetting',
    },
  });
}
/** Writes a temp workspace directory holding one JSON file per `reports` entry (name -> body). */
function writeWorkspace(reports) {
  const dir = mkdtempSync(join(tmpdir(), 'ws-'));
  for (const [name, body] of Object.entries(reports))
    writeFileSync(join(dir, name), JSON.stringify(body));
  return dir;
}

describe('audit', () => {
  it('derives the package from standing rules, touched areas, and ids, and runs every check', () => {
    const root = makeRepo(auditEntries(), {
      'docs/x.md': 'FORBIDDEN\n',
      'crates/db/migrations.rs': 'a\nb\n',
      'crates/lib.rs': 'x\n',
    });
    const base = commit(root, 'chore: base');
    write(root, 'tools/package.json', '{"dependencies":{"x":"^1.0.0"}}\n');
    write(root, 'crates/db/migrations.rs', 'a\n');
    write(root, 'crates/lib.rs', 'y\n');
    commit(root, 'feat: change', 'Co-Authored-By: someone');
    const json = join(root, 'audit.json');
    const { result, failed } = audit(loadBase(root), {
      baseRef: base,
      headRef: 'HEAD',
      ids: ['process.proc'],
      json,
    });
    expect(failed).toBe(true);
    expect(result.rules.map((r) => [r.id, r.mode, r.result])).toEqual([
      ['infra.a19', 'deterministic', 'skipped'],
      ['infra.pins', 'deterministic', 'fail'],
      ['infra.tree', 'deterministic', 'warn'],
      ['process.commits', 'deterministic', 'fail'],
      ['process.proc', 'judged', 'open'],
      ['rust.append', 'deterministic', 'warn'],
      ['rust.cochange', 'deterministic', 'fail'],
      ['rust.judged', 'judged', 'open'],
    ]);
    expect(result.rules.find((r) => r.id === 'infra.pins').evidence).toBe(
      'tools/package.json:1 matches "[\\^~]\\d',
    );
    expect(result.rules.find((r) => r.id === 'infra.tree').evidence).toBe(
      'docs/x.md:1 matches FORBIDDEN',
    );
    expect(result.rules.find((r) => r.id === 'process.commits').evidence).toBe(
      'commit "feat: change" body matches co-authored-by',
    );
    expect(result.rules.find((r) => r.id === 'rust.append').evidence).toBe(
      '1 removed lines in crates/db/migrations.rs',
    );
    expect(result.rules.find((r) => r.id === 'rust.cochange').evidence).toBe(
      'crates/lib.rs changed without crates/tests/harness.rs',
    );
    expect(result.rules.find((r) => r.id === 'infra.a19').evidence).toBe(
      'no --report given',
    );
    expect(result.summary).toEqual({
      base: result.base,
      head: result.head,
      deterministic: 6,
      pass: 0,
      fail: 3,
      warn: 2,
      skipped: 1,
      judged: 2,
    });
    expect(result.summary).not.toHaveProperty('empty_range');
    const data = JSON.parse(readFileSync(json, 'utf8'));
    expect(data).toEqual(result);
    expect(data.ids).toEqual(['process.proc']);
    expect(data.changed_files).toEqual([
      'crates/db/migrations.rs',
      'crates/lib.rs',
      'tools/package.json',
    ]);
    expect(data.areas).toEqual(['global', 'infra', 'rust']);
    expect(data.area_files).toEqual({
      global: [],
      infra: ['tools/package.json'],
      rust: ['crates/db/migrations.rs', 'crates/lib.rs'],
    });
    expect(data.rules).toHaveLength(8);
  });
  // HR-026: a check needs an audit loading path of its own; an area match
  // must admit a checked entry of any kind, not only rule and invariant.
  it('joins a checked procedure entry when its area is touched', () => {
    const root = makeRepo(
      [
        entry({
          id: 'infra.checked-proc',
          kind: 'procedure',
          area: 'infra',
          standing: false,
          summary: 'A checked procedure.',
          check: {
            type: 'report-field',
            level: 'warn',
            if: '**',
            field: 'live_run',
          },
        }),
      ],
      { 'tools/x.txt': 'a\n' },
    );
    const base = commit(root, 'chore: base');
    write(root, 'tools/x.txt', 'b\n');
    commit(root, 'feat: touch infra');
    const rows = audit(loadBase(root), { baseRef: base }).result.rules;
    expect(rows.map((r) => r.id)).toContain('infra.checked-proc');
  });
  it('still excludes an unchecked procedure entry even when its area is touched', () => {
    const root = makeRepo(
      [
        entry({
          id: 'infra.unchecked-proc',
          kind: 'procedure',
          area: 'infra',
          standing: false,
          summary: 'An unchecked procedure.',
        }),
      ],
      { 'tools/x.txt': 'a\n' },
    );
    const base = commit(root, 'chore: base');
    write(root, 'tools/x.txt', 'b\n');
    commit(root, 'feat: touch infra');
    const rows = audit(loadBase(root), { baseRef: base }).result.rules;
    expect(rows.map((r) => r.id)).not.toContain('infra.unchecked-proc');
  });
  // HR-024: an audit whose range holds no commits used to read as clean
  // evidence ('0 commits checked') instead of a vacuous one.
  it('stamps a base==head audit as vacuous', () => {
    const root = makeRepo(auditEntries());
    const base = commit(root, 'chore: base');
    const { result } = audit(loadBase(root), {
      baseRef: base,
      headRef: base,
      ids: ['infra.a19', 'rust.judged'],
    });
    expect(result.summary.empty_range).toBe(true);
    const deterministic = result.rules.filter(
      (r) => r.mode === 'deterministic',
    );
    expect(deterministic.length).toBeGreaterThan(1);
    expect(
      deterministic.every((r) => r.evidence.startsWith('empty range: ')),
    ).toBe(true);
    expect(
      result.rules.find((r) => r.id === 'process.commits').evidence,
    ).toBe('empty range: 0 commits checked');
    expect(result.rules.find((r) => r.id === 'infra.a19').evidence).toBe(
      'empty range: not triggered',
    );
    expect(result.rules.find((r) => r.id === 'rust.judged')).toMatchObject({
      mode: 'judged',
      evidence: '—',
    });
  });
  it('passes a clean range and reports report-field against a JSON report', () => {
    const root = makeRepo(auditEntries(), {
      'crates/tests/harness.rs': 'h\n',
    });
    const base = commit(root, 'chore: base');
    write(root, 'tools/package.json', '{"dependencies":{"x":"1.0.0"}}\n');
    write(root, 'crates/lib.rs', 'y\n');
    write(root, 'crates/tests/harness.rs', 'h2\n');
    write(root, 'crates/db/migrations.rs', 'new\n');
    commit(root, 'feat: clean change');
    const reportA = join(root, 'report-a.json');
    writeFileSync(reportA, JSON.stringify({ a19: null }));
    let { result, failed } = audit(loadBase(root), {
      baseRef: base,
      report: reportA,
    });
    expect(failed).toBe(true);
    expect(result.rules.find((r) => r.id === 'infra.a19').evidence).toBe(
      'report lacks a value for a19 (triggered by tools/package.json)',
    );
    const reportB = join(root, 'report-b.json');
    writeFileSync(
      reportB,
      JSON.stringify({
        a19: { manifests: ['tools/package.json'], dependencies: [] },
      }),
    );
    ({ result, failed } = audit(loadBase(root), {
      baseRef: base,
      report: reportB,
    }));
    expect(failed).toBe(false);
    expect(result.rules.map((r) => [r.id, r.result, r.evidence])).toEqual([
      ['infra.a19', 'pass', 'report field a19 is set'],
      ['infra.pins', 'pass', '1 files checked'],
      ['infra.tree', 'pass', '0 files checked'],
      ['process.commits', 'pass', '1 commits checked'],
      ['rust.append', 'pass', 'crates/db/migrations.rs: no removed lines'],
      [
        'rust.cochange',
        'pass',
        'crates/lib.rs changed with crates/tests/harness.rs',
      ],
      ['rust.judged', 'open', '—'],
    ]);
    expect(result.summary).toEqual({
      base: result.base,
      head: result.head,
      deterministic: 6,
      pass: 6,
      fail: 0,
      warn: 0,
      skipped: 0,
      judged: 1,
    });
  });
  it('throws a UsageError when --report does not hold valid JSON', () => {
    const root = makeRepo(auditEntries());
    const base = commit(root, 'chore: base');
    const bad = join(root, 'report.json');
    writeFileSync(bad, 'not json');
    expect(() => audit(loadBase(root), { baseRef: base, report: bad })).toThrow(
      UsageError,
    );
    expect(() => audit(loadBase(root), { baseRef: base, report: bad })).toThrow(
      /invalid JSON/,
    );
  });
  it('reads a dotted field path and reports warn or pass by its value', () => {
    const root = makeRepo([
      entry({
        id: 'process.dotted',
        summary: 'Self-audit summary is filled.',
        check: {
          type: 'report-field',
          level: 'warn',
          if: '**',
          field: 'self_audit.summary',
        },
      }),
    ]);
    const base = git(root, 'rev-parse', 'HEAD').trim();
    write(root, 'a.txt', 'x\n');
    commit(root, 'feat: change');
    const warnReport = join(root, 'report-warn.json');
    writeFileSync(
      warnReport,
      JSON.stringify({ self_audit: { summary: null } }),
    );
    let rows = audit(loadBase(root), {
      baseRef: base,
      report: warnReport,
    }).result.rules;
    expect(rows[0].result).toBe('warn');
    const passReport = join(root, 'report-pass.json');
    writeFileSync(
      passReport,
      JSON.stringify({ self_audit: { summary: { pass: 1 } } }),
    );
    rows = audit(loadBase(root), { baseRef: base, report: passReport }).result
      .rules;
    expect(rows[0].result).toBe('pass');
  });
  // HR-016: houserules.live-run-recipe (knowledge/houserules.json) carries
  // this exact check shape (`if: '**'`, `field: 'live_run'`) so an implementer
  // report missing a live-run recipe warns instead of a full-text search. The
  // report-field mechanism above already covers the generic case, so this is
  // a disclosed-mutation proof, not a natural RED (process.tdd): with `if`
  // set to a glob that never matches, the missing-report assertion below
  // fails ('pass', not 'warn', because the check never triggers); restoring
  // `if: '**'` makes it pass again.
  it('warns a report-field row when live_run is missing and passes when present, even empty', () => {
    const root = makeRepo([
      entry({
        id: 'houserules.livesample',
        summary: 'Every report carries live_run.',
        check: {
          type: 'report-field',
          level: 'warn',
          if: '**',
          field: 'live_run',
        },
      }),
    ]);
    const base = git(root, 'rev-parse', 'HEAD').trim();
    write(root, 'a.txt', 'x\n');
    commit(root, 'feat: change');
    const missingReport = join(root, 'report-missing.json');
    writeFileSync(missingReport, JSON.stringify({}));
    let rows = audit(loadBase(root), {
      baseRef: base,
      report: missingReport,
    }).result.rules;
    expect(rows[0].result).toBe('warn');
    const emptyReport = join(root, 'report-empty.json');
    writeFileSync(emptyReport, JSON.stringify({ live_run: [] }));
    rows = audit(loadBase(root), { baseRef: base, report: emptyReport }).result
      .rules;
    expect(rows[0].result).toBe('pass');
  });
  // HR-019: a `co-change` `then` glob (matchAny) must cross a dot-segment,
  // the same defect as areaFiles but at the check-runner call site.
  it('matches a co-change then glob across a dot-segment', () => {
    const root = makeRepo(
      [
        entry({
          id: 'process.dotcochange',
          area: 'global',
          standing: false,
          summary: 'trigger.txt co-changes with anything under src/, dot-segments included.',
          check: {
            type: 'co-change',
            level: 'fail',
            if: 'trigger.txt',
            then: 'src/**',
          },
        }),
      ],
      { 'trigger.txt': 'a\n' },
    );
    const base = commit(root, 'chore: base');
    write(root, 'trigger.txt', 'b\n');
    write(root, 'src/.config/x.json', '{}\n');
    commit(root, 'feat: change');
    const rows = audit(loadBase(root), { baseRef: base }).result.rules;
    expect(rows.find((r) => r.id === 'process.dotcochange').evidence).toBe(
      'trigger.txt changed with src/.config/x.json',
    );
  });
  // HR-018: when the only `if` match is the `then` path itself, naming it as
  // both the trigger and the record reads as circular ("record.json changed
  // with record.json"); the evidence names the case plainly instead.
  it('names a record-only co-change satisfied by definition', () => {
    const root = makeRepo(
      [
        entry({
          id: 'process.recordonly',
          area: 'global',
          standing: false,
          summary: 'trigger.txt or record.json co-changes with record.json.',
          check: {
            type: 'co-change',
            level: 'fail',
            if: ['trigger.txt', 'record.json'],
            then: 'record.json',
          },
        }),
      ],
      { 'trigger.txt': 'a\n', 'record.json': '{}\n' },
    );
    const base = commit(root, 'chore: base');
    write(root, 'record.json', '{"n":1}\n');
    commit(root, 'feat: append a run');
    const rows = audit(loadBase(root), { baseRef: base }).result.rules;
    expect(rows.find((r) => r.id === 'process.recordonly').evidence).toBe(
      'only record.json changed; the co-change is satisfied by definition',
    );
  });
  it('names the real trigger, not the record, in a mixed co-change', () => {
    const root = makeRepo(
      [
        entry({
          id: 'process.recordmixed',
          area: 'global',
          standing: false,
          summary: 'trigger.txt or record.json co-changes with record.json.',
          check: {
            type: 'co-change',
            level: 'fail',
            if: ['trigger.txt', 'record.json'],
            then: 'record.json',
          },
        }),
      ],
      { 'trigger.txt': 'a\n', 'record.json': '{}\n' },
    );
    const base = commit(root, 'chore: base');
    write(root, 'trigger.txt', 'b\n');
    write(root, 'record.json', '{"n":1}\n');
    commit(root, 'feat: change trigger and record');
    const rows = audit(loadBase(root), { baseRef: base }).result.rules;
    expect(rows.find((r) => r.id === 'process.recordmixed').evidence).toBe(
      'trigger.txt changed with record.json',
    );
  });
  // Review finding (task 2, round 1): a `then` glob that matches several changed files, with
  // nothing else matching `if`, must not name any of them as the trigger either.
  it('names no then-matching file as the trigger when several then files changed', () => {
    const root = makeRepo(
      [
        entry({
          id: 'process.recordmulti',
          area: 'global',
          standing: false,
          summary: 'Any recs/*.json co-changes with any recs/*.json.',
          check: {
            type: 'co-change',
            level: 'fail',
            if: 'recs/*.json',
            then: 'recs/*.json',
          },
        }),
      ],
      { 'recs/a.json': '{}\n', 'recs/record.json': '{}\n' },
    );
    const base = commit(root, 'chore: base');
    write(root, 'recs/a.json', '{"n":1}\n');
    write(root, 'recs/record.json', '{"n":1}\n');
    commit(root, 'feat: append two runs');
    const rows = audit(loadBase(root), { baseRef: base }).result.rules;
    expect(rows.find((r) => r.id === 'process.recordmulti').evidence).toBe(
      'only recs/a.json, recs/record.json changed; the co-change is satisfied by definition',
    );
  });
  it('reports untriggered checks and a bad subject', () => {
    const root = makeRepo(auditEntries());
    const base = commit(root, 'chore: base');
    write(root, 'docs/new.md', 'hello\n');
    commit(root, 'bad subject');
    const rows = audit(loadBase(root), { baseRef: base }).result.rules;
    expect(rows.map((r) => [r.id, r.result, r.evidence])).toEqual([
      [
        'process.commits',
        'fail',
        'commit "bad subject" does not match ^(feat|fix|chore|docs|test): .+',
      ],
    ]);
    write(root, 'Cargo.toml', '[package]\n');
    write(root, 'tools/Cargo.toml', '[package]\n');
    commit(root, 'chore: cargo');
    const again = audit(loadBase(root), { baseRef: `${base}` }).result.rules;
    expect(again.find((r) => r.id === 'rust.append').evidence).toBe(
      'not triggered',
    );
    expect(again.find((r) => r.id === 'rust.cochange').evidence).toBe(
      'not triggered',
    );
    expect(again.find((r) => r.id === 'infra.a19').result).toBe('skipped');
  });
  it('rejects a missing base, a bad ref, and an unknown id', () => {
    const root = makeRepo(auditEntries());
    expect(() => audit(loadBase(root), {})).toThrow(UsageError);
    expect(() => audit(loadBase(root), { baseRef: 'nope' })).toThrow(
      /bad ref "nope"/,
    );
    expect(() =>
      audit(loadBase(root), { baseRef: 'HEAD', ids: ['x.y'] }),
    ).toThrow(/unknown id "x.y"/);
  });
  // Not in the brief: auditEntries()'s checks always carry both `subject`
  // and `body_absent` on a commits check, and always trigger; these three
  // tests cover the branches that combination never reaches (a commits
  // check missing one field, an untriggered report-field, and a cached
  // second read of an already-checked file).
  it('runs a commits check with only body_absent, and one with only subject', () => {
    const root = makeRepo([
      entry({
        id: 'process.bodyonly',
        summary: 'No co-author.',
        check: {
          type: 'commits',
          level: 'fail',
          body_absent: 'co-authored-by',
          flags: 'i',
        },
      }),
      entry({
        id: 'process.subjectonly',
        summary: 'Conventional subject.',
        check: { type: 'commits', level: 'warn', subject: '^ok: ' },
      }),
    ]);
    const base = commit(root, 'chore: base');
    commit(root, 'ok: real commit', 'No trailer here.');
    const rows = audit(loadBase(root), { baseRef: base }).result.rules;
    expect(rows.find((r) => r.id === 'process.bodyonly').result).toBe('pass');
    expect(rows.find((r) => r.id === 'process.subjectonly').result).toBe(
      'pass',
    );
  });
  it('passes a body_line_max check when the longest body line is exactly the limit', () => {
    const root = makeRepo([
      entry({
        id: 'process.bodylimit',
        summary: 'Wrapped commit bodies.',
        check: { type: 'commits', level: 'fail', body_line_max: 100 },
      }),
    ]);
    const base = commit(root, 'chore: base');
    commit(root, 'feat: at limit', 'x'.repeat(100));
    const rows = audit(loadBase(root), { baseRef: base }).result.rules;
    expect(rows[0]).toMatchObject({
      id: 'process.bodylimit',
      result: 'pass',
      evidence: '1 commits checked',
    });
  });
  it('fails a body_line_max check when a body line is one character over the limit', () => {
    const root = makeRepo([
      entry({
        id: 'process.bodylimit',
        summary: 'Wrapped commit bodies.',
        check: { type: 'commits', level: 'fail', body_line_max: 100 },
      }),
    ]);
    const base = commit(root, 'chore: base');
    commit(root, 'feat: over limit', 'x'.repeat(101));
    const rows = audit(loadBase(root), { baseRef: base }).result.rules;
    expect(rows[0]).toMatchObject({
      id: 'process.bodylimit',
      result: 'fail',
      evidence:
        'commit "feat: over limit" has a body line over 100 characters',
    });
  });
  it('reports a report-field check as not triggered when its trigger files do not change', () => {
    const root = makeRepo([
      entry({
        id: 'process.reportcheck',
        summary: 'Needs a report field.',
        check: {
          type: 'report-field',
          level: 'warn',
          if: '**/package.json',
          field: 'coverage',
        },
      }),
    ]);
    const base = commit(root, 'chore: base');
    commit(root, 'chore: unrelated');
    const rows = audit(loadBase(root), { baseRef: base }).result.rules;
    expect(rows[0]).toMatchObject({
      id: 'process.reportcheck',
      result: 'pass',
      evidence: 'not triggered',
    });
  });
  it('caches a file read across two grep-absent checks on the same path', () => {
    const root = makeRepo([
      entry({
        id: 'process.grepa',
        summary: 'First reader.',
        check: {
          type: 'grep-absent',
          level: 'warn',
          files: 'a.txt',
          pattern: 'nope',
          scope: 'changed',
        },
      }),
      entry({
        id: 'process.grepb',
        summary: 'Second reader, same file.',
        check: {
          type: 'grep-absent',
          level: 'warn',
          files: 'a.txt',
          pattern: 'nope',
          scope: 'changed',
        },
      }),
    ]);
    const base = commit(root, 'chore: base');
    write(root, 'a.txt', 'hello\n');
    commit(root, 'feat: add a');
    const rows = audit(loadBase(root), { baseRef: base }).result.rules;
    expect(rows.every((r) => r.result === 'pass')).toBe(true);
  });
  // Fix round 1 (Task 4 review, Important #2): `body_absent` must match
  // any line of the body, not only an anchored match against the whole
  // body string. Before the fix, an anchored `^` pattern only ever tested
  // the body's first line, so a trailer on a later line — the real shape
  // of a `Co-Authored-By:` footer under an innocent first paragraph —
  // wrongly passed.
  it('matches body_absent against any body line, not only the body start', () => {
    const root = makeRepo([
      entry({
        id: 'process.nocoauthor',
        summary: 'No co-author trailer.',
        check: {
          type: 'commits',
          level: 'fail',
          body_absent: '^co-authored-by',
          flags: 'i',
        },
      }),
    ]);
    const base = commit(root, 'chore: base');
    commit(
      root,
      'feat: change',
      'An innocent first line.\nCo-Authored-By: someone',
    );
    const rows = audit(loadBase(root), { baseRef: base }).result.rules;
    expect(rows[0]).toMatchObject({
      id: 'process.nocoauthor',
      result: 'fail',
      evidence: 'commit "feat: change" body matches ^co-authored-by',
    });
  });
  // Fix round 1 (Task 4 review, hardening): a check's regex is built once
  // and reused with `.test()` across every commit in the loop; a `g`
  // (or `y`) flag would make it stateful via `lastIndex`, silently
  // skipping matches on later commits after the first hit. Without the
  // fix, this test's second commit ("ok: second") would wrongly fail:
  // `lastIndex` left at 2 by the first commit's match means `.test()`
  // starts scanning "ok: second" from its index 2 ("second"), which
  // contains no "ok".
  it('strips a g/y flag so a check regex cannot leak lastIndex across commits', () => {
    const root = makeRepo([
      entry({
        id: 'process.stateful',
        summary: 'Subject mentions ok.',
        check: { type: 'commits', level: 'warn', subject: 'ok', flags: 'g' },
      }),
    ]);
    const base = git(root, 'rev-parse', 'HEAD').trim();
    commit(root, 'ok: first');
    commit(root, 'ok: second');
    const rows = audit(loadBase(root), { baseRef: base }).result.rules;
    expect(rows[0]).toMatchObject({
      id: 'process.stateful',
      result: 'pass',
      evidence: '2 commits checked',
    });
  });
  // HR-009: a two-dot diff between `base` and `head` compares their tips
  // directly, so a file `main` moves after the branch is cut shows up in
  // the branch's own diff. A three-dot diff compares from their merge base
  // to `head`, so main's later change to a file the branch never touches
  // stays out of it.
  it('diffs from the merge base, not a base tip that moved past the branch', () => {
    const root = makeRepo(
      [
        entry({
          id: 'process.mainonly',
          summary: 'main-only.txt stays off the branch diff.',
          check: {
            type: 'grep-absent',
            level: 'fail',
            files: 'main-only.txt',
            pattern: 'from-main',
          },
        }),
      ],
      { 'main-only.txt': 'seed\n' },
    );
    git(root, 'checkout', '-q', '-b', 'topic');
    write(root, 'topic.txt', 'topic\n');
    commit(root, 'feat: topic file');
    git(root, 'checkout', '-q', 'main');
    write(root, 'main-only.txt', 'seed\nfrom-main\n');
    commit(root, 'chore: main-only change');
    const { result } = audit(loadBase(root), {
      baseRef: 'main',
      headRef: 'topic',
    });
    expect(result.changed_files).toEqual(['topic.txt']);
    expect(result.rules[0]).toMatchObject({
      id: 'process.mainonly',
      result: 'pass',
      evidence: '0 files checked',
    });
  });
  // HR-008: `--workspace` judges a `report-field` check against every
  // `task-<n>-report.json` in a workspace directory, instead of the single
  // `--report` file.
  it('fails a workspace report-field check naming the first report lacking the field', () => {
    const root = makeRepo([reportFieldEntry()]);
    const base = commit(root, 'chore: base');
    write(root, 'tools/package.json', '{}\n');
    commit(root, 'feat: add a dependency');
    const dir = writeWorkspace({
      'task-1-report.json': {
        kind: 'task-report',
        files_changed: ['tools/package.json'],
        dependency_vetting: {
          manifests: ['tools/package.json'],
          dependencies: [],
        },
      },
      'task-2-report.json': {
        kind: 'task-report',
        files_changed: ['tools/package.json'],
        dependency_vetting: null,
      },
    });
    const { result, failed } = audit(loadBase(root), {
      baseRef: base,
      workspace: dir,
    });
    expect(failed).toBe(true);
    expect(result.rules[0]).toMatchObject({
      result: 'fail',
      evidence:
        'task-2-report.json lacks a value for dependency_vetting (triggered by tools/package.json)',
    });
  });
  it('passes a workspace report-field check with the hit count when every report has the field', () => {
    const root = makeRepo([reportFieldEntry()]);
    const base = commit(root, 'chore: base');
    write(root, 'tools/package.json', '{}\n');
    commit(root, 'feat: add a dependency');
    const vetted = {
      kind: 'task-report',
      files_changed: ['tools/package.json'],
      dependency_vetting: { manifests: ['tools/package.json'], dependencies: [] },
    };
    const dir = writeWorkspace({
      'task-1-report.json': vetted,
      'task-2-report.json': vetted,
    });
    const { result, failed } = audit(loadBase(root), {
      baseRef: base,
      workspace: dir,
    });
    expect(failed).toBe(false);
    expect(result.rules[0]).toMatchObject({
      result: 'pass',
      evidence: 'report field dependency_vetting is set in 2 reports',
    });
  });
  it('passes a workspace report-field check as not triggered by any report', () => {
    const root = makeRepo([reportFieldEntry()]);
    const base = commit(root, 'chore: base');
    write(root, 'tools/package.json', '{}\n');
    commit(root, 'feat: add a dependency');
    const dir = writeWorkspace({
      'task-1-report.json': { kind: 'task-report', files_changed: ['docs/x.md'] },
    });
    const { result, failed } = audit(loadBase(root), {
      baseRef: base,
      workspace: dir,
    });
    expect(failed).toBe(false);
    expect(result.rules[0]).toMatchObject({
      result: 'pass',
      evidence: 'not triggered by any report',
    });
  });
  // Task 4 review, Important #2: a workspace report is required to carry
  // `files_changed` (`.claude/schemas/deliverables.json`); one that lacks it
  // is malformed, not silently "no hit" — the row fails naming it, so a
  // truncated or hand-written report cannot escape a report-field check.
  it('fails a workspace report-field check naming a report that lacks files_changed', () => {
    const root = makeRepo([reportFieldEntry()]);
    const base = commit(root, 'chore: base');
    write(root, 'tools/package.json', '{}\n');
    commit(root, 'feat: add a dependency');
    const dir = writeWorkspace({
      'task-1-report.json': { kind: 'task-report' },
    });
    const { result, failed } = audit(loadBase(root), {
      baseRef: base,
      workspace: dir,
    });
    expect(failed).toBe(true);
    expect(result.rules[0]).toMatchObject({
      result: 'fail',
      evidence: 'task-1-report.json lacks files_changed',
    });
  });
  // Task 4 review, Minor #3: a missing --workspace directory used to dump a
  // raw Node ENOENT stack from readdirSync; it now reads like every other
  // audit misuse (--report on a missing file already threw UsageError).
  it('rejects a missing --workspace directory as a usage error, not a stack trace', () => {
    const root = makeRepo(auditEntries());
    const base = commit(root, 'chore: base');
    const missing = join(mkdtempSync(join(tmpdir(), 'ws-')), 'missing');
    expect(() =>
      audit(loadBase(root), { baseRef: base, workspace: missing }),
    ).toThrow(UsageError);
    const io = capture();
    expect(main(['audit', '--base', base, '--workspace', missing], io, root)).toBe(
      2,
    );
    expect(io.stderr.split('\n').filter(Boolean)).toHaveLength(1);
  });
  it('rejects --report together with --workspace', () => {
    const root = makeRepo(auditEntries());
    const base = commit(root, 'chore: base');
    const reportPath = join(root, 'report.json');
    writeFileSync(reportPath, JSON.stringify({}));
    const dir = mkdtempSync(join(tmpdir(), 'ws-'));
    expect(() =>
      audit(loadBase(root), {
        baseRef: base,
        report: reportPath,
        workspace: dir,
      }),
    ).toThrow(UsageError);
    expect(() =>
      audit(loadBase(root), {
        baseRef: base,
        report: reportPath,
        workspace: dir,
      }),
    ).toThrow(/audit takes --report or --workspace, not both/);
  });
});

const statsRules = (result) => [
  {
    id: 'a.rule',
    kind: 'rule',
    mode: 'deterministic',
    level: 'fail',
    result,
    evidence: '',
  },
];

describe('stats', () => {
  it('aggregates violations, unused ids, and file counts from a workspace of JSON deliverables', () => {
    const dir = mkdtempSync(join(tmpdir(), 'ws-'));
    writeFileSync(
      join(dir, 'task-1-audit.json'),
      JSON.stringify({ ids: ['a.rule', 'c.d'], rules: statsRules('fail') }),
    );
    writeFileSync(
      join(dir, 'task-2-audit-r1.json'),
      JSON.stringify({ ids: ['a.rule'], rules: statsRules('pass') }),
    );
    writeFileSync(
      join(dir, 'task-1-report.json'),
      JSON.stringify({
        kind: 'task-report',
        knowledge_used: ['a.rule', 'b.c'],
      }),
    );
    // The old markdown-report contract; stats must ignore it entirely.
    writeFileSync(
      join(dir, 'task-1-report.md'),
      '# r\n\nKnowledge used: a.rule, b.c\n',
    );
    writeFileSync(
      join(dir, 'task-2-review.json'),
      JSON.stringify({
        kind: 'task-review',
        rule_adherence: [
          { id: 'x.y', mode: 'judged', result: 'fail', evidence: 'ev' },
          {
            id: 'a.rule',
            mode: 'deterministic',
            result: 'pass',
            evidence: 'ok',
          },
        ],
      }),
    );
    writeFileSync(join(dir, 'unrelated.txt'), '');
    expect(stats(dir)).toEqual({
      violations: [
        { id: 'a.rule', count: 1, tasks: ['1'] },
        { id: 'x.y', count: 1, tasks: ['2'] },
      ],
      unused_ids: [{ id: 'c.d', tasks: ['1'] }],
      audits: { files: 2, tasks: 2 },
      reviews: { files: 1 },
    });
    expect(stats(mkdtempSync(join(tmpdir(), 'ws-')))).toEqual({
      violations: [],
      unused_ids: [],
      audits: { files: 0, tasks: 0 },
      reviews: { files: 0 },
    });
  });
  // Not in the brief: covers the `?? []` fallback for an audit file that
  // carries neither `ids` nor `rules` (a stats file, or a hand-written one).
  it('tolerates an audit file with no ids or rules', () => {
    const dir = mkdtempSync(join(tmpdir(), 'ws-'));
    writeFileSync(join(dir, 'task-9-audit.json'), JSON.stringify({}));
    expect(stats(dir)).toEqual({
      violations: [],
      unused_ids: [],
      audits: { files: 1, tasks: 1 },
      reviews: { files: 0 },
    });
  });
  // Not in the brief: covers the `?? []` fallback for a review with no
  // rule_adherence and a report with no knowledge_used.
  it('tolerates a review with no rule_adherence and a report with no knowledge_used', () => {
    const dir = mkdtempSync(join(tmpdir(), 'ws-'));
    writeFileSync(
      join(dir, 'task-1-review.json'),
      JSON.stringify({ kind: 'task-review' }),
    );
    writeFileSync(
      join(dir, 'task-1-report.json'),
      JSON.stringify({ kind: 'task-report' }),
    );
    expect(stats(dir)).toEqual({
      violations: [],
      unused_ids: [],
      audits: { files: 0, tasks: 0 },
      reviews: { files: 1 },
    });
  });
  it('raises a UsageError naming a malformed deliverable file, instead of crashing', () => {
    const dir = mkdtempSync(join(tmpdir(), 'ws-'));
    writeFileSync(join(dir, 'task-3-audit.json'), '{"ids": [');
    expect(() => stats(dir)).toThrow(UsageError);
    expect(() => stats(dir)).toThrow(/task-3-audit\.json/);
  });
});

describe('main (audit, stats)', () => {
  it('audit exits 1 on a failure, 0 when clean, 2 without --base; stats needs one dir', () => {
    const root = makeRepo(auditEntries());
    const base = commit(root, 'chore: base');
    write(root, 'docs/new.md', 'x\n');
    commit(root, 'docs: fine');
    let io = capture();
    expect(main(['audit', '--base', base], io, root)).toBe(0);
    expect(
      JSON.parse(io.stdout).rules.find((r) => r.id === 'process.commits'),
    ).toMatchObject({ result: 'pass', evidence: '1 commits checked' });
    commit(root, 'nope');
    io = capture();
    expect(main(['audit', '--base', base, '--head', 'HEAD'], io, root)).toBe(1);
    io = capture();
    expect(main(['audit'], io, root)).toBe(2);
    expect(io.stderr).toBe('audit needs --base <ref>\n');
    io = capture();
    expect(main(['stats'], io, root)).toBe(2);
    io = capture();
    expect(main(['stats', mkdtempSync(join(tmpdir(), 'ws-'))], io, root)).toBe(
      0,
    );
    expect(JSON.parse(io.stdout).audits).toEqual({ files: 0, tasks: 0 });
    const badWorkspace = mkdtempSync(join(tmpdir(), 'ws-'));
    writeFileSync(join(badWorkspace, 'task-3-audit.json'), '{"ids": [');
    io = capture();
    expect(main(['stats', badWorkspace], io, root)).toBe(2);
    expect(io.stderr).toContain('task-3-audit.json');
  });
  // Not in the brief: covers main's --ids/--report/--json forwarding
  // branches, which the --base/--head-only test above never exercises.
  it('forwards --ids, --report, and --json from the CLI to audit', () => {
    const root = makeRepo(auditEntries());
    const base = commit(root, 'chore: base');
    const reportPath = join(root, 'report.json');
    writeFileSync(
      reportPath,
      JSON.stringify({ a19: { manifests: [], dependencies: [] } }),
    );
    const jsonPath = join(root, 'out.json');
    const io = capture();
    const code = main(
      [
        'audit',
        '--base',
        base,
        '--ids',
        'process.proc',
        '--report',
        reportPath,
        '--json',
        jsonPath,
      ],
      io,
      root,
    );
    expect(code).toBe(0);
    expect(JSON.parse(readFileSync(jsonPath, 'utf8')).ids).toEqual([
      'process.proc',
    ]);
  });
  // Not in the brief: a dispatch lists its ids comma-and-space separated, so
  // the value pasted into --ids carries spaces.
  it('trims the values in --ids', () => {
    const root = makeRepo(auditEntries());
    const base = commit(root, 'chore: base');
    const jsonPath = join(root, 'out.json');
    const io = capture();
    const code = main(
      [
        'audit',
        '--base',
        base,
        '--ids',
        'process.proc, rust.judged',
        '--json',
        jsonPath,
      ],
      io,
      root,
    );
    expect(code).toBe(0);
    expect(JSON.parse(readFileSync(jsonPath, 'utf8')).ids).toEqual([
      'process.proc',
      'rust.judged',
    ]);
  });
  it('resolves --report and --json against the given cwd, not process.cwd()', () => {
    const root = makeRepo(auditEntries());
    const base = commit(root, 'chore: base');
    const dir = join(root, 'sub');
    mkdirSync(dir);
    writeFileSync(
      join(dir, 'rel.json'),
      JSON.stringify({ a19: { manifests: [], dependencies: [] } }),
    );
    const io = capture();
    const code = main(
      ['audit', '--base', base, '--report', 'rel.json', '--json', 'out.json'],
      io,
      dir,
    );
    expect(code).toBe(0);
    expect(existsSync(join(dir, 'out.json'))).toBe(true);
    expect(JSON.parse(readFileSync(join(dir, 'out.json'), 'utf8')).base).toBe(
      JSON.parse(io.stdout).base,
    );
  });
  // Review fix round 1 (HR-009, Minor #1): `base` and `head` on an orphan
  // branch share no merge base, so the three-dot `git diff` this fix
  // introduced fails with exit 128. Before the fix, that raw failure
  // propagated past `main`'s UsageError-only catch as a stack trace.
  it('reports a usage error, not a stack trace, when base and head share no merge base', () => {
    const root = makeRepo();
    git(root, 'checkout', '-q', '--orphan', 'orphan');
    git(root, 'rm', '-rf', '-q', '.');
    write(root, 'orphan.txt', 'x\n');
    commit(root, 'chore: orphan commit');
    git(root, 'checkout', '-q', 'main');
    const mainSha = git(root, 'rev-parse', '--short', 'main').trim();
    const orphanSha = git(root, 'rev-parse', '--short', 'orphan').trim();
    const io = capture();
    expect(
      main(['audit', '--base', 'main', '--head', 'orphan'], io, root),
    ).toBe(2);
    expect(io.stderr).toBe(
      `no merge base between "${mainSha}" and "${orphanSha}"\n`,
    );
  });
  it('carries git\'s own stderr line for a diff failure that is not a merge-base miss', () => {
    const root = makeRepo();
    expect(() =>
      gitDiff(root, 'main', 'main', ['--name-only', ':(bad']),
    ).toThrow(new UsageError("fatal: Invalid pathspec magic 'bad' in ':(bad'"));
  });
  it('does not mislabel a pathspec failure whose text happens to contain "merge base"', () => {
    const root = makeRepo();
    expect(() =>
      gitDiff(root, 'main', 'main', ['--name-only', ':(bad merge base']),
    ).toThrow(
      new UsageError(
        "fatal: Invalid pathspec magic 'bad merge base' in ':(bad merge base'",
      ),
    );
  });
  it('falls back to the caught error\'s own message when git never runs and leaves no stderr', () => {
    const missingRoot = join(mkdtempSync(join(tmpdir(), 'kb-')), 'missing');
    expect(() =>
      gitDiff(missingRoot, 'main', 'main', ['--name-only']),
    ).toThrow(UsageError);
    expect(() =>
      gitDiff(missingRoot, 'main', 'main', ['--name-only']),
    ).toThrow(/ENOENT/);
  });
  it('forwards --workspace from the CLI to audit', () => {
    const root = makeRepo([reportFieldEntry()]);
    const base = commit(root, 'chore: base');
    write(root, 'tools/package.json', '{}\n');
    commit(root, 'feat: add a dependency');
    const dir = writeWorkspace({
      'task-1-report.json': {
        kind: 'task-report',
        files_changed: ['tools/package.json'],
        dependency_vetting: {
          manifests: ['tools/package.json'],
          dependencies: [],
        },
      },
    });
    const io = capture();
    expect(main(['audit', '--base', base, '--workspace', dir], io, root)).toBe(
      0,
    );
    expect(JSON.parse(io.stdout).rules[0]).toMatchObject({
      result: 'pass',
      evidence: 'report field dependency_vetting is set in 1 reports',
    });
  });
  it('reports a usage error from the CLI when --report and --workspace are both given', () => {
    const root = makeRepo(auditEntries());
    const base = commit(root, 'chore: base');
    const reportPath = join(root, 'report.json');
    writeFileSync(reportPath, JSON.stringify({}));
    const dir = mkdtempSync(join(tmpdir(), 'ws-'));
    const io = capture();
    expect(
      main(
        ['audit', '--base', base, '--report', reportPath, '--workspace', dir],
        io,
        root,
      ),
    ).toBe(2);
    expect(io.stderr).toBe(
      'audit takes --report or --workspace, not both\n',
    );
  });
});

const REPORT = {
  kind: 'task-report',
  task: 1,
  backlog: ['WI-001'],
  status: 'DONE',
  implemented: 'x',
  commits: [{ sha: 'abc1234', subject: 'feat: x' }],
  tests: [{ command: 'vitest', output: 'ok' }],
  live_run: [{ command: 'houserules init --dir scratch', output: 'ok', exit: 0 }],
  tdd: [
    {
      test: 't',
      mode: 'natural',
      red: { command: 'c', output: 'FAIL' },
      green: { command: 'c', output: 'PASS' },
    },
  ],
  files_changed: ['a.mjs'],
  docs_verified: [],
  dependency_vetting: null,
  coverage: null,
  self_audit: null,
  self_review: [],
  concerns: [],
  knowledge_used: ['process.sequential'],
};

// HR-021: verdict.text carries re-review prose (a status sentence, a
// scheduled-elsewhere note) that `open` cannot hold without misreading as
// an unaddressed prior finding (spec T5).
const RE_REVIEW = {
  kind: 're-review',
  task: 1,
  round: 1,
  fix_base: 'abc1234',
  head: 'abc1235',
  finding_verdicts: [
    { finding: 'f', verdict: 'addressed', evidence: 'a.mjs:1' },
  ],
  rule_adherence: [
    { id: 'process.sequential', mode: 'judged', result: 'pass', evidence: 'x' },
  ],
  new_breakage: [],
  out_of_scope: [],
  verdict: { state: 'all-addressed', open: [] },
};

describe('validate', () => {
  it('validates a well-formed task report with no errors', () => {
    const root = makeRepo();
    const file = join(root, 'report.json');
    writeFileSync(file, JSON.stringify(REPORT));
    expect(validateDeliverable(root, file)).toEqual({
      file,
      kind: 'task-report',
      errors: [],
    });
  });
  // HR-016: live_run is required so a missing scratch recipe is a schema
  // error, not a fact buried in prose (spec T3).
  it('rejects a task report without live_run', () => {
    const root = makeRepo();
    const file = join(root, 'report.json');
    const { live_run, ...withoutLiveRun } = REPORT;
    writeFileSync(file, JSON.stringify(withoutLiveRun));
    expect(validateDeliverable(root, file).errors).toEqual([
      `${file}: missing "live_run"`,
    ]);
  });
  it('rejects a live_run that is not an array', () => {
    const root = makeRepo();
    const file = join(root, 'report.json');
    writeFileSync(file, JSON.stringify({ ...REPORT, live_run: 'nope' }));
    expect(validateDeliverable(root, file).errors).toEqual([
      `${file}.live_run: must be array`,
    ]);
  });
  it('rejects a live_run entry without a command', () => {
    const root = makeRepo();
    const file = join(root, 'report.json');
    writeFileSync(
      file,
      JSON.stringify({ ...REPORT, live_run: [{ output: 'ok' }] }),
    );
    expect(validateDeliverable(root, file).errors).toEqual([
      `${file}.live_run[0]: missing "command"`,
    ]);
  });
  // HR-017: mode is required so a tdd cycle names its provenance
  // explicitly (spec T4).
  it('rejects a tdd cycle without mode', () => {
    const root = makeRepo();
    const file = join(root, 'report.json');
    const { mode, ...cycleWithoutMode } = REPORT.tdd[0];
    writeFileSync(
      file,
      JSON.stringify({ ...REPORT, tdd: [cycleWithoutMode] }),
    );
    expect(validateDeliverable(root, file).errors).toEqual([
      `${file}.tdd[0]: missing "mode"`,
    ]);
  });
  it('rejects a tdd cycle with an unknown mode', () => {
    const root = makeRepo();
    const file = join(root, 'report.json');
    writeFileSync(
      file,
      JSON.stringify({
        ...REPORT,
        tdd: [{ ...REPORT.tdd[0], mode: 'guessed' }],
      }),
    );
    expect(validateDeliverable(root, file).errors).toEqual([
      `${file}.tdd[0].mode: must be one of "natural", "mutation", "reconstructed"`,
    ]);
  });
  // HR-024: self_audit.summary is a verbatim copy of the audit tool's own
  // summary output, so the schema must accept the field the tool now
  // stamps on an empty range.
  it('accepts a self_audit summary stamped empty_range: true', () => {
    const root = makeRepo();
    const file = join(root, 'report.json');
    writeFileSync(
      file,
      JSON.stringify({
        ...REPORT,
        self_audit: {
          summary: {
            base: 'abc1234',
            head: 'abc1234',
            deterministic: 1,
            pass: 1,
            fail: 0,
            warn: 0,
            skipped: 0,
            judged: 0,
            empty_range: true,
          },
          rows: [
            {
              id: 'process.sequential',
              mode: 'deterministic',
              result: 'pass',
              evidence: 'empty range: 0 commits checked',
            },
          ],
        },
      }),
    );
    expect(validateDeliverable(root, file).errors).toEqual([]);
  });
  it('rejects a self_audit summary with empty_range: false', () => {
    const root = makeRepo();
    const file = join(root, 'report.json');
    writeFileSync(
      file,
      JSON.stringify({
        ...REPORT,
        self_audit: {
          summary: {
            base: 'abc1234',
            head: 'abc1235',
            deterministic: 1,
            pass: 1,
            fail: 0,
            warn: 0,
            skipped: 0,
            judged: 0,
            empty_range: false,
          },
          rows: [
            {
              id: 'process.sequential',
              mode: 'deterministic',
              result: 'pass',
              evidence: '1 commits checked',
            },
          ],
        },
      }),
    );
    expect(validateDeliverable(root, file).errors).toEqual([
      `${file}.self_audit.summary.empty_range: must be one of true`,
    ]);
  });
  it('reports a bad enum value and an unknown field', () => {
    const root = makeRepo();
    const file = join(root, 'report.json');
    writeFileSync(
      file,
      JSON.stringify({ ...REPORT, status: 'MAYBE', extra: 1 }),
    );
    expect(validateDeliverable(root, file).errors).toEqual([
      `${file}.status: must be one of "DONE", "DONE_WITH_CONCERNS", "BLOCKED", "NEEDS_CONTEXT"`,
      `${file}: unknown field "extra"`,
    ]);
  });
  it('accepts a run whose exit code is an integer', () => {
    const root = makeRepo();
    const file = join(root, 'report.json');
    writeFileSync(
      file,
      JSON.stringify({
        ...REPORT,
        tests: [{ command: 'vitest', output: 'ok', exit: 2 }],
      }),
    );
    expect(validateDeliverable(root, file).errors).toEqual([]);
  });
  it('rejects a run whose exit code is not an integer', () => {
    const root = makeRepo();
    const file = join(root, 'report.json');
    writeFileSync(
      file,
      JSON.stringify({
        ...REPORT,
        tests: [{ command: 'vitest', output: 'ok', exit: '2' }],
      }),
    );
    expect(validateDeliverable(root, file).errors).toEqual([
      `${file}.tests[0].exit: must be integer`,
    ]);
  });
  it('validates a task-review, rejecting a rule_adherence result the schema forbids', () => {
    const root = makeRepo();
    const file = join(root, 'review.json');
    writeFileSync(
      file,
      JSON.stringify({
        kind: 'task-review',
        task: 1,
        base: 'abc1234',
        head: 'abc1235',
        spec_compliance: { verdict: 'compliant', items: [] },
        rule_adherence: [
          { id: 'a.b', mode: 'judged', result: 'open', evidence: 'x' },
        ],
        strengths: [],
        issues: [],
        assessment: { verdict: 'approved', text: 'ok' },
      }),
    );
    const { kind, errors } = validateDeliverable(root, file);
    expect(kind).toBe('task-review');
    expect(errors).toHaveLength(1);
    expect(errors[0]).toMatch(
      /rule_adherence\[0\]\.result: must be one of "pass", "fail", "warn", "skipped"$/,
    );
  });
  // HR-021: text carries the status sentences and scheduled-elsewhere notes
  // that used to misuse `open` (spec T5).
  it('accepts a re-review verdict with text', () => {
    const root = makeRepo();
    const file = join(root, 're-review.json');
    writeFileSync(
      file,
      JSON.stringify({
        ...RE_REVIEW,
        verdict: { ...RE_REVIEW.verdict, text: 'scheduled for task 7' },
      }),
    );
    expect(validateDeliverable(root, file).errors).toEqual([]);
  });
  it('accepts a re-review verdict without text', () => {
    const root = makeRepo();
    const file = join(root, 're-review.json');
    writeFileSync(file, JSON.stringify(RE_REVIEW));
    expect(validateDeliverable(root, file).errors).toEqual([]);
  });
  it('rejects a re-review verdict.text of the wrong type', () => {
    const root = makeRepo();
    const file = join(root, 're-review.json');
    writeFileSync(
      file,
      JSON.stringify({
        ...RE_REVIEW,
        verdict: { ...RE_REVIEW.verdict, text: 42 },
      }),
    );
    expect(validateDeliverable(root, file).errors).toEqual([
      `${file}.verdict.text: must be string`,
    ]);
  });
  it('rejects an unknown kind, a missing file, and invalid JSON as usage errors', () => {
    const root = makeRepo();
    const memo = join(root, 'memo.json');
    writeFileSync(memo, JSON.stringify({ kind: 'memo' }));
    expect(() => validateDeliverable(root, memo)).toThrow(
      /unknown deliverable kind "memo"/,
    );
    expect(() => validateDeliverable(root, join(root, 'missing.json'))).toThrow(
      UsageError,
    );
    const broken = join(root, 'broken.json');
    writeFileSync(broken, '{');
    expect(() => validateDeliverable(root, broken)).toThrow(UsageError);
  });
});

describe('main (validate)', () => {
  it('prints results and exits 1 when any file has errors, 0 when all are valid', () => {
    const root = makeRepo();
    const good = join(root, 'good.json');
    writeFileSync(good, JSON.stringify(REPORT));
    const bad = join(root, 'bad.json');
    writeFileSync(bad, JSON.stringify({ ...REPORT, status: 'MAYBE' }));
    let io = capture();
    expect(main(['validate', good, bad], io, root)).toBe(1);
    expect(JSON.parse(io.stdout)).toEqual([
      { file: good, kind: 'task-report', errors: [] },
      {
        file: bad,
        kind: 'task-report',
        errors: [
          `${bad}.status: must be one of "DONE", "DONE_WITH_CONCERNS", "BLOCKED", "NEEDS_CONTEXT"`,
        ],
      },
    ]);
    io = capture();
    expect(main(['validate', good], io, root)).toBe(0);
    io = capture();
    expect(main(['validate'], io, root)).toBe(2);
    expect(io.stderr).toBe('validate needs at least one file\n');
  });
  it('resolves a relative path against the given cwd, not process.cwd()', () => {
    const root = makeRepo();
    const dir = join(root, 'sub');
    mkdirSync(dir);
    writeFileSync(join(dir, 'rel.json'), JSON.stringify(REPORT));
    const io = capture();
    expect(main(['validate', 'rel.json'], io, dir)).toBe(0);
    expect(JSON.parse(io.stdout)).toEqual([
      { file: join(dir, 'rel.json'), kind: 'task-report', errors: [] },
    ]);
  });
});

describe('the repository knowledge base', () => {
  const TEMPLATE_ROOT = fileURLToPath(new URL('../template', import.meta.url));
  /** A temp git repo seeded with the real `template/knowledge` content and starter CLAUDE.md. */
  function makeSeedRepo() {
    const root = mkdtempSync(join(tmpdir(), 'kb-seed-'));
    git(root, 'init', '-q', '-b', 'main');
    const knowledgeFiles = readdirSync(join(TEMPLATE_ROOT, 'knowledge'))
      .filter((name) => name.endsWith('.json'))
      .toSorted();
    for (const name of knowledgeFiles) {
      write(
        root,
        `knowledge/${name}`,
        readFileSync(join(TEMPLATE_ROOT, 'knowledge', name), 'utf8'),
      );
    }
    write(root, '.claude/schemas/deliverables.json', DELIVERABLES_SCHEMA_CONTENT);
    write(root, 'CLAUDE.md', readFileSync(join(TEMPLATE_ROOT, 'CLAUDE.md'), 'utf8'));
    // Other entries' `verify` paths (process.backlog-drives-work,
    // process.ff-only-merges, process.evals-rerun) name files a real `init`
    // would seed too.
    for (const path of [
      '.claude/evals/record.json',
      'backlog/schema.json',
      '.claude/skills/finishing-a-feature/SKILL.md',
      '.claude/skills/orchestrating/SKILL.md',
    ]) {
      write(root, path, readFileSync(join(TEMPLATE_ROOT, path), 'utf8'));
    }
    commit(root, 'chore: seed');
    return root;
  }

  // Spec §4 T3's third test. This is a regression check over data that is
  // already correct, not new behavior, so it has no natural RED (process.tdd);
  // the next test proves it is not vacuous by breaking the same seed on purpose.
  it('renders and passes checkBase against the real template/knowledge seed', () => {
    const root = makeSeedRepo();
    render(loadBase(root));
    expect(checkBase(loadBase(root))).toEqual([]);
  });
  // Disclosed-mutation proof for the test above: a copy of the seed with
  // process.conventional-commits' new body_line_max set to 0 (the schema's
  // minimum is 1) must fail checkBase, showing the check above is not vacuous.
  it('fails checkBase when the seeded process.json is deliberately broken', () => {
    const root = makeSeedRepo();
    render(loadBase(root));
    const processPath = join(root, 'knowledge/process.json');
    const broken = JSON.parse(readFileSync(processPath, 'utf8'));
    broken.entries.find(
      (e) => e.id === 'process.conventional-commits',
    ).check.body_line_max = 0;
    write(root, 'knowledge/process.json', JSON.stringify(broken));
    expect(
      checkBase(loadBase(root)).some((e) =>
        e.endsWith('check.body_line_max: below 1'),
      ),
    ).toBe(true);
  });
  // A manifest holds glob strings as well as version requirements, so the
  // `exact-pins` pattern has to read a version context, not a bare `*`.
  it('matches a range requirement with the exact-pins pattern, and no glob', () => {
    const base = loadBase(TEMPLATE_ROOT);
    const { check } = base.entries.get('security-hygiene.exact-pins');
    const pattern = new RegExp(check.pattern, check.flags ?? '');
    for (const line of [
      '"x": "^1.0.0"',
      '"x": "~1.0"',
      '"x": ">=1.0.0"',
      '"x": "<2"',
      '"x": "*"',
      '"x": "latest"',
      '"x": "1.x"',
      '"x": "1.2.x"',
      '"^1.2.3"',
      '{"devDependencies":{"x":">=22"}}',
      '{"dependencies": {"pnpm": ">=9"}}',
      '{"dependencies": {"x": "22.x"}}',
    ])
      expect([line, pattern.test(line)]).toEqual([line, true]);
    for (const line of [
      '"files": ["*.md"]',
      '"x": "1.0.0"',
      '"main": "./dist/index.js"',
      '"*.md"',
      '{"engines":{"node":">=22"},"packageManager":"pnpm@10.16.0"}',
      '{"engines": {"node": ">=22", "pnpm": ">=10"}}',
      '{\n  "engines": {\n    "node": ">=22",\n    "pnpm": ">=10"\n  }\n}',
      '{"engines": {"node": "22.x"}}',
    ])
      expect([line, pattern.test(line)]).toEqual([line, false]);
  });
});
