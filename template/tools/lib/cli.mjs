// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Jannis Blossey
// Shared argv parsing, usage-error type, and entry-point guard for the kb,
// backlog, and houserules CLIs. Node built-ins only.
import { realpathSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

/** A CLI input error: reported on stderr with exit code 2, not a crash. */
export class UsageError extends Error {}

/**
 * True when this process was launched directly with the module at
 * `moduleUrl` as its entry point. Resolves both `process.argv[1]` and the
 * module's own path through `realpathSync` before comparing them, so the
 * match still holds when the launch path is a symlink — a package manager's
 * bin link, or a symlinked package directory. False when `argv[1]` is
 * unset, names a path that does not exist, or names a different file.
 */
export function isMainModule(moduleUrl) {
  const entry = process.argv[1];
  if (!entry) return false;
  try {
    return realpathSync(entry) === realpathSync(fileURLToPath(moduleUrl));
  } catch {
    return false;
  }
}

/**
 * Splits `argv` into positional arguments and `--flag`/`--key value`
 * options. An option named in `booleanOpts` never consumes the next token
 * as a value; every other option consumes the next token as its value
 * unless that token is itself a `--flag` or there is none, in which case
 * the option is a bare `true`.
 */
export function parseArgs(argv, booleanOpts = new Set()) {
  const positional = [];
  const opts = {};
  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i];
    if (!arg.startsWith('--')) {
      positional.push(arg);
      continue;
    }
    const name = arg.slice(2);
    const next = argv[i + 1];
    if (
      !booleanOpts.has(name) &&
      next !== undefined &&
      !next.startsWith('--')
    ) {
      opts[name] = next;
      i += 1;
    } else {
      opts[name] = true;
    }
  }
  return { positional, opts };
}
