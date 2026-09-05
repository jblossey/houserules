import {
  cpSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  readdirSync,
  rmSync,
  writeFileSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { execFileSync, spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { describe, expect, it, onTestFinished } from 'vitest';
import { byId, list, loadBase } from '../template/tools/kb.mjs';
import { scratchDir } from './scratch-dir.mjs';

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
  const root = scratchDir('kb-');
  git(root, 'init', '-q', '-b', 'main');
  write(root, 'knowledge/schema.json', SCHEMA);
  write(root, 'knowledge/areas.json', JSON.stringify(AREAS));
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

describe('the repository knowledge base', () => {
  const TEMPLATE_ROOT = fileURLToPath(new URL('../template', import.meta.url));
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

describe('the live check command over the frozen corpus', () => {
  const KB_MJS = fileURLToPath(new URL('../template/tools/kb.mjs', import.meta.url));
  const REPO_ROOT = fileURLToPath(new URL('..', import.meta.url));

  /** `tests/corpus/manifest.json`'s `frozen_sha`, read once rather than hardcoded a second time. */
  function frozenSha() {
    return JSON.parse(
      readFileSync(new URL('./corpus/manifest.json', import.meta.url), 'utf8'),
    ).frozen_sha;
  }
  /**
   * Checks out a detached worktree at `sha` under a fresh temp path, runs
   * `fn(worktreeDir)`, then removes it -- `tools/make-corpus.mjs`'s own
   * `withFrozenWorktree`, needed here (not only for `check`'s `root`
   * slice) because `audit`'s package assembly reads whatever is currently
   * on disk under `knowledge/`: running it from this repository's own,
   * ever-growing live tree would pull in standing rules added after the
   * corpus froze, diverging from the frozen expectation as the knowledge
   * base grows (verified live: the two frozen audit slices below fail
   * this way from `REPO_ROOT` once batch 16 added
   * `process.claims-match-artifacts`/`process.evidence-outlives-the-session`).
   * The named git range's own commits are unaffected either way, since a
   * worktree shares its parent's object database.
   */
  function withFrozenWorktree(sha, fn) {
    const dir = mkdtempSync(join(tmpdir(), 'houserules-live-audit-'));
    // Registered immediately after mkdtempSync, before the worktree even
    // exists (houserules.tests-clean-scratch-dirs; fix round 1, issue 3):
    // a failure anywhere below -- the free-the-path rmSync, `git worktree
    // add` itself, or fn(dir) -- must not leave a registration under this
    // repository's own live .git/worktrees/, not just a leaked temp
    // directory. `git worktree remove` is wrapped in its own try/catch
    // because it is a no-op error, not a real failure, when `add` never
    // ran (or itself failed) and there is nothing registered to remove.
    onTestFinished(() => {
      try {
        execFileSync('git', ['worktree', 'remove', '--force', dir], { cwd: REPO_ROOT });
      } catch {
        // No worktree was ever registered at `dir`; rmSync below still runs.
      }
      rmSync(dir, { recursive: true, force: true });
    });
    rmSync(dir, { recursive: true, force: true }); // git worktree add wants the path free
    execFileSync('git', ['worktree', 'add', '--detach', '--quiet', dir, sha], {
      cwd: REPO_ROOT,
    });
    return fn(dir);
  }

  /** Reads one frozen `tests/corpus/check/<slice>.json` capture. */
  function readCorpusCheck(slice) {
    return JSON.parse(
      readFileSync(new URL(`./corpus/check/${slice}.json`, import.meta.url), 'utf8'),
    );
  }
  /** Runs the live `template/tools/kb.mjs check` in `cwd`, without throwing on a non-zero exit. */
  function runKbCheck(cwd) {
    const result = spawnSync(process.execPath, [KB_MJS, 'check'], {
      cwd,
      encoding: 'utf8',
    });
    return { stdout: result.stdout, stderr: result.stderr, exit: result.status };
  }

  // Fix round 1, finding 4 (task-4-review.json): tests/corpus.test.mjs's
  // regeneration test only ever runs kb.mjs from a detached worktree at
  // the frozen sha, never the live template/tools/kb.mjs this tree ships
  // -- so a regression in the still-shipped check path (BUDGETS,
  // checkShape, checkBase's own logic) would pass every existing vitest
  // gate silently. This drives the live copy instead, over the three
  // portable fixture slices (root is this repository's own live tree,
  // exercised the same way `tools/kb.sh check` already gates every
  // commit) and asserts byte parity with the frozen captures the Rust
  // port's corpus tests also pin.
  it.each(['mini', 'mini-bad', 'mini-stale'])('matches the frozen %s slice', (slice) => {
    const root = scratchDir('kb-live-check-');
    cpSync(fileURLToPath(new URL(`./corpus/fixtures/${slice}`, import.meta.url)), root, {
      recursive: true,
    });
    git(root, 'init', '-q', '-b', 'main');
    commit(root, 'chore: init');
    const expected = readCorpusCheck(slice);
    expect(runKbCheck(root)).toEqual({
      stdout: expected.stdout,
      stderr: expected.stderr,
      exit: expected.exit,
    });
  });

  it("matches the frozen root slice's expectation against this repository's own live tree", () => {
    const expected = readCorpusCheck('root');
    expect(runKbCheck(REPO_ROOT)).toEqual({
      stdout: expected.stdout,
      stderr: expected.stderr,
      exit: expected.exit,
    });
  });

  // Batch 17 T3: audit/validate/stats ported to Rust, so their vitest
  // describe blocks left this file per the ratchet pattern
  // (houserules.vitest-coverage-floor-tracks-the-rust-port); this extends
  // the live behavioral gate above to cover them the same way -- the
  // frozen corpus's `<frozen-worktree>`-labeled captures for these three
  // commands reference only two things that never move once committed
  // (the static fixture files under `tests/corpus/fixtures/`, and
  // main-ancestry git commits the audit ranges name), so running today's
  // live `template/tools/kb.mjs` from this repository's own root
  // reproduces them exactly, the same way the `root` check slice above
  // does for `check`.

  /** Reads one frozen `tests/corpus/<relativePath>` capture. */
  function readCorpusCapture(relativePath) {
    return JSON.parse(
      readFileSync(new URL(`./corpus/${relativePath}`, import.meta.url), 'utf8'),
    );
  }
  /** Runs the live `template/tools/kb.mjs` with `args` in `cwd`, without throwing on a non-zero exit. */
  function runKb(args, cwd = REPO_ROOT) {
    const result = spawnSync(process.execPath, [KB_MJS, ...args], { cwd, encoding: 'utf8' });
    return { stdout: result.stdout, stderr: result.stderr, exit: result.status };
  }
  /** Replaces every occurrence of `from` in `text` with `to` -- the inverse of `tools/make-corpus.mjs`'s own `redactPath`, applied to a live run's own output before comparing it against the corpus's already-redacted bytes. */
  function redact(text, from, to) {
    return text.split(from).join(to);
  }

  it.each([
    ['batch14-workspace.json', 'batch14-workspace'],
    ['stats-workspace.json', 'stats-workspace'],
  ])('stats matches the frozen %s slice', (slice, fixtureDir) => {
    const fixtures = fileURLToPath(new URL(`./corpus/fixtures/${fixtureDir}`, import.meta.url));
    const expected = readCorpusCapture(`stats/${slice}`);
    expect(runKb(['stats', fixtures])).toEqual({
      stdout: expected.stdout,
      stderr: expected.stderr,
      exit: expected.exit,
    });
  });

  function assertValidateMatchesCorpus(slice, fixtureDir, files) {
    const fixtures = fileURLToPath(new URL(`./corpus/fixtures/${fixtureDir}`, import.meta.url));
    const placeholder = `<fixtures>/${fixtureDir}`;
    const expected = readCorpusCapture(`validate/${slice}`);
    const result = runKb(['validate', ...files.map((f) => join(fixtures, f))]);
    expect(redact(result.stdout, fixtures, placeholder)).toBe(expected.stdout);
    expect(redact(result.stderr, fixtures, placeholder)).toBe(expected.stderr);
    expect(result.exit).toBe(expected.exit);
  }

  it('validate matches the frozen batch14-workspace slice', () => {
    const fixtures = fileURLToPath(
      new URL('./corpus/fixtures/batch14-workspace', import.meta.url),
    );
    const files = readdirSync(fixtures)
      .filter((name) => name.endsWith('.json'))
      .toSorted();
    assertValidateMatchesCorpus('batch14-workspace.json', 'batch14-workspace', files);
  });

  it('validate matches the frozen task-1-report slice', () => {
    assertValidateMatchesCorpus('task-1-report.json', 'batch14-workspace', [
      'task-1-report.json',
    ]);
  });

  it('validate matches the frozen invalid-deliverable slice', () => {
    assertValidateMatchesCorpus('invalid-deliverable.json', 'invalid-deliverable', [
      'bad-report.json',
    ]);
  });

  it('validate matches the frozen skipped-report slice', () => {
    assertValidateMatchesCorpus('skipped-report.json', 'skipped-report', [
      'skipped-report.json',
    ]);
  });

  it.each([
    [
      'validate-terminal-report.json',
      'a13117540cc1480b00d9b57907d3ad4b02767b1c',
      '1537d89ad000d7376160c30fb06edc604ce4352c',
      'houserules.template-is-the-source,process.tdd,process.deliverables-json,quality.principles,writing-style.doc-comments',
    ],
    [
      'knowledge-retrospective.json',
      'c290a29526aa30c080cc9bfbdd7753b746e6e22d',
      '779300045991aa4349c2b6774c181aec36af7cb7',
      'houserules.template-is-the-source,process.deliverables-json,writing-style.principles,quality.principles,knowledge-base.state-only-the-source',
    ],
  ])('audit matches the frozen %s slice', (slice, base, head, ids) => {
    const expected = readCorpusCapture(`audit/${slice}`);
    withFrozenWorktree(frozenSha(), (worktree) => {
      const result = runKb(['audit', '--base', base, '--head', head, '--ids', ids], worktree);
      expect(JSON.parse(result.stdout)).toEqual(JSON.parse(expected.stdout));
      expect(result.stderr).toBe(expected.stderr);
      expect(result.exit).toBe(expected.exit);
    });
  });

  // Batch 17 T4: the knowledge read commands (topics/index/get/for/
  // standing) ported to Rust, so their vitest describe blocks left this
  // file per the ratchet pattern (houserules.vitest-coverage-floor-tracks-
  // the-rust-port); this extends the live behavioral gate the same way.
  // Unlike stats/validate (fixed, committed fixtures, safe to run against
  // this repository's own ever-growing REPO_ROOT), these five read
  // whatever is currently under knowledge/ -- the same reason `audit`
  // above runs inside a frozen worktree rather than from REPO_ROOT
  // directly, so this does too.
  it.each([
    ['topics', ['topics']],
    ['index', ['index']],
    ['index-standing', ['index', '--standing']],
    ['standing', ['standing']],
    [
      'get-houserules-template-is-the-source',
      ['get', 'houserules.template-is-the-source'],
    ],
    ['for-tools-kb-mjs', ['for', 'tools/kb.mjs']],
    ['for-tools-kb-mjs-full', ['for', 'tools/kb.mjs', '--full']],
  ])('%s matches the frozen knowledge corpus slice', (slice, args) => {
    const expected = readCorpusCapture(`knowledge/${slice}.json`);
    withFrozenWorktree(frozenSha(), (worktree) => {
      expect(runKb(args, worktree)).toEqual({
        stdout: expected.stdout,
        stderr: expected.stderr,
        exit: expected.exit,
      });
    });
  });
});
