import { execFileSync } from 'node:child_process';
import { writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { describe, expect, it } from 'vitest';
import { checkReportClaims, main } from '../tools/check-report-claims.mjs';
import { scratchDir } from './scratch-dir.mjs';

/** A fresh git repo under a scratch dir, with one commit so `HEAD` resolves. Returns `{ root, head }`. */
function initScratchRepo(prefix) {
  const root = scratchDir(prefix);
  execFileSync('git', ['init', '--quiet', root]);
  execFileSync(
    'git',
    ['-c', 'user.email=test@test.invalid', '-c', 'user.name=Test', 'commit', '--allow-empty', '-q', '-m', 'seed'],
    { cwd: root },
  );
  const head = execFileSync('git', ['rev-parse', '--short', 'HEAD'], { cwd: root, encoding: 'utf8' }).trim();
  return { root, head };
}

/** Writes `value` as JSON to `path`. */
function writeJson(path, value) {
  writeFileSync(path, JSON.stringify(value, null, 2));
}

/**
 * A minimal, otherwise-clean task-report shape: self_audit and `commits`
 * both naming `head` as the one and only commit, everything else empty --
 * so `head` is trivially the report's own newest listed commit, and every
 * test not specifically exercising the head-vs-listed-commits check can
 * ignore that machinery.
 */
function baseReport(head) {
  return {
    kind: 'task-report',
    self_audit: {
      summary: { base: head, head, deterministic: 3, pass: 3, fail: 0, warn: 0, skipped: 0, judged: 0 },
      rows: [],
    },
    implemented: 'nothing checkable here',
    commits: [{ sha: head, subject: 'seed' }],
    self_review: [],
    tests: [],
    live_run: [],
    tdd: [],
    fix_rounds: [],
  };
}

describe('checkReportClaims', () => {
  it('passes a report with no captures, markers, or narrative claims', () => {
    const { root, head } = initScratchRepo('check-report-claims-clean-');
    const reportPath = join(root, 'report.json');
    writeJson(reportPath, baseReport(head));
    expect(checkReportClaims(reportPath, { root })).toEqual({ errors: [] });
  });

  it('flags a redirected capture whose output does not byte-match its own target file', () => {
    const { root, head } = initScratchRepo('check-report-claims-redirect-');
    writeFileSync(join(root, 'capture.txt'), 'real content\n');
    const report = baseReport(head);
    report.live_run = [{ command: 'echo hi > capture.txt 2>&1', output: 'stale content\n' }];
    const reportPath = join(root, 'report.json');
    writeJson(reportPath, report);
    const { errors } = checkReportClaims(reportPath, { root });
    expect(errors).toEqual([
      'live_run[0]: output does not byte-match the file its own command redirects to (capture.txt)',
    ]);
  });

  it('passes a redirected capture whose output byte-matches its target file', () => {
    const { root, head } = initScratchRepo('check-report-claims-redirect-ok-');
    writeFileSync(join(root, 'capture.txt'), 'real content\n');
    const report = baseReport(head);
    report.live_run = [{ command: 'echo hi > capture.txt 2>&1', output: 'real content\n' }];
    const reportPath = join(root, 'report.json');
    writeJson(reportPath, report);
    expect(checkReportClaims(reportPath, { root })).toEqual({ errors: [] });
  });

  it('resolves an absolute-path redirect target as-is, not joined onto root', () => {
    const { root, head } = initScratchRepo('check-report-claims-absolute-redirect-');
    const outsideDir = scratchDir('check-report-claims-absolute-target-');
    const absolutePath = join(outsideDir, 'capture.txt');
    writeFileSync(absolutePath, 'real content\n');
    const report = baseReport(head);
    report.live_run = [{ command: `echo hi > ${absolutePath} 2>&1`, output: 'real content\n' }];
    const reportPath = join(root, 'report.json');
    writeJson(reportPath, report);
    // join(root, absolutePath) would append the absolute path onto root instead of
    // using it as-is, so this only passes once the check resolves it correctly.
    expect(checkReportClaims(reportPath, { root })).toEqual({ errors: [] });
  });

  it('flags a truncation marker whose claimed remaining-line count is wrong', () => {
    const { root, head } = initScratchRepo('check-report-claims-marker-');
    writeFileSync(join(root, 'full.txt'), 'one\ntwo\nthree\nfour\n');
    const report = baseReport(head);
    report.tdd = [
      {
        test: 'x',
        mode: 'natural',
        red: { command: 'x', output: 'one\n[... 99 more lines; full run: full.txt ...]\n' },
        green: { command: 'x', output: 'one\ntwo\nthree\nfour\n' },
      },
    ];
    const reportPath = join(root, 'report.json');
    writeJson(reportPath, report);
    const { errors } = checkReportClaims(reportPath, { root });
    expect(errors).toEqual([
      'tdd[0].red: marker "[... 99 more lines; full run: full.txt ...]" claims 99 more lines; full.txt actually has 3 more',
    ]);
  });

  it('passes a truncation marker with the correct remaining-line count', () => {
    const { root, head } = initScratchRepo('check-report-claims-marker-ok-');
    writeFileSync(join(root, 'full.txt'), 'one\ntwo\nthree\nfour\n');
    const report = baseReport(head);
    report.tdd = [
      {
        test: 'x',
        mode: 'natural',
        red: { command: 'x', output: 'one\n[... 3 more lines; full run: full.txt ...]\n' },
        green: { command: 'x', output: 'one\ntwo\nthree\nfour\n' },
      },
    ];
    const reportPath = join(root, 'report.json');
    writeJson(reportPath, report);
    expect(checkReportClaims(reportPath, { root })).toEqual({ errors: [] });
  });

  it('flags self_audit.summary.head when it is not the report\'s own newest listed commit', () => {
    const { root, head: firstCommit } = initScratchRepo('check-report-claims-stale-head-');
    execFileSync(
      'git',
      ['-c', 'user.email=test@test.invalid', '-c', 'user.name=Test', 'commit', '--allow-empty', '-q', '-m', 'second'],
      { cwd: root },
    );
    const secondCommit = execFileSync('git', ['rev-parse', '--short', 'HEAD'], {
      cwd: root,
      encoding: 'utf8',
    }).trim();
    const report = baseReport(firstCommit); // self_audit.head names the OLDER of the two
    report.commits = [{ sha: firstCommit, subject: 'first' }, { sha: secondCommit, subject: 'second' }];
    const reportPath = join(root, 'report.json');
    writeJson(reportPath, report);
    const { errors } = checkReportClaims(reportPath, { root });
    expect(errors).toEqual([
      `self_audit.summary.head is "${firstCommit}", but the report's own newest listed commit ` +
        `is "${secondCommit}" -- self_audit is stale`,
    ]);
  });

  it('passes when self_audit.summary.head is behind live HEAD but is still the report\'s own newest listed commit', () => {
    const { root, head } = initScratchRepo('check-report-claims-behind-head-ok-');
    const report = baseReport(head); // baseReport already lists `head` as its one commit
    const reportPath = join(root, 'report.json');
    writeJson(reportPath, report);
    // A later, unrelated commit -- simulating the next task landing on the same branch.
    // self_audit must still describe this report's own final commit, not the branch tip.
    execFileSync(
      'git',
      ['-c', 'user.email=test@test.invalid', '-c', 'user.name=Test', 'commit', '--allow-empty', '-q', '-m', 'later'],
      { cwd: root },
    );
    expect(checkReportClaims(reportPath, { root })).toEqual({ errors: [] });
  });

  it('flags a listed commit sha that does not resolve, instead of silently skipping the head check', () => {
    const { root, head } = initScratchRepo('check-report-claims-unresolvable-listed-sha-');
    const report = baseReport(head);
    // A bogus entry alongside the real, resolvable one: newestListedCommit's
    // own every() can satisfy neither "ancestor of" nor "descendant of" for
    // it, so every candidate (including the real head) fails, and the
    // unguarded function returned null for the whole list -- the same null
    // an honest unrelated-history report also produces, which this check
    // silently skips (r4 new_breakage 1, task-1-review-r4.json). A stale
    // head next to this same bogus sha must now be reported, not swallowed.
    report.self_audit.summary.head = 'deadbee1'; // deliberately stale too
    report.commits = [{ sha: head, subject: 'seed' }, { sha: 'deadbee2', subject: 'bogus' }];
    const reportPath = join(root, 'report.json');
    writeJson(reportPath, report);
    const { errors } = checkReportClaims(reportPath, { root });
    expect(errors).toEqual([
      'a commit this report lists, "deadbee2", does not resolve to a commit -- ' +
        'self_audit.summary.head cannot be checked against it',
    ]);
  });

  it('does not check self_audit.summary.head when self_audit is still null', () => {
    const { root, head } = initScratchRepo('check-report-claims-null-audit-');
    const report = baseReport(head);
    report.self_audit = null;
    const reportPath = join(root, 'report.json');
    writeJson(reportPath, report);
    expect(checkReportClaims(reportPath, { root })).toEqual({ errors: [] });
  });

  it('flags a narrative sha that does not resolve to a real commit', () => {
    const { root, head } = initScratchRepo('check-report-claims-bad-sha-');
    const report = baseReport(head);
    report.self_review = ['see commit deadbeef1 for context'];
    const reportPath = join(root, 'report.json');
    writeJson(reportPath, report);
    const { errors } = checkReportClaims(reportPath, { root });
    expect(errors).toEqual([
      'self_review[0]: "deadbeef1" looks like a commit sha but does not resolve to one',
    ]);
  });

  it('does not flag an all-digit token even though it matches the sha shape', () => {
    const { root, head } = initScratchRepo('check-report-claims-digit-token-');
    const report = baseReport(head);
    report.self_review = ['the capture is 1234567 bytes'];
    const reportPath = join(root, 'report.json');
    writeJson(reportPath, report);
    expect(checkReportClaims(reportPath, { root })).toEqual({ errors: [] });
  });

  it('flags a deterministic-pass ratio sitting next to a real sha that is neither base nor head', () => {
    const { root } = initScratchRepo('check-report-claims-stale-narrative-sha-');
    const shortHead = () =>
      execFileSync('git', ['rev-parse', '--short', 'HEAD'], { cwd: root, encoding: 'utf8' }).trim();
    const commit = (message) =>
      execFileSync(
        'git',
        ['-c', 'user.email=test@test.invalid', '-c', 'user.name=Test', 'commit', '--allow-empty', '-q', '-m', message],
        { cwd: root },
      );
    // priorHead must contain an a-f letter: the narrative check applies the same
    // digit-only guard checkNarrativeShasResolve does (a bare number is far more
    // likely a count than a sha), and a random short sha is all-digit about 1 run
    // in 28 (16 values, 10 digits: (10/16)^7). Retry commits until one qualifies,
    // rather than accept that flakiness.
    let priorHead = shortHead();
    for (let i = 0; !/[a-f]/.test(priorHead) && i < 20; i += 1) {
      commit(`retry ${i}`);
      priorHead = shortHead();
    }
    expect(priorHead).toMatch(/[a-f]/);
    // One more, real commit: priorHead is now a genuinely resolvable sha that is
    // neither self_audit.summary.base nor .head, isolating the narrative check
    // from checkNarrativeShasResolve (which priorHead would pass regardless).
    commit('final');
    const currentHead = shortHead();
    const report = baseReport(currentHead);
    // The ratio is the check's trigger and matches self_audit exactly (3/3, per
    // baseReport); only the co-located sha is wrong, isolating this instance.
    report.self_review = [`refreshed from an earlier run at head ${priorHead}, 3/3 deterministic pass`];
    const reportPath = join(root, 'report.json');
    writeJson(reportPath, report);
    const { errors } = checkReportClaims(reportPath, { root });
    expect(errors).toEqual([
      `self_review[0]: "3/3 deterministic" sits within 100 characters of sha "${priorHead}", ` +
        `which is neither self_audit.summary.base ("${currentHead}") nor .head ("${currentHead}")`,
    ]);
  });

  it('flags a self_audit-describing sentence with the wrong deterministic pass ratio', () => {
    const { root, head } = initScratchRepo('check-report-claims-stale-ratio-');
    const report = baseReport(head);
    report.self_review = ['self_audit shows 2/2 deterministic pass, 0 fail'];
    const reportPath = join(root, 'report.json');
    writeJson(reportPath, report);
    const { errors } = checkReportClaims(reportPath, { root });
    expect(errors).toEqual([
      'self_review[0]: "2/2 deterministic" does not match self_audit.summary (pass 3, deterministic 3)',
    ]);
  });

  it('passes a self_audit-describing sentence naming the real head and the real ratio', () => {
    const { root, head } = initScratchRepo('check-report-claims-narrative-ok-');
    const report = baseReport(head);
    report.self_review = [`self_audit at head ${head} shows 3/3 deterministic pass, 0 fail`];
    const reportPath = join(root, 'report.json');
    writeJson(reportPath, report);
    expect(checkReportClaims(reportPath, { root })).toEqual({ errors: [] });
  });
});

describe('main', () => {
  it('exits 0 and reports no mismatches for a clean report', () => {
    const { root, head } = initScratchRepo('check-report-claims-main-clean-');
    writeJson(join(root, 'report.json'), baseReport(head));
    const out = [];
    const exit = main(['report.json'], { out: (s) => out.push(s), err: () => {} }, root);
    expect(exit).toBe(0);
    expect(out.join('')).toBe('report.json: no claim mismatches found\n');
  });

  it('exits 1 and prints one line per mismatch for a broken report', () => {
    const { root, head } = initScratchRepo('check-report-claims-main-broken-');
    const report = baseReport(head);
    report.self_review = ['see commit deadbeef1 for context'];
    writeJson(join(root, 'report.json'), report);
    const err = [];
    const exit = main(['report.json'], { out: () => {}, err: (s) => err.push(s) }, root);
    expect(exit).toBe(1);
    expect(err.join('')).toContain('deadbeef1');
  });

  it('throws a UsageError when no report path is given', () => {
    expect(() => main([], { out: () => {}, err: () => {} }, process.cwd())).toThrow(
      'usage: check-report-claims.mjs <report.json>',
    );
  });
});
