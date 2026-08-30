// Shared argv parsing and usage-error type for the kb and backlog CLIs.
// Node built-ins only.

/** A CLI input error: reported on stderr with exit code 2, not a crash. */
export class UsageError extends Error {}

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
