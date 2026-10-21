//! The exact lines that the tracker contract prints (spec sections 3.3,
//! 3.4, 3.5, 3.8, and 3.10). Each line is a contract surface with its own
//! test: a change here changes the text that a project reads. A line that
//! more than one adapter prints has its one definition here.

/// Line A: no tracker is installed, and the install has no `backlog/`
/// directory.
pub(crate) const NO_TRACKER: &str =
    "no tracker installed; run houserules init --tracker json|github|beads to add one";

/// Line B: no tracker is installed, and the install has a `backlog/`
/// directory.
pub(crate) const BACKLOG_WITHOUT_TRACKER: &str =
    "backlog/ exists but no tracker is installed; run houserules init --tracker json";

/// The refusal of `init --tracker github|beads` on an install that has a
/// `backlog/` directory.
pub(crate) const BACKLOG_HOLDS_JSON_DATA: &str = "backlog/ holds JSON tracker data; move it as the migrating-knowledge skill describes and remove backlog/, or run houserules init --tracker json";

/// The refusal of `init --tracker github` when neither `--repo` nor the
/// `origin` remote names a repository.
pub(crate) const GITHUB_NEEDS_REPOSITORY: &str =
    "--tracker github needs a repository: pass --repo <owner>/<name>";

/// The refusal of `init --tracker beads` when `bd` is missing or the install
/// has no Beads store.
pub(crate) const BEADS_NEEDS_STORE: &str = "--tracker beads needs an initialized Beads store: run bd init (or bd bootstrap in a clone), then houserules init --tracker beads";

/// The refusal of `--repo` with another adapter than GitHub.
pub(crate) const REPO_NEEDS_GITHUB: &str = "--repo applies to --tracker github only";

/// The refusal of a `tracker.repo` value that is no `<owner>/<name>` for the
/// GitHub adapter, for a marker that holds one and for an adapter that is
/// built without one.
pub(crate) const TRACKER_REPO_FORMAT: &str =
    "tracker.repo must be <owner>/<name> for the github adapter";

/// The refusal of `set batch=<n>` for a value that is no positive integer.
pub(crate) const BATCH_NOT_POSITIVE: &str = "batch must be a positive integer";

/// The refusal of `set milestone=<m>` for a value that the backlog schema's
/// milestone does not take.
pub(crate) const MILESTONE_FORMAT: &str =
    "milestone must be 1 to 16 characters from A-Z, a-z, 0-9, ., /, and -";

/// The refusal of a `defer` that names both of `--title` and `--item`, or
/// neither.
pub(crate) const DEFER_NEEDS_ONE_TARGET: &str = "defer needs exactly one of --title and --item";

/// The line that `archive` prints in place of the backlog half when no
/// tracker is installed and no `backlog/` directory exists.
pub(crate) fn archive_skipped() -> String {
    format!("archive: skipped the backlog half: {NO_TRACKER}")
}

/// The line that `audit` prints on every run of an install with no tracker:
/// `n` is the number of work-item references it did not resolve.
pub(crate) fn audit_skipped(n: usize) -> String {
    format!("skipped {n} work-item references: no tracker installed (houserules init --tracker)")
}

/// The line for an installed tracker that cannot be reached: the `adapter`,
/// the `cause`, and the `setup` step that fixes it.
pub(crate) fn unreachable(adapter: &str, cause: &str, setup: &str) -> String {
    format!("tracker {adapter} cannot be reached: {cause}; {setup}")
}

/// The refusal of `pack add` for the pack `pack` that needs a tracker.
pub(crate) fn pack_needs_tracker(pack: &str) -> String {
    format!("pack {pack} needs a tracker; run houserules init --tracker json|github|beads")
}

/// The refusal of `init --tracker` on an install that has the tracker
/// `adapter`.
pub(crate) fn already_has_tracker(adapter: &str) -> String {
    format!("this install already has the {adapter} tracker; houserules changes no tracker")
}

/// The refusal of a `--repo` value that is no `<owner>/<name>`.
pub(crate) fn repo_is_malformed(repo: &str) -> String {
    format!("--repo \"{repo}\" must be <owner>/<name>")
}

/// The line of `get` for an id that names the pack `pack`, which the binary
/// carries and the install lacks.
pub(crate) fn pack_not_installed(id: &str, pack: &str) -> String {
    format!("{id}: pack {pack} is not installed; run houserules pack add {pack}")
}

/// The refusal of `get` for an id that the knowledge base and the tracker
/// `adapter` both resolve.
pub(crate) fn resolves_in_both(id: &str, adapter: &str) -> String {
    format!("{id} resolves both in the knowledge base and in the {adapter} tracker")
}

/// The refusal of `defer` for a title of `count` characters.
pub(crate) fn title_too_long(count: usize) -> String {
    format!("defer: the title is {count} characters; the limit is 120")
}

/// The refusal of `defer` for a `reason`, an `origin`, or a title (`what`)
/// that says nothing.
pub(crate) fn defer_text_empty(what: &str) -> String {
    format!("defer: the {what} is empty")
}

/// The refusal of `defer` for a `section` that holds a character outside
/// letters, digits, and hyphens.
pub(crate) fn section_malformed(section: &str) -> String {
    format!("defer: the section \"{section}\" must hold only letters, digits, and hyphens")
}

/// The refusal of `set status=<s>` for a status that is none of `names`.
pub(crate) fn status_must_be(names: &str) -> String {
    format!("status must be one of {names}")
}

/// The refusal of `set` for a field that it does not take.
pub(crate) fn unknown_field(field: &str) -> String {
    format!("unknown field \"{field}\"")
}

/// The line of `get` for an id that names no record.
pub(crate) fn unknown_id(id: &str) -> String {
    format!("unknown id \"{id}\"")
}

/// The line of `set` and `defer --item` for an id that names no item, the id
/// as the caller gave it.
pub(crate) fn unknown_item(id: &str) -> String {
    format!("unknown item \"{id}\"")
}

/// The line that `archive` prints for the backlog half on a tracker that
/// keeps closed items where they are: `adapter` is its name.
pub(crate) fn archive_in_place(adapter: &str) -> String {
    format!("archive: the {adapter} tracker keeps closed items in place")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Each line is the text of its spec section, byte for byte.
    #[test]
    fn the_lines_are_the_text_of_the_spec() {
        assert_eq!(
            NO_TRACKER,
            "no tracker installed; run houserules init --tracker json|github|beads to add one"
        );
        assert_eq!(
            BACKLOG_WITHOUT_TRACKER,
            "backlog/ exists but no tracker is installed; run houserules init --tracker json"
        );
        assert_eq!(
            archive_skipped(),
            "archive: skipped the backlog half: no tracker installed; run houserules init --tracker json|github|beads to add one"
        );
        assert_eq!(
            audit_skipped(0),
            "skipped 0 work-item references: no tracker installed (houserules init --tracker)"
        );
        assert_eq!(
            audit_skipped(12),
            "skipped 12 work-item references: no tracker installed (houserules init --tracker)"
        );
        assert_eq!(
            unreachable("github", "gh is not logged in", "run gh auth login"),
            "tracker github cannot be reached: gh is not logged in; run gh auth login"
        );
        assert_eq!(
            pack_needs_tracker("process"),
            "pack process needs a tracker; run houserules init --tracker json|github|beads"
        );
        assert_eq!(
            already_has_tracker("json"),
            "this install already has the json tracker; houserules changes no tracker"
        );
        assert_eq!(
            BACKLOG_HOLDS_JSON_DATA,
            "backlog/ holds JSON tracker data; move it as the migrating-knowledge skill describes and remove backlog/, or run houserules init --tracker json"
        );
        assert_eq!(
            GITHUB_NEEDS_REPOSITORY,
            "--tracker github needs a repository: pass --repo <owner>/<name>"
        );
        assert_eq!(
            BEADS_NEEDS_STORE,
            "--tracker beads needs an initialized Beads store: run bd init (or bd bootstrap in a clone), then houserules init --tracker beads"
        );
        assert_eq!(
            pack_not_installed("process.tdd", "process"),
            "process.tdd: pack process is not installed; run houserules pack add process"
        );
        assert_eq!(
            resolves_in_both("a.b", "beads"),
            "a.b resolves both in the knowledge base and in the beads tracker"
        );
        assert_eq!(
            title_too_long(121),
            "defer: the title is 121 characters; the limit is 120"
        );
    }

    /// Each line of `set`, `defer`, `get`, and `archive` that more than one
    /// adapter prints is its own text, byte for byte.
    #[test]
    fn the_lines_of_set_defer_get_and_archive_are_the_text_of_the_contract() {
        assert_eq!(
            TRACKER_REPO_FORMAT,
            "tracker.repo must be <owner>/<name> for the github adapter"
        );
        assert_eq!(BATCH_NOT_POSITIVE, "batch must be a positive integer");
        assert_eq!(
            MILESTONE_FORMAT,
            "milestone must be 1 to 16 characters from A-Z, a-z, 0-9, ., /, and -"
        );
        assert_eq!(
            DEFER_NEEDS_ONE_TARGET,
            "defer needs exactly one of --title and --item"
        );
        assert_eq!(defer_text_empty("reason"), "defer: the reason is empty");
        assert_eq!(
            section_malformed("a b"),
            "defer: the section \"a b\" must hold only letters, digits, and hyphens"
        );
        assert_eq!(
            status_must_be("open, partial"),
            "status must be one of open, partial"
        );
        assert_eq!(unknown_field("colour"), "unknown field \"colour\"");
        assert_eq!(unknown_id("WI-9"), "unknown id \"WI-9\"");
        assert_eq!(unknown_item("#9"), "unknown item \"#9\"");
        assert_eq!(
            archive_in_place("github"),
            "archive: the github tracker keeps closed items in place"
        );
    }
}
