import { cpSync, readFileSync, rmSync } from 'node:fs';
import { join } from 'node:path';
import { execFileSync, spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { describe, expect, it, onTestFinished } from 'vitest';
import { scratchDir } from './scratch-dir.mjs';

// tools/backlog.mjs's CLI surface (loadBacklog, checkBacklog, cmdGet,
// cmdList, cmdBatch, cmdSet, main) ported to crates/houserules/src/backlog/
// (batch 17 T2, docs/specs/2026-09-04-batch-15-tier2-spec.md §5 phase 2):
// crates/houserules/src/backlog/{load,commands,cli}.rs carry the ported
// logic and crates/houserules/tests/backlog_parity.rs the ported test
// cases, byte-compared against the compiled binary. This file keeps the
// one behavioral gate the still-shipped `template/tools/backlog.mjs` needs
// per houserules.vitest-coverage-floor-tracks-the-rust-port: a live
// regression check driving the SHIPPED JS itself, not only the
// corpus-regeneration self-consistency check (tests/corpus.test.mjs),
// which would not catch a regression in backlog.mjs's own logic. Coverage
// for this file moves to its own ratchet run (vitest.backlog-coverage.
// config.mts), pinned at the numbers measured once these cases left.

const BACKLOG_MJS = fileURLToPath(new URL('../template/tools/backlog.mjs', import.meta.url));
const REPO_ROOT = fileURLToPath(new URL('..', import.meta.url));

/** Reads one frozen `tests/corpus/backlog/<relative>` capture. */
function readCorpusRun(relative) {
  return JSON.parse(readFileSync(new URL(`./corpus/backlog/${relative}`, import.meta.url), 'utf8'));
}

/** This checkout's frozen corpus sha (`tests/corpus/manifest.json`), read rather than duplicated. */
function frozenSha() {
  return JSON.parse(readFileSync(new URL('./corpus/manifest.json', import.meta.url), 'utf8'))
    .frozen_sha;
}

/** Runs the live `template/tools/backlog.mjs <args>` in `cwd`, without throwing on a non-zero exit. */
function runBacklog(args, cwd) {
  const result = spawnSync(process.execPath, [BACKLOG_MJS, ...args], {
    cwd,
    encoding: 'utf8',
  });
  return { stdout: result.stdout, stderr: result.stderr, exit: result.status };
}

/**
 * Copies the `mini` fixture into a fresh scratch directory and `git init`s
 * it, then runs `fn(root)` there. `backlog.mjs`'s `repoRoot` resolves via
 * `git rev-parse --show-toplevel`, which climbs to the *nearest* enclosing
 * `.git` -- the committed fixture under `tests/corpus/fixtures/mini` has
 * none of its own, so running against that path directly would silently
 * resolve to this repository's own root instead (verified live: doing so
 * loads and checks the real `backlog/`, not the fixture's, and happens to
 * pass only because the real backlog is also clean). A private copy with
 * its own `.git` is the only way to exercise the fixture's own data.
 */
function withMiniFixtureRepo(fn) {
  const root = scratchDir('backlog-live-check-');
  cpSync(fileURLToPath(new URL('./corpus/fixtures/mini', import.meta.url)), root, {
    recursive: true,
  });
  execFileSync('git', ['init', '--quiet', root]);
  return fn(root);
}

/**
 * Creates a detached git worktree at the frozen sha under a temp dir, runs
 * `fn(worktreeDir)`, then removes it -- `tools/make-corpus.mjs`'s
 * `withFrozenWorktree`, reimplemented locally rather than imported: that
 * function is not exported, and `tools/*.mjs` is out of this task's scope
 * to modify. `list --open`/`get HR-052`/`batch 14` are content-specific
 * (their expected output names actual items and their current state), so
 * this pins them against the frozen sha's own backlog, immune to this
 * repository's own backlog changing on every later batch -- unlike
 * `check`, whose "ok" result is content-independent and safe to assert
 * against the live tree the same way `tests/kb.test.mjs`'s live-check
 * gate does for its own `root` slice.
 *
 * The scratch directory comes from `scratchDir` (houserules.tests-clean-
 * scratch-dirs), not a raw `mkdtempSync`, and `git worktree remove`'s own
 * `onTestFinished` is registered separately from -- and after -- the one
 * `scratchDir` already registered: `onTestFinished` callbacks run in
 * reverse registration order (verified live, vitest 4.1.11), so the
 * worktree-removal callback registered here runs FIRST, and `scratchDir`'s
 * own `rmSync` still runs after it even when `git worktree remove` itself
 * throws (task-2-review.json, issue 4: the previous try/finally version
 * skipped that `rmSync` on exactly this failure, matching `common/mod.rs`'s
 * `Drop` impl on the Rust side, which reports rather than swallows it).
 */
function withFrozenWorktree(fn) {
  const dir = scratchDir('backlog-worktree-');
  rmSync(dir, { recursive: true, force: true }); // git worktree add wants the path free
  execFileSync('git', ['worktree', 'add', '--detach', '--quiet', dir, frozenSha()], {
    cwd: REPO_ROOT,
  });
  onTestFinished(() => {
    try {
      execFileSync('git', ['worktree', 'remove', '--force', dir], { cwd: REPO_ROOT });
    } catch (error) {
      console.error(
        `warning: git worktree remove --force ${dir} failed: ${error.message}; ` +
          `run \`git worktree prune\` in ${REPO_ROOT}`,
      );
    }
  });
  return fn(dir);
}

describe('the live check command over the frozen corpus', () => {
  it("matches the frozen check slice's ok result against the mini fixture", () => {
    const expected = readCorpusRun('check.json');
    withMiniFixtureRepo((root) => {
      expect(runBacklog(['check'], root)).toEqual({
        stdout: expected.stdout,
        stderr: expected.stderr,
        exit: expected.exit,
      });
    });
  });

  it("matches the frozen check slice's ok result against this repository's own live tree", () => {
    const expected = readCorpusRun('check.json');
    expect(runBacklog(['check'], REPO_ROOT)).toEqual({
      stdout: expected.stdout,
      stderr: expected.stderr,
      exit: expected.exit,
    });
  });

  it('matches the frozen list --open, get HR-052, and batch 14 slices against a worktree pinned at the frozen sha', () => {
    withFrozenWorktree((root) => {
      for (const [args, slice] of [
        [['list', '--open'], 'list-open.json'],
        [['get', 'HR-052'], 'get-hr-052.json'],
        [['batch', '14'], 'batch-14.json'],
      ]) {
        const expected = readCorpusRun(slice);
        expect(runBacklog(args, root)).toEqual({
          stdout: expected.stdout,
          stderr: expected.stderr,
          exit: expected.exit,
        });
      }
    });
  });

  it('matches the frozen set slice: stdout and the written file, on a scratch copy of mini', () => {
    const expected = readCorpusRun('set/mini/command.json');
    withMiniFixtureRepo((root) => {
      expect(runBacklog(['set', 'HR-901', 'status=done', 'batch=2'], root)).toEqual({
        stdout: expected.stdout,
        stderr: expected.stderr,
        exit: expected.exit,
      });
      const written = readFileSync(join(root, 'backlog/items/misc.json'), 'utf8');
      const expectedWritten = readFileSync(
        new URL('./corpus/backlog/set/mini/backlog/items/misc.json', import.meta.url),
        'utf8',
      );
      expect(written).toBe(expectedWritten);
    });
  });
});
