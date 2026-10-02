//! The kit-owned citation lint of `check-knowledge`: every id of a seeded
//! topic that a kit-owned file cites must be an entry of the loaded knowledge
//! base.
//!
//! Seeded knowledge belongs to the adopter. An adopter may delete a seeded
//! entry, and `update` respects the deletion (`baseline::Status::Deleted`).
//! The kit-owned skills and agent templates cite seeded ids. `update` treats
//! each kit-owned file by what the adopter did to it (`install`'s module doc,
//! "`update` and the ownership baseline"): it overwrites a file the adopter
//! has not changed, keeps a locally modified file and reports it
//! (`kept <path> (locally modified)`), and leaves a file listed in
//! `overrides` as found. So a citation dangles when the adopter deletes the
//! entry, and it also dangles in a kept file whose older text cites an id the
//! payload no longer seeds. This lint turns the dangling citation into a
//! finding. The remedy it prints depends on whether the payload seeds the id
//! (`missing_finding`).
//!
//! The lint is a function of a loaded [`Base`] and a [`KitScope`]. `rules`
//! never imports `install`: the crate root passes `install::kit_citation_scope`
//! to [`super::cmd_check_knowledge`], which hands the scope to the lint.
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
//!    and is not an id. The seeded topics keep the prefix valid after an
//!    adopter deletes a whole topic file.
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
//!   (`an_absent_kit_owned_file_is_skipped`), or the adopter's `overrides`
//!   list it, which `install::kit_citation_scope` decides (the install test
//!   `kit_citation_scope_leaves_out_the_kit_owned_files_the_stamp_overrides`
//!   and the command test `an_overridden_kit_owned_file_is_skipped`).
//! - The base holds the id (`every_cited_id_that_resolves_yields_no_finding`).
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
//! An id is live when `Base::entries` holds it: `load_base` indexes the
//! entries of the top-level `knowledge/<topic>.json` files and nothing else.
//! An entry that `houserules archive` moved to `knowledge/archive/` is not
//! live here. It renders into no rules file and joins no audit package
//! (`render_all` and `audit` read `Base::entries`), although `get` and the
//! `see` check still resolve it (`crate::archive`, `check::check_archive`).

use std::collections::HashSet;
use std::fs;
use std::io;

use regress::Regex;
use serde_json::Value;

use super::model::Base;

/// Where the entry id pattern sits in the knowledge schema.
const ID_PATTERN_POINTER: &str = "/$defs/entry/properties/id/pattern";

/// What the lint needs from the install layer. `install::kit_citation_scope`
/// builds it; the tests build it by hand.
pub(crate) struct KitScope {
    /// The kit-owned files to check, as root-relative paths with forward
    /// slashes: the kit-owned paths minus the ones the adopter's `overrides`
    /// list. The install layer decides which files are the adopter's own;
    /// the lint checks every file listed here that exists.
    pub owned_files: Vec<String>,
    /// The topics whose files the kit seeds: the only prefixes that make a
    /// backticked token an id. A topic the adopter adds does not count.
    pub seeded_topics: Vec<String>,
    /// The id of every entry the payload seeds.
    pub seeded_ids: Vec<String>,
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

/// The finding for a cited id the base does not hold. The text before the
/// first semicolon names the file and the id. The remedy has one arm for each
/// kind of id, and each way it names clears the finding when followed as
/// printed.
///
/// A seeded id (`seeded`: the payload seeds it) names an entry the adopter
/// deleted or archived, and the arm names two ways. The first restores the
/// entry with the shipped binary alone. `init` keeps an existing topic file,
/// so only `update` writes an entry back. `update` respects a deletion while
/// `.houserules.json` records the entry's baseline, and it leaves an id or a
/// whole topic file listed in `overrides` as found, so the first way clears
/// those records and then runs `update`. It holds whether the citing file is
/// unmodified or kept as locally modified, because the entry returns either
/// way. It holds for an install with no stamp too: there is no record to
/// clear, and `update` writes the entry and a stamp. The second way keeps
/// the adopter's decision: it lists the citing file in `overrides`, which
/// makes the file no subject of this lint. An install with no stamp has no
/// `overrides` yet, so the adopter creates `.houserules.json` with the list.
/// Each file that cites the id has its own finding and so its own listing.
/// A deleted topic file is cited by more files than one entry is, so more
/// findings name a file. Deleting the topic file also leaves the `see` links
/// of other topics dangling, and `check-knowledge` reports those apart from
/// this lint: the second way clears the citation findings and leaves them,
/// and the first way clears both
/// (`listing_the_files_the_findings_name_clears_the_citations_of_a_deleted_topic_file_only`).
///
/// For an archived entry the first way leaves the id in two places: active
/// in `knowledge/<topic>.json`, where `update` writes it back, and retired in
/// `knowledge/archive/<topic>.json`, which `update` does not touch. A later
/// `check-knowledge` and a later `archive` sweep report nothing about the
/// second copy, and `get` returns the active entry. The second way leaves
/// the entry archived.
/// `the_printed_remedy_restores_an_archived_entry_and_leaves_its_archive_copy`
/// and `listing_the_citing_files_keeps_an_archived_entry_archived` pin the
/// two states.
///
/// An unseeded id names no entry `update` can write. The file cites it
/// because it holds an older kit's text. If the adopter has not changed the
/// file, `update` overwrites it with the current text, which cites no such
/// id. If the adopter has changed it, `update` keeps it and prints `kept
/// <file> (locally modified)`; the file is then the adopter's own text, so
/// the adopter removes the citation or lists the file in `overrides`, which
/// makes it no subject of this lint. The shipped template never cites an
/// unseeded id (`shipped_kit_owned_files_cite_only_shipped_ids`), so a file
/// the current `update` writes does not reach this arm.
fn missing_finding(file: &str, id: &str, seeded: bool) -> String {
    if !seeded {
        return format!(
            "{file}: cites \"{id}\", which is not in the knowledge base; the kit does not seed it, \
             so no entry can be restored: run houserules update, and if update prints \"kept \
             {file} (locally modified)\", remove the citation from {file} or list \"{file}\" in \
             \"overrides\" in .houserules.json"
        );
    }
    let topic = topic_of(id);
    format!(
        "{file}: cites \"{id}\", which is not in the knowledge base; restore it: in \
         .houserules.json, delete \"{id}\" from \"baselines\" and, if listed, \"{id}\" and \
         \"knowledge/{topic}.json\" from \"overrides\", then run houserules update; or, to keep \
         the entry deleted or archived, list \"{file}\" in \"overrides\" in .houserules.json"
    )
}

/// The lint's findings, in the order `check-knowledge` prints them: first
/// each input the pass could not read or derive, then one line for each
/// file and id pair where the id is not live.
pub(crate) fn kit_citation_findings(base: &Base, scope: &KitScope) -> Vec<String> {
    let Scan { cited, problems } = scan_kit_citations(base, scope);
    let missing = cited
        .iter()
        .filter(|citation| !base.entries.contains_key(&citation.id))
        .map(|citation| {
            let seeded = scope.seeded_ids.contains(&citation.id);
            missing_finding(&citation.file, &citation.id, seeded)
        });
    problems.into_iter().chain(missing).collect()
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use serde_json::{Value, json};

    use super::super::model::load_base;
    use super::*;

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

    /// The ids the fixtures cite as missing and the scope below treats as
    /// seeded. `process.unseeded` is deliberately absent from this list.
    const SEEDED_FIXTURE_IDS: [&str; 8] = [
        "process.gone",
        "process.one",
        "process.two",
        "process.three",
        "process.four",
        "process.archived",
        "process.evals-rerun",
        "process.no-tech-debt",
    ];

    /// A scope over `owned` whose payload seeds the topic `process` and the
    /// ids of `SEEDED_FIXTURE_IDS`.
    fn scope(owned: &[&str]) -> KitScope {
        KitScope {
            owned_files: owned.iter().map(|file| file.to_string()).collect(),
            seeded_topics: vec!["process".to_string()],
            seeded_ids: SEEDED_FIXTURE_IDS.iter().map(|id| id.to_string()).collect(),
        }
    }

    /// The whole finding line for a file and an id of topic `process`,
    /// spelled out so a test pins the contract text.
    fn missing_line(file: &str, id: &str) -> String {
        format!(
            "{file}: cites \"{id}\", which is not in the knowledge base; restore it: in \
             .houserules.json, delete \"{id}\" from \"baselines\" and, if listed, \"{id}\" and \
             \"knowledge/process.json\" from \"overrides\", then run houserules update; or, to \
             keep the entry deleted or archived, list \"{file}\" in \"overrides\" in \
             .houserules.json"
        )
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
            vec![missing_line(FILE, "process.gone")]
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
            vec![missing_line(FILE, "process.gone")]
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
                missing_line(FILE, "process.one"),
                missing_line(FILE, "process.two"),
                missing_line(FILE, "process.three"),
                missing_line(FILE, "process.four"),
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
            vec![missing_line(FILE, "process.gone")]
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
            vec![missing_line(FILE, "process.gone")]
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
            vec![missing_line(FILE, "process.evals-rerun")]
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
            vec![missing_line(FILE, "process.gone")]
        );
        // The control: when the payload seeds `source`, the same token is an
        // id, and the lint reports it. The seeded list alone decides.
        let mut seeding_source = scope(&[FILE]);
        seeding_source.seeded_topics.push("source".to_string());
        assert_eq!(
            kit_citation_findings(&base, &seeding_source),
            vec![
                unseeded_line(FILE, "source.date"),
                missing_line(FILE, "process.gone"),
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
            vec![missing_line(FILE, "process.gone")]
        );
        // The second control: were `team` a seeded topic, the same token
        // would be reported.
        let mut seeding_team = scope(&[FILE]);
        seeding_team.seeded_topics.push("team".to_string());
        assert_eq!(
            kit_citation_findings(&base, &seeding_team),
            vec![
                unseeded_line(FILE, "team.gone"),
                missing_line(FILE, "process.gone"),
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
            vec![missing_line(FILE, "process.gone")]
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
            vec![missing_line(FILE, "process.archived")]
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
                missing_line(FILE, "process.gone"),
                missing_line(".claude/skills/other.md", "process.gone"),
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

    /// The unseeded counterpart of `missing_line`: the whole finding line for
    /// an id the payload never seeded.
    fn unseeded_line(file: &str, id: &str) -> String {
        format!(
            "{file}: cites \"{id}\", which is not in the knowledge base; the kit does not seed it, \
             so no entry can be restored: run houserules update, and if update prints \"kept {file} \
             (locally modified)\", remove the citation from {file} or list \"{file}\" in \
             \"overrides\" in .houserules.json"
        )
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
            vec![missing_line(FILE, "process.gone")]
        );
        // The control: the same text on one line gives the same finding.
        write(
            root.path(),
            FILE,
            "Run `houserules set <id> status=done` first (`process.gone`).\n",
        );
        assert_eq!(
            kit_citation_findings(&base, &scope(&[FILE])),
            vec![missing_line(FILE, "process.gone")]
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
                missing_line(FILE, "process.one"),
                missing_line(FILE, "process.two"),
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
            vec![missing_line(FILE, "process.gone")]
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
        let expected = vec![missing_line(FILE, "process.gone")];
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
        let expected = vec![missing_line(FILE, "process.gone")];
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
        let expected = vec![missing_line(FILE, "process.gone")];
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
        let expected = vec![missing_line(FILE, "process.gone")];
        assert_eq!(findings_for("- one\n- see (`process.gone`)\n"), expected);
        assert_eq!(findings_for("- `one\n- see (`process.gone`)\n"), expected);
    }

    #[test]
    fn an_id_inside_a_longer_code_span_is_not_a_citation() {
        // A command that names an id is one span with more than the id in it.
        // The id in its own span after it is the control that the pass ran.
        assert_eq!(
            findings_for("Run `houserules get process.gone` now, then read `process.one`.\n"),
            vec![missing_line(FILE, "process.one")]
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
            vec![missing_line(FILE, "process.three")]
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
            vec![missing_line(FILE, "process.gone")]
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
                missing_line(FILE, "process.no-tech-debt"),
                missing_line(FILE, "process.evals-rerun"),
            ]
        );
        // The control: the real citation alone.
        assert_eq!(
            findings_for("`a` and `process.evals-rerun`\n"),
            vec![missing_line(FILE, "process.evals-rerun")]
        );
    }

    #[test]
    fn an_id_in_its_own_backticks_inside_a_fence_is_a_citation() {
        // The lint parses no Markdown, so a fence changes nothing: an id in
        // its own backticks inside it is a citation, and an id on a line of
        // its own is not.
        assert_eq!(
            findings_for("```sh\nhouserules get `process.gone`\nprocess.one\n```\n"),
            vec![missing_line(FILE, "process.gone")]
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
            include_str!("../install.rs"),
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
    fn an_unseeded_missing_id_prints_the_update_or_edit_remedy() {
        let root = knowledge_root(&[("process", &["process.kept"])]);
        write(
            root.path(),
            FILE,
            "Seeded `process.gone`, unseeded `process.unseeded`.\n",
        );
        let base = load_base(root.path()).expect("loads");
        assert_eq!(
            kit_citation_findings(&base, &scope(&[FILE])),
            vec![
                missing_line(FILE, "process.gone"),
                unseeded_line(FILE, "process.unseeded"),
            ]
        );
    }
}
