//! The batch-wide argv-closure pass (HR-059's fold-in list, batch 17 T4):
//! one pass over the WHOLE flat surface (spec §3) pinning the bare `--dir`
//! divergence class clap's own value-required error introduces on every
//! command -- a shape the frozen JS never has at all (`tools/kb.mjs`/
//! `tools/backlog.mjs` accept no `--dir`; it is this binary's own
//! testability addition).
//!
//! Every other argv shape (unknown flags, duplicated value flags,
//! unexpected positionals) has its own pin, per command, in that
//! command's own `_parity.rs` file, verified true of every command it
//! names by probing (batch 17 T4 fix round 2, review new_breakage 1 --
//! this sentence twice asserted a command was covered, or exempt, before
//! checking; both times the review or a live probe found it false):
//! `backlog_parity.rs` for `list`/`get`/`batch`/`set`/`check-backlog`,
//! `check_parity.rs` for `check-knowledge`, `validate_stats_audit_parity.rs`
//! for `audit`/`validate`/`stats`, `read_parity.rs` for `index`/`for`/
//! `topics`/`standing`, and `render_parity.rs` for `render`. No command in
//! the flat surface is exempt from this class: `check-knowledge` and
//! `check-backlog` take only `--dir` beyond their own name, which looked
//! exempt (fix round 1's own version of this sentence said so), but
//! probing them the same way as every other command (`tools/kb.mjs check
//! --bogus`/`extra`, `tools/backlog.mjs check --bogus`/`extra`) found the
//! same JS-ignores-it-and-succeeds divergence the rest have; both are
//! pinned now too. This file is only the one shape common to all
//! fourteen.
//!
//! `.superpowers/sdd/2026-09-04-batch-17/task-4-enumerate-argv.sh` is the
//! companion, re-runnable evidence artifact: it runs the bare-`--dir`
//! shape on both engines, with each command's OTHER arguments filled in
//! validly so the comparison is a genuine exit-code divergence (JS
//! ignores `--dir` and succeeds; the binary's own parse fails first) and
//! not merely "both fail, for unrelated reasons" -- the T3 lesson this
//! task's brief names verbatim -- plus every shape named above, each also
//! verified live against the frozen worktree before being pinned in its
//! own `_parity.rs` file. This test itself needs no such fixture: clap's
//! own argument-parsing pass fails on a bare `--dir` before `main` ever
//! dispatches to a command, so the binary's answer (exit 2, the same
//! message, naming that one flag) is unconditional -- true with every
//! other argument omitted, which is what this test actually gives it.
//!
//! `--dir X --dir Y` (both copies carrying a value) needed its own pin
//! too, the residual branch review issue 2 recorded on HR-059 named and
//! sanctioned closing here: a duplicated `--dir` is common to all fourteen
//! commands the same way a bare one is, so it belongs beside it, not in
//! any one command's own `_parity.rs` file. The binary's half needs no
//! fixture either, for the same structural reason as the bare case: `dir:
//! Option<PathBuf>` is a plain, non-multiple clap argument on every
//! variant, and clap's "cannot be used multiple times" refusal fires at
//! parse time, before `main` dispatches, regardless of whether the
//! command's OTHER arguments are even present (verified live for all
//! fourteen with no arguments beyond the two `--dir`s: uniformly exit 2,
//! the identical message naming `--dir <DIR>`). The JS half rests on
//! `tools/lib/cli.mjs`'s own `parseArgs`, read directly rather than
//! inferred: `dir` is not in any command's `booleanOpts`, so each `--dir`
//! occurrence consumes the following token as `opts.dir`'s value,
//! unconditionally overwriting the prior one -- neither occurrence is
//! ever a stray positional, and `opts.dir` itself is never read by any
//! command, so both are silently absorbed regardless of what else is on
//! the line. This test does not additionally claim the JS run then exits
//! 0 overall: seven of the fourteen commands need their own required
//! argument to reach that (audit's own base, get's own id, and so on),
//! already established, and irrelevant to the flag-handling divergence
//! this test and the one above it both pin.

use std::process::Command;

/// A `Command` for the compiled `houserules` binary under test -- this
/// file's own copy of `common::houserules` rather than `mod common;`
/// itself: the shared module's other helpers (`FrozenWorktree`,
/// `copy_dir_recursive`, `repo_root`) go unused here, since a bare `--dir`
/// needs no fixture at all, and pulling in the whole module just to warn
/// about the rest of it as dead code (`render_parity.rs`'s own module doc
/// names this exact tension for a different helper) is worse than a
/// three-line duplicate of the one function this file actually needs.
fn houserules() -> Command {
    Command::new(env!("CARGO_BIN_EXE_houserules"))
}

/// Every flat subcommand name, spec §3's full list as of batch 17 T4.
const COMMANDS: &[&str] = &[
    "render",
    "check-knowledge",
    "get",
    "list",
    "batch",
    "set",
    "check-backlog",
    "audit",
    "validate",
    "stats",
    "index",
    "for",
    "topics",
    "standing",
];

/// `houserules <command> --dir`, no value following: clap's own "a value
/// is required" usage error, exit 2, for every command in the flat
/// surface -- verified live against the frozen source first (the
/// enumeration script above): every one of these instead runs normally
/// under a plain, unqualified `--dir` in JS (there being no such flag to
/// begin with, `--dir` is absorbed as a harmless, ignored unknown option),
/// so this binary's own uniform failure here is the whole of the
/// divergence class, not a coincidence of any one command's own logic.
#[test]
fn bare_dir_exits_2_with_claps_value_required_message_on_every_flat_command() {
    for command in COMMANDS {
        let output = houserules()
            .args([*command, "--dir"])
            .output()
            .unwrap_or_else(|error| panic!("run houserules {command} --dir: {error}"));
        assert_eq!(
            output.status.code(),
            Some(2),
            "{command} --dir: expected exit 2, got {:?}",
            output.status.code()
        );
        let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
        assert!(
            stderr
                .starts_with("error: a value is required for '--dir <DIR>' but none was supplied"),
            "{command} --dir: got {stderr:?}"
        );
    }
}

/// `houserules <command> --dir a --dir b`: clap's own "cannot be used
/// multiple times" usage error, exit 2, for every command in the flat
/// surface -- verified live first: JS's `parseArgs` (`tools/lib/cli.mjs`)
/// consumes each `--dir` occurrence as a value for the same `opts.dir`
/// key, the second overwriting the first, and never reads it, so both are
/// absorbed without error there too (branch review, issue 2 -- the
/// duplicated-`--dir`-with-values residual HR-059 recorded and this
/// review sanctioned closing here).
#[test]
fn duplicated_dir_with_values_exits_2_with_claps_cannot_be_used_multiple_times_message_on_every_flat_command()
 {
    for command in COMMANDS {
        let output = houserules()
            .args([*command, "--dir", "a", "--dir", "b"])
            .output()
            .unwrap_or_else(|error| panic!("run houserules {command} --dir a --dir b: {error}"));
        assert_eq!(
            output.status.code(),
            Some(2),
            "{command} --dir a --dir b: expected exit 2, got {:?}",
            output.status.code()
        );
        let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
        assert!(
            stderr.starts_with("error: the argument '--dir <DIR>' cannot be used multiple times"),
            "{command} --dir a --dir b: got {stderr:?}"
        );
    }
}
