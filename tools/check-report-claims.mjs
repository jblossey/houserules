#!/usr/bin/env node
// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Jannis Blossey
// Cross-checks one deliverable report's claims against the artifacts and git
// history it cites. Dev-only, root-level like tools/make-corpus.mjs: not
// shipped in template/ or the payload, not KIT_OWNED (README's ownership
// table is unaffected). Born from batch 17 task 1 (HR-059): four fix rounds
// in a row each closed on one process.claims-match-artifacts finding, and
// each finding was a narrative sentence describing a field the same round
// had just changed -- a hand re-check missed it every time, including once
// in the sentence describing this tool's own coverage. See that task's
// task-1-report.json (fix_rounds[0..3]) for the case history this tool
// exists to stop repeating.
//
// Limits (every doc comment below that says "see the module doc's limits"
// means this list): this tool narrows deliberately, in favour of a low
// false-positive rate over completeness, and a description of it should
// name what it does not catch, not only what it does.
// - Only four fields are scanned for narrative claims (`collectNarrative`):
//   `implemented`, `self_review[]`, and `fix_rounds[].findings[].finding`/
//   `.fix`. `docs_verified`, `concerns`, and an issue's `file`/`why` are not
//   -- widening the set is future work, not a limit of the mechanism itself.
// - `checkSelfAuditNarrative` fires only on a `"<N>/<M> deterministic"`
//   ratio, not on the word `self_audit` (an earlier version keyed on that
//   word and flagged nine lines of this report's own legitimate history --
//   a fixture's unrelated `self_audit` field, a past round's sha with no
//   ratio nearby). A sentence naming a stale head with no ratio nearby is
//   not caught.
// - `SHA_TOKEN` skips a token with no `a`-`f` letter: an all-digit run is
//   far more likely a byte or line count than a short sha (real, but rare).
// - No check here can tell a sentence *asserting* a fact from one
//   *quoting* a past mistake -- both contain the same stale sha and ratio.
//   `fix_rounds[3]`'s own finding text works around this by spelling
//   quoted historical counts as words ("nine of nine"), which the digit-
//   based checks do not parse.
// - `checkTruncationMarkers` verifies the excerpt is a byte-prefix of the
//   named file and the remaining-line count is exact; it never checks that
//   the file is really that *other* command's output, only that the bytes
//   match. A marker citing the right file with the wrong command label in
//   the `run.command` field passes clean (batch 17 task 1's own round-2
//   `tdd[1].green` mislabel, restored as a probe against this tool during
//   its review, is exactly this shape).
// - `checkSelfAuditHeadIsCurrent` compares `self_audit.summary.head`
//   against the newest commit the report itself lists in `commits[]` and
//   `fix_rounds[].commits[]` (see that function's own doc for why, and
//   what it replaced) -- not against live HEAD. It trusts that list: a
//   report that quietly omits one of its own real commits is not caught.
// - Both `checkSelfAuditHeadIsCurrent` and `checkNarrativeShasResolve`
//   assume every sha a report names stays resolvable, which holds only
//   while the branch is live: houserules.pinned-shas-live-on-mains-ancestry
//   records that this repository's fast-forward merges discard every
//   pre-aggregation branch sha, unreachable from any ref afterward. Run
//   this tool while the branch that produced the report is still live --
//   after aggregation, every commit sha the report names (including one a
//   sentence itself marks as replaced, like a pre-review amend) reads as
//   unresolvable, and this tool cannot yet tell that shape of "correct but
//   aged" report from a genuinely broken one.
import { execFileSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { UsageError, isMainModule } from './lib/cli.mjs';
import { readJson, repoRoot } from './lib/json-store.mjs';

/**
 * Every deliverable field shaped `{ command, output, exit? }` (the `run`
 * definition in `.claude/schemas/deliverables.json`) this tool can inspect,
 * each paired with a JSON-path-ish label for its findings. Covers `tests`,
 * `live_run`, each `tdd[].red`/`.green`, and each `fix_rounds[].tests[]` --
 * every place the schema allows a captured command's output.
 */
function collectRuns(report) {
  const runs = [];
  const push = (label, run) => {
    if (run && typeof run.command === 'string' && typeof run.output === 'string')
      runs.push({ label, run });
  };
  for (const [i, r] of (report.tests ?? []).entries()) push(`tests[${i}]`, r);
  for (const [i, r] of (report.live_run ?? []).entries()) push(`live_run[${i}]`, r);
  for (const [i, cycle] of (report.tdd ?? []).entries()) {
    push(`tdd[${i}].red`, cycle.red);
    push(`tdd[${i}].green`, cycle.green);
  }
  for (const [i, round] of (report.fix_rounds ?? []).entries())
    for (const [j, r] of (round.tests ?? []).entries()) push(`fix_rounds[${i}].tests[${j}]`, r);
  return runs;
}

/**
 * Every free-text field a `process.claims-match-artifacts` review reads as
 * prose making checkable claims, not structured data: `implemented`, each
 * `self_review[]` entry, and each `fix_rounds[].findings[].finding`/`.fix`.
 * `concerns`, `docs_verified`, and issue/finding `file`/`why` fields are
 * left out -- not because they cannot carry a claim, but because the three
 * failures this tool was built from were all in these four shapes; widening
 * the set is future work, named in the module doc's limits.
 */
function collectNarrative(report) {
  const narrative = [];
  if (typeof report.implemented === 'string')
    narrative.push({ label: 'implemented', text: report.implemented });
  for (const [i, text] of (report.self_review ?? []).entries())
    if (typeof text === 'string') narrative.push({ label: `self_review[${i}]`, text });
  for (const [i, round] of (report.fix_rounds ?? []).entries())
    for (const [j, f] of (round.findings ?? []).entries()) {
      if (typeof f.finding === 'string')
        narrative.push({ label: `fix_rounds[${i}].findings[${j}].finding`, text: f.finding });
      if (typeof f.fix === 'string')
        narrative.push({ label: `fix_rounds[${i}].findings[${j}].fix`, text: f.fix });
    }
  return narrative;
}

const REDIRECT_CAPTURE = />\s*(\S+)\s+2>&1\s*$/;
const TRUNCATION_MARKER = /\[\.\.\. (\d+) more lines?; full run: (\S+?) \.\.\.\]/;
/** A commit-like hex token, filtered to exclude a bare run of digits (a byte or line count is
 * far more likely in report prose than an all-digit short sha; see the module doc's limits). */
const SHA_TOKEN = /\b[0-9a-f]{7,40}\b/g;
const DETERMINISTIC_RATIO = /(\d+)\s*\/\s*(\d+)\s+deterministic/gi;

/** `true` when `sha` resolves to a real commit reachable in `root`'s object database. */
function resolvesToCommit(root, sha) {
  try {
    execFileSync('git', ['rev-parse', '--verify', '--quiet', `${sha}^{commit}`], {
      cwd: root,
      stdio: ['ignore', 'ignore', 'ignore'],
    });
    return true;
  } catch {
    return false;
  }
}

/**
 * Checks every `collectRuns` entry whose `command` ends in a `> <path>
 * 2>&1` redirect: `output` must be byte-identical to the named file's
 * current content, resolved against `root`. A command that redirects to a
 * file is asserting "this is what that file holds" -- the strongest, most
 * literal claim a `run` entry can make, so this check accepts no
 * near-match, unlike the truncation-marker check below.
 */
function checkRedirectedCaptures(root, runs, errors) {
  for (const { label, run } of runs) {
    const match = run.command.match(REDIRECT_CAPTURE);
    if (!match) continue;
    const relPath = match[1];
    let fileContent;
    try {
      fileContent = readFileSync(resolve(root, relPath), 'utf8');
    } catch (error) {
      errors.push(`${label}: command redirects to "${relPath}", which could not be read (${error.message})`);
      continue;
    }
    if (run.output !== fileContent)
      errors.push(
        `${label}: output does not byte-match the file its own command redirects to (${relPath})`,
      );
  }
}

/**
 * Checks every `collectRuns` entry whose `output` carries a truncation
 * marker (`[... N more lines; full run: <path> ...]`, the shape
 * `process.evidence-outlives-the-session` asks for): the text before the
 * marker must be an exact byte-prefix of the named file, and `N` must
 * exactly equal that file's remaining line count.
 */
function checkTruncationMarkers(root, runs, errors) {
  for (const { label, run } of runs) {
    const match = run.output.match(TRUNCATION_MARKER);
    if (!match) continue;
    const [markerText, claimedRemaining, relPath] = match;
    const excerpt = run.output.slice(0, match.index);
    let fullText;
    try {
      fullText = readFileSync(resolve(root, relPath), 'utf8');
    } catch (error) {
      errors.push(`${label}: marker names "${relPath}", which could not be read (${error.message})`);
      continue;
    }
    if (!fullText.startsWith(excerpt)) {
      errors.push(`${label}: the excerpt before its marker is not a byte-prefix of ${relPath}`);
      continue;
    }
    const fullLines = fullText.split('\n');
    const fullLineCount = fullLines.at(-1) === '' ? fullLines.length - 1 : fullLines.length;
    const excerptLineCount = excerpt.split('\n').length - 1;
    const realRemaining = fullLineCount - excerptLineCount;
    if (realRemaining !== Number(claimedRemaining))
      errors.push(
        `${label}: marker "${markerText}" claims ${claimedRemaining} more lines; ${relPath} actually has ${realRemaining} more`,
      );
  }
}

/** Every commit sha the report itself names: `commits[].sha` plus every `fix_rounds[].commits[].sha`. */
function collectListedCommitShas(report) {
  const shas = [];
  for (const c of report.commits ?? []) if (typeof c?.sha === 'string') shas.push(c.sha);
  for (const round of report.fix_rounds ?? [])
    for (const c of round.commits ?? []) if (typeof c?.sha === 'string') shas.push(c.sha);
  return shas;
}

/** `true` when `ancestorSha` is an ancestor of (or equal to) `descendantSha` in `root`. */
function isAncestor(root, ancestorSha, descendantSha) {
  try {
    execFileSync('git', ['merge-base', '--is-ancestor', ancestorSha, descendantSha], {
      cwd: root,
      stdio: 'ignore',
    });
    return true;
  } catch {
    return false;
  }
}

/**
 * The one sha among `shas` that every other sha is an ancestor of, or
 * `null` when no such total order exists (an empty list, or shas from
 * unrelated history). `shas` are expected to form one straight line --
 * this task's own commit-by-commit history -- so this is the list's tip,
 * found by testing each candidate against every other rather than
 * assuming list order. Every element of `shas` must already resolve to a
 * real commit: one that does not makes `isAncestor` return `false` for
 * every pairing it appears in, so no candidate can ever satisfy its own
 * `.every()` check and this returns `null` for the whole list -- the same
 * `null` an honest unrelated-history list produces. `checkSelfAuditHeadIsCurrent`,
 * this function's only caller, resolves that ambiguity itself before
 * calling in (see its own doc).
 */
function newestListedCommit(root, shas) {
  const unique = [...new Set(shas)];
  for (const candidate of unique) {
    if (unique.every((other) => other === candidate || isAncestor(root, other, candidate)))
      return candidate;
  }
  return null;
}

/**
 * `self_audit.summary.head` must equal the newest commit the report itself
 * lists (`commits[]` plus every `fix_rounds[].commits[]`), not `root`'s
 * live `git rev-parse HEAD`. `process.deliverables-json` scopes `self_audit`
 * to `BASE..HEAD` at report time: a finished report's head is that task's
 * final commit forever, whatever the branch tip becomes afterward. An
 * earlier version of this check compared against live HEAD instead, which
 * is correct only until the next commit lands on the branch -- proven
 * false-positive on a committed, correct historical report in this very
 * repository (`tests/corpus/fixtures/batch14-workspace/task-1-report.json`)
 * during this tool's own review. Skipped (not an error) when `self_audit`
 * is still `null` (a legitimate in-flight state) or the report lists no
 * commit at all (nothing to compare against).
 *
 * Every listed sha must resolve before `newestListedCommit` runs at all: a
 * sha `isAncestor` cannot resolve makes EVERY candidate fail its own
 * `.every()` check (it can satisfy neither "ancestor of" nor "descendant
 * of" any real sha), so `newestListedCommit` returns `null` for the whole
 * list -- indistinguishable, without this guard, from the genuine
 * unrelated-history case that same `null` also signals, and this check
 * silently skipped both alike (r4 new_breakage 1, task-1-review-r4.json: a
 * bogus sha added anywhere in `commits[]` switched off the strongest check
 * this tool has, with a clean run and no sign anything was wrong). A
 * resolution failure here is reported as its own named error instead, and
 * the silent skip is reserved for the case `newestListedCommit` alone
 * still can't resolve: every sha real, but forming no single line.
 */
function checkSelfAuditHeadIsCurrent(root, report, errors) {
  const head = report.self_audit?.summary?.head;
  if (!head) return;
  const listed = collectListedCommitShas(report);
  if (listed.length === 0) return;
  const unresolved = [...new Set(listed)].filter((sha) => !resolvesToCommit(root, sha));
  if (unresolved.length) {
    for (const sha of unresolved)
      errors.push(
        `a commit this report lists, "${sha}", does not resolve to a commit -- ` +
          `self_audit.summary.head cannot be checked against it`,
      );
    return;
  }
  const newest = newestListedCommit(root, listed);
  if (newest === null) return; // every listed sha is real; they just form no single line
  if (head !== newest)
    errors.push(
      `self_audit.summary.head is "${head}", but the report's own newest listed commit is ` +
        `"${newest}" -- self_audit is stale`,
    );
}

/**
 * Every commit-shaped token in narrative prose must resolve to a real
 * commit in `root`'s object database. Catches a typo or a fabricated sha;
 * does not catch a real, resolvable sha used to describe the wrong thing
 * (see `checkSelfAuditNarrative` for the one shape of that this tool does
 * check, and the module doc for the rest).
 */
function checkNarrativeShasResolve(root, narrative, errors) {
  for (const { label, text } of narrative) {
    for (const token of new Set(text.match(SHA_TOKEN) ?? [])) {
      if (!/[a-f]/.test(token)) continue; // an all-digit token is far more likely a count
      if (!resolvesToCommit(root, token))
        errors.push(`${label}: "${token}" looks like a commit sha but does not resolve to one`);
    }
  }
}

/** Characters either side of a `DETERMINISTIC_RATIO` match searched for a co-located sha. */
const RATIO_SHA_WINDOW = 100;

/**
 * A `"<N>/<M> deterministic"` ratio anywhere in narrative prose is a
 * specific, low-noise signal that the sentence is asserting a fact about
 * the report's own self-audit block (unlike the bare word `self_audit`,
 * which also names unrelated things -- a fixture's own `self_audit` field,
 * the concept in the abstract -- so it is not this check's trigger; see
 * the module doc's limits). Two things about that ratio must hold: it
 * must equal `pass`/`deterministic`, and any commit sha within
 * `RATIO_SHA_WINDOW` characters of it must be `base` or `head`. This is
 * the check the tool exists for: all three of batch 17 task 1's fix-round
 * findings were exactly this shape -- a sentence naming a stale head next
 * to a stale pass count for the self-audit sitting elsewhere in the file.
 * Narrower than it could be, on purpose: a sentence naming only a stale
 * head, with no ratio nearby, is not caught (also a module-doc limit).
 */
function checkSelfAuditNarrative(root, narrative, report, errors) {
  const summary = report.self_audit?.summary;
  if (!summary) return;
  const known = new Set([summary.base, summary.head]);
  for (const { label, text } of narrative) {
    for (const match of text.matchAll(DETERMINISTIC_RATIO)) {
      const [ratioText, pass, deterministic] = match;
      if (Number(pass) !== summary.pass || Number(deterministic) !== summary.deterministic) {
        errors.push(
          `${label}: "${ratioText}" does not match self_audit.summary (pass ${summary.pass}, ` +
            `deterministic ${summary.deterministic})`,
        );
        continue;
      }
      const start = Math.max(0, match.index - RATIO_SHA_WINDOW);
      const end = Math.min(text.length, match.index + ratioText.length + RATIO_SHA_WINDOW);
      for (const token of new Set(text.slice(start, end).match(SHA_TOKEN) ?? [])) {
        if (/[a-f]/.test(token) && !known.has(token))
          errors.push(
            `${label}: "${ratioText}" sits within ${RATIO_SHA_WINDOW} characters of sha ` +
              `"${token}", which is neither self_audit.summary.base ("${summary.base}") nor ` +
              `.head ("${summary.head}")`,
          );
      }
    }
  }
}

/**
 * Runs every check above against the report at `reportPath` (resolved
 * against `root`) and returns its errors, each a one-line, self-contained
 * description naming the field and the mismatch. An empty array is a clean
 * run: every capture, marker, and narrative claim this tool knows how to
 * check matched its artifact.
 */
export function checkReportClaims(reportPath, { root = repoRoot() } = {}) {
  const report = readJson(reportPath);
  const runs = collectRuns(report);
  const narrative = collectNarrative(report);
  const errors = [];
  checkRedirectedCaptures(root, runs, errors);
  checkTruncationMarkers(root, runs, errors);
  checkSelfAuditHeadIsCurrent(root, report, errors);
  checkNarrativeShasResolve(root, narrative, errors);
  checkSelfAuditNarrative(root, narrative, report, errors);
  return { errors };
}

/** Parses argv, runs the checks, and writes the result through `io`. */
export function main(argv, io, cwd) {
  const [reportArg] = argv;
  if (!reportArg) throw new UsageError('usage: check-report-claims.mjs <report.json>');
  const root = repoRoot(cwd);
  const reportPath = resolve(cwd, reportArg);
  const { errors } = checkReportClaims(reportPath, { root });
  if (errors.length) {
    io.err(errors.map((e) => `${e}\n`).join(''));
    return 1;
  }
  io.out(`${reportArg}: no claim mismatches found\n`);
  return 0;
}

if (isMainModule(import.meta.url)) {
  try {
    process.exitCode = main(
      process.argv.slice(2),
      { out: (s) => process.stdout.write(s), err: (s) => process.stderr.write(s) },
      process.cwd(),
    );
  } catch (error) {
    if (error instanceof UsageError) {
      process.stderr.write(`${error.message}\n`);
      process.exitCode = 2;
    } else {
      throw error;
    }
  }
}
