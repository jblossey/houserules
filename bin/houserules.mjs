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
import { emit } from '../template/tools/lib/json-store.mjs';

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
  'tools/lib/cli.mjs',
  'tools/lib/json-store.mjs',
  '.claude/agents/implementer.md',
  '.claude/agents/task-reviewer.md',
  '.claude/agents/branch-reviewer.md',
  '.claude/skills/orchestrating/SKILL.md',
  '.claude/skills/finishing-a-feature/SKILL.md',
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

/** Reads one template file, rewriting the backlog id prefix where it applies. */
function templateContent(file, prefix) {
  const text = readFileSync(join(TEMPLATE_DIR, file), 'utf8');
  if (!PREFIXED.has(file) || prefix === 'WI') return text;
  return text.replaceAll('WI-', `${prefix}-`);
}

/** Writes `content` to `file` under `target`, keeping shell scripts executable. */
function writeInto(target, file, content) {
  const path = join(target, file);
  mkdirSync(dirname(path), { recursive: true });
  writeFileSync(path, content);
  if (file.endsWith('.sh')) chmodSync(path, 0o755);
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
  let settings;
  try {
    settings = JSON.parse(readFileSync(path, 'utf8'));
  } catch (error) {
    throw new UsageError(`.claude/settings.json: not readable JSON (${error.message})`);
  }
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
 * Installs the setup into `target`: kit-owned files always, seed-once files
 * only when `seed` is set and the file is absent. Renders the generated
 * markdown afterwards and stamps `.houserules.json`.
 */
function install(io, opts, { seed }, cwd) {
  const target = resolve(cwd, typeof opts.dir === 'string' ? opts.dir : '.');
  if (!existsSync(join(target, '.git')))
    throw new UsageError(`${target} is not a git repository (run git init first)`);
  const prefix = typeof opts['id-prefix'] === 'string' ? opts['id-prefix'] : 'WI';
  if (!/^[A-Z][A-Z0-9]{0,7}$/.test(prefix))
    throw new UsageError('id-prefix must be 1-8 characters, A-Z then A-Z0-9');
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
  const markerPath = join(target, '.houserules.json');
  const marker = existsSync(markerPath)
    ? JSON.parse(readFileSync(markerPath, 'utf8'))
    : { idPrefix: prefix };
  writeFileSync(markerPath, emit({ version: VERSION, idPrefix: marker.idPrefix ?? prefix }));
  const rendered = execFileSync(
    process.execPath,
    [join(target, 'tools/kb.mjs'), 'render'],
    { cwd: target, encoding: 'utf8' },
  );
  io.out(rendered);
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
