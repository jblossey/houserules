import { mkdirSync, mkdtempSync, readFileSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { execFileSync } from 'node:child_process';
import { describe, expect, it } from 'vitest';
import {
  checkBacklog,
  cmdBatch,
  cmdGet,
  cmdList,
  cmdSet,
  loadBacklog,
  main,
} from '../template/tools/backlog.mjs';
import { UsageError } from '../template/tools/lib/cli.mjs';

const SCHEMA = readFileSync(
  new URL('../template/backlog/schema.json', import.meta.url),
  'utf8',
);
// Derived, never hand-typed: the enum grows as new item types are converted.
const TYPE_ENUM = JSON.parse(SCHEMA)
  .$defs.type.enum.map((value) => JSON.stringify(value))
  .join(', ');

function write(root, path, content) {
  mkdirSync(join(root, dirname(path)), { recursive: true });
  writeFileSync(
    join(root, path),
    typeof content === 'string'
      ? content
      : `${JSON.stringify(content, null, 2)}\n`,
  );
}
function item(over = {}) {
  return {
    id: 'WI-001',
    type: 'feat',
    milestone: 'M0',
    status: 'open',
    title: 'First item.',
    body: ['Body one.', 'Body two.'],
    ...over,
  };
}
// Shared with the `read commands` tests below: those tests assert against
// these literals directly, never against the map `loadBacklog` builds from
// them, so a bug in `loadBacklog` cannot hide behind a fixture that mirrors
// the same bug.
const DEFAULT_ITEMS = [
  item(),
  item({
    id: 'WI-002',
    status: 'done',
    batch: 1,
    title: 'Second item:',
    milestone: null,
    rulings: [
      {
        date: '2026-08-29',
        by: 'user',
        text: 'USER RULING 2026-08-29: do it.',
      },
    ],
    see: ['WI-001', 'A-01'],
  }),
];
const DEFAULT_AMENDMENT = {
  id: 'A-01',
  type: 'constraint',
  status: 'done',
  text: ['Latest stable versions.'],
};
function makeRepo({
  items = DEFAULT_ITEMS,
  batches = [
    {
      number: 1,
      items: ['WI-002'],
      summary: 'WI-002 — second',
      kickoff: 'user 2026-08-01',
      status: { state: 'done', text: 'done — merged' },
    },
  ],
  extra = {},
} = {}) {
  const root = mkdtempSync(join(tmpdir(), 'backlog-'));
  execFileSync('git', ['init', '-q', root]);
  write(root, 'backlog/schema.json', SCHEMA);
  write(root, 'backlog/amendments.json', {
    heading: 'A. Amendments',
    amendments: [DEFAULT_AMENDMENT],
  });
  write(root, 'backlog/items/E01.json', {
    section: 'E01',
    heading: 'E01. Product scope (§1)',
    title: 'Product scope',
    spec: '§1',
    items,
  });
  write(root, 'backlog/batches.json', {
    heading: 'Batch planning',
    intro: [],
    table_header: [
      '| Batch | Items | Kick-off artifact | Status |',
      '|---|---|---|---|',
    ],
    batches,
  });
  write(root, 'backlog/decisions.json', {
    preamble: '# Backlog',
    decisions: [{ date: '2026-08-01', text: 'Markdown only.' }],
    notes: ['A note.'],
  });
  write(root, 'backlog/parked.json', {
    groups: [
      {
        batch: 29,
        intro: 'Batch 29 parked polish',
        items: [{ id: 'PP-29-01', text: 'A parked item.' }],
      },
    ],
  });
  for (const [path, content] of Object.entries(extra))
    write(root, path, content);
  return root;
}
function capture() {
  const io = {
    stdout: '',
    stderr: '',
    out: (s) => (io.stdout += s),
    err: (s) => (io.stderr += s),
  };
  return io;
}

describe('loadBacklog and checkBacklog', () => {
  it('loads every file and indexes items with their section', () => {
    const b = loadBacklog(makeRepo());
    expect(b.items.get('WI-002').section).toBe('E01');
    expect(b.sections.map((s) => s.name)).toEqual(['E01']);
    expect(checkBacklog(b)).toEqual({ errors: [], warnings: [] });
  });
  it('reports schema errors, duplicate ids, dangling references, and batch problems', () => {
    const root = makeRepo({
      items: [
        item(),
        item({ id: 'WI-001', type: 'nope' }),
        item({ id: 'WI-003', see: ['WI-999'] }),
      ],
      batches: [
        {
          number: 2,
          items: ['WI-404'],
          summary: 's',
          kickoff: '',
          status: { state: 'in-progress', text: '' },
        },
        {
          number: 2,
          items: [],
          summary: 's',
          kickoff: '',
          status: { state: 'in-progress', text: '' },
        },
      ],
    });
    const first = checkBacklog(loadBacklog(root));
    expect(first.errors).toEqual([
      `backlog/items/E01.json.items[1].type: must be one of ${TYPE_ENUM}`,
    ]);
    write(root, 'backlog/items/E01.json', {
      section: 'E01',
      heading: 'h',
      title: 't',
      spec: '',
      items: [
        item(),
        item({ id: 'WI-001' }),
        item({ id: 'WI-003', see: ['WI-999'] }),
      ],
    });
    const second = checkBacklog(loadBacklog(root));
    expect(second.errors).toEqual([
      'backlog/items/E01.json WI-001: duplicate id (also in backlog/items/E01.json)',
      'backlog/items/E01.json WI-003: see "WI-999" does not exist',
      'backlog/batches.json batch 2: item "WI-404" does not exist',
      'backlog/batches.json: duplicate batch 2',
      'backlog/batches.json: 2 batches in progress (at most one)',
    ]);
  });
  it('warns about done items without a batch and a section whose name differs from its file', () => {
    const root = makeRepo({ items: [item({ status: 'done' })] });
    expect(checkBacklog(loadBacklog(root)).warnings).toEqual([
      'backlog/items/E01.json WI-001: done without a batch',
    ]);
    write(root, 'backlog/items/E01.json', {
      section: 'E02',
      heading: 'h',
      title: 't',
      spec: '',
      items: [],
    });
    expect(checkBacklog(loadBacklog(root)).errors).toEqual([
      'backlog/items/E01.json: section "E02" must equal the file name "E01"',
    ]);
  });
  // Not in the brief: covers loadBacklog's `Array.isArray(section.items) ? section.items : []`
  // ternary false branch — a section file with no `items` array (the schema
  // would flag this, but loadBacklog itself must not crash reading it).
  it('treats a section with no items array as having none', () => {
    const root = makeRepo();
    write(root, 'backlog/items/E02.json', {
      section: 'E02',
      heading: 'h',
      title: 't',
      spec: '',
    });
    const b = loadBacklog(root);
    const e02 = b.sections.find((s) => s.name === 'E02');
    expect(e02.items).toBeUndefined();
    expect(b.items.has('WI-001')).toBe(true);
  });
});

const listRowFixture = (id, status, milestone, batch, title) => ({
  id,
  status,
  milestone,
  batch,
  title,
});

describe('read commands', () => {
  it('get returns items with section and file, an amendment record, and a parked item with batch; rejects unknown ids', () => {
    const b = loadBacklog(makeRepo());
    // Compares against the fixture literals (DEFAULT_ITEMS, DEFAULT_AMENDMENT)
    // as authored, not against `b.items.get(...)`/`b.amendments...` — a test
    // asserting on the same structures the command reads from cannot catch a
    // change in what `loadBacklog` puts into them.
    expect(cmdGet(b, ['WI-002'])).toEqual([
      { ...DEFAULT_ITEMS[1], section: 'E01', file: 'backlog/items/E01.json' },
    ]);
    expect(cmdGet(b, ['WI-001'])).toEqual([
      { ...DEFAULT_ITEMS[0], section: 'E01', file: 'backlog/items/E01.json' },
    ]);
    expect(cmdGet(b, ['A-01'])).toEqual([DEFAULT_AMENDMENT]);
    expect(cmdGet(b, ['PP-29-01'])).toEqual([
      { id: 'PP-29-01', text: 'A parked item.', batch: 29 },
    ]);
    expect(() => cmdGet(b, ['WI-999'])).toThrow(UsageError);
  });
  it('list filters and returns one row per item, with null for a missing milestone or batch', () => {
    const b = loadBacklog(makeRepo());
    const row = listRowFixture;
    expect(cmdList(b, {})).toEqual([
      row('WI-001', 'open', 'M0', null, 'First item.'),
      row('WI-002', 'done', null, 1, 'Second item:'),
    ]);
    expect(cmdList(b, { open: true })).toEqual([
      row('WI-001', 'open', 'M0', null, 'First item.'),
    ]);
    expect(
      cmdList(b, {
        status: 'done',
        batch: '1',
        section: 'E01',
        type: 'feat',
        milestone: '-',
      }),
    ).toEqual([row('WI-002', 'done', null, 1, 'Second item:')]);
    expect(cmdList(b, { milestone: 'M0' })).toEqual([
      row('WI-001', 'open', 'M0', null, 'First item.'),
    ]);
    expect(cmdList(b, { batch: '9' })).toEqual([]);
  });
  it('batch returns the record and its item rows', () => {
    const b = loadBacklog(makeRepo());
    expect(cmdBatch(b, '1')).toEqual({
      number: 1,
      summary: 'WI-002 — second',
      kickoff: 'user 2026-08-01',
      status: { state: 'done', text: 'done — merged' },
      items: [
        {
          id: 'WI-002',
          status: 'done',
          milestone: null,
          batch: 1,
          title: 'Second item:',
        },
      ],
    });
    expect(() => cmdBatch(b, '7')).toThrow(/unknown batch "7"/);
    expect(() => cmdBatch(b, 'x')).toThrow(UsageError);
  });
});

describe('set', () => {
  it('updates status and batch in the item file, keeping the file formatting', () => {
    const root = makeRepo();
    const b = loadBacklog(root);
    expect(cmdSet(b, 'WI-001', ['status=done', 'batch=3'])).toBe(
      'WI-001: status=done batch=3\n',
    );
    const text = readFileSync(join(root, 'backlog/items/E01.json'), 'utf8');
    expect(text.endsWith('\n')).toBe(true);
    const saved = JSON.parse(text).items[0];
    expect(saved.status).toBe('done');
    expect(saved.batch).toBe(3);
    expect(Object.keys(saved)).toEqual([
      'id',
      'type',
      'milestone',
      'status',
      'title',
      'body',
      'batch',
    ]);
    expect(() => cmdSet(loadBacklog(root), 'WI-001', ['status=nope'])).toThrow(
      /status must be one of/,
    );
    expect(() => cmdSet(loadBacklog(root), 'WI-001', ['batch=x'])).toThrow(
      /batch must be a positive integer/,
    );
    // Not in the brief: covers the `value ?? ''` fallback in the batch check —
    // an assignment with no `=` at all leaves `value` undefined.
    expect(() => cmdSet(loadBacklog(root), 'WI-001', ['batch'])).toThrow(
      /batch must be a positive integer/,
    );
    expect(() => cmdSet(loadBacklog(root), 'WI-001', ['title=x'])).toThrow(
      /unknown field "title"/,
    );
    expect(() => cmdSet(loadBacklog(root), 'WI-001', [])).toThrow(/set needs/);
    expect(() => cmdSet(loadBacklog(root), 'WI-404', ['status=done'])).toThrow(
      /unknown item/,
    );
  });
});

describe('main', () => {
  it('dispatches commands and exit codes', () => {
    const root = makeRepo();
    const b = loadBacklog(root);
    let io = capture();
    expect(main(['list', '--open'], io, root)).toBe(0);
    expect(JSON.parse(io.stdout)).toEqual(cmdList(b, { open: true }));
    io = capture();
    // Not in the brief: covers parseArgs' `--key value` branch (a bare `--flag`
    // is the only shape the rest of this test exercises).
    expect(main(['list', '--status', 'done'], io, root)).toBe(0);
    expect(JSON.parse(io.stdout)).toEqual(cmdList(b, { status: 'done' }));
    io = capture();
    expect(main(['get', 'A-01'], io, root)).toBe(0);
    expect(JSON.parse(io.stdout)).toEqual(cmdGet(b, ['A-01']));
    io = capture();
    expect(main(['get'], io, root)).toBe(2);
    io = capture();
    expect(main(['batch', '1'], io, root)).toBe(0);
    expect(JSON.parse(io.stdout)).toEqual(cmdBatch(b, '1'));
    io = capture();
    expect(main(['batch'], io, root)).toBe(2);
    io = capture();
    expect(main(['set', 'WI-001', 'status=partial'], io, root)).toBe(0);
    expect(io.stdout).toBe('WI-001: status=partial\n');
    io = capture();
    expect(main(['set'], io, root)).toBe(2);
    io = capture();
    expect(main(['check'], io, root)).toBe(0);
    expect(io.stdout).toBe('backlog: ok\n');
    write(root, 'backlog/items/E01.json', {
      section: 'E01',
      heading: 'h',
      title: 't',
      spec: '',
      items: [item({ status: 'done' })],
    });
    // The default batches.json still points batch 1 at WI-002; dropping WI-002
    // from the items file above would otherwise turn this into a dangling-
    // reference error instead of the done-without-a-batch warning under test.
    write(root, 'backlog/batches.json', {
      heading: 'h',
      intro: [],
      table_header: [],
      batches: [],
    });
    io = capture();
    expect(main(['check'], io, root)).toBe(0);
    expect(io.stdout).toBe(
      'warn: backlog/items/E01.json WI-001: done without a batch\nbacklog: ok\n',
    );
    write(root, 'backlog/batches.json', {
      heading: 'h',
      intro: [],
      table_header: [],
      batches: [
        {
          number: 1,
          items: ['WI-404'],
          summary: 's',
          kickoff: '',
          status: { state: 'done', text: '' },
        },
      ],
    });
    io = capture();
    expect(main(['check'], io, root)).toBe(1);
    expect(io.stderr).toBe(
      'backlog/batches.json batch 1: item "WI-404" does not exist\n',
    );
    io = capture();
    expect(main(['bogus'], io, root)).toBe(2);
    expect(io.stderr).toMatch(/^usage: backlog </);
    write(root, 'backlog/parked.json', '{');
    expect(() => main(['check'], capture(), root)).toThrow(/invalid JSON/);
  });
});
