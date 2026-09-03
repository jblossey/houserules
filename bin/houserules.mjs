#!/usr/bin/env node
// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Jannis Blossey
// houserules: installs and updates the knowledge-management setup in a project
// repository. Design record: docs/design.md in this repository.
import { execFileSync } from 'node:child_process';
import {
  chmodSync,
  existsSync,
  mkdirSync,
  readFileSync,
  writeFileSync,
} from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { UsageError, isMainModule, parseArgs } from '../template/tools/lib/cli.mjs';
import { emit, readJson } from '../template/tools/lib/json-store.mjs';

const TEMPLATE_DIR = fileURLToPath(new URL('../template/', import.meta.url));
const VERSION = JSON.parse(
  readFileSync(new URL('../package.json', import.meta.url), 'utf8'),
).version;

/**
 * Machinery files houserules owns: `init` writes them and `update` overwrites
 * them. A project edit to one of these is lost on the next update — project
 * customization belongs in the seed-once files instead.
 */
export const KIT_OWNED = [
  'tools/kb.mjs',
  'tools/backlog.mjs',
  'tools/kb.sh',
  'tools/backlog.sh',
  'tools/claude-session-start.sh',
  '.githooks/commit-msg',
  'tools/lib/cli.mjs',
  'tools/lib/json-store.mjs',
  '.claude/agents/implementer.md',
  '.claude/agents/task-reviewer.md',
  '.claude/agents/branch-reviewer.md',
  '.claude/skills/orchestrating/SKILL.md',
  '.claude/skills/finishing-a-feature/SKILL.md',
  '.claude/skills/migrating-knowledge/SKILL.md',
];

/**
 * Project-data files `init` seeds once and never touches again: schemas,
 * knowledge topics, the backlog, evals, CI wiring, and the starter CLAUDE.md.
 * The project owns them from the first write on; `update` skips them.
 */
export const SEED_ONCE = [
  'knowledge/schema.json',
  'knowledge/areas.json',
  'knowledge/process.json',
  'knowledge/quality.json',
  'knowledge/security-hygiene.json',
  'knowledge/writing-style.json',
  'knowledge/knowledge-base.json',
  'backlog/schema.json',
  'backlog/amendments.json',
  'backlog/batches.json',
  'backlog/decisions.json',
  'backlog/parked.json',
  'backlog/items/general.json',
  '.claude/schemas/deliverables.json',
  '.claude/evals/dependency-add.json',
  '.claude/evals/docs-edit.json',
  '.claude/evals/record.json',
  '.claude/evals/seeded-violations.json',
  '.github/workflows/knowledge.yml',
  'CLAUDE.md',
];

// Seed files that carry the backlog id prefix; --id-prefix rewrites them.
const PREFIXED = new Set([
  'backlog/schema.json',
  'backlog/items/general.json',
  '.claude/schemas/deliverables.json',
]);

const USAGE = 'usage: houserules <init|update|files> [--dir <target>] [--id-prefix <PREFIX>]';

/** The shared constraint text for both `idPrefix` rejection messages. */
const ID_PREFIX_HINT = 'must be 1-8 characters, A-Z then A-Z0-9';

/**
 * True when `value` is a valid backlog id prefix: one uppercase letter
 * followed by up to seven more uppercase letters or digits (1-8 characters
 * total). Shared by the `--id-prefix` flag and the `.houserules.json`
 * stamp's `idPrefix`, so a hand-edited stamp can never carry a prefix the
 * flag would refuse.
 */
function isIdPrefix(value) {
  return typeof value === 'string' && /^[A-Z][A-Z0-9]{0,7}$/.test(value);
}

/** Reads one template file, rewriting the backlog id prefix where it applies. */
function templateContent(file, prefix) {
  const text = readFileSync(join(TEMPLATE_DIR, file), 'utf8');
  if (!PREFIXED.has(file) || prefix === 'WI') return text;
  return text.replaceAll('WI-', `${prefix}-`);
}

/** Writes `content` to `file` under `target`, keeping shell scripts and git hooks executable. */
function writeInto(target, file, content) {
  const path = join(target, file);
  mkdirSync(dirname(path), { recursive: true });
  writeFileSync(path, content);
  if (file.endsWith('.sh') || file.startsWith('.githooks/')) chmodSync(path, 0o755);
}

/**
 * Reads and parses `path` as JSON, requiring the result to be a plain
 * object: not `null`, a string, a number, a boolean, or an array. Raises
 * `UsageError('<path>: not a JSON object')` for any other shape. `readJson`
 * itself raises `UsageError` for invalid JSON and a plain `Error` when the
 * file cannot be read.
 */
function readJsonObject(path) {
  const value = readJson(path);
  if (value === null || typeof value !== 'object' || Array.isArray(value))
    throw new UsageError(`${path}: not a JSON object`);
  return value;
}

/**
 * Merges the template's SessionStart hooks into an existing settings.json,
 * appending only entries whose `matcher` is not already present. Returns
 * true when the merge changed the file.
 */
function mergeSettings(target) {
  const path = join(target, '.claude/settings.json');
  const template = JSON.parse(
    readFileSync(join(TEMPLATE_DIR, '.claude/settings.json'), 'utf8'),
  );
  const settings = readJsonObject(path);
  settings.hooks ??= {};
  settings.hooks.SessionStart ??= [];
  const matchers = new Set(settings.hooks.SessionStart.map((e) => e.matcher));
  let changed = false;
  for (const entry of template.hooks.SessionStart) {
    if (matchers.has(entry.matcher)) continue;
    settings.hooks.SessionStart.push(entry);
    changed = true;
  }
  if (changed) writeFileSync(path, emit(settings));
  return changed;
}

/**
 * Runs `tools/kb.mjs render` in `target` and returns its stdout. The piped
 * stdio keeps the child's stderr out of the parent's output. A failed render
 * throws `UsageError` with the child's stderr, or with the spawn error's
 * message when no child ran. When that text is empty, the message is the
 * fixed `tools/kb.mjs render failed`.
 */
function renderIn(target) {
  try {
    return execFileSync(process.execPath, [join(target, 'tools/kb.mjs'), 'render'], {
      cwd: target,
      encoding: 'utf8',
      stdio: ['ignore', 'pipe', 'pipe'],
    });
  } catch (error) {
    throw new UsageError(
      (error.stderr ?? error.message).trim() || 'tools/kb.mjs render failed',
    );
  }
}

/**
 * Installs the setup into `target`. Reads and validates the existing
 * `.houserules.json` stamp before writing anything, so a corrupt stamp
 * leaves `target` untouched: raises `UsageError` when the stamp is not a
 * JSON object, when its `idPrefix` is present but fails `isIdPrefix`, or
 * when its `version` is present but is not a non-empty string.
 * Then writes kit-owned files always, seed-once files only when `seed` is
 * set and the file is absent, restamps `.houserules.json`, and renders the
 * generated markdown. An `update` call (`seed` false) also prints the
 * stamped-to-running version drift as one `kit <stamped> -> <running>`
 * line, reusing the stamp already read above; an unchanged version prints
 * the same shape with both sides equal, and an absent `version` prints the
 * designed token `none` rather than a fabricated version.
 */
function install(io, opts, { seed }, cwd) {
  const target = resolve(cwd, typeof opts.dir === 'string' ? opts.dir : '.');
  if (!existsSync(join(target, '.git')))
    throw new UsageError(`${target} is not a git repository (run git init first)`);
  const prefix = typeof opts['id-prefix'] === 'string' ? opts['id-prefix'] : 'WI';
  if (!isIdPrefix(prefix)) throw new UsageError(`id-prefix ${ID_PREFIX_HINT}`);
  const markerPath = join(target, '.houserules.json');
  const marker = existsSync(markerPath) ? readJsonObject(markerPath) : { idPrefix: prefix };
  if (marker.idPrefix !== undefined && !isIdPrefix(marker.idPrefix))
    throw new UsageError(`${markerPath}: idPrefix ${ID_PREFIX_HINT}`);
  if (marker.version !== undefined && (typeof marker.version !== 'string' || marker.version === ''))
    throw new UsageError(`${markerPath}: version must be a non-empty string`);
  for (const file of KIT_OWNED) {
    writeInto(target, file, templateContent(file, prefix));
    io.out(`wrote ${file}\n`);
  }
  if (seed) {
    for (const file of SEED_ONCE) {
      if (existsSync(join(target, file))) {
        io.out(`kept ${file}\n`);
        continue;
      }
      writeInto(target, file, templateContent(file, prefix));
      io.out(`wrote ${file}\n`);
    }
    if (existsSync(join(target, '.claude/settings.json'))) {
      io.out(
        mergeSettings(target)
          ? 'merged .claude/settings.json (SessionStart hooks added)\n'
          : 'kept .claude/settings.json (hooks already present)\n',
      );
    } else {
      writeInto(target, '.claude/settings.json', templateContent('.claude/settings.json', prefix));
      io.out('wrote .claude/settings.json\n');
    }
  }
  const stampedVersion = marker.version ?? 'none';
  writeFileSync(markerPath, emit({ version: VERSION, idPrefix: marker.idPrefix ?? prefix }));
  io.out(renderIn(target));
  if (!seed) io.out(`kit ${stampedVersion} -> ${VERSION}\n`);
  io.out(`houserules: ${seed ? 'initialized' : 'updated'} ${target}\n`);
  io.out('next: tools/kb.sh check && tools/backlog.sh check\n');
  return 0;
}

/** Parses argv, dispatches to the matching command, and writes its result through `io`. `--dir` resolves against `cwd`. */
export function main(argv, io, cwd = process.cwd()) {
  const [command, ...rest] = argv;
  const { opts } = parseArgs(rest);
  try {
    switch (command) {
      case 'init':
        return install(io, opts, { seed: true }, cwd);
      case 'update':
        return install(io, opts, { seed: false }, cwd);
      case 'files':
        io.out(emit({ kitOwned: KIT_OWNED, seedOnce: SEED_ONCE }));
        return 0;
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
  process.exitCode = main(process.argv.slice(2), {
    out: (s) => process.stdout.write(s),
    err: (s) => process.stderr.write(s),
  });
}
