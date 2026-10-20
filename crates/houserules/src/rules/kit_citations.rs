//! The kit-owned citation lint of `check-knowledge`: every id of a kit topic
//! that a kit-owned file cites must be an entry of the loaded knowledge base.
//!
//! The kit's packs ship the entries that the kit-owned skills and agent
//! templates cite. A project may leave one out of its base: it excludes the
//! entry in `knowledge/overrides.json`, it sets a retired or superseded
//! status for the entry in an override, it never installs the pack that
//! ships it, or the installed copy does not ship it as an active entry.
//! `update` treats each kit-owned file by what the project did to
//! it (`install`'s module doc, "`update` and the ownership baseline"): it
//! overwrites a file the project has not changed, keeps a locally modified
//! file and reports it (`kept <path> (locally modified)`), and leaves a file
//! that the project owns as found. So a citation dangles when the base lacks
//! the cited entry, and it also dangles in a kept file whose older text cites
//! an id that no kit pack ships any more. This lint turns the dangling
//! citation into a finding. The remedy it prints depends on the case
//! (`missing_finding`).
//!
//! The lint is a function of a loaded [`Base`] and a [`KitScope`]. `rules`
//! reads the install marker through `install::marker` only: the crate root
//! passes `install::kit_citation_scope`, which owns the kit-owned file list, to
//! [`super::cmd_check_knowledge`], which hands the scope to the lint.
//! `check_base` stays a function of the base alone, so its callers that hold
//! no install scope run no kit lint.
//!
//! What counts as an id, in order:
//!
//! 1. A citation is a backtick, an id-shaped token, and a backtick, adjacent
//!    with nothing between them. One pattern finds the citations: the id
//!    pattern of the loaded `knowledge/schema.json`
//!    (`/$defs/entry/properties/id/pattern`, the pattern `check_base`
//!    validates every entry id against) with one leading `^` and one
//!    trailing `$` removed, after a literal backtick and before a lookahead
//!    for a literal backtick. The closing backtick is asserted and not
//!    consumed, so one backtick can close one token and open the next. The
//!    lint retypes no pattern. Punctuation outside the backticks is no part
//!    of the token: in "(see `process.tdd`)." the token is `process.tdd`.
//! 2. The token's prefix, the text before its dot, is a topic the payload
//!    seeds ([`KitScope::seeded_topics`]). The vocabulary is the kit's. A
//!    topic the adopter adds does not count, because the kit text shows
//!    backticked names such as `source.date` and `verdict.text`: a topic of
//!    one of those names would turn kit text into a finding, and `update`
//!    writes the same text again
//!    (`an_adopter_topic_never_makes_kit_text_a_finding`). A token such as
//!    `verdict.text` or `implementer.md` has no seeded topic as its prefix
//!    and is not an id. The seeded topics keep the prefix valid when the
//!    project loads no entry of that topic.
//!
//! The lint parses no Markdown and pairs no backticks. Every backtick, token,
//! backtick run in a checked file is a citation candidate, and nothing
//! earlier in the file hides or shifts it: not a stray backtick, a block
//! boundary, a line break, a fence, nor the closing backtick of the span
//! before it. The tests that prove it are the four stray-backtick tests
//! (`a_stray_backtick_in_a_paragraph_does_not_hide_a_later_citation` and its
//! heading, top-of-file, and list-item siblings),
//! `an_id_after_a_line_wrapped_code_span_is_still_found`, and
//! `a_shared_backtick_does_not_hide_the_next_citation`.
//!
//! A candidate goes unreported in these states, and no others:
//!
//! - Its prefix is no seeded topic (item 2;
//!   `a_token_without_a_topic_prefix_is_not_an_id` and
//!   `a_topic_the_payload_does_not_seed_is_no_id_prefix`).
//! - Its file is not checked: the file is absent under the root
//!   (`an_absent_kit_owned_file_is_skipped`), or the project owns it, which
//!   `install::kit_citation_scope` decides (the install test
//!   `kit_citation_scope_leaves_out_the_kit_owned_files_the_2_0_stamp_owns`
//!   and the command test `an_overridden_kit_owned_file_is_skipped`).
//! - The base holds the id (`every_cited_id_that_resolves_yields_no_finding`
//!   and `a_cited_id_that_an_installed_pack_holds_yields_no_finding`).
//!
//! One limit follows from item 2. A kit-owned file that the adopter changed
//! so that it cites an id of a topic the adopter wrote (`team.gone`, where
//! the payload seeds no topic `team`) is not checked for that id: `update`
//! keeps the file as the adopter's own text, and the lint reads only the
//! kit's vocabulary
//! (`a_kit_owned_file_citing_an_id_of_an_adopter_topic_is_not_checked_for_it`).
//!
//! Text that is not a backtick, an id, and a backtick is no candidate, so the
//! lint does not extract it:
//!
//! - A span that pads the id with a space or a line ending, or wraps the id
//!   itself across a line break, because something sits between a backtick
//!   and the id
//!   (`a_span_that_pads_the_id_or_wraps_it_across_a_line_break_is_not_extracted`).
//! - A span that holds more than the id, such as a command that names one
//!   (`an_id_inside_a_longer_code_span_is_not_a_citation`).
//!
//! The pattern also reports shapes in which a renderer shows no code span
//! around the id. One is the closing backtick of one span, an id, and the
//! opening backtick of the next (`` `a`process.tdd`b` ``): the lint reads the
//! id as a citation and reports it when the base lacks it
//! (`an_id_shared_between_two_spans_is_taken_as_a_citation`). When that id
//! is followed by a real citation, as in `` `a`process.tdd`process.gone` ``,
//! the lint reads both tokens: the first is this false-positive class, and
//! the second is found. Another such shape is an id in its own backticks
//! inside a fenced block
//! (`an_id_in_its_own_backticks_inside_a_fence_is_a_citation`), or one whose
//! opening backtick a backslash escapes. An id in its own backticks is a
//! citation wherever it stands, so the second kind is no defect.
//!
//! An id is live when `Base::entries` holds it: the active entries of the
//! local topic files and of the pack copies. An entry that `houserules
//! archive` moved to `knowledge/archive/` is not live here. It renders into
//! no rules file and joins no audit package (`rules::render` and `audit` read
//! `Base::entries`), although `get` and the `see` check still resolve it
//! (`crate::archive`, `check::check_archive`). An excluded pack entry
//! (`Base::excluded`) and a pack entry with a retired or superseded status
//! (`Base::inactive`) are not live either.
//!
//! A cited id that is not live gets one finding, by the first case that
//! applies (`missing_finding`). Every state has a line:
//!
//! 1. The project excludes it. The line names `knowledge/overrides.json` and
//!    says to remove the exclusion or to override the entry with the
//!    project's own text
//!    (`a_cited_id_that_the_project_excludes_is_one_finding_with_the_whole_line`).
//! 2. An override of the entry sets a retired or superseded status. The line
//!    names `knowledge/overrides.json` and the status, and says to remove the
//!    status from the override or to replace the override with one that
//!    carries the project's own text
//!    (`an_id_whose_override_sets_an_inactive_status_is_one_finding_with_the_whole_line`).
//!    The line follows the override: a status of the copy itself gets the
//!    line of case 6, even when an override changes another field
//!    (`an_override_of_another_field_never_gets_the_override_status_line`).
//! 3. No kit pack ships it. The line is the 1.4.0 line for an id the kit does
//!    not seed, with `ownedPaths` as the key that lists the file the project
//!    owns
//!    (`a_cited_id_that_no_kit_pack_ships_is_one_finding_with_the_ownedpaths_line`).
//!    The embedded packs decide which pack ships an id
//!    (`every_id_a_kit_pack_ships_names_its_pack_when_the_install_lacks_it`).
//! 4. A kit pack ships it, and the install holds no copy under the name of
//!    that pack. The line names the pack and `houserules pack add`
//!    (`a_cited_id_of_a_pack_the_install_lacks_is_one_finding_with_the_whole_line`).
//! 5. A kit pack ships it, and a copy under its name is another pack: the pin
//!    of the copy records a source other than `kit`, or the copy has no pin.
//!    The line names the kit pack and the recorded source, which is `an
//!    unrecorded source` for a copy without a pin
//!    (`an_id_of_a_kit_pack_whose_name_another_pack_holds_is_one_finding_with_the_whole_line`
//!    and `a_copy_without_a_pin_is_not_the_kit_pack_and_has_no_recorded_source`).
//! 6. A kit pack ships it, and the copy under its name is the installed kit
//!    pack, which the source `kit` of its pin says. The installed pack does
//!    not ship the id as an active entry: the copy is older than the kit text
//!    and lacks the id, or its own status retires or supersedes it. The line
//!    names the pack and the version of the pin, and says to run `houserules
//!    update`
//!    (`an_id_the_installed_kit_pack_does_not_ship_as_active_is_one_finding_with_the_whole_line`
//!    and `the_version_in_the_inactive_in_copy_line_is_the_pins`).
//!
//! The lint identifies the installed kit pack by the source of its pin
//! (spec L2), never by the name of a copy: an upstream pack that carries the
//! name of a kit pack does not make the citations of kit text live. The pins
//! come from `.houserules.json` (`install::marker::read_pins_for_gate`). A file that
//! cannot give its pins is one named finding, and no citation is checked
//! (`a_pin_file_that_cannot_give_its_pins_is_a_named_finding`).

use std::cell::OnceCell;
use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::io;

use regress::Regex;
use serde_json::Value;

use crate::install::marker::{Pin, read_pins_for_gate};
use crate::packs::{PackCopy, kit_packs};

use super::model::Base;

/// Where the entry id pattern sits in the knowledge schema.
const ID_PATTERN_POINTER: &str = "/$defs/entry/properties/id/pattern";

/// The source that the pin of an installed kit pack records (spec L3a): the
/// binary carries the pack. The lint identifies the installed kit pack by
/// this source and never by the name of a copy (spec L2).
const KIT_SOURCE: &str = "kit";

/// What a finding prints as the source of an installed pack that has no
/// pin.
const UNRECORDED_SOURCE: &str = "an unrecorded source";

/// The finding for a `.houserules.json` that cannot give its pins. The pin
/// check names the defect (`check_packs::pin_findings`), so this line names
/// the consequence only.
const UNREADABLE_PINS_LINE: &str =
    ".houserules.json: the pins cannot be read, so kit-owned citations are not checked";

/// What the lint needs from the install layer. `install::kit_citation_scope`
/// builds it; the tests build it by hand.
pub(crate) struct KitScope {
    /// The kit-owned files to check, as root-relative paths with forward
    /// slashes: the kit-owned paths minus the ones the project owns. The
    /// install layer decides which files are the project's own; the lint
    /// checks every file listed here that exists.
    pub owned_files: Vec<String>,
    /// The topics that the kit seeds: the only prefixes that make a
    /// backticked token an id. A topic the adopter adds does not count.
    pub seeded_topics: Vec<String>,
}

/// One id a kit-owned file cites.
pub(crate) struct Citation {
    /// The kit-owned file, as it appears in [`KitScope::owned_files`].
    pub file: String,
    /// The cited id.
    pub id: String,
}

/// What one pass over the kit-owned files found.
pub(crate) struct Scan {
    /// Every id a checked file cites, once per file, in file order and then
    /// in the order the file first cites it. Live and missing ids alike.
    pub cited: Vec<Citation>,
    /// One line for each input the pass could not read or derive.
    pub problems: Vec<String>,
}

/// Compiles the citation pattern from the entry id pattern of `schema`: that
/// pattern with one leading `^` and one trailing `$` removed, after a
/// backtick and before a lookahead for a backtick, with the token in capture
/// group 1. The lookahead leaves the closing backtick unconsumed, so the next
/// match can open with it. `regress` parses `(?=` as a positive lookahead
/// (its `parse.rs`, "Positive lookahead"). The error names the schema and says
/// that no citation was checked.
fn citation_shape(schema: &Value) -> Result<Regex, String> {
    let pattern = schema
        .pointer(ID_PATTERN_POINTER)
        .and_then(Value::as_str)
        .ok_or_else(|| {
            format!(
                "knowledge/schema.json: {ID_PATTERN_POINTER} is missing; kit-owned citations \
                 not checked"
            )
        })?;
    let unanchored = pattern.strip_prefix('^').unwrap_or(pattern);
    let unanchored = unanchored.strip_suffix('$').unwrap_or(unanchored);
    Regex::new(&format!("`({unanchored})(?=`)")).map_err(|_| {
        format!(
            "knowledge/schema.json: {ID_PATTERN_POINTER} {pattern:?} does not compile; \
             kit-owned citations not checked"
        )
    })
}

/// The text of `id` before its dot.
fn topic_of(id: &str) -> &str {
    id.split_once('.').map_or(id, |(topic, _)| topic)
}

/// The ids `text` cites, once each, in first-seen order. `citation` finds
/// the backticked tokens shaped like an id, and `topics` keeps the ones whose
/// prefix is a seeded topic. `regress` reports the range of capture group 1
/// as byte offsets into `text`; the test `an_id_after_non_ascii_text_is_still_found`
/// pins that.
fn cited_ids(text: &str, citation: &Regex, topics: &HashSet<&str>) -> Vec<String> {
    let mut ids: Vec<String> = Vec::new();
    for found in citation.find_iter(text) {
        let Some(range) = found.group(1) else {
            continue;
        };
        let token = &text[range];
        if !topics.contains(topic_of(token)) {
            continue;
        }
        if !ids.iter().any(|seen| seen == token) {
            ids.push(token.to_string());
        }
    }
    ids
}

/// Reads every checked kit-owned file and collects the ids it cites. A file
/// is checked when `scope.owned_files` lists it and it exists under
/// `base.root`. An absent file is outside the subject set, not an input the
/// pass failed to read, so it prints no line: `update` restores an absent,
/// non-overridden kit-owned file. A file that exists but cannot be read is a
/// problem line with the error text of the read, never a skip.
pub(crate) fn scan_kit_citations(base: &Base, scope: &KitScope) -> Scan {
    let mut scan = Scan {
        cited: Vec::new(),
        problems: Vec::new(),
    };
    let citation = match citation_shape(&base.schema) {
        Ok(citation) => citation,
        Err(problem) => {
            scan.problems.push(problem);
            return scan;
        }
    };
    let topics: HashSet<&str> = scope.seeded_topics.iter().map(String::as_str).collect();
    for file in &scope.owned_files {
        let text = match fs::read_to_string(base.root.join(file)) {
            Ok(text) => text,
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(error) => {
                scan.problems.push(format!("{file}: {error}"));
                continue;
            }
        };
        for id in cited_ids(&text, &citation, &topics) {
            scan.cited.push(Citation {
                file: file.clone(),
                id,
            });
        }
    }
    scan
}

/// `true` when `pack` ships an entry with `id`.
fn ships(pack: &PackCopy, id: &str) -> bool {
    pack.entries
        .iter()
        .any(|entry| entry.get("id").and_then(Value::as_str) == Some(id))
}

/// The status that an override of `id` sets when that status takes the entry
/// out of the reads, or `None` when no override of the id does. The merged
/// base decides: the id is in `Base::inactive`, and the override replaces its
/// `status` field, so the status that `Base::inactive` holds is the
/// override's.
fn status_an_override_sets<'a>(base: &'a Base, id: &str) -> Option<&'a str> {
    if !base
        .overridden
        .get(id)?
        .iter()
        .any(|field| field == "status")
    {
        return None;
    }
    base.inactive.get(id)?.get("status")?.as_str()
}

/// The finding for a cited id the base does not hold. The text before the
/// first semicolon names the file and the id. `embedded_packs` holds the
/// embedded packs, read once for a pass. `pins` holds the pins of
/// `.houserules.json`.
///
/// The cases, in order (the module doc has the account):
///
/// - The base excludes the id: the project's choice, and the line says how
///   to take it back.
/// - An override sets an inactive status for the id: the line names the
///   status and says how to take it back.
/// - No kit pack ships the id: the file cites it because it holds an older
///   kit's text. If the project has not changed the file, `update` overwrites
///   it with the current text, which cites no such id. If the project has
///   changed it, `update` keeps it and prints `kept <file> (locally
///   modified)`; the file is then the project's own text, so the project
///   removes the citation or lists the file in `ownedPaths`, which makes it no
///   subject of this lint. The shipped template never cites an id that no kit
///   pack ships (`shipped_kit_owned_files_cite_only_shipped_ids`), so a file
///   the current `update` writes does not reach this arm.
/// - A kit pack ships the id, and the install holds no copy under the name of
///   that pack: `pack add` installs the pack.
/// - A kit pack ships the id, and a copy holds its name. The pin of that copy
///   decides which pack the copy is: the source `kit` makes it the installed
///   kit pack, which then does not ship the id as an active entry (an older
///   copy, or the copy's own status). Any other source, or no pin, makes it
///   another pack that holds the name.
fn missing_finding(
    base: &Base,
    embedded_packs: &OnceCell<Vec<PackCopy>>,
    pins: &BTreeMap<String, Pin>,
    file: &str,
    id: &str,
) -> String {
    if base.excluded.contains_key(id) {
        return format!(
            "{file}: cites \"{id}\", which knowledge/overrides.json excludes; remove the \
             exclusion, or replace it with an override that carries your own text"
        );
    }
    if let Some(status) = status_an_override_sets(base, id) {
        return format!(
            "{file}: cites \"{id}\", which knowledge/overrides.json sets to status \"{status}\"; \
             remove that status from the override, or replace the override with one that \
             carries your own text"
        );
    }
    let Some(kit_pack) = embedded_packs
        .get_or_init(kit_packs)
        .iter()
        .find(|pack| ships(pack, id))
    else {
        return format!(
            "{file}: cites \"{id}\", which is not in the knowledge base; the kit does not seed \
             it, so no entry can be restored: run houserules update, and if update prints \"kept \
             {file} (locally modified)\", remove the citation from {file} or list \"{file}\" in \
             \"ownedPaths\" in .houserules.json"
        );
    };
    let pack = &kit_pack.name;
    if !base.packs.iter().any(|copy| copy.name == *pack) {
        return format!(
            "{file}: cites \"{id}\" from the pack \"{pack}\", which is not installed; run \
             houserules pack add {pack}"
        );
    }
    match pins.get(pack) {
        Some(pin) if pin.source == KIT_SOURCE => format!(
            "{file}: cites \"{id}\", which the installed pack \"{pack}\" {0} does not ship as an \
             active entry; run houserules update",
            pin.version
        ),
        other => format!(
            "{file}: cites \"{id}\" from the kit pack \"{pack}\", but the installed pack \
             \"{pack}\" comes from {0}; the kit pack cannot be installed while another pack \
             holds its name",
            other.map_or(UNRECORDED_SOURCE, |pin| pin.source.as_str())
        ),
    }
}

/// The lint's findings, in the order `check-knowledge` prints them: first
/// each input the pass could not read or derive, then one line for each
/// file and id pair where the id is not live (`missing_finding`). A
/// `.houserules.json` that cannot give its pins is one more input the pass
/// could not read: its line says that no citation was checked, and leaves
/// the defect to the pin check, which names it once.
pub(crate) fn kit_citation_findings(base: &Base, scope: &KitScope) -> Vec<String> {
    let Scan {
        cited,
        mut problems,
    } = scan_kit_citations(base, scope);
    let Ok(pins) = read_pins_for_gate(&base.root) else {
        problems.push(UNREADABLE_PINS_LINE.to_string());
        return problems;
    };
    let packs = OnceCell::new();
    let missing = cited
        .iter()
        .filter(|citation| !base.entries.contains_key(&citation.id))
        .map(|citation| missing_finding(base, &packs, &pins, &citation.file, &citation.id));
    problems.into_iter().chain(missing).collect()
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use serde_json::{Value, json};

    use super::super::model::load_base;
    use super::*;
    use crate::packs::pack_sha256;

    /// The seed knowledge schema, the one `template/knowledge/schema.json`
    /// ships: the lint derives its id pattern from the loaded schema, so the
    /// fixtures load the real file.
    fn seed_schema_text() -> String {
        let path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../template/knowledge/schema.json");
        fs::read_to_string(path).expect("read the seed schema")
    }

    /// A schema-valid entry with the given id.
    fn entry(id: &str) -> Value {
        json!({
            "id": id,
            "kind": "rule",
            "area": "global",
            "summary": "A rule.",
            "body": ["Body."],
            "tags": ["test"],
            "source": {"date": "2026-10-01", "by": "user"},
        })
    }

    /// Writes `root/<relative>`, creating its directories.
    fn write(root: &Path, relative: &str, content: &str) {
        let path = root.join(relative);
        fs::create_dir_all(path.parent().expect("a parent")).expect("create directories");
        fs::write(path, content).expect("write file");
    }

    /// A knowledge base under a fresh temp directory: the seed schema, an
    /// empty `areas.json`, and one topic file per `(topic, ids)` pair.
    fn knowledge_root(topics: &[(&str, &[&str])]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("tempdir");
        write(dir.path(), "knowledge/schema.json", &seed_schema_text());
        write(dir.path(), "knowledge/areas.json", "{}");
        for (topic, ids) in topics {
            let entries: Vec<Value> = ids.iter().map(|id| entry(id)).collect();
            let content = json!({"topic": topic, "title": topic, "entries": entries});
            write(
                dir.path(),
                &format!("knowledge/{topic}.json"),
                &content.to_string(),
            );
        }
        dir
    }

    /// A scope over `owned` whose payload seeds the topic `process`.
    fn scope(owned: &[&str]) -> KitScope {
        KitScope {
            owned_files: owned.iter().map(|file| file.to_string()).collect(),
            seeded_topics: vec!["process".to_string()],
        }
    }

    /// The whole finding line for a cited id that the base lacks and no kit
    /// pack ships: the 1.4.0 line for an id the kit does not seed, with
    /// `ownedPaths` in place of `overrides`. Spelled out so a test pins the
    /// contract text. The fixtures cite ids such as `process.gone`, which
    /// no kit pack ships.
    fn unshipped_line(file: &str, id: &str) -> String {
        format!(
            "{file}: cites \"{id}\", which is not in the knowledge base; the kit does not seed it, \
             so no entry can be restored: run houserules update, and if update prints \"kept {file} \
             (locally modified)\", remove the citation from {file} or list \"{file}\" in \
             \"ownedPaths\" in .houserules.json"
        )
    }

    /// The whole finding line for a cited id that the project excludes.
    fn excluded_line(file: &str, id: &str) -> String {
        format!(
            "{file}: cites \"{id}\", which knowledge/overrides.json excludes; remove the \
             exclusion, or replace it with an override that carries your own text"
        )
    }

    /// The whole finding line for a cited id of a kit pack that the install
    /// lacks.
    fn not_installed_line(file: &str, id: &str, pack: &str) -> String {
        format!(
            "{file}: cites \"{id}\" from the pack \"{pack}\", which is not installed; run \
             houserules pack add {pack}"
        )
    }

    /// Copies the kit pack `name` into `knowledge/packs/` of `root`, as
    /// `pack add` leaves it.
    fn install_kit_pack(root: &Path, name: &str) {
        let source = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../template/packs")
            .join(format!("{name}.json"));
        write(
            root,
            &format!("knowledge/packs/{name}.json"),
            &fs::read_to_string(source).expect("read a kit pack"),
        );
    }

    /// The whole finding line for a cited id whose override sets an inactive
    /// status.
    fn override_status_line(file: &str, id: &str, status: &str) -> String {
        format!(
            "{file}: cites \"{id}\", which knowledge/overrides.json sets to status \"{status}\"; \
             remove that status from the override, or replace the override with one that \
             carries your own text"
        )
    }

    /// The whole finding line for a cited id that the installed kit pack
    /// does not ship as an active entry.
    fn inactive_in_copy_line(file: &str, id: &str, pack: &str, version: &str) -> String {
        format!(
            "{file}: cites \"{id}\", which the installed pack \"{pack}\" {version} does not \
             ship as an active entry; run houserules update"
        )
    }

    /// The whole finding line for a cited id of a kit pack whose name
    /// another installed pack holds.
    fn name_held_line(file: &str, id: &str, pack: &str, source: &str) -> String {
        format!(
            "{file}: cites \"{id}\" from the kit pack \"{pack}\", but the installed pack \
             \"{pack}\" comes from {source}; the kit pack cannot be installed while another \
             pack holds its name"
        )
    }

    /// Installs the kit pack `name` as `install_kit_pack` does, with the
    /// parsed copy changed by `edit` first.
    fn install_edited_kit_pack(root: &Path, name: &str, edit: impl FnOnce(&mut Value)) {
        let path = root.join(format!("knowledge/packs/{name}.json"));
        install_kit_pack(root, name);
        let mut copy: Value =
            serde_json::from_str(&fs::read_to_string(&path).expect("read the copy"))
                .expect("parse the copy");
        edit(&mut copy);
        fs::write(&path, copy.to_string()).expect("write the copy");
    }

    /// Writes the copy of an upstream pack `name` that ships each of `ids`,
    /// all under the topic of the pack.
    fn write_pack(root: &Path, name: &str, ids: &[&str]) {
        let entries: Vec<Value> = ids.iter().map(|id| entry(id)).collect();
        write(
            root,
            &format!("knowledge/packs/{name}.json"),
            &json!({
                "name": name, "version": "1.0.0", "schema": "houserules/pack@1.0.0",
                "topics": {name: {"title": "An upstream pack."}},
                "areas": {}, "entries": entries,
            })
            .to_string(),
        );
    }

    /// Writes `.houserules.json` under `root` with the one pin of the
    /// installed copy of `pack`: the pin records `version` and `source`, and
    /// the checksum of the copy as it stands.
    fn pin_installed_copy(root: &Path, pack: &str, version: &str, source: &str) {
        let copy: Value = serde_json::from_str(
            &fs::read_to_string(root.join(format!("knowledge/packs/{pack}.json")))
                .expect("read the copy"),
        )
        .expect("parse the copy");
        let pins = json!({"packs": {pack: {
            "version": version, "source": source,
            "sha256": pack_sha256(&copy), "addedAreas": [],
        }}});
        write(root, ".houserules.json", &pins.to_string());
    }

    /// Writes `knowledge/overrides.json` with the one override of `id`.
    fn override_fields(root: &Path, id: &str, fields: Value) {
        write(
            root,
            "knowledge/overrides.json",
            &json!({
                "schema": "houserules/overrides@1.0.0",
                "overrides": [{"id": id, "fields": fields}],
            })
            .to_string(),
        );
    }

    const FILE: &str = ".claude/agents/branch-reviewer.md";

    #[test]
    fn every_cited_id_that_resolves_yields_no_finding() {
        let root = knowledge_root(&[("process", &["process.kept", "process.also-kept"])]);
        write(
            root.path(),
            FILE,
            "A rule: `process.kept`. Another: `process.also-kept`.\n",
        );
        let base = load_base(root.path()).expect("loads");
        // The control: the pass saw both ids, so the empty findings below
        // say "resolved" and not "nothing was scanned".
        let cited: Vec<String> = scan_kit_citations(&base, &scope(&[FILE]))
            .cited
            .into_iter()
            .map(|citation| citation.id)
            .collect();
        assert_eq!(cited, ["process.kept", "process.also-kept"]);
        assert_eq!(
            kit_citation_findings(&base, &scope(&[FILE])),
            Vec::<String>::new()
        );
    }

    #[test]
    fn a_cited_id_the_base_lacks_is_one_finding_with_the_whole_line() {
        let root = knowledge_root(&[("process", &["process.kept"])]);
        write(
            root.path(),
            FILE,
            "See `process.kept` and `process.gone`.\n",
        );
        let base = load_base(root.path()).expect("loads");
        assert_eq!(
            kit_citation_findings(&base, &scope(&[FILE])),
            vec![unshipped_line(FILE, "process.gone")]
        );
    }

    #[test]
    fn a_token_without_a_topic_prefix_is_not_an_id() {
        let root = knowledge_root(&[("process", &["process.kept"])]);
        write(
            root.path(),
            FILE,
            "Fields `verdict.text`, `source.date`; file `implementer.md`; id `process.gone`.\n",
        );
        let base = load_base(root.path()).expect("loads");
        // The control: the one token with a topic prefix is reported, so the
        // silence of the three others is the prefix rule and not a dead
        // extractor.
        assert_eq!(
            kit_citation_findings(&base, &scope(&[FILE])),
            vec![unshipped_line(FILE, "process.gone")]
        );
    }

    #[test]
    fn an_id_followed_by_punctuation_is_still_checked() {
        let root = knowledge_root(&[("process", &["process.kept"])]);
        write(
            root.path(),
            FILE,
            "Under `process.one`, then (`process.two`); also `process.three`. And `process.four`:\n",
        );
        let base = load_base(root.path()).expect("loads");
        assert_eq!(
            kit_citation_findings(&base, &scope(&[FILE])),
            vec![
                unshipped_line(FILE, "process.one"),
                unshipped_line(FILE, "process.two"),
                unshipped_line(FILE, "process.three"),
                unshipped_line(FILE, "process.four"),
            ]
        );
    }

    #[test]
    fn punctuation_inside_the_backticks_makes_the_token_no_id() {
        let root = knowledge_root(&[("process", &["process.kept"])]);
        write(
            root.path(),
            FILE,
            "Odd: `process.gone,` and `process.gone.`; real: `process.gone`.\n",
        );
        let base = load_base(root.path()).expect("loads");
        // The control: the one clean token is reported, once. A token with
        // punctuation inside the backticks would add a line for its own
        // text, such as "process.gone,".
        assert_eq!(
            kit_citation_findings(&base, &scope(&[FILE])),
            vec![unshipped_line(FILE, "process.gone")]
        );
    }

    #[test]
    fn an_id_after_non_ascii_text_is_still_found() {
        let root = knowledge_root(&[("process", &["process.kept"])]);
        write(
            root.path(),
            FILE,
            "Zeichen: \u{e4}\u{f6}\u{fc} \u{2014} `process.gone`\n",
        );
        let base = load_base(root.path()).expect("loads");
        assert_eq!(
            kit_citation_findings(&base, &scope(&[FILE])),
            vec![unshipped_line(FILE, "process.gone")]
        );
    }

    #[test]
    fn a_deleted_seeded_topic_still_fails_its_cited_ids() {
        // The whole `process` topic file is gone: `process` is no loaded
        // topic, so only the seeded-topic list keeps the prefix valid.
        let root = knowledge_root(&[("quality", &["quality.kept"])]);
        write(root.path(), FILE, "Under `process.evals-rerun`.\n");
        let base = load_base(root.path()).expect("loads");
        let mut seeded = scope(&[FILE]);
        seeded.seeded_topics = vec!["process".to_string(), "quality".to_string()];
        assert_eq!(
            kit_citation_findings(&base, &seeded),
            vec![not_installed_line(FILE, "process.evals-rerun", "process")]
        );
        // The control: without the seeded list the prefix is no topic, and
        // the same file passes. The seeded list is what fires the finding.
        let mut unseeded = scope(&[FILE]);
        unseeded.seeded_topics = Vec::new();
        assert_eq!(
            kit_citation_findings(&base, &unseeded),
            Vec::<String>::new()
        );
    }

    #[test]
    fn a_topic_the_payload_does_not_seed_is_no_id_prefix() {
        // The adopter's topic `source` is loaded, and the payload seeds only
        // `process`. The kit text names a field `source.date`, and the topic
        // does not make that field an id.
        let root = knowledge_root(&[("process", &["process.kept"]), ("source", &["source.note"])]);
        write(
            root.path(),
            FILE,
            "Field `source.date`, entry `source.note`, id `process.gone`.\n",
        );
        let base = load_base(root.path()).expect("loads");
        assert_eq!(
            kit_citation_findings(&base, &scope(&[FILE])),
            vec![unshipped_line(FILE, "process.gone")]
        );
        // The control: when the payload seeds `source`, the same token is an
        // id, and the lint reports it. The seeded list alone decides.
        let mut seeding_source = scope(&[FILE]);
        seeding_source.seeded_topics.push("source".to_string());
        assert_eq!(
            kit_citation_findings(&base, &seeding_source),
            vec![
                unshipped_line(FILE, "source.date"),
                unshipped_line(FILE, "process.gone"),
            ]
        );
    }

    #[test]
    fn a_kit_owned_file_citing_an_id_of_an_adopter_topic_is_not_checked_for_it() {
        // The adopter wrote the topic `team` and changed the kit-owned file
        // to cite `team.gone`, an entry nobody wrote. The lint does not
        // check that citation: the payload seeds no topic `team`.
        let root = knowledge_root(&[("process", &["process.kept"]), ("team", &["team.kept"])]);
        write(
            root.path(),
            FILE,
            "The adopter's `team.gone`, the kit's `process.gone`.\n",
        );
        let base = load_base(root.path()).expect("loads");
        // The firing control is the seeded citation in the same file: the
        // pass ran and the file was checked.
        assert_eq!(
            kit_citation_findings(&base, &scope(&[FILE])),
            vec![unshipped_line(FILE, "process.gone")]
        );
        // The second control: were `team` a seeded topic, the same token
        // would be reported.
        let mut seeding_team = scope(&[FILE]);
        seeding_team.seeded_topics.push("team".to_string());
        assert_eq!(
            kit_citation_findings(&base, &seeding_team),
            vec![
                unshipped_line(FILE, "team.gone"),
                unshipped_line(FILE, "process.gone"),
            ]
        );
    }

    #[test]
    fn an_absent_kit_owned_file_is_skipped() {
        let root = knowledge_root(&[("process", &["process.kept"])]);
        write(root.path(), FILE, "Under `process.gone`.\n");
        let base = load_base(root.path()).expect("loads");
        // The listed file `.claude/skills/absent.md` does not exist. The
        // present file in the same scope still fires, so the one finding
        // proves the absent file was skipped and the pass kept going.
        assert_eq!(
            kit_citation_findings(&base, &scope(&[".claude/skills/absent.md", FILE])),
            vec![unshipped_line(FILE, "process.gone")]
        );
    }

    #[test]
    fn an_archived_entry_does_not_resolve() {
        let root = knowledge_root(&[("process", &["process.kept"])]);
        let archived = json!({
            "topic": "process",
            "title": "process",
            "entries": [entry("process.archived")],
        });
        write(
            root.path(),
            "knowledge/archive/process.json",
            &archived.to_string(),
        );
        write(root.path(), FILE, "Under `process.archived`.\n");
        let base = load_base(root.path()).expect("loads");
        assert_eq!(
            kit_citation_findings(&base, &scope(&[FILE])),
            vec![unshipped_line(FILE, "process.archived")]
        );
    }

    #[test]
    fn one_id_cited_twice_in_one_file_yields_one_finding() {
        let root = knowledge_root(&[("process", &["process.kept"])]);
        write(
            root.path(),
            FILE,
            "First `process.gone`. Again `process.gone`, and `process.kept`.\n",
        );
        write(
            root.path(),
            ".claude/skills/other.md",
            "Also `process.gone`.\n",
        );
        let base = load_base(root.path()).expect("loads");
        // One finding per file and id: two files cite the id, so two lines.
        assert_eq!(
            kit_citation_findings(&base, &scope(&[FILE, ".claude/skills/other.md"])),
            vec![
                unshipped_line(FILE, "process.gone"),
                unshipped_line(".claude/skills/other.md", "process.gone"),
            ]
        );
    }

    #[test]
    fn a_kit_owned_file_that_cannot_be_read_is_a_named_finding() {
        let root = knowledge_root(&[("process", &["process.kept"])]);
        // A directory stands where the file belongs: it exists, and reading
        // it fails on every platform.
        fs::create_dir_all(root.path().join(FILE)).expect("create a directory at the file path");
        let base = load_base(root.path()).expect("loads");
        let error = fs::read_to_string(root.path().join(FILE)).expect_err("a directory is no file");
        assert_eq!(
            kit_citation_findings(&base, &scope(&[FILE])),
            vec![format!("{FILE}: {error}")]
        );
    }

    #[test]
    fn a_schema_without_an_id_pattern_is_a_named_finding() {
        let root = knowledge_root(&[("process", &["process.kept"])]);
        let mut schema: Value = serde_json::from_str(&seed_schema_text()).expect("parse schema");
        schema["$defs"]["entry"]["properties"]["id"]
            .as_object_mut()
            .expect("the id schema is an object")
            .remove("pattern");
        write(root.path(), "knowledge/schema.json", &schema.to_string());
        write(root.path(), FILE, "Under `process.gone`.\n");
        let base = load_base(root.path()).expect("loads");
        assert_eq!(
            kit_citation_findings(&base, &scope(&[FILE])),
            vec![
                "knowledge/schema.json: /$defs/entry/properties/id/pattern is missing; \
                 kit-owned citations not checked"
                    .to_string()
            ]
        );
    }

    #[test]
    fn a_schema_whose_id_pattern_does_not_compile_is_a_named_finding() {
        let root = knowledge_root(&[("process", &["process.kept"])]);
        let mut schema: Value = serde_json::from_str(&seed_schema_text()).expect("parse schema");
        schema["$defs"]["entry"]["properties"]["id"]["pattern"] = json!("^(unclosed");
        write(root.path(), "knowledge/schema.json", &schema.to_string());
        write(root.path(), FILE, "Under `process.gone`.\n");
        let base = load_base(root.path()).expect("loads");
        assert_eq!(
            kit_citation_findings(&base, &scope(&[FILE])),
            vec![
                "knowledge/schema.json: /$defs/entry/properties/id/pattern \"^(unclosed\" does \
                 not compile; kit-owned citations not checked"
                    .to_string()
            ]
        );
    }

    #[test]
    fn an_id_after_a_line_wrapped_code_span_is_still_found() {
        let root = knowledge_root(&[("process", &["process.kept"])]);
        write(
            root.path(),
            FILE,
            "Run `houserules set <id>\nstatus=done` first (`process.gone`).\n",
        );
        let base = load_base(root.path()).expect("loads");
        assert_eq!(
            kit_citation_findings(&base, &scope(&[FILE])),
            vec![unshipped_line(FILE, "process.gone")]
        );
        // The control: the same text on one line gives the same finding.
        write(
            root.path(),
            FILE,
            "Run `houserules set <id> status=done` first (`process.gone`).\n",
        );
        assert_eq!(
            kit_citation_findings(&base, &scope(&[FILE])),
            vec![unshipped_line(FILE, "process.gone")]
        );
    }

    #[test]
    fn a_double_backtick_span_around_an_id_is_found() {
        let root = knowledge_root(&[("process", &["process.kept"])]);
        write(
            root.path(),
            FILE,
            "Use ``process.one`` and ``code with ` inside``, then `process.two`.\n",
        );
        let base = load_base(root.path()).expect("loads");
        assert_eq!(
            kit_citation_findings(&base, &scope(&[FILE])),
            vec![
                unshipped_line(FILE, "process.one"),
                unshipped_line(FILE, "process.two"),
            ]
        );
    }

    #[test]
    fn an_unclosed_double_backtick_does_not_hide_a_later_citation() {
        let root = knowledge_root(&[("process", &["process.kept"])]);
        write(
            root.path(),
            FILE,
            "An unclosed `` run stays literal. See `process.gone`.\n",
        );
        let base = load_base(root.path()).expect("loads");
        assert_eq!(
            kit_citation_findings(&base, &scope(&[FILE])),
            vec![unshipped_line(FILE, "process.gone")]
        );
    }

    /// Writes `text` as the kit-owned file and returns its findings.
    fn findings_for(text: &str) -> Vec<String> {
        let root = knowledge_root(&[("process", &["process.kept"])]);
        write(root.path(), FILE, text);
        let base = load_base(root.path()).expect("loads");
        kit_citation_findings(&base, &scope(&[FILE]))
    }

    #[test]
    fn a_stray_backtick_in_a_paragraph_does_not_hide_a_later_citation() {
        // The control is the same two paragraphs without the stray backtick.
        let expected = vec![unshipped_line(FILE, "process.gone")];
        assert_eq!(
            findings_for("Press the key to open the console.\n\nSee (`process.gone`).\n"),
            expected
        );
        assert_eq!(
            findings_for("Press the ` key to open the console.\n\nSee (`process.gone`).\n"),
            expected
        );
    }

    #[test]
    fn a_stray_backtick_in_a_heading_does_not_hide_a_later_citation() {
        let expected = vec![unshipped_line(FILE, "process.gone")];
        assert_eq!(
            findings_for("# Press the key\n\nSee (`process.gone`).\n"),
            expected
        );
        assert_eq!(
            findings_for("# Press the ` key\n\nSee (`process.gone`).\n"),
            expected
        );
    }

    #[test]
    fn a_stray_backtick_at_the_top_of_the_file_does_not_hide_a_citation_far_below() {
        let filler = "A line of plain prose.\n".repeat(70);
        let expected = vec![unshipped_line(FILE, "process.gone")];
        assert_eq!(
            findings_for(&format!(
                "Press the key.\n\n{filler}\nSee (`process.gone`).\n"
            )),
            expected
        );
        assert_eq!(
            findings_for(&format!(
                "Press the ` key.\n\n{filler}\nSee (`process.gone`).\n"
            )),
            expected
        );
    }

    #[test]
    fn a_stray_backtick_in_a_list_item_does_not_hide_a_later_citation() {
        let expected = vec![unshipped_line(FILE, "process.gone")];
        assert_eq!(findings_for("- one\n- see (`process.gone`)\n"), expected);
        assert_eq!(findings_for("- `one\n- see (`process.gone`)\n"), expected);
    }

    #[test]
    fn an_id_inside_a_longer_code_span_is_not_a_citation() {
        // A command that names an id is one span with more than the id in it.
        // The id in its own span after it is the control that the pass ran.
        assert_eq!(
            findings_for("Run `houserules get process.gone` now, then read `process.one`.\n"),
            vec![unshipped_line(FILE, "process.one")]
        );
    }

    #[test]
    fn a_span_that_pads_the_id_or_wraps_it_across_a_line_break_is_not_extracted() {
        // The lint parses no Markdown, so a citation is a backtick, the id,
        // and a backtick with nothing between them. A span that pads the id
        // with a space or a line ending, or wraps the id itself across a
        // line break, is not one. The id in plain backticks at the end is
        // the control that the pass ran.
        assert_eq!(
            findings_for(
                "A ` process.one ` and `process.\ngone` and `\nprocess.two\n` and `process.three`.\n"
            ),
            vec![unshipped_line(FILE, "process.three")]
        );
    }

    #[test]
    fn an_id_shared_between_two_spans_is_taken_as_a_citation() {
        // `a`process.gone`b` is two spans with plain text between them, and a
        // renderer shows process.gone as text. The pattern sees a backtick,
        // an id, and a backtick, so the lint reports it: a known false
        // positive, which no kit-owned file shows (the module doc says so).
        assert_eq!(
            findings_for("Two spans `a`process.gone`b` abut.\n"),
            vec![unshipped_line(FILE, "process.gone")]
        );
    }

    #[test]
    fn a_shared_backtick_does_not_hide_the_next_citation() {
        // The closing backtick of `process.no-tech-debt` is also the opening
        // backtick of `process.evals-rerun`. A renderer shows the second as
        // a code span and the first as plain text. The lint reads both as
        // citations: the first is the documented false-positive class, and
        // the second must not be lost to it.
        assert_eq!(
            findings_for("See `a`process.no-tech-debt`process.evals-rerun` here.\n"),
            vec![
                not_installed_line(FILE, "process.no-tech-debt", "process"),
                not_installed_line(FILE, "process.evals-rerun", "process"),
            ]
        );
        // The control: the real citation alone.
        assert_eq!(
            findings_for("`a` and `process.evals-rerun`\n"),
            vec![not_installed_line(FILE, "process.evals-rerun", "process")]
        );
    }

    #[test]
    fn an_id_in_its_own_backticks_inside_a_fence_is_a_citation() {
        // The lint parses no Markdown, so a fence changes nothing: an id in
        // its own backticks inside it is a citation, and an id on a line of
        // its own is not.
        assert_eq!(
            findings_for("```sh\nhouserules get `process.gone`\nprocess.one\n```\n"),
            vec![unshipped_line(FILE, "process.gone")]
        );
    }

    /// The module doc names the tests that prove its claims. A backticked
    /// identifier with four or more underscores in that doc is a test name,
    /// and it must be a test function in one of the three files that hold the
    /// lint's tests, so a rename cannot leave the doc pointing at nothing.
    #[test]
    fn every_test_the_module_doc_names_exists() {
        let doc: String = include_str!("kit_citations.rs")
            .lines()
            .take_while(|line| line.starts_with("//!") || line.is_empty())
            .collect::<Vec<_>>()
            .join("\n");
        let sources = [
            include_str!("kit_citations.rs"),
            include_str!("../install/mod.rs"),
            include_str!("../../tests/kit_citations.rs"),
        ];
        let named: Vec<&str> = doc
            .split('`')
            .skip(1)
            .step_by(2)
            .filter(|token| {
                token.bytes().filter(|byte| *byte == b'_').count() >= 4
                    && token.bytes().all(|byte| {
                        byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_'
                    })
            })
            .collect();
        assert!(
            named.len() >= 10,
            "the doc names too few tests to be this guard's subject: {named:?}"
        );
        for name in named {
            let declared = format!("fn {name}(");
            assert!(
                sources.iter().any(|source| source.contains(&declared)),
                "the module doc names `{name}`, which no test file declares"
            );
        }
    }

    #[test]
    fn a_cited_id_that_no_kit_pack_ships_is_one_finding_with_the_ownedpaths_line() {
        let root = knowledge_root(&[("process", &["process.kept"])]);
        write(
            root.path(),
            FILE,
            "Under `process.cited-by-an-older-kit`.\n",
        );
        let base = load_base(root.path()).expect("loads");
        assert_eq!(
            kit_citation_findings(&base, &scope(&[FILE])),
            vec![unshipped_line(FILE, "process.cited-by-an-older-kit")]
        );
    }

    #[test]
    fn a_cited_id_that_the_project_excludes_is_one_finding_with_the_whole_line() {
        let root = knowledge_root(&[("process", &["process.kept"])]);
        install_kit_pack(root.path(), "process");
        write(
            root.path(),
            "knowledge/overrides.json",
            &json!({
                "schema": "houserules/overrides@1.0.0",
                "exclusions": [{"id": "process.evals-rerun", "reason": "not for us"}],
            })
            .to_string(),
        );
        write(root.path(), FILE, "Under `process.evals-rerun`.\n");
        let base = load_base(root.path()).expect("loads");
        assert_eq!(
            kit_citation_findings(&base, &scope(&[FILE])),
            vec![excluded_line(FILE, "process.evals-rerun")]
        );
        // The control: without the exclusion the pack holds the id, and the
        // same file passes.
        fs::remove_file(root.path().join("knowledge/overrides.json")).expect("remove");
        let base = load_base(root.path()).expect("loads");
        assert_eq!(
            kit_citation_findings(&base, &scope(&[FILE])),
            Vec::<String>::new()
        );
    }

    #[test]
    fn a_cited_id_of_a_pack_the_install_lacks_is_one_finding_with_the_whole_line() {
        let root = knowledge_root(&[("process", &["process.kept"])]);
        write(root.path(), FILE, "Under `process.evals-rerun`.\n");
        let base = load_base(root.path()).expect("loads");
        assert_eq!(
            kit_citation_findings(&base, &scope(&[FILE])),
            vec![not_installed_line(FILE, "process.evals-rerun", "process")]
        );
        // The control: once the pack is installed the same file passes, so
        // `pack add` clears the finding.
        install_kit_pack(root.path(), "process");
        let base = load_base(root.path()).expect("loads");
        assert_eq!(
            kit_citation_findings(&base, &scope(&[FILE])),
            Vec::<String>::new()
        );
    }

    /// Every id that a kit pack ships names that pack when the install lacks
    /// it. The ids and packs come from the embedded packs, so a pack that
    /// changes needs no edit here.
    #[test]
    fn every_id_a_kit_pack_ships_names_its_pack_when_the_install_lacks_it() {
        let packs = crate::packs::kit_packs();
        let mut expected = Vec::new();
        let mut text = String::new();
        let mut topics = Vec::new();
        for pack in &packs {
            topics.extend(pack.topics.iter().map(|(topic, _)| topic.clone()));
            for item in &pack.entries {
                let id = item["id"].as_str().expect("an entry id");
                text.push_str(&format!("`{id}` "));
                expected.push(not_installed_line(FILE, id, &pack.name));
            }
        }
        assert!(
            expected.len() >= 58,
            "the kit packs ship {} ids",
            expected.len()
        );
        let root = knowledge_root(&[]);
        write(root.path(), FILE, &text);
        let base = load_base(root.path()).expect("loads");
        let mut everything = scope(&[FILE]);
        everything.seeded_topics = topics;
        assert_eq!(kit_citation_findings(&base, &everything), expected);
    }

    /// An override that sets a retired or superseded status takes the entry
    /// out of the reads. The line names the override file and the status,
    /// and both ways it prints clear the finding: an override without the
    /// status, and an override that carries the project's own text.
    #[test]
    fn an_id_whose_override_sets_an_inactive_status_is_one_finding_with_the_whole_line() {
        for status in ["retired", "superseded"] {
            let root = knowledge_root(&[("process", &["process.kept"])]);
            install_kit_pack(root.path(), "process");
            pin_installed_copy(root.path(), "process", "1.0.0", "kit");
            write(root.path(), FILE, "Under `process.evals-rerun`.\n");
            override_fields(
                root.path(),
                "process.evals-rerun",
                json!({"status": status}),
            );
            let base = load_base(root.path()).expect("loads");
            assert!(
                base.inactive.contains_key("process.evals-rerun"),
                "{status}"
            );
            assert_eq!(
                kit_citation_findings(&base, &scope(&[FILE])),
                vec![override_status_line(FILE, "process.evals-rerun", status)],
                "{status}"
            );
            // The first way: the override without the status.
            override_fields(root.path(), "process.evals-rerun", json!({}));
            let base = load_base(root.path()).expect("loads");
            assert_eq!(
                kit_citation_findings(&base, &scope(&[FILE])),
                Vec::<String>::new(),
                "{status}: the status removed"
            );
            // The second way: an override with the project's own text.
            override_fields(
                root.path(),
                "process.evals-rerun",
                json!({"summary": "The project's own summary."}),
            );
            let base = load_base(root.path()).expect("loads");
            assert_eq!(
                kit_citation_findings(&base, &scope(&[FILE])),
                Vec::<String>::new(),
                "{status}: own text"
            );
        }
    }

    /// The line says "sets to status" only for a status that an override
    /// sets. A status of the copy itself gets the next case's line, even when
    /// an override changes another field of the entry.
    #[test]
    fn an_override_of_another_field_never_gets_the_override_status_line() {
        let root = knowledge_root(&[("process", &["process.kept"])]);
        install_edited_kit_pack(root.path(), "process", |copy| {
            for item in copy["entries"].as_array_mut().expect("entries") {
                if item["id"] == "process.evals-rerun" {
                    item["status"] = json!("retired");
                }
            }
        });
        pin_installed_copy(root.path(), "process", "1.0.0", "kit");
        write(root.path(), FILE, "Under `process.evals-rerun`.\n");
        override_fields(
            root.path(),
            "process.evals-rerun",
            json!({"summary": "The project's own summary."}),
        );
        let base = load_base(root.path()).expect("loads");
        assert_eq!(
            kit_citation_findings(&base, &scope(&[FILE])),
            vec![inactive_in_copy_line(
                FILE,
                "process.evals-rerun",
                "process",
                "1.0.0"
            )]
        );
    }

    /// The installed kit pack is the one whose pin records the source `kit`.
    /// When it does not ship the cited id as an active entry, the line says
    /// so and names the version of the pin. The copy is older than the kit
    /// text and lacks the id, or the copy's own status retires or
    /// supersedes it. A copy of the current kit text passes.
    #[test]
    fn an_id_the_installed_kit_pack_does_not_ship_as_active_is_one_finding_with_the_whole_line() {
        let older = |copy: &mut Value| {
            copy["version"] = json!("0.9.0");
            copy["entries"]
                .as_array_mut()
                .expect("entries")
                .retain(|item| item["id"] != "process.evals-rerun");
        };
        let with_status = |status: &'static str| {
            move |copy: &mut Value| {
                for item in copy["entries"].as_array_mut().expect("entries") {
                    if item["id"] == "process.evals-rerun" {
                        item["status"] = json!(status);
                    }
                }
            }
        };
        type Edit = Box<dyn Fn(&mut Value)>;
        let states: Vec<(&str, Edit, &str)> = vec![
            ("an older copy", Box::new(older), "0.9.0"),
            ("a retired copy", Box::new(with_status("retired")), "1.0.0"),
            (
                "a superseded copy",
                Box::new(with_status("superseded")),
                "1.0.0",
            ),
        ];
        for (state, edit, version) in states {
            let root = knowledge_root(&[("process", &["process.kept"])]);
            install_edited_kit_pack(root.path(), "process", edit);
            pin_installed_copy(root.path(), "process", version, "kit");
            write(root.path(), FILE, "Under `process.evals-rerun`.\n");
            let base = load_base(root.path()).expect("loads");
            assert!(!base.entries.contains_key("process.evals-rerun"), "{state}");
            assert_eq!(
                kit_citation_findings(&base, &scope(&[FILE])),
                vec![inactive_in_copy_line(
                    FILE,
                    "process.evals-rerun",
                    "process",
                    version
                )],
                "{state}"
            );
        }
        // The control: a copy of the current kit text holds the id.
        let root = knowledge_root(&[("process", &["process.kept"])]);
        install_kit_pack(root.path(), "process");
        pin_installed_copy(root.path(), "process", "1.0.0", "kit");
        write(root.path(), FILE, "Under `process.evals-rerun`.\n");
        let base = load_base(root.path()).expect("loads");
        assert_eq!(
            kit_citation_findings(&base, &scope(&[FILE])),
            Vec::<String>::new()
        );
    }

    /// The line prints the version that the pin records, not the version of
    /// the copy: the pin is what `update` moves.
    #[test]
    fn the_version_in_the_inactive_in_copy_line_is_the_pins() {
        let root = knowledge_root(&[("process", &["process.kept"])]);
        install_edited_kit_pack(root.path(), "process", |copy| {
            copy["version"] = json!("0.9.0");
            copy["entries"]
                .as_array_mut()
                .expect("entries")
                .retain(|item| item["id"] != "process.evals-rerun");
        });
        pin_installed_copy(root.path(), "process", "0.8.0", "kit");
        write(root.path(), FILE, "Under `process.evals-rerun`.\n");
        let base = load_base(root.path()).expect("loads");
        assert_eq!(
            kit_citation_findings(&base, &scope(&[FILE])),
            vec![inactive_in_copy_line(
                FILE,
                "process.evals-rerun",
                "process",
                "0.8.0"
            )]
        );
    }

    /// A copy that carries the name of the kit pack is the kit pack only when
    /// its pin records the source `kit`. An upstream pack with that name does
    /// not make the kit text's citations live. The line prints the recorded
    /// source as it stands, for each citing file and id.
    #[test]
    fn an_id_of_a_kit_pack_whose_name_another_pack_holds_is_one_finding_with_the_whole_line() {
        for source in [
            "git+https://example.invalid/org/repo@v1.0.0#process.json",
            "git+ssh://git@example.invalid/org/repo@v1.0.0#packs/process.json",
            "file:vendor/process.json",
        ] {
            let root = knowledge_root(&[]);
            write_pack(root.path(), "process", &["process.upstream-only"]);
            pin_installed_copy(root.path(), "process", "1.0.0", source);
            write(
                root.path(),
                FILE,
                "Under `process.evals-rerun`, `process.upstream-only`, and `process.tdd`.\n",
            );
            write(
                root.path(),
                ".claude/skills/other.md",
                "Also `process.tdd`.\n",
            );
            let base = load_base(root.path()).expect("loads");
            // The control: the id of the upstream pack is live, so only the
            // ids that the base lacks are reported.
            assert!(base.entries.contains_key("process.upstream-only"));
            let both_files = scope(&[FILE, ".claude/skills/other.md"]);
            let expected = vec![
                name_held_line(FILE, "process.evals-rerun", "process", source),
                name_held_line(FILE, "process.tdd", "process", source),
                name_held_line(".claude/skills/other.md", "process.tdd", "process", source),
            ];
            assert_eq!(
                kit_citation_findings(&base, &both_files),
                expected,
                "{source}"
            );
            // A harmless edit of the pin file changes nothing: the `commit`
            // that a pin of a `git+` source also records, and the keys of
            // the file beside `packs`.
            let copy: Value = serde_json::from_str(
                &fs::read_to_string(root.path().join("knowledge/packs/process.json"))
                    .expect("read the copy"),
            )
            .expect("parse the copy");
            write(
                root.path(),
                ".houserules.json",
                &json!({"idPrefix": "WI", "packs": {"process": {
                    "commit": "0123456789abcdef0123456789abcdef01234567",
                    "version": "1.0.0", "source": source,
                    "sha256": pack_sha256(&copy), "addedAreas": [],
                }}})
                .to_string(),
            );
            assert_eq!(
                kit_citation_findings(&base, &both_files),
                expected,
                "{source}: a harmless edit of the pin file"
            );
        }
    }

    /// A copy without a pin has no recorded source, so it is not the kit
    /// pack. The pin check reports the missing pin on its own.
    #[test]
    fn a_copy_without_a_pin_is_not_the_kit_pack_and_has_no_recorded_source() {
        let root = knowledge_root(&[]);
        write_pack(root.path(), "process", &["process.upstream-only"]);
        write(root.path(), FILE, "Under `process.evals-rerun`.\n");
        let base = load_base(root.path()).expect("loads");
        assert_eq!(
            kit_citation_findings(&base, &scope(&[FILE])),
            vec![name_held_line(
                FILE,
                "process.evals-rerun",
                "process",
                "an unrecorded source"
            )]
        );
    }

    /// The pins decide, so a pin file that cannot give them is one finding,
    /// and no citation is checked. The finding states that consequence and
    /// leaves the defect to the pin check (`check_packs::pin_findings`).
    #[test]
    fn a_pin_file_that_cannot_give_its_pins_is_a_named_finding() {
        let root = knowledge_root(&[("process", &["process.kept"])]);
        write(root.path(), FILE, "Under `process.gone`.\n");
        write(
            root.path(),
            ".houserules.json",
            &json!({"packs": {"process": {"sha256": "abc"}}}).to_string(),
        );
        let base = load_base(root.path()).expect("loads");
        assert_eq!(
            kit_citation_findings(&base, &scope(&[FILE])),
            vec![
                ".houserules.json: the pins cannot be read, so kit-owned citations are not checked"
                    .to_string()
            ]
        );
    }

    /// An id that an installed pack holds is live, as a local id is.
    #[test]
    fn a_cited_id_that_an_installed_pack_holds_yields_no_finding() {
        let root = knowledge_root(&[("process", &["process.kept"])]);
        install_kit_pack(root.path(), "process");
        write(
            root.path(),
            FILE,
            "Under `process.evals-rerun` and `process.kept`.\n",
        );
        let base = load_base(root.path()).expect("loads");
        assert!(base.entries.contains_key("process.evals-rerun"));
        assert_eq!(
            kit_citation_findings(&base, &scope(&[FILE])),
            Vec::<String>::new()
        );
    }
}
