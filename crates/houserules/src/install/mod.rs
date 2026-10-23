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
//! `init` writes a 2.0 install (`seed`'s own doc has the steps): the minimal
//! default of spec 4.6. It holds the `KIT_OWNED` files, the `SEED_ONCE` files,
//! the `DEFAULT_PACKS` as copies in `knowledge/packs/` with their pins in
//! `.houserules.json` (a new install only), the `.houserules/` line of
//! `.gitignore`, the CI workflow, and the marker with `schema`
//! `houserules/install@2.2.0`. It installs no tracker and no workflow, and so
//! writes none of the five 1.x topic files, no `backlog/` file, and no file of
//! a workflow. `init --tracker json` installs the JSON tracker (`classes`' own
//! module doc names the file classes; `tracker`'s names the trackers).
//! `init --tracker <adapter>` writes the `tracker` key of the marker last,
//! after the files of the adapter, so a stop in between leaves a tree that the
//! next run finishes; an install that has the same tracker keeps it, and one
//! that has another refuses. `init --workflow reference` needs a tracker,
//! installs the reference workflow after it (`workflow`'s own module doc has
//! the files and the pins), and, with `--add-required-packs`, adds the packs
//! that hold the rules the workflow requires through `pack add`. It writes the
//! `workflow` key of the marker last, so that a stop in between leaves a tree
//! that the next run finishes.
//! Without `--target claude` it also leaves out the Claude files
//! (`CLAUDE_FILES`) and the hook entries of `.claude/settings.json`, and
//! prints the line that enables them; with the flag, on a new install or an
//! existing one, it records `claude` in `renderers`, seeds the Claude files
//! that are missing, and keeps the ones that exist. Where `renderers` holds
//! `claude` and the install pins a workflow, `init --workflow` and `init
//! --target claude` add the `SessionStart` entry of the session-start script
//! of the workflow (spec 4.3) to `.claude/settings.json` too.
//!
//! `init` seeds what is missing and keeps what exists: it writes no file that
//! exists, a kit-owned file the project changed included, and lists each one
//! as `kept`. A kit-owned file is baselined only when `init` writes it or
//! finds it at the kit's bytes, so `update` still sees a change. Five writes
//! to a file that exists remain, each by design: the hook entries that
//! `--target claude` and `--workflow` add to a `.claude/settings.json` that
//! `ownedPaths` does not list; the `.houserules/` line that `init` adds to a
//! `.gitignore` that lacks it, or puts in the place of the older line
//! `.houserules/step/` (`ensure_ignore_line`); the areas that a kit pack adds
//! to `knowledge/areas.json`
//! when a new install gets the pack; the stamp of `.houserules.json`; and the
//! files `render` generates.
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
//! under the key `:/<path>`. `init` refuses a new root that nests with another
//! install of the repository and one whose workflow file name another
//! install uses, before it writes anything; a root that already holds its
//! `.houserules.json` is no new root, and `init` runs no such check there.
//! `update` accepts a directory below the top level only when it holds a
//! `.houserules.json`, and rewrites only the workflow file of its own
//! install.
//!
//! # `update` and the ownership baseline
//!
//! `update` resolves the target the same way `init` does, places it in its
//! repository (`location::locate`), and reads and validates the marker
//! (`Marker::read_or_default`). Its steps run in the order of spec section 5:
//!
//! - A directory with no `.houserules.json` and no `knowledge/`, at or above
//!   it within the git top level, is refused with `NO_INSTALL_LINE` (and
//!   `--dir` for a target that is not the working directory, `no_install_line`)
//!   before any write (HR-216): `update` has no install to bring up to date.
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
//! - The conversion steps of batch 34 (`conversion::steps` has them): the
//!   adoption of the reference workflow, the split of the session-start
//!   script, and the delivery of the two schemas, on every run.
//! - The file sync (`sync`'s own module doc has the outcomes): every
//!   `KIT_OWNED` file not in `ownedPaths` is written when it is missing, at
//!   its baseline, or equal to a content that the kit shipped before (spec
//!   4.7, C35); it is kept and reported once when the adopter changed it
//!   (`kept <path> (locally modified)`), and neither written nor listed when
//!   its bytes would not change. A path in `ownedPaths` is left exactly as
//!   found, present or absent, with no report line at all. The D3 files, the
//!   knowledge schema, the deliverables schema, and the CI workflow of the
//!   tracker, join the sync once the marker baselines them. `update` reads the
//!   `backlogId` pattern of the installed tracker for the deliverables schema,
//!   and the Beads adapter asks `bd` for its issue prefix there: a `bd` that
//!   cannot be reached leaves the pattern as it is, prints the unreachable line
//!   on stderr, and does not change the exit status (spec 3.3, C38).
//! - The workflow sync (`workflow`'s own module doc has the outcomes): an
//!   install with a `workflow` key moves the pins of its workflow with the
//!   binary. An install without the key gets no workflow file.
//! - Any `RETIRED` path still present is deleted and reported
//!   (`delete_retired`, below).
//! - Every `SEED_ONCE` path that is entirely absent is written (a later kit
//!   release can add one after an install's `init` already ran) unless
//!   `ownedPaths` lists it; a file that exists is adopter data and stays. The
//!   CI workflow is backfilled the same way, but only once
//!   `origin_is_github_hosted` is true for the target; a non-GitHub origin
//!   leaves it silently absent. An install with the JSON tracker and a
//!   `backlog/` directory gets each absent `TRACKER_DATA` file the same way,
//!   and keeps `backlog/schema.json` current as a D3 file; with the
//!   directory missing, `update` writes nothing under `backlog/`, so that the
//!   tracker stays unreachable until the project restores its data.
//! - The ignore line of `.gitignore` (`ensure_ignore_line`): `.houserules/`
//!   replaces the older line `.houserules/step/` in its place, with the line
//!   ending of the file, and is added where neither line is present. `update`
//!   prints a line for a change and none for a file that holds `.houserules/`.
//!   A `.gitignore` in `ownedPaths` stays as found.
//! - Each hand step that the earlier steps left is printed as `hand step:
//!   <text>` (`hand_steps` holds every text).
//! - The restamp of the install schema: a marker with the schema
//!   `houserules/install@2.1.0` gets `houserules/install@2.2.0` in the write of
//!   the marker that ends `update`, and `update` prints `wrote .houserules.json
//!   (houserules/install@2.2.0)` after that write. The step tests its own
//!   state, so a run that stopped before the write restamps at the next run, and
//!   a second `update` prints no such line (`houserules.migrations-finish-after-a-stop`).
//!   The keys `runners` and `workspaces` stay as found (`marker`'s own module
//!   doc).
//!
//! An install with no `baselines` recorded at all needs no separate migration
//! step: `baseline::classify` treats an unrecorded item the same way whether
//! the whole map is missing or just one key is -- compare its current content
//! directly against the payload, stamp a baseline on a match, and report a
//! divergence without overwriting anything. Nothing already on disk is ever
//! overwritten by a bare reconciliation run; only a genuine at-baseline match
//! triggers a write.
//!
//! An install with a marker but no `knowledge/schema.json` present before
//! this run starts skips the conversion, the pack moves, and the `SEED_ONCE`
//! backfill entirely and fails at the same render step every other `update`
//! failure path already names (this module's own "Failure paths" section);
//! there is no kit-shipped knowledge base yet to convert. (A directory with
//! neither a marker nor a `knowledge/` is no install: `update` refuses it
//! before any write, `no_install_line`.) A 1.x marker is enough to convert: the
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
//! that does not exist yet, and a file name `git init`), a new target (one
//! with no `.houserules.json` yet) that nests with another install of the
//! repository or that would write another install's workflow file
//! (`location::refuse_overlap`), a target whose path a workflow cannot hold
//! (`Workflow::for_install`), a malformed
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
//! `update` over a target that was never `init`ed is a failure path too: a
//! directory with no `.houserules.json` and no `knowledge/`, at or above it
//! within the git top level, gets `no_install_line` and exit 2 before `update`
//! writes any file (HR-216). `tests/update.rs`'s own never-`init`ed test pins
//! the line (its own doc comment and `seeded_repo`'s carry the fuller account).

mod ci_workflow;
mod classes;
mod conversion;
pub(crate) mod hand_steps;
mod location;
pub(crate) mod marker;
mod pack_moves;
mod sync;

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
use crate::tracker::{self, Adapter, TrackerConfig, TrackerInstall};
use crate::workflow;

use ci_workflow::Workflow;
use classes::{
    AREAS_FILE, CLAUDE_FILES, CLAUDE_SESSION_START, D3_FILES, DEFAULT_PACKS, DELIVERABLES_FILE,
    KIT_OWNED, PREFIXED, RETIRED, SCHEMA_FILE, SEED_ONCE, TRACKER_DATA, TRACKER_SCHEMA,
};
use location::Placement;
use marker::{
    ID_PREFIX_HINT, INSTALL_SCHEMA, InstallStamp, Marker, Pin, is_id_prefix, unconverted_line,
};

pub(crate) use conversion::known_contents;
pub(crate) use marker::{Guard, refuse_install_schema};

/// The kit payload, embedded from this repository's `template/` at compile
/// time (this module's own doc has the full configuration account).
#[derive(RustEmbed)]
#[folder = "../../template/"]
struct Payload;

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
pub(crate) fn join_components(target: &Path, relative: &str) -> PathBuf {
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
pub(crate) fn write_into(target: &Path, file: &str, content: &[u8]) -> Result<(), String> {
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
pub(crate) fn read_if_present(path: &Path) -> Result<Option<Vec<u8>>, String> {
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
/// (`rules::KitScope`'s own doc has the contract): the `KIT_OWNED` paths and
/// the workflow files that the install pins, minus the ones the stamp lists in
/// `ownedPaths` (`Marker::owned_paths`), which are the project's own, and the
/// topics that the kit's packs serve. A workflow path with no pin, or with a
/// `null` pin, holds a file of the project, and the lint reads none of it.
/// An absent stamp owns no path; a present stamp that
/// `Marker::read_or_default_for_gate` rejects is that function's own named
/// error, which names the stamp by its relative name.
pub(crate) fn kit_citation_scope(root: &Path) -> Result<crate::rules::KitScope, String> {
    let marker = Marker::read_or_default_for_gate(root, "WI")?;
    let owned = marker.owned_paths();
    let workflow_files: Vec<String> = workflow::pins::read(&marker)?
        .into_iter()
        .flat_map(|pin| pin.files)
        .filter_map(|(file, hash)| hash.map(|_| file))
        .filter(|file| !is_overridden(&owned, file))
        .collect();
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
            .chain(workflow_files)
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
    origin_url(target).is_some_and(|url| url.to_lowercase().contains("github.com"))
}

/// The URL of the `origin` remote of the repository of `target`
/// (`git config --get remote.origin.url`, read locally with no network
/// call), or `None` when there is none or `git` fails.
fn origin_url(target: &Path) -> Option<String> {
    let output = Command::new("git")
        .args(["config", "--get", "remote.origin.url"])
        .current_dir(target)
        .output()
        .ok()
        .filter(|output| output.status.success())?;
    Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// The `<owner>/<name>` that a GitHub remote `url` names, in every real
/// remote form (`https://github.com/<owner>/<name>.git`,
/// `git@github.com:<owner>/<name>.git`, `ssh://git@github.com/<owner>/<name>`),
/// or `None` for a URL of another host or of another shape.
fn repository_of_url(url: &str) -> Option<String> {
    let (before, after) = url.trim().split_once("github.com")?;
    if !(before.ends_with('/') || before.ends_with('@')) {
        return None;
    }
    let path = after
        .strip_prefix(':')
        .or_else(|| after.strip_prefix('/'))?
        .trim_end_matches('/');
    let path = path.strip_suffix(".git").unwrap_or(path);
    tracker::is_repo_name(path).then(|| path.to_string())
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
fn merge_settings(path: &Path, prefix: &str) -> Result<bool, String> {
    let template: Value = serde_json::from_slice(&payload_content(SETTINGS_PATH, prefix)?)
        .expect("the embedded settings.json is valid JSON");
    let entries = template["hooks"]["SessionStart"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    merge_session_start(path, entries, |existing, entry| {
        let matcher = |entry: &Value| {
            entry
                .get("matcher")
                .and_then(Value::as_str)
                .map(str::to_string)
        };
        existing
            .iter()
            .any(|present| matcher(present) == matcher(entry))
    })
}

/// Appends each of `entries` to `hooks.SessionStart` of the `settings.json`
/// at `path` for which `present` says the file lacks it: `present` gets the
/// entries of the file as it was read and one of `entries`. Returns whether
/// the merge changed the file.
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
fn merge_session_start(
    path: &Path,
    entries: Vec<Value>,
    present: impl Fn(&[Value], &Value) -> bool,
) -> Result<bool, String> {
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

    let read_len = session_start_arr.len();
    let mut changed = false;
    for entry in entries {
        if present(&session_start_arr[..read_len], &entry) {
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
    /// The Claude Code files: the session-start script and its hook entries
    /// in `.claude/settings.json`.
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
/// is written, listed as `wrote`, and baselined. A file that exists and equals
/// a content that the kit shipped before is the kit's (spec 4.7, C35): it is
/// replaced, listed as `wrote`, and baselined. Any other file that exists is
/// kept and listed as `kept`: its bytes never change, and it is baselined only
/// when they already match the kit's. A path in `owned` is kept when it exists
/// and left absent when it does not, and its baseline never changes.
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
            Some(current)
                if !is_owned
                    && current != content
                    && known_contents(file, prefix)?.contains(&current) =>
            {
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

/// The content of the seed-once `file` that `init` and `update` write when the
/// file is absent: the payload content with the id prefix `prefix`, and for the
/// deliverables schema the `backlogId` pattern that `pattern` decides
/// (`sync::deliverables_content`).
fn seed_content(
    target: &Path,
    prefix: &str,
    file: &str,
    pattern: &sync::BacklogIdPattern,
) -> Result<Vec<u8>, String> {
    if file == DELIVERABLES_FILE {
        sync::deliverables_content(target, prefix, pattern)
    } else {
        payload_content(file, prefix)
    }
}

/// Writes every `SEED_ONCE` file that is absent and reports the ones that
/// exist as `kept`. An absent path in `owned` stays absent. The kit's rules
/// are no seed-once files: they come as packs. A D3 file (`D3_FILES`) is
/// baselined when it is written. The deliverables schema holds the
/// `backlogId` pattern that `pattern` decides. On an existing install
/// (`existing_install`) an absent `knowledge/areas.json` is left to
/// `seed_areas_of_installed_packs` (spec C18). Returns `true` when it wrote
/// `knowledge/areas.json` from the kit.
fn seed_once_files(
    target: &Path,
    prefix: &str,
    owned: &[String],
    existing_install: bool,
    pattern: &sync::BacklogIdPattern,
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
        let content = seed_content(target, prefix, file, pattern)?;
        write_into(target, file, &content)?;
        println!("wrote {file}");
        if D3_FILES.contains(&file) {
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

/// Installs each pack of `DEFAULT_PACKS` into a new install: writes the copy
/// in emit form, adds the areas it declares and `knowledge/areas.json` lacks,
/// and pins it with `source: kit`. A copy that exists is kept. An existing
/// install never reaches this function (spec C18). The other kit packs come
/// with `pack add`.
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
    for pack in kit_packs()
        .into_iter()
        .filter(|pack| DEFAULT_PACKS.contains(&pack.name.as_str()))
    {
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

/// The line of `.gitignore` that keeps the scratch output of the step and the
/// batch workspaces out of the repository (spec 4.4a and batch 35, spec 7).
pub(crate) const IGNORE_LINE: &str = ".houserules/";

/// The older line that `ensure_ignore_line` replaces with `IGNORE_LINE`: it
/// ignores the output of the step and not the workspaces.
const STEP_IGNORE_LINE: &str = ".houserules/step/";

/// The file that holds the ignore line.
const GITIGNORE: &str = ".gitignore";

/// What `ensure_ignore_line` did to the `.gitignore` of an install.
#[derive(Debug, PartialEq, Eq)]
enum IgnoreChange {
    /// The file was absent: it now holds `IGNORE_LINE` alone.
    Wrote,
    /// A `STEP_IGNORE_LINE` line became `IGNORE_LINE`, in its place.
    Replaced,
    /// `IGNORE_LINE` went to the end of the file.
    Added,
    /// The file held `IGNORE_LINE`: nothing changed.
    Present,
}

impl IgnoreChange {
    /// The line that `init` and `update` print for the change. `update` prints
    /// none for `Present`.
    fn line(&self) -> String {
        match self {
            Self::Wrote => format!("wrote {GITIGNORE}"),
            Self::Replaced => {
                format!("merged {GITIGNORE} ({STEP_IGNORE_LINE} replaced by {IGNORE_LINE})")
            }
            Self::Added => format!("merged {GITIGNORE} ({IGNORE_LINE} added)"),
            Self::Present => format!("kept {GITIGNORE} ({IGNORE_LINE} already present)"),
        }
    }
}

/// `line`, a line of a file with its terminator, split into the text and the
/// terminator (`\r\n`, `\n`, or none for the last line of a file).
fn split_terminator(line: &[u8]) -> (&[u8], &[u8]) {
    let body = line.strip_suffix(b"\n").unwrap_or(line);
    let body = if body.len() < line.len() {
        body.strip_suffix(b"\r").unwrap_or(body)
    } else {
        body
    };
    line.split_at(body.len())
}

/// `true` when the line `text` of `.gitignore` is the pattern `pattern` alone,
/// as git reads it: unescaped trailing spaces do not count, and leading spaces
/// and tabs do. No pattern of the kit holds a backslash, so none of its
/// trailing spaces is escaped.
fn is_ignore_line(text: &[u8], pattern: &str) -> bool {
    let end = text
        .iter()
        .rposition(|byte| *byte != b' ')
        .map_or(0, |last| last + 1);
    &text[..end] == pattern.as_bytes()
}

/// Brings the `.gitignore` of the install root to hold `IGNORE_LINE`: it writes
/// a new file, replaces the first `STEP_IGNORE_LINE` line in its place with the
/// line ending of the file, or appends the line to a file that holds neither,
/// and keeps every other line of the project byte for byte. A file that holds
/// `IGNORE_LINE` stays as it is. A `.gitignore` in `owned` is the project's own
/// and stays as it is: the result is then `None`.
fn ensure_ignore_line(target: &Path, owned: &[String]) -> Result<Option<IgnoreChange>, String> {
    if is_overridden(owned, GITIGNORE) {
        return Ok(None);
    }
    let Some(mut bytes) = read_if_present(&target.join(GITIGNORE))? else {
        write_into(target, GITIGNORE, format!("{IGNORE_LINE}\n").as_bytes())?;
        return Ok(Some(IgnoreChange::Wrote));
    };
    let lines: Vec<&[u8]> = bytes.split_inclusive(|byte| *byte == b'\n').collect();
    let texts: Vec<&[u8]> = lines.iter().map(|line| split_terminator(line).0).collect();
    if texts.iter().any(|text| is_ignore_line(text, IGNORE_LINE)) {
        return Ok(Some(IgnoreChange::Present));
    }
    if let Some(at) = texts
        .iter()
        .position(|text| is_ignore_line(text, STEP_IGNORE_LINE))
    {
        let mut replaced = Vec::with_capacity(bytes.len());
        for (index, line) in lines.iter().enumerate() {
            if index == at {
                replaced.extend_from_slice(IGNORE_LINE.as_bytes());
                replaced.extend_from_slice(split_terminator(line).1);
            } else {
                replaced.extend_from_slice(line);
            }
        }
        write_into(target, GITIGNORE, &replaced)?;
        return Ok(Some(IgnoreChange::Replaced));
    }
    if !bytes.is_empty() && !bytes.ends_with(b"\n") {
        bytes.push(b'\n');
    }
    bytes.extend_from_slice(format!("{IGNORE_LINE}\n").as_bytes());
    write_into(target, GITIGNORE, &bytes)?;
    Ok(Some(IgnoreChange::Added))
}

/// Brings the `.gitignore` of the install root to hold `IGNORE_LINE` and prints
/// the line of the change (`ensure_ignore_line`), if the project does not own
/// the file.
fn seed_gitignore(target: &Path, owned: &[String]) -> Result<(), String> {
    if let Some(change) = ensure_ignore_line(target, owned)? {
        println!("{}", change.line());
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

/// Adds the `SessionStart` entry that runs the session-start script of the
/// workflow (spec 4.3) to `.claude/settings.json`, where the install pins a
/// workflow and `owned` lists neither the settings file nor the script. The
/// caller has checked that the Claude renderer is on. A file that has a hook
/// command which names the script has the entry already. It prints one line
/// when it changes the file.
fn seed_workflow_hook(target: &Path, marker: &Marker) -> Result<(), String> {
    let Some(pin) = workflow::pins::read(marker)? else {
        return Ok(());
    };
    let owned = marker.owned_paths();
    if is_overridden(&owned, SETTINGS_PATH) || is_overridden(&owned, workflow::SESSION_START_SCRIPT)
    {
        return Ok(());
    }
    let names_the_script = |entry: &Value| {
        entry["hooks"].as_array().is_some_and(|hooks| {
            hooks.iter().any(|hook| {
                hook["command"]
                    .as_str()
                    .is_some_and(|command| command.contains(workflow::SESSION_START_SCRIPT))
            })
        })
    };
    let added = merge_session_start(
        &target.join(SETTINGS_PATH),
        vec![workflow::session_start_entry()],
        |existing, _| existing.iter().any(names_the_script),
    )?;
    if added {
        println!(
            "merged {SETTINGS_PATH} ({} workflow SessionStart hook added)",
            pin.name
        );
    }
    Ok(())
}

/// Seeds `target` as a 2.0 install from the embedded payload: the
/// `KIT_OWNED` files (`seed_kit_owned`), the absent `SEED_ONCE` files, the
/// `DEFAULT_PACKS` with their pins (a new install only: an existing 2.0 install
/// gets no pack and no area, spec C18), the `.houserules/` line of
/// `.gitignore`, the deliverables schema and the CI workflow for a GitHub-hosted
/// origin (each in the form of the tracker that the same command installs, so
/// that the tracker's own write of it finds it current; with no tracker flag,
/// in the form of the tracker that the marker names, HR-220), and
/// `.claude/settings.json` with the hook entry; then it stamps
/// `.houserules.json` and renders the generated markdown. It writes no file
/// that exists but five (this module's own doc names them), and it never
/// seeds a path in the marker's owned paths. Without
/// `target_claude`, and on an install whose `renderers` lacks `claude`, it
/// leaves out the Claude Code files and the hook entry (spec 7, C9) and
/// prints the line that enables them. Every payload file this writes is
/// rewritten with `Marker::id_prefix_or`'s resolution: the marker's own
/// stamped `idPrefix` when one is already on record, falling back to the
/// `--id-prefix` flag and then `WI` -- never the flag alone, so a re-`init`
/// over an install that already committed to a prefix cannot backfill a
/// `PREFIXED` file under a different one. An install of houserules 1.x is
/// refused: `update` converts it. So is a new install root (one with no
/// marker yet) that nests with another install of the repository or that
/// would write another install's workflow file (`location::refuse_overlap`),
/// and a root whose path a workflow cannot hold (`Workflow::for_install`);
/// each refusal comes before any write. A target that holds its own marker is
/// no new root: `init` runs no overlap check there, as `update` runs none, so
/// that the remedy of line A (`init --tracker json`) works on an install
/// whose repository tracks another `.houserules.json` below the root.
///
/// With a `workflow_request`, `init` refuses before any write when the install
/// has no tracker and `request` names none (spec 4.3), and installs the
/// workflow after the tracker (`install_workflow`).
///
/// With a tracker `request`, `init` then installs the tracker (spec 3.3): the
/// refusals of `tracker_to_install` come before any write too, the per-adapter
/// work follows the stamp (`tracker::prepare_init`), and the `tracker` key of
/// the marker is the last write before the render, so that a stop in between
/// leaves a tree that the next `init --tracker` finishes.
fn seed(
    target: &Path,
    id_prefix: Option<String>,
    target_claude: bool,
    request: Option<&TrackerRequest>,
    workflow_request: Option<&WorkflowRequest>,
) -> Result<(), String> {
    let placement = location::locate(target)?;
    let prefix = id_prefix.unwrap_or_else(|| "WI".to_string());
    if !is_id_prefix(&prefix) {
        return Err(format!("id-prefix {ID_PREFIX_HINT}"));
    }
    let mut marker = Marker::read_or_default(target, &prefix)?;
    if marker.is_1_x() {
        return Err(unconverted_line(marker.path()));
    }
    if marker.is_fresh() {
        location::refuse_overlap(&placement, &location::other_installs(&placement)?)?;
    }
    let tracker_install = request
        .map(|request| tracker_to_install(target, &marker, request))
        .transpose()?
        .flatten();
    let manifest = workflow_request.map(|request| request.workflow.manifest());
    if let Some(manifest) = &manifest
        && request.is_none()
        && marker.tracker()?.is_none()
    {
        return Err(workflow::needs_tracker_at_init(manifest));
    }
    let effective_prefix = marker.id_prefix_or(&prefix);
    let adapter = match &tracker_install {
        Some(install) => Some(install.config.adapter),
        None => marker.tracker()?.map(|config| config.adapter),
    };
    let workflow = Workflow::for_install(
        &placement,
        &ci_workflow::template_for(adapter, &effective_prefix)?,
    )?;
    let backlog_id_pattern = match &tracker_install {
        Some(install) => sync::BacklogIdPattern::Named(
            tracker::install_backlog_id_pattern(target, &effective_prefix, install)
                .map_err(|error| error.to_string())?,
        ),
        None => tracker_backlog_id_pattern(target, &marker)?,
    };
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
        &backlog_id_pattern,
        &mut baselines,
    )?;
    if !existing_install {
        seed_packs(target, &mut pins, areas_seeded)?;
    } else if areas_absent {
        seed_areas_of_installed_packs(target, &mut pins)?;
    }
    seed_gitignore(target, &owned)?;
    seed_workflow(&placement, &workflow, &owned, &mut baselines)?;
    if claude {
        seed_settings(target, &effective_prefix, &owned)?;
        seed_workflow_hook(target, &marker)?;
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
    if let Some(install) = &tracker_install {
        install_tracker(target, &mut marker, install)?;
    }
    if let (Some(request), Some(manifest)) = (workflow_request, &manifest) {
        install_workflow(target, &mut marker, request, manifest)?;
    }
    crate::rules::render_and_report(target)?;
    println!("houserules: initialized {}", target.display());
    if !claude {
        println!("{ENABLE_CLAUDE_LINE}");
    }
    println!("next: houserules check-knowledge && houserules check-backlog");
    Ok(())
}

/// A workflow that `init --workflow` installs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub(crate) enum InitWorkflow {
    /// The reference workflow: agent templates, skills, evals, and a commit-msg
    /// hook that fill the report format of the kit.
    Reference,
}

impl InitWorkflow {
    /// The manifest of the workflow, as the binary carries it.
    fn manifest(self) -> workflow::Manifest {
        match self {
            InitWorkflow::Reference => workflow::reference(),
        }
    }
}

/// What `init --workflow` asks for: the workflow, and whether `init` adds the
/// packs that hold the rules the workflow requires.
pub(crate) struct WorkflowRequest {
    /// The workflow to install.
    pub(crate) workflow: InitWorkflow,
    /// The `--add-required-packs` flag.
    pub(crate) add_required_packs: bool,
}

/// Installs the workflow `manifest` into the install that `marker` holds,
/// which `init` has stamped and written, with its tracker. The packs that
/// provide the required rules and that the install lacks are added through
/// `pack add` with `--add-required-packs` (the marker is read again, since
/// the command writes it), and named in one line without it. Then it writes
/// the files and the pins (`workflow::install`), and the marker, the last
/// write.
fn install_workflow(
    target: &Path,
    marker: &mut Marker,
    request: &WorkflowRequest,
    manifest: &workflow::Manifest,
) -> Result<(), String> {
    let missing = workflow::requirements::packs_to_add(manifest, &marker.pins()?);
    if request.add_required_packs {
        for name in &missing {
            for line in crate::pack::add(target, name)? {
                println!("{line}");
            }
        }
        *marker = Marker::read(target)?;
    } else if !missing.is_empty() {
        println!(
            "{}",
            workflow::requirements::add_packs_line(manifest, &missing)
        );
    }
    for line in workflow::install(target, marker, manifest)? {
        println!("{line}");
    }
    if marker.claude_enabled() {
        seed_workflow_hook(target, marker)?;
    }
    marker.write()
}

/// What `init --tracker` asks for: the adapter, and for GitHub the
/// repository that `--repo` names.
pub(crate) struct TrackerRequest {
    /// The adapter to install.
    pub(crate) adapter: Adapter,
    /// The `--repo` value, when the command line has one.
    pub(crate) repo: Option<String>,
}

/// The tracker that `init` installs for `request` on the install that
/// `marker` holds, or `None` when the install already has that adapter, so
/// that `init --tracker json` on a JSON install does nothing for the tracker.
/// The refusals of spec 3.3, each one line and each before any write: a
/// `--repo` that is no `<owner>/<name>` or that goes with another adapter, an
/// install with another tracker, `github` or `beads` over a `backlog/`
/// directory, `github` with neither `--repo` nor an `origin` remote that names
/// a repository, and `beads` with no `bd`, no store, or a store whose issue
/// prefix `bd` cannot name. The one read of that prefix is here, so that a
/// refused `init --tracker` writes nothing on every adapter (C37).
fn tracker_to_install(
    target: &Path,
    marker: &Marker,
    request: &TrackerRequest,
) -> Result<Option<TrackerInstall>, String> {
    if let Some(repo) = &request.repo {
        if request.adapter != Adapter::Github {
            return Err(tracker::lines::REPO_NEEDS_GITHUB.to_string());
        }
        if !tracker::is_repo_name(repo) {
            return Err(tracker::lines::repo_is_malformed(repo));
        }
    }
    if let Some(installed) = marker.tracker()? {
        return if installed.adapter == request.adapter {
            Ok(None)
        } else {
            Err(tracker::lines::already_has_tracker(
                installed.adapter.as_str(),
            ))
        };
    }
    if request.adapter != Adapter::Json && target.join("backlog").is_dir() {
        return Err(tracker::lines::BACKLOG_HOLDS_JSON_DATA.to_string());
    }
    let repo = match request.adapter {
        Adapter::Github => Some(
            request
                .repo
                .clone()
                .or_else(|| origin_url(target).and_then(|url| repository_of_url(&url)))
                .ok_or_else(|| tracker::lines::GITHUB_NEEDS_REPOSITORY.to_string())?,
        ),
        Adapter::Json | Adapter::Beads => None,
    };
    let issue_prefix = match request.adapter {
        Adapter::Beads => {
            Some(tracker::beads::prefix_for_init(target).map_err(|error| error.to_string())?)
        }
        Adapter::Json | Adapter::Github => None,
    };
    Ok(Some(TrackerInstall {
        config: TrackerConfig {
            adapter: request.adapter,
            repo,
        },
        issue_prefix,
    }))
}

/// Installs the tracker `install` into the install that `marker` holds, which
/// `init` has stamped and written: the per-adapter work and its lines
/// (`tracker::prepare_init`), then the `tracker` key, the last write. The
/// `backlogId` pattern that `prepare_init` writes is a setting of the install
/// and no change by the project, so a deliverables schema that was at its
/// baseline stays at it.
fn install_tracker(
    target: &Path,
    marker: &mut Marker,
    install: &TrackerInstall,
) -> Result<(), String> {
    let config = &install.config;
    let schema = join_components(target, DELIVERABLES_FILE);
    let before = read_if_present(&schema)?;
    for line in tracker::prepare_init(target, marker, install).map_err(|error| error.to_string())? {
        println!("{line}");
    }
    if let (Some(before), Some(after)) = (before, read_if_present(&schema)?)
        && before != after
    {
        let mut baselines = marker.baselines();
        if baseline_hash(&baselines, DELIVERABLES_FILE) == Some(baseline::hash(&before).as_str()) {
            baselines.insert(DELIVERABLES_FILE.to_string(), json!(baseline::hash(&after)));
            marker.set_baselines(baselines);
        }
    }
    marker.set_tracker(config);
    marker.write()?;
    match &config.repo {
        Some(repo) => println!("tracker: {} ({repo})", config.adapter.as_str()),
        None => println!("tracker: {}", config.adapter.as_str()),
    }
    Ok(())
}

/// Seeds the files of the JSON tracker into `target`: each `TRACKER_DATA`
/// file and `TRACKER_SCHEMA` that is missing is written, listed as `wrote`;
/// one that exists is kept and listed as `kept`, whatever it holds; a path in
/// `ownedPaths` is kept when it exists and left absent when it does not. The
/// schema is a D3 file: it is baselined in `marker` when written here or
/// found at the kit's bytes. Returns the lines `init` prints.
pub(crate) fn seed_tracker_files(
    target: &Path,
    marker: &mut Marker,
) -> Result<Vec<String>, String> {
    let prefix = marker.id_prefix_or("WI");
    let owned = marker.owned_paths();
    let mut baselines = marker.baselines();
    let mut lines = Vec::new();
    for &file in TRACKER_DATA.iter().chain([&TRACKER_SCHEMA]) {
        let is_owned = is_overridden(&owned, file);
        let content = payload_content(file, &prefix)?;
        match read_if_present(&join_components(target, file))? {
            None if is_owned => continue,
            None => {
                write_into(target, file, &content)?;
                lines.push(format!("wrote {file}"));
            }
            Some(current) => {
                lines.push(format!("kept {file}"));
                if is_owned || current != content {
                    continue;
                }
            }
        }
        if file == TRACKER_SCHEMA {
            baselines.insert(file.to_string(), json!(baseline::hash(&content)));
        }
    }
    marker.set_baselines(baselines);
    Ok(lines)
}

/// Writes the seeded CI workflow of the install for the tracker `adapter`
/// (`ci_workflow::write_for`); the lines name each file written.
pub(crate) fn write_ci_workflow(
    root: &Path,
    marker: &mut Marker,
    adapter: Option<Adapter>,
) -> Result<Vec<String>, String> {
    ci_workflow::write_for(root, marker, adapter)
}

/// Runs the `init` subcommand: resolves `dir` via
/// `node_path::resolve_like_node` (this module's own doc explains why
/// this, not `crate::root::resolve_root`'s git-root walk, is the correct
/// resolution here), then seeds it from the embedded payload, and installs
/// the tracker that `request` names and the workflow that `workflow_request`
/// names. Every error is one named stderr line and exit 2
/// (`houserules.crash-paths-are-named`).
pub(crate) fn cmd_init(
    dir: Option<PathBuf>,
    id_prefix: Option<String>,
    target: Option<InitTarget>,
    request: Option<TrackerRequest>,
    workflow_request: Option<WorkflowRequest>,
) -> ExitCode {
    let root = match resolve_like_node(dir.as_deref().unwrap_or_else(|| Path::new("."))) {
        Ok(root) => root,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::from(2);
        }
    };
    match seed(
        &root,
        id_prefix,
        target == Some(InitTarget::Claude),
        request.as_ref(),
        workflow_request.as_ref(),
    ) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("{message}");
            ExitCode::from(2)
        }
    }
}

/// The refusal of `update` for a directory that holds no install at or above
/// it, within the git top level (spec 8.2, HR-216): there is nothing to bring
/// up to date, and `init` starts an install in the working directory.
/// `no_install_line` adds the target when it is another directory.
const NO_INSTALL_LINE: &str =
    "no houserules install here (no .houserules.json); run houserules init";

/// The refusal line of `update` for `target`, the directory that `given` names
/// (the value of `--dir`, `.` when the flag is absent). `init` installs in the
/// working directory, so a target that is another directory gets the remedy
/// `run houserules init --dir <given>`, with `given` as the user wrote it,
/// quoted for a POSIX shell where it needs quoting (`shell::quote`).
fn no_install_line(target: &Path, given: &Path) -> String {
    if std::env::current_dir().is_ok_and(|working| working == target) {
        NO_INSTALL_LINE.to_string()
    } else {
        format!(
            "{NO_INSTALL_LINE} --dir {}",
            crate::shell::quote(&given.to_string_lossy())
        )
    }
}

/// `true` when the install root of `placement`, or a directory between it and
/// the git top level, holds a `.houserules.json` or a `knowledge/` directory.
fn holds_an_install_at_or_above(placement: &Placement) -> bool {
    (0..=placement.components.len()).any(|depth| {
        let directory = placement.components[..depth]
            .iter()
            .fold(placement.top_level.clone(), |parent, part| {
                parent.join(part)
            });
        directory.join(crate::root::INSTALL_MARKER).exists() || directory.join("knowledge").is_dir()
    })
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
/// origin is GitHub hosted; a D3 file gets its baseline. With the JSON
/// tracker (`plan.json_tracker`) it also writes each absent `TRACKER_DATA` file
/// and the schema of the tracker. The deliverables schema holds the `backlogId`
/// pattern of the plan (`sync::deliverables_content`).
fn backfill_missing_files(
    plan: &sync::Plan,
    placement: &Placement,
    baselines: &mut Map<String, Value>,
) -> Result<(), String> {
    let (target, prefix, owned, workflow) = (plan.target, plan.prefix, plan.owned, plan.workflow);
    let tracker_data: &[&str] = if plan.json_tracker { TRACKER_DATA } else { &[] };
    let tracker_schema: &[&str] = if plan.json_tracker {
        &[TRACKER_SCHEMA]
    } else {
        &[]
    };
    for file in SEED_ONCE.iter().chain(tracker_data).chain(tracker_schema) {
        if target.join(file).exists() || is_overridden(owned, file) {
            continue;
        }
        let content = seed_content(target, prefix, file, plan.backlog_id_pattern)?;
        write_into(target, file, &content)?;
        println!("wrote {file}");
        if D3_FILES.contains(file) || *file == TRACKER_SCHEMA {
            baselines.insert(file.to_string(), json!(baseline::hash(&content)));
        }
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

/// What decides the `backlogId` pattern of the install at `root` (spec 3.9):
/// the pattern that its tracker names, the pattern of the kit (any token) for
/// an install with no tracker, or the pattern that the schema holds for a
/// tracker that cannot name its pattern (Beads with an unreachable `bd`):
/// `init` with no tracker flag and `update` print the unreachable line of
/// spec 3.4 on stderr and go on (C38). A `tracker` key that cannot be read is
/// an error.
fn tracker_backlog_id_pattern(
    root: &Path,
    marker: &Marker,
) -> Result<sync::BacklogIdPattern, String> {
    let Some(tracker) = tracker::installed(root, marker).map_err(|error| error.to_string())? else {
        return Ok(sync::BacklogIdPattern::Kit);
    };
    Ok(match tracker.backlog_id_pattern() {
        Ok(pattern) => sync::BacklogIdPattern::Named(pattern),
        Err(error) => {
            eprintln!("{error}");
            sync::BacklogIdPattern::Held
        }
    })
}

/// Syncs `target` from the embedded payload against its recorded ownership
/// baseline (this module's own "`update` and the ownership baseline" doc
/// section has the full account), in the order of spec section 5: the
/// conversion of a 1.x install (`conversion::convert`), which stops before
/// any write at step 0 and for a tree without entries in `knowledge/`; the
/// pack moves (`pack_moves`) and the one-id-one-source step
/// (`pack::one_source`); the file sync (`sync`); then the retired paths, the
/// seed-once backfill, the restamp of `.houserules.json` (its install schema
/// included), the render, and the stamped-to-running version drift.
/// Every `KIT_OWNED` file is overwritten, kept, or -- unless the project
/// owns it -- restored if the project deleted it, since kit machinery that
/// the project has not claimed is always present after `update`. Every
/// payload file this writes is rewritten with `Marker::id_prefix_or`'s
/// resolution (`seed`'s own doc explains why the marker's stamped prefix
/// wins over the flag). `given` is the `--dir` value as the user wrote it, `.`
/// when the flag is absent: the refusal of a target with no install names it
/// (`no_install_line`).
fn update(target: &Path, given: &Path, id_prefix: Option<String>) -> Result<(), String> {
    let placement = location::locate(target)?;
    let id_prefix_given = id_prefix.is_some();
    let prefix = id_prefix.unwrap_or_else(|| "WI".to_string());
    if !is_id_prefix(&prefix) {
        return Err(format!("id-prefix {ID_PREFIX_HINT}"));
    }
    let mut marker = Marker::read_or_default(target, &prefix)?;
    if marker.is_fresh() && !holds_an_install_at_or_above(&placement) {
        return Err(no_install_line(target, given));
    }
    if !placement.is_top_level() && marker.is_fresh() {
        let root = placement.root();
        return Err(format!(
            "{}: this directory is inside the git repository at {} and holds no {}; run houserules init --dir {} to install there",
            root.display(),
            placement.top_level.display(),
            crate::root::INSTALL_MARKER,
            crate::shell::quote(&root.to_string_lossy())
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
        &payload_content(ci_workflow::TEMPLATE_PATH, &effective_prefix)?,
    )?;

    let mut hand_steps = Vec::new();
    if previously_seeded {
        let ctx = conversion::Context {
            root: target,
            top_level: &placement.top_level,
            prefix: &effective_prefix,
            workflow: &workflow,
        };
        hand_steps = convert_or_move_packs(&ctx, &mut marker)?;
        marker = Marker::read_or_default(target, &prefix)?;
        hand_steps.extend(conversion::steps::run(&ctx, &mut marker)?);
    }
    let owned = marker.owned_paths();
    let mut baselines = marker.baselines();
    let adapter = marker.tracker()?.map(|config| config.adapter);
    let json_tracker = adapter == Some(Adapter::Json) && target.join("backlog").is_dir();
    let kit_workflow = Workflow::for_install(
        &placement,
        &ci_workflow::template_for(adapter, &effective_prefix)?,
    )?;
    let backlog_id_pattern = tracker_backlog_id_pattern(target, &marker)?;
    let plan = sync::Plan {
        target,
        prefix: &effective_prefix,
        owned: &owned,
        claude_enabled: marker.claude_enabled(),
        json_tracker,
        adapter,
        backlog_id_pattern: &backlog_id_pattern,
        top_level: &placement.top_level,
        workflow: &kit_workflow,
    };

    let mut sync_lines = Vec::new();
    let synced = sync::sync_owned_files(&plan, &mut baselines, &mut sync_lines, &mut hand_steps);
    for line in &sync_lines {
        println!("{line}");
    }
    synced?;
    for line in workflow::sync(target, &mut marker, &workflow::reference())? {
        println!("{line}");
    }
    for file in delete_retired(target, RETIRED)? {
        println!("removed {file}");
    }

    if previously_seeded {
        backfill_missing_files(&plan, &placement, &mut baselines)?;
    }
    if let Some(change) = ensure_ignore_line(target, &owned)?
        && change != IgnoreChange::Present
    {
        println!("{}", change.line());
    }

    if let Some(line) = workflow::forget_pinned_baselines(&marker, &mut baselines)? {
        println!("{line}");
    }
    for step in &hand_steps {
        println!("{}{step}", hand_steps::PREFIX);
    }
    let stamped_version = marker.version().unwrap_or("none").to_string();
    let running_version = kit_version();
    let restamped_schema = marker.restamp_install_schema();
    marker.restamp(&running_version, &effective_prefix, baselines);
    marker.write()?;
    if restamped_schema {
        println!("wrote {} ({INSTALL_SCHEMA})", crate::root::INSTALL_MARKER);
    }
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
    let given = dir.as_deref().unwrap_or_else(|| Path::new("."));
    let target = match resolve_like_node(given) {
        Ok(target) => target,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::from(2);
        }
    };
    match refuse_install_schema(&target, Guard::ConvertsInstall)
        .and_then(|()| update(&target, given, id_prefix))
    {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("{message}");
            ExitCode::from(2)
        }
    }
}

/// Runs the `files` subcommand: prints the file classes of the kit as JSON:
/// the kit-owned files, the seed-once files, the copies of the kit packs, the
/// D3 files, the tracker files (the five data files and the schema of the
/// JSON tracker), the files of the reference workflow, and its data file.
/// Each path is in exactly one class: the D3 files, the CI workflow among
/// them, are not seed-once files.
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
    let tracker_files: Vec<&str> = TRACKER_DATA
        .iter()
        .copied()
        .chain([TRACKER_SCHEMA])
        .collect();
    print!(
        "{}",
        emit(&json!({
            "kitOwned": KIT_OWNED,
            "seedOnce": seed_once,
            "packCopies": pack_copies,
            "d3Files": D3_FILES,
            "trackerFiles": tracker_files,
            "workflowFiles": workflow::reference().files,
            "workflowData": workflow::DATA_FILES,
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
            .chain(TRACKER_DATA)
            .chain([&TRACKER_SCHEMA, &SETTINGS_PATH])
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
    /// state, over a scratch install: the base holds the kit packs of
    /// `template/packs/` as installed copies, and its root holds the kit-owned
    /// files of `template/` and the files of the reference workflow at their
    /// install paths, as an install with the workflow pins them. The guard
    /// keeps the scratch install alive. Fails when a file is absent from
    /// `template/`, because the lint would skip it without a line.
    fn template_citations() -> (
        tempfile::TempDir,
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
        let manifest = workflow::reference();
        for file in KIT_OWNED {
            let bytes = fs::read(template.join(file)).unwrap_or_else(|error| {
                panic!("{file} is kit-owned but absent from template/, so the lint would skip it: {error}")
            });
            write_into(scratch.path(), file, &bytes).expect("write a kit-owned file");
        }
        for file in &manifest.files {
            write_into(scratch.path(), file, &manifest.content(file))
                .expect("write a workflow file");
        }
        let base = crate::rules::load_base(scratch.path()).expect("the kit packs load");
        let mut scope =
            kit_citation_scope(scratch.path()).expect("the scratch install carries no stamp");
        scope.owned_files.extend(manifest.files.iter().cloned());
        let scan = crate::rules::scan_kit_citations(&base, &scope);
        (scratch, base, scope, scan)
    }

    /// The kit-side proof: every id a kit-owned file or a file of the
    /// reference workflow cites resolves in the kit packs of
    /// `template/packs/`. Both sides come from the code: the file lists from
    /// `kit_citation_scope` and the manifest, the ids from the lint's own
    /// extractor. Two controls keep the pass from being vacuous:
    /// the extractor sees a citation the kit is known to make, and the lint
    /// reports that citation once its entry leaves the base. The test prints
    /// the count of distinct ids it checked (visible with `--nocapture`).
    #[test]
    fn shipped_kit_owned_files_cite_only_shipped_ids() {
        let (_scratch, base, scope, scan) = template_citations();
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
        let (_scratch, _, _, scan) = template_citations();
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
