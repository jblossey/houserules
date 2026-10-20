//! The hand steps of `update` (spec section 5): the work that `update` cannot
//! do for the project and tells it to do.
//!
//! Every hand step that `update` can print is one function of this module, so
//! the documents that list them have one place to read. Each function returns
//! the whole text; the caller prints it after `hand step: `. The first
//! sentence names the cause or the file, and the rest says what to do.

use crate::pack::source::PackSource;

/// The prefix of a printed hand step.
pub(super) const PREFIX: &str = "hand step: ";

/// Step 0: a 1.x topic file `knowledge/overrides.json` holds the reserved
/// name of the file that 2.0 reads as the overrides of the project. The
/// conversion stops before it changes a file.
pub(super) fn reserved_topic() -> String {
    "knowledge/overrides.json is a topic file of houserules 1.x, and houserules 2.0 reserves the name overrides for its own file; move its entries to a topic file with another name, then run houserules update again".to_string()
}

/// Step 7: the `CLAUDE.md` of the install does not import `AGENTS.md`.
pub(super) fn claude_md_import() -> String {
    "add @AGENTS.md to CLAUDE.md: without the import, the standing rules no longer reach Claude Code"
        .to_string()
}

/// Step 8: the knowledge schema is in `ownedPaths` and still holds the area
/// enum, so `update` does not rewrite it. `area_property` is the text of the
/// property that replaces the `$ref` of the area property.
pub(super) fn schema_owned_with_area_enum(area_property: &str) -> String {
    format!(
        "remove $defs.area from knowledge/schema.json; your areas live in knowledge/areas.json. Then replace the $ref of its area property with {area_property}"
    )
}

/// Step 8: the knowledge schema holds changes beyond the area enum. `update`
/// removed `$defs.area` and the `$ref` of the area property and nothing else.
pub(super) fn schema_with_other_changes() -> String {
    "knowledge/schema.json holds changes beyond the area enum, so houserules update removed $defs.area and the $ref of its area property and nothing else; compare the file with the schema that houserules init writes, and merge the rest by hand".to_string()
}

/// Step 8: the knowledge schema has no area enum to remove and differs from
/// the schema of the kit, so `update` kept it.
pub(super) fn schema_differs() -> String {
    "knowledge/schema.json differs from the schema that houserules init writes and holds no area enum to remove, so houserules update kept it; compare the two files, and merge the rest by hand".to_string()
}

/// Step 8: the knowledge schema is no JSON object, so `update` kept it.
pub(super) fn schema_unreadable() -> String {
    "knowledge/schema.json is no JSON object, so houserules update kept it; replace it with the schema that houserules init writes".to_string()
}

/// Step 8: the CI workflow at `path` differs from every workflow that
/// houserules 1.x released, so `update` kept it.
pub(super) fn workflow_differs(path: &str) -> String {
    format!(
        "{path} differs from every workflow that houserules 1.x released, so houserules update kept it; compare it with the workflow that houserules init writes, and merge the rest by hand"
    )
}

/// Pack moves: the new version of the kit pack `kit` ships `id`, which the
/// installed pack `other` ships too, so `update` kept `kit` at `old`.
/// `other_source` is the source that the pin of `other` records, when it
/// reads as one. The command that drops the id from `other` depends on it: a
/// git source takes a tag, a file source takes none and is copied again after
/// the project edits it, and a pin with no upstream source has no such
/// command.
pub(super) fn kit_pack_shares_an_id(
    kit: &str,
    new: &str,
    old: &str,
    id: &str,
    other: &str,
    other_source: Option<&PackSource>,
) -> String {
    let drop_the_id = match other_source {
        Some(PackSource::Git { .. }) => format!(
            "move {other} to a version that drops the id with houserules pack upgrade {other} <tag>, or "
        ),
        Some(PackSource::File(path)) => {
            format!("edit {path} to drop the id and run houserules pack upgrade {other}, or ")
        }
        Some(PackSource::Kit) | None => String::new(),
    };
    format!(
        "pack {kit} {new} ships {id}, which the installed pack {other} ships too, so houserules update kept {kit} at {old}; {drop_the_id}run houserules pack remove {other}, then run houserules update again"
    )
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::fs;
    use std::path::Path;

    use super::*;

    /// The upgrade guide, which shows every printed hand step.
    const GUIDE: &str = "../../docs/upgrading-to-2.md";

    /// The names of the functions of this module, read from its own source:
    /// each line that starts with `pub(super) fn `.
    fn function_names() -> BTreeSet<String> {
        include_str!("hand_steps.rs")
            .lines()
            .filter_map(|line| line.strip_prefix("pub(super) fn "))
            .map(|rest| rest.split('(').next().expect("a name").to_string())
            .collect()
    }

    /// Every text that `update` can print after `hand step: `, with the
    /// function that returns it. A parameter is the placeholder that the
    /// guide shows in its place. A function whose text depends on the source
    /// of the other pack has one case for each source.
    fn printed_texts() -> Vec<(&'static str, String)> {
        let git = PackSource::Git {
            url: "<url>".to_string(),
            tag: "<tag>".to_string(),
            path: "<path>".to_string(),
        };
        let file = PackSource::File("<path>".to_string());
        let shared = |source: Option<&PackSource>| {
            kit_pack_shares_an_id("<kit>", "<new>", "<old>", "<id>", "<other>", source)
        };
        vec![
            ("reserved_topic", reserved_topic()),
            ("claude_md_import", claude_md_import()),
            (
                "schema_owned_with_area_enum",
                schema_owned_with_area_enum("<area-property>"),
            ),
            ("schema_with_other_changes", schema_with_other_changes()),
            ("schema_differs", schema_differs()),
            ("schema_unreadable", schema_unreadable()),
            ("workflow_differs", workflow_differs("<path>")),
            ("kit_pack_shares_an_id", shared(Some(&git))),
            ("kit_pack_shares_an_id", shared(Some(&file))),
            ("kit_pack_shares_an_id", shared(Some(&PackSource::Kit))),
            ("kit_pack_shares_an_id", shared(None)),
        ]
    }

    /// A function with no case here would escape the guide check below.
    #[test]
    fn every_function_has_a_printed_text() {
        let cased: BTreeSet<String> = printed_texts()
            .into_iter()
            .map(|(name, _)| name.to_string())
            .collect();

        assert_eq!(
            function_names(),
            cased,
            "give each hand step a case in printed_texts and show its text in docs/upgrading-to-2.md"
        );
    }

    /// The guide shows each hand step as `update` prints it, so a project
    /// finds the line it reads and the work it asks for.
    #[test]
    fn the_upgrade_guide_shows_every_hand_step() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(GUIDE);
        let guide =
            fs::read_to_string(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));

        let missing: Vec<String> = printed_texts()
            .into_iter()
            .map(|(_, text)| format!("{PREFIX}{text}"))
            .filter(|line| !guide.contains(line))
            .collect();

        assert!(
            missing.is_empty(),
            "docs/upgrading-to-2.md lacks these hand steps:\n{}",
            missing.join("\n")
        );
    }
}
