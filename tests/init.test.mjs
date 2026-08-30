import {
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  readdirSync,
  rmSync,
  writeFileSync,
} from 'node:fs';
import { fileURLToPath } from 'node:url';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { execFileSync } from 'node:child_process';
import { describe, expect, it } from 'vitest';
import { KIT_OWNED, SEED_ONCE, main } from '../bin/lorekit.mjs';
import { UsageError } from '../template/tools/lib/cli.mjs';

/** A fresh temporary git repository to initialize into. */
function makeTarget() {
  const dir = mkdtempSync(join(tmpdir(), 'lorekit-target-'));
  execFileSync('git', ['init', '-q', '-b', 'main', dir]);
  return dir;
}
function capture() {
  const io = {
    stdout: '',
    stderr: '',
    out: (s) => (io.stdout += s),
    err: (s) => (io.stderr += s),
  };
  return io;
}
/** Runs a target project's own CLI and returns its exit code. */
function runTool(dir, tool, ...args) {
  try {
    execFileSync(process.execPath, [join(dir, 'tools', tool), ...args], {
      cwd: dir,
      encoding: 'utf8',
    });
    return 0;
  } catch (error) {
    return error.status;
  }
}

describe('manifest', () => {
  it('separates kit-owned machinery from seed-once project data', () => {
    expect(KIT_OWNED).toContain('tools/kb.mjs');
    expect(KIT_OWNED).toContain('.claude/agents/implementer.md');
    expect(KIT_OWNED).toContain('.claude/skills/orchestrating/SKILL.md');
    expect(SEED_ONCE).toContain('knowledge/schema.json');
    expect(SEED_ONCE).toContain('backlog/schema.json');
    expect(SEED_ONCE).toContain('.claude/schemas/deliverables.json');
    expect(SEED_ONCE).toContain('CLAUDE.md');
    expect(KIT_OWNED.filter((f) => SEED_ONCE.includes(f))).toEqual([]);
  });
  it('covers every template file, with settings.json handled specially', () => {
    const templateDir = fileURLToPath(new URL('../template/', import.meta.url));
    const actual = readdirSync(templateDir, {
      recursive: true,
      withFileTypes: true,
    })
      .filter((d) => d.isFile())
      .map((d) => join(d.parentPath.slice(templateDir.length), d.name))
      .toSorted();
    expect(actual).toEqual(
      [...KIT_OWNED, ...SEED_ONCE, '.claude/settings.json'].toSorted(),
    );
  });
});

describe('init', () => {
  it('installs the full setup into a fresh git repo and its gates pass', () => {
    const dir = makeTarget();
    const io = capture();
    expect(main(['init', '--dir', dir], io)).toBe(0);
    for (const file of [...KIT_OWNED, ...SEED_ONCE])
      expect([file, existsSync(join(dir, file))]).toEqual([file, true]);
    // init ran render: the generated rule files exist and check passes.
    expect(existsSync(join(dir, '.claude/rules/standing-rules.md'))).toBe(true);
    expect(runTool(dir, 'kb.mjs', 'check')).toBe(0);
    expect(runTool(dir, 'backlog.mjs', 'check')).toBe(0);
    const marker = JSON.parse(readFileSync(join(dir, '.lorekit.json'), 'utf8'));
    expect(marker.idPrefix).toBe('WI');
    expect(marker.version).toMatch(/^\d+\.\d+\.\d+$/);
    expect(io.stdout).toContain('wrote tools/kb.mjs\n');
  });
  it('rewrites the backlog id prefix in the seeded schemas and items', () => {
    const dir = makeTarget();
    expect(main(['init', '--dir', dir, '--id-prefix', 'TG'], capture())).toBe(
      0,
    );
    expect(readFileSync(join(dir, 'backlog/schema.json'), 'utf8')).toContain(
      'TG-\\\\d{3}',
    );
    const items = JSON.parse(
      readFileSync(join(dir, 'backlog/items/general.json'), 'utf8'),
    );
    expect(items.items[0].id).toBe('TG-001');
    expect(
      readFileSync(join(dir, '.claude/schemas/deliverables.json'), 'utf8'),
    ).toContain('TG-[0-9]+');
    expect(runTool(dir, 'backlog.mjs', 'check')).toBe(0);
    expect(
      JSON.parse(readFileSync(join(dir, '.lorekit.json'), 'utf8')).idPrefix,
    ).toBe('TG');
  });
  it('keeps an existing CLAUDE.md and merges hooks into an existing settings.json', () => {
    const dir = makeTarget();
    writeFileSync(join(dir, 'CLAUDE.md'), '# Mine\n');
    mkdirSync(join(dir, '.claude'), { recursive: true });
    writeFileSync(
      join(dir, '.claude/settings.json'),
      JSON.stringify({ permissions: { allow: ['Bash(ls:*)'] } }, null, 2),
    );
    const io = capture();
    expect(main(['init', '--dir', dir], io)).toBe(0);
    expect(readFileSync(join(dir, 'CLAUDE.md'), 'utf8')).toBe('# Mine\n');
    expect(io.stdout).toContain('kept CLAUDE.md\n');
    const settings = JSON.parse(
      readFileSync(join(dir, '.claude/settings.json'), 'utf8'),
    );
    expect(settings.permissions).toEqual({ allow: ['Bash(ls:*)'] });
    expect(settings.hooks.SessionStart).toHaveLength(2);
    // Running init again adds nothing: the merge is idempotent.
    expect(main(['init', '--dir', dir], capture())).toBe(0);
    const again = JSON.parse(
      readFileSync(join(dir, '.claude/settings.json'), 'utf8'),
    );
    expect(again.hooks.SessionStart).toHaveLength(2);
  });
  it('refuses a target that is not a git repository', () => {
    const dir = mkdtempSync(join(tmpdir(), 'lorekit-nogit-'));
    const io = capture();
    expect(main(['init', '--dir', dir], io)).toBe(2);
    expect(io.stderr).toContain('not a git repository');
  });
  it('defaults the target to the given cwd', () => {
    const dir = makeTarget();
    expect(main(['init'], capture(), dir)).toBe(0);
    expect(existsSync(join(dir, 'tools/kb.mjs'))).toBe(true);
  });
  it('reports an unreadable settings.json as a usage error', () => {
    const dir = makeTarget();
    mkdirSync(join(dir, '.claude'), { recursive: true });
    writeFileSync(join(dir, '.claude/settings.json'), '{');
    const io = capture();
    expect(main(['init', '--dir', dir], io)).toBe(2);
    expect(io.stderr).toContain('settings.json');
  });
  it('lets a render failure on broken project data propagate as a real error', () => {
    const dir = makeTarget();
    mkdirSync(join(dir, 'knowledge'), { recursive: true });
    writeFileSync(join(dir, 'knowledge/process.json'), '{');
    expect(() => main(['init', '--dir', dir], capture())).toThrow();
  });
  it('refuses a malformed id prefix', () => {
    const dir = makeTarget();
    const io = capture();
    expect(main(['init', '--dir', dir, '--id-prefix', 'x!'], io)).toBe(2);
    expect(io.stderr).toContain('id-prefix');
  });
});

describe('update', () => {
  it('overwrites kit-owned files only, leaving project data and deletions alone', () => {
    const dir = makeTarget();
    expect(main(['init', '--dir', dir], capture())).toBe(0);
    writeFileSync(join(dir, 'tools/kb.mjs'), '// clobbered\n');
    const processTopic = join(dir, 'knowledge/process.json');
    const edited = readFileSync(processTopic, 'utf8').replace(
      'How work runs',
      'How OUR work runs',
    );
    writeFileSync(processTopic, edited);
    rmSync(join(dir, '.claude/evals/docs-edit.json'));
    const io = capture();
    expect(main(['update', '--dir', dir], io)).toBe(0);
    expect(readFileSync(join(dir, 'tools/kb.mjs'), 'utf8')).not.toBe(
      '// clobbered\n',
    );
    expect(readFileSync(processTopic, 'utf8')).toBe(edited);
    expect(existsSync(join(dir, '.claude/evals/docs-edit.json'))).toBe(false);
    expect(io.stdout).toContain('wrote tools/kb.mjs\n');
  });
});

describe('update marker', () => {
  it('falls back to the default prefix when the marker lacks one', () => {
    const dir = makeTarget();
    expect(main(['init', '--dir', dir, '--id-prefix', 'TG'], capture())).toBe(
      0,
    );
    writeFileSync(join(dir, '.lorekit.json'), '{"version":"0.0.1"}\n');
    expect(main(['update', '--dir', dir], capture())).toBe(0);
    expect(
      JSON.parse(readFileSync(join(dir, '.lorekit.json'), 'utf8')).idPrefix,
    ).toBe('WI');
  });
});

describe('main', () => {
  it('prints the manifest as JSON for files, and usage for unknown commands', () => {
    let io = capture();
    expect(main(['files'], io)).toBe(0);
    expect(JSON.parse(io.stdout)).toEqual({
      kitOwned: KIT_OWNED,
      seedOnce: SEED_ONCE,
    });
    io = capture();
    expect(main(['bogus'], io)).toBe(2);
    const shebangOut = execFileSync(
      process.execPath,
      [fileURLToPath(new URL('../bin/lorekit.mjs', import.meta.url)), 'files'],
      { encoding: 'utf8' },
    );
    expect(JSON.parse(shebangOut).kitOwned).toEqual(KIT_OWNED);
    expect(io.stderr).toMatch(/^usage: lorekit </);
    expect(new UsageError('x')).toBeInstanceOf(Error);
  });
});
