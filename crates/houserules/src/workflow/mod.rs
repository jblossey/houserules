//! The workflow packages of the kit (batch 34, spec 4.1 to 4.3): agent
//! templates, skills, evals, and hooks that fill the core report format and
//! cite rule ids. A package is a directory of `template/workflows/` with a
//! manifest (`manifest`); the binary embeds it, and the kit ships one, the
//! reference workflow.
//!
//! `init --workflow reference` writes the files of the manifest at their
//! install paths and pins each one in the `workflow` key of
//! `.houserules.json` (`pins`): the pin is the SHA-256 of the content that
//! the kit last wrote. `update` moves the pins with the binary:
//!
//! - a file whose bytes equal its pin is written with the new content, and
//!   the pin moves with it;
//! - a file whose bytes differ from its pin stays, and `update` prints
//!   `kept <file> (locally modified; reference workflow <version>)` on every
//!   run until the project merges the change by hand; a file whose bytes the
//!   project merged to equal the new content gets its pin moved, with no line;
//! - a file that is gone comes back;
//! - a path in `ownedPaths` stays as found, with no line;
//! - a path with no pin whose file is not the kit's belongs to the project.
//!   `init --workflow` and `update` report it once, with the line of spec 4.7
//!   step 2, and record `null` for it in `files`: a new binary whose workflow
//!   adds a path that the project already holds gets the line on its first
//!   `update`. A path that `files` maps to `null` gets no line and no write
//!   while the project holds a file there; a gone file comes back, and a file
//!   that equals the kit's content gets a pin, like any other path.
//!
//! A file is the kit's, and so gets a pin, when `.houserules.json`
//! `baselines` holds its path, or its bytes equal the content of the package
//! or a content that the kit shipped before ([`is_kit_file`]). A file pinned
//! at a content that the kit shipped before is the file of an older release:
//! `update` writes it with the new content. `init` writes no file that exists.
//!
//! An install with workflow files of the kit and no `workflow` key is adopted
//! by [`adopt`] (spec 4.7, step 2): each kit file is pinned at its recorded
//! baseline, else at the known content it equals, and an absent file is
//! written.
//!
//! A path that `files` pins has no entry in `baselines`: the pin is its one
//! provenance record. [`install`] and [`adopt`], which create the pins, drop the
//! baselines of the paths they pin and report it in one line, and
//! [`forget_pinned_baselines`] does the same for the baselines that `update`
//! carries, so that an install that an earlier build adopted loses them too.
//!
//! `requirements` checks that the rule ids the files cite come from the
//! installed packs. The data file of a workflow ([`DATA_FILES`]) has no pin:
//! `init` writes it once, and nothing replaces or reports it.

mod manifest;
pub(crate) mod pins;
pub(crate) mod requirements;

use std::path::Path;

use serde_json::{Map, Value};

use crate::baseline::{self, Status};
use crate::install::hand_steps;
use crate::install::marker::Marker;
use crate::install::{join_components, known_contents, read_if_present, write_into};

pub(crate) use manifest::{Manifest, reference};
use pins::WorkflowPin;

/// The project data files of a workflow. `init --workflow` writes each one
/// empty and once; no pin covers it.
pub(crate) const DATA_FILES: &[&str] = &[".claude/evals/record.json"];

/// The install path of the session-start script of the reference workflow.
/// The `SessionStart` hook entry of `.claude/settings.json` runs it
/// ([`session_start_entry`]).
pub(crate) const SESSION_START_SCRIPT: &str = "tools/reference-session-start.sh";

/// The `SessionStart` entry of `.claude/settings.json` that runs
/// [`SESSION_START_SCRIPT`]. It has no matcher, so it runs at every start of
/// a session: Claude Code resolves `CLAUDE_PROJECT_DIR` to the project root.
pub(crate) fn session_start_entry() -> Value {
    serde_json::json!({
        "hooks": [{
            "type": "command",
            "command": format!("${{CLAUDE_PROJECT_DIR}}/{SESSION_START_SCRIPT}"),
        }]
    })
}

/// The refusal of `init --workflow` when the install has no tracker and the
/// command line names none (spec 4.3).
pub(crate) fn needs_tracker_at_init(manifest: &Manifest) -> String {
    let name = &manifest.name;
    format!(
        "the {name} workflow needs a tracker; run houserules init --workflow {name} --tracker json|github|beads"
    )
}

/// The finding of `check-knowledge` for an install that has the workflow and
/// no tracker (spec 3.10).
pub(crate) fn needs_tracker_at_check(manifest: &Manifest) -> String {
    format!(
        "the {} workflow needs a tracker; run houserules init --tracker json|github|beads",
        manifest.name
    )
}

/// The line for a file at a workflow path that is not the kit's (spec 4.7,
/// step 2): the hand step, with its prefix.
fn not_the_kits(manifest: &Manifest, path: &str) -> String {
    format!(
        "{}{}",
        hand_steps::PREFIX,
        hand_steps::not_the_kits(&manifest.name, path)
    )
}

/// The pin that the kit would give the file at `path`, whose bytes are
/// `current`, when it is the kit's: the baseline that `.houserules.json`
/// records for the path, else the hash of the content of the package when the
/// bytes equal it, else the hash of the known content that they equal (a
/// content that the kit shipped before, with the id prefix `prefix`). `None`
/// for a file that is not the kit's.
fn kit_pin(
    baselines: &Map<String, Value>,
    path: &str,
    current: &[u8],
    content: &[u8],
    prefix: &str,
) -> Result<Option<String>, String> {
    if let Some(recorded) = baselines.get(path).and_then(Value::as_str) {
        return Ok(Some(recorded.to_string()));
    }
    if current == content {
        return Ok(Some(baseline::hash(content)));
    }
    let known = known_contents(path, prefix)?;
    Ok(known
        .iter()
        .any(|old| old == current)
        .then(|| baseline::hash(current)))
}

/// `true` when the file at `path` below `root` exists and is the kit's: the
/// baselines of `marker` hold its path, or its bytes equal the content that
/// `manifest` ships for it or a content that the kit shipped before (spec 4.7).
/// The adoption of the workflow files of an install that has no `workflow` key
/// (spec 4.7, step 2) asks it for each path.
pub(crate) fn is_kit_file(
    root: &Path,
    manifest: &Manifest,
    path: &str,
    marker: &Marker,
) -> Result<bool, String> {
    let Some(current) = read_if_present(&join_components(root, path))? else {
        return Ok(false);
    };
    let kit = kit_pin(
        &marker.baselines(),
        path,
        &current,
        &manifest.content(path),
        &marker.id_prefix_or("WI"),
    )?;
    Ok(kit.is_some())
}

/// What a run does with a file that exists.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Run {
    /// `init`: it writes no file that exists.
    Install,
    /// `update`: it moves the pin of a file that the project did not change.
    Sync,
    /// The adoption of the workflow by `update`: as `Sync`, but a file that the
    /// project changed gets no kept line, because the `sync` of the same run
    /// prints it.
    Adopt,
}

/// What a run over the files of a workflow printed.
pub(crate) struct Outcome {
    /// The lines to print, in order. The line of a file left to the project is
    /// among them, with the prefix of a hand step.
    pub(crate) lines: Vec<String>,
    /// The hand steps of the run, without their prefix: one for each file at a
    /// workflow path that is not the kit's.
    pub(crate) hand_steps: Vec<String>,
}

/// One run over the files of a workflow: the pins as they change, and the
/// lines to print.
struct Reconcile<'a> {
    run: Run,
    root: &'a Path,
    manifest: &'a Manifest,
    owned: Vec<String>,
    baselines: Map<String, Value>,
    prefix: String,
    pin: WorkflowPin,
    lines: Vec<String>,
    hand_steps: Vec<String>,
}

impl<'a> Reconcile<'a> {
    /// Starts a run over the workflow of `marker`. An install with a pin of
    /// another workflow is one named line; an install with none starts a pin
    /// at the manifest.
    fn start(
        run: Run,
        root: &'a Path,
        marker: &Marker,
        manifest: &'a Manifest,
    ) -> Result<Self, String> {
        let pin = match pins::read(marker)? {
            Some(pin) if pin.name != manifest.name => {
                return Err(format!(
                    "{}: the install runs the workflow {:?}, and this houserules carries {:?} only",
                    marker.file_name(),
                    pin.name,
                    manifest.name
                ));
            }
            Some(pin) => pin,
            None => WorkflowPin {
                name: manifest.name.clone(),
                version: manifest.version.clone(),
                files: Default::default(),
                models: Default::default(),
            },
        };
        Ok(Self {
            run,
            root,
            manifest,
            owned: marker.owned_paths(),
            baselines: marker.baselines(),
            prefix: marker.id_prefix_or("WI"),
            pin,
            lines: Vec::new(),
            hand_steps: Vec::new(),
        })
    }

    /// `true` when the project owns `path`.
    fn owns(&self, path: &str) -> bool {
        self.owned.iter().any(|owned| owned == path)
    }

    /// Writes `content` to `path` and pushes `wrote <path>`.
    fn write(&mut self, path: &str, content: &[u8]) -> Result<(), String> {
        write_into(self.root, path, content)?;
        self.lines.push(format!("wrote {path}"));
        Ok(())
    }

    /// Pins `path` at the content `hash`.
    fn pin_at(&mut self, path: &str, hash: String) {
        self.pin.files.insert(path.to_string(), Some(hash));
    }

    /// Reports the file at `path` as the project's and records `null` for it,
    /// so that no later run reports it again.
    fn leave_to_the_project(&mut self, path: &str) {
        self.lines.push(not_the_kits(self.manifest, path));
        self.hand_steps
            .push(hand_steps::not_the_kits(&self.manifest.name, path));
        self.pin.files.insert(path.to_string(), None);
    }

    /// Reconciles the file at `path` of the manifest.
    fn file(&mut self, path: &str) -> Result<(), String> {
        if self.owns(path) {
            return Ok(());
        }
        let content = self.manifest.content(path);
        let Some(current) = read_if_present(&join_components(self.root, path))? else {
            self.write(path, &content)?;
            self.pin_at(path, baseline::hash(&content));
            return Ok(());
        };
        let pinned = self.pin.files.get(path).cloned();
        let recorded = match &pinned {
            Some(Some(hash)) => Some(hash.clone()),
            _ => kit_pin(&self.baselines, path, &current, &content, &self.prefix)?,
        };
        let Some(recorded) = recorded else {
            if pinned.is_none() {
                self.leave_to_the_project(path);
            }
            return Ok(());
        };
        if self.run == Run::Install {
            self.lines.push(format!("kept {path}"));
            let pinned = if current == content {
                baseline::hash(&content)
            } else {
                recorded
            };
            self.pin_at(path, pinned);
            return Ok(());
        }
        match baseline::classify(Some(&recorded), Some(&current), &content, false) {
            Status::AtBaseline => {
                if current != content {
                    self.write(path, &content)?;
                }
                self.pin_at(path, baseline::hash(&content));
            }
            Status::Modified => {
                if self.run == Run::Sync {
                    self.lines.push(format!(
                        "kept {path} (locally modified; {} workflow {})",
                        self.manifest.name, self.manifest.version
                    ));
                }
                self.pin_at(path, recorded);
            }
            Status::Overridden | Status::Deleted => {
                unreachable!("the file exists, and the caller decided ownership")
            }
        }
        Ok(())
    }

    /// Writes each data file of the workflow that is absent. `init` runs it;
    /// `update` never does.
    fn data_files(&mut self) -> Result<(), String> {
        for &path in DATA_FILES {
            if self.owns(path) {
                continue;
            }
            if read_if_present(&join_components(self.root, path))?.is_some() {
                self.lines.push(format!("kept {path}"));
            } else {
                let content = self.manifest.content(path);
                self.write(path, &content)?;
            }
        }
        Ok(())
    }

    /// Writes each data file of the workflow that is absent, with a line, and
    /// says nothing about one that exists. The adoption of an install runs it.
    fn absent_data_files(&mut self) -> Result<(), String> {
        for &path in DATA_FILES {
            if !self.owns(path) && read_if_present(&join_components(self.root, path))?.is_none() {
                let content = self.manifest.content(path);
                self.write(path, &content)?;
            }
        }
        Ok(())
    }

    /// Ends the run: adds the roles that `models` lacks, moves the version of
    /// the pin where `update` moved the files, writes the pin to `marker`,
    /// and returns what the run printed. The project's value and its unknown
    /// roles stay.
    fn finish(mut self, marker: &mut Marker) -> Outcome {
        for (role, model) in &self.manifest.models {
            self.pin
                .models
                .entry(role.clone())
                .or_insert_with(|| model.clone());
        }
        if self.run != Run::Install {
            self.pin.version = self.manifest.version.clone();
        }
        pins::write(marker, &self.pin);
        if self.run != Run::Sync {
            let mut baselines = marker.baselines();
            if let Some(line) = drop_baselines_of(&self.pin, &mut baselines) {
                marker.set_baselines(baselines);
                self.lines.push(line);
            }
        }
        Outcome {
            lines: self.lines,
            hand_steps: self.hand_steps,
        }
    }
}

/// Removes from `baselines` each path that `pin` pins, a pin of `null`
/// included: a pinned path has no baseline (the module doc has the rule).
/// Returns the line that reports the removal, or `None` when no baseline went.
fn drop_baselines_of(pin: &WorkflowPin, baselines: &mut Map<String, Value>) -> Option<String> {
    let before = baselines.len();
    baselines.retain(|path, _| !pin.files.contains_key(path));
    match before - baselines.len() {
        0 => None,
        1 => Some(
            "removed 1 baseline from .houserules.json; the pin of the workflow replaces it"
                .to_string(),
        ),
        count => Some(format!(
            "removed {count} baselines from .houserules.json; the pins of the workflow replace them"
        )),
    }
}

/// Removes from `baselines` each path that the `workflow` key of `marker` pins.
/// An install with no `workflow` key keeps `baselines` as it is. Returns the
/// line that reports the removal, or `None` when no baseline went.
pub(crate) fn forget_pinned_baselines(
    marker: &Marker,
    baselines: &mut Map<String, Value>,
) -> Result<Option<String>, String> {
    Ok(pins::read(marker)?.and_then(|pin| drop_baselines_of(&pin, baselines)))
}

/// Installs the workflow `manifest` for `init`: writes each file of the
/// package that is absent, keeps each that exists, pins the files that the kit
/// wrote or owns, and writes the empty data file. A file at a workflow path
/// that is not the kit's stays the project's, with its line once. The pins go
/// into `marker`, whose caller writes it last. Returns the lines `init`
/// prints, the last one `workflow: <name> <version>`.
pub(crate) fn install(
    root: &Path,
    marker: &mut Marker,
    manifest: &Manifest,
) -> Result<Vec<String>, String> {
    let mut reconcile = Reconcile::start(Run::Install, root, marker, manifest)?;
    for path in &manifest.files {
        reconcile.file(path)?;
    }
    reconcile.data_files()?;
    let version = reconcile.pin.version.clone();
    let mut lines = reconcile.finish(marker).lines;
    lines.push(format!("workflow: {} {version}", manifest.name));
    Ok(lines)
}

/// Adopts the workflow `manifest` into an install that has the files of the kit
/// and no `workflow` key (spec 4.7, step 2). Each file that the kit wrote is
/// pinned at its recorded baseline, else at the known content it equals, and
/// `update` brings it to the content of the package at once, as `sync` does an
/// installed workflow: a file at its pin is written, a file that the project
/// changed is kept with its line. An absent file is written and pinned, and so
/// is the absent data file. A file at a workflow path that is not the kit's
/// stays the project's: it gets no pin but `null` and its hand step. A file
/// that the project changed gets its kept line from the `sync` that follows.
/// The pins go into `marker`, whose caller writes it. Returns the lines to
/// print and the hand steps.
pub(crate) fn adopt(
    root: &Path,
    marker: &mut Marker,
    manifest: &Manifest,
) -> Result<Outcome, String> {
    let mut reconcile = Reconcile::start(Run::Adopt, root, marker, manifest)?;
    for path in &manifest.files {
        reconcile.file(path)?;
    }
    reconcile.absent_data_files()?;
    Ok(reconcile.finish(marker))
}

/// Moves the pins of the installed workflow with the binary for `update`
/// (the module doc has the outcomes). An install with no workflow gets
/// nothing. The pins go into `marker`, whose caller writes it. Returns the
/// lines `update` prints.
pub(crate) fn sync(
    root: &Path,
    marker: &mut Marker,
    manifest: &Manifest,
) -> Result<Vec<String>, String> {
    if pins::read(marker)?.is_none() {
        return Ok(Vec::new());
    }
    let mut reconcile = Reconcile::start(Run::Sync, root, marker, manifest)?;
    for path in &manifest.files {
        reconcile.file(path)?;
    }
    Ok(reconcile.finish(marker).lines)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use serde_json::json;

    use super::*;

    /// A scratch install root with a fresh marker.
    fn fresh() -> (tempfile::TempDir, Marker) {
        let dir = tempfile::tempdir().expect("tempdir");
        let marker = Marker::read_or_default(dir.path(), "WI").expect("a default marker");
        (dir, marker)
    }

    /// The first file of the manifest, its path, and its content.
    fn first_file(manifest: &Manifest) -> (String, Vec<u8>) {
        let path = manifest.files[0].clone();
        let content = manifest.content(&path);
        (path, content)
    }

    /// `init` writes every file and the data file, pins each file, and ends
    /// with the summary line.
    #[test]
    fn install_writes_pins_and_summarizes() {
        let (dir, mut marker) = fresh();
        let manifest = reference();
        let lines = install(dir.path(), &mut marker, &manifest).expect("install");
        let mut expected: Vec<String> = manifest
            .files
            .iter()
            .map(|path| format!("wrote {path}"))
            .collect();
        expected.push("wrote .claude/evals/record.json".to_string());
        expected.push(format!("workflow: reference {}", manifest.version));
        assert_eq!(lines, expected);
        let pin = pins::read(&marker).expect("pins").expect("a pin");
        assert_eq!(pin.files.len(), manifest.files.len());
        assert_eq!(pin.models, manifest.models);
    }

    /// `init` writes no file that exists: a file of the kit at its content is
    /// kept and pinned, and a project file is reported and left unpinned.
    #[test]
    fn install_keeps_a_file_that_exists() {
        let (dir, mut marker) = fresh();
        let manifest = reference();
        let (own, _) = first_file(&manifest);
        let second = manifest.files[1].clone();
        write_into(dir.path(), &own, b"The project's own file.\n").expect("write");
        write_into(dir.path(), &second, &manifest.content(&second)).expect("write");
        let lines = install(dir.path(), &mut marker, &manifest).expect("install");
        assert!(lines.contains(&not_the_kits(&manifest, &own)), "{lines:?}");
        assert!(lines.contains(&format!("kept {second}")), "{lines:?}");
        let pin = pins::read(&marker).expect("pins").expect("a pin");
        assert_eq!(pin.files[&own], None, "the project's file is pinned null");
        assert_eq!(
            pin.files[&second],
            Some(baseline::hash(&manifest.content(&second)))
        );
        assert_eq!(
            fs::read(dir.path().join(&own)).expect("read"),
            b"The project's own file.\n"
        );
    }

    /// `sync` writes a file at its pin with the new content, keeps a changed
    /// file with the kept line, restores a deleted file, and leaves a project
    /// file alone and silent.
    #[test]
    fn sync_moves_what_the_project_did_not_change() {
        let (dir, mut marker) = fresh();
        let manifest = reference();
        install(dir.path(), &mut marker, &manifest).expect("install");
        let (changed, _) = first_file(&manifest);
        let deleted = manifest.files[1].clone();
        let stale = manifest.files[2].clone();
        write_into(dir.path(), &changed, b"Changed by the project.\n").expect("write");
        fs::remove_file(dir.path().join(&deleted)).expect("delete");
        write_into(dir.path(), &stale, b"An older kit content.\n").expect("write");
        let mut pin = pins::read(&marker).expect("pins").expect("a pin");
        pin.files.insert(
            stale.clone(),
            Some(baseline::hash(b"An older kit content.\n")),
        );
        pins::write(&mut marker, &pin);

        let lines = sync(dir.path(), &mut marker, &manifest).expect("sync");
        assert_eq!(
            lines,
            [
                format!(
                    "kept {changed} (locally modified; reference workflow {})",
                    manifest.version
                ),
                format!("wrote {deleted}"),
                format!("wrote {stale}"),
            ]
        );
        assert_eq!(
            fs::read(dir.path().join(&changed)).expect("read"),
            b"Changed by the project.\n"
        );
        assert_eq!(
            fs::read(dir.path().join(&stale)).expect("read"),
            manifest.content(&stale)
        );
        let again = sync(dir.path(), &mut marker, &manifest).expect("sync");
        assert_eq!(
            again,
            [format!(
                "kept {changed} (locally modified; reference workflow {})",
                manifest.version
            )]
        );
    }

    /// A file that the project merged to equal the new content gets its pin
    /// moved, with no line.
    #[test]
    fn sync_moves_the_pin_of_a_file_merged_by_hand() {
        let (dir, mut marker) = fresh();
        let manifest = reference();
        install(dir.path(), &mut marker, &manifest).expect("install");
        let (path, _) = first_file(&manifest);
        let mut pin = pins::read(&marker).expect("pins").expect("a pin");
        pin.files
            .insert(path.clone(), Some(baseline::hash(b"older")));
        pins::write(&mut marker, &pin);
        let lines = sync(dir.path(), &mut marker, &manifest).expect("sync");
        assert_eq!(lines, Vec::<String>::new());
        let moved = pins::read(&marker).expect("pins").expect("a pin");
        assert_eq!(
            moved.files[&path],
            Some(baseline::hash(&manifest.content(&path)))
        );
    }

    /// A path in `ownedPaths` stays as found: no write, no line, no pin move.
    #[test]
    fn sync_leaves_an_owned_path_as_found() {
        let (dir, mut marker) = fresh();
        let manifest = reference();
        install(dir.path(), &mut marker, &manifest).expect("install");
        let (path, _) = first_file(&manifest);
        marker.set_owned_paths(std::slice::from_ref(&path));
        write_into(dir.path(), &path, b"Owned.\n").expect("write");
        let lines = sync(dir.path(), &mut marker, &manifest).expect("sync");
        assert_eq!(lines, Vec::<String>::new());
        assert_eq!(fs::read(dir.path().join(&path)).expect("read"), b"Owned.\n");
    }

    /// A file that `install` left to the project has the pin `null`: `sync`
    /// neither writes it nor prints a line.
    #[test]
    fn sync_prints_nothing_for_a_project_file_pinned_null() {
        let (dir, mut marker) = fresh();
        let manifest = reference();
        let (own, _) = first_file(&manifest);
        write_into(dir.path(), &own, b"Ours.\n").expect("write");
        install(dir.path(), &mut marker, &manifest).expect("install");
        let lines = sync(dir.path(), &mut marker, &manifest).expect("sync");
        assert_eq!(lines, Vec::<String>::new());
        assert_eq!(fs::read(dir.path().join(&own)).expect("read"), b"Ours.\n");
    }

    /// A new binary whose manifest adds a path that the project already holds
    /// (spec 4.3, C33): the first `sync` prints the hand step once and records
    /// `null`, the second prints nothing, and the file stays as found.
    #[test]
    fn sync_reports_a_path_the_manifest_adds_once_and_records_null() {
        let (dir, mut marker) = fresh();
        let manifest = reference();
        let (added, _) = first_file(&manifest);
        let mut older = manifest.clone();
        older.files.retain(|path| *path != added);
        install(dir.path(), &mut marker, &older).expect("install");
        let pinned = pins::read(&marker).expect("pins").expect("a pin");
        assert!(!pinned.files.contains_key(&added), "the older manifest");
        write_into(dir.path(), &added, b"Ours.\n").expect("write");

        let first = sync(dir.path(), &mut marker, &manifest).expect("sync");
        assert_eq!(first, [not_the_kits(&manifest, &added)]);
        let after_first = pins::read(&marker).expect("pins").expect("a pin");
        assert_eq!(after_first.files[&added], None);
        let second = sync(dir.path(), &mut marker, &manifest).expect("sync");
        assert_eq!(second, Vec::<String>::new());
        assert_eq!(
            pins::read(&marker).expect("pins").expect("a pin"),
            after_first
        );
        assert_eq!(fs::read(dir.path().join(&added)).expect("read"), b"Ours.\n");
    }

    /// A path that `files` maps to `null` gets no line while it holds a file
    /// of the project. Once the file is gone the kit writes it back and pins
    /// it, and once the file equals the kit's content the pin moves silently.
    #[test]
    fn a_null_path_is_silent_while_the_project_holds_it_and_ordinary_after() {
        let (dir, mut marker) = fresh();
        let manifest = reference();
        let (own, content) = first_file(&manifest);
        write_into(dir.path(), &own, b"Ours.\n").expect("write");
        install(dir.path(), &mut marker, &manifest).expect("install");
        for _ in 0..2 {
            let lines = sync(dir.path(), &mut marker, &manifest).expect("sync");
            assert_eq!(lines, Vec::<String>::new());
        }
        assert_eq!(fs::read(dir.path().join(&own)).expect("read"), b"Ours.\n");

        write_into(dir.path(), &own, &content).expect("write the kit's content");
        let lines = sync(dir.path(), &mut marker, &manifest).expect("sync");
        assert_eq!(lines, Vec::<String>::new());
        let pin = pins::read(&marker).expect("pins").expect("a pin");
        assert_eq!(pin.files[&own], Some(baseline::hash(&content)));

        let (dir, mut marker) = fresh();
        write_into(dir.path(), &own, b"Ours.\n").expect("write");
        install(dir.path(), &mut marker, &manifest).expect("install");
        fs::remove_file(dir.path().join(&own)).expect("delete");
        let lines = sync(dir.path(), &mut marker, &manifest).expect("sync");
        assert_eq!(lines, [format!("wrote {own}")]);
        let pin = pins::read(&marker).expect("pins").expect("a pin");
        assert_eq!(pin.files[&own], Some(baseline::hash(&content)));
    }

    /// The models of the project stay; a role the manifest names and the pin
    /// lacks is added; an unknown role stays.
    #[test]
    fn sync_adds_missing_roles_and_keeps_the_rest() {
        let (dir, mut marker) = fresh();
        let manifest = reference();
        install(dir.path(), &mut marker, &manifest).expect("install");
        let mut pin = pins::read(&marker).expect("pins").expect("a pin");
        pin.models.clear();
        pin.models
            .insert("implementer".to_string(), "haiku".to_string());
        pin.models
            .insert("designer".to_string(), "mine".to_string());
        pins::write(&mut marker, &pin);
        sync(dir.path(), &mut marker, &manifest).expect("sync");
        let moved = pins::read(&marker).expect("pins").expect("a pin");
        assert_eq!(moved.models["implementer"], "haiku");
        assert_eq!(moved.models["designer"], "mine");
        assert_eq!(moved.models["task-reviewer"], "opus");
        assert_eq!(moved.models.len(), 5);
    }

    /// An install with no workflow gets nothing from `sync`, and no key.
    #[test]
    fn sync_does_nothing_without_the_workflow() {
        let (dir, mut marker) = fresh();
        let lines = sync(dir.path(), &mut marker, &reference()).expect("sync");
        assert_eq!(lines, Vec::<String>::new());
        assert!(marker.workflow_value().is_none());
        assert_eq!(fs::read_dir(dir.path()).expect("read").count(), 0);
    }

    /// An install that runs another workflow is one named line.
    #[test]
    fn a_pin_of_another_workflow_is_refused() {
        let (dir, mut marker) = fresh();
        marker.set_workflow_value(
            json!({"name": "other", "version": "1.0.0", "files": {}, "models": {}}),
        );
        let error = sync(dir.path(), &mut marker, &reference()).expect_err("another workflow");
        assert!(
            error.ends_with(
                "the install runs the workflow \"other\", and this houserules carries \"reference\" only"
            ),
            "{error}"
        );
    }

    /// The script that the hook entry runs is a file of the manifest, and the
    /// entry runs it from the project root.
    #[test]
    fn the_hook_entry_runs_a_script_of_the_manifest() {
        assert!(reference().files.iter().any(|f| f == SESSION_START_SCRIPT));
        assert_eq!(
            session_start_entry(),
            json!({"hooks": [{
                "type": "command",
                "command": "${CLAUDE_PROJECT_DIR}/tools/reference-session-start.sh",
            }]})
        );
    }

    /// A file is the kit's when the baselines hold its path, its bytes equal
    /// the package content, or its bytes equal a content that the kit shipped
    /// before; an absent file is not.
    #[test]
    fn a_file_is_the_kits_by_baseline_or_by_content() {
        let (dir, mut marker) = fresh();
        let manifest = reference();
        let path = manifest.files[0].clone();
        assert!(!is_kit_file(dir.path(), &manifest, &path, &marker).expect("absent"));
        write_into(dir.path(), &path, b"Ours.\n").expect("write");
        assert!(!is_kit_file(dir.path(), &manifest, &path, &marker).expect("own"));
        let mut baselines = marker.baselines();
        baselines.insert(path.clone(), json!("00"));
        marker.set_baselines(baselines);
        assert!(is_kit_file(dir.path(), &manifest, &path, &marker).expect("baseline"));
        let (dir, marker) = fresh();
        write_into(dir.path(), &path, &manifest.content(&path)).expect("write");
        assert!(is_kit_file(dir.path(), &manifest, &path, &marker).expect("content"));
        let (dir, marker) = fresh();
        let known = known_contents(&path, "WI").expect("known contents");
        assert!(!known.is_empty(), "{path} has released contents");
        write_into(dir.path(), &path, &known[0]).expect("write");
        assert!(is_kit_file(dir.path(), &manifest, &path, &marker).expect("known content"));
    }

    /// The path of a file of the manifest that an older release shipped with
    /// a content other than the package's, and that older content.
    fn path_with_an_older_content(manifest: &Manifest) -> (String, Vec<u8>) {
        manifest
            .files
            .iter()
            .find_map(|path| {
                let older = known_contents(path, "WI")
                    .expect("known contents")
                    .into_iter()
                    .find(|content| *content != manifest.content(path))?;
                Some((path.clone(), older))
            })
            .expect("a file of the manifest changed since a release")
    }

    /// Adoption (spec 4.7, step 2): a file at its recorded baseline is pinned
    /// there and gets the new content; a file that equals a known content is
    /// pinned at it; an absent file is written and pinned; a file that is not
    /// the kit's gets the `null` pin and its hand step; an owned path is left
    /// alone. The version of the pin is the one of the manifest.
    #[test]
    fn adopt_pins_by_baseline_and_by_known_content_and_writes_what_is_absent() {
        let (dir, mut marker) = fresh();
        let manifest = reference();
        let (by_content, older) = path_with_an_older_content(&manifest);
        let others: Vec<&String> = manifest
            .files
            .iter()
            .filter(|path| **path != by_content)
            .collect();
        let (by_baseline, owned, own) = (others[0].clone(), others[1].clone(), others[2].clone());
        write_into(dir.path(), &by_content, &older).expect("write");
        write_into(dir.path(), &by_baseline, b"An older kit file.\n").expect("write");
        write_into(dir.path(), &owned, b"Owned.\n").expect("write");
        write_into(dir.path(), &own, b"Ours.\n").expect("write");
        let mut baselines = marker.baselines();
        baselines.insert(
            by_baseline.clone(),
            json!(baseline::hash(b"An older kit file.\n")),
        );
        marker.set_baselines(baselines);
        marker.set_owned_paths(std::slice::from_ref(&owned));

        let outcome = adopt(dir.path(), &mut marker, &manifest).expect("adopt");

        let pin = pins::read(&marker).expect("pins").expect("a pin");
        assert_eq!(pin.version, manifest.version);
        for path in [&by_content, &by_baseline] {
            let content = manifest.content(path);
            assert_eq!(fs::read(dir.path().join(path)).expect("read"), content);
            assert_eq!(pin.files[path], Some(baseline::hash(&content)), "{path}");
            assert!(outcome.lines.contains(&format!("wrote {path}")), "{path}");
        }
        assert_eq!(pin.files[&own], None);
        assert_eq!(
            outcome.hand_steps,
            [hand_steps::not_the_kits("reference", &own)]
        );
        assert!(!pin.files.contains_key(&owned));
        assert_eq!(
            fs::read(dir.path().join(&owned)).expect("read"),
            b"Owned.\n"
        );
        let absent = manifest
            .files
            .iter()
            .find(|path| ![&by_content, &by_baseline, &owned, &own].contains(path))
            .expect("an absent file");
        assert_eq!(
            fs::read(dir.path().join(absent)).expect("read"),
            manifest.content(absent)
        );
        assert!(outcome.lines.contains(&format!("wrote {absent}")));
        assert!(
            dir.path().join(DATA_FILES[0]).is_file(),
            "the data file is written"
        );
    }

    /// Adoption keeps a file that the project changed at its baseline pin and
    /// prints no kept line: the `sync` of the same run prints it.
    #[test]
    fn adopt_keeps_a_changed_file_at_its_baseline_and_prints_no_kept_line() {
        let (dir, mut marker) = fresh();
        let manifest = reference();
        let (path, _) = first_file(&manifest);
        write_into(dir.path(), &path, b"Changed.\n").expect("write");
        let mut baselines = marker.baselines();
        baselines.insert(path.clone(), json!(baseline::hash(b"The release.\n")));
        marker.set_baselines(baselines);

        let outcome = adopt(dir.path(), &mut marker, &manifest).expect("adopt");

        assert_eq!(
            fs::read(dir.path().join(&path)).expect("read"),
            b"Changed.\n"
        );
        let pin = pins::read(&marker).expect("pins").expect("a pin");
        assert_eq!(pin.files[&path], Some(baseline::hash(b"The release.\n")));
        assert!(
            !outcome.lines.iter().any(|line| line.starts_with("kept ")),
            "{:?}",
            outcome.lines
        );
        let lines = sync(dir.path(), &mut marker, &manifest).expect("sync");
        assert_eq!(
            lines,
            [format!(
                "kept {path} (locally modified; reference workflow {})",
                manifest.version
            )]
        );
    }
}
