//! The install surface: bringing the kit into and up to date in a project
//! repository. Owns `init`, `update`, and `files`: the `KIT_OWNED` sync,
//! the conversion of a 1.x install to the 2.0 layout, the pack moves, the
//! `SEED_ONCE` backfill, and the deletion of `RETIRED` paths.
//!
//! # The payload embeds at compile time
//!
//! `Payload` (`rust-embed`) walks `template/` at compile time and bakes
//! every file it holds into the binary, so `init` needs no checkout and
//! no package manager at runtime.
//!
//! `debug-embed` (Cargo.toml) is load-bearing, not cosmetic. Without it,
//! a debug build (a plain `cargo test`/`cargo run`) resolves the embedded
//! `folder` relative to the process's current directory at that moment,
//! not this crate's own `Cargo.toml` directory -- `Payload::get` could
//! then silently read a DIFFERENT `template/` than the one this checkout
//! carries, and a test's outcome would depend on process cwd rather than
//! on what the binary carries. With the feature (or in a release build),
//! `folder` resolves relative to `Cargo.toml` and is genuinely embedded,
//! so a debug build already proves the release build's embed.
//!
//! `#[folder = "../../template/"]` is relative to this crate's own
//! `Cargo.toml` (`crates/houserules/Cargo.toml`), landing on the
//! repository-root `template/`. `walkdir` applies no hidden-file
//! filtering of its own, so `template/`'s dot-directories (`.claude/`,
//! `.githooks/`, `.github/`) embed along with everything else.
//!
//! # `init`
//!
//! `--dir` resolves like `node_path::resolve_like_node`, NOT
//! `crate::root::resolve_root`'s enclosing-git-root walk every read
//! command uses: `init` seeds the directory it is given (or the
//! process's own working directory), never an ancestor.
//!
//! `init` writes a 2.0 install (`seed`'s own doc has the steps): the
//! `KIT_OWNED` files, the `SEED_ONCE` files, the five kit packs as copies in
//! `knowledge/packs/` with their pins in `.houserules.json` (a new install
//! only), the CI workflow, and the marker with `schema`
//! `houserules/install@2.0.0`. It writes none of the five 1.x topic files.
//! Without `--target claude` it also leaves out the Claude files
//! (`CLAUDE_FILES`) and the hook entry of `.claude/settings.json`, and prints
//! the line that enables them; with the flag, on a new install or an existing
//! one, it records `claude` in `renderers`, seeds the Claude files that are
//! missing, and keeps the ones that exist.
//!
//! `init` seeds what is missing and keeps what exists: it writes no file that
//! exists, a kit-owned file the project changed included, and lists each one
//! as `kept`. A kit-owned file is baselined only when `init` writes it or
//! finds it at the kit's bytes, so `update` still sees a change. Four writes
//! to a file that exists remain, each by design: the hook entry that
//! `--target claude` adds to a `.claude/settings.json` that `ownedPaths` does
//! not list; the areas that a kit pack adds to `knowledge/areas.json` when a
//! new install gets the pack; the stamp of `.houserules.json`; and the files
//! `render` generates.
//!
//! `init` on an existing 2.0 install installs no pack and adds no area (spec
//! C18): the pins name the installed packs, and only the `pack` commands and
//! `update` change them. So it puts back no pack that `pack remove` took out,
//! and it writes no pack copy and adds no area to `knowledge/areas.json`
//! there. A pinned kit pack whose copy is missing is `update`'s to restore. An
//! absent `knowledge/areas.json` is the one file `init` seeds there with
//! something other than the kit's seed: the render that ends `init` needs it,
//! so `init` writes it with the areas that the installed packs declare, each
//! recorded in the pin of the pack that declares it
//! (`seed_areas_of_installed_packs`). When `ownedPaths` lists the absent file,
//! `init` refuses before it writes anything and names it.
//!
//! `init` never seeds a path in `ownedPaths` (a `KIT_OWNED` or `SEED_ONCE`
//! file, the CI workflow, or `.claude/settings.json`): such a path is kept
//! when it exists and left absent when it does not. A new install has no
//! `ownedPaths`, so a pack copy and `knowledge/areas.json` never fall in them.
//!
//! When `init` seeds `knowledge/areas.json` itself, each area in it that
//! holds the globs of a kit pack came with that pack: the pin of the first
//! pack that declares it records the area in `addedAreas`, and
//! `check-knowledge` leaves it out of its dead-glob check (spec C16). So a
//! fresh `init` passes `check-knowledge` while `tools/**` matches no file.
//! An `areas.json` that the project already holds is the project's own.
//!
//! `render_and_report` (`rules::render`) reruns this crate's own renderer
//! on the freshly-seeded target (`rules::render`'s own module doc has the
//! shared-helper account).
//!
//! # Several installs
//!
//! An install root is the git top level or a directory below it
//! (`location`'s own module doc has the placement). An install below the top
//! level keeps `.claude/`, `.githooks/`, and its knowledge base in its own
//! root, and writes one file outside it: its CI workflow at the top level
//! (`workflow`'s own module doc has the form and the file name), baselined
//! under the key `:/<path>`. `init` refuses a root that nests with another
//! install of the repository and one whose workflow file name another
//! install uses, before it writes anything. `update` accepts a directory
//! below the top level only when it holds a `.houserules.json`, and rewrites
//! only the workflow file of its own install.
//!
//! # `update` and the ownership baseline
//!
//! `update` resolves the target the same way `init` does, places it in its
//! repository (`location::locate`), and reads and validates the marker
//! (`Marker::read_or_default`). Its steps run in the order of spec section 5:
//!
//! - A directory with a `knowledge/` directory and no `.houserules.json` is
//!   refused unless `--id-prefix` is given (D4's remedy names the flag), so no
//!   remedy stamps `WI` on a project with its own prefix.
//! - The conversion (`conversion`'s own module doc has the steps): while the
//!   marker has no `schema` key, `update` converts the 1.x tree to the 2.0
//!   layout. A topic file `knowledge/overrides.json` of 1.x stops it before any
//!   write.
//! - The pack moves (`pack_moves`' own module doc has the three states that
//!   keep a pack as it is) and the one-id-one-source step
//!   (`pack::one_source`): the pin of each kit pack moves to the version of the
//!   binary, and a local entry with a pack id becomes an override.
//! - The file sync (`sync`'s own module doc has the outcomes): every
//!   `KIT_OWNED` file not in `ownedPaths` is written when it is missing or at
//!   its baseline, kept and reported once when the adopter changed it
//!   (`kept <path> (locally modified)`), and neither written nor listed when
//!   its bytes would not change. A path in `ownedPaths` is left exactly as
//!   found, present or absent, with no report line at all. The two D3 files,
//!   `knowledge/schema.json` and the CI workflow, join the sync once the marker
//!   baselines them.
//! - Any `RETIRED` path still present is deleted and reported
//!   (`delete_retired`, below).
//! - Every `SEED_ONCE` path that is entirely absent is written (a later kit
//!   release can add one after an install's `init` already ran) unless
//!   `ownedPaths` lists it; a file that exists is adopter data and stays. The
//!   CI workflow is backfilled the same way, but only once
//!   `origin_is_github_hosted` is true for the target; a non-GitHub origin
//!   leaves it silently absent.
//! - Each hand step that the earlier steps left is printed as `hand step:
//!   <text>` (`hand_steps` holds every text).
//!
//! An install with no `baselines` recorded at all needs no separate migration
//! step: `baseline::classify` treats an unrecorded item the same way whether
//! the whole map is missing or just one key is -- compare its current content
//! directly against the payload, stamp a baseline on a match, and report a
//! divergence without overwriting anything. Nothing already on disk is ever
//! overwritten by a bare reconciliation run; only a genuine at-baseline match
//! triggers a write.
//!
//! An install that was never `init`ed at all -- no `.houserules.json` of 1.x
//! and no `knowledge/schema.json` present before this run starts -- skips the
//! conversion, the pack moves, and the `SEED_ONCE` backfill entirely and fails
//! at the same render step every other `update` failure path already names
//! (this module's own "Failure paths" section); there is no kit-shipped
//! knowledge base yet to convert. A 1.x marker is enough to convert: the
//! schema file and `knowledge/areas.json` of such an install may be absent
//! (spec C21), and the conversion puts them back, or stops before any write
//! when `ownedPaths` lists the absent file. A 1.x marker with a `knowledge/`
//! directory that does not exist or is empty is not converted (spec C21):
//! `update` stops before any write, because it cannot tell which entries the
//! project deleted.
//! `update` still writes every `KIT_OWNED` file and deletes any `RETIRED` path
//! present before that failure, exactly as it always has.
//!
//! `update`'s own report ends with the version drift as one
//! `kit <stamped> -> <running> ` line, reusing the marker read before the
//! restamp overwrote it. The `none` token
//! (`quality.absence-is-designed`) covers one shape: a stamp that exists but
//! carries no `version` key keeps its `idPrefix`. A JSON `null` at `version`
//! is NOT this arm -- `marker.version !== undefined` is true for `null`, so it
//! fails the same "must be a non-empty string" named error every other
//! invalid `version` shape gets.
//!
//! # Deletion
//!
//! `RETIRED`, below, is a fixed list in the binary of paths the kit does
//! not ship. `delete_retired` removes each one present under `target`
//! and reports it, one path per line (`removed <path>`), mirroring the
//! `wrote <path>` shape the sync step uses.
//!
//! A `RETIRED` path is deleted whether or not the adopter has changed
//! it, because the kit owns these paths' lifecycle, not the adopter:
//! `delete_retired` checks only that the path exists, never its content.
//! An edited copy of a retired file is removed exactly like an untouched
//! one.
//!
//! `delete_retired`'s own tests inject their own list directly, a plain
//! function parameter rather than a `RETIRED` override, so the mechanism
//! itself is proved independent of what `RETIRED` happens to hold;
//! `retired_holds_the_shell_tools_and_the_js_engines_they_fronted` pins
//! `RETIRED`'s exact, production contents: six paths, two shell wrappers
//! and the four JS engine files they fronted.
//!
//! # Failure paths (`houserules.crash-paths-are-named`)
//!
//! `init` prints one named stderr line and exits 2 for: a target that is
//! not in a git repository (`location::locate`: a target outside one, one
//! that does not exist yet, and a file name `git init`), a target
//! that nests with another install of the repository or that would write
//! another install's workflow file (`location::refuse_overlap`), a target
//! whose path a workflow cannot hold (`Workflow::for_install`), a malformed
//! `--id-prefix`, and a pre-existing `.houserules.json` that is not valid JSON, is valid
//! JSON but not an object, carries an invalid `idPrefix`, carries an
//! empty/non-string `version`, or carries a `defaultBranch` that is no string
//! (`audit` reads that key). An invalid-JSON marker's inner message text is
//! `serde_json`'s own (`read_json_object`'s own doc), the same shape
//! `rules::deliverable::read_deliverable_value` carries for the same
//! reason. A pre-existing `.houserules.json` with no `schema` key is an
//! install of houserules 1.x: `init` names it and says that `update`
//! converts it, and changes nothing. An existing 2.0 install whose
//! `knowledge/areas.json` is absent and listed in `ownedPaths` is refused
//! before any write, with a line that names the file. A target directory that
//! already holds unrelated files, or a second `init` run over an
//! already-seeded target, is NOT a failure path at all: `init` seeds what is
//! missing, keeps what already exists, and leaves every unrelated file
//! untouched.
//!
//! `update` shares every one of `init`'s marker-validation errors above
//! (`Marker::read_or_default`'s own doc), even though the two commands are separate
//! CLI surfaces: unparseable JSON, a non-object, a bad `idPrefix`, an
//! empty `version`, and a `null` `version` all exit 2, with no `wrote
//! <file>` line ahead of it, since the marker is read and validated
//! before any file is written or printed. `update`'s own `--id-prefix`
//! is validated the same way `init`'s is, exit 2 on the same malformed
//! flag.
//!
//! `update` prints one named stderr line and exits 2 for a tree it cannot
//! convert without losing an entry, and changes nothing before it says so: a
//! topic file `knowledge/overrides.json` of 1.x (a hand step), an id that two
//! topic files hold or that is active and archived at once, a
//! `knowledge/schema.json` or a `knowledge/areas.json` that `ownedPaths` lists
//! and that does not exist (spec C21), a kit pack with no pin whose copy path
//! holds a file of the project (spec C20), a 1.x install whose `knowledge/`
//! directory does not exist or is empty (the line says to restore the directory
//! from version control and run `update` again, or to remove `.houserules.json`
//! and run `init --id-prefix <prefix>` for a fresh install, with the `idPrefix`
//! of the marker; spec C21), and a `knowledge/` directory with no
//! `.houserules.json` when `--id-prefix` is missing. A failure after the
//! conversion wrote its first file leaves a tree that the next `update`
//! finishes (`conversion`'s own module doc has the order of the writes;
//! `pack_moves`' has the order of a pack move).
//!
//! `update` and `init` refuse a tree whose install schema is newer than the
//! major that this binary writes (D4, spec C22): one line that names the
//! release it needs, exit 2. They refuse a tree whose install schema is no
//! install schema id the same way (spec C23): the line is the one that a
//! reading command prints, and they change no file.
//! `init` refuses before it reads anything else (`main` runs D4). `update`
//! refuses after its self-update phase and before its repo phase
//! (`cmd_update` runs D4), so that it can install the release that the line
//! names.
//!
//! `update` over a target that was never `init`ed is a failure path too:
//! it prints one named line naming `<target>/knowledge/schema.json` and
//! exits 2. The exact `io::Error` text and path separator are
//! platform-specific; `tests/update.rs`'s own never-`init`ed test
//! derives the expected line from a real error on the test's own
//! platform instead of hardcoding one (its own doc comment and
//! `seeded_repo`'s carry the fuller account).

mod conversion;
mod hand_steps;
mod location;
pub(crate) mod marker;
mod pack_moves;
mod sync;
mod workflow;

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use rust_embed::RustEmbed;
use serde_json::{Map, Value, json};

use crate::archive::write_json;
use crate::baseline;
use crate::emit::emit;
use crate::node_path::resolve_like_node;
use crate::pack::areas::{add_pack_areas, seeded_areas_of};
use crate::packs::{kit_packs, pack_sha256, parse_pack};

use location::Placement;
use marker::{ID_PREFIX_HINT, InstallStamp, Marker, Pin, is_id_prefix, unconverted_line};
use workflow::Workflow;

pub(crate) use marker::{Guard, refuse_install_schema, stamped_id_prefix};

/// The kit payload, embedded from this repository's `template/` at compile
/// time (this module's own doc has the full configuration account).
#[derive(RustEmbed)]
#[folder = "../../template/"]
struct Payload;

/// Machinery files houserules owns: `init` writes them, and `update`
/// overwrites each one the adopter has not changed (this module's own doc,
/// "`update` and the ownership baseline", has the keep and override cases).
/// `tests/install.rs`'s own copy pins this list, so the two cannot silently
/// drift apart.
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

/// The `KIT_OWNED` files that only the Claude Code renderer needs: the agent
/// templates preload the `project-knowledge` skill, which only that renderer
/// writes, and the session-start script runs from the hook entry of
/// `.claude/settings.json`. An install whose `renderers` lacks `claude` has
/// none of them, and the sync restores none.
const CLAUDE_FILES: &[&str] = &[
    "tools/claude-session-start.sh",
    ".claude/agents/implementer.md",
    ".claude/agents/task-reviewer.md",
    ".claude/agents/branch-reviewer.md",
];

/// Project-data files `init` seeds once, and `update` never overwrites once
/// they exist: an adopter's own edits to backlog items, evals, or
/// `AGENTS.md`/`CLAUDE.md` are never kit-owned content. `update` still
/// BACKFILLS a `SEED_ONCE` path
/// that is entirely absent (this module's own "update" doc section has the
/// full account) -- a later kit release can add a new path to this list, and
/// an install seeded by an older release never had a chance to receive it.
/// A subset of these paths -- `knowledge_topic_files`, below -- is the 1.x
/// layout's topic files: `init` seeds none of them (the kit's rules come as
/// packs), and `update` reconciles them entry by entry on a 1.x tree only.
const SEED_ONCE: &[&str] = &[
    "knowledge/schema.json",
    "knowledge/areas.json",
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
    "docs/README.md",
    "AGENTS.md",
    "CLAUDE.md",
];

/// The knowledge schema, a D3 file: the sync covers it once
/// `.houserules.json` baselines it.
const SCHEMA_FILE: &str = "knowledge/schema.json";

/// The one list of areas, which the project owns.
const AREAS_FILE: &str = "knowledge/areas.json";

/// The D3 files: `init` seeds each once, and `update` then keeps it at the
/// content of the kit through a baseline, unless the project changed it or
/// owns it (`sync`'s own module doc has the account).
///
/// The CI workflow is the one D3 file that only a GitHub-hosted target gets:
/// the gate itself assumes a GitHub Actions runner and a GitHub PR event, so
/// writing it into a project hosted elsewhere would ship a workflow file
/// nothing ever runs. `seed` (`init`) writes it when the origin is GitHub
/// hosted (`origin_is_github_hosted`), keeps a file that exists and reports
/// it, and skips the write with a line that says why for any other origin.
/// `update` backfills a still-absent workflow the same way, but only once the
/// origin has become GitHub-hosted; skipping it for another origin stays
/// silent there, matching `update`'s own convention of reporting only what it
/// changes. A project not on GitHub gets its own CI gate from the
/// `migrating-knowledge` skill's own instruction instead.
const D3_FILES: &[&str] = &[SCHEMA_FILE, workflow::TEMPLATE_PATH];

/// Seed files that carry the backlog id prefix; `--id-prefix` rewrites
/// them.
const PREFIXED: &[&str] = &[
    "backlog/schema.json",
    "backlog/items/general.json",
    ".claude/schemas/deliverables.json",
];

/// Paths the kit does not ship, that `update` deletes from an install if
/// present (this module's own doc, "Deletion", has the full account of
/// the mechanism).
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

/// The kit version stamped into `.houserules.json`'s `version` field --
/// `env!("CARGO_PKG_VERSION")`, the same value `houserules --version`
/// reports (`tests/version.rs`), baked in by cargo itself at compile
/// time from `Cargo.toml`, the only version source in this repository.
fn kit_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

/// Reads one payload file from the compile-time embedded `Payload`,
/// rewriting the backlog id prefix where it applies.
fn payload_content(file: &str, prefix: &str) -> Result<Vec<u8>, String> {
    let bytes = Payload::get(file)
        .unwrap_or_else(|| panic!("{file} is a fixed, checked-in KIT_OWNED/SEED_ONCE/settings path missing from the embedded payload"))
        .data
        .into_owned();
    with_id_prefix(file, bytes, prefix)
}

/// `bytes`, the content of the kit file `file`, with the backlog id prefix
/// `prefix` where the file carries one (`PREFIXED`). Any other file, and the
/// default prefix `WI`, give `bytes` back.
fn with_id_prefix(file: &str, bytes: Vec<u8>, prefix: &str) -> Result<Vec<u8>, String> {
    if !PREFIXED.contains(&file) || prefix == "WI" {
        return Ok(bytes);
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

/// `true` for a file that must stay executable: a shell script or a git
/// hook.
fn is_executable_file(file: &str) -> bool {
    file.ends_with(".sh") || file.starts_with(".githooks/")
}

/// Writes `content` to `file` under `target`, keeping shell scripts and
/// git hooks executable.
fn write_into(target: &Path, file: &str, content: &[u8]) -> Result<(), String> {
    let path = target.join(file);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| format!("{}: {error}", parent.display()))?;
    }
    fs::write(&path, content).map_err(|error| format!("{}: {error}", path.display()))?;
    if is_executable_file(file) {
        mark_executable(&path)?;
    }
    Ok(())
}

/// The bytes of the file at `path`, or `None` when the file does not exist.
/// Any other read error is one line that names the path with the operating
/// system's own text.
fn read_if_present(path: &Path) -> Result<Option<Vec<u8>>, String> {
    match fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(format!("{}: {error}", path.display())),
    }
}

/// Writes `content` to `file` under `target` unless the file already holds
/// exactly those bytes. A script or hook stays executable either way.
/// Returns `true` when it wrote the file.
fn write_if_changed(target: &Path, file: &str, content: &[u8]) -> Result<bool, String> {
    let path = target.join(file);
    if read_if_present(&path)?.is_some_and(|current| current == content) {
        if is_executable_file(file) {
            mark_executable(&path)?;
        }
        return Ok(false);
    }
    write_into(target, file, content)?;
    Ok(true)
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
/// object. The invalid-JSON message embeds `serde_json`'s own error text
/// (`rules::deliverable::read_deliverable_value`'s own doc names the same
/// shape).
fn read_json_object(path: &Path) -> Result<Map<String, Value>, String> {
    let text = fs::read_to_string(path).map_err(|error| format!("{}: {error}", path.display()))?;
    let value: Value = serde_json::from_str(&text)
        .map_err(|error| format!("{}: invalid JSON ({error})", path.display()))?;
    match value {
        Value::Object(map) => Ok(map),
        _ => Err(format!("{}: not a JSON object", path.display())),
    }
}

/// What `check-knowledge`'s kit-owned citation lint needs from this module
/// (`rules::KitScope`'s own doc has the contract): the `KIT_OWNED` paths
/// minus the ones the stamp lists in `ownedPaths` (`Marker::owned_paths`),
/// which are the project's own, and the topics that the kit's packs serve.
/// An absent stamp owns no path; a present stamp that
/// `Marker::read_or_default_for_gate` rejects is that function's own named
/// error, which names the stamp by its relative name.
pub(crate) fn kit_citation_scope(root: &Path) -> Result<crate::rules::KitScope, String> {
    let owned = Marker::read_or_default_for_gate(root, "WI")?.owned_paths();
    let mut seeded_topics: Vec<String> = kit_packs()
        .into_iter()
        .flat_map(|pack| pack.topics.into_iter().map(|(topic, _)| topic))
        .collect();
    seeded_topics.sort();
    seeded_topics.dedup();
    Ok(crate::rules::KitScope {
        owned_files: KIT_OWNED
            .iter()
            .filter(|file| !is_overridden(&owned, file))
            .map(|file| file.to_string())
            .collect(),
        seeded_topics,
    })
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

/// `true` when `target`'s configured `origin` remote resolves to a
/// GitHub-hosted URL: `git config --get remote.origin.url`, read locally
/// with no network call, matched for a `github.com` host segment against
/// every real remote form (`https://github.com/<org>/<repo>.git`,
/// `git@github.com:<org>/<repo>.git`, `ssh://git@github.com/<org>/<repo>`)
/// -- the host segment is the same literal substring in all three. `false`
/// covers every non-matching case alike: no `origin` configured at all (a
/// bare `git init`, most of this crate's own test fixtures), a `git`
/// invocation that fails outright, and an `origin` hosted anywhere else
/// (GitLab, Bitbucket, a self-hosted forge) -- `seed`/`update` read this as
/// one boolean gate, never distinguishing why it came back `false`.
fn origin_is_github_hosted(target: &Path) -> bool {
    let output = Command::new("git")
        .args(["config", "--get", "remote.origin.url"])
        .current_dir(target)
        .output();
    let Ok(output) = output else {
        return false;
    };
    if !output.status.success() {
        return false;
    }
    String::from_utf8_lossy(&output.stdout)
        .trim()
        .to_lowercase()
        .contains("github.com")
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
/// present. Returns whether the merge changed the file.
///
/// Two cases worth naming explicitly: a `null` at `hooks` or
/// `hooks.SessionStart` is treated the same as a missing key -- replaced
/// with the default (an empty object or array) before either type check
/// -- rather than left as `null` to fail `as_object_mut`/`as_array_mut`
/// below.
///
/// A `hooks` value that is itself a JSON ARRAY is its own case, not an
/// error: this arm leaves the array's own elements untouched on disk, the
/// attempted merge has no effect, and `changed` is still `true` (the
/// write always runs), exit 0.
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

/// A render target that `init --target` enables.
#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub(crate) enum InitTarget {
    /// The Claude Code files: the agent templates, the session-start script,
    /// and its hook entry in `.claude/settings.json`.
    Claude,
}

/// The path of a kit pack's copy below the install root.
fn pack_copy_path(name: &str) -> String {
    format!("knowledge/packs/{name}.json")
}

/// The sentence that `init` prints when the Claude renderer is off.
const ENABLE_CLAUDE_LINE: &str =
    "for Claude Code: run houserules init --target claude in the install root";

/// Seeds every `KIT_OWNED` file that `claude` allows. A file that is missing
/// is written, listed as `wrote`, and baselined. A file that exists is kept
/// and listed as `kept`: its bytes never change, and it is baselined only
/// when they already match the kit's. A path in `owned` is kept when it
/// exists and left absent when it does not, and its baseline never changes.
fn seed_kit_owned(
    target: &Path,
    prefix: &str,
    claude: bool,
    owned: &[String],
    baselines: &mut Map<String, Value>,
) -> Result<(), String> {
    for &file in KIT_OWNED {
        if CLAUDE_FILES.contains(&file) && !claude {
            continue;
        }
        let path = target.join(file);
        let content = payload_content(file, prefix)?;
        let is_owned = is_overridden(owned, file);
        match read_if_present(&path)? {
            None if is_owned => continue,
            None => {
                write_into(target, file, &content)?;
                println!("wrote {file}");
            }
            Some(current) => {
                println!("kept {file}");
                if is_owned || current != content {
                    continue;
                }
                if is_executable_file(file) {
                    mark_executable(&path)?;
                }
            }
        }
        baselines.insert(file.to_string(), json!(baseline::hash(&content)));
    }
    Ok(())
}

/// Writes every `SEED_ONCE` file that is absent and reports the ones that
/// exist as `kept`. An absent path in `owned` stays absent. The kit's rules
/// are no seed-once files: they come as packs. `knowledge/schema.json` is
/// baselined when it is written. On an existing install (`existing_install`)
/// an absent `knowledge/areas.json` is left to `seed_areas_of_installed_packs`
/// (spec C18). Returns `true` when it wrote `knowledge/areas.json` from the
/// kit.
fn seed_once_files(
    target: &Path,
    prefix: &str,
    owned: &[String],
    existing_install: bool,
    baselines: &mut Map<String, Value>,
) -> Result<bool, String> {
    let mut wrote_areas = false;
    for &file in SEED_ONCE {
        if target.join(file).exists() {
            println!("kept {file}");
            continue;
        }
        if is_overridden(owned, file) || (existing_install && file == AREAS_FILE) {
            continue;
        }
        let content = payload_content(file, prefix)?;
        write_into(target, file, &content)?;
        println!("wrote {file}");
        if file == SCHEMA_FILE {
            baselines.insert(file.to_string(), json!(baseline::hash(&content)));
        }
        wrote_areas |= file == AREAS_FILE;
    }
    Ok(wrote_areas)
}

/// Seeds the absent `knowledge/areas.json` of an existing 2.0 install with the
/// areas that its installed packs declare, and nothing else (spec C18). Each
/// pin records the areas its pack added to the new file, as `init` records them
/// for a new install; the record of the old file is gone with it. A pack whose
/// copy is missing adds no area: `update` restores the copy.
fn seed_areas_of_installed_packs(
    target: &Path,
    pins: &mut BTreeMap<String, Pin>,
) -> Result<(), String> {
    write_json(&target.join("knowledge").join("areas.json"), &json!({}))?;
    println!("wrote {AREAS_FILE}");
    for (name, pin) in pins.iter_mut() {
        let relative = pack_copy_path(name);
        let Some(bytes) = read_if_present(&join_components(target, &relative))? else {
            pin.added_areas.clear();
            continue;
        };
        let raw: Value = serde_json::from_slice(&bytes)
            .map_err(|error| format!("{relative}: invalid JSON ({error})"))?;
        let pack = parse_pack(raw, &relative)?;
        let mut record = Vec::new();
        let lines = add_pack_areas(target, &pack, &mut record)?;
        pin.added_areas = record;
        for line in lines {
            println!("{line}");
        }
    }
    Ok(())
}

/// Installs each kit pack into a new install: writes the copy in emit form,
/// adds the areas it declares and `knowledge/areas.json` lacks, and pins it
/// with `source: kit`. A copy that exists is kept. An existing install never
/// reaches this function (spec C18).
///
/// When `areas_seeded` is `true`, `init` wrote `knowledge/areas.json` from the
/// kit in this run, so each area that holds the globs of the pack that
/// declares it came with that pack. The pin of the first such pack records it
/// as added (`pack::areas::seeded_areas_of`), and `check-knowledge` leaves it
/// out of its dead-glob check (spec C16). An `areas.json` that the project
/// held before is the project's own: nothing of it is recorded.
fn seed_packs(
    target: &Path,
    pins: &mut BTreeMap<String, Pin>,
    areas_seeded: bool,
) -> Result<(), String> {
    for pack in kit_packs() {
        let relative = pack_copy_path(&pack.name);
        let copy = join_components(target, &relative);
        if copy.exists() {
            println!("kept {relative}");
            continue;
        }
        let mut added_areas = Vec::new();
        let area_lines = add_pack_areas(target, &pack, &mut added_areas)?;
        if areas_seeded {
            let claimed: BTreeSet<String> = pins
                .values()
                .flat_map(|pin| pin.added_areas.iter().cloned())
                .chain(added_areas.iter().cloned())
                .collect();
            added_areas.extend(seeded_areas_of(target, &pack, &claimed)?);
        }
        write_json(&copy, &pack.raw)?;
        pins.insert(
            pack.name.clone(),
            Pin {
                version: pack.version.clone(),
                source: "kit".to_string(),
                sha256: pack_sha256(&pack.raw),
                added_areas,
                commit: None,
            },
        );
        println!("wrote {relative}");
        for line in area_lines {
            println!("{line}");
        }
    }
    Ok(())
}

/// Seeds the CI workflow of the install once, at the git top level, when the
/// origin is GitHub hosted, and baselines it under its top-level key. An
/// existing file is kept; an absent path in `owned` stays absent; a
/// non-GitHub origin skips the write and says why. Every line names the file
/// by its path below the top level.
fn seed_workflow(
    placement: &Placement,
    workflow: &Workflow,
    owned: &[String],
    baselines: &mut Map<String, Value>,
) -> Result<(), String> {
    let file = workflow.path.as_str();
    if placement.top_level.join(file).exists() {
        println!("kept {file}");
        return Ok(());
    }
    if is_overridden(owned, file) {
        return Ok(());
    }
    if !origin_is_github_hosted(&placement.top_level) {
        println!(
            "skipped {file} (origin is not GitHub-hosted; see the migrating-knowledge skill to add your own CI gate)"
        );
        return Ok(());
    }
    write_into(&placement.top_level, file, &workflow.content)?;
    println!("wrote {file}");
    baselines.insert(
        workflow.key.clone(),
        json!(baseline::hash(&workflow.content)),
    );
    Ok(())
}

/// Seeds `.claude/settings.json`, or merges the `SessionStart` hook entries
/// of the kit into the file that exists. A settings file in `owned` is kept
/// as it is, or left absent.
fn seed_settings(target: &Path, prefix: &str, owned: &[String]) -> Result<(), String> {
    let settings_path = target.join(SETTINGS_PATH);
    if is_overridden(owned, SETTINGS_PATH) {
        if settings_path.exists() {
            println!("kept {SETTINGS_PATH}");
        }
        return Ok(());
    }
    if settings_path.exists() {
        if merge_settings(&settings_path, prefix)? {
            println!("merged {SETTINGS_PATH} (SessionStart hooks added)");
        } else {
            println!("kept {SETTINGS_PATH} (hooks already present)");
        }
    } else {
        write_into(
            target,
            SETTINGS_PATH,
            &payload_content(SETTINGS_PATH, prefix)?,
        )?;
        println!("wrote {SETTINGS_PATH}");
    }
    Ok(())
}

/// Seeds `target` as a 2.0 install from the embedded payload: the
/// `KIT_OWNED` files (`seed_kit_owned`), the absent `SEED_ONCE` files, the
/// five kit packs with their pins (a new install only: an existing 2.0 install
/// gets no pack and no area, spec C18), the CI workflow for a GitHub-hosted
/// origin, and `.claude/settings.json` with the hook entry; then it stamps
/// `.houserules.json` and renders the generated markdown. It writes no file
/// that exists but four (this module's own doc names them), and it never
/// seeds a path in the marker's owned paths. Without
/// `target_claude`, and on an install whose `renderers` lacks `claude`, it
/// leaves out the Claude Code files and the hook entry (spec 7, C9) and
/// prints the line that enables them. Every payload file this writes is
/// rewritten with `Marker::id_prefix_or`'s resolution: the marker's own
/// stamped `idPrefix` when one is already on record, falling back to the
/// `--id-prefix` flag and then `WI` -- never the flag alone, so a re-`init`
/// over an install that already committed to a prefix cannot backfill a
/// `PREFIXED` file under a different one. An install of houserules 1.x is
/// refused: `update` converts it. So is an install root that nests with
/// another install of the repository or that would write another install's
/// workflow file (`location::refuse_overlap`), and a root whose path a
/// workflow cannot hold (`Workflow::for_install`); each refusal comes before
/// any write.
fn seed(target: &Path, id_prefix: Option<String>, target_claude: bool) -> Result<(), String> {
    let placement = location::locate(target)?;
    let prefix = id_prefix.unwrap_or_else(|| "WI".to_string());
    if !is_id_prefix(&prefix) {
        return Err(format!("id-prefix {ID_PREFIX_HINT}"));
    }
    let mut marker = Marker::read_or_default(target, &prefix)?;
    if marker.is_1_x() {
        return Err(unconverted_line(marker.path()));
    }
    location::refuse_overlap(&placement, &location::other_installs(&placement)?)?;
    let effective_prefix = marker.id_prefix_or(&prefix);
    let workflow = Workflow::for_install(
        &placement,
        &payload_content(workflow::TEMPLATE_PATH, &effective_prefix)?,
    )?;
    let claude = target_claude || (!marker.is_fresh() && marker.claude_enabled());
    let existing_install = marker.is_2_0();
    let owned = marker.owned_paths();
    let mut baselines = marker.baselines();
    let mut pins = marker.pins()?;
    let areas_path = join_components(target, AREAS_FILE);
    let areas_absent = !areas_path.exists();
    if existing_install && areas_absent && is_overridden(&owned, AREAS_FILE) {
        return Err(format!(
            "{}: this file is in ownedPaths and does not exist, so init cannot seed it; restore it, or remove it from ownedPaths",
            areas_path.display()
        ));
    }

    seed_kit_owned(target, &effective_prefix, claude, &owned, &mut baselines)?;
    let areas_seeded = seed_once_files(
        target,
        &effective_prefix,
        &owned,
        existing_install,
        &mut baselines,
    )?;
    if !existing_install {
        seed_packs(target, &mut pins, areas_seeded)?;
    } else if areas_absent {
        seed_areas_of_installed_packs(target, &mut pins)?;
    }
    seed_workflow(&placement, &workflow, &owned, &mut baselines)?;
    if claude {
        seed_settings(target, &effective_prefix, &owned)?;
    }

    let renderers = marker.renderers_for_init(target_claude);
    marker.stamp_install(InstallStamp {
        version: &kit_version(),
        id_prefix: &effective_prefix,
        pins: &pins,
        renderers: renderers.as_deref(),
        baselines,
    });
    marker.write()?;
    crate::rules::render_and_report(target)?;
    println!("houserules: initialized {}", target.display());
    if !claude {
        println!("{ENABLE_CLAUDE_LINE}");
    }
    println!("next: houserules check-knowledge && houserules check-backlog");
    Ok(())
}

/// Runs the `init` subcommand: resolves `dir` via
/// `node_path::resolve_like_node` (this module's own doc explains why
/// this, not `crate::root::resolve_root`'s git-root walk, is the correct
/// resolution here), then seeds it from the embedded payload. Every error
/// is one named stderr line and exit 2 (`houserules.crash-paths-are-named`).
pub(crate) fn cmd_init(
    dir: Option<PathBuf>,
    id_prefix: Option<String>,
    target: Option<InitTarget>,
) -> ExitCode {
    let root = match resolve_like_node(dir.as_deref().unwrap_or_else(|| Path::new("."))) {
        Ok(root) => root,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::from(2);
        }
    };
    match seed(&root, id_prefix, target == Some(InitTarget::Claude)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("{message}");
            ExitCode::from(2)
        }
    }
}

/// The refusal of `update` for a tree that has a `knowledge/` directory and
/// no marker: the project passes its backlog id prefix, so that no remedy
/// stamps `WI` on a project with its own prefix (spec C14).
fn missing_marker_refusal(marker: &Path, knowledge: &Path) -> String {
    format!(
        "{}: not found, but {} exists; run houserules update --id-prefix <prefix> with the id prefix of your backlog (WI for WI-001), so that update stamps the right one",
        marker.display(),
        knowledge.display()
    )
}

/// The first two steps of `update` that follow step 0 (spec section 5): the
/// conversion of a 1.x install, or the pack moves of a 2.0 install, and then
/// the one-id-one-source step over the knowledge base as they left it.
/// Returns the hand steps they left.
fn convert_or_move_packs(
    ctx: &conversion::Context<'_>,
    marker: &mut Marker,
) -> Result<Vec<String>, String> {
    let hand_steps = if marker.is_2_0() {
        pack_moves::move_kit_packs(ctx.root)?
    } else {
        conversion::convert(ctx, marker, &kit_packs())?
    };
    let base = crate::rules::load_base(ctx.root).map_err(|error| error.to_string())?;
    for line in crate::pack::one_source::one_source(ctx.root, &base)? {
        println!("{line}");
    }
    Ok(hand_steps)
}

/// Writes every `SEED_ONCE` path that is absent and that `owned` does not
/// list (a later kit release can add one after the install's `init` ran), and
/// the CI workflow of the install when it is absent, not owned, and the
/// origin is GitHub hosted; the workflow gets its baseline.
fn backfill_missing_files(
    target: &Path,
    prefix: &str,
    owned: &[String],
    placement: &Placement,
    workflow: &Workflow,
    baselines: &mut Map<String, Value>,
) -> Result<(), String> {
    for file in SEED_ONCE {
        if target.join(file).exists() || is_overridden(owned, file) {
            continue;
        }
        write_into(target, file, &payload_content(file, prefix)?)?;
        println!("wrote {file}");
    }
    let top_level_workflow = placement.top_level.join(&workflow.path);
    if !top_level_workflow.exists()
        && !is_overridden(owned, &workflow.path)
        && origin_is_github_hosted(&placement.top_level)
    {
        write_into(&placement.top_level, &workflow.path, &workflow.content)?;
        println!("wrote {}", workflow.path);
        baselines.insert(
            workflow.key.clone(),
            json!(baseline::hash(&workflow.content)),
        );
    }
    Ok(())
}

/// Syncs `target` from the embedded payload against its recorded ownership
/// baseline (this module's own "`update` and the ownership baseline" doc
/// section has the full account), in the order of spec section 5: the
/// conversion of a 1.x install (`conversion::convert`), which stops before
/// any write at step 0 and for a tree without entries in `knowledge/`; the
/// pack moves (`pack_moves`) and the one-id-one-source step
/// (`pack::one_source`); the file sync (`sync`); then the retired paths, the
/// seed-once backfill, the restamp of `.houserules.json`, the render, and the
/// stamped-to-running version drift.
/// Every `KIT_OWNED` file is overwritten, kept, or -- unless the project
/// owns it -- restored if the project deleted it, since kit machinery that
/// the project has not claimed is always present after `update`. Every
/// payload file this writes is rewritten with `Marker::id_prefix_or`'s
/// resolution (`seed`'s own doc explains why the marker's stamped prefix
/// wins over the flag).
fn update(target: &Path, id_prefix: Option<String>) -> Result<(), String> {
    let placement = location::locate(target)?;
    let id_prefix_given = id_prefix.is_some();
    let prefix = id_prefix.unwrap_or_else(|| "WI".to_string());
    if !is_id_prefix(&prefix) {
        return Err(format!("id-prefix {ID_PREFIX_HINT}"));
    }
    let mut marker = Marker::read_or_default(target, &prefix)?;
    if !placement.is_top_level() && marker.is_fresh() {
        let root = placement.root();
        return Err(format!(
            "{}: this directory is inside the git repository at {} and holds no {}; run houserules init --dir {} to install there",
            root.display(),
            placement.top_level.display(),
            crate::root::INSTALL_MARKER,
            root.display()
        ));
    }
    let knowledge = target.join("knowledge");
    if marker.is_fresh() && !id_prefix_given && knowledge.is_dir() {
        return Err(missing_marker_refusal(marker.path(), &knowledge));
    }
    let previously_seeded = marker.is_1_x() || target.join(SCHEMA_FILE).exists();
    let effective_prefix = marker.id_prefix_or(&prefix);
    let workflow = Workflow::for_install(
        &placement,
        &payload_content(workflow::TEMPLATE_PATH, &effective_prefix)?,
    )?;

    let mut hand_steps = Vec::new();
    if previously_seeded {
        hand_steps = convert_or_move_packs(
            &conversion::Context {
                root: target,
                top_level: &placement.top_level,
                prefix: &effective_prefix,
                workflow: &workflow,
            },
            &mut marker,
        )?;
        marker = Marker::read_or_default(target, &prefix)?;
    }
    let owned = marker.owned_paths();
    let mut baselines = marker.baselines();

    let mut sync_lines = Vec::new();
    let synced = sync::sync_owned_files(
        &sync::Plan {
            target,
            prefix: &effective_prefix,
            owned: &owned,
            claude_enabled: marker.claude_enabled(),
            top_level: &placement.top_level,
            workflow: &workflow,
        },
        &mut baselines,
        &mut sync_lines,
    );
    for line in &sync_lines {
        println!("{line}");
    }
    synced?;
    for file in delete_retired(target, RETIRED)? {
        println!("removed {file}");
    }

    if previously_seeded {
        backfill_missing_files(
            target,
            &effective_prefix,
            &owned,
            &placement,
            &workflow,
            &mut baselines,
        )?;
    }

    for step in &hand_steps {
        println!("{}{step}", hand_steps::PREFIX);
    }
    let stamped_version = marker.version().unwrap_or("none").to_string();
    let running_version = kit_version();
    marker.restamp(&running_version, &effective_prefix, baselines);
    marker.write()?;
    crate::rules::render_and_report(target)?;
    println!("kit {stamped_version} -> {running_version}");
    println!("houserules: updated {}", target.display());
    println!("next: houserules check-knowledge && houserules check-backlog");
    Ok(())
}

/// Runs the `update` subcommand: first runs the self-update phase
/// (`selfupdate::run_before_repo_phase`'s own doc has the full account --
/// keeps the installed binary itself current, on the channels that
/// support it, never a hard error), then resolves `dir` the same way
/// `init` does (`cmd_init`'s own doc explains the choice), runs D4 on the
/// target (`Guard::ConvertsInstall`), and syncs it from the embedded payload.
/// D4 runs after the self-update phase, so that `update` can install the
/// release that its line asks for (spec C22), and before the repo phase,
/// which it keeps from writing. Every error from D4 and from the repo phase
/// is one named stderr line and exit 2 (`houserules.crash-paths-are-named`).
pub(crate) fn cmd_update(dir: Option<PathBuf>, id_prefix: Option<String>) -> ExitCode {
    crate::selfupdate::run_before_repo_phase();
    let target = match resolve_like_node(dir.as_deref().unwrap_or_else(|| Path::new("."))) {
        Ok(target) => target,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::from(2);
        }
    };
    match refuse_install_schema(&target, Guard::ConvertsInstall)
        .and_then(|()| update(&target, id_prefix))
    {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("{message}");
            ExitCode::from(2)
        }
    }
}

/// Runs the `files` subcommand: prints the file classes of the kit as JSON:
/// the kit-owned files, the seed-once files, the copies of the kit packs, and
/// the D3 files. Each path is in exactly one class: the D3 files, the CI
/// workflow among them, are not seed-once files.
pub(crate) fn cmd_files() -> ExitCode {
    let seed_once: Vec<&str> = SEED_ONCE
        .iter()
        .copied()
        .filter(|file| !D3_FILES.contains(file))
        .collect();
    let pack_copies: Vec<String> = kit_packs()
        .iter()
        .map(|pack| pack_copy_path(&pack.name))
        .collect();
    print!(
        "{}",
        emit(&json!({
            "kitOwned": KIT_OWNED,
            "seedOnce": seed_once,
            "packCopies": pack_copies,
            "d3Files": D3_FILES,
        }))
    );
    ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Confirms `walkdir`'s no-hidden-file-filtering behavior (this
    /// module's own doc) holds for this crate's real `Payload`: a
    /// dot-directory entry the `KIT_OWNED`/`SEED_ONCE` lists depend on is
    /// present in the compiled-in payload.
    #[test]
    fn the_embedded_payload_carries_every_kit_owned_and_seed_once_path() {
        for file in KIT_OWNED
            .iter()
            .chain(SEED_ONCE)
            .chain(D3_FILES)
            .chain([&SETTINGS_PATH])
        {
            assert!(
                Payload::get(file).is_some(),
                "{file} is missing from the embedded payload"
            );
        }
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

    /// `tests/install.rs`'s
    /// `init_stamps_the_marker_with_the_kit_version_and_the_default_id_
    /// prefix` is the real cross-check, comparing a fresh
    /// `.houserules.json` stamp against that test's own, independent
    /// `env!("CARGO_PKG_VERSION")` -- this unit test only pins that
    /// `kit_version` extracts a non-empty string at all.
    #[test]
    fn kit_version_is_a_non_empty_string() {
        assert!(!kit_version().is_empty());
    }

    /// Injects its own list directly at the `delete_retired` call site
    /// rather than depending on `RETIRED`'s own contents (this module's
    /// own "Deletion" doc section explains the mechanism), which the next
    /// test pins separately.
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
    /// install has nothing to delete: `tests/update.rs`'s own
    /// `update_deletes_retired_shell_tools_from_an_old_install` and
    /// `update_deletes_retired_js_engines_from_an_old_install` cover the
    /// CLI-visible half; this is the one place a hand edit widening or
    /// reordering `RETIRED` shows up as a conscious diff to this exact
    /// list, not a silent pass. An install that still carries any of the
    /// six has it deleted, not resynced, at its next `update`.
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

    /// A scratch git repository, `origin` unset -- `origin_is_github_hosted`'s
    /// own no-remote-configured case.
    fn git_repo_with_no_origin() -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("tempdir");
        let status = std::process::Command::new("git")
            .args(["init", "-q"])
            .current_dir(dir.path())
            .status()
            .expect("run git init");
        assert!(status.success(), "git init failed");
        dir
    }

    /// `git_repo_with_no_origin` plus a configured `origin` remote at `url`.
    fn git_repo_with_origin(url: &str) -> tempfile::TempDir {
        let dir = git_repo_with_no_origin();
        let status = std::process::Command::new("git")
            .args(["remote", "add", "origin", url])
            .current_dir(dir.path())
            .status()
            .expect("run git remote add");
        assert!(status.success(), "git remote add failed");
        dir
    }

    #[test]
    fn origin_is_github_hosted_true_for_an_https_github_origin() {
        let dir = git_repo_with_origin("https://github.com/jblossey/houserules.git");
        assert!(origin_is_github_hosted(dir.path()));
    }

    #[test]
    fn origin_is_github_hosted_true_for_an_ssh_style_github_origin() {
        let dir = git_repo_with_origin("git@github.com:jblossey/houserules.git");
        assert!(origin_is_github_hosted(dir.path()));
    }

    #[test]
    fn origin_is_github_hosted_false_with_no_origin_configured() {
        let dir = git_repo_with_no_origin();
        assert!(!origin_is_github_hosted(dir.path()));
    }

    #[test]
    fn origin_is_github_hosted_false_for_a_non_github_origin() {
        let dir = git_repo_with_origin("https://gitlab.com/jblossey/houserules.git");
        assert!(!origin_is_github_hosted(dir.path()));
    }

    /// This checkout's `template/` directory, resolved at compile time from
    /// the crate's manifest directory.
    fn template_dir() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../template")
    }

    /// The scope lists the `KIT_OWNED` paths as they are and one seeded topic
    /// for each topic that a pack file of `template/packs/` serves. The
    /// expected topics come from those files, not from the embedded packs, so
    /// a topic added to one side only fails here.
    #[test]
    fn kit_citation_scope_carries_the_kit_owned_paths_and_the_seeded_topics() {
        let dir = tempfile::tempdir().expect("tempdir");
        let scope = kit_citation_scope(dir.path()).expect("an absent stamp is no error");
        assert_eq!(scope.owned_files, KIT_OWNED);
        let mut shipped: Vec<String> = fs::read_dir(template_dir().join("packs"))
            .expect("read template/packs")
            .map(|entry| entry.expect("read a directory entry").path())
            .flat_map(|path| {
                let text = fs::read_to_string(&path).expect("read a pack file");
                let pack: Value = serde_json::from_str(&text).expect("parse a pack file");
                pack["topics"]
                    .as_object()
                    .expect("the topics of a pack")
                    .keys()
                    .cloned()
                    .collect::<Vec<_>>()
            })
            .collect();
        shipped.sort();
        shipped.dedup();
        assert!(!shipped.is_empty(), "the template ships no pack");
        assert_eq!(scope.seeded_topics, shipped);
    }

    /// The `overrides` key of a marker owns no path: only `ownedPaths` does,
    /// and the conversion moved the paths of `overrides` there.
    #[test]
    fn kit_citation_scope_reads_owned_paths_and_not_the_overrides_of_a_1_x_marker() {
        let dir = tempfile::tempdir().expect("tempdir");
        fs::write(
            dir.path().join(crate::root::INSTALL_MARKER),
            r#"{"schema": "houserules/install@2.0.0", "idPrefix": "WI", "overrides": [".claude/agents/branch-reviewer.md"]}"#,
        )
        .expect("write the stamp");
        let scope = kit_citation_scope(dir.path()).expect("a valid stamp");
        assert_eq!(scope.owned_files, KIT_OWNED);
    }

    /// A 2.0 install lists the paths it owns in `ownedPaths`, and the scope
    /// leaves them out as it leaves out the `overrides` of a 1.x stamp.
    #[test]
    fn kit_citation_scope_leaves_out_the_kit_owned_files_the_2_0_stamp_owns() {
        let dir = tempfile::tempdir().expect("tempdir");
        fs::write(
            dir.path().join(crate::root::INSTALL_MARKER),
            r#"{"schema": "houserules/install@2.0.0", "idPrefix": "WI", "ownedPaths": [".claude/agents/branch-reviewer.md"]}"#,
        )
        .expect("write the stamp");
        let scope = kit_citation_scope(dir.path()).expect("a valid stamp");
        let expected: Vec<String> = KIT_OWNED
            .iter()
            .filter(|file| **file != ".claude/agents/branch-reviewer.md")
            .map(|file| file.to_string())
            .collect();
        assert_eq!(scope.owned_files, expected);
    }

    /// A stamp that `Marker::read_or_default_for_gate` rejects is its own named error, which
    /// names the stamp by its relative name, never an empty override list.
    #[test]
    fn kit_citation_scope_names_a_malformed_stamp() {
        let dir = tempfile::tempdir().expect("tempdir");
        let marker = dir.path().join(crate::root::INSTALL_MARKER);
        fs::write(&marker, r#"{"idPrefix": "WI", "overrides": "process.tdd"}"#)
            .expect("write the stamp");
        let Err(error) = kit_citation_scope(dir.path()) else {
            panic!("a stamp with a string `overrides` is an error");
        };
        assert_eq!(
            error,
            format!(
                "{}: overrides must be an array of strings",
                crate::root::INSTALL_MARKER
            )
        );
    }

    /// The knowledge base, the scope, and the citation scan of the shipped
    /// state: the base holds the kit packs of `template/packs/` as installed
    /// copies, and its root is `template/`, where the kit-owned files lie.
    /// Fails when a kit-owned file is absent from `template/`, because the
    /// lint would skip it without a line.
    fn template_citations() -> (
        crate::rules::Base,
        crate::rules::KitScope,
        crate::rules::Scan,
    ) {
        let template = template_dir();
        let scratch = tempfile::tempdir().expect("tempdir");
        let knowledge = scratch.path().join("knowledge");
        let packs = knowledge.join("packs");
        fs::create_dir_all(&packs).expect("create knowledge/packs");
        for name in ["schema.json", "areas.json"] {
            fs::copy(template.join("knowledge").join(name), knowledge.join(name))
                .expect("copy a seed file");
        }
        for entry in fs::read_dir(template.join("packs")).expect("read template/packs") {
            let source = entry.expect("read a directory entry").path();
            fs::copy(
                &source,
                packs.join(source.file_name().expect("a file name")),
            )
            .expect("copy a kit pack");
        }
        let mut base = crate::rules::load_base(scratch.path()).expect("the kit packs load");
        base.root = template.clone();
        let scope = kit_citation_scope(&template).expect("template/ carries no stamp");
        for file in &scope.owned_files {
            assert!(
                template.join(file).is_file(),
                "{file} is kit-owned but absent from template/, so the lint would skip it"
            );
        }
        let scan = crate::rules::scan_kit_citations(&base, &scope);
        (base, scope, scan)
    }

    /// The kit-side proof: every id a kit-owned file under `template/`
    /// cites resolves in the kit packs of `template/packs/`. Both sides come from the
    /// code: the kit-owned list from `kit_citation_scope`, the ids from the
    /// lint's own extractor. Two controls keep the pass from being vacuous:
    /// the extractor sees a citation the kit is known to make, and the lint
    /// reports that citation once its entry leaves the base. The test prints
    /// the count of distinct ids it checked (visible with `--nocapture`).
    #[test]
    fn shipped_kit_owned_files_cite_only_shipped_ids() {
        let (base, scope, scan) = template_citations();
        let distinct: std::collections::BTreeSet<&str> = scan
            .cited
            .iter()
            .map(|citation| citation.id.as_str())
            .collect();
        assert!(
            distinct.contains("process.evals-rerun"),
            "the extractor missed the citation of process.evals-rerun in branch-reviewer.md"
        );
        assert_eq!(
            crate::rules::kit_citation_findings(&base, &scope),
            Vec::<String>::new()
        );
        // The control: with the entry removed from the base, the lint names
        // each file that cites it. The empty result above therefore comes
        // from resolving every id, not from a lint that reports nothing.
        let mut without_it = base;
        without_it.entries.remove("process.evals-rerun");
        let findings = crate::rules::kit_citation_findings(&without_it, &scope);
        assert_eq!(
            findings.len(),
            2,
            "expected one finding per citing file: {findings:?}"
        );
        assert!(
            findings
                .iter()
                .all(|finding| finding.contains("cites \"process.evals-rerun\"")),
            "unexpected findings: {findings:?}"
        );
        eprintln!(
            "shipped_kit_owned_files_cite_only_shipped_ids: {} distinct ids in {} files",
            distinct.len(),
            scan.cited
                .iter()
                .map(|citation| citation.file.as_str())
                .collect::<std::collections::BTreeSet<_>>()
                .len()
        );
    }

    /// The `migrating-knowledge` skill tells an adopter to search two
    /// directories for an id before deleting its entry. Every kit-owned file
    /// that cites an id must sit under one of them, or the sentence misses a
    /// citing file. The directories come from the skill's own text. Each
    /// must hold a citing file, or the skill names a directory the kit does
    /// not cite from.
    #[test]
    fn the_migrating_skill_search_covers_every_kit_owned_file_that_cites_an_id() {
        let skill =
            fs::read_to_string(template_dir().join(".claude/skills/migrating-knowledge/SKILL.md"))
                .expect("read the migrating-knowledge skill");
        let search = skill
            .split("`grep -rn '<id>' ")
            .nth(1)
            .and_then(|rest| rest.split('`').next())
            .expect("the skill names its grep before deleting a seeded entry");
        let directories: Vec<&str> = search.split_whitespace().collect();
        assert!(
            !directories.is_empty(),
            "the skill's grep names no directory"
        );
        let (_, _, scan) = template_citations();
        for citation in &scan.cited {
            assert!(
                directories
                    .iter()
                    .any(|dir| citation.file.starts_with(&format!("{dir}/"))),
                "{} cites {} outside the directories the skill searches: {directories:?}",
                citation.file,
                citation.id
            );
        }
        for dir in &directories {
            assert!(
                scan.cited
                    .iter()
                    .any(|citation| citation.file.starts_with(&format!("{dir}/"))),
                "no kit-owned file under {dir} cites an id"
            );
        }
    }
}
