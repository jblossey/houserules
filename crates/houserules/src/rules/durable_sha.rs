//! HR-120: `CheckType::DurableSha`'s own logic -- flags a 7-40-hex-
//! character token a diff hunk in one of the check's own `files` ADDS
//! that resolves, in this repository's own object database, to a commit
//! NOT reachable from the repository's real default branch.
//! `knowledge-base.cite-durable-refs`'s own subject: an in-branch sha
//! cited from a permanent file dies the moment the branch rebases or
//! aggregates (batch 21's own incident; two more instances at the
//! batch-24 close review).
//!
//! # Why the default branch is resolved explicitly, never `base_sha`
//!
//! An earlier version of this module used the audit's own `--base` as a
//! stand-in for the default branch, reasoning that `ci.yml`'s PR-audit
//! caller passes `pull_request.base.sha`, a real point on it. That
//! reasoning does not extend to this project's OTHER real callers of
//! `houserules audit`, which the branch review's own caller-space walk
//! (`t3b-review-hr120-caller-space.sh`) exercises: the per-task
//! self-audit the retrieval protocol mandates
//! (`houserules audit --base <task BASE> --head HEAD`) and the
//! branch-review `--workspace` form (`--base <the branch's merge base>`)
//! both pass an IN-BRANCH commit as `--base`, not a point on the default
//! branch at all. Against such a caller the old design failed in BOTH
//! directions at once: an in-branch sha cited from an earlier task in
//! the same batch was MISSED (trivially "reachable" from `base_sha`,
//! since `base_sha` IS that commit), while a commit genuinely already on
//! the default branch -- one that landed there mid-batch, a
//! release-please merge, say -- was wrongly FLAGGED, because it is not
//! an ancestor of the earlier in-branch `base_sha` the per-task caller
//! passes. Both directions are proven by
//! `find_violation_catches_a_citation_reachable_from_base_sha_but_not_the_default_branch`
//! and
//! `find_violation_passes_a_citation_reachable_from_the_default_branch_but_not_base_sha`
//! below, mirroring the review's own probes B1/B2 exactly.
//!
//! [`resolve_default_branch`] resolves the real default branch instead,
//! independent of whatever `--base`/`--head` a caller happens to pass,
//! trying in order: `refs/remotes/origin/HEAD` (set on a plain `git
//! clone`, but NOT reliably by `actions/checkout` even with
//! `fetch-depth: 0` -- a documented regression against Git 2.50,
//! actions/checkout#2219); `refs/remotes/origin/main` and
//! `refs/remotes/origin/master` (present after a `fetch-depth: 0`
//! checkout regardless, since that fetches every branch into
//! `refs/remotes/origin/*`, `origin/HEAD` symref or not -- this is what
//! makes `ci.yml`'s own PR-audit caller, and any other CI job checked
//! out this way, resolve correctly even when the symref is missing);
//! then a bare local `refs/heads/main`/`refs/heads/master`, for a
//! repository with no configured remote at all (this crate's own test
//! fixtures, a plain local clone). Reachability is then judged against
//! the resolved default-branch sha, decoupled from `base_sha` entirely;
//! `base_sha` still bounds the diff range (which lines the check reads),
//! never the reachability comparison. Unresolvable is a named error,
//! never a silent substitution of `base_sha` or any other ref.
//!
//! # False positives this design already excludes
//!
//! A content hash, digest, or id that merely looks hex is excluded by
//! construction, not by a denylist: [`resolve_commit`] accepts only a
//! candidate that `git rev-parse --verify` resolves, following an
//! annotated tag's own peel, to a real COMMIT object in this repository
//! -- a sha256 digest (64 hex characters) is never even considered a
//! candidate ([`hex_candidates`] only yields whole runs of 7-40
//! characters; a longer run is skipped entirely, never truncated into a
//! false 40-character prefix), and a random decimal number or invented
//! example sha simply does not resolve to any real object.
//!
//! # Scope: added lines only
//!
//! Only lines a hunk ADDS (`+`, not `+++`) are scanned: an unmodified
//! context line was already accepted in an earlier, already-judged
//! change -- this check exists to catch a NEW citation, not to
//! re-litigate history `knowledge-base.cite-durable-refs`'s own "nine
//! pre-batch-21 bare historical markers... dispositioned as accepted
//! history" clause already settled. [`find_violation`]'s caller
//! (`audit.rs`'s `run_check`) narrows `files` to the check's own
//! declared `files` glob before calling in, so this module never
//! hardcodes `backlog/`/`knowledge/` itself.

use std::path::Path;
use std::process::Command;

use super::audit::{git_diff, range};

/// One line a hunk added: the file it belongs to, its 1-based line
/// number in the new (head-side) file, and its text (the leading `+`
/// stripped).
struct AddedLine {
    file: String,
    line: usize,
    text: String,
}

/// Every line `git diff <range> -- <files>` adds, across every hunk of
/// every file in `files`, in the order the diff prints them --
/// `audit.rs`'s own `removed_lines` is the sibling reader for `-` lines;
/// this one also tracks the new-side line number from each hunk's own
/// `@@ -a,b +c,d @@` header, since evidence naming a bare file with no
/// line number is far less useful for a citation review. A line before
/// the first `@@` of a file (a `diff --git`/`index`/mode-change header
/// line) is never scanned: `in_hunk` stays `false` until the first hunk
/// header, and resets at each new file's own `+++` line.
fn added_lines(
    root: &Path,
    base: &str,
    head: &str,
    files: &[String],
) -> Result<Vec<AddedLine>, String> {
    if files.is_empty() {
        return Ok(Vec::new());
    }
    let mut args = vec![range(base, head), "--".to_string()];
    args.extend(files.iter().cloned());
    let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let output = git_diff(root, base, head, &arg_refs)?;

    let mut result = Vec::new();
    let mut current_file = String::new();
    let mut new_line = 0usize;
    let mut in_hunk = false;
    for line in output.lines() {
        if let Some(rest) = line.strip_prefix("+++ ") {
            current_file = rest.strip_prefix("b/").unwrap_or(rest).to_string();
            in_hunk = false;
            continue;
        }
        if line.starts_with("--- ") {
            continue;
        }
        if let Some(hunk) = line.strip_prefix("@@ ") {
            new_line = hunk
                .split_whitespace()
                .find_map(|token| token.strip_prefix('+'))
                .and_then(|new_side| new_side.split(',').next())
                .and_then(|start| start.parse().ok())
                .unwrap_or(1);
            in_hunk = true;
            continue;
        }
        if !in_hunk {
            continue;
        }
        match line.chars().next() {
            Some('+') => {
                result.push(AddedLine {
                    file: current_file.clone(),
                    line: new_line,
                    text: line[1..].to_string(),
                });
                new_line += 1;
            }
            Some('-') => {} // removed: does not exist in the new file, no line number to track
            _ => new_line += 1, // an unmodified context line
        }
    }
    Ok(result)
}

/// Every maximal run of 7-40 ASCII hex characters in `text`, lowercased
/// -- this module's own doc, "False positives this design already
/// excludes", explains why the length bound alone already excludes a
/// sha256 digest.
fn hex_candidates(text: &str) -> Vec<String> {
    let mut candidates = Vec::new();
    let mut current = String::new();
    for ch in text.chars().chain(std::iter::once('\0')) {
        if ch.is_ascii_hexdigit() {
            current.push(ch.to_ascii_lowercase());
        } else {
            if (7..=40).contains(&current.len()) {
                candidates.push(std::mem::take(&mut current));
            }
            current.clear();
        }
    }
    candidates
}

/// `Some(full sha)` when `candidate` resolves, in `root`'s object
/// database, to a commit (following an annotated tag's own peel);
/// `None` when it does not resolve to a commit at all -- `git rev-parse
/// --verify`'s own `^{commit}` peel decides this, never a hand-rolled
/// object-type check. `git rev-parse --verify --quiet` exits 1 for a
/// name that does not resolve to an object (a normal "not a candidate"
/// answer, `None` here) and 128 for a real failure such as running
/// outside a repository at all (a named error, matching `is_ancestor`'s
/// own exit-code split just below and `houserules.crash-paths-are-
/// named`: this exact asymmetry was a Minor review finding when only
/// `is_ancestor` made the distinction).
fn resolve_commit(root: &Path, candidate: &str) -> Result<Option<String>, String> {
    let output = Command::new("git")
        .args([
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("{candidate}^{{commit}}"),
        ])
        .env("LC_ALL", "C")
        .current_dir(root)
        .output()
        .map_err(|error| error.to_string())?;
    match output.status.code() {
        Some(0) => {
            let sha = String::from_utf8_lossy(&output.stdout).trim().to_string();
            Ok(if sha.is_empty() { None } else { Some(sha) })
        }
        Some(1) => Ok(None),
        _ => Err(format!(
            "git rev-parse --verify --quiet {candidate}^{{commit}}: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )),
    }
}

/// Resolves this repository's real default branch tip, independent of
/// whatever `--base`/`--head` an audit invocation happens to use --
/// this module's own doc, "Why the default branch is resolved
/// explicitly", has the full account of why `--base` cannot stand in
/// for it, and the order these candidates are tried in. `Err` names the
/// failure; this function never falls back to substituting some other
/// ref when none of these resolve. `pub(super)`: `audit.rs`'s own
/// `run_check` resolves the default branch once per `CheckType::
/// DurableSha` evaluation through this same function.
pub(super) fn resolve_default_branch(root: &Path) -> Result<String, String> {
    const CANDIDATES: [&str; 5] = [
        "refs/remotes/origin/HEAD",
        "refs/remotes/origin/main",
        "refs/remotes/origin/master",
        "refs/heads/main",
        "refs/heads/master",
    ];
    for candidate in CANDIDATES {
        let output = Command::new("git")
            .args(["rev-parse", "--verify", "--quiet", candidate])
            .env("LC_ALL", "C")
            .current_dir(root)
            .output()
            .map_err(|error| error.to_string())?;
        if output.status.success() {
            let sha = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !sha.is_empty() {
                return Ok(sha);
            }
        }
    }
    Err(
        "durable-sha: could not resolve the repository's default branch (no origin/HEAD, \
         origin/main, origin/master, or local main/master branch) -- reachability cannot be \
         judged without it"
            .to_string(),
    )
}

/// `true` when `commit` is `of` itself or an ancestor of it. Git's own
/// docs for `--is-ancestor` distinguish exit 1 (a real, valid "no") from
/// any other nonzero code ("errors are signaled by a non-zero status
/// that is not 1"); this function keeps that distinction rather than
/// treating every nonzero exit as "not an ancestor" -- both `commit` and
/// `of` are already-resolved, guaranteed-valid object names by the time
/// this runs (`resolve_commit` for the former, `rev` upstream for the
/// latter), so a nonzero exit other than 1 here is a real git failure to
/// name, not a normal outcome to swallow (`houserules.crash-paths-are-
/// named`).
fn is_ancestor(root: &Path, commit: &str, of: &str) -> Result<bool, String> {
    let output = Command::new("git")
        .args(["merge-base", "--is-ancestor", commit, of])
        .env("LC_ALL", "C")
        .current_dir(root)
        .output()
        .map_err(|error| error.to_string())?;
    match output.status.code() {
        Some(0) => Ok(true),
        Some(1) => Ok(false),
        _ => Err(format!(
            "git merge-base --is-ancestor {commit} {of}: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )),
    }
}

/// The full check: every line `files` adds between `base_sha` and
/// `head_sha` (the diff range), scanned for a hex candidate that
/// resolves to a commit not reachable from `default_branch_sha` --
/// `Some(evidence)` naming the first one found (file, line, the cited
/// candidate, and the resolved sha), `None` when none is found.
/// `default_branch_sha` is resolved by the caller
/// ([`resolve_default_branch`]) and is deliberately a SEPARATE argument
/// from `base_sha`: this module's own doc, "Why the default branch is
/// resolved explicitly", has the full account of why conflating the two
/// missed and flagged citations in both directions for this project's
/// own per-task and branch-review callers.
pub(super) fn find_violation(
    root: &Path,
    base_sha: &str,
    head_sha: &str,
    default_branch_sha: &str,
    files: &[String],
) -> Result<Option<String>, String> {
    for line in added_lines(root, base_sha, head_sha, files)? {
        for candidate in hex_candidates(&line.text) {
            let Some(resolved) = resolve_commit(root, &candidate)? else {
                continue;
            };
            if !is_ancestor(root, &resolved, default_branch_sha)? {
                return Ok(Some(format!(
                    "{}:{}: {:?} cites {candidate}, resolving to commit {resolved}, not \
                     reachable from the default branch ({default_branch_sha})",
                    line.file,
                    line.line,
                    line.text.trim(),
                )));
            }
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use std::process::Command;

    use super::*;

    /// This module's own copy of the small git-fixture helper every
    /// git-backed test module in this crate keeps (`audit.rs`'s own `fn
    /// git`, `check.rs`'s own `fn git`, `report_claims.rs`'s own `fn
    /// git`).
    fn git(root: &Path, args: &[&str]) -> String {
        let output = Command::new("git")
            .args(args)
            .current_dir(root)
            .output()
            .expect("run git");
        assert!(
            output.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).into_owned()
    }

    fn write_file(root: &Path, relative: &str, content: &str) {
        let path = root.join(relative);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(path, content).unwrap();
    }

    fn commit(root: &Path, message: &str) -> String {
        git(root, &["add", "-A"]);
        git(
            root,
            &[
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@t.t",
                "-c",
                "commit.gpgsign=false",
                "commit",
                "-q",
                "--no-verify",
                "--allow-empty",
                "-m",
                message,
            ],
        );
        git(root, &["rev-parse", "HEAD"]).trim().to_string()
    }

    /// `hex_candidates` extracts a bare 7-character run, ignores runs
    /// shorter than 7 and a 64-character sha256-shaped run, and never
    /// truncates the long run into a false 40-character prefix.
    #[test]
    fn hex_candidates_bounds_run_length_to_seven_through_forty() {
        let sha256_shaped = "a".repeat(64);
        let text = format!("see 1a2b3c4 and ab12 and {sha256_shaped} and cite this");
        assert_eq!(hex_candidates(&text), vec!["1a2b3c4".to_string()]);
    }

    /// `hex_candidates` lowercases an uppercase hex run.
    #[test]
    fn hex_candidates_lowercases_the_run() {
        assert_eq!(hex_candidates("1A2B3D4"), vec!["1a2b3d4".to_string()]);
    }

    /// A scratch repository: `main` with one commit, and one throwaway
    /// commit `main` never reaches -- the exact shape a rebase or
    /// aggregation leaves behind (`knowledge-base.cite-durable-refs`).
    struct Fixture {
        dir: tempfile::TempDir,
        main_tip: String,
        stray_sha: String,
    }

    impl Fixture {
        fn new() -> Self {
            let dir = tempfile::tempdir().expect("tempdir");
            let root = dir.path();
            git(root, &["init", "-q", "-b", "main"]);
            write_file(root, "README.md", "seed\n");
            let main_tip = commit(root, "chore: seed");
            // A stray branch, then deleted, so `stray_sha` is a real
            // commit in the object database but reachable from no ref at
            // all -- `git branch -D` alone, with no `git gc`, leaves the
            // object live in a fresh checkout's own database for as long
            // as the reflog does, which is exactly this test's own
            // window.
            git(root, &["checkout", "-q", "-b", "stray"]);
            write_file(root, "stray.md", "stray\n");
            let stray_sha = commit(root, "chore: stray, never reaches main");
            git(root, &["checkout", "-q", "main"]);
            git(root, &["branch", "-D", "stray"]);
            Self {
                dir,
                main_tip,
                stray_sha,
            }
        }

        fn root(&self) -> &Path {
            self.dir.path()
        }
    }

    /// The live catch: an added backlog line citing the stray sha (not
    /// reachable from `main_tip`) is a violation naming the file, line,
    /// and resolved sha.
    #[test]
    fn find_violation_catches_an_added_citation_to_an_unreachable_commit() {
        let fixture = Fixture::new();
        let root = fixture.root();
        write_file(
            root,
            "backlog/items/kit.json",
            &format!("{{\"note\": \"see {}\"}}\n", fixture.stray_sha),
        );
        let head = commit(root, "docs(backlog): cite an in-branch sha");
        let violation = find_violation(
            root,
            &fixture.main_tip,
            &head,
            &fixture.main_tip,
            &["backlog/items/kit.json".to_string()],
        )
        .expect("find_violation runs");
        let evidence = violation.expect("a violation is found");
        assert!(evidence.contains("backlog/items/kit.json"), "{evidence}");
        assert!(evidence.contains(&fixture.stray_sha), "{evidence}");
    }

    /// A citation to a sha that IS reachable from the default branch (an
    /// already-merged commit) is not a violation.
    #[test]
    fn find_violation_passes_a_citation_reachable_from_the_default_branch() {
        let fixture = Fixture::new();
        let root = fixture.root();
        write_file(
            root,
            "backlog/items/kit.json",
            &format!("{{\"note\": \"see {}\"}}\n", fixture.main_tip),
        );
        let head = commit(root, "docs(backlog): cite the seed commit");
        let violation = find_violation(
            root,
            &fixture.main_tip,
            &head,
            &fixture.main_tip,
            &["backlog/items/kit.json".to_string()],
        )
        .expect("find_violation runs");
        assert!(violation.is_none(), "{violation:?}");
    }

    /// A content hash that merely looks hex (too long to be a git sha)
    /// is not a violation: `hex_candidates` never yields it as a
    /// candidate at all.
    #[test]
    fn find_violation_ignores_a_sha256_shaped_digest() {
        let fixture = Fixture::new();
        let root = fixture.root();
        let digest = "f".repeat(64);
        write_file(
            root,
            "knowledge/houserules.json",
            &format!("{{\"digest\": \"{digest}\"}}\n"),
        );
        let head = commit(root, "docs(knowledge): record a content digest");
        let violation = find_violation(
            root,
            &fixture.main_tip,
            &head,
            &fixture.main_tip,
            &["knowledge/houserules.json".to_string()],
        )
        .expect("find_violation runs");
        assert!(violation.is_none(), "{violation:?}");
    }

    /// A made-up, non-existent hex string (an example placeholder) never
    /// resolves to a real object, so it is never a violation either.
    #[test]
    fn find_violation_ignores_a_string_that_resolves_to_no_object() {
        let fixture = Fixture::new();
        let root = fixture.root();
        write_file(
            root,
            "knowledge/houserules.json",
            "{\"example\": \"see 0000000\"}\n",
        );
        let head = commit(root, "docs(knowledge): a placeholder example sha");
        let violation = find_violation(
            root,
            &fixture.main_tip,
            &head,
            &fixture.main_tip,
            &["knowledge/houserules.json".to_string()],
        )
        .expect("find_violation runs");
        assert!(violation.is_none(), "{violation:?}");
    }

    /// An unmodified CONTEXT line citing a stray sha (already present
    /// before `base_sha`, on its OWN line so a later, unrelated edit
    /// leaves it untouched by the diff) is never scanned -- only lines
    /// the diff ADDS are in scope. Git diffs at line granularity, so the
    /// citation and the later edit must sit on separate lines for this
    /// test to exercise a genuine context line rather than a whole-line
    /// replacement that happens to still contain the same text.
    #[test]
    fn find_violation_never_scans_an_unmodified_context_line() {
        let fixture = Fixture::new();
        let root = fixture.root();
        write_file(
            root,
            "notes.txt",
            &format!("see {}\nline one\n", fixture.stray_sha),
        );
        let base_with_context = commit(root, "docs: pre-existing stray citation");
        write_file(
            root,
            "notes.txt",
            &format!("see {}\nline two\n", fixture.stray_sha),
        );
        let head = commit(root, "docs: unrelated edit, same file");
        let violation = find_violation(
            root,
            &base_with_context,
            &head,
            &base_with_context,
            &["notes.txt".to_string()],
        )
        .expect("find_violation runs");
        assert!(violation.is_none(), "{violation:?}");
    }

    /// `files` empty (the check's own `run_check` already handles "not
    /// triggered" before calling in, but this module's own function
    /// stays correct standalone too).
    #[test]
    fn find_violation_is_none_for_an_empty_file_list() {
        let fixture = Fixture::new();
        let violation = find_violation(
            fixture.root(),
            &fixture.main_tip,
            &fixture.main_tip,
            &fixture.main_tip,
            &[],
        )
        .expect("find_violation runs");
        assert!(violation.is_none());
    }

    /// Reproduces the branch review's own caller-space probe B1
    /// (t3b-review-hr120-caller-space.sh): a per-task `--base` is an
    /// IN-BRANCH commit (the retrieval protocol's own mandated shape),
    /// and the cited commit IS that same in-branch commit -- trivially
    /// "reachable from base_sha", which is exactly the false negative
    /// the old design shipped. `default_branch_sha` here is
    /// `fixture.main_tip`, a point BEFORE the in-branch commit exists at
    /// all: the citation must still be caught, decoupled entirely from
    /// `base_sha`.
    #[test]
    fn find_violation_catches_a_citation_reachable_from_base_sha_but_not_the_default_branch() {
        let fixture = Fixture::new();
        let root = fixture.root();
        let in_branch = commit(root, "feat: an in-branch commit (an earlier task)");
        write_file(
            root,
            "knowledge/houserules.json",
            &format!("{{\"note\": \"see {in_branch}\"}}\n"),
        );
        let head = commit(
            root,
            "docs(knowledge): cite an in-branch sha from an earlier task",
        );
        let violation = find_violation(
            root,
            &in_branch,
            &head,
            &fixture.main_tip,
            &["knowledge/houserules.json".to_string()],
        )
        .expect("find_violation runs");
        let evidence = violation.expect(
            "reachable from base_sha (base_sha IS the cited commit) but not from the default \
             branch -- must still be caught",
        );
        assert!(evidence.contains(&in_branch), "{evidence}");
    }

    /// Reproduces the branch review's own caller-space probe B2: a
    /// commit that IS on the real default branch (main keeps moving --
    /// a release-please merge, say) cited from a diff whose own
    /// `base_sha` predates it. The old design flagged this as
    /// unreachable, which is the false positive: a durable citation
    /// that this task's own accepted fix must stop rejecting.
    #[test]
    fn find_violation_passes_a_citation_reachable_from_the_default_branch_but_not_base_sha() {
        let fixture = Fixture::new();
        let root = fixture.root();
        write_file(root, "BOT.md", "bot\n");
        let bot_on_main = commit(root, "chore(main): a bot commit after the branch point");
        git(
            root,
            &["checkout", "-q", "-b", "feature", &fixture.main_tip],
        );
        write_file(
            root,
            "knowledge/houserules.json",
            &format!("{{\"note\": \"see {bot_on_main}\"}}\n"),
        );
        let head = commit(
            root,
            "docs(knowledge): cite a commit already merged to main",
        );
        let violation = find_violation(
            root,
            &fixture.main_tip,
            &head,
            &bot_on_main,
            &["knowledge/houserules.json".to_string()],
        )
        .expect("find_violation runs");
        assert!(
            violation.is_none(),
            "reachable from the default branch even though base_sha predates it: {violation:?}"
        );
    }

    /// A repository with no remote at all resolves the default branch
    /// through a bare local `main` branch.
    #[test]
    fn resolve_default_branch_falls_back_to_a_local_main_branch() {
        let fixture = Fixture::new();
        let resolved = resolve_default_branch(fixture.root()).expect("resolves");
        assert_eq!(resolved, fixture.main_tip);
    }

    /// `refs/remotes/origin/main` (present after a real `fetch-depth: 0`
    /// checkout even when `origin/HEAD` is NOT set -- a documented
    /// regression against Git 2.50, actions/checkout#2219) resolves
    /// correctly without the symbolic ref at all.
    #[test]
    fn resolve_default_branch_falls_back_to_origin_main_without_origin_head() {
        let fixture = Fixture::new();
        let root = fixture.root();
        let extra = commit(
            root,
            "chore: a commit past main_tip, unrelated to origin/HEAD",
        );
        git(root, &["update-ref", "refs/remotes/origin/main", &extra]);
        let resolved = resolve_default_branch(root).expect("resolves");
        assert_eq!(resolved, extra);
    }

    /// `refs/remotes/origin/HEAD`, when present, wins over a same-named
    /// local branch -- the normal-clone case.
    #[test]
    fn resolve_default_branch_prefers_origin_head_when_set() {
        let fixture = Fixture::new();
        let root = fixture.root();
        let extra = commit(root, "chore: a commit only origin/HEAD should surface");
        git(root, &["update-ref", "refs/remotes/origin/main", &extra]);
        git(
            root,
            &[
                "symbolic-ref",
                "refs/remotes/origin/HEAD",
                "refs/remotes/origin/main",
            ],
        );
        let resolved = resolve_default_branch(root).expect("resolves");
        assert_eq!(resolved, extra);
    }

    /// No origin ref and no local `main`/`master` branch at all is a
    /// named error, never a silent substitution of some other ref.
    #[test]
    fn resolve_default_branch_names_the_failure_when_unresolvable() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        git(root, &["init", "-q", "-b", "trunk"]);
        write_file(root, "README.md", "seed\n");
        commit(root, "chore: seed on trunk");
        let error = resolve_default_branch(root).expect_err("no main/master, no origin");
        assert!(error.contains("default branch"), "{error}");
    }
}
