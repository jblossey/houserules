import { existsSync, readFileSync, readdirSync } from 'node:fs';
import { join } from 'node:path';
import { describe, expect, it } from 'vitest';
import {
  FROZEN_SHA,
  generateCorpus,
  listFilesRecursive,
  sha256,
} from '../tools/make-corpus.mjs';
import { scratchDir } from './scratch-dir.mjs';

const COMMITTED = join(import.meta.dirname, 'corpus');

/**
 * `manifest.json` without its `node_version` field, as a comparable object.
 * `node_version` stamps the environment that generated the corpus, not the
 * frozen JS behavior the regeneration test asserts, so a node bump must not
 * read as a corpus mismatch: compare every other field, and the field's
 * presence, without pinning its value.
 */
function manifestWithoutNodeVersion(text) {
  const { node_version, ...rest } = JSON.parse(text);
  return { hadNodeVersion: typeof node_version === 'string' && node_version.length > 0, rest };
}

describe('the frozen fixture corpus (HR-054)', () => {
  it(
    'regenerates byte-identical to the committed corpus, node_version aside',
    () => {
      const outDir = scratchDir('corpus-gen-');
      const { files } = generateCorpus({ outDir });
      const committedFiles = listFilesRecursive(COMMITTED);

      expect(files).toEqual(committedFiles);

      const mismatches = files.filter((relPath) => {
        const generated = readFileSync(join(outDir, relPath));
        const committed = readFileSync(join(COMMITTED, relPath));
        if (relPath === 'manifest.json') {
          return (
            JSON.stringify(manifestWithoutNodeVersion(generated.toString('utf8'))) !==
            JSON.stringify(manifestWithoutNodeVersion(committed.toString('utf8')))
          );
        }
        return !generated.equals(committed);
      });
      expect(mismatches).toEqual([]);
    },
    30000,
  );

  it('the manifest pins the frozen sha and its inventory matches every corpus file', () => {
    const manifest = JSON.parse(
      readFileSync(join(COMMITTED, 'manifest.json'), 'utf8'),
    );
    expect(manifest.frozen_sha).toBe(FROZEN_SHA);
    const paths = Object.keys(manifest.inventory);
    expect(paths.length).toBeGreaterThan(0);
    for (const relPath of paths) {
      const abs = join(COMMITTED, relPath);
      expect(existsSync(abs)).toBe(true);
      expect(sha256(readFileSync(abs))).toBe(manifest.inventory[relPath]);
    }
  });

  // "freezes non-trivial render and check output" and "freezes every
  // message shape checkBase can emit" (both reading `COMMITTED/render/` and
  // `COMMITTED/check/`) were removed at batch 18 T5, docs/specs/2026-09-05-
  // batch-18-phase3.md §3: `render`'s and `check-knowledge`'s corpus slices
  // moved to reviewed, Rust-generated goldens (`tests/goldens/`, `cargo run
  // --bin gen-goldens`) -- `generateCorpus` no longer produces either
  // directory (`OWNED_ENTRIES`'s own doc in make-corpus.mjs has the full
  // account), so `tests/corpus/render/` and `tests/corpus/check/` no longer
  // exist for these tests to read. `crates/houserules/tests/check_parity.rs`
  // pins the same message shapes against the new goldens.

  it('discloses every path normalization in the manifest and the frozen bytes', () => {
    const manifest = JSON.parse(readFileSync(join(COMMITTED, 'manifest.json'), 'utf8'));
    expect(manifest.normalizations.length).toBeGreaterThan(0);
    for (const normalization of manifest.normalizations) {
      expect(normalization.substituted).toMatch(/^<.+>\//);
      const content = readFileSync(join(COMMITTED, normalization.slice), 'utf8');
      expect(content).toContain(normalization.substituted);
    }
    // No real absolute path from the generating machine survives into the
    // slice the redaction targets (batch 16 task 1 review, finding 4).
    for (const name of readdirSync(join(COMMITTED, 'validate'))) {
      const text = readFileSync(join(COMMITTED, 'validate', name), 'utf8');
      expect(text).not.toMatch(/\/(home|tmp|private)\//);
    }
  });

  it('freezes at least one failing validate run', () => {
    const validateRuns = readdirSync(join(COMMITTED, 'validate')).map((name) =>
      JSON.parse(readFileSync(join(COMMITTED, 'validate', name), 'utf8')),
    );
    expect(validateRuns.some((run) => run.exit !== 0)).toBe(true);
  });

  it('freezes the skipped>0 validate message naming --report (batch 17 T1)', () => {
    const run = JSON.parse(
      readFileSync(join(COMMITTED, 'validate/skipped-report.json'), 'utf8'),
    );
    expect(run.exit).toBe(1);
    expect(run.stdout).toContain(
      'self_audit.summary.skipped is 1; re-run audit with --report',
    );
  });

  it('freezes a stats slice over the committed batch-14 workspace fixtures', () => {
    // This slice exercises stats()'s reviews input path only: the fixture
    // holds no task-*-audit*.json, so audits and unused_ids stay
    // structurally empty here. stats/stats-workspace.json (below) is the
    // slice that exercises the audits path and the unused_ids
    // cross-reference (batch 17 T1 fix round 1, review issue 4).
    const run = JSON.parse(
      readFileSync(join(COMMITTED, 'stats/batch14-workspace.json'), 'utf8'),
    );
    expect(run.exit).toBe(0);
    const stats = JSON.parse(run.stdout);
    expect(stats.reviews.files).toBeGreaterThan(0);
    expect(stats.violations.length).toBeGreaterThan(0);
  });

  it('freezes a stats slice over a workspace with an audit file and a report (violations, unused_ids, audits, and the knowledge_used cross-reference all pinned)', () => {
    const run = JSON.parse(
      readFileSync(join(COMMITTED, 'stats/stats-workspace.json'), 'utf8'),
    );
    expect(run.exit).toBe(0);
    const stats = JSON.parse(run.stdout);
    expect(stats.audits.files).toBeGreaterThan(0);
    expect(stats.audits.tasks).toBeGreaterThan(0);
    expect(stats.violations.length).toBeGreaterThan(0);
    // The fixture's report cites exactly one of the audit file's two injected
    // ids in knowledge_used, so unused_ids must filter that one out -- a port
    // that never reads knowledge_used would leave both ids unused instead of
    // just the uncited one (batch 17 T1 fix round 2, review r1 new_breakage (c)).
    expect(stats.unused_ids.map((u) => u.id)).toEqual(['quality.principles']);
  });

  it('freezes the set-written bytes backlog.mjs produces for a status+batch change', () => {
    const run = JSON.parse(
      readFileSync(join(COMMITTED, 'backlog/set/mini/command.json'), 'utf8'),
    );
    expect(run.exit).toBe(0);
    expect(run.stdout).toBe('HR-901: status=done batch=2\n');
    const written = readFileSync(
      join(COMMITTED, 'backlog/set/mini/backlog/items/misc.json'),
      'utf8',
    );
    expect(written).toContain('"status": "done"');
    expect(written).toContain('"batch": 2');
  });
});
