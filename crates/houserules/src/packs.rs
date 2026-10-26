//! The kit's knowledge packs: five versioned sets of entries that the
//! binary carries, and the checks every pack copy passes before any
//! command reads it.
//!
//! A pack copy is one JSON file: its `name`, `version`, `schema`, the
//! `topics` it serves (each with a `title`), the `areas` its entries use
//! (each with default `paths`), and its `entries`. An entry's topic is
//! the first part of its id. `parse_pack` validates a copy against the
//! embedded schema of the major that the copy declares; the project's own
//! `knowledge/schema.json` never takes part.

use rust_embed::RustEmbed;
use serde_json::Value;

use crate::baseline;
use crate::rules::validate;

/// The pack files of the kit, embedded from this repository's
/// `template/packs/` at compile time.
#[derive(RustEmbed)]
#[folder = "../../template/packs/"]
struct KitPackFiles;

/// The pack schema major that this binary reads.
pub(crate) const PACK_SCHEMA_MAJOR: u64 = 1;

/// The newest pack schema minor of that major that the embedded schema
/// names. A copy that declares a newer minor has its unnamed keys set
/// aside before validation. Minor 1 names the `optIn` list and the
/// `require_trailer`, `exempt_authors`, and `exempt_subjects` fields of a
/// `commits` check. Minor 2 names the `runner` check type and the `tool`,
/// `non_empty`, and `covers_ids` fields of a check. Minor 3 names the
/// `covers` field of a check. Minor 4 names the `intent-required` and
/// `intent-resolves` check types and the `sources` field of a check.
const PACK_SCHEMA_MINOR: u64 = 4;

/// The text that a pack's `schema` id starts with.
const PACK_SCHEMA_PREFIX: &str = "houserules/pack@";

/// The pack name that belongs to the project's own entries.
const RESERVED_PACK_NAME: &str = "local";

/// The embedded schema of pack schema major 1.
const PACK_SCHEMA: &str = include_str!("../schemas/pack-1.json");

/// The embedded schema of overrides schema major 1: the schema of
/// `knowledge/overrides.json`.
pub(crate) const OVERRIDES_SCHEMA: &str = include_str!("../schemas/overrides-1.json");

/// One parsed pack copy.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PackCopy {
    /// The pack name, unique among installed packs.
    pub(crate) name: String,
    /// The pack's own semantic version.
    pub(crate) version: String,
    /// The declared schema id, `houserules/pack@<major>.<minor>.<patch>`.
    pub(crate) schema: String,
    /// Each topic the pack serves, with its title.
    pub(crate) topics: Vec<(String, String)>,
    /// Each area the pack's entries use, with its default globs.
    pub(crate) areas: Vec<(String, Vec<String>)>,
    /// The pack's entries, as the embedded schema names them.
    pub(crate) entries: Vec<Value>,
    /// The whole parsed copy, unknown keys of a newer minor included.
    pub(crate) raw: Value,
}

impl PackCopy {
    /// The ids that the pack lists under `optIn`, in the order it lists
    /// them; none when it lists none. `parse_pack` has checked that each is
    /// an entry of the pack.
    pub(crate) fn opt_in(&self) -> Vec<String> {
        self.raw
            .get("optIn")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect()
    }
}

/// The kit's packs, sorted by name.
///
/// The packs are part of the binary, so a pack that fails `parse_pack`
/// is a build defect, not adopter data: this function panics with the
/// refusal line. A test parses every embedded pack.
pub(crate) fn kit_packs() -> Vec<PackCopy> {
    let mut packs: Vec<PackCopy> = KitPackFiles::iter()
        .map(|file| {
            let bytes = KitPackFiles::get(&file)
                .expect("an embedded file is readable")
                .data;
            let raw: Value =
                serde_json::from_slice(&bytes).expect("an embedded kit pack is valid JSON");
            parse_pack(raw, &format!("template/packs/{file}"))
                .unwrap_or_else(|refusal| panic!("an embedded kit pack is invalid: {refusal}"))
        })
        .collect();
    packs.sort_by(|left, right| left.name.cmp(&right.name));
    packs
}

/// Parses one pack copy. `origin` names the file in every error line.
///
/// The copy is refused, with one line that names `origin` and the case,
/// when its `schema` id is malformed or names a major other than
/// `PACK_SCHEMA_MAJOR`, when its `name` is not a legal pack name, when
/// it breaks the embedded schema, when a topic name is not a slug, when
/// an entry's id starts with a topic the pack does not declare, when
/// an id appears twice, or when an `optIn` id is no entry of the pack.
///
/// A copy that declares a newer minor of the major is checked against the
/// embedded schema with every key the schema does not name set aside: a
/// minor adds only optional keys. `PackCopy::raw` keeps those keys;
/// `PackCopy::entries` does not.
pub(crate) fn parse_pack(raw: Value, origin: &str) -> Result<PackCopy, String> {
    let declared = raw
        .get("schema")
        .and_then(Value::as_str)
        .and_then(|id| schema_version(id).map(|version| (id, version)));
    let Some((id, (major, minor))) = declared else {
        return Err(format!(
            "{origin}: \"schema\" must be \"{PACK_SCHEMA_PREFIX}<major>.<minor>.<patch>\""
        ));
    };
    if major > PACK_SCHEMA_MAJOR {
        return Err(format!(
            "{origin}: pack schema {id:?} is newer than houserules {} reads; install a houserules release that reads pack schema major {major}",
            env!("CARGO_PKG_VERSION")
        ));
    }
    if major < PACK_SCHEMA_MAJOR {
        return Err(format!(
            "{origin}: pack schema {id:?} is older than houserules {} reads (pack schema major {PACK_SCHEMA_MAJOR})",
            env!("CARGO_PKG_VERSION")
        ));
    }
    if let Some(name) = raw.get("name").and_then(Value::as_str)
        && !valid_pack_name(name)
    {
        return Err(format!(
            "{origin}: pack name {name:?} is not allowed; it must match ^[a-z0-9-]+$ and must not be \"{RESERVED_PACK_NAME}\""
        ));
    }
    let schema: Value =
        serde_json::from_str(PACK_SCHEMA).expect("the embedded pack schema is valid JSON");
    let mut known = raw.clone();
    if minor > PACK_SCHEMA_MINOR {
        drop_unnamed_keys(&mut known, &schema, &schema);
    }
    let mut errors = Vec::new();
    validate(&known, &schema, origin, &mut errors, &schema);
    if !errors.is_empty() {
        return Err(errors.join("; "));
    }
    let copy = PackCopy {
        name: validated_text(&known["name"]),
        version: validated_text(&known["version"]),
        schema: id.to_string(),
        topics: validated_object(&known["topics"])
            .iter()
            .map(|(topic, def)| (topic.clone(), validated_text(&def["title"])))
            .collect(),
        areas: validated_object(&known["areas"])
            .iter()
            .map(|(area, def)| {
                let paths = def["paths"].as_array().expect("validated paths");
                (area.clone(), paths.iter().map(validated_text).collect())
            })
            .collect(),
        entries: known["entries"]
            .as_array()
            .expect("validated entries")
            .clone(),
        raw,
    };
    check_topics_and_ids(&copy, origin)?;
    check_opt_in(&copy, origin)?;
    Ok(copy)
}

/// The SHA-256 of a pack copy's canonical serialization, in lowercase
/// hex: the compact JSON of the parsed copy with the keys of every
/// object sorted. Line endings, indentation, and key order of the file
/// never change it; any change to a value, an array's order, or a key
/// does.
pub(crate) fn pack_sha256(raw: &Value) -> String {
    let mut canonical = raw.clone();
    canonical.sort_all_objects();
    baseline::hash(&serde_json::to_vec(&canonical).expect("a JSON Value always serializes"))
}

/// `true` when `name` is a legal pack name: one or more lowercase
/// letters, digits, or hyphens, and not `local`, which is reserved for the
/// project's own entries. The embedded schema alone refuses the same
/// names.
pub(crate) fn valid_pack_name(name: &str) -> bool {
    is_slug(name) && name != RESERVED_PACK_NAME
}

/// The topic of an entry id: the part before the first `.`, or the whole
/// id when it holds none.
pub(crate) fn topic_of(id: &str) -> &str {
    id.split_once('.').map_or(id, |(topic, _)| topic)
}

/// `true` when `text` is an entry id: `^[a-z0-9-]+\.[a-z0-9-]+$`, the id
/// pattern of the knowledge schema.
pub(crate) fn is_entry_id(text: &str) -> bool {
    text.split_once('.')
        .is_some_and(|(topic, slug)| is_slug(topic) && is_slug(slug))
}

/// `true` when `text` matches `^[a-z0-9-]+$`, the pattern of a topic name
/// and of a pack name.
fn is_slug(text: &str) -> bool {
    !text.is_empty()
        && text
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

/// The major and minor of a `houserules/pack@<major>.<minor>.<patch>` id,
/// or `None` for any other text.
fn schema_version(id: &str) -> Option<(u64, u64)> {
    let mut parts = id.strip_prefix(PACK_SCHEMA_PREFIX)?.split('.');
    let mut number = || {
        let part = parts.next()?;
        if part.is_empty() || !part.bytes().all(|byte| byte.is_ascii_digit()) {
            return None;
        }
        part.parse::<u64>().ok()
    };
    let (major, minor, _patch) = (number()?, number()?, number()?);
    parts.next().is_none().then_some((major, minor))
}

/// The text of a value that the schema has validated as a string.
fn validated_text(value: &Value) -> String {
    value.as_str().expect("validated string").to_string()
}

/// The map of a value that the schema has validated as an object.
fn validated_object(value: &Value) -> &serde_json::Map<String, Value> {
    value.as_object().expect("validated object")
}

/// Refuses a topic name that is not a slug, an entry id whose topic the
/// pack does not declare, and an id that appears twice.
fn check_topics_and_ids(copy: &PackCopy, origin: &str) -> Result<(), String> {
    for (topic, _) in &copy.topics {
        if !is_slug(topic) {
            return Err(format!(
                "{origin}: topic name {topic:?} is not allowed; it must match ^[a-z0-9-]+$"
            ));
        }
    }
    let declared: Vec<&str> = copy
        .topics
        .iter()
        .map(|(topic, _)| topic.as_str())
        .collect();
    let mut seen: Vec<&str> = Vec::new();
    for entry in &copy.entries {
        let id = entry["id"].as_str().expect("validated entry id");
        let topic = topic_of(id);
        if !declared.contains(&topic) {
            let declared = if declared.is_empty() {
                "none".to_string()
            } else {
                declared.join(", ")
            };
            return Err(format!(
                "{origin}: entry {id:?} is outside the topics this pack declares ({declared})"
            ));
        }
        if seen.contains(&id) {
            return Err(format!("{origin}: entry {id:?} appears twice"));
        }
        seen.push(id);
    }
    Ok(())
}

/// Refuses an `optIn` id that no entry of the pack carries: an exclusion of
/// such an id would name nothing.
fn check_opt_in(copy: &PackCopy, origin: &str) -> Result<(), String> {
    let shipped: Vec<&str> = copy
        .entries
        .iter()
        .map(|entry| entry["id"].as_str().expect("validated entry id"))
        .collect();
    match copy
        .opt_in()
        .into_iter()
        .find(|id| !shipped.contains(&id.as_str()))
    {
        Some(id) => Err(format!(
            "{origin}: optIn id {id:?} is not an entry of this pack"
        )),
        None => Ok(()),
    }
}

/// Removes from `value` every key that `schema` forbids: a key that an
/// object schema with `additionalProperties: false` does not list under
/// `properties`, at every depth. A key that the schema permits stays.
fn drop_unnamed_keys(value: &mut Value, schema: &Value, root: &Value) {
    let schema = schema
        .get("$ref")
        .and_then(Value::as_str)
        .and_then(|reference| reference.strip_prefix('#'))
        .and_then(|pointer| root.pointer(pointer))
        .unwrap_or(schema);
    match value {
        Value::Object(map) => {
            let properties = schema.get("properties").and_then(Value::as_object);
            let additional = schema.get("additionalProperties");
            if additional == Some(&Value::Bool(false)) {
                map.retain(|key, _| properties.is_some_and(|named| named.contains_key(key)));
            }
            for (key, child) in map.iter_mut() {
                let child_schema = properties
                    .and_then(|named| named.get(key))
                    .or(additional.filter(|extra| extra.is_object()));
                if let Some(child_schema) = child_schema {
                    drop_unnamed_keys(child, child_schema, root);
                }
            }
        }
        Value::Array(items) => {
            if let Some(item_schema) = schema.get("items") {
                for item in items {
                    drop_unnamed_keys(item, item_schema, root);
                }
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    use crate::emit::emit;
    use crate::rules::validate;

    /// An entry id is two slugs around one dot. Every path that release 1.x
    /// listed in `overrides` has a slash or an upper-case letter, so none has
    /// the shape of an id: the conversion tells ids from paths by this
    /// predicate.
    #[test]
    fn an_entry_id_is_two_slugs_around_one_dot() {
        for id in ["process.tdd", "knowledge-base.cite-durable-refs", "a1.b-2"] {
            assert!(is_entry_id(id), "{id}");
        }
        for text in [
            "",
            ".",
            "process",
            "process.",
            ".tdd",
            "Process.tdd",
            "a.b.c",
            "a b.c",
            "AGENTS.md",
            "CLAUDE.md",
            "docs/README.md",
            "knowledge/process.json",
            "tools/claude-session-start.sh",
            ":/.github/workflows/knowledge.yml",
        ] {
            assert!(!is_entry_id(text), "{text:?}");
        }
    }

    /// The id of every entry that the kit's packs carry, sorted: the 58
    /// seeded entries of release 1.4.0, the two entries that release
    /// 2.0.0 adds to `knowledge-base`, the opt-in entry, the four
    /// design-phase entries, and the seed rule of the evidence bundle that
    /// it adds to `process`, and the two pin entries that it adds to
    /// `security-hygiene`. A change to this list is a change to a pack's
    /// content and needs a version change in `KIT_PACK_VERSIONS`.
    const KIT_IDS: [&str; 68] = [
        "knowledge-base.cite-durable-refs",
        "knowledge-base.context-proxy-output-is-lossy",
        "knowledge-base.controller-rules-are-tagged",
        "knowledge-base.ids-are-permanent",
        "knowledge-base.rules-need-a-loading-path",
        "knowledge-base.state-only-the-source",
        "knowledge-base.summary-is-the-rule",
        "process.ask-when-missing",
        "process.backlog-drives-work",
        "process.bounded-review-loop",
        "process.brainstorm-first",
        "process.brief-carries-the-spec",
        "process.claims-match-artifacts",
        "process.closure-claims-carry-enumeration",
        "process.code-health-scan",
        "process.contract-refresh-sweep",
        "process.conventional-commits",
        "process.deliverables-json",
        "process.design-claims-are-labelled",
        "process.design-decisions-carry-probes",
        "process.eval-fixture-procedure",
        "process.evals-rerun",
        "process.every-commit-names-a-work-item",
        "process.evidence-before-review",
        "process.evidence-outlives-the-session",
        "process.fix-proofs-extend-the-reviewers-run",
        "process.fix-round-verification-record",
        "process.gate-shell-chains",
        "process.keep-knowledge-current",
        "process.knowledge-earns-its-place",
        "process.knowledge-first",
        "process.live-run-before-ci",
        "process.main-wins-backlog-collisions",
        "process.model-policy",
        "process.no-tech-debt",
        "process.owner-content-mid-batch",
        "process.owner-rulings-need-owner-supersession",
        "process.parallel-on-disjoint-files",
        "process.pointer-sweep-unfiltered",
        "process.report-evidence-verbatim",
        "process.reversals-are-recorded",
        "process.review-findings-are-claims-too",
        "process.risk-tiers",
        "process.rulings-to-file",
        "process.severity-by-impact",
        "process.skills-for-procedures",
        "process.specs-list-borrowed-concepts",
        "process.suggestions-are-unverified",
        "process.tdd",
        "process.verify-in-persisted-state",
        "process.wiring-checks-run-the-resolution",
        "quality.gates-derive-their-scope",
        "quality.no-compat-softening",
        "quality.pin-copies-byte-exact",
        "quality.principles",
        "quality.tests-clean-temp-dirs",
        "quality.well-maintained-libraries",
        "security-hygiene.dependency-vetting",
        "security-hygiene.exact-pins",
        "security-hygiene.no-focused-tests",
        "security-hygiene.tool-pins-are-exact",
        "security-hygiene.tool-versions-pins-are-exact",
        "security-hygiene.verify-current-docs",
        "writing-style.code-comments",
        "writing-style.doc-comments",
        "writing-style.instructions-cover-the-state-space",
        "writing-style.principles",
        "writing-style.readme-is-an-adopter-funnel",
    ];

    /// Each kit pack with its version and the SHA-256 of its canonical
    /// serialization (`pack_sha256`). A change to a pack's content changes
    /// the checksum and so fails the test until this row changes with the
    /// pack's version. The version names the class of the change: major for a
    /// removed entry or a stricter check (a wider `if` glob is a stricter
    /// check), minor for an added entry, patch for a wording change. The
    /// version moves once for each change, from the version before the
    /// change: fix rounds that widen the same globs for one work item move it
    /// once, never once for each round.
    const KIT_PACK_VERSIONS: [(&str, &str, &str); 5] = [
        (
            "knowledge-base",
            "3.0.0",
            "fccdf3f7f38ffdc3d3f45e3d99b3d52d84f44e83f0636d56963cb4712b9f33d9",
        ),
        (
            "process",
            "2.1.1",
            "c207b1a729a974c294ec6f6d10a0a630ba0979f34e9498251f9593daa10fb7a5",
        ),
        (
            "quality",
            "1.0.3",
            "2eb25b6f2ad6e916795987ee0ef9661b4150a1a6d6b445b051feeb480077f4a3",
        ),
        (
            "security-hygiene",
            "5.0.0",
            "ddda662be06eff51dd80afc62e3caec14f89b4c2177e22da870a76883892128e",
        ),
        (
            "writing-style",
            "1.0.2",
            "96797a15b1cbf71f274ab45e3f66185c1d2de98247594e43bafa3e3f296e4e55",
        ),
    ];

    /// The title that each kit pack declares for its one topic.
    const KIT_TOPIC_TITLES: [(&str, &str); 5] = [
        ("knowledge-base", "Authoring knowledge entries"),
        (
            "process",
            "How work runs: batches, dispatch, reviews, rulings",
        ),
        ("quality", "Quality principles"),
        ("security-hygiene", "Dependency, commit, and test hygiene"),
        (
            "writing-style",
            "Writing style for docs, comments, commits, reports",
        ),
    ];

    /// An area name with its default globs.
    type AreaGlobs = (&'static str, &'static [&'static str]);

    /// The areas that each kit pack declares, with their default globs.
    const KIT_PACK_AREAS: [(&str, &[AreaGlobs]); 5] = [
        ("knowledge-base", &[("docs", &DOCS_GLOBS)]),
        (
            "process",
            &[("global", &[]), ("process", &[]), ("docs", &DOCS_GLOBS)],
        ),
        (
            "quality",
            &[("global", &[]), ("process", &[]), ("tools", &["tools/**"])],
        ),
        ("security-hygiene", &[("global", &[])]),
        ("writing-style", &[("global", &[]), ("docs", &DOCS_GLOBS)]),
    ];

    /// The default globs of the `docs` area.
    const DOCS_GLOBS: [&str; 5] = [
        "docs/**",
        "AGENTS.md",
        "knowledge/**",
        "backlog/**",
        ".claude/**",
    ];

    const ORIGIN: &str = "knowledge/packs/demo.json";

    /// A pack copy with every key the schema names, so a schema keyword
    /// that the validator does not implement fails in every test that
    /// parses it.
    fn full_pack() -> Value {
        json!({
            "name": "demo",
            "version": "1.2.3",
            "schema": "houserules/pack@1.2.0",
            "topics": {"demo": {"title": "Demo topic"}},
            "areas": {"docs": {"paths": ["docs/**", "AGENTS.md"]}, "global": {"paths": []}},
            "entries": [
                {
                    "id": "demo.one",
                    "kind": "rule",
                    "area": "docs",
                    "standing": true,
                    "status": "active",
                    "summary": "The first demo rule.",
                    "body": ["One paragraph.", "Another paragraph."],
                    "tags": ["demo", "first"],
                    "source": {"date": "2026-10-19", "by": "docs", "ref": "demo"},
                    "see": ["demo.two"],
                    "verify": ["docs/demo.md"],
                    "check": {
                        "type": "grep-absent",
                        "level": "warn",
                        "files": ["docs/**", "AGENTS.md"],
                        "pattern": "forbidden",
                        "flags": "i",
                        "scope": "changed",
                        "subject": "subject",
                        "body_absent": "absent",
                        "body_line_max": 100,
                        "require_trailer": "Work-Item",
                        "exempt_authors": ["dependabot[bot]"],
                        "exempt_subjects": ["^chore\\(.+\\): release "],
                        "if": "src/**",
                        "then": "docs/**",
                        "per_commit": true,
                        "field": "live_run"
                    }
                },
                {
                    "id": "demo.two",
                    "kind": "gotcha",
                    "area": "global",
                    "summary": "The second demo rule.",
                    "body": [],
                    "tags": [],
                    "source": {"date": "2026-10-19", "by": "review"}
                }
            ],
            "optIn": ["demo.two"]
        })
    }

    /// The smallest valid pack copy.
    fn minimal_pack() -> Value {
        json!({
            "name": "demo",
            "version": "1.0.0",
            "schema": "houserules/pack@1.0.0",
            "topics": {"demo": {"title": "Demo"}},
            "areas": {"global": {"paths": []}},
            "entries": [{
                "id": "demo.one",
                "kind": "rule",
                "area": "global",
                "summary": "S",
                "body": ["b"],
                "tags": [],
                "source": {"date": "2026-10-19", "by": "docs"}
            }]
        })
    }

    /// The refusal line for `raw`; fails the test when `parse_pack` accepts it.
    fn refusal(raw: Value) -> String {
        parse_pack(raw, ORIGIN).expect_err("parse_pack must refuse this copy")
    }

    /// `value` with the keys of every object in reverse order.
    fn with_reversed_keys(value: &Value) -> Value {
        match value {
            Value::Object(map) => Value::Object(
                map.iter()
                    .rev()
                    .map(|(key, child)| (key.clone(), with_reversed_keys(child)))
                    .collect(),
            ),
            Value::Array(items) => Value::Array(items.iter().map(with_reversed_keys).collect()),
            other => other.clone(),
        }
    }

    /// Every string value of `value`, with its JSON path.
    fn string_values(value: &Value, at: &str, out: &mut Vec<(String, String)>) {
        match value {
            Value::String(text) => out.push((at.to_string(), text.clone())),
            Value::Array(items) => {
                for (index, item) in items.iter().enumerate() {
                    string_values(item, &format!("{at}[{index}]"), out);
                }
            }
            Value::Object(map) => {
                for (key, child) in map {
                    string_values(child, &format!("{at}.{key}"), out);
                }
            }
            _ => {}
        }
    }

    /// The kit's packs; fails the test when the kit does not carry five, so a
    /// loop over them can never pass on an empty set.
    fn five_kit_packs() -> Vec<PackCopy> {
        let packs = kit_packs();
        assert_eq!(packs.len(), 5, "the kit carries five packs");
        packs
    }

    /// The entry of the kit pack `pack` whose id is `id`.
    fn kit_entry(pack: &str, id: &str) -> Value {
        kit_packs()
            .into_iter()
            .find(|copy| copy.name == pack)
            .and_then(|copy| {
                copy.entries
                    .into_iter()
                    .find(|entry| entry["id"] == json!(id))
            })
            .unwrap_or_else(|| panic!("kit pack {pack} has no entry {id}"))
    }

    #[test]
    fn kit_packs_are_the_five_kit_packs_sorted_by_name() {
        let names: Vec<String> = kit_packs().into_iter().map(|pack| pack.name).collect();
        assert_eq!(
            names,
            [
                "knowledge-base",
                "process",
                "quality",
                "security-hygiene",
                "writing-style"
            ]
        );
    }

    #[test]
    fn the_payload_holds_exactly_the_five_pack_files() {
        let mut files: Vec<String> = KitPackFiles::iter().map(|file| file.into_owned()).collect();
        files.sort();
        assert_eq!(
            files,
            [
                "knowledge-base.json",
                "process.json",
                "quality.json",
                "security-hygiene.json",
                "writing-style.json"
            ]
        );
    }

    #[test]
    fn kit_pack_ids_are_the_1_4_0_ids_plus_the_ten_new_ids_each_once() {
        let mut ids: Vec<String> = kit_packs()
            .iter()
            .flat_map(|pack| pack.entries.iter())
            .map(|entry| entry["id"].as_str().expect("entry id").to_string())
            .collect();
        ids.sort();
        assert_eq!(ids, KIT_IDS);
    }

    #[test]
    fn every_kit_pack_serves_one_topic_named_like_the_pack_with_its_1_4_0_title() {
        let actual: Vec<(String, Vec<(String, String)>)> = kit_packs()
            .into_iter()
            .map(|pack| (pack.name, pack.topics))
            .collect();
        let expected: Vec<(String, Vec<(String, String)>)> = KIT_TOPIC_TITLES
            .iter()
            .map(|(name, title)| {
                (
                    (*name).to_string(),
                    vec![((*name).to_string(), (*title).to_string())],
                )
            })
            .collect();
        assert_eq!(actual, expected);
    }

    #[test]
    fn every_kit_pack_declares_exactly_the_areas_its_entries_use_with_the_default_globs() {
        for pack in five_kit_packs() {
            let expected = KIT_PACK_AREAS
                .iter()
                .find(|(name, _)| *name == pack.name)
                .map(|(_, areas)| *areas)
                .unwrap_or_else(|| panic!("no area table for {}", pack.name));
            let mut declared = pack.areas.clone();
            declared.sort();
            let mut wanted: Vec<(String, Vec<String>)> = expected
                .iter()
                .map(|(area, globs)| {
                    (
                        (*area).to_string(),
                        globs.iter().map(|glob| (*glob).to_string()).collect(),
                    )
                })
                .collect();
            wanted.sort();
            assert_eq!(declared, wanted, "areas of {}", pack.name);
            let mut used: Vec<String> = pack
                .entries
                .iter()
                .map(|entry| entry["area"].as_str().expect("entry area").to_string())
                .collect();
            used.sort();
            used.dedup();
            let declared_names: Vec<String> = wanted.iter().map(|(area, _)| area.clone()).collect();
            assert_eq!(used, declared_names, "areas used by {}", pack.name);
        }
    }

    #[test]
    fn no_kit_pack_entry_links_to_an_id_outside_its_own_pack() {
        for pack in five_kit_packs() {
            let own: Vec<&str> = pack
                .entries
                .iter()
                .map(|entry| entry["id"].as_str().expect("entry id"))
                .collect();
            for entry in &pack.entries {
                for target in entry["see"].as_array().into_iter().flatten() {
                    let target = target.as_str().expect("see target");
                    assert!(
                        own.contains(&target),
                        "{} links to {target}, which {} does not ship",
                        entry["id"],
                        pack.name
                    );
                }
                assert_ne!(
                    entry["see"],
                    json!([]),
                    "{} keeps an empty see",
                    entry["id"]
                );
            }
        }
    }

    #[test]
    fn every_kit_pack_copy_is_in_emit_form() {
        assert_eq!(KitPackFiles::iter().count(), 5);
        for file in KitPackFiles::iter() {
            let bytes = KitPackFiles::get(&file).expect("embedded pack file").data;
            let value: Value = serde_json::from_slice(&bytes).expect("embedded pack is JSON");
            assert_eq!(
                emit(&value).as_bytes(),
                &bytes[..],
                "{file} differs from emit's output"
            );
        }
    }

    #[test]
    fn kit_pack_checksums_match_the_version_table() {
        let actual: Vec<(String, String, String)> = kit_packs()
            .iter()
            .map(|pack| {
                (
                    pack.name.clone(),
                    pack.version.clone(),
                    pack_sha256(&pack.raw),
                )
            })
            .collect();
        let expected: Vec<(String, String, String)> = KIT_PACK_VERSIONS
            .iter()
            .map(|(name, version, sha)| {
                (
                    (*name).to_string(),
                    (*version).to_string(),
                    (*sha).to_string(),
                )
            })
            .collect();
        assert_eq!(
            actual, expected,
            "a kit pack changed: change its version and its row in KIT_PACK_VERSIONS together"
        );
    }

    #[test]
    fn the_new_knowledge_base_entries_name_no_id_outside_the_pack_and_nothing_of_this_repository() {
        // `.houserules.json` is a file name of every install, not an id.
        let forbidden = regress::Regex::new(
            r"\b(houserules|process|quality|security-hygiene|writing-style)\.(?!json\b)[a-z0-9-]+|crates/|\.rs\b|\bHR-\d+|\b[Bb]atch \d+",
        )
        .expect("the pattern compiles");
        for id in [
            "knowledge-base.cite-durable-refs",
            "knowledge-base.context-proxy-output-is-lossy",
        ] {
            let entry = kit_entry("knowledge-base", id);
            let mut strings = Vec::new();
            string_values(&entry["summary"], "summary", &mut strings);
            string_values(&entry["body"], "body", &mut strings);
            assert!(!strings.is_empty());
            for (at, text) in strings {
                assert!(
                    forbidden.find(&text).is_none(),
                    "{id} {at} names an id outside knowledge-base or a part of this repository: {text}"
                );
            }
            assert!(
                entry.get("verify").is_none(),
                "{id} names a path of this repository"
            );
        }
    }

    #[test]
    fn cite_durable_refs_ships_with_its_fail_level_durable_sha_check_and_its_source() {
        let entry = kit_entry("knowledge-base", "knowledge-base.cite-durable-refs");
        // The `files` of the check are pinned by `tests/pin_checks.rs`.
        assert_eq!(entry["check"]["type"], json!("durable-sha"));
        assert_eq!(entry["check"]["level"], json!("fail"));
        assert_eq!(
            entry["source"],
            json!({"date": "2026-10-19", "by": "docs", "ref": "HR-178, issue 50"})
        );
        assert_eq!(entry["kind"], json!("rule"));
        assert_eq!(entry["area"], json!("docs"));
    }

    #[test]
    fn cite_durable_refs_names_the_default_branch_key_of_the_install_marker() {
        let entry = kit_entry("knowledge-base", "knowledge-base.cite-durable-refs");
        let body = entry["body"]
            .as_array()
            .expect("body")
            .iter()
            .map(|paragraph| paragraph.as_str().expect("paragraph"))
            .collect::<Vec<_>>()
            .join("\n");
        for named in ["`defaultBranch`", "`.houserules.json`"] {
            assert!(body.contains(named), "the body must name {named}");
        }
    }

    #[test]
    fn the_context_proxy_gotcha_states_the_measured_loss_and_names_no_proxy_or_setting() {
        let entry = kit_entry(
            "knowledge-base",
            "knowledge-base.context-proxy-output-is-lossy",
        );
        assert_eq!(entry["kind"], json!("gotcha"));
        assert_eq!(entry["area"], json!("docs"));
        assert_eq!(
            entry["summary"],
            json!(
                "A context proxy can shorten or drop houserules output with no visible sign; the source files and the gates are the truth, not the transcript."
            )
        );
        let mut strings = Vec::new();
        string_values(&entry, "entry", &mut strings);
        let text = strings
            .iter()
            .map(|(_, text)| text.to_lowercase())
            .collect::<Vec<_>>()
            .join("\n");
        for word in [
            "headroom",
            "ccr",
            "environment variable",
            "setting",
            "configur",
        ] {
            assert!(!text.contains(word), "the gotcha names {word:?}");
        }
    }

    #[test]
    fn no_kit_pack_string_holds_a_compression_marker() {
        for pack in five_kit_packs() {
            assert_eq!(
                crate::rules::marker_findings(&pack.raw, &pack.name),
                Vec::<String>::new()
            );
        }
    }

    #[test]
    fn pack_sha256_is_equal_across_line_endings_whitespace_and_key_order() {
        let raw = full_pack();
        let pretty = emit(&raw);
        let crlf = pretty.replace('\n', "\r\n");
        let compact = serde_json::to_string(&raw).expect("serializes");
        let reordered =
            serde_json::to_string_pretty(&with_reversed_keys(&raw)).expect("serializes");
        assert_ne!(reordered, pretty, "the reordered copy must differ in bytes");
        let expected = pack_sha256(&raw);
        assert_eq!(expected.len(), 64, "a SHA-256 digest in hex");
        for (label, text) in [
            ("crlf", crlf),
            ("compact", compact),
            ("reordered keys", reordered),
        ] {
            let parsed: Value = serde_json::from_str(&text).expect("parses");
            assert_eq!(pack_sha256(&parsed), expected, "{label}");
        }
    }

    #[test]
    fn pack_sha256_is_a_lowercase_hex_digest_that_changes_with_each_content_change() {
        let raw = full_pack();
        let digest = pack_sha256(&raw);
        assert_eq!(digest.len(), 64);
        assert!(
            digest
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        );
        let mut summary = raw.clone();
        summary["entries"][0]["summary"] = json!("Another summary.");
        let mut version = raw.clone();
        version["version"] = json!("1.2.4");
        let mut order = raw.clone();
        order["entries"].as_array_mut().expect("entries").reverse();
        let mut body_order = raw.clone();
        body_order["entries"][0]["body"]
            .as_array_mut()
            .expect("body")
            .reverse();
        for (label, changed) in [
            ("summary", summary),
            ("version", version),
            ("entry order", order),
            ("body paragraph order", body_order),
        ] {
            assert_ne!(pack_sha256(&changed), digest, "{label}");
        }
    }

    #[test]
    fn parse_pack_accepts_a_minimal_pack_and_a_pack_with_every_key() {
        let minimal = parse_pack(minimal_pack(), ORIGIN).expect("minimal pack parses");
        assert_eq!(minimal.name, "demo");
        assert_eq!(minimal.version, "1.0.0");
        assert_eq!(minimal.schema, "houserules/pack@1.0.0");
        assert_eq!(minimal.topics, [("demo".to_string(), "Demo".to_string())]);
        assert_eq!(
            minimal.areas,
            [("global".to_string(), Vec::<String>::new())]
        );
        assert_eq!(minimal.entries.len(), 1);
        assert_eq!(minimal.raw, minimal_pack());
        let full = parse_pack(full_pack(), ORIGIN).expect("full pack parses");
        assert_eq!(full.entries.len(), 2);
        assert_eq!(
            full.areas,
            [
                (
                    "docs".to_string(),
                    vec!["docs/**".to_string(), "AGENTS.md".to_string()]
                ),
                ("global".to_string(), Vec::new())
            ]
        );
    }

    #[test]
    fn parse_pack_reads_the_opt_in_ids_in_the_order_the_pack_lists_them() {
        let mut raw = full_pack();
        raw["optIn"] = json!(["demo.two", "demo.one"]);
        let copy = parse_pack(raw, ORIGIN).expect("a pack with opt-in ids parses");
        assert_eq!(copy.opt_in(), ["demo.two", "demo.one"]);
        let minimal = parse_pack(minimal_pack(), ORIGIN).expect("minimal pack parses");
        assert_eq!(minimal.opt_in(), Vec::<String>::new());
    }

    #[test]
    fn parse_pack_refuses_an_opt_in_id_that_the_pack_does_not_ship() {
        let mut raw = full_pack();
        raw["optIn"] = json!(["demo.two", "demo.three"]);
        assert_eq!(
            refusal(raw),
            format!("{ORIGIN}: optIn id \"demo.three\" is not an entry of this pack")
        );
    }

    #[test]
    fn parse_pack_refuses_a_malformed_opt_in_list() {
        for (optin, message) in [
            (json!("demo.one"), ".optIn: must be array"),
            (
                json!(["demo.one", "demo.one"]),
                ".optIn: items must be unique",
            ),
            (
                json!(["Demo.One"]),
                ".optIn[0]: must match ^[a-z0-9-]+\\.[a-z0-9-]+$",
            ),
            (json!([1]), ".optIn[0]: must be string"),
        ] {
            let mut raw = full_pack();
            raw["optIn"] = optin;
            assert_eq!(refusal(raw), format!("{ORIGIN}{message}"));
        }
    }

    #[test]
    fn the_embedded_pack_schema_names_minor_4_and_holds_the_check_of_the_knowledge_schema() {
        assert_eq!(PACK_SCHEMA_MINOR, 4);
        let pack: Value = serde_json::from_str(PACK_SCHEMA).expect("the embedded schema is JSON");
        let knowledge: Value =
            serde_json::from_str(include_str!("../../../template/knowledge/schema.json"))
                .expect("the knowledge schema is JSON");
        assert_eq!(
            pack["$defs"]["check"], knowledge["$defs"]["check"],
            "the two check definitions are one definition"
        );
    }

    /// An override may set every field of a check that a knowledge entry may
    /// hold: the two definitions list the same properties, require the same
    /// ones, and refuse the same others. Only the `null` that removes a check
    /// differs.
    #[test]
    fn the_embedded_overrides_schema_holds_every_property_of_the_check_of_the_knowledge_schema() {
        let overrides: Value =
            serde_json::from_str(OVERRIDES_SCHEMA).expect("the embedded schema is JSON");
        let knowledge: Value =
            serde_json::from_str(include_str!("../../../template/knowledge/schema.json"))
                .expect("the knowledge schema is JSON");
        let override_check =
            &overrides["$defs"]["override"]["properties"]["fields"]["properties"]["check"];
        let knowledge_check = &knowledge["$defs"]["check"];

        for key in ["properties", "required", "additionalProperties"] {
            assert_eq!(
                override_check[key], knowledge_check[key],
                "the override check and the knowledge check differ in {key}"
            );
        }
    }

    #[test]
    fn the_process_pack_lists_the_every_commit_entry_under_opt_in_at_pack_schema_1_2_0() {
        let process = five_kit_packs()
            .into_iter()
            .find(|pack| pack.name == "process")
            .expect("the kit carries the process pack");
        assert_eq!(process.schema, "houserules/pack@1.2.0");
        assert_eq!(process.opt_in(), ["process.every-commit-names-a-work-item"]);
        for other in five_kit_packs()
            .iter()
            .filter(|pack| pack.name != "process")
        {
            assert_eq!(other.opt_in(), Vec::<String>::new(), "{}", other.name);
        }
    }

    /// Case 14: the two kit packs whose checks use the report-field options of
    /// pack schema 1.2.0 declare that schema. The other three keep their id.
    #[test]
    fn the_kit_packs_that_use_the_report_field_options_declare_pack_schema_1_2_0() {
        let declared: Vec<(String, String)> = five_kit_packs()
            .into_iter()
            .map(|pack| (pack.name, pack.schema))
            .collect();
        assert_eq!(
            declared,
            [
                ("knowledge-base", "houserules/pack@1.0.0"),
                ("process", "houserules/pack@1.2.0"),
                ("quality", "houserules/pack@1.0.0"),
                ("security-hygiene", "houserules/pack@1.2.0"),
                ("writing-style", "houserules/pack@1.0.0"),
            ]
            .map(|(name, schema)| (name.to_string(), schema.to_string()))
        );
    }

    /// Spec 5 and 202: the kit entries that read a report field hold the
    /// content options and keep level `warn`; the entry that reads
    /// `self_audit` keeps its check, because the comparison needs no field.
    #[test]
    fn the_kit_entries_that_read_report_fields_hold_the_content_options_at_level_warn() {
        assert_eq!(
            kit_entry("process", "process.knowledge-first")["check"],
            json!({
                "type": "report-field", "level": "warn", "if": "**",
                "field": "knowledge_used", "non_empty": true, "covers_ids": true,
            })
        );
        let mut docs =
            kit_entry("security-hygiene", "security-hygiene.verify-current-docs")["check"].clone();
        docs.as_object_mut()
            .expect("a check is an object")
            .remove("if");
        assert_eq!(
            docs,
            json!({
                "type": "report-field", "level": "warn", "field": "docs_verified",
                "non_empty": true,
            })
        );
        assert_eq!(
            kit_entry("process", "process.deliverables-json")["check"],
            json!({
                "type": "report-field", "level": "fail", "if": "**", "field": "self_audit",
            })
        );
    }

    /// Spec 6 and 12: the body of `process.deliverables-json` states the
    /// comparison, and the body of `process.knowledge-earns-its-place` names
    /// the window of `stats`.
    #[test]
    fn the_bodies_of_two_process_entries_state_the_comparison_and_the_window() {
        let body = |id: &str| -> Vec<String> {
            kit_entry("process", id)["body"]
                .as_array()
                .expect("a body is an array")
                .iter()
                .map(|line| line.as_str().expect("a body line is text").to_string())
                .collect()
        };
        let deliverables = body("process.deliverables-json");
        for sentence in [
            "It leaves out the row of this check.",
            "It takes the result of that row out of the result counts of the summary (`pass`, `fail`, \
             `warn`, and `skipped`) on each side; a copy that lacks the row keeps its counts.",
            "A difference fails the row with `self_audit differs at <JSON path>`.",
            "A `--workspace` audit does not compare.",
            "After the last edit to the report, run the audit again and copy `self_audit` again.",
        ] {
            assert!(
                deliverables.iter().any(|line| line.contains(sentence)),
                "process.deliverables-json lacks {sentence:?}"
            );
        }
        assert!(
            !deliverables.iter().any(|line| line.contains(
                "the result counts of the summary (`pass`, `fail`, `warn`, and `skipped`), and"
            )),
            "process.deliverables-json still leaves out every result count"
        );
        let stats = body("process.knowledge-earns-its-place");
        assert!(
            stats
                .iter()
                .any(|line| line.contains("`houserules stats --window 5 <WORKSPACE>...`")),
            "process.knowledge-earns-its-place names no window"
        );
        assert!(
            !stats
                .iter()
                .any(|line| line.contains("`houserules stats <WORKSPACE>...`")),
            "process.knowledge-earns-its-place still names the run without a window"
        );
    }

    #[test]
    fn parse_pack_refuses_each_forbidden_name_with_one_line_that_names_it() {
        for name in [
            "../schema",
            "a/b",
            "local",
            "Upper",
            "",
            "with space",
            "under_score",
            "dot.name",
            "..",
        ] {
            let mut raw = minimal_pack();
            raw["name"] = json!(name);
            assert_eq!(
                refusal(raw),
                format!(
                    "{ORIGIN}: pack name {name:?} is not allowed; it must match ^[a-z0-9-]+$ and must not be \"local\""
                )
            );
        }
    }

    #[test]
    fn parse_pack_accepts_a_name_of_lowercase_letters_digits_and_hyphens() {
        for name in ["a", "0", "kit-1", "-", "9-lives"] {
            let mut raw = minimal_pack();
            raw["name"] = json!(name);
            assert_eq!(parse_pack(raw, ORIGIN).expect(name).name, name);
        }
    }

    #[test]
    fn valid_pack_name_agrees_with_the_embedded_schema_pattern_and_refuses_local() {
        let schema: Value = serde_json::from_str(PACK_SCHEMA).expect("the embedded schema is JSON");
        let name_schema = &schema["properties"]["name"];
        for name in [
            "a", "0", "kit-1", "-", "local", "locale", "local-x", "xlocal", "Upper", "", "a/b",
            "../x", "x y", "é", "a.b", "a_b",
        ] {
            let mut errors = Vec::new();
            validate(&json!(name), name_schema, "name", &mut errors, &schema);
            assert_eq!(valid_pack_name(name), errors.is_empty(), "{name:?}");
        }
    }

    #[test]
    fn the_embedded_schema_alone_refuses_a_copy_named_local() {
        let schema: Value = serde_json::from_str(PACK_SCHEMA).expect("the embedded schema is JSON");
        let mut raw = minimal_pack();
        raw["name"] = json!("local");
        let mut errors = Vec::new();
        validate(&raw, &schema, ORIGIN, &mut errors, &schema);
        assert_eq!(
            errors,
            [format!("{ORIGIN}.name: must match ^(?!local$)[a-z0-9-]+$")]
        );
    }

    #[test]
    fn parse_pack_refuses_a_newer_schema_major_and_names_the_schema_it_needs() {
        let mut raw = minimal_pack();
        raw["schema"] = json!("houserules/pack@2.0.0");
        assert_eq!(
            refusal(raw),
            format!(
                "{ORIGIN}: pack schema \"houserules/pack@2.0.0\" is newer than houserules {} reads; install a houserules release that reads pack schema major 2",
                env!("CARGO_PKG_VERSION")
            )
        );
    }

    #[test]
    fn parse_pack_refuses_an_older_schema_major() {
        let mut raw = minimal_pack();
        raw["schema"] = json!("houserules/pack@0.9.0");
        assert_eq!(
            refusal(raw),
            format!(
                "{ORIGIN}: pack schema \"houserules/pack@0.9.0\" is older than houserules {} reads (pack schema major {PACK_SCHEMA_MAJOR})",
                env!("CARGO_PKG_VERSION")
            )
        );
    }

    #[test]
    fn parse_pack_refuses_a_missing_or_malformed_schema_id() {
        for schema in [
            json!("houserules/overrides@1.0.0"),
            json!("houserules/pack@1.0"),
            json!("houserules/pack@one.0.0"),
            json!("pack@1.0.0"),
            json!(1),
            json!(null),
        ] {
            let mut raw = minimal_pack();
            raw["schema"] = schema.clone();
            assert_eq!(
                refusal(raw),
                format!("{ORIGIN}: \"schema\" must be \"houserules/pack@<major>.<minor>.<patch>\"")
            );
        }
        let mut raw = minimal_pack();
        raw.as_object_mut().expect("object").remove("schema");
        assert_eq!(
            refusal(raw),
            format!("{ORIGIN}: \"schema\" must be \"houserules/pack@<major>.<minor>.<patch>\"")
        );
        assert_eq!(
            refusal(json!([])),
            format!("{ORIGIN}: \"schema\" must be \"houserules/pack@<major>.<minor>.<patch>\"")
        );
    }

    #[test]
    fn parse_pack_refuses_an_id_outside_the_declared_topics() {
        let mut raw = minimal_pack();
        raw["entries"][0]["id"] = json!("other.one");
        assert_eq!(
            refusal(raw),
            format!(
                "{ORIGIN}: entry \"other.one\" is outside the topics this pack declares (demo)"
            )
        );
    }

    #[test]
    fn parse_pack_names_a_pack_without_topics_in_words() {
        let mut raw = minimal_pack();
        raw["topics"] = json!({});
        assert_eq!(
            refusal(raw),
            format!("{ORIGIN}: entry \"demo.one\" is outside the topics this pack declares (none)")
        );
    }

    #[test]
    fn parse_pack_refuses_an_id_that_appears_twice() {
        let mut raw = minimal_pack();
        let entry = raw["entries"][0].clone();
        raw["entries"].as_array_mut().expect("entries").push(entry);
        assert_eq!(
            refusal(raw),
            format!("{ORIGIN}: entry \"demo.one\" appears twice")
        );
    }

    #[test]
    fn parse_pack_refuses_a_topic_name_outside_the_slug_pattern() {
        for topic in ["Demo", "a/b", "", "a.b"] {
            let mut raw = minimal_pack();
            raw["topics"] = json!({ topic: {"title": "T"} });
            assert_eq!(
                refusal(raw),
                format!(
                    "{ORIGIN}: topic name {topic:?} is not allowed; it must match ^[a-z0-9-]+$"
                )
            );
        }
    }

    #[test]
    fn parse_pack_reports_each_schema_violation_with_its_path() {
        let mut raw = minimal_pack();
        raw["entries"][0]
            .as_object_mut()
            .expect("entry")
            .remove("summary");
        raw["entries"][0]["kind"] = json!("law");
        raw["version"] = json!("1.0");
        let line = refusal(raw);
        assert_eq!(
            line,
            format!(
                "{ORIGIN}.version: must match ^\\d+\\.\\d+\\.\\d+$; {ORIGIN}.entries[0]: missing \"summary\"; {ORIGIN}.entries[0].kind: must be one of \"rule\", \"invariant\", \"gotcha\", \"procedure\", \"decision\", \"history\""
            )
        );
        assert!(!line.contains('\n'), "one line");
    }

    #[test]
    fn parse_pack_refuses_an_unknown_key_at_the_minor_it_knows_and_names_the_key() {
        for (path, mutate) in [
            ("top level", "extra"),
            ("topic", "topics.demo.extra"),
            ("area", "areas.global.extra"),
            ("entry", "entries.0.extra"),
            ("source", "entries.0.source.extra"),
        ] {
            let mut raw = minimal_pack();
            set_path(&mut raw, mutate, json!(1));
            let line = refusal(raw);
            assert!(line.contains("unknown field \"extra\""), "{path}: {line}");
        }
    }

    #[test]
    fn parse_pack_ignores_unknown_keys_of_a_newer_minor_and_keeps_them_in_raw() {
        let mut raw = minimal_pack();
        raw["schema"] = json!("houserules/pack@1.7.0");
        for path in [
            "extra",
            "topics.demo.extra",
            "areas.global.extra",
            "entries.0.extra",
            "entries.0.source.extra",
        ] {
            set_path(&mut raw, path, json!({"new": true}));
        }
        raw["entries"][0]["check"] = json!({"type": "commits", "level": "warn", "extra": 1});
        let pack = parse_pack(raw.clone(), ORIGIN).expect("a newer minor parses");
        assert_eq!(pack.schema, "houserules/pack@1.7.0");
        assert_eq!(pack.raw, raw, "raw keeps every key");
        let entry = &pack.entries[0];
        assert!(entry.get("extra").is_none());
        assert!(entry["source"].get("extra").is_none());
        assert!(entry["check"].get("extra").is_none());
        assert_eq!(entry["check"]["type"], json!("commits"));
    }

    #[test]
    fn parse_pack_still_refuses_a_newer_minor_that_breaks_a_known_rule() {
        let mut raw = minimal_pack();
        raw["schema"] = json!("houserules/pack@1.7.0");
        raw["entries"][0]
            .as_object_mut()
            .expect("entry")
            .remove("summary");
        raw["entries"][0]["extra"] = json!(1);
        assert_eq!(
            refusal(raw),
            format!("{ORIGIN}.entries[0]: missing \"summary\"")
        );
    }

    /// Sets the value at a dotted path, where a numeric step indexes an array.
    fn set_path(root: &mut Value, path: &str, value: Value) {
        let mut steps: Vec<&str> = path.split('.').collect();
        let last = steps.pop().expect("a path has a step");
        let mut at = root;
        for step in steps {
            at = match step.parse::<usize>() {
                Ok(index) => &mut at[index],
                Err(_) => &mut at[step],
            };
        }
        at[last] = value;
    }

    /// The embedded overrides schema, parsed.
    fn overrides_schema() -> Value {
        serde_json::from_str(OVERRIDES_SCHEMA).expect("the embedded schema is JSON")
    }

    fn overrides_errors(doc: &Value) -> Vec<String> {
        let schema = overrides_schema();
        let mut errors = Vec::new();
        validate(doc, &schema, "overrides.json", &mut errors, &schema);
        errors
    }

    #[test]
    fn the_overrides_schema_accepts_overrides_and_exclusions_with_every_key() {
        let doc = json!({
            "schema": "houserules/overrides@1.0.0",
            "overrides": [
                {"id": "demo.one", "fields": {
                    "kind": "rule",
                    "area": "docs",
                    "standing": null,
                    "status": "retired",
                    "summary": "New summary.",
                    "body": ["Paragraph."],
                    "tags": ["a"],
                    "source": {"date": "2026-10-19", "by": "controller", "ref": "r"},
                    "see": ["demo.two"],
                    "verify": ["docs/x.md"],
                    "check": {
                        "type": "grep-absent", "level": "fail", "files": "docs/**",
                        "pattern": "p", "flags": "i", "scope": "tree", "subject": "s",
                        "body_absent": "b", "body_line_max": 100, "if": ["a"], "then": ["b"],
                        "per_commit": false, "field": "live_run"
                    }
                }},
                {"id": "demo.two", "fields": {
                    "standing": null, "status": null, "see": null, "verify": null, "check": null
                }}
            ],
            "exclusions": [
                {"id": "demo.three"},
                {"id": "demo.four", "reason": "The project has its own rule."}
            ]
        });
        assert_eq!(overrides_errors(&doc), Vec::<String>::new());
        assert_eq!(
            overrides_errors(&json!({"schema": "houserules/overrides@1.3.0"})),
            Vec::<String>::new()
        );
    }

    #[test]
    fn the_overrides_schema_refuses_each_forbidden_shape() {
        let base = |fields: Value| {
            json!({
                "schema": "houserules/overrides@1.0.0",
                "overrides": [{"id": "demo.one", "fields": fields}]
            })
        };
        let cases: Vec<(&str, Value, &str)> = vec![
            ("no schema", json!({"overrides": []}), "missing \"schema\""),
            (
                "newer major",
                json!({"schema": "houserules/overrides@2.0.0"}),
                "must match ^houserules/overrides@1\\.\\d+\\.\\d+$",
            ),
            (
                "unknown top-level key",
                json!({"schema": "houserules/overrides@1.0.0", "extra": 1}),
                "unknown field \"extra\"",
            ),
            (
                "override without fields",
                json!({"schema": "houserules/overrides@1.0.0", "overrides": [{"id": "demo.one"}]}),
                "missing \"fields\"",
            ),
            (
                "override without id",
                json!({"schema": "houserules/overrides@1.0.0", "overrides": [{"fields": {}}]}),
                "missing \"id\"",
            ),
            (
                "id as a field",
                base(json!({"id": "demo.two"})),
                "unknown field \"id\"",
            ),
            (
                "unknown field",
                base(json!({"topic": "demo"})),
                "unknown field \"topic\"",
            ),
            (
                "malformed id",
                json!({"schema": "houserules/overrides@1.0.0", "overrides": [{"id": "Demo", "fields": {}}]}),
                "must match ^[a-z0-9-]+\\.[a-z0-9-]+$",
            ),
            (
                "summary null",
                base(json!({"summary": null})),
                "must be string",
            ),
            (
                "summary too long",
                base(json!({"summary": "x".repeat(161)})),
                "longer than 160",
            ),
            ("kind null", base(json!({"kind": null})), "must be one of"),
            (
                "kind unknown",
                base(json!({"kind": "law"})),
                "must be one of",
            ),
            ("area null", base(json!({"area": null})), "must be string"),
            ("body null", base(json!({"body": null})), "must be array"),
            ("tags null", base(json!({"tags": null})), "must be array"),
            (
                "tags repeated",
                base(json!({"tags": ["a", "a"]})),
                "items must be unique",
            ),
            (
                "source null",
                base(json!({"source": null})),
                "must be object",
            ),
            (
                "source without date",
                base(json!({"source": {"by": "docs"}})),
                "missing \"date\"",
            ),
            (
                "standing text",
                base(json!({"standing": "yes"})),
                "must be boolean or null",
            ),
            (
                "status unknown",
                base(json!({"status": "gone"})),
                "must be one of",
            ),
            (
                "see text",
                base(json!({"see": "demo.two"})),
                "must be array or null",
            ),
            (
                "verify text",
                base(json!({"verify": "x"})),
                "must be array or null",
            ),
            (
                "check without level",
                base(json!({"check": {"type": "commits"}})),
                "missing \"level\"",
            ),
            (
                "check unknown type",
                base(json!({"check": {"type": "magic", "level": "fail"}})),
                "must be one of",
            ),
            (
                "check unknown key",
                base(json!({"check": {"type": "commits", "level": "fail", "extra": 1}})),
                "unknown field \"extra\"",
            ),
            (
                "exclusion without id",
                json!({"schema": "houserules/overrides@1.0.0", "exclusions": [{"reason": "r"}]}),
                "missing \"id\"",
            ),
            (
                "exclusion with an empty reason",
                json!({"schema": "houserules/overrides@1.0.0", "exclusions": [{"id": "demo.one", "reason": ""}]}),
                "shorter than 1",
            ),
            (
                "exclusion with a field",
                json!({"schema": "houserules/overrides@1.0.0", "exclusions": [{"id": "demo.one", "fields": {}}]}),
                "unknown field \"fields\"",
            ),
        ];
        for (label, doc, expected) in cases {
            let errors = overrides_errors(&doc);
            assert!(
                errors.iter().any(|error| error.contains(expected)),
                "{label}: wanted {expected:?}, got {errors:?}"
            );
        }
    }
}
