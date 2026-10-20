//! `.houserules.json`: the install marker, and the one reader and writer of
//! the file.
//!
//! [`Marker`] holds the file whole, so a write keeps every key it does not
//! own, in its order. It validates the keys `init` and `update` read
//! (`idPrefix`, `version`, `overrides`, `ownedPaths`, `renderers`,
//! `baselines`, `schema`, `defaultBranch`) and hands out the `packs` key as
//! [`Pin`]s. A command names the file by its path. [`read_pins_for_gate`]
//! reads the same pins for `check-knowledge`, which names the file by its
//! relative name; [`Marker::read_or_default_for_gate`] reads the whole marker
//! for its kit-owned citation lint the same way.
//! [`read_default_branch_for_gate`] reads the `defaultBranch` for `audit`'s
//! durable-sha check, which words each fault in its own line. No other module
//! parses the file.
//!
//! A pin records where a pack's copy came from and what the copy held:
//! `version`, `source` (`kit`, `file:<path>`, or `git+<url>@<tag>#<path>`),
//! `sha256` of the copy's canonical form, `addedAreas` (the areas the pack
//! added to `knowledge/areas.json`), and, for a git source, the `commit`
//! its tag resolved to.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde_json::{Map, Value, json};

use crate::archive::{read_json, read_json_as, write_json};
use crate::root::INSTALL_MARKER;

/// The shared constraint text for both `idPrefix` rejection messages.
pub(super) const ID_PREFIX_HINT: &str = "must be 1-8 characters, A-Z then A-Z0-9";

/// `true` when `value` is a valid backlog id prefix: one uppercase ASCII
/// letter followed by up to seven more uppercase ASCII letters or digits.
/// A direct byte check rather than a `regress` pattern: every character
/// class here is a fixed ASCII range, so a regex engine would add
/// indirection a `char::is_ascii_uppercase`/`is_ascii_digit` pair already
/// expresses exactly.
pub(super) fn is_id_prefix(value: &str) -> bool {
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

/// One installed pack's pin.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Pin {
    /// The pack's own version.
    pub(crate) version: String,
    /// The recorded source string (`PackSource`'s `Display`).
    pub(crate) source: String,
    /// The SHA-256 of the copy's canonical form (`packs::pack_sha256`).
    pub(crate) sha256: String,
    /// The areas this pack added to `knowledge/areas.json`, in the order it
    /// added them.
    pub(crate) added_areas: Vec<String>,
    /// The commit a git source's tag resolved to; `None` for any other
    /// source.
    pub(crate) commit: Option<String>,
}

impl Pin {
    /// The pin as `.houserules.json` holds it: `version`, `source`,
    /// `sha256`, `addedAreas`, then `commit` when there is one.
    fn to_value(&self) -> Value {
        let mut map = Map::new();
        map.insert("version".to_string(), json!(self.version));
        map.insert("source".to_string(), json!(self.source));
        map.insert("sha256".to_string(), json!(self.sha256));
        map.insert("addedAreas".to_string(), json!(self.added_areas));
        if let Some(commit) = &self.commit {
            map.insert("commit".to_string(), json!(commit));
        }
        Value::Object(map)
    }

    /// Reads the pin of `name` from its JSON, or names the first field that
    /// is missing, has the wrong type, or is not a field of a pin. `file` is
    /// how the line names the marker.
    fn from_value(file: &str, name: &str, value: &Value) -> Result<Self, String> {
        let at = format!("{file}: packs.{name}");
        let Some(map) = value.as_object() else {
            return Err(format!("{at} must be an object"));
        };
        if let Some(key) = map.keys().find(|key| {
            !["version", "source", "sha256", "addedAreas", "commit"].contains(&key.as_str())
        }) {
            return Err(format!("{at}: unknown key {key:?}"));
        }
        let text = |key: &str| -> Result<String, String> {
            match map.get(key) {
                Some(Value::String(text)) => Ok(text.clone()),
                Some(_) => Err(format!("{at}.{key} must be a string")),
                None => Err(format!("{at}: missing \"{key}\"")),
            }
        };
        let added_areas = match map.get("addedAreas") {
            None => Vec::new(),
            Some(Value::Array(areas)) => areas
                .iter()
                .map(|area| {
                    area.as_str()
                        .map(str::to_string)
                        .ok_or_else(|| format!("{at}.addedAreas must be an array of strings"))
                })
                .collect::<Result<_, _>>()?,
            Some(_) => return Err(format!("{at}.addedAreas must be an array of strings")),
        };
        let commit = match map.get("commit") {
            None => None,
            Some(Value::String(commit)) => Some(commit.clone()),
            Some(_) => return Err(format!("{at}.commit must be a string")),
        };
        Ok(Self {
            version: text("version")?,
            source: text("source")?,
            sha256: text("sha256")?,
            added_areas,
            commit,
        })
    }
}

/// The install marker `.houserules.json`, read whole so that a write keeps
/// every key it does not own, in its order.
pub(crate) struct Marker {
    path: PathBuf,
    /// How a line names the file: its path for a command, its relative name
    /// for a gate.
    file: String,
    map: Map<String, Value>,
    /// `true` when the file does not exist: the marker holds the defaults
    /// that [`Marker::read_or_default`] made.
    fresh: bool,
}

/// The install schema id that `init` stamps (spec section 3).
const INSTALL_SCHEMA: &str = "houserules/install@2.0.0";

/// The text that an install schema id starts with.
const INSTALL_SCHEMA_PREFIX: &str = "houserules/install@";

/// The install schema major that this binary writes. D4
/// ([`refuse_install_schema`]) compares the major of a marker with it.
const INSTALL_SCHEMA_MAJOR: u64 = 2;

/// The install `schema` of a marker: the id as written, and its major.
struct InstallSchema {
    id: String,
    major: u64,
}

/// D4's line for a tree whose install schema is newer than this binary
/// writes (spec C22). `marker` is the marker file. The form follows the line
/// for a pack copy with a newer major (`packs::parse_pack`).
fn newer_line(marker: &Path, schema: &InstallSchema) -> String {
    format!(
        "{}: install schema {:?} is newer than houserules {} reads; install a houserules release that reads install schema major {}",
        marker.display(),
        schema.id,
        env!("CARGO_PKG_VERSION"),
        schema.major
    )
}

/// The line for an install of houserules 1.x: `marker` is its marker file.
/// `init` refuses such an install with this line, and so does D4.
pub(crate) fn unconverted_line(marker: &Path) -> String {
    format!(
        "{}: this is an install of houserules 1.x; run houserules update to convert it",
        marker.display()
    )
}

/// D4's line for a tree with no marker file: `update` converts it when the
/// project passes its backlog id prefix, so no remedy stamps `WI` on a
/// project with its own prefix.
pub(crate) fn absent_marker_line(marker: &Path) -> String {
    format!(
        "{}: not found; run houserules update --id-prefix <prefix> to convert an install of houserules 1.x, or houserules init to start a new one",
        marker.display()
    )
}

/// What `init` stamps into the marker.
pub(crate) struct InstallStamp<'a> {
    /// The running kit's version.
    pub(crate) version: &'a str,
    /// The id prefix of the install.
    pub(crate) id_prefix: &'a str,
    /// Every installed pack's pin.
    pub(crate) pins: &'a BTreeMap<String, Pin>,
    /// The `renderers` value; `None` leaves the key as found.
    pub(crate) renderers: Option<&'a [String]>,
    /// The path baselines.
    pub(crate) baselines: Map<String, Value>,
}

impl Marker {
    /// Reads the marker at the install root. A file that cannot be read or
    /// parsed, or that is no JSON object, is one named line. Every line
    /// names the file by its path.
    pub(crate) fn read(root: &Path) -> Result<Self, String> {
        let path = root.join(INSTALL_MARKER);
        let file = path.display().to_string();
        let value = read_json(&path)?;
        Self::from_json(path, file, value)
    }

    /// Reads the marker at the install root for `init` and `update`: an
    /// absent file reads as `{idPrefix: default_prefix}`. A present marker
    /// is one named line when its `idPrefix` fails [`is_id_prefix`], its
    /// `version` is not a non-empty string (a JSON `null` included, since
    /// `null` is a present value), its `overrides`, `ownedPaths`, or
    /// `renderers` is not an array of strings, its `baselines` is not an
    /// object of strings, or its `schema` or `defaultBranch` is not a string.
    /// An adopter-hand-edited field never vanishes into a silent default. Only a
    /// file that is not there is absent: a file whose existence cannot be told
    /// is one line with the operating system's own text.
    pub(crate) fn read_or_default(root: &Path, default_prefix: &str) -> Result<Self, String> {
        let path = root.join(INSTALL_MARKER);
        let file = path.display().to_string();
        Self::read_or_default_named(path, file, default_prefix)
    }

    /// Reads the marker at the install root for a gate: the same reader,
    /// defaults, and checks as [`Marker::read_or_default`], with every line
    /// naming the file by its relative name, as each finding of a gate does.
    pub(crate) fn read_or_default_for_gate(
        root: &Path,
        default_prefix: &str,
    ) -> Result<Self, String> {
        let path = root.join(INSTALL_MARKER);
        Self::read_or_default_named(path, INSTALL_MARKER.to_string(), default_prefix)
    }

    /// The reader behind [`Marker::read_or_default`] and
    /// [`Marker::read_or_default_for_gate`]: `file` is how a line names the
    /// marker at `path`.
    fn read_or_default_named(
        path: PathBuf,
        file: String,
        default_prefix: &str,
    ) -> Result<Self, String> {
        let present = path
            .try_exists()
            .map_err(|error| format!("{file}: {error}"))?;
        if !present {
            let map = Map::from_iter([("idPrefix".to_string(), json!(default_prefix))]);
            return Ok(Self {
                path,
                file,
                map,
                fresh: true,
            });
        }
        let value = read_json_as(&path, &file)?;
        let marker = Self::from_json(path, file, value)?;
        marker.validate()?;
        Ok(marker)
    }

    /// The marker that `value` holds, or the line that names `file` as a
    /// file that is no JSON object.
    fn from_json(path: PathBuf, file: String, value: Value) -> Result<Self, String> {
        match value {
            Value::Object(map) => Ok(Self {
                path,
                file,
                map,
                fresh: false,
            }),
            _ => Err(format!("{file}: not a JSON object")),
        }
    }

    /// The first key of `init` and `update` whose value has the wrong shape,
    /// as one line.
    fn validate(&self) -> Result<(), String> {
        let file = &self.file;
        if let Some(id_prefix) = self.map.get("idPrefix")
            && !id_prefix.as_str().is_some_and(is_id_prefix)
        {
            return Err(format!("{file}: idPrefix {ID_PREFIX_HINT}"));
        }
        if let Some(version) = self.map.get("version")
            && !version.as_str().is_some_and(|v| !v.is_empty())
        {
            return Err(format!("{file}: version must be a non-empty string"));
        }
        if let Some(overrides) = self.map.get("overrides")
            && !overrides
                .as_array()
                .is_some_and(|entries| entries.iter().all(Value::is_string))
        {
            return Err(format!("{file}: overrides must be an array of strings"));
        }
        if let Some(baselines) = self.map.get("baselines")
            && !baselines
                .as_object()
                .is_some_and(|entries| entries.values().all(Value::is_string))
        {
            return Err(format!("{file}: baselines must be an object of strings"));
        }
        if let Some(schema) = self.map.get("schema")
            && !schema.is_string()
        {
            return Err(format!("{file}: schema must be a string"));
        }
        if !self.has_default_branch_shape() {
            return Err(format!("{file}: defaultBranch must be a string"));
        }
        for key in ["ownedPaths", "renderers"] {
            if let Some(list) = self.map.get(key)
                && !list
                    .as_array()
                    .is_some_and(|items| items.iter().all(Value::is_string))
            {
                return Err(format!("{file}: {key} must be an array of strings"));
            }
        }
        Ok(())
    }

    /// The strings of the array under `key`. [`Marker::validate`] has
    /// rejected a present value that is no array of strings, so an absent
    /// key is the only other case, as an empty list.
    fn strings(&self, key: &str) -> Vec<String> {
        self.map
            .get(key)
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default()
    }

    /// `true` for a 2.0 install: the marker holds the install `schema` key,
    /// which `init` stamps and a 1.x tree lacks.
    pub(crate) fn is_2_0(&self) -> bool {
        self.map.contains_key("schema")
    }

    /// The install `schema` key, or `None` when the marker has no such key.
    /// A value that is no `houserules/install@<major>.<minor>.<patch>` id is
    /// one line that names the file.
    fn install_schema(&self) -> Result<Option<InstallSchema>, String> {
        let Some(value) = self.map.get("schema") else {
            return Ok(None);
        };
        let Some(id) = value.as_str() else {
            return Err(format!("{}: schema must be a string", self.file));
        };
        let parts: Option<Vec<u64>> = id
            .strip_prefix(INSTALL_SCHEMA_PREFIX)
            .map(|version| version.split('.').map(|part| part.parse().ok()).collect())
            .unwrap_or(None);
        match parts.as_deref() {
            Some([major, _minor, _patch]) => Ok(Some(InstallSchema {
                id: id.to_string(),
                major: *major,
            })),
            _ => Err(format!(
                "{}: schema {id:?} is not {INSTALL_SCHEMA_PREFIX}<major>.<minor>.<patch>",
                self.file
            )),
        }
    }

    /// The paths the project owns: the `ownedPaths` list. `update` never
    /// rewrites such a path. A 1.x marker lists them under `overrides`, and
    /// the conversion moves them (`Marker::overrides`).
    pub(crate) fn owned_paths(&self) -> Vec<String> {
        self.strings("ownedPaths")
    }

    /// `true` when the Claude renderer is enabled: `renderers` lists
    /// `claude`, or the marker has no `renderers` key. A 1.x tree has no such
    /// key and keeps its Claude files, as does a 2.0 marker that lost the
    /// key.
    pub(crate) fn claude_enabled(&self) -> bool {
        !self.map.contains_key("renderers")
            || self.strings("renderers").iter().any(|r| r == "claude")
    }

    /// The path of the marker file.
    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    /// `true` when the file does not exist yet.
    pub(crate) fn is_fresh(&self) -> bool {
        self.fresh
    }

    /// `true` for a marker file of a 1.x tree: it exists and has no
    /// `schema` key.
    pub(crate) fn is_1_x(&self) -> bool {
        !self.fresh && !self.is_2_0()
    }

    /// The `renderers` value that `init` writes. A marker that holds the
    /// key keeps its list, with `claude` added when `add_claude`. A new
    /// marker gets `[]` or `["claude"]`. A 2.0 marker without the key stays
    /// without it: an absent key already reads as `["claude"]`.
    pub(crate) fn renderers_for_init(&self, add_claude: bool) -> Option<Vec<String>> {
        if self.map.contains_key("renderers") {
            let mut renderers = self.strings("renderers");
            if add_claude && !renderers.iter().any(|r| r == "claude") {
                renderers.push("claude".to_string());
            }
            Some(renderers)
        } else if self.fresh {
            Some(if add_claude {
                vec!["claude".to_string()]
            } else {
                Vec::new()
            })
        } else {
            None
        }
    }

    /// Stamps the 2.0 install: the `schema` key, `version`, `idPrefix`, the
    /// pins, `renderers` when `stamp` names a value, and `baselines`. A new
    /// marker gets its keys in the order of the layout (spec section 3);
    /// an existing marker keeps its own order, and every key that `stamp`
    /// does not name stays as read.
    pub(crate) fn stamp_install(&mut self, stamp: InstallStamp<'_>) {
        if self.fresh {
            self.map = [
                "schema",
                "version",
                "idPrefix",
                "packs",
                "renderers",
                "baselines",
            ]
            .into_iter()
            .map(|key| (key.to_string(), Value::Null))
            .collect();
        }
        self.map.insert("schema".to_string(), json!(INSTALL_SCHEMA));
        self.restamp(stamp.version, stamp.id_prefix, stamp.baselines);
        self.set_pins(stamp.pins);
        if let Some(renderers) = stamp.renderers {
            self.map.insert("renderers".to_string(), json!(renderers));
        }
    }

    /// The id prefix payload content is rewritten with: the marker's own
    /// stamped `idPrefix` when it has one, else `flag_prefix` (the resolved
    /// `--id-prefix` value, already defaulted to `WI`). An install that has
    /// committed to a prefix keeps it whatever a later flag says; only an
    /// install with no stamped prefix yet takes the flag's value.
    pub(crate) fn id_prefix_or(&self, flag_prefix: &str) -> String {
        self.map
            .get("idPrefix")
            .and_then(Value::as_str)
            .unwrap_or(flag_prefix)
            .to_string()
    }

    /// The stamped `version`, when the marker has one.
    pub(crate) fn version(&self) -> Option<&str> {
        self.map.get("version").and_then(Value::as_str)
    }

    /// `true` when the marker has no `defaultBranch` key or the key holds a
    /// string.
    fn has_default_branch_shape(&self) -> bool {
        self.map.get("defaultBranch").is_none_or(Value::is_string)
    }

    /// The `defaultBranch`: the branch that the durable-sha check judges
    /// reachability against. [`Marker::validate`] and
    /// [`read_default_branch_for_gate`] have rejected a value that is no
    /// string, so an absent key is the only other case.
    fn default_branch(&self) -> Option<&str> {
        self.map.get("defaultBranch").and_then(Value::as_str)
    }

    /// The `overrides` list of a 1.x marker: each entry a kit-owned or
    /// seed-once path, or a knowledge-entry id the adopter declared their
    /// own, or `None` when the marker has no such key. [`Marker::validate`]
    /// has rejected a present value that is no array of strings.
    pub(crate) fn overrides(&self) -> Option<Vec<String>> {
        self.map
            .contains_key("overrides")
            .then(|| self.strings("overrides"))
    }

    /// Removes the 1.x `overrides` key. The conversion has moved its values
    /// to `ownedPaths` and `knowledge/overrides.json`.
    pub(crate) fn remove_overrides(&mut self) {
        self.map.shift_remove("overrides");
    }

    /// Sets `ownedPaths`. The key keeps its place when it exists and goes
    /// last when it does not.
    pub(crate) fn set_owned_paths(&mut self, paths: &[String]) {
        self.map.insert("ownedPaths".to_string(), json!(paths));
    }

    /// Sets `renderers`. The key keeps its place when it exists and goes
    /// last when it does not.
    pub(crate) fn set_renderers(&mut self, renderers: &[&str]) {
        self.map.insert("renderers".to_string(), json!(renderers));
    }

    /// Sets the install `schema` key to the id that this binary writes: the
    /// step that makes a converted tree a 2.0 install. The key keeps its
    /// place when it exists and goes last when it does not.
    pub(crate) fn stamp_schema(&mut self) {
        self.map.insert("schema".to_string(), json!(INSTALL_SCHEMA));
    }

    /// The `baselines` object: the hex SHA-256 the kit last wrote for each
    /// kit-owned path and kit-shipped knowledge entry. [`Marker::validate`]
    /// has rejected a present value that is no object of strings, so an
    /// absent key is the only other case, as an empty object.
    pub(crate) fn baselines(&self) -> Map<String, Value> {
        self.map
            .get("baselines")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default()
    }

    /// Sets `version`, `idPrefix`, and `baselines`. Each key keeps its place
    /// when it exists and goes last when it does not; every other key,
    /// an adopter's hand-added key included, stays as read.
    pub(crate) fn restamp(
        &mut self,
        version: &str,
        id_prefix: &str,
        baselines: Map<String, Value>,
    ) {
        self.map.insert("version".to_string(), json!(version));
        self.map.insert("idPrefix".to_string(), json!(id_prefix));
        self.set_baselines(baselines);
    }

    /// Sets `baselines`. The key keeps its place when it exists and goes
    /// last when it does not.
    pub(crate) fn set_baselines(&mut self, baselines: Map<String, Value>) {
        self.map
            .insert("baselines".to_string(), Value::Object(baselines));
    }

    /// Every pin, by pack name. An absent `packs` key holds no pin. A
    /// `packs` value or a pin of another shape is one line that names the
    /// field.
    pub(crate) fn pins(&self) -> Result<BTreeMap<String, Pin>, String> {
        let Some(packs) = self.map.get("packs") else {
            return Ok(BTreeMap::new());
        };
        let Some(packs) = packs.as_object() else {
            return Err(format!("{}: packs must be an object", self.file));
        };
        packs
            .iter()
            .map(|(name, value)| Ok((name.clone(), Pin::from_value(&self.file, name, value)?)))
            .collect()
    }

    /// Replaces the `packs` key with `pins`, sorted by pack name. The key
    /// keeps its place when it exists and goes last when it does not. An
    /// empty `pins` leaves an absent key absent.
    pub(crate) fn set_pins(&mut self, pins: &BTreeMap<String, Pin>) {
        if pins.is_empty() && !self.map.contains_key("packs") {
            return;
        }
        let packs: Map<String, Value> = pins
            .iter()
            .map(|(name, pin)| (name.clone(), pin.to_value()))
            .collect();
        self.map.insert("packs".to_string(), Value::Object(packs));
    }

    /// Writes the marker in emit form. A file whose bytes would not change
    /// is not written.
    pub(crate) fn write(&self) -> Result<(), String> {
        write_json(&self.path, &Value::Object(self.map.clone()))
    }
}

/// `root`'s own stamped `idPrefix`, for a reader that needs it outside
/// `init` and `update` -- `get`'s id-shape routing
/// (`crate::get::is_backlog_id`) is the one caller. `"WI"` stands in for
/// the CLI's `--id-prefix` flag (`get` takes none): an absent marker, or
/// one without an `idPrefix` key, resolves to `"WI"`, the default of `init`
/// and `update`; a present, invalid `idPrefix` is the named error of
/// [`Marker::read_or_default`] (`houserules.crash-paths-are-named`).
pub(crate) fn stamped_id_prefix(root: &Path) -> Result<String, String> {
    Ok(Marker::read_or_default(root, "WI")?.id_prefix_or("WI"))
}

/// The commands that D4 guards, by the marker states they run on
/// (`refuse_install_schema` lists the states and what each guard does with
/// them).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Guard {
    /// A command that reads the install: it runs on a tree whose install
    /// schema has the major that this binary writes, and on no other tree.
    ReadsInstall,
    /// `init` and `update`: they also run on a tree with no marker, on a marker
    /// that cannot be read (they name that defect themselves), on a marker with
    /// no `schema` key (an install of houserules 1.x), and on an older major.
    /// They refuse a `schema` that is malformed or newer, as every command does.
    ConvertsInstall,
}

/// D4: refuses the tree at `root` that `guard` does not allow. The refusal is
/// one line that names the marker. By the state of the marker:
///
/// - No marker file: `absent_marker_line` for [`Guard::ReadsInstall`]; passes
///   for [`Guard::ConvertsInstall`].
/// - A file that cannot be examined, read, or parsed, or that is no JSON
///   object: the line of that defect for [`Guard::ReadsInstall`]; passes for
///   [`Guard::ConvertsInstall`], since `init` and `update` read the marker next
///   (`Marker::read_or_default`) and name the defect after their own placement
///   errors.
/// - A `schema` that is no string, or no
///   `houserules/install@<major>.<minor>.<patch>` id: the line of that defect,
///   for every guard (spec C23).
/// - An install schema with a higher major than `INSTALL_SCHEMA_MAJOR`:
///   `newer_line`, for every guard (spec C22).
/// - An install schema with the major that this binary writes: passes, for
///   every guard.
/// - No `schema` key, or an older major: `unconverted_line` for
///   [`Guard::ReadsInstall`]; passes for [`Guard::ConvertsInstall`].
///
/// The guard reads the `schema` key and no other key: `init` and `update`
/// name every other defect of the marker themselves (`Marker::validate`).
pub(crate) fn refuse_install_schema(root: &Path, guard: Guard) -> Result<(), String> {
    let path = root.join(INSTALL_MARKER);
    let marker = match guard {
        Guard::ReadsInstall => {
            let present = path
                .try_exists()
                .map_err(|error| format!("{}: {error}", path.display()))?;
            if !present {
                return Err(absent_marker_line(&path));
            }
            Marker::read(root)?
        }
        Guard::ConvertsInstall => match path.try_exists() {
            Ok(true) => match Marker::read(root) {
                Ok(marker) => marker,
                Err(_) => return Ok(()),
            },
            _ => return Ok(()),
        },
    };
    match (guard, marker.install_schema()?) {
        (_, Some(schema)) if schema.major > INSTALL_SCHEMA_MAJOR => Err(newer_line(&path, &schema)),
        (Guard::ReadsInstall, Some(schema)) if schema.major == INSTALL_SCHEMA_MAJOR => Ok(()),
        (Guard::ReadsInstall, _) => Err(unconverted_line(&path)),
        (Guard::ConvertsInstall, _) => Ok(()),
    }
}

/// The pins of the install at `root`, as `check-knowledge` reads them: the
/// reader of [`Marker::pins`], with every line naming the file by its
/// relative name, as each finding of a gate names its file. An absent
/// marker pins nothing. A marker that cannot give its pins is one line that
/// names the defect.
pub(crate) fn read_pins_for_gate(root: &Path) -> Result<BTreeMap<String, Pin>, String> {
    let path = root.join(INSTALL_MARKER);
    let present = path
        .try_exists()
        .map_err(|error| format!("{INSTALL_MARKER}: {error}"))?;
    if !present {
        return Ok(BTreeMap::new());
    }
    let value = read_json_as(&path, INSTALL_MARKER)?;
    Marker::from_json(path, INSTALL_MARKER.to_string(), value)?.pins()
}

/// `true` when the install at `root` enables the Claude renderer, as `render`
/// and `check-knowledge` read it: `renderers` lists `claude`, or the marker or
/// its `renderers` key is absent ([`Marker::claude_enabled`]). A marker that
/// cannot give the answer is one line that names the defect, with the file by
/// its relative name.
pub(crate) fn read_claude_enabled_for_gate(root: &Path) -> Result<bool, String> {
    let path = root.join(INSTALL_MARKER);
    let present = path
        .try_exists()
        .map_err(|error| format!("{INSTALL_MARKER}: {error}"))?;
    if !present {
        return Ok(true);
    }
    let value = read_json_as(&path, INSTALL_MARKER)?;
    let marker = Marker::from_json(path, INSTALL_MARKER.to_string(), value)?;
    if let Some(renderers) = marker.map.get("renderers")
        && !renderers
            .as_array()
            .is_some_and(|items| items.iter().all(Value::is_string))
    {
        return Err(format!(
            "{INSTALL_MARKER}: renderers must be an array of strings"
        ));
    }
    Ok(marker.claude_enabled())
}

/// Why a gate cannot take the `defaultBranch` from the marker. A gate words
/// each case in its own line.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum DefaultBranchFault {
    /// The file cannot be read; carries the operating system's own text.
    Unreadable(String),
    /// The file holds invalid JSON; carries the parser's own text.
    InvalidJson(String),
    /// The file holds JSON that is no object.
    NotAnObject,
    /// `defaultBranch` holds a value that is no string.
    NotAString,
}

/// The `defaultBranch` of the install at `root`, as a gate reads it. `None`
/// when the marker or the key is absent; only a file that is not there is
/// absent. Every other defect is a [`DefaultBranchFault`].
pub(crate) fn read_default_branch_for_gate(
    root: &Path,
) -> Result<Option<String>, DefaultBranchFault> {
    let path = root.join(INSTALL_MARKER);
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(DefaultBranchFault::Unreadable(error.to_string())),
    };
    let value: Value = serde_json::from_str(&text)
        .map_err(|error| DefaultBranchFault::InvalidJson(error.to_string()))?;
    let Value::Object(map) = value else {
        return Err(DefaultBranchFault::NotAnObject);
    };
    let marker = Marker {
        path,
        file: INSTALL_MARKER.to_string(),
        map,
        fresh: false,
    };
    if !marker.has_default_branch_shape() {
        return Err(DefaultBranchFault::NotAString);
    }
    Ok(marker.default_branch().map(str::to_string))
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::emit::emit;

    fn marker_in(dir: &Path, value: &Value) -> Marker {
        std::fs::write(dir.join(INSTALL_MARKER), emit(value)).expect("write the marker");
        Marker::read(dir).expect("read the marker")
    }

    fn pin(version: &str, commit: Option<&str>) -> Pin {
        Pin {
            version: version.to_string(),
            source: "kit".to_string(),
            sha256: "abc".to_string(),
            added_areas: vec!["docs".to_string()],
            commit: commit.map(str::to_string),
        }
    }

    /// A pin reads back as written, with or without a commit.
    #[test]
    fn a_pin_reads_back_as_it_was_written() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut marker = marker_in(dir.path(), &json!({"idPrefix": "WI"}));
        let pins = BTreeMap::from([
            ("b".to_string(), pin("1.0.0", Some("deadbeef"))),
            ("a".to_string(), pin("2.0.0", None)),
        ]);

        marker.set_pins(&pins);
        marker.write().expect("write the marker");

        assert_eq!(
            Marker::read(dir.path())
                .expect("read")
                .pins()
                .expect("pins"),
            pins
        );
    }

    /// The key keeps its place, a new key goes last, and the names sort.
    #[test]
    fn set_pins_keeps_the_key_in_place_and_sorts_the_names() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut marker = marker_in(
            dir.path(),
            &json!({"idPrefix": "WI", "packs": {}, "baselines": {}}),
        );

        marker.set_pins(&BTreeMap::from([
            ("b".to_string(), pin("1.0.0", None)),
            ("a".to_string(), pin("1.0.0", None)),
        ]));
        marker.write().expect("write the marker");

        let written: Value = read_json(&dir.path().join(INSTALL_MARKER)).expect("read");
        let keys: Vec<&String> = written.as_object().expect("an object").keys().collect();
        assert_eq!(keys, ["idPrefix", "packs", "baselines"]);
        let names: Vec<&String> = written["packs"]
            .as_object()
            .expect("an object")
            .keys()
            .collect();
        assert_eq!(names, ["a", "b"]);
    }

    /// An empty set leaves an absent `packs` key absent and an existing
    /// one empty.
    #[test]
    fn set_pins_with_no_pin_leaves_an_absent_key_absent() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut absent = marker_in(dir.path(), &json!({"idPrefix": "WI"}));
        absent.set_pins(&BTreeMap::new());
        assert!(!absent.map.contains_key("packs"));

        let mut present = marker_in(
            dir.path(),
            &json!({"packs": {"a": pin("1.0.0", None).to_value()}}),
        );
        present.set_pins(&BTreeMap::new());
        assert_eq!(present.map["packs"], json!({}));
    }

    /// Each malformed shape is one line that names the field.
    #[test]
    fn a_malformed_pin_names_its_field() {
        let dir = tempfile::tempdir().expect("tempdir");
        let at = dir.path().join(INSTALL_MARKER).display().to_string();
        let cases = [
            (
                json!({"packs": []}),
                format!("{at}: packs must be an object"),
            ),
            (
                json!({"packs": {"a": 1}}),
                format!("{at}: packs.a must be an object"),
            ),
            (
                json!({"packs": {"a": {"source": "kit", "sha256": "x"}}}),
                format!("{at}: packs.a: missing \"version\""),
            ),
            (
                json!({"packs": {"a": {"version": 1, "source": "kit", "sha256": "x"}}}),
                format!("{at}: packs.a.version must be a string"),
            ),
            (
                json!({"packs": {"a": {"version": "1.0.0", "sha256": "x"}}}),
                format!("{at}: packs.a: missing \"source\""),
            ),
            (
                json!({"packs": {"a": {"version": "1.0.0", "source": 1, "sha256": "x"}}}),
                format!("{at}: packs.a.source must be a string"),
            ),
            (
                json!({"packs": {"a": {"version": "1.0.0", "source": "kit"}}}),
                format!("{at}: packs.a: missing \"sha256\""),
            ),
            (
                json!({"packs": {"a": {"version": "1.0.0", "source": "kit", "sha256": 1}}}),
                format!("{at}: packs.a.sha256 must be a string"),
            ),
            (
                json!({"packs": {"a": {"version": "1.0.0", "source": "kit", "sha256": "x", "addedAreas": "docs"}}}),
                format!("{at}: packs.a.addedAreas must be an array of strings"),
            ),
            (
                json!({"packs": {"a": {"version": "1.0.0", "source": "kit", "sha256": "x", "addedAreas": [1]}}}),
                format!("{at}: packs.a.addedAreas must be an array of strings"),
            ),
            (
                json!({"packs": {"a": {"version": "1.0.0", "source": "kit", "sha256": "x", "commit": 1}}}),
                format!("{at}: packs.a.commit must be a string"),
            ),
            (
                json!({"packs": {"a": {"version": "1.0.0", "source": "kit", "sha256": "x", "extra": 1}}}),
                format!("{at}: packs.a: unknown key \"extra\""),
            ),
        ];
        for (marker, expected) in cases {
            let error = marker_in(dir.path(), &marker)
                .pins()
                .expect_err("a malformed pack key");
            assert_eq!(error, expected);
        }
    }

    /// A gate reads the pins that a pack command reads, from the same file.
    #[test]
    fn a_gate_reads_the_pins_that_a_pack_command_reads() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut marker = marker_in(dir.path(), &json!({"idPrefix": "WI"}));
        let pins = BTreeMap::from([
            ("b".to_string(), pin("1.0.0", Some("deadbeef"))),
            ("a".to_string(), pin("2.0.0", None)),
        ]);
        marker.set_pins(&pins);
        marker.write().expect("write the marker");

        assert_eq!(read_pins_for_gate(dir.path()), Ok(pins));
    }

    /// An absent marker, and a marker with no `packs` key, pin nothing.
    #[test]
    fn a_gate_reads_no_pin_from_an_absent_marker_or_an_absent_packs_key() {
        let dir = tempfile::tempdir().expect("tempdir");
        assert_eq!(read_pins_for_gate(dir.path()), Ok(BTreeMap::new()));

        marker_in(dir.path(), &json!({"idPrefix": "WI"}));
        assert_eq!(read_pins_for_gate(dir.path()), Ok(BTreeMap::new()));
    }

    /// A gate names the marker by its relative name, as every other finding
    /// of a gate names its file, never by the path of the install.
    #[test]
    fn a_gate_names_the_marker_by_its_relative_name() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path().display().to_string();
        let marker = dir.path().join(INSTALL_MARKER);

        std::fs::write(&marker, "{").expect("write the marker");
        let invalid = read_pins_for_gate(dir.path()).expect_err("invalid JSON");
        assert!(
            invalid.starts_with(".houserules.json: invalid JSON ("),
            "{invalid}"
        );

        std::fs::write(&marker, "[]").expect("write the marker");
        assert_eq!(
            read_pins_for_gate(dir.path()),
            Err(".houserules.json: not a JSON object".to_string())
        );

        marker_in(dir.path(), &json!({"packs": {"a": {"source": "kit"}}}));
        assert_eq!(
            read_pins_for_gate(dir.path()),
            Err(".houserules.json: packs.a: missing \"version\"".to_string())
        );

        std::fs::remove_file(&marker).expect("remove the marker");
        std::fs::create_dir(&marker).expect("make a directory of the marker");
        let unreadable = read_pins_for_gate(dir.path()).expect_err("a directory");
        assert!(unreadable.starts_with(".houserules.json: "), "{unreadable}");
        assert!(!unreadable.contains(&root), "{unreadable}");
    }

    /// The Claude renderer is on when `renderers` lists `claude`, and when the
    /// marker or its `renderers` key is absent.
    #[test]
    fn a_gate_reads_the_claude_renderer_from_the_renderers_key() {
        let dir = tempfile::tempdir().expect("tempdir");
        assert_eq!(read_claude_enabled_for_gate(dir.path()), Ok(true));

        for (renderers, enabled) in [
            (json!([]), false),
            (json!(["claude"]), true),
            (json!(["other"]), false),
            (json!(["other", "claude"]), true),
        ] {
            marker_in(dir.path(), &json!({"renderers": renderers}));
            assert_eq!(
                read_claude_enabled_for_gate(dir.path()),
                Ok(enabled),
                "{renderers}"
            );
        }

        marker_in(dir.path(), &json!({"idPrefix": "WI"}));
        assert_eq!(read_claude_enabled_for_gate(dir.path()), Ok(true));
    }

    /// A `renderers` value that is no array of strings, and a marker that
    /// cannot be parsed, are one line that names the marker by its relative
    /// name.
    #[test]
    fn a_gate_names_a_renderers_value_of_the_wrong_shape() {
        let dir = tempfile::tempdir().expect("tempdir");
        for renderers in [json!("claude"), json!(5), json!(["claude", 5]), json!(null)] {
            marker_in(dir.path(), &json!({"renderers": renderers}));
            assert_eq!(
                read_claude_enabled_for_gate(dir.path()),
                Err(".houserules.json: renderers must be an array of strings".to_string()),
                "{renderers}"
            );
        }

        std::fs::write(dir.path().join(INSTALL_MARKER), "{").expect("write the marker");
        let invalid = read_claude_enabled_for_gate(dir.path()).expect_err("invalid JSON");
        assert!(
            invalid.starts_with(".houserules.json: invalid JSON ("),
            "{invalid}"
        );
    }

    /// A marker whose existence cannot be told is one line that names the
    /// file by its relative name and keeps the operating system's own text,
    /// never an empty set of pins. The install root is a regular file here,
    /// so the stat of `<root>/.houserules.json` fails with `ENOTDIR`, which
    /// `Path::try_exists` reports as an error and not as absence. Windows
    /// reports the same path as absent, so the case is Unix only.
    #[cfg(unix)]
    #[test]
    fn a_gate_names_a_marker_whose_existence_cannot_be_told() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path().join("not-a-directory");
        std::fs::write(&root, "a file").expect("write the file");
        let expected = root
            .join(INSTALL_MARKER)
            .try_exists()
            .expect_err("a path below a file cannot be stat'ed");

        assert_eq!(
            read_pins_for_gate(&root),
            Err(format!("{INSTALL_MARKER}: {expected}"))
        );
    }

    /// A command reads the marker the way a gate does: a marker whose
    /// existence cannot be told is one line that names the file by its path
    /// and keeps the operating system's own text, never a fresh default that
    /// reads as an absent marker. The install root is a regular file here;
    /// see the gate's case above for why this is Unix only.
    #[cfg(unix)]
    #[test]
    fn a_command_names_a_marker_whose_existence_cannot_be_told() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path().join("not-a-directory");
        std::fs::write(&root, "a file").expect("write the file");
        let path = root.join(INSTALL_MARKER);
        let expected = path
            .try_exists()
            .expect_err("a path below a file cannot be stat'ed");

        assert_eq!(
            Marker::read_or_default(&root, "WI").err(),
            Some(format!("{}: {expected}", path.display()))
        );
    }

    /// A gate that reads the whole marker names the file by its relative name
    /// for each defect that a command names by its path.
    #[test]
    fn a_gate_names_a_marker_of_the_wrong_shape_by_its_relative_name() {
        let dir = tempfile::tempdir().expect("tempdir");
        for (marker, line) in [
            (
                json!({"renderers": 5}),
                "renderers must be an array of strings",
            ),
            (
                json!({"ownedPaths": "x"}),
                "ownedPaths must be an array of strings",
            ),
            (json!({"version": 5}), "version must be a non-empty string"),
            (
                json!({"idPrefix": "no"}),
                "idPrefix must be 1-8 characters, A-Z then A-Z0-9",
            ),
        ] {
            std::fs::write(dir.path().join(INSTALL_MARKER), emit(&marker)).expect("write");
            assert_eq!(
                Marker::read_or_default_for_gate(dir.path(), "WI").err(),
                Some(format!("{INSTALL_MARKER}: {line}")),
                "{marker}"
            );
        }

        std::fs::write(dir.path().join(INSTALL_MARKER), "{").expect("write the marker");
        let invalid = Marker::read_or_default_for_gate(dir.path(), "WI").err();
        assert!(
            invalid
                .as_deref()
                .is_some_and(|line| line.starts_with(".houserules.json: invalid JSON (")),
            "{invalid:?}"
        );
    }

    /// A marker whose existence cannot be told is one line that names the
    /// file by its relative name when a gate reads it. See the gate's case
    /// above for why this is Unix only.
    #[cfg(unix)]
    #[test]
    fn a_gate_that_reads_the_whole_marker_names_one_whose_existence_cannot_be_told() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path().join("not-a-directory");
        std::fs::write(&root, "a file").expect("write the file");
        let expected = root
            .join(INSTALL_MARKER)
            .try_exists()
            .expect_err("a path below a file cannot be stat'ed");

        assert_eq!(
            Marker::read_or_default_for_gate(&root, "WI").err(),
            Some(format!("{INSTALL_MARKER}: {expected}"))
        );
    }

    /// An absent marker reads as the defaults, and is the only absence that
    /// does.
    #[test]
    fn an_absent_marker_reads_as_the_defaults() {
        let dir = tempfile::tempdir().expect("tempdir");

        let marker = Marker::read_or_default(dir.path(), "AB").expect("an absent marker");

        assert!(marker.is_fresh());
        assert_eq!(marker.id_prefix_or("WI"), "AB");
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

    /// `defaultBranch` is a string: a command refuses another shape with one
    /// line that names the file by its path.
    #[test]
    fn a_default_branch_that_is_no_string_is_one_named_line() {
        let dir = tempfile::tempdir().expect("tempdir");
        let at = dir.path().join(INSTALL_MARKER).display().to_string();
        for value in [
            json!(5),
            json!(["main"]),
            json!(null),
            json!(true),
            json!({}),
        ] {
            std::fs::write(
                dir.path().join(INSTALL_MARKER),
                emit(&json!({ "defaultBranch": value })),
            )
            .expect("write the marker");

            assert_eq!(
                Marker::read_or_default(dir.path(), "WI").err(),
                Some(format!("{at}: defaultBranch must be a string")),
                "{value}"
            );
        }
    }

    /// A gate reads `defaultBranch` from the marker: an absent marker and an
    /// absent key give `None`, a string gives the name, and each defect is a
    /// fault that the gate words itself.
    #[test]
    fn a_gate_reads_the_default_branch_or_names_the_fault() {
        let dir = tempfile::tempdir().expect("tempdir");
        let marker = dir.path().join(INSTALL_MARKER);
        assert_eq!(read_default_branch_for_gate(dir.path()), Ok(None));

        let write = |text: &str| std::fs::write(&marker, text).expect("write the marker");
        write("{\"version\": \"2.0.0\"}\n");
        assert_eq!(read_default_branch_for_gate(dir.path()), Ok(None));
        write("{\"defaultBranch\": \"trunk\"}\n");
        assert_eq!(
            read_default_branch_for_gate(dir.path()),
            Ok(Some("trunk".to_string()))
        );
        for value in ["5", "[\"main\"]", "null", "true", "{}"] {
            write(&format!("{{\"defaultBranch\": {value}}}\n"));
            assert_eq!(
                read_default_branch_for_gate(dir.path()),
                Err(DefaultBranchFault::NotAString),
                "{value}"
            );
        }
        for value in ["\"trunk\"", "[\"trunk\"]", "5", "null", "true"] {
            write(&format!("{value}\n"));
            assert_eq!(
                read_default_branch_for_gate(dir.path()),
                Err(DefaultBranchFault::NotAnObject),
                "{value}"
            );
        }
        write("{\"defaultBranch\": ");
        let parse_error = serde_json::from_str::<Value>("{\"defaultBranch\": ")
            .expect_err("truncated JSON")
            .to_string();
        assert_eq!(
            read_default_branch_for_gate(dir.path()),
            Err(DefaultBranchFault::InvalidJson(parse_error))
        );
        std::fs::remove_file(&marker).expect("remove the marker");
        std::fs::create_dir(&marker).expect("make a directory of the marker");
        let os_error = std::fs::read_to_string(&marker)
            .expect_err("a directory is no file")
            .to_string();
        assert_eq!(
            read_default_branch_for_gate(dir.path()),
            Err(DefaultBranchFault::Unreadable(os_error))
        );
    }

    /// A marker that is no JSON object is one named line.
    #[test]
    fn a_marker_that_is_no_object_is_one_named_line() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join(INSTALL_MARKER), "[]").expect("write the marker");

        let error = Marker::read(dir.path()).err().expect("a refusal");

        assert_eq!(
            error,
            format!(
                "{}: not a JSON object",
                dir.path().join(INSTALL_MARKER).display()
            )
        );
    }

    /// D4 lets a tree through when its install schema major is 2, and refuses
    /// the rest: a missing key and an older major get the line of
    /// an install of 1.x, a missing file gets the line that names
    /// `--id-prefix`, and a value that is no install schema id is one line
    /// that names it.
    #[test]
    fn d4_judges_the_install_schema_major() {
        let dir = tempfile::tempdir().expect("tempdir");
        let marker = dir.path().join(INSTALL_MARKER);
        let refusal_for = |stamp: Value| {
            std::fs::write(&marker, emit(&stamp)).expect("write the marker");
            refuse_install_schema(dir.path(), Guard::ReadsInstall)
        };

        assert_eq!(
            refuse_install_schema(dir.path(), Guard::ReadsInstall),
            Err(absent_marker_line(&marker))
        );
        for schema in ["houserules/install@2.0.0", "houserules/install@2.7.1"] {
            assert_eq!(refusal_for(json!({"schema": schema})), Ok(()), "{schema}");
        }
        for stamp in [
            json!({"idPrefix": "WI"}),
            json!({"schema": "houserules/install@1.0.0"}),
            json!({"schema": "houserules/install@0.9.9"}),
        ] {
            assert_eq!(
                refusal_for(stamp.clone()),
                Err(unconverted_line(&marker)),
                "{stamp}"
            );
        }
        for schema in [
            "nonsense",
            "houserules/install@2.0",
            "houserules/install@2.0.0.0",
            "houserules/install@x.y.z",
            "houserules/pack@2.0.0",
        ] {
            assert_eq!(
                refusal_for(json!({"schema": schema})),
                Err(format!(
                    "{}: schema {schema:?} is not houserules/install@<major>.<minor>.<patch>",
                    marker.display()
                )),
                "{schema}"
            );
        }
        assert_eq!(
            refusal_for(json!({"schema": 2})),
            Err(format!("{}: schema must be a string", marker.display()))
        );
    }

    /// D4 refuses an install schema with a higher major than this binary
    /// writes (spec C22). The line has the form of the line for a newer pack
    /// copy: the marker, the schema id, the running version, and the major
    /// that a release must read.
    #[test]
    fn d4_refuses_a_newer_install_schema_major_with_the_line_of_a_newer_pack_copy() {
        let dir = tempfile::tempdir().expect("tempdir");
        let marker = dir.path().join(INSTALL_MARKER);
        for (schema, major) in [
            ("houserules/install@3.0.0", 3),
            ("houserules/install@3.4.1", 3),
            ("houserules/install@10.0.0", 10),
        ] {
            std::fs::write(&marker, emit(&json!({"schema": schema}))).expect("write the marker");

            assert_eq!(
                refuse_install_schema(dir.path(), Guard::ReadsInstall),
                Err(format!(
                    "{}: install schema {schema:?} is newer than houserules {} reads; install a houserules release that reads install schema major {major}",
                    marker.display(),
                    env!("CARGO_PKG_VERSION")
                )),
                "{schema}"
            );
        }
    }

    /// `init` and `update` run on an absent marker, on a marker that cannot be
    /// read (they name that defect themselves), and on a marker whose install
    /// schema is missing, older, or current. They refuse a value that is no
    /// install schema id with the line that a reading command prints for it
    /// (spec C23).
    #[test]
    fn d4_lets_init_and_update_run_on_every_tree_but_a_newer_or_malformed_one() {
        let dir = tempfile::tempdir().expect("tempdir");
        let marker = dir.path().join(INSTALL_MARKER);
        let guard = |text: &str| {
            std::fs::write(&marker, text).expect("write the marker");
            refuse_install_schema(dir.path(), Guard::ConvertsInstall)
        };

        assert_eq!(
            refuse_install_schema(dir.path(), Guard::ConvertsInstall),
            Ok(())
        );
        for text in [
            "{\"idPrefix\": \"WI\"}",
            "{\"schema\": \"houserules/install@1.0.0\"}",
            "{\"schema\": \"houserules/install@2.7.1\"}",
            "[]",
            "{",
        ] {
            assert_eq!(guard(text), Ok(()), "{text}");
        }
        for text in [
            "{\"schema\": \"nonsense\"}",
            "{\"schema\": 2}",
            "{\"schema\": \"houserules/install@3\"}",
        ] {
            let refusal = guard(text);
            assert!(refusal.is_err(), "{text}");
            assert_eq!(
                refusal,
                refuse_install_schema(dir.path(), Guard::ReadsInstall),
                "{text}"
            );
        }
        assert_eq!(
            guard("{\"schema\": \"houserules/install@3.0.0\"}"),
            Err(newer_line(
                &marker,
                &InstallSchema {
                    id: "houserules/install@3.0.0".to_string(),
                    major: 3
                }
            ))
        );
    }
}
