#!/usr/bin/env node
// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Jannis Blossey
// Knowledge base CLI, shipped by houserules. The design record lives in the houserules repository (docs/design.md).
import { execFileSync } from 'node:child_process';
import {
  existsSync,
  mkdirSync,
  readFileSync,
  readdirSync,
  writeFileSync,
} from 'node:fs';
import { dirname, join, matchesGlob, resolve } from 'node:path';
import { UsageError, isMainModule, parseArgs } from './lib/cli.mjs';
import {
  Errors,
  emit,
  readJson,
  repoRoot,
  validate,
} from './lib/json-store.mjs';

/** Header stamped on every file `render` writes, so an editor knows not to hand-edit it. */
export const GENERATED =
  'Generated from knowledge/ by tools/kb.sh render. Do not edit.';
/** Size limits `checkBase` enforces on the generated markdown files. */
export const BUDGETS = {
  claudeMdLines: 200,
  claudeMdBytes: 12288,
  standingLines: 60,
  areaLines: 160,
  skillLines: 120,
};
/** The command a `for` result points readers at, to see the full standing set. */
export const STANDING_COMMAND = 'tools/kb.sh standing';
/** Repo-relative path of the generated knowledge skill file. */
export const SKILL_PATH = '.claude/skills/project-knowledge/SKILL.md';
/** Entry kinds eligible for `standing`, and for area membership in an audit package. */
export const RULE_KINDS = ['rule', 'invariant'];
/** Entry kinds rendered into a per-area `.claude/rules/<area>.md` file. */
export const AREA_FILE_KINDS = ['rule', 'invariant', 'gotcha'];
const FOR_KINDS = [...AREA_FILE_KINDS, 'procedure'];
/** Path, relative to the repo root, of the agent-deliverables JSON Schema. */
export const DELIVERABLES_SCHEMA = '.claude/schemas/deliverables.json';
/** Maps a deliverable's `kind` field to its definition name in `DELIVERABLES_SCHEMA`. */
const DELIVERABLE_KINDS = {
  'task-report': 'taskReport',
  'task-review': 'taskReview',
  're-review': 'reReview',
  'branch-review': 'branchReview',
};
const CHECK_FIELDS = {
  'grep-absent': ['files', 'pattern', 'scope'],
  commits: [],
  'co-change': ['if', 'then'],
  'diff-append-only': ['files'],
  'report-field': ['if', 'field'],
};

/** Orders two entries (or any object with an `id`) by id, for `Array.prototype.toSorted`. */
export const byId = (a, b) => (a.id < b.id ? -1 : a.id > b.id ? 1 : 0);
/** Normalizes a schema field that may be one string or a list into always a list. */
export const list = (value) => (Array.isArray(value) ? value : [value]);
const stripDot = (path) => path.replace(/^\.\//, '');

// ---- loading ----------------------------------------------------------------

/** Loads every knowledge topic file under `root`, indexing entries by id. */
export function loadBase(root) {
  const dir = join(root, 'knowledge');
  const schema = readJson(join(dir, 'schema.json'));
  const areas = readJson(join(dir, 'areas.json'));
  const topics = readdirSync(dir)
    .filter(
      (name) =>
        name.endsWith('.json') &&
        name !== 'schema.json' &&
        name !== 'areas.json',
    )
    .toSorted()
    .map((name) => ({
      file: `knowledge/${name}`,
      name: name.slice(0, -5),
      ...readJson(join(dir, name)),
    }));
  const entries = new Map();
  for (const topic of topics) {
    for (const item of Array.isArray(topic.entries) ? topic.entries : []) {
      if (item && typeof item.id === 'string' && !entries.has(item.id))
        entries.set(item.id, { ...item, topic: topic.name });
    }
  }
  return { root, schema, areas, topics, entries };
}

/** Escapes every RegExp metacharacter in `text`, so it matches only itself literally. */
function escapeRegExp(text) {
  return text.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}
/**
 * Compiles `glob` into a `RegExp` that matches a whole repo-relative path,
 * covering only `**` (any number of complete path segments, including none)
 * and `*` (any characters within one segment) with dot-segments included.
 * Every other glob metacharacter (`?`, bracket classes, brace lists) is left
 * to `matchesGlob` itself in `globMatch` below and is escaped here as a
 * plain literal: this is a fallback for the one case `matchesGlob` gets
 * wrong, not a full glob engine.
 */
function globToRegExp(glob) {
  let source = '';
  for (let i = 0; i < glob.length; i++) {
    const c = glob[i];
    if (c === '*' && glob[i + 1] === '*') {
      if (glob[i + 2] === '/') {
        source += '(?:.*/)?';
        i += 2;
      } else {
        source += '.*';
        i += 1;
      }
      continue;
    }
    if (c === '*') {
      source += '[^/]*';
      continue;
    }
    source += escapeRegExp(c);
  }
  return new RegExp(`^${source}$`);
}
/**
 * Matches `path` against `glob`; the one matcher `areaFiles` and `matchAny`
 * both call. `matchesGlob` covers the full glob vocabulary (`?`, bracket
 * classes, brace lists, `**`, `*`) but excludes a path segment that starts
 * with `.` under `**` — undocumented as of Node 24.18.1 (verified live:
 * `matchesGlob('a/.b/c', 'a/**')` is `false`) — which silently dropped this
 * repository's own `template` area, and any `co-change`/`diff-append-only`/
 * file-scoped check whose glob crosses a dot-segment, from every audit
 * package. globMatch tries `matchesGlob` first and falls back to
 * `globToRegExp` only when it returns false, so every glob `matchesGlob`
 * already matched still matches, and dot-segment globs now match too.
 * A glob that combines `?`, a bracket class, or a brace list with a
 * dot-segment crossing matches neither engine: write area and check globs
 * with `**`, `*`, and literals, or extend `globToRegExp` first.
 */
function globMatch(path, glob) {
  return matchesGlob(path, glob) || globToRegExp(glob).test(path);
}

/**
 * Groups `paths` by every area whose globs they match, each area mapped to
 * the paths that matched it. `global` always appears, mapped to `[]`: it
 * has no globs of its own but applies to every path.
 */
export function areaFiles(paths, areas) {
  const found = { global: [] };
  for (const path of paths) {
    const rel = stripDot(path);
    for (const [area, def] of Object.entries(areas)) {
      if (def.paths.some((glob) => globMatch(rel, glob)))
        (found[area] ??= []).push(path);
    }
  }
  return found;
}
/** Resolves repo-relative paths to their areas through the glob map; `global` always applies. */
export function areasFor(paths, areas) {
  return Object.keys(areaFiles(paths, areas)).toSorted();
}

// ---- check -------------------------------------------------------------------

function checkShape(check, at, errors) {
  const fields = CHECK_FIELDS[check.type];
  if (!fields) return; // the schema already reported an unknown type
  for (const field of fields)
    if (!(field in check))
      errors.add(`${at}: check "${check.type}" needs "${field}"`);
  if (
    check.type === 'commits' &&
    !check.subject &&
    !check.body_absent &&
    !check.body_line_max
  ) {
    errors.add(
      `${at}: check "commits" needs "subject", "body_absent", or "body_line_max"`,
    );
  }
  for (const field of ['pattern', 'subject', 'body_absent']) {
    if (typeof check[field] !== 'string') continue;
    try {
      RegExp(check[field], check.flags ?? '');
    } catch (error) {
      errors.add(
        `${at}: check ${field} is not a valid regex (${error.message})`,
      );
    }
  }
}

function checkBudget(root, path, maxLines, maxBytes, errors) {
  const abs = join(root, path);
  if (!existsSync(abs)) {
    errors.add(`${path}: missing`);
    return;
  }
  const text = readFileSync(abs, 'utf8');
  const lines = text.split('\n').length - (text.endsWith('\n') ? 1 : 0);
  if (lines > maxLines)
    errors.add(`${path}: ${lines} lines, budget ${maxLines}`);
  const bytes = Buffer.byteLength(text);
  if (maxBytes != null && bytes > maxBytes)
    errors.add(`${path}: ${bytes} bytes, budget ${maxBytes}`);
}

/** Validates a loaded base against the schema and every cross-entry and generated-file invariant. */
export function checkBase(base) {
  const errors = new Errors();
  const { schema, areas, topics, root } = base;
  validate(areas, schema.$defs.areas, 'knowledge/areas.json', errors, schema);
  const areaNames = schema.$defs.area.enum;
  for (const area of areaNames)
    if (!(area in areas))
      errors.add(`knowledge/areas.json: area "${area}" is missing`);
  for (const area of Object.keys(areas))
    if (!areaNames.includes(area))
      errors.add(`knowledge/areas.json: unknown area "${area}"`);
  const seen = new Map();
  for (const topic of topics) {
    const { file, name, ...content } = topic;
    validate(content, schema, file, errors, schema);
    if (content.topic !== name)
      errors.add(
        `${file}: topic "${content.topic}" must equal the file name "${name}"`,
      );
    for (const item of Array.isArray(content.entries) ? content.entries : []) {
      if (!item || typeof item.id !== 'string') continue;
      const at = `${file} ${item.id}`;
      if (!item.id.startsWith(`${name}.`))
        errors.add(`${at}: id must start with "${name}."`);
      if (seen.has(item.id))
        errors.add(`${at}: duplicate id (also in ${seen.get(item.id)})`);
      seen.set(item.id, file);
      if (
        item.standing &&
        !(
          RULE_KINDS.includes(item.kind) &&
          ['global', 'process'].includes(item.area)
        )
      ) {
        errors.add(
          `${at}: standing needs kind rule or invariant and area global or process`,
        );
      }
      for (const id of Array.isArray(item.see) ? item.see : [])
        if (!base.entries.has(id))
          errors.add(`${at}: see "${id}" does not exist`);
      for (const path of Array.isArray(item.verify) ? item.verify : []) {
        if (!existsSync(join(root, path)))
          errors.add(`${at}: verify path "${path}" does not exist`);
      }
      if (item.check && typeof item.check === 'object')
        checkShape(item.check, at, errors);
    }
  }
  if (errors.any) return errors.list; // rendering needs a valid base
  const rendered = renderAll(base);
  for (const [path, content] of rendered) {
    const abs = join(root, path);
    if (!existsSync(abs) || readFileSync(abs, 'utf8') !== content)
      errors.add(
        `${path}: generated file is out of date (run tools/kb.sh render)`,
      );
  }
  const rulesDir = join(root, '.claude/rules');
  if (existsSync(rulesDir)) {
    for (const name of readdirSync(rulesDir)) {
      if (name.endsWith('.md') && !rendered.has(`.claude/rules/${name}`))
        errors.add(`.claude/rules/${name}: not generated by kb; remove it`);
    }
  }
  checkBudget(
    root,
    'CLAUDE.md',
    BUDGETS.claudeMdLines,
    BUDGETS.claudeMdBytes,
    errors,
  );
  for (const path of rendered.keys()) {
    if (path === '.claude/rules/standing-rules.md')
      checkBudget(root, path, BUDGETS.standingLines, null, errors);
    else if (path === SKILL_PATH)
      checkBudget(root, path, BUDGETS.skillLines, null, errors);
    else checkBudget(root, path, BUDGETS.areaLines, null, errors);
  }
  return errors.list;
}

// ---- render -----------------------------------------------------------------------

const PROTOCOL = [
  '1. Resolve every id under `Knowledge:` in your task: `tools/kb.sh get <ids>` (JSON).',
  '2. Before editing, run `tools/kb.sh for <every file you will change>` and `get` any rule you are unsure about.',
  "3. Write `REPORT_FILE` as a `task-report` (schema `.claude/schemas/deliverables.json`, `self_audit: null`), run `tools/kb.sh validate <REPORT_FILE>`, then `tools/kb.sh audit --base <BASE> --head HEAD --ids <ids, comma-separated> --report <REPORT_FILE>`. The `--ids` value is the task's `Knowledge:` list, generated from it, never typed separately. Copy the audit `summary` and its `deterministic` rows into `self_audit` — never hand-written rows; the judged rows are the reviewer's. Fix every `fail`, re-run until clean, validate again. List the ids you relied on in `knowledge_used`.",
];
const cap = (s) => s[0].toUpperCase() + s.slice(1);

/** Builds the generated markdown files (standing rules, per-area rules, the knowledge skill). */
export function renderAll(base) {
  const files = new Map();
  files.set(
    '.claude/rules/standing-rules.md',
    `${GENERATED}\n\n# Standing rules\n\n${standingLines(base).join('\n')}\n`,
  );
  for (const [area, def] of Object.entries(base.areas)) {
    if (def.paths.length === 0) continue;
    const entries = [...base.entries.values()].filter(
      (e) => e.area === area && AREA_FILE_KINDS.includes(e.kind),
    );
    if (entries.length === 0) continue;
    const sections = [
      ['Rules', 'rule'],
      ['Invariants', 'invariant'],
      ['Gotchas', 'gotcha'],
    ]
      .map(([title, kind]) => [
        title,
        entries.filter((e) => e.kind === kind).toSorted(byId),
      ])
      .filter(([, items]) => items.length > 0)
      .map(
        ([title, items]) =>
          `## ${title}\n\n${items.map(ruleLine).join('\n')}\n`,
      );
    const paths = def.paths
      .map((glob) => `  - ${JSON.stringify(glob)}`)
      .join('\n');
    files.set(
      `.claude/rules/${area}.md`,
      `---\npaths:\n${paths}\n---\n${GENERATED}\n\n# ${cap(area)} rules\n\n${sections.join('\n')}\nDetail: tools/kb.sh get <id>\n`,
    );
  }
  files.set(
    SKILL_PATH,
    [
      '---',
      'name: project-knowledge',
      'description: Use when working on this repository as a dispatched subagent, before reading or changing any file',
      'user-invocable: false',
      '---',
      GENERATED,
      '',
      '# Project knowledge',
      '',
      '## Standing rules',
      '',
      ...standingLines(base),
      '',
      '## Retrieval protocol',
      '',
      ...PROTOCOL,
      '',
      '## Topics',
      '',
      ...topicLines(base),
      '',
    ].join('\n'),
  );
  return files;
}

/** Writes every stale generated file (or, with `check`, only reports which ones are stale). */
export function render(base, { check = false } = {}) {
  const stale = [];
  for (const [path, content] of renderAll(base)) {
    const abs = join(base.root, path);
    if (existsSync(abs) && readFileSync(abs, 'utf8') === content) continue;
    stale.push(path);
    if (!check) {
      mkdirSync(dirname(abs), { recursive: true });
      writeFileSync(abs, content);
    }
  }
  return stale;
}

// ---- formatting ----------------------------------------------------------------

const ruleLine = (e) => `- [${e.id}] ${e.summary}`;

function ordered(entries, kinds) {
  return kinds.flatMap((kind) =>
    entries.filter((e) => e.kind === kind).toSorted(byId),
  );
}
const standingEntries = (base) =>
  ordered(
    [...base.entries.values()].filter((e) => e.standing),
    RULE_KINDS,
  );
/** Renders the standing rules as `- [id] summary` markdown lines, rules then invariants. */
export function standingLines(base) {
  return standingEntries(base).map(ruleLine);
}
const topicEntryCount = (t) =>
  Array.isArray(t.entries) ? t.entries.length : 0;
/** Renders the `## Topics` markdown lines the knowledge skill file lists. */
export function topicLines(base) {
  return base.topics.map((t) => `${t.name}  ${topicEntryCount(t)}  ${t.title}`);
}

// ---- git ----------------------------------------------------------------------------

function git(root, args, options = {}) {
  return execFileSync('git', args, {
    cwd: root,
    encoding: 'utf8',
    maxBuffer: 64 * 1024 * 1024,
    env: { ...process.env, LC_ALL: 'C' },
    ...options,
  });
}
function rev(root, ref) {
  try {
    return git(root, ['rev-parse', '--short', '--verify', `${ref}^{commit}`], {
      stdio: ['ignore', 'pipe', 'ignore'],
    }).trim();
  } catch {
    throw new UsageError(`bad ref "${ref}"`);
  }
}
const lines = (text) => text.split('\n').filter(Boolean);
/**
 * A three-dot `git diff` range from the merge base of `base` and `head` to
 * `head` (`git diff A...B` is `git diff $(git merge-base A B) B`). A file
 * `base`'s branch gains after `head` was cut is not on either side of this
 * range, so it is never attributed to `head`'s own changes.
 */
const range = (base, head) => `${base}...${head}`;
/**
 * Runs `git diff` with `args`, which must place `range(base, head)`
 * wherever the caller needs it.
 * When `base` and `head` share no merge base (an orphan branch, or two
 * grafted histories), this throws the fixed `UsageError` message
 * `no merge base between "<base>" and "<head>"`.
 * Any other failure (a bad pathspec in `args`, for example) throws a
 * `UsageError` carrying the stderr's first non-empty line, trimmed, or the
 * caught error's own message when stderr is empty.
 */
export function gitDiff(root, base, head, args) {
  try {
    return git(root, ['diff', ...args], {
      stdio: ['ignore', 'pipe', 'pipe'],
    });
  } catch (error) {
    const stderr = error.stderr ?? '';
    if (stderr.includes('no merge base'))
      throw new UsageError(`no merge base between "${base}" and "${head}"`);
    const firstLine = stderr.split('\n').find((line) => line.trim() !== '');
    throw new UsageError((firstLine ?? error.message).trim());
  }
}
const changedFiles = (root, base, head) =>
  lines(
    gitDiff(root, base, head, [
      '--name-only',
      '--diff-filter=ACMR',
      range(base, head),
    ]),
  );
const treeFiles = (root, head) =>
  lines(git(root, ['ls-tree', '-r', '--name-only', head]));
const showFile = (root, head, path) => git(root, ['show', `${head}:${path}`]);
function commitsIn(root, base, head) {
  return git(root, ['log', '--format=%s%x00%b%x1e', `${base}..${head}`])
    .split('\x1e')
    .map((s) => s.replace(/^\n/, ''))
    .filter((s) => s.length > 0)
    .map((s) => {
      const [subject, body = ''] = s.split('\x00');
      return { subject, body };
    });
}
const removedLines = (root, base, head, files) =>
  gitDiff(root, base, head, [range(base, head), '--', ...files])
    .split('\n')
    .filter((l) => l.startsWith('-') && !l.startsWith('---'));
const matchAny = (path, globs) =>
  list(globs).some((glob) => globMatch(path, glob));

/** Reads a JSON deliverable file; a missing or malformed file is a usage error. */
function readDeliverable(path) {
  try {
    return readJson(resolve(path));
  } catch (error) {
    throw new UsageError(error.message);
  }
}
/** Reads the value at a dot-separated `field` path, or `undefined` if any segment is missing. */
const fieldValue = (object, field) =>
  field
    .split('.')
    .reduce((node, key) => (node == null ? undefined : node[key]), object);

/**
 * Lists a workspace directory's deliverable filenames by kind, each sorted:
 * `audits` (`task-*-audit*.json`), `reports` (`task-<n>-report.json`), and
 * `reviews` (`task-*-review*.json`). Shared by `stats` and `audit --workspace`.
 */
function workspaceFiles(dir) {
  let names;
  try {
    names = readdirSync(dir);
  } catch (error) {
    throw new UsageError(error.message);
  }
  return {
    audits: names.filter((n) => /^task-.+-audit.*\.json$/.test(n)).toSorted(),
    reports: names
      .filter((n) => /^task-\d+-report\.json$/.test(n))
      .toSorted(),
    reviews: names
      .filter((n) => /^task-.+-review.*\.json$/.test(n))
      .toSorted(),
  };
}

// ---- audit --------------------------------------------------------------------------

function runCheck(entry, ctx) {
  const c = entry.check;
  const row = {
    id: entry.id,
    kind: entry.kind,
    mode: 'deterministic',
    level: c.level,
    result: 'pass',
    evidence: '',
  };
  const violate = (evidence) => {
    row.result = c.level === 'warn' ? 'warn' : 'fail';
    row.evidence = evidence;
    return row;
  };
  // Strip `g`/`y`: a check's subject/body/pattern regex is built once and
  // reused with `.test()` across every commit or file in a loop; a global
  // or sticky flag would make it stateful via `lastIndex`, silently
  // skipping matches after the first.
  const re = (source) =>
    new RegExp(source, (c.flags ?? '').replace(/[gy]/g, ''));
  switch (c.type) {
    case 'grep-absent': {
      const pool = c.scope === 'tree' ? ctx.tree() : ctx.changed;
      const files = pool.filter((p) => matchAny(p, c.files));
      const pattern = re(c.pattern);
      for (const path of files) {
        const text = ctx.show(path);
        const match = pattern.exec(text);
        if (match)
          return violate(
            `${path}:${text.slice(0, match.index).split('\n').length} matches ${c.pattern}`,
          );
      }
      row.evidence = `${files.length} files checked`;
      return row;
    }
    case 'commits': {
      const subject = c.subject ? re(c.subject) : null;
      const body = c.body_absent ? re(c.body_absent) : null;
      for (const commit of ctx.commits()) {
        if (subject && !subject.test(commit.subject))
          return violate(
            `commit "${commit.subject}" does not match ${c.subject}`,
          );
        if (body && commit.body.split('\n').some((line) => body.test(line)))
          return violate(
            `commit "${commit.subject}" body matches ${c.body_absent}`,
          );
        if (
          c.body_line_max &&
          commit.body
            .split('\n')
            .some((line) => line.length > c.body_line_max)
        )
          return violate(
            `commit "${commit.subject}" has a body line over ${c.body_line_max} characters`,
          );
      }
      row.evidence = `${ctx.commits().length} commits checked`;
      return row;
    }
    case 'co-change': {
      const trigger = ctx.changed.filter((p) => matchAny(p, c.if));
      if (trigger.length === 0) {
        row.evidence = 'not triggered';
        return row;
      }
      const satisfying = ctx.changed.find((p) => matchAny(p, c.then));
      if (satisfying) {
        const realTrigger = trigger.find((p) => !matchAny(p, c.then));
        row.evidence = realTrigger
          ? `${realTrigger} changed with ${satisfying}`
          : `only ${trigger.join(', ')} changed; the co-change is satisfied by definition`;
        return row;
      }
      return violate(
        `${trigger[0]} changed without ${list(c.then).join(' or ')}`,
      );
    }
    case 'diff-append-only': {
      const files = ctx.changed.filter((p) => matchAny(p, c.files));
      if (files.length === 0) {
        row.evidence = 'not triggered';
        return row;
      }
      const removed = ctx.removed(files);
      if (removed.length)
        return violate(
          `${removed.length} removed lines in ${files.join(', ')}`,
        );
      row.evidence = `${files.join(', ')}: no removed lines`;
      return row;
    }
    case 'report-field': {
      const trigger = ctx.changed.filter((p) => matchAny(p, c.if));
      if (trigger.length === 0) {
        row.evidence = 'not triggered';
        return row;
      }
      const hasField = (data) => {
        const value = fieldValue(data, c.field);
        return value !== undefined && value !== null;
      };
      if (ctx.report !== null) {
        if (hasField(ctx.report)) {
          row.evidence = `report field ${c.field} is set`;
          return row;
        }
        return violate(
          `report lacks a value for ${c.field} (triggered by ${trigger[0]})`,
        );
      }
      if (ctx.reports !== null) {
        const malformed = ctx.reports.find((r) => r.data.files_changed == null);
        if (malformed) return violate(`${malformed.name} lacks files_changed`);
        const hits = ctx.reports.filter((r) =>
          r.data.files_changed.some((f) => matchAny(f, c.if)),
        );
        if (hits.length === 0) {
          row.evidence = 'not triggered by any report';
          return row;
        }
        const missing = hits.find((r) => !hasField(r.data));
        if (missing) {
          const file = missing.data.files_changed.find((f) =>
            matchAny(f, c.if),
          );
          return violate(
            `${missing.name} lacks a value for ${c.field} (triggered by ${file})`,
          );
        }
        row.evidence = `report field ${c.field} is set in ${hits.length} reports`;
        return row;
      }
      row.result = 'skipped';
      row.evidence = 'no --report given';
      return row;
    }
  }
}

/**
 * Builds the rule package for a git range — every standing rule, every rule
 * or invariant in a touched or global area, every entry of any kind that
 * carries a `check` in a touched or global area, plus any `--ids` addition
 * — runs each member's deterministic check, and reports the result and
 * whether any check failed. `report` and `workspace` are exclusive:
 * `report` names one JSON deliverable a `report-field` check reads
 * directly; `workspace` names a directory of `task-<n>-report.json` files
 * a `report-field` check judges by each report's `files_changed`.
 * `report`, `workspace`, and `json` are paths resolved against `cwd`
 * (default `process.cwd()`); the result carries `area_files`, the changed
 * files that pulled in each area. When the range holds no commits, the
 * summary gains `empty_range: true` and every deterministic row's evidence
 * is prefixed `empty range:`, so a vacuous audit never reads as clean
 * evidence.
 */
export function audit(
  base,
  {
    baseRef,
    headRef = 'HEAD',
    ids = [],
    report,
    workspace,
    json,
    cwd = process.cwd(),
  } = {},
) {
  if (!baseRef) throw new UsageError('audit needs --base <ref>');
  if (report && workspace)
    throw new UsageError('audit takes --report or --workspace, not both');
  const { root } = base;
  const baseSha = rev(root, baseRef);
  const headSha = rev(root, headRef);
  const changed = changedFiles(root, baseSha, headSha);
  const areaFileMap = areaFiles(changed, base.areas);
  const areas = Object.keys(areaFileMap).toSorted();
  const pkg = new Map();
  for (const e of base.entries.values()) {
    if (
      e.standing ||
      (RULE_KINDS.includes(e.kind) && areas.includes(e.area)) ||
      (e.check && areas.includes(e.area))
    )
      pkg.set(e.id, e);
  }
  for (const id of ids) {
    const e = base.entries.get(id);
    if (!e) throw new UsageError(`unknown id "${id}"`);
    pkg.set(id, e);
  }
  const cache = new Map();
  let tree;
  let commitList;
  const ctx = {
    changed,
    report: report ? readDeliverable(resolve(cwd, report)) : null,
    reports: workspace
      ? workspaceFiles(resolve(cwd, workspace)).reports.map((name) => ({
          name,
          data: readDeliverable(resolve(cwd, workspace, name)),
        }))
      : null,
    show: (path) => {
      if (!cache.has(path)) cache.set(path, showFile(root, headSha, path));
      return cache.get(path);
    },
    tree: () => (tree ??= treeFiles(root, headSha)),
    commits: () => (commitList ??= commitsIn(root, baseSha, headSha)),
    removed: (files) => removedLines(root, baseSha, headSha, files),
  };
  const rows = [...pkg.values()].toSorted(byId).map((e) =>
    e.check
      ? runCheck(e, ctx)
      : {
          id: e.id,
          kind: e.kind,
          mode: 'judged',
          level: null,
          result: 'open',
          evidence: '—',
        },
  );
  const det = rows.filter((r) => r.mode === 'deterministic');
  const count = (result) => det.filter((r) => r.result === result).length;
  const emptyRange = ctx.commits().length === 0;
  if (emptyRange)
    for (const row of det) row.evidence = `empty range: ${row.evidence}`;
  const summary = {
    base: baseSha,
    head: headSha,
    deterministic: det.length,
    pass: count('pass'),
    fail: count('fail'),
    warn: count('warn'),
    skipped: count('skipped'),
    judged: rows.length - det.length,
    ...(emptyRange ? { empty_range: true } : {}),
  };
  const result = {
    base: baseSha,
    head: headSha,
    ids,
    changed_files: changed,
    areas,
    area_files: areaFileMap,
    rules: rows,
    summary,
  };
  if (json) writeFileSync(resolve(cwd, json), emit(result));
  return { result, failed: rows.some((r) => r.result === 'fail') };
}

/**
 * Validates one deliverable file against the definition its `kind` names in
 * `.claude/schemas/deliverables.json`. Returns the file, its kind, and the
 * list of schema errors (empty when valid).
 */
export function validateDeliverable(root, path) {
  const schema = readJson(join(root, DELIVERABLES_SCHEMA));
  const value = readDeliverable(path);
  const def = DELIVERABLE_KINDS[value?.kind];
  if (!def)
    throw new UsageError(
      `${path}: unknown deliverable kind ${JSON.stringify(value?.kind)}`,
    );
  const errors = new Errors();
  validate(value, { $ref: `#/$defs/${def}` }, path, errors, schema);
  return { file: path, kind: value.kind, errors: errors.list };
}

// ---- stats ---------------------------------------------------------------------------

const statsTask = (name) => name.match(/^task-([^-]+)/)[1];
/** Records that task `t` triggered (or violated) rule `id`, deduplicating repeats. */
function statsHit(map, id, t) {
  if (!map.has(id)) map.set(id, new Set());
  map.get(id).add(t);
}
const statsTasks = (set) => [...set].toSorted();

/**
 * Aggregates rule violations and unused injected ids across a workspace's
 * JSON deliverables: `task-*-audit*.json` for injected ids and deterministic
 * failures, `task-*-review*.json` for judged failures, `task-<n>-report.json`
 * for the ids a report cites as used.
 */
export function stats(dir) {
  const { audits, reports, reviews } = workspaceFiles(dir);
  const violations = new Map();
  const injected = new Map();
  for (const name of audits) {
    const data = readDeliverable(join(dir, name));
    for (const id of data.ids ?? []) statsHit(injected, id, statsTask(name));
    for (const rule of data.rules ?? [])
      if (rule.result === 'fail')
        statsHit(violations, rule.id, statsTask(name));
  }
  for (const name of reviews) {
    const data = readDeliverable(join(dir, name));
    for (const row of data.rule_adherence ?? [])
      if (row.mode === 'judged' && row.result === 'fail')
        statsHit(violations, row.id, statsTask(name));
  }
  const cited = new Set();
  for (const name of reports)
    for (const id of readDeliverable(join(dir, name)).knowledge_used ?? [])
      cited.add(id);
  return {
    violations: [...violations.entries()]
      .toSorted()
      .map(([id, set]) => ({ id, count: set.size, tasks: statsTasks(set) })),
    unused_ids: [...injected.entries()]
      .filter(([id]) => !cited.has(id))
      .toSorted()
      .map(([id, set]) => ({ id, tasks: statsTasks(set) })),
    audits: {
      files: audits.length,
      tasks: new Set(audits.map(statsTask)).size,
    },
    reviews: { files: reviews.length },
  };
}

// ---- read commands ------------------------------------------------------------

function filterEntries(base, opts) {
  let entries = [...base.entries.values()];
  if (opts.area) entries = entries.filter((e) => e.area === opts.area);
  if (opts.topic) entries = entries.filter((e) => e.topic === opts.topic);
  if (opts.tag) entries = entries.filter((e) => e.tags.includes(opts.tag));
  if (opts.kind) entries = entries.filter((e) => e.kind === opts.kind);
  if (opts.standing) entries = entries.filter((e) => e.standing);
  return entries.toSorted(byId);
}
const indexRow = (e) => ({
  id: e.id,
  kind: e.kind,
  area: e.area,
  standing: Boolean(e.standing),
  summary: e.summary,
});

/** One row per topic: its name, entry count, and title. */
export function cmdTopics(base) {
  return base.topics.map((t) => ({
    topic: t.name,
    entries: topicEntryCount(t),
    title: t.title,
  }));
}
/** Index rows for entries matching every given filter, sorted by id. */
export function cmdIndex(base, opts) {
  return filterEntries(base, opts).map(indexRow);
}
/** The stored entries (plus `topic`) for the given ids, in the order given. */
export function cmdGet(base, ids) {
  return ids.map((id) => {
    const e = base.entries.get(id);
    if (!e) throw new UsageError(`unknown id "${id}"`);
    return e;
  });
}
/** The rule package a set of changed paths pulls in: their areas' rule-shaped entries, plus every entry whose `verify` names one of the paths. */
export function cmdFor(base, paths, { full = false } = {}) {
  const areas = areasFor(paths, base.areas);
  const wanted = new Set(paths.map(stripDot));
  const entries = [...base.entries.values()]
    .filter(
      (e) =>
        (areas.includes(e.area) && FOR_KINDS.includes(e.kind)) ||
        (e.verify ?? []).some((path) => wanted.has(stripDot(path))),
    )
    .toSorted(byId);
  return {
    paths: paths.map(stripDot),
    areas,
    entries: full ? entries : entries.map(indexRow),
    standing: STANDING_COMMAND,
  };
}
/** The standing rules, as `{ id, summary }`, rules before invariants. */
export function cmdStanding(base) {
  return standingEntries(base).map((e) => ({ id: e.id, summary: e.summary }));
}

// Options the design (§7.2) declares bracket-only, with no value: `index
// [--standing]`, `for … [--full]`, `render [--check]`. Every other `--flag`
// takes the next non-flag token as its value when one follows.
const BOOLEAN_OPTS = new Set(['full', 'standing', 'check']);

const USAGE =
  'usage: kb <topics|index|get|for|standing|render|check|audit|stats|validate> [options]\n' +
  'audit --base <ref> [--head <ref>] [--ids <id,id>] [--report <file> | --workspace <dir>] [--json <file>]';

/** Parses argv, dispatches to the matching command, and writes its result through `io`. */
export function main(argv, io, cwd) {
  const [command, ...rest] = argv;
  const { positional, opts } = parseArgs(rest, BOOLEAN_OPTS);
  try {
    const base = loadBase(repoRoot(cwd));
    switch (command) {
      case 'topics':
        io.out(emit(cmdTopics(base)));
        return 0;
      case 'index':
        io.out(emit(cmdIndex(base, opts)));
        return 0;
      case 'get':
        if (!positional.length)
          throw new UsageError('get needs at least one id');
        io.out(emit(cmdGet(base, positional)));
        return 0;
      case 'for':
        if (!positional.length)
          throw new UsageError('for needs at least one path');
        io.out(emit(cmdFor(base, positional, { full: opts.full === true })));
        return 0;
      case 'standing':
        io.out(emit(cmdStanding(base)));
        return 0;
      case 'render': {
        const stale = render(base, { check: opts.check === true });
        if (opts.check === true && stale.length) {
          io.err(stale.map((p) => `${p}: would change\n`).join(''));
          return 1;
        }
        io.out(
          stale.length && opts.check !== true
            ? stale.map((p) => `${p}: written\n`).join('')
            : 'render: up to date\n',
        );
        return 0;
      }
      case 'check': {
        const errors = checkBase(base);
        if (errors.length) {
          io.err(errors.map((e) => `${e}\n`).join(''));
          return 1;
        }
        io.out('knowledge: ok\n');
        return 0;
      }
      case 'audit': {
        const { result, failed } = audit(base, {
          baseRef: typeof opts.base === 'string' ? opts.base : undefined,
          headRef: typeof opts.head === 'string' ? opts.head : 'HEAD',
          ids:
            typeof opts.ids === 'string'
              ? opts.ids
                  .split(',')
                  .map((id) => id.trim())
                  .filter(Boolean)
              : [],
          report: typeof opts.report === 'string' ? opts.report : undefined,
          workspace:
            typeof opts.workspace === 'string' ? opts.workspace : undefined,
          json: typeof opts.json === 'string' ? opts.json : undefined,
          cwd,
        });
        io.out(emit(result));
        return failed ? 1 : 0;
      }
      case 'stats':
        if (positional.length !== 1)
          throw new UsageError('stats needs one workspace directory');
        io.out(emit(stats(resolve(cwd, positional[0]))));
        return 0;
      case 'validate': {
        if (!positional.length)
          throw new UsageError('validate needs at least one file');
        const results = positional.map((p) =>
          validateDeliverable(base.root, resolve(cwd, p)),
        );
        io.out(emit(results));
        return results.some((r) => r.errors.length) ? 1 : 0;
      }
      default:
        throw new UsageError(USAGE);
    }
  } catch (error) {
    if (error instanceof UsageError) {
      io.err(`${error.message}\n`);
      return 2;
    }
    throw error;
  }
}

if (isMainModule(import.meta.url)) {
  process.exitCode = main(
    process.argv.slice(2),
    {
      out: (s) => process.stdout.write(s),
      err: (s) => process.stderr.write(s),
    },
    process.cwd(),
  );
}
