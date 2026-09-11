//! The install surface: bringing the kit into and up to date in a project
//! repository.
//!
//! Owns everything the spec assigns to the `install` module boundary
//! (docs/specs/2026-09-04-batch-15-tier2-spec.md §3): `init`, `update`, and
//! `files`, including the KIT_OWNED sync and the vendored-file deletion
//! `update` gains for the no-shims migration. Gains its first code in
//! Tier-2 phase 3 (spec §5): batch 18 T3 (HR-047, docs/specs/
//! 2026-09-05-batch-18-phase3.md §§1-2) lands `init` and `files`; batch 18
//! T4 (same spec, §1) lands `update`.
//!
//! # The payload embeds at compile time (spec §2)
//!
//! `Payload` (`rust-embed`, exact-pinned `=8.12.0`, `security-hygiene.
//! dependency-vetting`/`exact-pins`) walks `template/` at compile time and
//! bakes every file it holds into the binary, so `init` needs no checkout
//! and no package manager at runtime -- the goal spec §1 states directly.
//! Owner-ruled at the spec gate over `include_dir` (stale: no release in
//! ~27 months) and a hand-rolled `build.rs` + `include_bytes!` codegen
//! (the custom solution `quality.well-maintained-libraries` exists to
//! avoid); the full dependency vet, including the self-hosted-repository
//! provenance check the spec flags, is in this task's report
//! (`dependency_vetting`), not repeated here.
//!
//! `debug-embed` (Cargo.toml) is load-bearing, not cosmetic. rust-embed's
//! own current docs (docs.rs 8.12.0, verified at this task's
//! docs_verified) state the split plainly. Without the feature, in a
//! debug build, "the folder path is resolved relative to where the binary
//! is run from". The file is then read from the filesystem at that
//! runtime location, not embedded at all. With the feature, or in
//! release, "the folder path is resolved relative to where Cargo.toml
//! is". The file is then genuinely embedded.
//!
//! Without `debug-embed`, a plain `cargo test`/`cargo run` (both debug
//! builds) would resolve `folder` against the caller's current directory
//! at that moment. That directory is not this crate's own `Cargo.toml`
//! directory. `Payload::get` could then silently miss, or silently read a
//! DIFFERENT `template/` than the one this checkout carries. Every test
//! in `tests/install.rs` would then pass or fail depending on process
//! cwd, not on what the binary actually carries. `debug-embed` makes both
//! profiles resolve `folder` relative to `Cargo.toml` and genuinely embed,
//! so `cargo test`'s own debug binary already proves the release binary's
//! embed -- the live-run release-build spot check (this task's `live_run`)
//! is the belt-and-braces confirmation that a `--release` build, launched
//! from a directory with no `template/` at all, behaves identically.
//!
//! `#[folder = "../../template/"]` is relative to this crate's own
//! `Cargo.toml` (`crates/houserules/Cargo.toml`), landing on the
//! repository-root `template/` -- the same directory `tests/common/mod.rs`'s
//! `repo_root().join("template")` and this crate's other fixture builders
//! (`backlog::test_support::vendored_schema`, `check_commit.rs`'s
//! `vendored_schema_path`) already read from disk for their own, unrelated
//! purposes; this is simply the first reader that ships inside the binary
//! itself. `walkdir` (rust-embed-utils's own dependency, read at
//! docs.rs's source view) walks every entry under `folder` with no hidden-
//! file filtering of its own, so `template/`'s dot-directories
//! (`.claude/`, `.githooks/`, `.github/`) embed along with everything
//! else -- confirmed live: `Payload::iter()` in this module's own tests
//! lists `.claude/agents/implementer.md` and `.githooks/commit-msg`.
//!
//! # `init` (spec §2, plan T3)
//!
//! Ports `bin/houserules.mjs`'s `install(io, opts, { seed: true }, cwd)`
//! for `seed = true` only (the JS function's `seed = false` branch --
//! `update`'s own KIT_OWNED sync, deletion, and drift line -- is T4's; no
//! code for it exists here). `--dir` resolves like Node's own
//! `resolve(cwd, opts.dir ?? '.')` (`node_path::resolve_like_node`), NOT
//! `crate::root::resolve_root`'s enclosing-git-root walk every read
//! command uses: `init` seeds the directory it is given (or the process's
//! own working directory), never an ancestor, matching the frozen JS
//! exactly (verified live: `node bin/houserules.mjs init` from a
//! subdirectory of a git repository, with no `--dir`, fails with "is not a
//! git repository" rather than seeding the enclosing repo's top level).
//!
//! Before batch 18 T5, the embedded payload was today's frozen-JS payload
//! byte-for-byte, so a fresh `houserules init` and a fresh `node
//! bin/houserules.mjs init` produced byte-identical trees. T5 (this commit)
//! is the sanctioned exception the parent spec names (docs/specs/
//! 2026-09-05-batch-18-phase3.md §3): every shipped reference to the two
//! retired shell wrappers rewrites to the flat `houserules` command,
//! `KIT_OWNED` drops both, and the "next:" line below prints `houserules
//! check-knowledge && houserules check-backlog`, not the two shell-wrapper
//! invocations `node bin/houserules.mjs init` still prints -- so the two
//! engines' seeded trees now diverge on exactly the rewritten bytes, and
//! stay byte-identical on everything else (this task's diff-shape gate
//! proves the "exactly" part).
//!
//! `render_and_report` (`rules::render`) reruns this crate's own already-
//! ported renderer on the freshly-seeded target, in place of the JS
//! writer's `execFileSync(node, [target/tools/kb.mjs, 'render'])`: this
//! binary has no Node to shell out to, and a second, independently-written
//! "load the base, write stale files, report which" sequence here would
//! risk drifting from `cmd_render`'s own (`rules::render`'s own module doc
//! has the shared-helper account).
//!
//! # `update` (spec §1, plan T4)
//!
//! Ports `bin/houserules.mjs`'s `install(io, opts, { seed: false }, cwd)`:
//! target resolution, the `.git` check, and the marker read-and-validate
//! (`read_marker`, shared with `seed` unmodified -- the JS source shares
//! one function for both, and this binary now does too) are byte-for-byte
//! the same code `init` already runs, since the JS never branches on
//! `seed` until after that point. What differs from `init`: `update`
//! writes `KIT_OWNED` only (no `SEED_ONCE`, no settings merge -- those stay
//! `init`-only, spec §1 and the T3 review that moved the settings merge
//! there), then reports the version drift as one `kit <stamped> -> <running>`
//! line, reusing the marker read before the restamp overwrote it. Before T5
//! rewrote `template/`, a fresh `node bin/houserules.mjs update` and a fresh
//! `houserules update` over the same already-`init`ed install produced
//! identical trees, stdout, and exit codes (T4's own `live_run` `diff -r`)
//! -- `wrote <file>` for every `KIT_OWNED` path, `render: up to date` (the
//! freshly-seeded knowledge base has nothing stale to render), `kit <old>
//! -> <new>`, then `houserules: updated <target>` and the literal `next:`
//! line. T5 (this commit) is where that literal changes, and where
//! `update` gains its first real deletion to report: an install seeded by
//! the OLD JS (or the OLD binary) still carries both retired shell
//! wrappers; this binary's `update` now reports each `removed <path>` for
//! them (`delete_retired`, below, and `RETIRED`'s own doc has their exact
//! names) in the same run that resyncs every other `KIT_OWNED` file.
//!
//! The drift line's `none` token (`quality.absence-is-designed`) covers two
//! measured shapes, not one. An install whose `.houserules.json` is
//! entirely absent (a project that deleted its own stamp) restores it with
//! `idPrefix` defaulted from `--id-prefix`/`WI`, same as `init`'s own
//! missing-marker default. An install whose stamp exists but carries no
//! `version` key (only `idPrefix`, hand-edited or from a pre-drift-tracking
//! release) keeps that `idPrefix`. Both shapes were measured on both
//! engines for this task, not assumed. This task's `live_run` entries hold
//! all four runs, JS and the binary, one per shape; every pair prints `kit
//! none -> <version>` and restamps the marker identically.
//!
//! A JSON `null` at `version` is NOT this arm. `marker.version !==
//! undefined` is true for `null`, so it falls through to the same "must be
//! a non-empty string" named error every other invalid `version` shape
//! gets, not to `none`. This shape too was measured on both engines, in
//! this task's `live_run`.
//!
//! # Deletion (spec §1, new capability, no JS predecessor)
//!
//! The two retired shell wrappers (`RETIRED`'s own doc, below, names them)
//! are meant to leave the payload and leave existing installs at their
//! next `update` (spec §1's own words).
//! Measured first, per this task's brief: `bin/houserules.mjs` and every
//! `template/tools/*.mjs` file carry no `rmSync`/`unlinkSync` call at all
//! (`grep -rn "RETIRED\|rmSync\|unlink"` over both, empty). The frozen JS
//! has no deletion path to measure parity against, so this is new
//! behavior, not a port. The controller's own default mechanism stands
//! unchallenged: `RETIRED`, a fixed list in the binary of formerly-
//! `KIT_OWNED` paths, and `delete_retired`, which removes each one present
//! under `target` and reports it, one path per line (`removed <path>`),
//! mirroring the `wrote <path>` shape the sync step already uses.
//!
//! A `RETIRED` path is deleted whether or not the adopter has changed it,
//! because the path was kit-owned, not adopter-owned. Spec §1 states the
//! deletion as unconditional: "leave existing installs at their next
//! update" carries no modified-file exception. `delete_retired` (below)
//! checks only that the path exists, never its content. An edited copy of
//! a retired file is removed exactly like an untouched one.
//!
//! `RETIRED` was EMPTY through T4: both shell wrappers were still
//! `KIT_OWNED`, so a production `update` run deleted nothing. T5 (this
//! commit) moves both paths from `KIT_OWNED` to `RETIRED` (§4: they retire
//! from this repository's own tree at T6), in the same commit that
//! rewrites every shipped reference off them -- a `houserules update`
//! over an install still carrying either file now reports its own
//! `removed <path>` line (`RETIRED`'s own doc names both paths) and
//! deletes it.
//! `delete_retired`'s own tests inject their own list directly, a plain
//! function parameter rather than a `RETIRED` override, so the mechanism
//! itself is proved independent of what `RETIRED` happens to hold; a
//! further unit test (`retired_holds_the_shell_tools_moved_at_t5`) pins
//! `RETIRED`'s exact, production contents.
//!
//! # Failure paths (spec §6, `houserules.crash-paths-are-named`)
//!
//! Every one of `init`'s named errors below was measured against the real
//! `node bin/houserules.mjs init`, not assumed: a missing `.git` in the
//! target, a malformed `--id-prefix`, and a pre-existing `.houserules.json`
//! that is not valid JSON, is valid JSON but not an object, carries an
//! invalid `idPrefix`, or carries an empty/non-string `version` all print
//! one stderr line and exit 2 on both engines (this task's report quotes
//! each captured JS line). The one accepted divergence: an invalid-JSON
//! marker's inner message text is `serde_json`'s own, not V8's
//! (`read_json_object`'s own doc), the same accepted-divergence shape
//! `rules::deliverable::read_deliverable_value` already carries for the
//! same reason. A target directory that already holds unrelated files, or
//! a second `init` run over an already-seeded target, is NOT a failure
//! path at all -- measured live, both engines seed what is missing, `kept`
//! what already exists, and leave every unrelated file untouched.
//!
//! `update` shares every one of `init`'s marker-validation errors above
//! (`read_marker`'s own doc). The two commands are separate CLI surfaces.
//! The JS runs one shared function for both, but each of the five invalid
//! shapes was re-measured against `node bin/houserules.mjs update` for
//! this task, not assumed from `init`'s account: unparseable JSON, a
//! non-object, a bad `idPrefix`, an empty `version`, and a `null`
//! `version`. This task's `live_run` entries hold all ten runs, JS and the
//! binary, one pair per shape. Every pair exits 2, with no `wrote <file>`
//! line ahead of it on either engine, since the marker is read and
//! validated before any file is written or printed. Four of the five
//! pairs also print the identical named line. The unparseable-JSON pair
//! does not, for the same accepted divergence the init section above
//! names: `serde_json`'s inner message text, not V8's. `update`'s own
//! `--id-prefix` is validated the same way `init`'s is, exit 2 on the same
//! malformed flag, measured live over an already-seeded target.
//!
//! `update` over a target that was never `init`ed is a failure path too,
//! not a crash to reproduce. Measured live on both engines for this task
//! (this task's `live_run`, Unix hosts), the binary prints one named line
//! naming `<target>/knowledge/schema.json` and exits 2; `node
//! bin/houserules.mjs update` dumps a 26-line stack trace for the same
//! missing file instead. The exact `io::Error` text and path separator are
//! platform-specific -- a hardcoded Unix message broke this arm's CLI test
//! on Windows (batch-18 PR #6, run 34056836063) with a different message
//! and separator. `tests/update.rs`'s own never-`init`ed test derives the
//! expected line from a real error on this platform instead, fixing both;
//! its own doc comment and `seeded_repo`'s carry the fuller account.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use rust_embed::RustEmbed;
use serde_json::{Map, Value, json};

use crate::emit::emit;
use crate::node_path::resolve_like_node;

/// The kit payload, embedded from this repository's `template/` at compile
/// time (this module's own doc has the full vetting and configuration
/// account).
#[derive(RustEmbed)]
#[folder = "../../template/"]
struct Payload;

/// Machinery files houserules owns: `init` writes them and `update`
/// overwrites them. `bin/houserules.mjs`'s own `KIT_OWNED`, originally
/// ported verbatim -- `tests/install.rs`'s own copy pins this list, so
/// the two cannot silently drift apart. `tools/kb.mjs`, `tools/backlog.mjs`,
/// `tools/lib/cli.mjs`, and `tools/lib/json-store.mjs` left this list at
/// batch 20 T3 (HR-047, docs/specs/2026-09-07-batch-20-phase5.md §2): the
/// shipped-but-inert JS engines (`houserules.payload-runs-on-builtins`
/// already made every reference to them dead code) retired from the
/// payload outright, joining `RETIRED` below.
const KIT_OWNED: &[&str] = &[
    "tools/claude-session-start.sh",
    ".githooks/commit-msg",
    ".claude/agents/implementer.md",
    ".claude/agents/task-reviewer.md",
    ".claude/agents/branch-reviewer.md",
    ".claude/skills/orchestrating/SKILL.md",
    ".claude/skills/finishing-a-feature/SKILL.md",
    ".claude/skills/migrating-knowledge/SKILL.md",
];

/// Project-data files `init` seeds once and never touches again.
/// `bin/houserules.mjs`'s own `SEED_ONCE`, ported verbatim, plus
/// `docs/README.md` (HR-087, fix round 1, important issue 3): without it
/// a fresh `init`'s own `docs` area (`knowledge/areas.json`) declares
/// `docs/**` over a directory the seed otherwise never creates, so
/// `check-knowledge` -- the very next step `init` prints -- failed on
/// every fresh install until this file gave that glob something real to
/// match.
const SEED_ONCE: &[&str] = &[
    "knowledge/schema.json",
    "knowledge/areas.json",
    "knowledge/process.json",
    "knowledge/quality.json",
    "knowledge/security-hygiene.json",
    "knowledge/writing-style.json",
    "knowledge/knowledge-base.json",
    "backlog/schema.json",
    "backlog/amendments.json",
    "backlog/batches.json",
    "backlog/decisions.json",
    "backlog/parked.json",
    "backlog/items/general.json",
    ".claude/schemas/deliverables.json",
    ".claude/evals/dependency-add.json",
    ".claude/evals/docs-edit.json",
    ".claude/evals/record.json",
    ".claude/evals/seeded-violations.json",
    ".github/workflows/knowledge.yml",
    "docs/README.md",
    "CLAUDE.md",
];

/// Seed files that carry the backlog id prefix; `--id-prefix` rewrites
/// them. `bin/houserules.mjs`'s own `PREFIXED`.
const PREFIXED: &[&str] = &[
    "backlog/schema.json",
    "backlog/items/general.json",
    ".claude/schemas/deliverables.json",
];

/// Formerly-`KIT_OWNED` paths `update` deletes from an install if present
/// (this module's own doc, "Deletion", has the full account of the
/// mechanism and why it is new rather than ported). Batch 18 T5 moved
/// `tools/kb.sh` and `tools/backlog.sh` here, in the same commit that
/// rewrote every shipped reference to them off the flat `houserules`
/// command surface. Batch 20 T3 (HR-047) adds the four JS engines those
/// two shims used to front -- `tools/kb.mjs`, `tools/backlog.mjs`,
/// `tools/lib/cli.mjs`, `tools/lib/json-store.mjs` -- the mechanism's
/// second use (this module's own doc, "Deletion", names the first).
const RETIRED: &[&str] = &[
    "tools/kb.sh",
    "tools/backlog.sh",
    "tools/kb.mjs",
    "tools/backlog.mjs",
    "tools/lib/cli.mjs",
    "tools/lib/json-store.mjs",
];

/// Path, relative to a target repository, `.claude/settings.json` seeds or merges at.
const SETTINGS_PATH: &str = ".claude/settings.json";

/// Path, relative to a target repository, the install stamp lives at.
const MARKER_PATH: &str = ".houserules.json";

/// The shared constraint text for both `idPrefix` rejection messages --
/// `bin/houserules.mjs`'s own `ID_PREFIX_HINT`.
const ID_PREFIX_HINT: &str = "must be 1-8 characters, A-Z then A-Z0-9";

/// `true` when `value` is a valid backlog id prefix: one uppercase ASCII
/// letter followed by up to seven more uppercase ASCII letters or digits.
/// `bin/houserules.mjs`'s own `isIdPrefix` (`/^[A-Z][A-Z0-9]{0,7}$/`),
/// ported as a direct byte check rather than a `regress` pattern: every
/// character class here is a fixed ASCII range, so a regex engine adds
/// indirection a `char::is_ascii_uppercase`/`is_ascii_digit` pair already
/// expresses exactly.
fn is_id_prefix(value: &str) -> bool {
    let bytes = value.as_bytes();
    match bytes.split_first() {
        Some((&first, rest)) if bytes.len() <= 8 => {
            first.is_ascii_uppercase()
                && rest
                    .iter()
                    .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
        }
        _ => false,
    }
}

/// The kit version stamped into `.houserules.json`'s `version` field --
/// `env!("CARGO_PKG_VERSION")`, the same value `houserules --version`
/// reports (`tests/version.rs`), baked in by cargo itself at compile
/// time. Before batch 20 T3 (HR-047, docs/specs/2026-09-07-batch-20-
/// phase5.md §2) this read `package.json` instead, via `include_str!`,
/// as an independent cross-check against `CARGO_PKG_VERSION`:
/// release-please's `extra-files` config kept `Cargo.toml`'s and
/// `package.json`'s `version` fields in lockstep at every release, and a
/// dedicated test pinned the two answers equal. `package.json` retired
/// with the rest of the JS toolchain at T3 (HR-073 tracks release-please's
/// own config catching up), so `Cargo.toml` is now the only version
/// source in this repository and the cross-check collapses to this one
/// field.
fn kit_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

/// Reads one payload file, rewriting the backlog id prefix where it
/// applies -- `bin/houserules.mjs`'s own `templateContent`, reading from
/// `Payload` (compile-time embed) instead of `template/` on disk
/// (runtime read).
fn payload_content(file: &str, prefix: &str) -> Result<Vec<u8>, String> {
    let bytes = Payload::get(file)
        .unwrap_or_else(|| panic!("{file} is a fixed, checked-in KIT_OWNED/SEED_ONCE/settings path missing from the embedded payload"))
        .data;
    if !PREFIXED.contains(&file) || prefix == "WI" {
        return Ok(bytes.into_owned());
    }
    let text = std::str::from_utf8(&bytes)
        .map_err(|_| format!("{file}: not valid UTF-8, cannot rewrite its id prefix"))?;
    Ok(text.replace("WI-", &format!("{prefix}-")).into_bytes())
}

/// Writes `content` to `file` under `target`, keeping shell scripts and
/// git hooks executable -- `bin/houserules.mjs`'s own `writeInto`.
fn write_into(target: &Path, file: &str, content: &[u8]) -> Result<(), String> {
    let path = target.join(file);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| format!("{}: {error}", parent.display()))?;
    }
    fs::write(&path, content).map_err(|error| format!("{}: {error}", path.display()))?;
    if file.ends_with(".sh") || file.starts_with(".githooks/") {
        mark_executable(&path)?;
    }
    Ok(())
}

/// Marks `path` executable (mode 0o755) on Unix, matching `writeInto`'s
/// `chmodSync(path, 0o755)`. A no-op on Windows: `std::fs::Permissions`
/// exposes only the portable read-only flag there, and the POSIX-shell
/// tooling this bit governs (`.sh` scripts, git hooks) already assumes a
/// POSIX shell to run at all.
#[cfg(unix)]
fn mark_executable(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o755))
        .map_err(|error| format!("{}: {error}", path.display()))
}

/// See the Unix `mark_executable`'s own doc.
#[cfg(not(unix))]
fn mark_executable(_path: &Path) -> Result<(), String> {
    Ok(())
}

/// Reads and parses `path` as JSON, requiring the result to be a plain
/// object -- `bin/houserules.mjs`'s own `readJsonObject`. The invalid-JSON
/// message embeds `serde_json`'s own error text, not V8's
/// (`rules::deliverable::read_deliverable_value`'s own doc names this same
/// accepted divergence for the identical reason: only the outer
/// `"<path>: invalid JSON (...)"` shape is part of the parity contract).
fn read_json_object(path: &Path) -> Result<Map<String, Value>, String> {
    let text = fs::read_to_string(path).map_err(|error| format!("{}: {error}", path.display()))?;
    let value: Value = serde_json::from_str(&text)
        .map_err(|error| format!("{}: invalid JSON ({error})", path.display()))?;
    match value {
        Value::Object(map) => Ok(map),
        _ => Err(format!("{}: not a JSON object", path.display())),
    }
}

/// Reads `target`'s `.houserules.json` stamp, or synthesizes the default
/// `{idPrefix: prefix}` when none exists yet, and validates it -- the block
/// `bin/houserules.mjs`'s own `install` runs before writing anything,
/// shared unmodified by `seed` (`init`) and `update` below, since the JS
/// itself does not branch on `seed` until after this point. Raises a named
/// error, matching either engine (this module's own "Failure paths"
/// section has every shape re-measured for `update`): the stamp is present
/// but is not a JSON object, its `idPrefix` is present but fails
/// `is_id_prefix`, or its `version` is present but is not a non-empty
/// string -- a JSON `null` at `version` included, since `null` is a
/// present value, not an absent key.
fn read_marker(marker_path: &Path, prefix: &str) -> Result<Map<String, Value>, String> {
    let marker = if marker_path.exists() {
        read_json_object(marker_path)?
    } else {
        Map::from_iter([("idPrefix".to_string(), json!(prefix))])
    };
    if let Some(id_prefix) = marker.get("idPrefix")
        && !id_prefix.as_str().is_some_and(is_id_prefix)
    {
        return Err(format!(
            "{}: idPrefix {ID_PREFIX_HINT}",
            marker_path.display()
        ));
    }
    if let Some(version) = marker.get("version")
        && !version.as_str().is_some_and(|v| !v.is_empty())
    {
        return Err(format!(
            "{}: version must be a non-empty string",
            marker_path.display()
        ));
    }
    Ok(marker)
}

/// Deletes each `retired` path found under `target`, returning the ones
/// actually removed, in call order -- `update`'s own report prints one
/// `removed <path>` line per entry this returns (this module's own doc,
/// "Deletion", has the full account). A `retired` path absent from `target`
/// is left alone and not reported: `RETIRED` widens over releases, and an
/// install already missing a since-retired file is not an error.
fn delete_retired(target: &Path, retired: &[&str]) -> Result<Vec<String>, String> {
    let mut deleted = Vec::new();
    for &file in retired {
        let path = target.join(file);
        if path.exists() {
            fs::remove_file(&path).map_err(|error| format!("{}: {error}", path.display()))?;
            deleted.push(file.to_string());
        }
    }
    Ok(deleted)
}

/// Merges the payload's `SessionStart` hooks into an existing
/// `settings.json`, appending only entries whose `matcher` is not already
/// present. Returns whether the merge changed the file --
/// `bin/houserules.mjs`'s own `mergeSettings`.
///
/// Two JS quirks, both measured live and ported exactly (fix round 1,
/// issue 1): `settings.hooks ??= {}` and `settings.hooks.SessionStart ??=
/// []` are nullish-coalescing ASSIGNMENT -- they replace `null` (and a
/// missing key) with the default, same as `undefined`, but leave any other
/// value alone. A prior cut here used `serde_json::Map::entry(...)
/// .or_insert_with(...)`, which only inserts for a MISSING key and leaves
/// an existing `null` untouched, so `{"hooks": null}` and
/// `{"hooks": {"SessionStart": null}}` reached `as_object_mut`/
/// `as_array_mut` and became a named error where the frozen JS seeds
/// cleanly (exit 0, both matchers merged) -- `null` is handled explicitly
/// below, before either type check.
///
/// A `hooks` value that is itself a JSON ARRAY is its own case, not a
/// crash: `settings.hooks.SessionStart ??= []` sets a plain, non-index
/// property on that array OBJECT (arrays take arbitrary string keys in
/// JS), and the loop below pushes every template matcher into it -- but
/// `JSON.stringify` on an array serializes only its indexed elements,
/// silently dropping a non-index property and everything pushed into it.
/// So a `hooks` array reaches disk with its own elements untouched, the
/// attempted merge invisible, `changed` still `true` (the write always
/// runs), and exit 0. `serde_json::Value::Array` has no equivalent
/// "extra named property" a Rust value could carry, so this arm returns
/// the same observable result directly instead of modeling the JS
/// mechanism that produces it.
fn merge_settings(path: &Path, prefix: &str) -> Result<bool, String> {
    let template: Value = serde_json::from_slice(&payload_content(SETTINGS_PATH, prefix)?)
        .expect("the embedded settings.json is valid JSON");
    let mut settings = read_json_object(path)?;
    let hooks = settings
        .entry("hooks".to_string())
        .or_insert_with(|| json!({}));
    if hooks.is_null() {
        *hooks = json!({});
    }
    if hooks.is_array() {
        fs::write(path, emit(&Value::Object(settings)))
            .map_err(|error| format!("{}: {error}", path.display()))?;
        return Ok(true);
    }
    let hooks_obj = hooks
        .as_object_mut()
        .ok_or_else(|| format!("{}: hooks is not an object", path.display()))?;
    let session_start = hooks_obj
        .entry("SessionStart".to_string())
        .or_insert_with(|| json!([]));
    if session_start.is_null() {
        *session_start = json!([]);
    }
    let session_start_arr = session_start
        .as_array_mut()
        .ok_or_else(|| format!("{}: hooks.SessionStart is not an array", path.display()))?;

    let existing_matchers: std::collections::HashSet<Option<String>> = session_start_arr
        .iter()
        .map(|entry| {
            entry
                .get("matcher")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .collect();
    let template_entries = template["hooks"]["SessionStart"]
        .as_array()
        .cloned()
        .unwrap_or_default();

    let mut changed = false;
    for entry in template_entries {
        let matcher = entry
            .get("matcher")
            .and_then(Value::as_str)
            .map(str::to_string);
        if existing_matchers.contains(&matcher) {
            continue;
        }
        session_start_arr.push(entry);
        changed = true;
    }
    if changed {
        fs::write(path, emit(&Value::Object(settings)))
            .map_err(|error| format!("{}: {error}", path.display()))?;
    }
    Ok(changed)
}

/// Seeds `target` from the embedded payload: writes every `KIT_OWNED` file,
/// then every `SEED_ONCE` file absent from `target` (an existing one is
/// left untouched, reported `kept`), seeds or merges `.claude/settings.json`,
/// stamps `.houserules.json`, and renders the generated markdown --
/// `bin/houserules.mjs`'s own `install(io, opts, { seed: true }, cwd)`.
fn seed(target: &Path, id_prefix: Option<String>) -> Result<(), String> {
    if !target.join(".git").exists() {
        return Err(format!(
            "{} is not a git repository (run git init first)",
            target.display()
        ));
    }
    let prefix = id_prefix.unwrap_or_else(|| "WI".to_string());
    if !is_id_prefix(&prefix) {
        return Err(format!("id-prefix {ID_PREFIX_HINT}"));
    }
    let marker_path = target.join(MARKER_PATH);
    let marker = read_marker(&marker_path, &prefix)?;

    for file in KIT_OWNED {
        write_into(target, file, &payload_content(file, &prefix)?)?;
        println!("wrote {file}");
    }
    for file in SEED_ONCE {
        if target.join(file).exists() {
            println!("kept {file}");
            continue;
        }
        write_into(target, file, &payload_content(file, &prefix)?)?;
        println!("wrote {file}");
    }
    let settings_path = target.join(SETTINGS_PATH);
    if settings_path.exists() {
        if merge_settings(&settings_path, &prefix)? {
            println!("merged {SETTINGS_PATH} (SessionStart hooks added)");
        } else {
            println!("kept {SETTINGS_PATH} (hooks already present)");
        }
    } else {
        write_into(
            target,
            SETTINGS_PATH,
            &payload_content(SETTINGS_PATH, &prefix)?,
        )?;
        println!("wrote {SETTINGS_PATH}");
    }

    let stamped_id_prefix = marker
        .get("idPrefix")
        .and_then(Value::as_str)
        .unwrap_or(&prefix);
    fs::write(
        &marker_path,
        emit(&json!({"version": kit_version(), "idPrefix": stamped_id_prefix})),
    )
    .map_err(|error| format!("{}: {error}", marker_path.display()))?;
    crate::rules::render_and_report(target)?;
    println!("houserules: initialized {}", target.display());
    println!("next: houserules check-knowledge && houserules check-backlog");
    Ok(())
}

/// Runs the `init` subcommand: resolves `dir` like Node's own
/// `path.resolve(cwd, dir ?? '.')` (this module's own doc explains why
/// this, not `crate::root::resolve_root`'s git-root walk, is the correct
/// resolution here), then seeds it from the embedded payload. Every error
/// is one named stderr line and exit 2 (`houserules.crash-paths-are-named`).
pub(crate) fn cmd_init(dir: Option<PathBuf>, id_prefix: Option<String>) -> ExitCode {
    let target = match resolve_like_node(dir.as_deref().unwrap_or_else(|| Path::new("."))) {
        Ok(target) => target,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::from(2);
        }
    };
    match seed(&target, id_prefix) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("{message}");
            ExitCode::from(2)
        }
    }
}

/// Syncs `target`'s `KIT_OWNED` files from the embedded payload, deletes
/// any `RETIRED` file still present, restamps `.houserules.json`, and
/// reports the stamped-to-running version drift -- `bin/houserules.mjs`'s
/// own `install(io, opts, { seed: false }, cwd)` (this module's own
/// `update` doc section has the full account, including the deletion step
/// the JS has no counterpart for).
fn update(target: &Path, id_prefix: Option<String>) -> Result<(), String> {
    if !target.join(".git").exists() {
        return Err(format!(
            "{} is not a git repository (run git init first)",
            target.display()
        ));
    }
    let prefix = id_prefix.unwrap_or_else(|| "WI".to_string());
    if !is_id_prefix(&prefix) {
        return Err(format!("id-prefix {ID_PREFIX_HINT}"));
    }
    let marker_path = target.join(MARKER_PATH);
    let marker = read_marker(&marker_path, &prefix)?;

    for file in KIT_OWNED {
        write_into(target, file, &payload_content(file, &prefix)?)?;
        println!("wrote {file}");
    }
    for file in delete_retired(target, RETIRED)? {
        println!("removed {file}");
    }

    let stamped_version = marker
        .get("version")
        .and_then(Value::as_str)
        .unwrap_or("none")
        .to_string();
    let stamped_id_prefix = marker
        .get("idPrefix")
        .and_then(Value::as_str)
        .unwrap_or(&prefix);
    let running_version = kit_version();
    fs::write(
        &marker_path,
        emit(&json!({"version": running_version, "idPrefix": stamped_id_prefix})),
    )
    .map_err(|error| format!("{}: {error}", marker_path.display()))?;
    crate::rules::render_and_report(target)?;
    println!("kit {stamped_version} -> {running_version}");
    println!("houserules: updated {}", target.display());
    println!("next: houserules check-knowledge && houserules check-backlog");
    Ok(())
}

/// Runs the `update` subcommand: resolves `dir` the same way `init` does
/// (`cmd_init`'s own doc explains the choice), then syncs it from the
/// embedded payload. Every error is one named stderr line and exit 2
/// (`houserules.crash-paths-are-named`).
pub(crate) fn cmd_update(dir: Option<PathBuf>, id_prefix: Option<String>) -> ExitCode {
    let target = match resolve_like_node(dir.as_deref().unwrap_or_else(|| Path::new("."))) {
        Ok(target) => target,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::from(2);
        }
    };
    match update(&target, id_prefix) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("{message}");
            ExitCode::from(2)
        }
    }
}

/// Runs the `files` subcommand: prints the kit-owned and seed-once path
/// lists as JSON -- `bin/houserules.mjs`'s own `case 'files'` arm.
pub(crate) fn cmd_files() -> ExitCode {
    print!(
        "{}",
        emit(&json!({"kitOwned": KIT_OWNED, "seedOnce": SEED_ONCE}))
    );
    ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Confirms `walkdir`'s no-hidden-file-filtering behavior (this
    /// module's own doc) actually holds for this crate's real `Payload`,
    /// not just the upstream source read at docs.rs: a dot-directory entry
    /// this task's own `KIT_OWNED`/`SEED_ONCE` lists depend on is present
    /// in the compiled-in payload.
    #[test]
    fn the_embedded_payload_carries_every_kit_owned_and_seed_once_path() {
        for file in KIT_OWNED.iter().chain(SEED_ONCE).chain([&SETTINGS_PATH]) {
            assert!(
                Payload::get(file).is_some(),
                "{file} is missing from the embedded payload"
            );
        }
    }

    #[test]
    fn is_id_prefix_accepts_one_to_eight_upper_alnum_starting_with_a_letter() {
        assert!(is_id_prefix("A"));
        assert!(is_id_prefix("WI"));
        assert!(is_id_prefix("ABCDEFGH"));
        assert!(is_id_prefix("A1B2C3D4"));
    }

    #[test]
    fn is_id_prefix_rejects_lowercase_leading_digit_empty_and_overlong() {
        assert!(!is_id_prefix(""));
        assert!(!is_id_prefix("wi"));
        assert!(!is_id_prefix("1AB"));
        assert!(!is_id_prefix("ABCDEFGHI"));
        assert!(!is_id_prefix("A-B"));
    }

    #[test]
    fn payload_content_rewrites_the_id_prefix_only_in_prefixed_files() {
        let rewritten = payload_content("backlog/items/general.json", "FOO").unwrap();
        let text = String::from_utf8(rewritten).unwrap();
        assert!(text.contains("FOO-001"), "{text}");
        assert!(!text.contains("WI-"), "{text}");

        let unprefixed = payload_content("CLAUDE.md", "FOO").unwrap();
        let original = Payload::get("CLAUDE.md").unwrap().data;
        assert_eq!(unprefixed, original.into_owned());
    }

    #[test]
    fn payload_content_leaves_prefixed_files_untouched_for_the_default_wi_prefix() {
        let content = payload_content("backlog/items/general.json", "WI").unwrap();
        let original = Payload::get("backlog/items/general.json").unwrap().data;
        assert_eq!(content, original.into_owned());
    }

    /// `tests/install.rs`'s `init_stamps_the_marker_with_the_kit_version_and_
    /// the_default_id_prefix` is the real cross-check, comparing a fresh
    /// `.houserules.json` stamp against that test's own, independent
    /// `env!("CARGO_PKG_VERSION")` -- this unit test only pins that
    /// `kit_version` extracts a non-empty string at all. Batch 20 T3
    /// removed the sibling `kit_version_matches_the_crate_s_own_cargo_pkg_
    /// version` test this doc used to point to: since `kit_version` now IS
    /// `env!("CARGO_PKG_VERSION").to_string()` (that function's own doc has
    /// the account), asserting the two equal had become a tautology, true
    /// by construction and unable to ever fail -- dead weight, not
    /// coverage.
    #[test]
    fn kit_version_is_a_non_empty_string() {
        assert!(!kit_version().is_empty());
    }

    /// `RETIRED` holds real paths now (this module's own "Deletion" doc
    /// section explains why), but this test still injects its own list
    /// directly at the `delete_retired` call site -- the test-only
    /// injection the T4 brief called for -- rather than depending on
    /// `RETIRED`'s own contents, which the next test pins separately.
    #[test]
    fn delete_retired_removes_present_paths_and_reports_them_in_call_order() {
        let dir = tempfile::tempdir().expect("tempdir");
        fs::create_dir_all(dir.path().join("tools")).expect("mkdir tools");
        fs::write(dir.path().join("tools/old.sh"), b"old").expect("write tools/old.sh");
        fs::write(dir.path().join("also-retired.txt"), b"x").expect("write also-retired.txt");

        let deleted = delete_retired(dir.path(), &["tools/old.sh", "also-retired.txt"])
            .expect("delete_retired");

        assert_eq!(
            deleted,
            vec!["tools/old.sh".to_string(), "also-retired.txt".to_string()]
        );
        assert!(!dir.path().join("tools/old.sh").exists());
        assert!(!dir.path().join("also-retired.txt").exists());
    }

    #[test]
    fn delete_retired_leaves_an_absent_path_alone_and_unreported() {
        let dir = tempfile::tempdir().expect("tempdir");
        let deleted = delete_retired(dir.path(), &["never/written.txt"]).expect("delete_retired");
        assert!(deleted.is_empty());
        assert!(!dir.path().join("never/written.txt").exists());
    }

    #[test]
    fn delete_retired_reports_only_the_paths_that_existed_from_a_mixed_list() {
        let dir = tempfile::tempdir().expect("tempdir");
        fs::write(dir.path().join("present.txt"), b"x").expect("write present.txt");

        let deleted =
            delete_retired(dir.path(), &["missing.txt", "present.txt"]).expect("delete_retired");

        assert_eq!(deleted, vec!["present.txt".to_string()]);
        assert!(!dir.path().join("present.txt").exists());
    }

    /// Pins `RETIRED`'s own contents and call order, not just that a fresh
    /// install has nothing to delete: `update_deletes_retired_shell_tools_
    /// from_an_old_install` (tests/update.rs) covers the CLI-visible half;
    /// this is the one place a hand edit widening or reordering `RETIRED`
    /// shows up as a conscious diff to this exact list, not a silent pass.
    /// Batch 18 T5 moved `tools/kb.sh` and `tools/backlog.sh` here first
    /// (every shipped reference to the shell wrappers rewritten to the flat
    /// `houserules` command in that same commit); batch 20 T3 (HR-047)
    /// adds the four JS engines those wrappers used to front, in the same
    /// commit that drops them from `KIT_OWNED` (this module's own
    /// "Deletion" doc). An install that still carries any of the six has
    /// it deleted, not resynced, at its next `update`.
    #[test]
    fn retired_holds_the_shell_tools_and_the_js_engines_they_fronted() {
        assert_eq!(
            RETIRED,
            [
                "tools/kb.sh",
                "tools/backlog.sh",
                "tools/kb.mjs",
                "tools/backlog.mjs",
                "tools/lib/cli.mjs",
                "tools/lib/json-store.mjs",
            ]
        );
    }
}
