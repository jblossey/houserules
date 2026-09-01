// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Jannis Blossey
// End-to-end coverage for HR-001: each CLI entry point must recognize itself
// as the process entry point when launched through a symlink, the shape a
// package manager's bin shim or a symlinked package directory produces.
import { execFileSync } from 'node:child_process';
import { mkdtempSync, symlinkSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

const ROOT = fileURLToPath(new URL('../', import.meta.url));

/**
 * Symlinks `file` into a fresh temp directory and runs it there as
 * `process.argv[1]`, mirroring how a package manager launches an installed
 * CLI. Returns the parsed JSON the process wrote to stdout.
 */
function runThroughSymlink(file, args) {
  const dir = mkdtempSync(join(tmpdir(), 'entry-symlink-'));
  const link = join(dir, 'entry-link.mjs');
  symlinkSync(fileURLToPath(new URL(file, import.meta.url)), link);
  const stdout = execFileSync(process.execPath, [link, ...args], {
    cwd: ROOT,
    encoding: 'utf8',
  });
  return JSON.parse(stdout);
}

describe('CLI entry points run through a symlink', () => {
  it('bin/houserules.mjs files prints the manifest', () => {
    const result = runThroughSymlink('../bin/houserules.mjs', ['files']);
    expect(result).toHaveProperty('kitOwned');
  });

  it('template/tools/kb.mjs topics prints the topic list', () => {
    const result = runThroughSymlink('../template/tools/kb.mjs', ['topics']);
    expect(result.map((t) => t.topic)).toContain('houserules');
  });

  it('template/tools/backlog.mjs list prints the item list', () => {
    const result = runThroughSymlink('../template/tools/backlog.mjs', [
      'list',
    ]);
    expect(Array.isArray(result)).toBe(true);
  });
});
