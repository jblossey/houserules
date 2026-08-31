#!/usr/bin/env node
// Backlog CLI, shipped by houserules. The design record lives in the houserules repository (docs/design.md).
import { readdirSync, writeFileSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import { UsageError, parseArgs } from './lib/cli.mjs';
import {
  Errors,
  emit,
  readJson,
  repoRoot,
  validate,
} from './lib/json-store.mjs';

const STATUSES = ['open', 'partial', 'done', 'dropped'];
const USAGE = 'usage: backlog <get|list|batch|set|check> [options]';

/** Loads every backlog file under `root`, indexing items by id with their section and file. */
export function loadBacklog(root) {
  const dir = join(root, 'backlog');
  const schema = readJson(join(dir, 'schema.json'));
  const amendments = readJson(join(dir, 'amendments.json'));
  const batches = readJson(join(dir, 'batches.json'));
  const decisions = readJson(join(dir, 'decisions.json'));
  const parked = readJson(join(dir, 'parked.json'));
  const sections = readdirSync(join(dir, 'items'))
    .filter((name) => name.endsWith('.json'))
    .toSorted()
    .map((name) => ({
      file: `backlog/items/${name}`,
      name: name.slice(0, -5),
      ...readJson(join(dir, 'items', name)),
    }));
  const items = new Map();
  for (const section of sections) {
    for (const item of Array.isArray(section.items) ? section.items : []) {
      if (item && typeof item.id === 'string' && !items.has(item.id))
        items.set(item.id, {
          ...item,
          section: section.section,
          file: section.file,
        });
    }
  }
  return {
    root,
    dir,
    schema,
    amendments,
    batches,
    decisions,
    parked,
    sections,
    items,
  };
}

/** Validates a loaded backlog against its schema and every cross-file invariant. */
export function checkBacklog(b) {
  const errors = new Errors();
  const warnings = [];
  const check = (value, def, at) =>
    validate(value, { $ref: `#/$defs/${def}` }, at, errors, b.schema);
  check(b.amendments, 'amendmentsFile', 'backlog/amendments.json');
  check(b.batches, 'batchesFile', 'backlog/batches.json');
  check(b.decisions, 'decisionsFile', 'backlog/decisions.json');
  check(b.parked, 'parkedFile', 'backlog/parked.json');
  for (const section of b.sections) {
    const { file, name, ...content } = section;
    check(content, 'itemsFile', file);
    if (content.section !== name)
      errors.add(
        `${file}: section "${content.section}" must equal the file name "${name}"`,
      );
  }
  if (errors.any) return { errors: errors.list, warnings };
  const ids = new Set(b.amendments.amendments.map((a) => a.id));
  for (const group of b.parked.groups)
    for (const p of group.items) ids.add(p.id);
  const seen = new Map();
  for (const section of b.sections) {
    for (const item of section.items) {
      if (seen.has(item.id))
        errors.add(
          `${section.file} ${item.id}: duplicate id (also in ${seen.get(item.id)})`,
        );
      seen.set(item.id, section.file);
      ids.add(item.id);
    }
  }
  for (const section of b.sections) {
    for (const item of section.items) {
      for (const ref of item.see ?? [])
        if (!ids.has(ref))
          errors.add(`${section.file} ${item.id}: see "${ref}" does not exist`);
      if (item.status === 'done' && item.batch == null)
        warnings.push(`${section.file} ${item.id}: done without a batch`);
    }
  }
  const numbers = new Set();
  let inProgress = 0;
  for (const batch of b.batches.batches) {
    for (const id of batch.items)
      if (!b.items.has(id))
        errors.add(
          `backlog/batches.json batch ${batch.number}: item "${id}" does not exist`,
        );
    if (numbers.has(batch.number))
      errors.add(`backlog/batches.json: duplicate batch ${batch.number}`);
    numbers.add(batch.number);
    if (batch.status.state === 'in-progress') inProgress += 1;
  }
  if (inProgress > 1)
    errors.add(
      `backlog/batches.json: ${inProgress} batches in progress (at most one)`,
    );
  return { errors: errors.list, warnings };
}

// ---- formatting --------------------------------------------------------------------

const listRow = (item) => ({
  id: item.id,
  status: item.status,
  milestone: item.milestone ?? null,
  batch: item.batch ?? null,
  title: item.title,
});

/** The stored records (items with `section`/`file`, amendments, or parked items with `batch`) for the given ids. */
export function cmdGet(b, ids) {
  return ids.map((id) => {
    const item = b.items.get(id);
    if (item) return item;
    const amendment = b.amendments.amendments.find((a) => a.id === id);
    if (amendment) return amendment;
    for (const group of b.parked.groups) {
      const parked = group.items.find((p) => p.id === id);
      if (parked) return { ...parked, batch: group.batch };
    }
    throw new UsageError(`unknown id "${id}"`);
  });
}

/** List rows for items matching every given filter. */
export function cmdList(b, opts) {
  let items = [...b.items.values()];
  if (opts.open)
    items = items.filter((i) => i.status === 'open' || i.status === 'partial');
  if (opts.status) items = items.filter((i) => i.status === opts.status);
  if (opts.milestone)
    items = items.filter((i) => (i.milestone ?? '-') === opts.milestone);
  if (opts.section) items = items.filter((i) => i.section === opts.section);
  if (opts.type) items = items.filter((i) => i.type === opts.type);
  if (opts.batch)
    items = items.filter((i) => String(i.batch ?? '') === String(opts.batch));
  return items.map(listRow);
}

/** The batch record with its number, summary, kickoff, status, and item rows. */
export function cmdBatch(b, number) {
  if (!/^\d+$/.test(String(number)))
    throw new UsageError('batch needs a number');
  const batch = b.batches.batches.find((x) => x.number === Number(number));
  if (!batch) throw new UsageError(`unknown batch "${number}"`);
  return {
    number: batch.number,
    summary: batch.summary,
    kickoff: batch.kickoff,
    status: batch.status,
    items: batch.items
      .map((id) => b.items.get(id))
      .filter(Boolean)
      .map(listRow),
  };
}

/** Applies `field=value` assignments to an item's file on disk and reports what changed. */
export function cmdSet(b, id, assignments) {
  if (!id || assignments.length === 0)
    throw new UsageError('set needs <id> and at least one field=value');
  const item = b.items.get(id);
  if (!item) throw new UsageError(`unknown item "${id}"`);
  const changes = {};
  for (const assignment of assignments) {
    const [field, value] = assignment.split('=');
    if (field === 'status') {
      if (!STATUSES.includes(value))
        throw new UsageError(`status must be one of ${STATUSES.join(', ')}`);
      changes.status = value;
    } else if (field === 'batch') {
      if (!/^[1-9]\d*$/.test(value ?? ''))
        throw new UsageError('batch must be a positive integer');
      changes.batch = Number(value);
    } else {
      throw new UsageError(`unknown field "${field}"`);
    }
  }
  const path = join(b.root, item.file);
  const file = readJson(path);
  const target = file.items.find((i) => i.id === id);
  Object.assign(target, changes);
  writeFileSync(path, `${JSON.stringify(file, null, 2)}\n`);
  return `${id}: ${Object.entries(changes)
    .map(([k, v]) => `${k}=${v}`)
    .join(' ')}\n`;
}

// ---- cli -------------------------------------------------------------------------------

/** Parses argv, dispatches to the matching command, and writes its result through `io`. */
export function main(argv, io, cwd) {
  const [command, ...rest] = argv;
  const { positional, opts } = parseArgs(rest);
  try {
    const b = loadBacklog(repoRoot(cwd));
    switch (command) {
      case 'get':
        if (!positional.length)
          throw new UsageError('get needs at least one id');
        io.out(emit(cmdGet(b, positional)));
        return 0;
      case 'list':
        io.out(emit(cmdList(b, opts)));
        return 0;
      case 'batch':
        if (positional.length !== 1)
          throw new UsageError('batch needs one number');
        io.out(emit(cmdBatch(b, positional[0])));
        return 0;
      case 'set':
        io.out(cmdSet(b, positional[0], positional.slice(1)));
        return 0;
      case 'check': {
        const { errors, warnings } = checkBacklog(b);
        for (const warning of warnings) io.out(`warn: ${warning}\n`);
        if (errors.length) {
          io.err(errors.map((e) => `${e}\n`).join(''));
          return 1;
        }
        io.out('backlog: ok\n');
        return 0;
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

if (
  process.argv[1] &&
  import.meta.url === pathToFileURL(resolve(process.argv[1])).href
) {
  process.exitCode = main(
    process.argv.slice(2),
    {
      out: (s) => process.stdout.write(s),
      err: (s) => process.stderr.write(s),
    },
    process.cwd(),
  );
}
