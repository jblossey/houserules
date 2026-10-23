//! The hand steps of `update` (spec section 5): the work that `update` cannot
//! do for the project and tells it to do.
//!
//! Every hand step that `update` can print is one function of this module, so
//! the documents that list them have one place to read. Each function returns
//! the whole text; the caller prints it after `hand step: `. The first
//! sentence names the cause or the file, and the rest says what to do.

use crate::pack::source::PackSource;

/// The prefix of a printed hand step.
pub(crate) const PREFIX: &str = "hand step: ";

/// Step 2 of the conversion steps of batch 34 (spec 4.7): the file at a path of
/// the workflow `workflow` is not the kit's, so the workflow leaves it to the
/// project. `update` and `init --workflow` print it once for the path.
pub(crate) fn not_the_kits(workflow: &str, path: &str) -> String {
    format!("{path} is not the kit's; the {workflow} workflow leaves it to the project")
}

/// Step 2 (spec 4.7) and `check-knowledge` (spec 4.3): the project excludes the
/// rule `id`, which the workflow `workflow` requires.
pub(crate) fn workflow_excludes_a_required_id(workflow: &str, id: &str) -> String {
    format!(
        "workflow {workflow} requires {id}, which knowledge/overrides.json excludes; remove the exclusion, and override the entry to change it"
    )
}

/// Step 2 (spec 4.7): the install adopted the reference workflow and has no
/// tracker.
pub(super) fn workflow_needs_a_tracker() -> String {
    "the reference workflow and the process pack need a tracker; run houserules init --tracker json|github|beads".to_string()
}

/// Step 3 (spec 4.7): the session-start script of the claude renderer is the
/// project's and still holds the ritual line that the reference workflow moved
/// to its own script.
pub(super) fn ritual_line_stays_in_the_session_script() -> String {
    "tools/claude-session-start.sh holds the reference workflow's ritual line; move it to tools/reference-session-start.sh".to_string()
}

/// Step 4 (spec 4.7): the schema at `path` equals no content that houserules
/// shipped, so `update` kept it.
pub(super) fn schema_kept(path: &str) -> String {
    format!(
        "{path} differs from every content of it that houserules shipped, so houserules update kept it; compare it with the file that houserules init writes, and merge the rest by hand"
    )
}

/// The seeded CI workflow (spec 8.5): the file at `path` is the project's, its
/// gates job holds no `permissions` key, and it lacks both `contents: read` and
/// `issues: read`, the permissions that the GitHub tracker needs. The step adds
/// the whole block.
pub(super) fn ci_lacks_the_github_permissions(path: &str) -> String {
    format!(
        "{path} lacks the permissions that the github tracker needs; add permissions: {{contents: read, issues: read}} to the gates job"
    )
}

/// The seeded CI workflow (spec 8.5): the file at `path` is the project's, it
/// holds `contents: read`, and it lacks `issues: read`, which the GitHub tracker
/// needs. The step adds the line to the permissions of the copy, so that the job
/// never holds two `permissions` keys.
pub(super) fn ci_lacks_issues_read(path: &str) -> String {
    format!(
        "{path} lacks issues: read, which the github tracker needs; add issues: read to the permissions of the gates job"
    )
}

/// The seeded CI workflow (spec 8.5, design.md 5.226): the file at `path` is the
/// project's, its gates job holds a `permissions` mapping, and the mapping lacks
/// `contents: read`; or the copy uses the GitHub tracker, holds `issues: read`,
/// and lacks `contents: read`. The step adds the line to the permissions of the
/// copy, so that the job never holds two `permissions` keys. It is the step of
/// every form.
pub(super) fn ci_lacks_contents_read_in_the_permissions(path: &str) -> String {
    format!("{path} lacks contents: read; add contents: read to the permissions of the gates job")
}

/// The seeded CI workflow (spec 8.5, design.md 5.226): the file at `path` is the
/// project's, its gates job holds a `permissions` mapping, and the mapping holds
/// neither `contents: read` nor `issues: read`, which the GitHub tracker needs.
/// The step adds both lines to the permissions of the copy.
pub(super) fn ci_lacks_the_github_scopes_in_the_permissions(path: &str) -> String {
    format!(
        "{path} lacks the permissions that the github tracker needs; add contents: read and issues: read to the permissions of the gates job"
    )
}

/// The seeded CI workflow (spec 8.5, design.md 5.226): the file at `path` is the
/// project's, and its gates job holds `permissions: write-all`. The step replaces
/// the scalar with the block of the form: `{contents: read}`, and
/// `{contents: read, issues: read}` when `github` is set for the GitHub tracker.
pub(super) fn ci_grants_write_all(path: &str, github: bool) -> String {
    let scopes = if github {
        "contents: read, issues: read"
    } else {
        "contents: read"
    };
    format!(
        "{path} grants write-all; replace permissions: write-all with permissions: {{{scopes}}} in the gates job"
    )
}

/// The seeded CI workflow (spec 8.5): the file at `path` is the project's, and
/// it lacks the `GH_TOKEN` env that the steps of the GitHub tracker need, because
/// `gh` takes its token from it.
pub(super) fn ci_lacks_the_github_env(path: &str) -> String {
    format!(
        "{path} lacks the GH_TOKEN env that the github tracker needs; add env: {{GH_TOKEN: ${{{{ github.token }}}}}} to each step that runs houserules check-backlog or houserules audit"
    )
}

/// The seeded CI workflow (spec 4.8): the file at `path` is the project's, and
/// it lacks the steps that the Beads tracker needs. `bd_version` is the release
/// of `bd` that the kit pins.
pub(super) fn ci_lacks_the_beads_steps(path: &str, bd_version: &str) -> String {
    format!(
        "{path} lacks the steps that the beads tracker needs; add a step that installs bd {bd_version} from its GitHub release, and a step that runs bd bootstrap --yes, before the step that runs houserules check-backlog"
    )
}

/// The seeded CI workflow (batch 35, spec 3.2): the file at `path` is the
/// project's, and its checkout step lacks the `ref` input that makes every step
/// of the job judge the head of a pull request.
pub(super) fn ci_lacks_the_head_checkout(path: &str) -> String {
    format!(
        "{path} lacks the PR-head checkout; add ref: ${{{{ github.event.pull_request.head.sha }}}} under with: of its actions/checkout step"
    )
}

/// The seeded CI workflow (spec 8.5): the file at `path` is the project's, its
/// gates job holds no `permissions` key, and it lacks `contents: read`, the
/// read-only token of the gates job. The GitHub tracker gets no such step: its
/// steps for the permissions add `contents: read`.
pub(super) fn ci_lacks_contents_read(path: &str) -> String {
    format!("{path} lacks contents: read; add permissions: {{contents: read}} to the gates job")
}

/// The seeded CI workflow (spec 8.5): the file at `path` is the project's, and
/// its checkout step keeps the persisted credential. The Beads tracker gets no
/// such step: `bd bootstrap --yes` needs the credential (spec 8.5, C11).
pub(super) fn ci_lacks_persisted_credentials_off(path: &str) -> String {
    format!(
        "{path} lacks persist-credentials: false; add it under with: of its actions/checkout step"
    )
}

/// The seeded CI workflow (spec 8.5): the file at `path` is the project's, it
/// uses the Beads tracker, and its checkout step drops the persisted credential
/// with `persist-credentials: false`. `bd bootstrap --yes` fetches from the
/// remote after the checkout and needs the credential (spec 8.5, C11).
pub(super) fn ci_holds_persist_credentials_false(path: &str) -> String {
    format!(
        "{path} holds persist-credentials: false, which the beads tracker cannot use; remove it from with: of its actions/checkout step, so that bd bootstrap can fetch from the remote"
    )
}

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
    /// each line that starts with `pub(super) fn ` or `pub(crate) fn `.
    fn function_names() -> BTreeSet<String> {
        include_str!("hand_steps.rs")
            .lines()
            .filter_map(|line| {
                line.strip_prefix("pub(super) fn ")
                    .or_else(|| line.strip_prefix("pub(crate) fn "))
            })
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
            ("not_the_kits", not_the_kits("reference", "<file>")),
            (
                "workflow_excludes_a_required_id",
                workflow_excludes_a_required_id("reference", "<id>"),
            ),
            ("workflow_needs_a_tracker", workflow_needs_a_tracker()),
            (
                "ritual_line_stays_in_the_session_script",
                ritual_line_stays_in_the_session_script(),
            ),
            ("schema_kept", schema_kept("<path>")),
            (
                "ci_lacks_the_github_permissions",
                ci_lacks_the_github_permissions("<path>"),
            ),
            ("ci_lacks_issues_read", ci_lacks_issues_read("<path>")),
            (
                "ci_lacks_contents_read_in_the_permissions",
                ci_lacks_contents_read_in_the_permissions("<path>"),
            ),
            (
                "ci_lacks_the_github_scopes_in_the_permissions",
                ci_lacks_the_github_scopes_in_the_permissions("<path>"),
            ),
            ("ci_grants_write_all", ci_grants_write_all("<path>", false)),
            ("ci_grants_write_all", ci_grants_write_all("<path>", true)),
            ("ci_lacks_the_github_env", ci_lacks_the_github_env("<path>")),
            (
                "ci_lacks_the_beads_steps",
                ci_lacks_the_beads_steps("<path>", "<bd-version>"),
            ),
            (
                "ci_lacks_the_head_checkout",
                ci_lacks_the_head_checkout("<path>"),
            ),
            ("ci_lacks_contents_read", ci_lacks_contents_read("<path>")),
            (
                "ci_lacks_persisted_credentials_off",
                ci_lacks_persisted_credentials_off("<path>"),
            ),
            (
                "ci_holds_persist_credentials_false",
                ci_holds_persist_credentials_false("<path>"),
            ),
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
