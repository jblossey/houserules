//! Regenerates the reviewed goldens under `tests/goldens/` from the
//! CURRENT, compiled `houserules` binary. Dev tooling: not part of the
//! flat command surface `crate::main` dispatches, not shipped in
//! `template/` or the payload.
//!
//! # What this writes
//!
//! - `tests/goldens/render/root/**` -- every file `houserules render`
//!   produces for a detached worktree at the frozen sha (this
//!   repository's own, larger knowledge base; area order table-stakes
//!   `render_parity.rs`'s own "root" test still needs).
//! - `tests/goldens/render/mini/**` -- the same, for a scratch copy of
//!   `tests/fixtures/mini` with its checked-in `.claude/rules` and
//!   skill removed first.
//! - `tests/goldens/check/{root,mini,mini-bad,mini-stale}.json` -- one
//!   `{command, cwd, stdout, stderr, exit}` capture per fixture, the
//!   shape `check_parity.rs`'s reader expects.
//! - `tests/goldens/read-parity/{for-tools-kb-mjs,for-tools-kb-mjs-full,
//!   topics,index,index-standing,standing,
//!   get-houserules-template-is-the-source}.json` and `tests/goldens/
//!   read-parity/mini/{for-mini-tools-build-sh,for-mini-tools-build-sh-
//!   full,topics,index,index-standing,standing,
//!   get-mini-build-cache}.json` -- the same capture shape, for every
//!   `houserules` knowledge-read command on the root worktree and the
//!   mini fixture.
//! - `tests/goldens/backlog/{list-open,get-hr-052,batch-14,check}.json`
//!   -- the same shape, for `houserules list --open`/`get HR-052`/`batch
//!   14`/`check-backlog` on the root worktree.
//! - `tests/goldens/backlog/set/mini/command.json` and `tests/goldens/
//!   backlog/set/mini/backlog/items/misc.json` -- the `set` slice's own
//!   capture plus the WRITTEN FILE it produces on a scratch copy of
//!   `mini`, the one slice that mutates a file rather than only printing.
//! - `tests/goldens/validate/{batch14-workspace,task-1-report,
//!   invalid-deliverable,skipped-report}.json` -- byte parity for
//!   `houserules validate`, with each slice's own absolute fixture path
//!   redacted to a `<fixtures>/...` placeholder (`redact_fixture_path`'s
//!   own doc explains why validate alone needs this).
//! - `tests/goldens/stats/{batch14-workspace,stats-workspace}.json` and
//!   `tests/goldens/audit/{validate-terminal-report,
//!   knowledge-retrospective}.json` -- byte parity for `houserules
//!   stats`/`audit`; neither command's own output embeds an absolute
//!   path, so neither needs redaction.
//!
//! Rebuilds `houserules` itself first (`cargo build --bin houserules`): the
//! whole point of this command is to capture what the CURRENT source
//! produces, and a stale binary would silently freeze the previous
//! rewrite's bytes instead.
//!
//! Usage: `cargo run --quiet --bin gen-goldens`. No arguments. Every
//! written path is printed; review the diff before committing.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};

/// The frozen sha every golden here checks out a worktree at -- this
/// binary's own copy of `tests/common/mod.rs`'s `FROZEN_SHA` (same value;
/// that module's own doc has the full provenance account). A
/// `src/bin/*.rs` target cannot depend on `tests/common/mod.rs`: Cargo
/// only compiles that module into `tests/*.rs` integration-test binaries,
/// never into a `[[bin]]` target, the same reason this file already keeps
/// its own `sorted_json_names` rather than importing the test suite's
/// copy.
const FROZEN_SHA: &str = "5f14727b4adeeb347a8d1f0c8f98d929f62bc7f4";

/// A fresh, empty directory under the OS temp root, removed by its own
/// `Drop`. `tempfile` (this crate's `[dev-dependencies]`) is unavailable to
/// a `src/bin/*.rs` target -- Cargo does not link dev-dependencies into a
/// binary target, only into tests -- so this is std-only: a
/// process-id-plus-nanosecond suffix on `std::env::temp_dir()` is unique
/// enough for one process's own scratch directories, the same guarantee
/// `mkdtemp`-style helpers give.
struct ScratchDir(PathBuf);

impl ScratchDir {
    fn new(prefix: &str) -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock is after the epoch")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("{prefix}-{}-{nanos}", std::process::id()));
        fs::create_dir_all(&dir).unwrap_or_else(|error| panic!("mkdir {}: {error}", dir.display()));
        ScratchDir(dir)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for ScratchDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// This checkout's repository root, resolved at compile time from the
/// crate's manifest directory -- every other `src/bin/*.rs` and
/// `tests/*.rs` file's own copy of this helper (`tests/common/mod.rs`'s own
/// doc explains why each keeps its own rather than sharing one: this
/// package has no library target, so a `src/bin/*.rs` file shares no code
/// with `src/main.rs` or with `tests/` at all).
fn repo_root() -> PathBuf {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    root.canonicalize()
        .unwrap_or_else(|error| panic!("canonicalize {}: {error}", root.display()))
}

/// Copies every file under `src` into `dst`, creating directories as
/// needed -- `tests/common/mod.rs`'s own copy of this helper.
fn copy_dir_recursive(src: &Path, dst: &Path) {
    fs::create_dir_all(dst).expect("create destination directory");
    for entry in fs::read_dir(src).expect("read source directory") {
        let entry = entry.expect("read directory entry");
        let dest_path = dst.join(entry.file_name());
        let file_type = entry.file_type().expect("read file type");
        if file_type.is_dir() {
            copy_dir_recursive(&entry.path(), &dest_path);
        } else {
            fs::copy(entry.path(), &dest_path).expect("copy file");
        }
    }
}

/// Builds `houserules` fresh and returns the path to the resulting binary
/// -- this command's whole point is capturing what the CURRENT source
/// produces (this module's own doc explains why a stale binary proves
/// nothing, the same reasoning `houserules.live-run-recipe` states for
/// every live proof). `cargo build` (not `cargo run`) so this process's own
/// stdout/stderr never mixes with the child binary's captured output.
fn build_houserules(root: &Path) -> PathBuf {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
    let status = Command::new(&cargo)
        .args(["build", "--quiet", "--bin", "houserules"])
        .current_dir(root)
        .status()
        .expect("run cargo build --bin houserules");
    assert!(status.success(), "cargo build --bin houserules failed");
    let suffix = std::env::consts::EXE_SUFFIX;
    root.join(format!("target/debug/houserules{suffix}"))
}

/// A detached git worktree at `sha`, removed on drop -- `tests/common/
/// mod.rs`'s `FrozenWorktree`, minus the cross-thread lock this single-
/// threaded command never needs.
struct Worktree {
    repo_root: PathBuf,
    path: PathBuf,
}

impl Worktree {
    fn checkout(repo_root: &Path, sha: &str) -> Self {
        // `git worktree add` wants the path free, the same mkdtemp-then-
        // remove approach `tests/common/mod.rs`'s `FrozenWorktree` uses.
        let holder = ScratchDir::new("houserules-goldens-worktree");
        let path = holder.path().to_path_buf();
        drop(holder);
        let status = Command::new("git")
            .args([
                "-c",
                "core.autocrlf=false",
                "-c",
                "core.eol=lf",
                "worktree",
                "add",
                "--detach",
                "--quiet",
            ])
            .arg(&path)
            .arg(sha)
            .current_dir(repo_root)
            .status()
            .expect("run git worktree add");
        assert!(status.success(), "git worktree add {sha} failed");
        Worktree {
            repo_root: repo_root.to_path_buf(),
            path,
        }
    }
}

impl Drop for Worktree {
    fn drop(&mut self) {
        let status = Command::new("git")
            .args(["worktree", "remove", "--force"])
            .arg(&self.path)
            .current_dir(&self.repo_root)
            .status();
        if !matches!(status, Ok(s) if s.success()) {
            eprintln!(
                "warning: could not remove worktree {}; run `git worktree prune`",
                self.path.display()
            );
        }
    }
}

/// Runs `bin subcommand --dir cwd` (every command this tool freezes takes
/// exactly this shape), returning `(stdout, stderr, exit)`.
fn run_dir(bin: &Path, subcommand: &str, cwd: &Path) -> (String, String, i32) {
    let output = Command::new(bin)
        .arg(subcommand)
        .arg("--dir")
        .arg(cwd)
        .output()
        .unwrap_or_else(|error| panic!("run {subcommand} --dir {}: {error}", cwd.display()));
    (
        String::from_utf8(output.stdout).expect("utf8 stdout"),
        String::from_utf8(output.stderr).expect("utf8 stderr"),
        output.status.code().unwrap_or(-1),
    )
}

/// Every `.claude/rules/*.md` file plus the knowledge skill, present under
/// `root` after a render.
fn rendered_paths(root: &Path) -> Vec<String> {
    let rules_dir = root.join(".claude/rules");
    let mut files: Vec<String> = fs::read_dir(&rules_dir)
        .unwrap_or_else(|error| panic!("read {}: {error}", rules_dir.display()))
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name().into_string().expect("utf8 filename"))
        .filter(|name| name.ends_with(".md"))
        .map(|name| format!(".claude/rules/{name}"))
        .collect();
    files.sort();
    files.push(".claude/skills/project-knowledge/SKILL.md".to_string());
    files
}

/// Runs `houserules render` in `cwd`, then copies every file it produces
/// (`rendered_paths`) to `goldens_dir/<prefix>/<relative path>`.
fn render_and_freeze(bin: &Path, cwd: &Path, goldens_dir: &Path, prefix: &str) -> Vec<String> {
    let (_, stderr, exit) = run_dir(bin, "render", cwd);
    assert_eq!(exit, 0, "render --dir {} failed: {stderr}", cwd.display());
    let mut written = Vec::new();
    for relative in rendered_paths(cwd) {
        let dest = goldens_dir.join("render").join(prefix).join(&relative);
        fs::create_dir_all(dest.parent().expect("dest has a parent")).expect("mkdir golden dir");
        fs::copy(cwd.join(&relative), &dest).expect("copy rendered file to golden");
        written.push(format!("render/{prefix}/{relative}"));
    }
    written
}

/// Runs `houserules check-knowledge` in `cwd`, freezing the
/// `{command, cwd, stdout, stderr, exit}` capture at
/// `goldens_dir/check/<slice>.json`, the shape `check_parity.rs`'s reader
/// expects.
fn check_and_freeze(
    bin: &Path,
    cwd: &Path,
    cwd_label: &str,
    goldens_dir: &Path,
    slice: &str,
) -> String {
    let (stdout, stderr, exit) = run_dir(bin, "check-knowledge", cwd);
    let capture: Value = json!({
        "command": format!("houserules check-knowledge --dir {cwd_label}"),
        "cwd": cwd_label,
        "stdout": stdout,
        "stderr": stderr,
        "exit": exit,
    });
    let dest = goldens_dir.join("check").join(format!("{slice}.json"));
    fs::create_dir_all(dest.parent().expect("dest has a parent")).expect("mkdir golden dir");
    fs::write(
        &dest,
        format!(
            "{}\n",
            serde_json::to_string_pretty(&capture).expect("serialize capture")
        ),
    )
    .expect("write golden check capture");
    format!("check/{slice}.json")
}

/// Runs `houserules for <path> [--full]` in `cwd`, freezing the
/// `{command, cwd, stdout, stderr, exit}` capture at
/// `goldens_dir/read-parity/<slice>.json` -- the same capture shape
/// `check_and_freeze` uses, so `read_parity.rs`'s reader needs no change
/// beyond the path it reads from.
fn for_and_freeze(
    bin: &Path,
    cwd: &Path,
    cwd_label: &str,
    goldens_dir: &Path,
    slice: &str,
    path: &str,
    full: bool,
) -> String {
    let mut args = vec!["for".to_string(), path.to_string()];
    if full {
        args.push("--full".to_string());
    }
    let output = Command::new(bin)
        .args(&args)
        .arg("--dir")
        .arg(cwd)
        .output()
        .unwrap_or_else(|error| panic!("run {args:?} --dir {}: {error}", cwd.display()));
    let capture: Value = json!({
        "command": format!("houserules {} --dir {cwd_label}", args.join(" ")),
        "cwd": cwd_label,
        "stdout": String::from_utf8(output.stdout).expect("utf8 stdout"),
        "stderr": String::from_utf8(output.stderr).expect("utf8 stderr"),
        "exit": output.status.code().unwrap_or(-1),
    });
    let dest = goldens_dir
        .join("read-parity")
        .join(format!("{slice}.json"));
    fs::create_dir_all(dest.parent().expect("dest has a parent")).expect("mkdir golden dir");
    fs::write(
        &dest,
        format!(
            "{}\n",
            serde_json::to_string_pretty(&capture).expect("serialize capture")
        ),
    )
    .expect("write golden read-parity capture");
    format!("read-parity/{slice}.json")
}

/// Serializes `capture` and writes it to `goldens_dir/<relative>`,
/// creating parent directories as needed -- the shared write step every
/// domain below shares with `check_and_freeze`/`for_and_freeze` above,
/// so a change to the write format needs one edit, not one per domain.
fn write_golden(goldens_dir: &Path, relative: &str, capture: &Value) -> String {
    let dest = goldens_dir.join(relative);
    fs::create_dir_all(dest.parent().expect("dest has a parent")).expect("mkdir golden dir");
    fs::write(
        &dest,
        format!(
            "{}\n",
            serde_json::to_string_pretty(capture).expect("serialize capture")
        ),
    )
    .unwrap_or_else(|error| panic!("write {relative}: {error}"));
    relative.to_string()
}

/// Runs `bin <args...> --dir cwd`, returning the `{command, cwd, stdout,
/// stderr, exit}` capture -- backlog's four worktree slices and every
/// remaining knowledge-read command (`topics`/`index`/`index --standing`/
/// `standing`/`get`) share this invocation shape with `check_and_freeze`/
/// `for_and_freeze` above; only the subcommand and its own arguments vary.
fn dir_capture(bin: &Path, args: &[&str], cwd: &Path, cwd_label: &str) -> Value {
    let output = Command::new(bin)
        .args(args)
        .arg("--dir")
        .arg(cwd)
        .output()
        .unwrap_or_else(|error| panic!("run {args:?} --dir {}: {error}", cwd.display()));
    json!({
        "command": format!("houserules {} --dir {cwd_label}", args.join(" ")),
        "cwd": cwd_label,
        "stdout": String::from_utf8(output.stdout).expect("utf8 stdout"),
        "stderr": String::from_utf8(output.stderr).expect("utf8 stderr"),
        "exit": output.status.code().unwrap_or(-1),
    })
}

/// Replaces every occurrence of `fixtures`' own displayed absolute path in
/// `text` with `placeholder` -- kept deliberately simpler than
/// `validate_stats_audit_parity.rs`'s own test-side `redact`: that
/// function also strips a Windows verbatim-disk prefix and un-doubles
/// JSON-escaped backslashes, because it normalizes a LIVE run against an
/// ALREADY-redacted golden on whatever OS a CI runner happens to use.
/// This one runs once, on whoever's own machine invokes `gen-goldens`, to
/// PRODUCE that already-redacted golden in the first place.
fn redact_fixture_path(text: &str, fixtures: &Path, placeholder: &str) -> String {
    text.replace(&fixtures.display().to_string(), placeholder)
}

/// Runs `houserules validate <files...>` with `worktree` as the working
/// directory, redacts `fixtures`' own absolute path out of both stdout
/// and stderr, and freezes the `{command, cwd, stdout, stderr, exit}`
/// capture at `tests/goldens/validate/<slice>.json` -- `validate` is the
/// one of these three commands whose own output embeds the caller's
/// resolved absolute path (each result's `file` field), so it alone needs
/// `redact_fixture_path`.
fn validate_and_freeze(
    bin: &Path,
    worktree: &Path,
    files: &[PathBuf],
    fixtures: &Path,
    placeholder: &str,
    goldens_dir: &Path,
    slice: &str,
) -> String {
    let mut cmd = Command::new(bin);
    cmd.arg("validate");
    for file in files {
        cmd.arg(file);
    }
    cmd.current_dir(worktree);
    let output = cmd.output().expect("run validate");
    let stdout = redact_fixture_path(
        &String::from_utf8(output.stdout).expect("utf8 stdout"),
        fixtures,
        placeholder,
    );
    let stderr = redact_fixture_path(
        &String::from_utf8(output.stderr).expect("utf8 stderr"),
        fixtures,
        placeholder,
    );
    let file_args: Vec<String> = files
        .iter()
        .map(|file| redact_fixture_path(&file.display().to_string(), fixtures, placeholder))
        .collect();
    let capture = json!({
        "command": format!("houserules validate {}", file_args.join(" ")),
        "cwd": "<frozen-worktree>",
        "stdout": stdout,
        "stderr": stderr,
        "exit": output.status.code().unwrap_or(-1),
    });
    write_golden(goldens_dir, &format!("validate/{slice}.json"), &capture)
}

/// Runs `houserules stats <workspace>` with `worktree` as the working
/// directory and freezes the capture at `tests/goldens/stats/<slice>.json`
/// -- unlike `validate`, `stats`' own output never embeds an absolute
/// path (it reports counts and ids, not file locations), so no redaction
/// applies here.
fn stats_and_freeze(
    bin: &Path,
    worktree: &Path,
    workspace: &Path,
    workspace_label: &str,
    goldens_dir: &Path,
    slice: &str,
) -> String {
    let output = Command::new(bin)
        .arg("stats")
        .arg(workspace)
        .current_dir(worktree)
        .output()
        .expect("run stats");
    let capture = json!({
        "command": format!("houserules stats {workspace_label}"),
        "cwd": "<frozen-worktree>",
        "stdout": String::from_utf8(output.stdout).expect("utf8 stdout"),
        "stderr": String::from_utf8(output.stderr).expect("utf8 stderr"),
        "exit": output.status.code().unwrap_or(-1),
    });
    write_golden(goldens_dir, &format!("stats/{slice}.json"), &capture)
}

/// Runs `houserules audit --base base --head head --ids ids` with
/// `worktree` as the working directory and freezes the capture at
/// `tests/goldens/audit/<slice>.json` -- like `stats`, `audit`'s own
/// output names only git-relative paths (`changed_files`), never an
/// absolute one, so no redaction applies here either.
fn audit_and_freeze(
    bin: &Path,
    worktree: &Path,
    base: &str,
    head: &str,
    ids: &str,
    goldens_dir: &Path,
    slice: &str,
) -> String {
    let output = Command::new(bin)
        .args(["audit", "--base", base, "--head", head, "--ids", ids])
        .current_dir(worktree)
        .output()
        .expect("run audit");
    let capture = json!({
        "command": format!("houserules audit --base {base} --head {head} --ids {ids}"),
        "cwd": "<frozen-worktree>",
        "stdout": String::from_utf8(output.stdout).expect("utf8 stdout"),
        "stderr": String::from_utf8(output.stderr).expect("utf8 stderr"),
        "exit": output.status.code().unwrap_or(-1),
    });
    write_golden(goldens_dir, &format!("audit/{slice}.json"), &capture)
}

/// Every `.json` file directly under `dir`, sorted -- `validate_stats_
/// audit_parity.rs`'s own `sorted_json_names`, this binary's independent
/// copy of the same small helper (this crate's `src/bin/*.rs` targets
/// share no code with `tests/`, `tests/common/mod.rs`'s own doc explains
/// why each such helper is duplicated rather than shared).
fn sorted_json_names(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .unwrap_or_else(|error| panic!("read {}: {error}", dir.display()))
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|name| name.ends_with(".json"))
        .collect();
    names.sort();
    names
}

fn main() {
    let root = repo_root();
    let bin = build_houserules(&root);
    let goldens_dir = root.join("tests/goldens");

    let frozen_sha = FROZEN_SHA;

    let mut written = Vec::new();

    // render/root and check/root: the frozen worktree, this repository's
    // own (larger, multi-area) knowledge base as of the corpus's frozen
    // sha -- the same base `render_parity.rs`'s area-order proof and
    // `check_parity.rs`'s root slice have always used.
    {
        let worktree = Worktree::checkout(&root, frozen_sha);
        fs::remove_dir_all(worktree.path.join(".claude/rules")).expect("remove .claude/rules");
        fs::remove_file(
            worktree
                .path
                .join(".claude/skills/project-knowledge/SKILL.md"),
        )
        .expect("remove the knowledge skill");
        written.extend(render_and_freeze(
            &bin,
            &worktree.path,
            &goldens_dir,
            "root",
        ));
        written.push(check_and_freeze(
            &bin,
            &worktree.path,
            "<frozen-worktree>",
            &goldens_dir,
            "root",
        ));
        written.push(for_and_freeze(
            &bin,
            &worktree.path,
            "<frozen-worktree>",
            &goldens_dir,
            "for-tools-kb-mjs",
            "tools/kb.mjs",
            false,
        ));
        written.push(for_and_freeze(
            &bin,
            &worktree.path,
            "<frozen-worktree>",
            &goldens_dir,
            "for-tools-kb-mjs-full",
            "tools/kb.mjs",
            true,
        ));
    }

    // render/mini and check/mini: a scratch copy of the small, synthetic
    // mini fixture, its checked-in `.claude/rules` and skill removed first
    // so the render below proves the golden, not the fixture's own
    // (already-current) copy.
    {
        let mini_src = root.join("tests/fixtures/mini");
        let scratch = ScratchDir::new("houserules-goldens-mini");
        copy_dir_recursive(&mini_src, scratch.path());
        fs::remove_dir_all(scratch.path().join(".claude/rules")).expect("remove .claude/rules");
        fs::remove_file(
            scratch
                .path()
                .join(".claude/skills/project-knowledge/SKILL.md"),
        )
        .expect("remove the knowledge skill");
        written.extend(render_and_freeze(
            &bin,
            scratch.path(),
            &goldens_dir,
            "mini",
        ));
    }
    written.push(check_and_freeze(
        &bin,
        &root.join("tests/fixtures/mini"),
        "tests/fixtures/mini",
        &goldens_dir,
        "mini",
    ));
    written.push(for_and_freeze(
        &bin,
        &root.join("tests/fixtures/mini"),
        "tests/fixtures/mini",
        &goldens_dir,
        "mini/for-mini-tools-build-sh",
        "mini-tools/build.sh",
        false,
    ));
    written.push(for_and_freeze(
        &bin,
        &root.join("tests/fixtures/mini"),
        "tests/fixtures/mini",
        &goldens_dir,
        "mini/for-mini-tools-build-sh-full",
        "mini-tools/build.sh",
        true,
    ));

    // check/mini-bad and check/mini-stale: schema-shape and post-early-
    // return findings, neither ever rendered (check_parity.rs's own doc
    // has the per-fixture account).
    written.push(check_and_freeze(
        &bin,
        &root.join("tests/fixtures/mini-bad"),
        "tests/fixtures/mini-bad",
        &goldens_dir,
        "mini-bad",
    ));
    written.push(check_and_freeze(
        &bin,
        &root.join("tests/fixtures/mini-stale"),
        "tests/fixtures/mini-stale",
        &goldens_dir,
        "mini-stale",
    ));

    // backlog (four worktree slices) and the remaining knowledge-read
    // commands (topics/index/index --standing/standing/get), on a fresh,
    // unmodified worktree at the frozen sha -- the render/check block
    // above deletes `.claude/rules`/the skill from ITS OWN worktree for
    // the render-freshness proof; none of these commands read that
    // directory at all, but a fresh checkout keeps this block
    // independent of that one's mutation rather than relying on it.
    {
        let worktree = Worktree::checkout(&root, frozen_sha);

        for (slice, args) in [
            ("list-open", vec!["list", "--open"]),
            ("get-hr-052", vec!["get", "HR-052"]),
            ("batch-14", vec!["batch", "14"]),
            ("check", vec!["check-backlog"]),
        ] {
            let capture = dir_capture(&bin, &args, &worktree.path, "<frozen-worktree>");
            written.push(write_golden(
                &goldens_dir,
                &format!("backlog/{slice}.json"),
                &capture,
            ));
        }
    }

    // backlog/mini/check.json: the clean path check-backlog's own output
    // needs a golden that can still be clean, since the frozen-worktree
    // slice above pins a failure -- tests/fixtures/mini round-trips
    // byte-for-byte, so it is the fixture that keeps `backlog: ok` pinned.
    written.push(write_golden(
        &goldens_dir,
        "backlog/mini/check.json",
        &dir_capture(
            &bin,
            &["check-backlog"],
            &root.join("tests/fixtures/mini"),
            "tests/fixtures/mini",
        ),
    ));

    {
        let worktree = Worktree::checkout(&root, frozen_sha);
        for (slice, args) in [
            ("topics", vec!["topics"]),
            ("index", vec!["index"]),
            ("index-standing", vec!["index", "--standing"]),
            ("standing", vec!["standing"]),
            (
                "get-houserules-template-is-the-source",
                vec!["get", "houserules.template-is-the-source"],
            ),
        ] {
            let capture = dir_capture(&bin, &args, &worktree.path, "<frozen-worktree>");
            written.push(write_golden(
                &goldens_dir,
                &format!("read-parity/{slice}.json"),
                &capture,
            ));
        }

        // validate/stats/audit: the same worktree, since none of these
        // three read `.claude/rules` either.
        let batch14 = root.join("tests/fixtures/batch14-workspace");
        let batch14_files: Vec<PathBuf> = sorted_json_names(&batch14)
            .into_iter()
            .map(|name| batch14.join(name))
            .collect();
        written.push(validate_and_freeze(
            &bin,
            &worktree.path,
            &batch14_files,
            &batch14,
            "<fixtures>/batch14-workspace",
            &goldens_dir,
            "batch14-workspace",
        ));
        written.push(validate_and_freeze(
            &bin,
            &worktree.path,
            &[batch14.join("task-1-report.json")],
            &batch14,
            "<fixtures>/batch14-workspace",
            &goldens_dir,
            "task-1-report",
        ));

        let invalid = root.join("tests/fixtures/invalid-deliverable");
        written.push(validate_and_freeze(
            &bin,
            &worktree.path,
            &[invalid.join("bad-report.json")],
            &invalid,
            "<fixtures>/invalid-deliverable",
            &goldens_dir,
            "invalid-deliverable",
        ));

        let skipped = root.join("tests/fixtures/skipped-report");
        written.push(validate_and_freeze(
            &bin,
            &worktree.path,
            &[skipped.join("skipped-report.json")],
            &skipped,
            "<fixtures>/skipped-report",
            &goldens_dir,
            "skipped-report",
        ));

        written.push(stats_and_freeze(
            &bin,
            &worktree.path,
            &batch14,
            "<fixtures>/batch14-workspace",
            &goldens_dir,
            "batch14-workspace",
        ));
        let stats_workspace = root.join("tests/fixtures/stats-workspace");
        written.push(stats_and_freeze(
            &bin,
            &worktree.path,
            &stats_workspace,
            "<fixtures>/stats-workspace",
            &goldens_dir,
            "stats-workspace",
        ));

        written.push(audit_and_freeze(
            &bin,
            &worktree.path,
            "9e959f6f297c41e273bb9d5c83d900a95974453c",
            "719d3f5883e973d8eb57c3db434fc1d016dddf6f",
            "houserules.template-is-the-source,process.tdd,process.deliverables-json,quality.principles,writing-style.doc-comments",
            &goldens_dir,
            "validate-terminal-report",
        ));
        written.push(audit_and_freeze(
            &bin,
            &worktree.path,
            "6c9cf8b48cbda2374414e230e66f043abf8708e9",
            "0654e82a0682011423b667dee248e5cb078be4e7",
            "houserules.template-is-the-source,process.deliverables-json,writing-style.principles,quality.principles,knowledge-base.state-only-the-source",
            &goldens_dir,
            "knowledge-retrospective",
        ));
    }

    // backlog's `set` slice: a scratch copy of `mini`, mutated in place --
    // both the command's own capture and the WRITTEN FILE it produces
    // freeze here.
    {
        let scratch = ScratchDir::new("houserules-goldens-backlog-set");
        copy_dir_recursive(&root.join("tests/fixtures/mini"), scratch.path());
        let capture = dir_capture(
            &bin,
            &["set", "HR-901", "status=done", "batch=2"],
            scratch.path(),
            "tests/fixtures/mini",
        );
        written.push(write_golden(
            &goldens_dir,
            "backlog/set/mini/command.json",
            &capture,
        ));

        let written_bytes = fs::read(scratch.path().join("backlog/items/misc.json"))
            .expect("read the set slice's written file");
        let dest = goldens_dir.join("backlog/set/mini/backlog/items/misc.json");
        fs::create_dir_all(dest.parent().expect("dest has a parent")).expect("mkdir golden dir");
        fs::write(&dest, &written_bytes).expect("write the set slice's golden written file");
        written.push("backlog/set/mini/backlog/items/misc.json".to_string());
    }

    // read-parity, the mini fixture: the same five remaining commands
    // over the small synthetic knowledge base.
    {
        let scratch = ScratchDir::new("houserules-goldens-read-parity-mini");
        copy_dir_recursive(&root.join("tests/fixtures/mini"), scratch.path());
        for (slice, args) in [
            ("topics", vec!["topics"]),
            ("index", vec!["index"]),
            ("index-standing", vec!["index", "--standing"]),
            ("standing", vec!["standing"]),
            ("get-mini-build-cache", vec!["get", "mini.build-cache"]),
        ] {
            let capture = dir_capture(&bin, &args, scratch.path(), "tests/fixtures/mini");
            written.push(write_golden(
                &goldens_dir,
                &format!("read-parity/mini/{slice}.json"),
                &capture,
            ));
        }
    }

    written.sort();
    println!(
        "wrote {} golden file(s) under {}:",
        written.len(),
        goldens_dir.display()
    );
    for path in &written {
        println!("  {path}");
    }
}
