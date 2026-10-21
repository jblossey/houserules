//! `houserules pack`: list, add, remove, and upgrade the knowledge packs of
//! an install (spec 4.2).
//!
//! A pack is one JSON file. `pack add` copies it into
//! `knowledge/packs/<name>.json` in emit form and pins it in
//! `.houserules.json` (`install::marker`); every other command reads the copies.
//! `source` holds the three source forms and the git fetch; `areas` holds
//! the area rules of spec section 6; `one_source` holds the one-id-one-source
//! step (L8, L9) that every add and upgrade runs; `opt_in` holds the offer of
//! the entries that a pack lists under `optIn`, which `add` and `upgrade`
//! make and `remove` withdraws (spec 3.7).
//!
//! Each command checks every refusal before it writes a file. A refusal is
//! one stderr line and exit 2, and it changes nothing. `add`, `remove`, and
//! `upgrade` end with the render that `init` and `update` run, so the
//! generated files never lag the knowledge base; `list` writes nothing.

pub(crate) mod areas;
pub(crate) mod one_source;
pub(crate) mod opt_in;
pub(crate) mod source;

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use serde_json::{Value, json};

use crate::archive::write_json;
use crate::emit::emit;
use crate::install::marker::{Marker, Pin};
use crate::packs::{PackCopy, kit_packs, pack_sha256, parse_pack};
use crate::rules::{Base, load_base, render_and_report};

use areas::{add_pack_areas, remove_pack_areas};
use source::{PackSource, fetch_git, read_pack_file};

/// The pack that needs a tracker (design.md 5.149).
pub(crate) const TRACKER_PACK: &str = "process";

/// What every command reads: the merged base and the pins.
struct Install {
    base: Base,
    marker: Marker,
    pins: BTreeMap<String, Pin>,
}

impl Install {
    /// Loads the knowledge base and the pins under `root`. A failure is one
    /// named line.
    fn open(root: &Path) -> Result<Self, String> {
        let base = load_base(root).map_err(|error| error.to_string())?;
        let marker = Marker::read(root)?;
        let pins = marker.pins()?;
        Ok(Self { base, marker, pins })
    }

    /// The copy of the pack `name`, when the install holds one.
    fn copy(&self, name: &str) -> Option<&PackCopy> {
        self.base.packs.iter().find(|pack| pack.name == name)
    }

    /// `true` when the install holds a copy or a pin of the pack `name`.
    fn is_installed(&self, name: &str) -> bool {
        self.copy(name).is_some() || self.pins.contains_key(name)
    }

    /// Every installed copy but the one named `name`.
    fn other_packs(&self, name: &str) -> Vec<PackCopy> {
        self.base
            .packs
            .iter()
            .filter(|pack| pack.name != name)
            .cloned()
            .collect()
    }
}

/// The path of a pack's copy: `knowledge/packs/<name>.json`, built one
/// component at a time.
fn copy_path(root: &Path, name: &str) -> PathBuf {
    root.join("knowledge")
        .join("packs")
        .join(format!("{name}.json"))
}

/// Plans the one-id-one-source step of `pack add` and `pack upgrade` for the
/// installed `packs`, `copy` among them. The step decides on the project's own
/// overrides. The offer of `copy` writes its exclusions before the copy and
/// the step writes `knowledge/overrides.json` again at the end, so the plan
/// takes the exclusions on top of its own edits: its write keeps them, also
/// for an id that a local entry carries.
fn plan_the_step(
    root: &Path,
    install: &Install,
    packs: &[PackCopy],
    copy: &PackCopy,
) -> Result<one_source::Plan, String> {
    let overrides = &install.base.overrides_raw;
    let mut step = one_source::plan(root, &install.base.topic_files, packs, overrides)?;
    step.edit_overrides(root, overrides, |document| {
        opt_in::with_offered_exclusions(document, &install.marker, copy)
    });
    Ok(step)
}

/// Resolves the install root, runs `action`, prints the text it returns on
/// stdout, and runs `after`. A failure of `action` or `after` is its one
/// line on stderr with exit 2.
fn run(
    dir: Option<PathBuf>,
    action: impl FnOnce(&Path) -> Result<String, String>,
    after: impl FnOnce(&Path) -> Result<(), String>,
) -> ExitCode {
    let root = match crate::root::resolve_root(dir) {
        Ok(root) => root,
        Err(code) => return code,
    };
    let outcome = action(&root).and_then(|text| {
        print!("{text}");
        after(&root)
    });
    match outcome {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("{message}");
            ExitCode::from(2)
        }
    }
}

/// The step after a command that reads only: none.
fn nothing_after(_root: &Path) -> Result<(), String> {
    Ok(())
}

/// The step after a command that changes files: the render that `init` and
/// `update` run, so the generated files never lag the knowledge base. A
/// failure says that the files are written already.
fn render_after_a_change(root: &Path) -> Result<(), String> {
    render_and_report(root).map_err(|error| {
        format!(
            "render failed after the pack command wrote its files: {error}; fix the cause, then run houserules render"
        )
    })
}

/// The lines of a receipt as the text that a command prints: one line each.
fn as_text(lines: Vec<String>) -> String {
    lines.into_iter().map(|line| format!("{line}\n")).collect()
}

/// Runs `pack list`: prints the installed packs and the kit packs the
/// install lacks, as JSON.
pub(crate) fn cmd_pack_list(dir: Option<PathBuf>) -> ExitCode {
    run(
        dir,
        |root| list(root).map(|listing| emit(&listing)),
        nothing_after,
    )
}

/// Runs `pack add <source>`, then renders.
pub(crate) fn cmd_pack_add(dir: Option<PathBuf>, source: String) -> ExitCode {
    run(
        dir,
        |root| add(root, &source).map(as_text),
        render_after_a_change,
    )
}

/// Runs `pack remove <name>`, then renders.
pub(crate) fn cmd_pack_remove(dir: Option<PathBuf>, name: String) -> ExitCode {
    run(
        dir,
        |root| remove(root, &name).map(as_text),
        render_after_a_change,
    )
}

/// Runs `pack upgrade <name> [<tag>]`, then renders.
pub(crate) fn cmd_pack_upgrade(
    dir: Option<PathBuf>,
    name: String,
    tag: Option<String>,
) -> ExitCode {
    run(
        dir,
        |root| upgrade(root, &name, tag.as_deref()).map(as_text),
        render_after_a_change,
    )
}

/// The ids of `copy`, sorted.
fn ids_of(copy: &PackCopy) -> BTreeSet<&str> {
    copy.entries
        .iter()
        .filter_map(|entry| entry.get("id").and_then(Value::as_str))
        .collect()
}

/// The ids that `overrides.json` names under `key` (`overrides` or
/// `exclusions`).
fn named_under<'a>(overrides: &'a Value, key: &str) -> impl Iterator<Item = &'a str> {
    overrides
        .get(key)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|item| item.get("id").and_then(Value::as_str))
}

/// The ids of `ids` that `overrides.json` names in an override or an
/// exclusion, sorted.
fn named_in_overrides<'a>(overrides: &Value, ids: &BTreeSet<&'a str>) -> Vec<&'a str> {
    let named: BTreeSet<&str> = named_under(overrides, "overrides")
        .chain(named_under(overrides, "exclusions"))
        .collect();
    ids.iter()
        .copied()
        .filter(|id| named.contains(id))
        .collect()
}

/// What `pack list` prints in place of a count when a pin has no copy to
/// count.
const NO_COPY: &str = "no copy";

/// Builds the listing of `pack list`. Each name that `is_installed` reports
/// is one row of `installed`, in name order: a copy, with the version and
/// source of its pin (`unpinned` when it has none), or a pin with no copy,
/// whose counts show `no copy`.
fn list(root: &Path) -> Result<Value, String> {
    let install = Install::open(root)?;
    let overrides = &install.base.overrides_raw;
    let mut rows: Vec<(&str, Value)> = install
        .base
        .packs
        .iter()
        .map(|pack| {
            let ids = ids_of(pack);
            let pin = install.pins.get(&pack.name);
            let row = json!({
                "name": pack.name,
                "version": pin.map_or(pack.version.as_str(), |pin| pin.version.as_str()),
                "source": pin.map_or("unpinned", |pin| pin.source.as_str()),
                "entries": pack.entries.len(),
                "overrides": named_under(overrides, "overrides").filter(|id| ids.contains(id)).count(),
                "exclusions": named_under(overrides, "exclusions").filter(|id| ids.contains(id)).count(),
            });
            (pack.name.as_str(), row)
        })
        .collect();
    rows.extend(
        install
            .pins
            .iter()
            .filter(|(name, _)| install.copy(name).is_none())
            .map(|(name, pin)| {
                let row = json!({
                    "name": name,
                    "version": pin.version,
                    "source": pin.source,
                    "entries": NO_COPY,
                    "overrides": NO_COPY,
                    "exclusions": NO_COPY,
                });
                (name.as_str(), row)
            }),
    );
    rows.sort_by(|left, right| left.0.cmp(right.0));
    let installed: Vec<Value> = rows.into_iter().map(|(_, row)| row).collect();
    let available: Vec<Value> = kit_packs()
        .into_iter()
        .filter(|kit| !install.is_installed(&kit.name))
        .map(|kit| json!({"name": kit.name, "version": kit.version}))
        .collect();
    Ok(json!({"installed": installed, "available": available}))
}

/// A pack file read from its source.
struct Fetched {
    raw: Value,
    /// The commit a git source's tag resolved to.
    commit: Option<String>,
}

/// Reads the pack file that `source` names. A kit source names the pack
/// `name` of the binary. A file source is read under `root`. `origin`
/// names the file in a read failure.
fn fetch(root: &Path, source: &PackSource, name: &str, origin: &str) -> Result<Fetched, String> {
    match source {
        PackSource::Kit => kit_packs()
            .into_iter()
            .find(|pack| pack.name == name)
            .map(|pack| Fetched {
                raw: pack.raw,
                commit: None,
            })
            .ok_or_else(|| format!("pack {name} is not a kit pack")),
        PackSource::File(relative) => {
            let path = relative
                .split('/')
                .fold(root.to_path_buf(), |path, part| path.join(part));
            read_pack_file(&path, origin).map(|raw| Fetched { raw, commit: None })
        }
        PackSource::Git { url, tag, path } => fetch_git(url, tag, path).map(|fetched| Fetched {
            raw: fetched.raw,
            commit: Some(fetched.commit),
        }),
    }
}

/// The path of the file `argument` relative to `root`, with `/` between
/// components. A file outside the install root is refused: a recorded
/// source path is relative to the root.
fn relative_to_root(root: &Path, argument: &str) -> Result<String, String> {
    let file = fs::canonicalize(argument).map_err(|error| format!("{argument}: {error}"))?;
    let base = fs::canonicalize(root).map_err(|error| format!("{}: {error}", root.display()))?;
    let relative = file.strip_prefix(&base).map_err(|_| {
        format!(
            "{argument}: the file is outside the install root {}; a file source must lie inside it",
            root.display()
        )
    })?;
    let parts: Vec<String> = relative
        .components()
        .map(|part| part.as_os_str().to_string_lossy().into_owned())
        .collect();
    Ok(parts.join("/"))
}

/// Reads the source that the argument of `pack add` names: a `git+` source,
/// else the kit pack of that name, else a pack file.
fn read_argument(root: &Path, argument: &str) -> Result<(PackSource, Fetched), String> {
    let source = if argument.starts_with("git+") {
        PackSource::parse(argument)?
    } else if kit_packs().iter().any(|pack| pack.name == argument) {
        PackSource::Kit
    } else if Path::new(argument).is_file() {
        PackSource::File(relative_to_root(root, argument)?)
    } else {
        let carried: Vec<String> = kit_packs().into_iter().map(|pack| pack.name).collect();
        return Err(format!(
            "{argument}: not a pack that houserules carries ({}) and not a file",
            carried.join(", ")
        ));
    };
    let fetched = fetch(root, &source, argument, argument)?;
    Ok((source, fetched))
}

/// Refuses a pack that ships an id which another installed pack ships
/// (5.110): one line names each id and that pack. `verb` is `add` or
/// `upgrade`.
fn refuse_shared_ids(verb: &str, copy: &PackCopy, others: &[PackCopy]) -> Result<(), String> {
    let mut by_name: Vec<&PackCopy> = others.iter().collect();
    by_name.sort_by(|left, right| left.name.cmp(&right.name));
    let shared: Vec<String> = ids_of(copy)
        .into_iter()
        .filter_map(|id| {
            by_name
                .iter()
                .find(|other| ids_of(other).contains(id))
                .map(|other| format!("{id} is also in pack {}", other.name))
        })
        .collect();
    if shared.is_empty() {
        return Ok(());
    }
    Err(format!(
        "cannot {verb} pack {}: {}",
        copy.name,
        shared.join(", ")
    ))
}

/// `pack add`: installs the pack that `argument` names. `init --workflow`
/// calls it for each pack that the workflow requires, so that every path that
/// brings a pack onto an install makes the same offer of its opt-in entries.
pub(crate) fn add(root: &Path, argument: &str) -> Result<Vec<String>, String> {
    let mut install = Install::open(root)?;
    let (source, fetched) = read_argument(root, argument)?;
    let copy = parse_pack(fetched.raw, argument)?;
    if install.is_installed(&copy.name) {
        return Err(format!(
            "pack {} is already installed; pack upgrade moves an upstream pack and update moves a kit pack",
            copy.name
        ));
    }
    if copy.name == TRACKER_PACK && install.marker.tracker()?.is_none() {
        return Err(crate::tracker::lines::pack_needs_tracker(TRACKER_PACK));
    }
    refuse_shared_ids("add", &copy, &install.base.packs)?;
    let mut packs = install.base.packs.clone();
    packs.push(copy.clone());
    packs.sort_by(|left, right| left.name.cmp(&right.name));
    let step = plan_the_step(root, &install, &packs, &copy)?;

    let active = opt_in::active_lines(&install.marker, &copy);
    let mut added_areas = Vec::new();
    let area_lines = add_pack_areas(root, &copy, &mut added_areas)?;
    let offered = opt_in::offer(root, &mut install.marker, &copy)?;
    write_json(&copy_path(root, &copy.name), &copy.raw)?;
    install.pins.insert(
        copy.name.clone(),
        Pin {
            version: copy.version.clone(),
            source: source.to_string(),
            sha256: pack_sha256(&copy.raw),
            added_areas,
            commit: fetched.commit,
        },
    );
    install.marker.set_pins(&install.pins);
    install.marker.write()?;
    step.apply()?;

    let mut lines = vec![format!(
        "added pack {} {} from {source}",
        copy.name, copy.version
    )];
    lines.extend(area_lines);
    lines.extend(step.lines().iter().cloned());
    lines.extend(active);
    lines.extend(offered);
    Ok(lines)
}

/// `pack remove`: deletes the copy and the pin of the pack `name`.
fn remove(root: &Path, name: &str) -> Result<Vec<String>, String> {
    let install = Install::open(root)?;
    let copy = install.copy(name);
    let version = match (install.pins.get(name), copy) {
        (None, None) => return Err(format!("pack {name} is not installed")),
        (Some(pin), _) => pin.version.as_str(),
        (None, Some(copy)) => copy.version.as_str(),
    };
    if let Some(copy) = copy {
        let offers = opt_in::untaken_offers(&install.base.overrides_raw, copy);
        let named: Vec<&str> = named_in_overrides(&install.base.overrides_raw, &ids_of(copy))
            .into_iter()
            .filter(|id| !offers.iter().any(|offer| offer == id))
            .collect();
        if !named.is_empty() {
            return Err(format!(
                "cannot remove pack {name}: knowledge/overrides.json names {}; remove those overrides and exclusions first",
                named.join(", ")
            ));
        }
    }
    let mut lines = vec![format!("removed pack {name} {version}")];
    if let Some(copy) = copy {
        opt_in::withdraw(root, &mut Marker::read(root)?, copy)?;
        let remaining = install.other_packs(name);
        lines.extend(remove_pack_areas(root, copy, &remaining, &install.base)?);
        let path = copy_path(root, name);
        fs::remove_file(&path).map_err(|error| format!("{}: {error}", path.display()))?;
    }
    let mut marker = Marker::read(root)?;
    let mut pins = marker.pins()?;
    pins.remove(name);
    marker.set_pins(&pins);
    marker.write()?;
    Ok(lines)
}

/// `pack upgrade`: moves the installed upstream pack `name` to `tag` of its
/// git source, or re-copies it from its file source.
fn upgrade(root: &Path, name: &str, tag: Option<&str>) -> Result<Vec<String>, String> {
    let install = Install::open(root)?;
    if !install.is_installed(name) {
        return Err(format!("pack {name} is not installed"));
    }
    let Some(pin) = install.pins.get(name).cloned() else {
        return Err(format!(
            "pack {name} has no pin in .houserules.json; remove it and add it again"
        ));
    };
    let Some(old) = install.copy(name).cloned() else {
        return Err(format!(
            "pack {name} has a pin and no copy in knowledge/packs; remove it and add it again"
        ));
    };
    let source = match (PackSource::parse(&pin.source)?, tag) {
        (PackSource::Kit, _) => {
            return Err(format!(
                "pack {name} is a kit pack; houserules update moves it"
            ));
        }
        (PackSource::Git { url, path, .. }, Some(tag)) => PackSource::Git {
            url,
            tag: tag.to_string(),
            path,
        },
        (PackSource::File(_), Some(_)) => {
            return Err(format!(
                "pack {name} comes from {}, which takes no tag; run houserules pack upgrade {name}",
                pin.source
            ));
        }
        (PackSource::Git { .. }, None) => {
            return Err(format!(
                "pack {name} comes from {}; run houserules pack upgrade {name} <tag>",
                pin.source
            ));
        }
        (file @ PackSource::File(_), None) => file,
    };
    let recorded = source.to_string();
    let fetched = fetch(root, &source, name, &recorded)?;
    let new = parse_pack(fetched.raw, &recorded)?;
    if new.name != name {
        return Err(format!(
            "pack {name}: the file at {recorded} names the pack \"{}\"; to change the source, run pack remove, then pack add",
            new.name
        ));
    }
    let others = install.other_packs(name);
    refuse_shared_ids("upgrade", &new, &others)?;
    let (old_ids, new_ids) = (ids_of(&old), ids_of(&new));
    let dropped: Vec<&str> = old_ids.difference(&new_ids).copied().collect();
    let added: Vec<&str> = new_ids.difference(&old_ids).copied().collect();
    let mut packs = others;
    packs.push(new.clone());
    packs.sort_by(|left, right| left.name.cmp(&right.name));
    let orphans = opt_in::orphaned_offers(&install.base.overrides_raw, &packs);
    let named: Vec<&str> = named_in_overrides(
        &install.base.overrides_raw,
        &dropped.iter().copied().collect(),
    )
    .into_iter()
    .filter(|id| !orphans.iter().any(|orphan| orphan == id))
    .collect();
    if !named.is_empty() {
        return Err(format!(
            "cannot upgrade pack {name}: knowledge/overrides.json names {}, which version {} drops; remove those overrides and exclusions first",
            named.join(", "),
            new.version
        ));
    }
    let step = plan_the_step(root, &install, &packs, &new)?;

    let mut record = pin.added_areas.clone();
    let area_additions = add_pack_areas(root, &new, &mut record)?;
    let Install {
        mut marker,
        mut pins,
        ..
    } = install;
    let offered = opt_in::offer(root, &mut marker, &new)?;
    write_json(&copy_path(root, name), &new.raw)?;
    pins.insert(
        name.to_string(),
        Pin {
            version: new.version.clone(),
            source: recorded,
            sha256: pack_sha256(&new.raw),
            added_areas: record,
            commit: fetched.commit,
        },
    );
    marker.set_pins(&pins);
    marker.write()?;
    step.apply()?;
    let upgraded = load_base(root).map_err(|error| error.to_string())?;
    let area_removals = remove_pack_areas(root, &old, &packs, &upgraded)?;
    opt_in::withdraw_orphans(root, &mut Marker::read(root)?, &packs)?;

    let mut lines = vec![format!(
        "upgraded pack {name} from {} to {}",
        pin.version, new.version
    )];
    lines.extend(added.iter().map(|id| format!("added id {id}")));
    lines.extend(dropped.iter().map(|id| format!("dropped id {id}")));
    lines.extend(area_removals);
    lines.extend(area_additions);
    lines.extend(step.lines().iter().cloned());
    lines.extend(offered);
    Ok(lines)
}
