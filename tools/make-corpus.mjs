#!/usr/bin/env node
// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Jannis Blossey
// Freezes the JS kb/backlog CLIs' observable behavior into the fixture
// corpus under tests/corpus/, for the Tier-2 (Rust) port's parity gates.
// Dev-only: not shipped in template/ or the payload. See HR-054.
import { execFileSync, spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
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
import { dirname, join, resolve } from 'node:path';
import { isMainModule } from './lib/cli.mjs';
import { repoRoot } from './lib/json-store.mjs';

/**
 * The commit the corpus is frozen against: the parent of the batch 16 plan
 * commit (3c4065f), i.e. the batch 16 kickoff commit a73a8c6. A detached
 * worktree at this sha supplies both the kb/backlog implementation and the
 * repository-based inputs (knowledge, backlog, git history) for every
 * repository-reading slice, so later edits to any of those never move the
 * corpus. Full 40-hex form: short shas can collide as the repository grows.
 */
export const FROZEN_SHA = 'a73a8c6b1c511217ceafa0bdaf6df8acdaaa1b71';

/**
 * `--base`/`--head`/`--ids` inputs for the two frozen `audit` slices. Both
 * ranges are real commits on `main`'s own ancestry, not the archived batch-14
 * workspace's base/head values: those name pre-aggregation batch-14 commits
 * the fast-forward merge discarded, unreachable from any ref (batch 16 task 1
 * review, finding 1). Checked with `git merge-base --is-ancestor <sha>
 * origin/main`, run against all four shas below inside a fresh `git clone
 * --no-local` of this repository: every one printed exit 0, so a fresh clone
 * (and CI's `fetch-depth: 0` checkout) resolves them.
 */
const AUDIT_RANGES = [
  {
    file: 'validate-terminal-report.json',
    base: 'a13117540cc1480b00d9b57907d3ad4b02767b1c',
    head: '1537d89ad000d7376160c30fb06edc604ce4352c',
    ids: [
      'houserules.template-is-the-source',
      'process.tdd',
      'process.deliverables-json',
      'quality.principles',
      'writing-style.doc-comments',
    ],
    expectExit: 1,
  },
  {
    file: 'knowledge-retrospective.json',
    base: 'c290a29526aa30c080cc9bfbdd7753b746e6e22d',
    head: '779300045991aa4349c2b6774c181aec36af7cb7',
    ids: [
      'houserules.template-is-the-source',
      'process.deliverables-json',
      'writing-style.principles',
      'quality.principles',
      'knowledge-base.state-only-the-source',
    ],
    expectExit: 0,
  },
];

/** `backlog.mjs` invocations frozen under `tests/corpus/backlog/`. */
const BACKLOG_RUNS = [
  { file: 'list-open.json', args: ['list', '--open'] },
  { file: 'get-hr-052.json', args: ['get', 'HR-052'] },
  { file: 'batch-14.json', args: ['batch', '14'] },
  { file: 'check.json', args: ['check'] },
];

const SKILL_PATH = '.claude/skills/project-knowledge/SKILL.md';
const WORKTREE_LABEL = '<frozen-worktree>';
const FIXTURES_DIR = 'tests/corpus/fixtures';

/** Runs `node <args>` in `cwd`, returning captured output without throwing on a non-zero exit. */
function runNode(args, cwd) {
  const result = spawnSync('node', args, { cwd, encoding: 'utf8' });
  if (result.error) throw result.error;
  return { stdout: result.stdout, stderr: result.stderr, exit: result.status };
}

/** The `{ command, cwd, stdout, stderr, exit }` record frozen for one CLI invocation. */
function capture(displayCommand, cwdLabel, result) {
  return {
    command: displayCommand,
    cwd: cwdLabel,
    stdout: result.stdout,
    stderr: result.stderr,
    exit: result.exit,
  };
}

/**
 * Replaces every occurrence of `absolutePath` in `text` with `placeholder`.
 * `kb.mjs validate` echoes back the resolved absolute path of every file it
 * validates; that path is the caller's own filesystem layout, not part of
 * the frozen behavior, so it cannot appear verbatim in a byte-for-byte
 * corpus that regenerates identically on every machine and in CI.
 */
function redactPath(text, absolutePath, placeholder) {
  return text.split(absolutePath).join(placeholder);
}

function assertExit(label, result, expected) {
  if (result.exit !== expected) {
    throw new Error(
      `${label}: expected exit ${expected}, got ${result.exit}\nstdout: ${result.stdout}\nstderr: ${result.stderr}`,
    );
  }
}

/** Creates a detached worktree at `FROZEN_SHA` under a temp dir, runs `fn(worktreeDir)`, then removes it. */
function withFrozenWorktree(root, fn) {
  const dir = mkdtempSync(join(tmpdir(), 'houserules-corpus-worktree-'));
  rmSync(dir, { recursive: true, force: true }); // git worktree add wants the path free
  execFileSync(
    'git',
    ['worktree', 'add', '--detach', '--quiet', dir, FROZEN_SHA],
    { cwd: root },
  );
  try {
    return fn(dir);
  } finally {
    execFileSync('git', ['worktree', 'remove', '--force', dir], { cwd: root });
    rmSync(dir, { recursive: true, force: true });
  }
}

/** Copies `sourceDir` into a fresh temp directory and `git init`s it, so `repoRoot` resolves there. */
function withFixtureCopy(sourceDir, fn) {
  const dir = mkdtempSync(join(tmpdir(), 'houserules-corpus-fixture-'));
  cpSync(sourceDir, dir, { recursive: true });
  execFileSync('git', ['init', '--quiet', dir]);
  try {
    return fn(dir);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}

/** Every `.claude/rules/*.md` file plus the knowledge skill, present under `root` after a render. */
function renderedPaths(root) {
  const rulesDir = join(root, '.claude/rules');
  const ruleFiles = readdirSync(rulesDir)
    .filter((name) => name.endsWith('.md'))
    .toSorted()
    .map((name) => `.claude/rules/${name}`);
  return [...ruleFiles, SKILL_PATH];
}

/**
 * Top-level `outDir` entries this generator owns and rebuilds on every run.
 * `fixtures/` is deliberately excluded: outDir defaults to this repo's own
 * `tests/corpus/`, the same directory the fixtures are read from, so wiping
 * the whole tree would delete the committed inputs before they get copied.
 */
const OWNED_ENTRIES = ['render', 'check', 'audit', 'validate', 'backlog', 'manifest.json'];

/**
 * Builds the frozen fixture corpus into `outDir` and returns the sorted
 * repo-relative paths written, `manifest.json` included. `root` is the
 * repository whose git history and `fixtures/` directory supply the frozen
 * worktree and the synthetic inputs; it defaults to this checkout's own
 * root, which is what both the regeneration test and CI use.
 */
export function generateCorpus({ outDir, root = repoRoot() } = {}) {
  if (!outDir) throw new Error('generateCorpus needs outDir');
  mkdirSync(outDir, { recursive: true });
  for (const entry of OWNED_ENTRIES)
    rmSync(join(outDir, entry), { recursive: true, force: true });

  const runs = [];
  const normalizations = [];
  function write(relPath, content) {
    const abs = join(outDir, relPath);
    mkdirSync(dirname(abs), { recursive: true });
    writeFileSync(abs, content);
  }
  function writeJson(relPath, value) {
    write(relPath, `${JSON.stringify(value, null, 2)}\n`);
  }
  /**
   * Runs one CLI invocation, freezes its `{ command, cwd, stdout, stderr,
   * exit }` record at `outPath`, and records the run in `manifest.runs`.
   * `redact` replaces one absolute filesystem path with a bracketed
   * placeholder in the captured stdout/stderr before freezing it (see
   * `redactPath`) and appends a matching entry to `manifest.normalizations`,
   * so the substitution is visible in the frozen bytes and self-described in
   * the manifest, not only in this function's source. `expectExit` throws
   * if the run's exit code differs, catching a drift in the frozen sha's
   * behavior at generation time instead of silently baking a wrong artifact
   * into the corpus.
   */
  function runAndFreeze({ execArgs, cwd, displayCommand, cwdLabel, outPath, expectExit, redact }) {
    const result = runNode(execArgs, cwd);
    if (expectExit !== undefined) assertExit(displayCommand, result, expectExit);
    const normalized = redact
      ? {
          stdout: redactPath(result.stdout, redact.from, redact.to),
          stderr: redactPath(result.stderr, redact.from, redact.to),
          exit: result.exit,
        }
      : result;
    writeJson(outPath, capture(displayCommand, cwdLabel, normalized));
    runs.push({ command: displayCommand, cwd: cwdLabel, produces: [outPath] });
    if (redact) {
      normalizations.push({
        slice: outPath,
        field: 'stdout/stderr: each result\'s "file"',
        substituted: redact.to,
        reason: redact.reason,
      });
    }
  }
  /** Runs `kb.mjs render` in `cwd`, freezes every produced file under `outPrefix/`, and records the run. */
  function renderAndFreeze({ execArgs, cwd, displayCommand, cwdLabel, outPrefix }) {
    const result = runNode(execArgs, cwd);
    assertExit(displayCommand, result, 0);
    const paths = renderedPaths(cwd);
    for (const path of paths) write(`${outPrefix}/${path}`, readFileSync(join(cwd, path)));
    runs.push({
      command: displayCommand,
      cwd: cwdLabel,
      produces: paths.map((path) => `${outPrefix}/${path}`),
    });
  }

  const fixturesMini = join(root, FIXTURES_DIR, 'mini');
  const fixturesMiniBad = join(root, FIXTURES_DIR, 'mini-bad');
  const fixturesMiniStale = join(root, FIXTURES_DIR, 'mini-stale');
  const fixturesBatch14 = join(root, FIXTURES_DIR, 'batch14-workspace');
  const fixturesInvalid = join(root, FIXTURES_DIR, 'invalid-deliverable');

  withFrozenWorktree(root, (worktree) => {
    // render/root: every file renderAll produces for the frozen repository.
    renderAndFreeze({
      execArgs: ['tools/kb.mjs', 'render'],
      cwd: worktree,
      displayCommand: 'node tools/kb.mjs render',
      cwdLabel: WORKTREE_LABEL,
      outPrefix: 'render/root',
    });

    // check/root: expected ok against the frozen repository base.
    runAndFreeze({
      execArgs: ['tools/kb.mjs', 'check'],
      cwd: worktree,
      displayCommand: 'node tools/kb.mjs check',
      cwdLabel: WORKTREE_LABEL,
      outPath: 'check/root.json',
      expectExit: 0,
    });

    // audit/: two ranges on main's own ancestry, re-run against the frozen kb.mjs.
    for (const range of AUDIT_RANGES) {
      const execArgs = [
        'tools/kb.mjs',
        'audit',
        '--base',
        range.base,
        '--head',
        range.head,
        '--ids',
        range.ids.join(','),
      ];
      runAndFreeze({
        execArgs,
        cwd: worktree,
        displayCommand: `node ${execArgs.join(' ')}`,
        cwdLabel: WORKTREE_LABEL,
        outPath: `audit/${range.file}`,
        expectExit: range.expectExit,
      });
    }

    // backlog/: verbatim output and exit for four frozen backlog.mjs invocations.
    for (const { file, args } of BACKLOG_RUNS) {
      const execArgs = ['tools/backlog.mjs', ...args];
      runAndFreeze({
        execArgs,
        cwd: worktree,
        displayCommand: `node ${execArgs.join(' ')}`,
        cwdLabel: WORKTREE_LABEL,
        outPath: `backlog/${file}`,
        expectExit: 0,
      });
    }

    // validate/: the frozen kb.mjs validate over the committed batch-14 deliverables,
    // plus one invalid fixture so the slice also freezes a failing verdict.
    const redactReason =
      "kb.mjs validate echoes the caller's resolved absolute path into each result's " +
      "\"file\" field; the placeholder keeps the corpus byte-identical across machines and CI.";
    const batch14Placeholder = '<fixtures>/batch14-workspace';
    const batch14Files = readdirSync(fixturesBatch14)
      .filter((name) => name.endsWith('.json'))
      .toSorted();
    runAndFreeze({
      execArgs: [
        'tools/kb.mjs',
        'validate',
        ...batch14Files.map((name) => join(fixturesBatch14, name)),
      ],
      cwd: worktree,
      displayCommand: `node tools/kb.mjs validate ${batch14Files.map((n) => `${batch14Placeholder}/${n}`).join(' ')}`,
      cwdLabel: WORKTREE_LABEL,
      outPath: 'validate/batch14-workspace.json',
      expectExit: 0,
      redact: { from: fixturesBatch14, to: batch14Placeholder, reason: redactReason },
    });

    runAndFreeze({
      execArgs: ['tools/kb.mjs', 'validate', join(fixturesBatch14, 'task-1-report.json')],
      cwd: worktree,
      displayCommand: `node tools/kb.mjs validate ${batch14Placeholder}/task-1-report.json`,
      cwdLabel: WORKTREE_LABEL,
      outPath: 'validate/task-1-report.json',
      expectExit: 0,
      redact: { from: fixturesBatch14, to: batch14Placeholder, reason: redactReason },
    });

    const invalidPlaceholder = '<fixtures>/invalid-deliverable';
    runAndFreeze({
      execArgs: ['tools/kb.mjs', 'validate', join(fixturesInvalid, 'bad-report.json')],
      cwd: worktree,
      displayCommand: `node tools/kb.mjs validate ${invalidPlaceholder}/bad-report.json`,
      cwdLabel: WORKTREE_LABEL,
      outPath: 'validate/invalid-deliverable.json',
      expectExit: 1,
      redact: { from: fixturesInvalid, to: invalidPlaceholder, reason: redactReason },
    });

    // render/mini and check/{mini,mini-bad}: synthetic fixtures, frozen kb.mjs.
    const kbInWorktree = join(worktree, 'tools/kb.mjs');
    const kbDisplay = `node ${WORKTREE_LABEL}/tools/kb.mjs`;
    const miniLabel = `${FIXTURES_DIR}/mini`;
    const miniBadLabel = `${FIXTURES_DIR}/mini-bad`;

    withFixtureCopy(fixturesMini, (miniCopy) => {
      renderAndFreeze({
        execArgs: [kbInWorktree, 'render'],
        cwd: miniCopy,
        displayCommand: `${kbDisplay} render`,
        cwdLabel: miniLabel,
        outPrefix: 'render/mini',
      });

      runAndFreeze({
        execArgs: [kbInWorktree, 'check'],
        cwd: miniCopy,
        displayCommand: `${kbDisplay} check`,
        cwdLabel: miniLabel,
        outPath: 'check/mini.json',
        expectExit: 0,
      });
    });

    withFixtureCopy(fixturesMiniBad, (miniBadCopy) => {
      runAndFreeze({
        execArgs: [kbInWorktree, 'check'],
        cwd: miniBadCopy,
        displayCommand: `${kbDisplay} check`,
        cwdLabel: miniBadLabel,
        outPath: 'check/mini-bad.json',
        expectExit: 1,
      });
    });

    // mini-stale is schema-valid, so it passes checkBase's early return and
    // reaches the five checks mini-bad's seeded violations never get to: a
    // stale generated file, a stray rules file, a missing rendered file, and
    // both budget shapes (lines and bytes).
    const miniStaleLabel = `${FIXTURES_DIR}/mini-stale`;
    withFixtureCopy(fixturesMiniStale, (miniStaleCopy) => {
      runAndFreeze({
        execArgs: [kbInWorktree, 'check'],
        cwd: miniStaleCopy,
        displayCommand: `${kbDisplay} check`,
        cwdLabel: miniStaleLabel,
        outPath: 'check/mini-stale.json',
        expectExit: 1,
      });
    });
  });

  // fixtures/: the static, committed inputs, copied into the corpus verbatim
  // so the corpus alone documents what produced every derived artifact.
  // batch14-workspace's policy exception (committed copies of gitignored
  // .superpowers/ deliverables) is recorded at houserules.corpus-batch14-fixtures-are-committed.
  for (const [label, dir] of [
    ['mini', fixturesMini],
    ['mini-bad', fixturesMiniBad],
    ['mini-stale', fixturesMiniStale],
    ['batch14-workspace', fixturesBatch14],
    ['invalid-deliverable', fixturesInvalid],
  ]) {
    for (const relPath of listFilesRecursive(dir)) {
      write(`fixtures/${label}/${relPath}`, readFileSync(join(dir, relPath)));
    }
  }

  const inventory = {};
  for (const relPath of listFilesRecursive(outDir)) {
    inventory[relPath] = sha256(readFileSync(join(outDir, relPath)));
  }
  const manifest = {
    frozen_sha: FROZEN_SHA,
    node_version: process.version,
    generated_by: 'tools/make-corpus.mjs',
    runs: runs.toSorted((a, b) => (a.produces[0] < b.produces[0] ? -1 : 1)),
    normalizations: normalizations.toSorted((a, b) => (a.slice < b.slice ? -1 : 1)),
    inventory,
  };
  writeJson('manifest.json', manifest);

  return { outDir, files: listFilesRecursive(outDir), manifest };
}

/** Relative (POSIX-separated) file paths under `dir`, recursed, sorted. */
export function listFilesRecursive(dir, prefix = '') {
  const paths = [];
  for (const entry of readdirSync(dir, { withFileTypes: true }).toSorted((a, b) =>
    a.name < b.name ? -1 : 1,
  )) {
    const relPath = prefix ? `${prefix}/${entry.name}` : entry.name;
    if (entry.isDirectory()) paths.push(...listFilesRecursive(join(dir, entry.name), relPath));
    else paths.push(relPath);
  }
  return paths;
}

/** Hex SHA-256 digest of `data` (a `Buffer` or string). */
export function sha256(data) {
  return createHash('sha256').update(data).digest('hex');
}

if (isMainModule(import.meta.url)) {
  const outArg = process.argv[2];
  const outDir = outArg
    ? resolve(process.cwd(), outArg)
    : join(repoRoot(), 'tests/corpus');
  const { files } = generateCorpus({ outDir });
  process.stdout.write(`wrote ${files.length} files to ${outDir}\n`);
}
