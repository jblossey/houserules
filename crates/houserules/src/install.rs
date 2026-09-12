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
//! proved the "exactly" part, before it retired as a spent one-time
//! proof, HR-093).
//!
//! `render_and_report` (`rules::render`) reruns this crate's own already-
//! ported renderer on the freshly-seeded target, in place of the JS
//! writer's `execFileSync(node, [target/tools/kb.mjs, 'render'])`: this
//! binary has no Node to shell out to, and a second, independently-written
//! "load the base, write stale files, report which" sequence here would
//! risk drifting from `cmd_render`'s own (`rules::render`'s own module doc
//! has the shared-helper account).
//!
//! # `update` and the ownership baseline
//!
//! `update` resolves the target the same way `init` does, runs the same
//! `.git` check and marker read-and-validate (`read_marker`), then
//! reconciles four kinds of kit-shipped content against `.houserules.json`'s
//! `baselines` map (`baseline::classify`'s own doc has the decision rule
//! every one of them shares) and its `overrides` list (a hand-edited JSON
//! array of paths and knowledge-entry ids the adopter has declared their
//! own, documented for adopters at `docs/README.md`):
//!
//! - Every `KIT_OWNED` file not in `overrides`: at its recorded baseline
//!   (or, with none recorded yet, identical to the running payload) gets
//!   overwritten and restamped, same as an unconditional sync would;
//!   content that diverges is kept and reported once (`kept <path> (locally
//!   modified)`); a path the adopter deleted outright is RESTORED, the same
//!   as the at-baseline case, since kit machinery an adopter has not
//!   claimed with an override is always present after `update` -- it is
//!   never merely reported absent. A path in `overrides` is left exactly as
//!   found, present or absent, with no report line at all.
//! - Every knowledge-topic path (a `SEED_ONCE` path under `knowledge/` other
//!   than `schema.json` and `areas.json`): listed in `overrides`, the whole
//!   file is left exactly as found, with no report line; absent and not
//!   overridden, the whole file is backfilled and every entry stamped,
//!   reported once (`wrote <path>`); otherwise entry-level reconciliation
//!   takes over, the same four outcomes at the granularity of one knowledge
//!   entry rather than one file (`upsert_topic_entries`'s own doc has the
//!   mechanics), with an id in `overrides` governing that one entry alone.
//!   An id the payload does not ship at all -- adopter-authored -- is never
//!   touched, whether or not any of its siblings drifted.
//! - Every other `SEED_ONCE` path: written when entirely absent (a later
//!   kit release can add one after an install's `init` already ran) unless
//!   overridden; left untouched when already present, since these files are
//!   adopter data once seeded, not kit content this baseline mechanism
//!   tracks.
//! - Any `RETIRED` path still present, deleted and reported
//!   (`delete_retired`, below).
//!
//! An install with no `baselines` recorded at all needs no separate
//! migration step: `baseline::classify` already treats an unrecorded item
//! the same way whether the whole map is missing (an install from before
//! this mechanism existed) or just one key is (a single new entry a later
//! release adds) -- compare its current content directly against the
//! payload, stamp a baseline on a match, and report a divergence without
//! overwriting anything. Nothing already on disk is ever overwritten by a
//! bare reconciliation run; only a genuine at-baseline match triggers a
//! write.
//!
//! An install that was never `init`ed at all -- no `knowledge/schema.json`
//! present before this run starts -- skips both the entry reconciliation
//! and the `SEED_ONCE` backfill entirely and fails at the same render step
//! every other `update` failure path already names (this module's own
//! "Failure paths" section); there is no kit-shipped knowledge base yet to
//! reconcile against. `update` still writes every `KIT_OWNED` file and
//! deletes any `RETIRED` path present before that failure, exactly as it
//! always has.
//!
//! `update`'s own report ends with the version drift as one
//! `kit <stamped> -> <running> ` line, reusing the marker read before the
//! restamp overwrote it. The `none` token
//! (`quality.absence-is-designed`) covers two shapes: an install whose
//! `.houserules.json` is entirely absent restores it with `idPrefix`
//! defaulted from `--id-prefix`/`WI`; one whose stamp exists but carries no
//! `version` key keeps its `idPrefix`. A JSON `null` at `version` is NOT
//! this arm -- `marker.version !== undefined` is true for `null`, so it
//! fails the same "must be a non-empty string" named error every other
//! invalid `version` shape gets.
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

use std::collections::HashSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use rust_embed::RustEmbed;
use serde_json::{Map, Value, json};

use crate::baseline::{self, Status};
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

/// Project-data files `init` seeds once, and `update` never overwrites once
/// they exist: an adopter's own edits to backlog items, evals, or `CLAUDE.md`
/// are never kit-owned content. `update` still BACKFILLS a `SEED_ONCE` path
/// that is entirely absent (this module's own "update" doc section has the
/// full account) -- a later kit release can add a new path to this list, and
/// an install seeded by an older release never had a chance to receive it.
/// A subset of these paths -- `knowledge_topic_files`, below -- gets finer,
/// entry-level reconciliation instead of this whole-file treatment.
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

/// The `SEED_ONCE` paths that hold knowledge entries: `knowledge/*.json`
/// other than `schema.json` and `areas.json`, the same split
/// `rules::model::load_base` draws when it collects topic files. Each entry
/// inside one of these files, not just the file as a whole, has its own
/// recorded baseline (`baseline::classify`'s own doc explains the
/// mechanism). `update` reconciles each entry by id -- replacing one the
/// adopter left at baseline, keeping one they modified, respecting one they
/// deleted, and leaving an adopter-authored id untouched -- instead of
/// treating the file as one opaque unit the way every other `SEED_ONCE`
/// path still is. Derived from `SEED_ONCE` rather than retyped, so a topic
/// added there never needs a matching, easily-forgotten edit here.
fn knowledge_topic_files() -> Vec<&'static str> {
    SEED_ONCE
        .iter()
        .copied()
        .filter(|file| {
            file.starts_with("knowledge/")
                && *file != "knowledge/schema.json"
                && *file != "knowledge/areas.json"
        })
        .collect()
}

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

/// Joins `target` and `relative` (a `/`-separated path string) one
/// component at a time, matching `rules::model::load_base`'s own join
/// chain: `Path::join` inserts the platform's own separator only between
/// components it joins itself. A single `target.join("a/b")` keeps a
/// literal `/`. A component-wise `target.join("a").join("b")` does not.
/// Windows is the one platform where this shows: reading a topic file back
/// from disk must build the same path the code that seeded it did, or a
/// named error naming that path prints the wrong separator.
fn join_components(target: &Path, relative: &str) -> PathBuf {
    relative
        .split('/')
        .fold(target.to_path_buf(), |path, part| path.join(part))
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
/// present value, not an absent key. `overrides` and `baselines` are
/// adopter-hand-edited fields this same function now owns validating:
/// present but not an array of strings, or not an object of string values
/// respectively, is one more named error beside the two above -- never a
/// silent default, since a malformed override or baseline would otherwise
/// vanish exactly where an adopter needs it to hold.
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
    if let Some(overrides) = marker.get("overrides")
        && !overrides
            .as_array()
            .is_some_and(|entries| entries.iter().all(Value::is_string))
    {
        return Err(format!(
            "{}: overrides must be an array of strings",
            marker_path.display()
        ));
    }
    if let Some(baselines) = marker.get("baselines")
        && !baselines
            .as_object()
            .is_some_and(|entries| entries.values().all(Value::is_string))
    {
        return Err(format!(
            "{}: baselines must be an object of strings",
            marker_path.display()
        ));
    }
    Ok(marker)
}

/// Reads `marker`'s `overrides` field: a JSON array of strings, each one a
/// `KIT_OWNED`/`RETIRED` path, a `SEED_ONCE` path, or a knowledge-entry id
/// the adopter has declared their own. `read_marker` has already rejected a
/// present `overrides` that is not an array of strings, so an absent field
/// is the only remaining case this handles, as an empty list.
fn read_overrides(marker: &Map<String, Value>) -> Vec<String> {
    marker
        .get("overrides")
        .and_then(Value::as_array)
        .map(|entries| {
            entries
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// Reads `marker`'s `baselines` field: a JSON object mapping each
/// `KIT_OWNED` path or knowledge-entry id to the hex SHA-256 the kit last
/// wrote for it (`baseline::hash`). `read_marker` has already rejected a
/// present `baselines` that is not an object of string values, so an
/// absent field is the only remaining case this handles, as an empty map.
fn read_baselines(marker: &Map<String, Value>) -> Map<String, Value> {
    marker
        .get("baselines")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default()
}

/// The recorded baseline hash for `key`, or `None` when `key` has never
/// been stamped.
fn baseline_hash<'a>(baselines: &'a Map<String, Value>, key: &str) -> Option<&'a str> {
    baselines.get(key).and_then(Value::as_str)
}

/// `true` when `key` (a path or a knowledge-entry id) appears in `overrides`.
fn is_overridden(overrides: &[String], key: &str) -> bool {
    overrides.iter().any(|item| item == key)
}

/// The `entries` array declared in a knowledge topic file's parsed JSON, or
/// empty for any other shape -- the same tolerance
/// `rules::model::load_base` gives a topic file's `entries` field.
fn entries_array(value: &Value) -> &[Value] {
    value
        .get("entries")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[])
}

/// The entry in `value`'s `entries` array whose `id` field is `id`, if any.
fn find_entry<'a>(value: &'a Value, id: &str) -> Option<&'a Value> {
    entries_array(value)
        .iter()
        .find(|entry| entry.get("id").and_then(Value::as_str) == Some(id))
}

/// The canonical bytes one knowledge entry hashes to: a plain, compact JSON
/// re-serialization of its parsed value. Deterministic for a given `Value`
/// regardless of the on-disk file's own whitespace, since `serde_json`'s
/// `preserve_order` feature keeps an object's key order exactly as parsed.
fn canonical_entry_bytes(value: &Value) -> Vec<u8> {
    serde_json::to_vec(value).expect("a JSON Value always serializes")
}

/// Stamps a baseline for every kit-shipped entry in `topic_file` that is
/// actually present on disk and matches the payload's own current content
/// -- true trivially for every entry in a file `seed`'s own `SEED_ONCE` loop
/// just wrote fresh, and true for an entry a pre-existing file already
/// carried unchanged. An entry already recorded, one whose on-disk content
/// diverges from the payload, and -- critically -- one the on-disk file
/// does not contain at all are all left unstamped, for `update` to
/// reconcile on its own first run over this install: a baseline is a claim
/// that specific content is on disk, so it is never recorded for content
/// that is not. Stamping an absent entry here would tell the next `update`
/// "the adopter deleted this on purpose", when what actually happened is
/// that `seed` never wrote it into a topic file it found already present.
fn stamp_topic_baselines(
    target: &Path,
    topic_file: &str,
    prefix: &str,
    baselines: &mut Map<String, Value>,
) -> Result<(), String> {
    let payload_value: Value = serde_json::from_slice(&payload_content(topic_file, prefix)?)
        .expect("the embedded topic file is valid JSON");
    let on_disk = Value::Object(read_json_object(&join_components(target, topic_file))?);
    for entry in entries_array(&payload_value) {
        let Some(id) = entry.get("id").and_then(Value::as_str) else {
            continue;
        };
        if baselines.contains_key(id) {
            continue;
        }
        let Some(current) = find_entry(&on_disk, id).map(canonical_entry_bytes) else {
            continue;
        };
        let payload_bytes = canonical_entry_bytes(entry);
        if current == payload_bytes {
            baselines.insert(id.to_string(), json!(baseline::hash(&payload_bytes)));
        }
    }
    Ok(())
}

/// Reconciles one knowledge-topic path against the payload. Whole-file
/// absence is one decision, made before any entry is ever looked at: a
/// `topic_file` path listed in `overrides` is never written at all, kept or
/// absent exactly as found, with no report line, since the adopter has
/// declared the whole file their own; one that is simply absent, and not
/// overridden, is backfilled in full -- every entry written and stamped in
/// the payload's own order, reported once (`wrote <topic_file>`) -- the same
/// "never arrived yet" contract every other missing `SEED_ONCE` path gets.
/// Only once the file is confirmed present does reconciliation drop to
/// entry granularity, by id: an entry at its recorded baseline is written
/// and (re)stamped, silently; one the adopter modified is kept, reported
/// once (`kept <id> (locally modified)`); one the adopter deleted outright
/// is left absent, reported once (`skipped <id> (deleted)`) unless
/// overridden; one in `overrides` is left exactly as found, with no report
/// line. An id the payload does not ship at all is adopter-authored and is
/// never touched, regardless of what its siblings in the same file do.
/// Existing entries keep their on-disk position; a newly written entry
/// appends at the end, in the payload's own order, so the resulting diff
/// stays reviewable. Returns the report lines produced, in encounter order;
/// the caller prints them.
fn upsert_topic_entries(
    target: &Path,
    topic_file: &str,
    prefix: &str,
    baselines: &mut Map<String, Value>,
    overrides: &[String],
) -> Result<Vec<String>, String> {
    let path = join_components(target, topic_file);
    if is_overridden(overrides, topic_file) {
        return Ok(Vec::new());
    }

    let payload_bytes = payload_content(topic_file, prefix)?;
    let payload_value: Value =
        serde_json::from_slice(&payload_bytes).expect("the embedded topic file is valid JSON");
    let payload_entries = entries_array(&payload_value).to_vec();

    if !path.exists() {
        write_into(target, topic_file, &payload_bytes)?;
        for entry in &payload_entries {
            if let Some(id) = entry.get("id").and_then(Value::as_str) {
                baselines.insert(
                    id.to_string(),
                    json!(baseline::hash(&canonical_entry_bytes(entry))),
                );
            }
        }
        return Ok(vec![format!("wrote {topic_file}")]);
    }

    let existing_value = Value::Object(read_json_object(&path)?);
    let existing_entries = entries_array(&existing_value).to_vec();

    let mut reports = Vec::new();
    let mut merged: Vec<Value> = Vec::with_capacity(existing_entries.len());
    let mut seen: HashSet<&str> = HashSet::new();
    let mut changed = 0usize;

    for entry in &existing_entries {
        let Some(id) = entry.get("id").and_then(Value::as_str) else {
            merged.push(entry.clone());
            continue;
        };
        let Some(payload_entry) = payload_entries
            .iter()
            .find(|candidate| candidate.get("id").and_then(Value::as_str) == Some(id))
        else {
            merged.push(entry.clone());
            continue;
        };
        seen.insert(id);
        let payload_bytes = canonical_entry_bytes(payload_entry);
        let current_bytes = canonical_entry_bytes(entry);
        let baseline = baseline_hash(baselines, id).map(str::to_string);
        match baseline::classify(
            baseline.as_deref(),
            Some(&current_bytes),
            &payload_bytes,
            is_overridden(overrides, id),
        ) {
            Status::AtBaseline => {
                if current_bytes != payload_bytes {
                    changed += 1;
                }
                merged.push(payload_entry.clone());
                baselines.insert(id.to_string(), json!(baseline::hash(&payload_bytes)));
            }
            Status::Modified => {
                merged.push(entry.clone());
                reports.push(format!("kept {id} (locally modified)"));
            }
            Status::Overridden => merged.push(entry.clone()),
            Status::Deleted => unreachable!("current is Some for an entry read back from disk"),
        }
    }

    for payload_entry in &payload_entries {
        let Some(id) = payload_entry.get("id").and_then(Value::as_str) else {
            continue;
        };
        if seen.contains(id) {
            continue;
        }
        let payload_bytes = canonical_entry_bytes(payload_entry);
        let baseline = baseline_hash(baselines, id).map(str::to_string);
        match baseline::classify(
            baseline.as_deref(),
            None,
            &payload_bytes,
            is_overridden(overrides, id),
        ) {
            Status::AtBaseline => {
                changed += 1;
                merged.push(payload_entry.clone());
                baselines.insert(id.to_string(), json!(baseline::hash(&payload_bytes)));
            }
            Status::Deleted => reports.push(format!("skipped {id} (deleted)")),
            Status::Overridden => {}
            Status::Modified => unreachable!("current is None for an entry absent from disk"),
        }
    }

    let mut file_value = existing_value;
    if let Value::Object(map) = &mut file_value {
        map.insert("entries".to_string(), Value::Array(merged));
    }
    let content = emit(&file_value);
    let unchanged = fs::read_to_string(&path).is_ok_and(|current| current == content);
    if !unchanged {
        write_into(target, topic_file, content.as_bytes())?;
    }
    if changed > 0 {
        let entry_word = if changed == 1 { "entry" } else { "entries" };
        reports.push(format!(
            "updated {topic_file} ({changed} {entry_word} changed)"
        ));
    }
    Ok(reports)
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
/// stamps `.houserules.json` -- `overrides` and `baselines` included -- and
/// renders the generated markdown. Every payload file this writes is
/// rewritten with `effective_id_prefix`'s resolution: the marker's own
/// stamped `idPrefix` when one is already on record, falling back to the
/// `--id-prefix` flag and then `WI` -- never the flag alone, so a re-`init`
/// over an install that already committed to a prefix cannot backfill a
/// `PREFIXED` file under a different one.
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
    let mut baselines = read_baselines(&marker);
    let effective_prefix = effective_id_prefix(&marker, &prefix);

    for file in KIT_OWNED {
        let payload_bytes = payload_content(file, &effective_prefix)?;
        write_into(target, file, &payload_bytes)?;
        println!("wrote {file}");
        baselines.insert(file.to_string(), json!(baseline::hash(&payload_bytes)));
    }
    for file in SEED_ONCE {
        if target.join(file).exists() {
            println!("kept {file}");
            continue;
        }
        write_into(target, file, &payload_content(file, &effective_prefix)?)?;
        println!("wrote {file}");
    }
    for topic_file in knowledge_topic_files() {
        stamp_topic_baselines(target, topic_file, &effective_prefix, &mut baselines)?;
    }
    let settings_path = target.join(SETTINGS_PATH);
    if settings_path.exists() {
        if merge_settings(&settings_path, &effective_prefix)? {
            println!("merged {SETTINGS_PATH} (SessionStart hooks added)");
        } else {
            println!("kept {SETTINGS_PATH} (hooks already present)");
        }
    } else {
        write_into(
            target,
            SETTINGS_PATH,
            &payload_content(SETTINGS_PATH, &effective_prefix)?,
        )?;
        println!("wrote {SETTINGS_PATH}");
    }

    fs::write(
        &marker_path,
        emit(&new_marker(
            &marker,
            kit_version(),
            &effective_prefix,
            baselines,
        )),
    )
    .map_err(|error| format!("{}: {error}", marker_path.display()))?;
    crate::rules::render_and_report(target)?;
    println!("houserules: initialized {}", target.display());
    println!("next: houserules check-knowledge && houserules check-backlog");
    Ok(())
}

/// The id prefix payload content is rewritten with: `marker`'s own stamped
/// `idPrefix` when it has one, falling back to `flag_prefix` (the resolved
/// `--id-prefix` value, already defaulted to `WI`). An install that has
/// already committed to a prefix keeps writing every `PREFIXED` file under
/// that same prefix regardless of what a later `--id-prefix` flag says;
/// only an install with no stamped prefix yet -- a fresh `init`, or a
/// marker predating this field -- takes the flag's value.
fn effective_id_prefix(marker: &Map<String, Value>, flag_prefix: &str) -> String {
    marker
        .get("idPrefix")
        .and_then(Value::as_str)
        .unwrap_or(flag_prefix)
        .to_string()
}

/// Builds `.houserules.json`'s content for a restamp: starts from `marker`
/// as read, so any field this module does not itself own -- an adopter's
/// own hand-added key included -- survives untouched, then overwrites only
/// `version`, `idPrefix`, and `baselines` in place.
fn new_marker(
    marker: &Map<String, Value>,
    version: String,
    id_prefix: &str,
    baselines: Map<String, Value>,
) -> Value {
    let mut next = marker.clone();
    next.insert("version".to_string(), json!(version));
    next.insert("idPrefix".to_string(), json!(id_prefix));
    next.insert("baselines".to_string(), Value::Object(baselines));
    Value::Object(next)
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

/// Syncs `target` from the embedded payload against its recorded ownership
/// baseline (this module's own "`update` and the ownership baseline" doc
/// section has the full account): every `KIT_OWNED` file is overwritten,
/// kept, or -- unless overridden -- restored if the adopter deleted it,
/// since kit machinery an adopter has not claimed with an override is
/// always present after `update`; every knowledge-topic entry is
/// overwritten, kept, or respected as deleted at the same per-item
/// granularity; every other `SEED_ONCE` path missing entirely is
/// backfilled; any `RETIRED` path present is deleted; `.houserules.json` is
/// restamped, and the stamped-to-running version drift is reported. Every
/// payload file this writes is rewritten with `effective_id_prefix`'s
/// resolution (`seed`'s own doc explains why the marker's stamped prefix
/// wins over the flag).
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
    let overrides = read_overrides(&marker);
    let mut baselines = read_baselines(&marker);
    let previously_seeded = target.join("knowledge/schema.json").exists();
    let effective_prefix = effective_id_prefix(&marker, &prefix);

    for file in KIT_OWNED {
        if is_overridden(&overrides, file) {
            continue;
        }
        let payload_bytes = payload_content(file, &effective_prefix)?;
        let path = target.join(file);
        let status = match fs::read(&path) {
            Ok(current) => baseline::classify(
                baseline_hash(&baselines, file),
                Some(&current),
                &payload_bytes,
                false,
            ),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Status::AtBaseline,
            Err(error) => return Err(format!("{}: {error}", path.display())),
        };
        match status {
            Status::AtBaseline => {
                write_into(target, file, &payload_bytes)?;
                println!("wrote {file}");
                baselines.insert(file.to_string(), json!(baseline::hash(&payload_bytes)));
            }
            Status::Modified => println!("kept {file} (locally modified)"),
            Status::Overridden | Status::Deleted => {
                unreachable!("overridden is handled above; a missing file is never classified")
            }
        }
    }
    for file in delete_retired(target, RETIRED)? {
        println!("removed {file}");
    }

    if previously_seeded {
        let topic_files = knowledge_topic_files();
        for topic_file in &topic_files {
            for line in upsert_topic_entries(
                target,
                topic_file,
                &effective_prefix,
                &mut baselines,
                &overrides,
            )? {
                println!("{line}");
            }
        }
        for file in SEED_ONCE.iter().filter(|file| !topic_files.contains(*file)) {
            if target.join(file).exists() || is_overridden(&overrides, file) {
                continue;
            }
            write_into(target, file, &payload_content(file, &effective_prefix)?)?;
            println!("wrote {file}");
        }
    }

    let stamped_version = marker
        .get("version")
        .and_then(Value::as_str)
        .unwrap_or("none")
        .to_string();
    let running_version = kit_version();
    fs::write(
        &marker_path,
        emit(&new_marker(
            &marker,
            running_version.clone(),
            &effective_prefix,
            baselines,
        )),
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
