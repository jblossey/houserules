//! The permanent post-JS residue gate (spec §7/§5.39, batch 20 T4, HR-047's
//! closing sweep): a zero-hit sweep of the retired forms' INVOCATION
//! patterns -- `node`, `npm`, `pnpm`, `npx`, `vitest`, `.mjs`/`.mts`, and
//! `package.json` -- across every file this repository tracks, outside a
//! declared sanctioned-history allowlist. A sibling to `find-shell-tool-
//! refs`/`diff-shape-gate` in shape (dev-only, not shipped in `template/`
//! or the payload), but unlike those two one-time rewrite proofs, this
//! gate is meant to fire forever: it is wired into `mise run lint`
//! (`mise.toml`'s `lint` task), so every future PR and the CI checks job
//! run it too.
//!
//! # Scope: the tracked-file list minus a declared exclusion list, not a
//! hand-typed inclusion list (fix round 1, important finding 1)
//!
//! The first shipped version of this gate named ten files to walk by
//! hand. A hand-typed inclusion list cannot cover a file that does not
//! exist yet, and it already missed ones that did: the reviewer seeded
//! `pnpm test` into this repository's own root `CLAUDE.md` and `npx
//! some-tool` into `tools/claude-session-start.sh` and both passed with
//! "0 unexpected", because neither file was ever added to the list. A
//! gate declared permanent has to default to gated, not to remembering.
//!
//! So the walk set is now derived: every path `git ls-files` names,
//! minus `EXCLUDED_PREFIXES` (`is_excluded`'s own doc names the reason
//! for each). A new file lands in the walk automatically the moment it is
//! tracked, unless it falls under a prefix already declared sanctioned.
//!
//! Invocation pattern vs mention is still the content-side boundary this
//! gate draws within that walk (the spec's own stated trap): a bare
//! content sweep of the WHOLE repository would flag the knowledge base's
//! own `RETIRED at batch 20 T3` histories, `crates/houserules/`'s
//! permanent JS-porting-parity narration (every doc comment explaining
//! what the frozen JS did, verified live against `node 24.18.1`, so a
//! future port stays byte-exact), and the ecosystem-agnostic multi-
//! language examples `security-hygiene.exact-pins`/`process.no-tech-debt`
//! ship generically to every adopter (JS included) -- none of them a
//! residue of THIS repository's own retired toolchain. `EXCLUDED_PREFIXES`
//! keeps those out of the walk; `exceptions()` handles the few remaining
//! legitimate mentions the walk still reaches.
//!
//! # `EXCLUDED_PREFIXES` (a tracked path starting with one of these is
//! never walked)
//!
//! - `crates/` -- source and tests narrate the frozen JS's permanently-
//!   fixed porting-parity history forever (`find-shell-tool-refs`'s own
//!   module doc states the identical boundary for its narrower sweep).
//! - `knowledge/`, `backlog/` -- `knowledge-base.state-only-the-source`
//!   and the batch process already govern every word here by hand at
//!   each closing sweep; `security-hygiene.exact-pins`'s own multi-
//!   ecosystem glob examples would false-positive a bare content sweep.
//! - `.claude/rules/`, `.claude/skills/project-knowledge/` -- both carry
//!   the header "Generated from knowledge/ by houserules render. Do not
//!   edit."; `render.rs` copies `knowledge/`'s own id/summary/body text
//!   into a template shape and adds no prose of its own, so a residue
//!   here can only be a residue already gated at its `knowledge/` source.
//! - `docs/specs/`, `docs/plans/`, `docs/design.md` -- frozen once
//!   approved.
//! - `tests/` -- `tests/fixtures/batch14-workspace/` is pinned byte for
//!   byte (`houserules.corpus-batch14-fixtures-are-committed`);
//!   `tests/fixtures/mini/` and `tests/goldens/` are live input data and
//!   generated golden output the Rust suites themselves exercise and
//!   regenerate (`gen-goldens.rs`, `common::FROZEN_SHA`), not
//!   instructional prose a residue could mislead a reader with (fix
//!   round 1, important finding 3: the first version of this doc called
//!   the whole directory frozen "byte for byte", which is only true of
//!   `batch14-workspace/` -- this same task edits `tests/fixtures/mini/
//!   CLAUDE.md`). Excluding the whole `tests/` prefix is still correct on
//!   the right ground: none of it is a live instruction this repository's
//!   own contributors follow.
//! - `.superpowers/` -- batch workspaces: dated session history.
//! - `.claude/evals/` -- eval scenarios model adopter repositories across
//!   ecosystems and are governed by `process.evals-rerun`, never touched
//!   by an unrelated gate. (`template/.claude/evals/` is NOT under this
//!   prefix -- it stays walked, with its own named exception below,
//!   because it ships inside the payload template/ itself covers.)
//!
//! # Unreadable and binary paths (fix round 1, important finding 2)
//!
//! A walked path `git` tracks but the filesystem does not have (removed,
//! renamed on disk without `git mv`, permission denied) is a named error
//! that fails the gate: the first version silently `continue`d past a
//! read failure, so renaming `CONTRIBUTING.md` on disk dropped it from
//! both the read count and the hit list while the summary line still
//! claimed the old file count -- the seeded violation in the renamed file
//! passed unnoticed. A walked path that reads but is not valid UTF-8 (a
//! genuinely binary payload asset under `template/`) is not an error: it
//! is counted and printed as a skip, separately from the scanned count,
//! which now names only the files actually read as text.
//!
//! # Named exceptions within the walked scope
//!
//! `exceptions()`'s own doc names each one and why it is not residue.
//!
//! Usage: `cargo run --quiet --bin residue-gate`, run from anywhere
//! inside this checkout. Prints every unreadable path, then every binary
//! skip, then every unexplained hit, then a summary line; exits 0 only
//! when no path was unreadable and no hit remains unexplained.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// The retired forms' substrings this gate flags, matched case-
/// insensitively against each line's lowercased text -- broad on
/// purpose, the same way `find-shell-tool-refs`'s own `PATTERNS` is:
/// precision comes from the walk (`EXCLUDED_PREFIXES`: what gets read at
/// all) and `exceptions()` (what a real hit there is excused for), not
/// from a narrower pattern.
const PATTERNS: [&str; 8] = [
    "node",
    "npm",
    "pnpm",
    "npx",
    "vitest",
    ".mjs",
    ".mts",
    "package.json",
];

/// Tracked-path prefixes never walked -- this module's own doc has the
/// reason for each.
const EXCLUDED_PREFIXES: [&str; 11] = [
    "crates/",
    "knowledge/",
    "backlog/",
    ".claude/rules/",
    ".claude/skills/project-knowledge/",
    "docs/specs/",
    "docs/plans/",
    "docs/design.md",
    "tests/",
    ".superpowers/",
    ".claude/evals/",
];

/// One matched line: its repo-relative path, 1-based line number, and
/// trimmed text.
struct Hit {
    file: String,
    line: usize,
    text: String,
}

/// This checkout's repository root, resolved at compile time -- every
/// other `src/bin/*.rs` file's own copy of this helper.
fn repo_root() -> PathBuf {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    root.canonicalize()
        .unwrap_or_else(|error| panic!("canonicalize {}: {error}", root.display()))
}

/// Every path `git ls-files` names under `root`, repo-relative POSIX
/// paths, in the order git prints them -- the walk set this gate audits
/// is exactly this list minus `EXCLUDED_PREFIXES`, so a newly tracked
/// file is gated by default (this module's own doc, "Scope", has the
/// finding this replaced).
fn tracked_files(root: &Path) -> Vec<String> {
    let output = Command::new("git")
        .args(["ls-files"])
        .current_dir(root)
        .output()
        .unwrap_or_else(|error| panic!("run git ls-files: {error}"));
    assert!(
        output.status.success(),
        "git ls-files failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .expect("git ls-files output is UTF-8")
        .lines()
        .map(str::to_string)
        .collect()
}

/// `true` when `rel_path` falls under a declared `EXCLUDED_PREFIXES`
/// entry -- that constant's own doc names the reason for each.
fn is_excluded(rel_path: &str) -> bool {
    EXCLUDED_PREFIXES
        .iter()
        .any(|prefix| rel_path.starts_with(prefix))
}

/// The result of trying to read one walked path's text: valid UTF-8, a
/// binary asset (reads but is not UTF-8), or unreadable (the filesystem
/// error message) -- this module's own doc, "Unreadable and binary
/// paths", has the account of why the three are handled differently.
enum ReadOutcome {
    Text(String),
    Binary,
    Unreadable(String),
}

/// Classifies `abs_path` per `ReadOutcome`'s own doc.
fn read_outcome(abs_path: &Path) -> ReadOutcome {
    match fs::read(abs_path) {
        Ok(bytes) => match String::from_utf8(bytes) {
            Ok(text) => ReadOutcome::Text(text),
            Err(_) => ReadOutcome::Binary,
        },
        Err(error) => ReadOutcome::Unreadable(error.to_string()),
    }
}

/// Every line of `text` whose lowercased form contains a `PATTERNS`
/// substring, as `Hit`s naming `rel_path`.
fn find_matches(rel_path: &str, text: &str) -> Vec<Hit> {
    let mut hits = Vec::new();
    for (index, line) in text.split('\n').enumerate() {
        let lower = line.to_lowercase();
        if PATTERNS.iter().any(|pattern| lower.contains(pattern)) {
            hits.push(Hit {
                file: rel_path.to_string(),
                line: index + 1,
                text: line.trim().to_string(),
            });
        }
    }
    hits
}

/// The 1-based line numbers of `docs/runbook.md`'s own "release-please's
/// release-type" section (its heading through the line before the next
/// `## ` heading, or end of file): explains `release-please`'s own
/// third-party internals -- a Node-based tool this repository's CI still
/// legitimately depends on (`.github/workflows/release-please.yml`) -- to
/// derive HR-073's fix from the pinned source
/// (`process.wiring-checks-run-the-resolution`), never an instruction
/// this repository's own toolchain follows. Heading-bounded rather than a
/// whole-file exception so the rest of the runbook -- the release and
/// restamp procedures -- stays gated. Batch 20 T5 renamed the section
/// from "Known gap" once HR-073 closed pre-merge; this function's own
/// heading match moved with it (this module's own doc, "Scope", names
/// the wider principle a gate this narrowly anchored has to keep up
/// with).
fn runbook_release_please_section_lines(text: &str) -> HashSet<usize> {
    let lines: Vec<&str> = text.split('\n').collect();
    let mut exempt = HashSet::new();
    let Some(start) = lines
        .iter()
        .position(|line| line.starts_with("## release-please's release-type"))
    else {
        return exempt;
    };
    exempt.insert(start + 1);
    for (offset, line) in lines.iter().enumerate().skip(start + 1) {
        if line.starts_with("## ") {
            break;
        }
        exempt.insert(offset + 1);
    }
    exempt
}

/// One exception category's name, the reason it stays, and the predicate
/// deciding whether a given hit (with its file's full text alongside)
/// falls inside it.
struct Exception {
    label: &'static str,
    reason: &'static str,
    matches: fn(&Hit, &str) -> bool,
}

/// The named, auditable exceptions within the walked scope -- this
/// module's own doc has the wider account of why the scope is what it
/// is; these are the individual mentions that scope still contains.
fn exceptions() -> Vec<Exception> {
    vec![
        Exception {
            label: "README.md: \"no Node\" toolchain summary",
            reason: "a negation, not an invocation: the sentence states the toolchain has no \
                Node, the opposite of residue.",
            matches: |hit, _file_text| hit.file == "README.md" && hit.text.contains("no Node"),
        },
        Exception {
            label: "README.md: \"will not publish to npm\" ruling",
            reason: "a negation citing a dated owner ruling (npm retires, decisions.json): \
                stating houserules will never publish to npm is not an npm invocation.",
            matches: |hit, _file_text| {
                hit.file == "README.md"
                    && hit.text.to_lowercase().contains("will not publish to npm")
            },
        },
        Exception {
            label: "docs/runbook.md: the release-please release-type section (HR-073)",
            reason: "`runbook_release_please_section_lines`'s own doc has the full account: \
                release-please's own third-party Node internals, derived from its pinned source \
                to explain and close HR-073's fix, never this repository's own toolchain.",
            matches: |hit, file_text| {
                hit.file == "docs/runbook.md"
                    && runbook_release_please_section_lines(file_text).contains(&hit.line)
            },
        },
        Exception {
            label: "template/knowledge/security-hygiene.json (whole file)",
            reason: "security-hygiene.exact-pins ships generic, ecosystem-agnostic manifest \
                globs and CLI examples (package.json alongside Cargo.toml/pyproject.toml/go.mod, \
                `pnpm add --save-exact` alongside `cargo add`) to every adopter regardless of \
                their own ecosystem -- verified clean of this repository's own toolchain by hand \
                at the batch 20 T4 closing sweep.",
            matches: |hit, _file_text| hit.file == "template/knowledge/security-hygiene.json",
        },
        Exception {
            label: "template/knowledge/process.json (whole file)",
            reason: "process.no-tech-debt's leftover-marker grep-absent check ships a multi-\
                language file-glob list (.mjs/.mts alongside .rs/.py/.go/.rb/.java/...) to every \
                adopter, not a claim this repository's own toolchain includes them.",
            matches: |hit, _file_text| hit.file == "template/knowledge/process.json",
        },
        Exception {
            label: "template/.claude/evals/ (whole subtree)",
            reason: "eval scenarios model a hypothetical adopter's own repository across \
                ecosystems (a seeded package.json is scratch-worktree fixture data, not this \
                repository's own manifest); governed by process.evals-rerun, never silently \
                touched by an unrelated gate.",
            matches: |hit, _file_text| hit.file.starts_with("template/.claude/evals/"),
        },
    ]
}

/// Walks the tracked-file list minus `EXCLUDED_PREFIXES`, classifies
/// every `PATTERNS` hit as excepted or unexpected, reports every
/// unreadable and binary path, prints all of it plus the summary line,
/// and exits 1 if any path was unreadable or any hit remains unexpected
/// (0 otherwise) -- this module's own doc has the full scope and
/// exception account.
fn main() {
    let root = repo_root();
    let tracked = tracked_files(&root);
    let walked: Vec<&String> = tracked.iter().filter(|path| !is_excluded(path)).collect();

    let exceptions = exceptions();
    let mut target: Vec<Hit> = Vec::new();
    let mut excepted: Vec<(Hit, &'static str, &'static str)> = Vec::new();
    let mut binary_skips: Vec<&String> = Vec::new();
    let mut unreadable: Vec<(&String, String)> = Vec::new();
    let mut read_count = 0usize;

    for rel_path in &walked {
        match read_outcome(&root.join(rel_path)) {
            ReadOutcome::Unreadable(message) => unreadable.push((rel_path, message)),
            ReadOutcome::Binary => binary_skips.push(rel_path),
            ReadOutcome::Text(file_text) => {
                read_count += 1;
                for hit in find_matches(rel_path, &file_text) {
                    match exceptions
                        .iter()
                        .find(|exception| (exception.matches)(&hit, &file_text))
                    {
                        Some(exception) => excepted.push((hit, exception.label, exception.reason)),
                        None => target.push(hit),
                    }
                }
            }
        }
    }

    if !unreadable.is_empty() {
        println!("-- {} walked path(s) failed to read --", unreadable.len());
        for (path, message) in &unreadable {
            println!("{path}: {message}");
        }
    }
    if !binary_skips.is_empty() {
        println!(
            "\n-- {} binary asset(s) skipped (not text, not fatal) --",
            binary_skips.len()
        );
        for path in &binary_skips {
            println!("  {path}");
        }
    }
    if !target.is_empty() {
        println!("\n-- {} unexplained retired-form hit(s) --", target.len());
        for hit in &target {
            println!("{}:{}: {}", hit.file, hit.line, hit.text);
        }
    }
    if !excepted.is_empty() {
        println!(
            "\n-- {} excepted hit(s) (named, sanctioned) --",
            excepted.len()
        );
        let mut by_label: Vec<(&'static str, &'static str, Vec<&Hit>)> = Vec::new();
        for (hit, label, reason) in &excepted {
            match by_label
                .iter_mut()
                .find(|(existing_label, _, _)| existing_label == label)
            {
                Some((_, _, hits)) => hits.push(hit),
                None => by_label.push((label, reason, vec![hit])),
            }
        }
        for (label, reason, hits) in &by_label {
            println!("{label}: {} hit(s) -- {reason}", hits.len());
            for hit in hits {
                println!("  {}:{}: {}", hit.file, hit.line, hit.text);
            }
        }
    }

    println!(
        "\nsummary: {} tracked, {} excluded, {} walked, {} read, {} binary skipped, \
         {} unreadable, {} total hits, {} excepted, {} unexpected",
        tracked.len(),
        tracked.len() - walked.len(),
        walked.len(),
        read_count,
        binary_skips.len(),
        unreadable.len(),
        target.len() + excepted.len(),
        excepted.len(),
        target.len(),
    );
    let failed = !target.is_empty() || !unreadable.is_empty();
    std::process::exit(if failed { 1 } else { 0 });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A line invoking `pnpm test` gets flagged: the core PATTERNS match
    /// fires on a real command shape, case-insensitively.
    #[test]
    fn a_pnpm_invocation_line_matches() {
        let text = "Run `Pnpm Test` before opening a PR.\n";
        let hits = find_matches("CONTRIBUTING.md", text);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].file, "CONTRIBUTING.md");
        assert_eq!(hits[0].line, 1);
    }

    /// A line with none of the retired forms matches nothing.
    #[test]
    fn a_clean_line_matches_nothing() {
        let text = "Run `cargo test` before opening a PR.\n";
        assert!(find_matches("CONTRIBUTING.md", text).is_empty());
    }

    /// Fix round 1, important finding 1's own two seeds: `CLAUDE.md` and
    /// `tools/claude-session-start.sh` are walked (not excluded) under
    /// the new prefix list -- reproduced live at fix round 1's BASE
    /// (t4-evidence/fix1/important1-red.txt: seeding both left the old
    /// WALK_FILES-based gate at "0 unexpected").
    #[test]
    fn claude_md_and_session_start_sh_are_not_excluded() {
        assert!(!is_excluded("CLAUDE.md"));
        assert!(!is_excluded("tools/claude-session-start.sh"));
    }

    /// The reviewer's exact first seed, reproduced as a pure match: a
    /// `pnpm test` line in `CLAUDE.md`'s content is caught.
    #[test]
    fn a_seeded_pnpm_line_in_claude_md_is_caught() {
        let text = "# Project\n\nRun `pnpm test` before committing.\n";
        assert!(!find_matches("CLAUDE.md", text).is_empty());
    }

    /// The reviewer's exact second seed, reproduced as a pure match: an
    /// `npx` line in `tools/claude-session-start.sh`'s content is caught.
    #[test]
    fn a_seeded_npx_line_in_session_start_sh_is_caught() {
        let text = "#!/bin/sh\nnpx some-tool\n";
        assert!(!find_matches("tools/claude-session-start.sh", text).is_empty());
    }

    /// Every `EXCLUDED_PREFIXES` category excludes a representative real
    /// path; none of them over-reach into a sibling live-instruction path
    /// -- the pairing this module's own doc lists.
    #[test]
    fn excluded_prefixes_cover_their_own_categories_and_no_more() {
        let excluded = [
            "crates/houserules/src/main.rs",
            "knowledge/houserules.json",
            "backlog/items/kit.json",
            ".claude/rules/standing-rules.md",
            ".claude/skills/project-knowledge/SKILL.md",
            "docs/specs/2026-09-07-batch-20-phase5.md",
            "docs/plans/2026-09-07-batch-20-phase5.md",
            "docs/design.md",
            "tests/fixtures/mini/CLAUDE.md",
            "tests/goldens/render/root/CLAUDE.md",
            ".superpowers/sdd/2026-09-07-batch-20/progress.md",
            ".claude/evals/record.json",
        ];
        for path in excluded {
            assert!(is_excluded(path), "{path} must be excluded");
        }
        let not_excluded = [
            "README.md",
            "CONTRIBUTING.md",
            "SECURITY.md",
            "docs/runbook.md",
            "mise.toml",
            "CLAUDE.md",
            "tools/claude-session-start.sh",
            ".github/workflows/ci.yml",
            "template/CLAUDE.md",
            "template/knowledge/quality.json",
            "template/.claude/evals/dependency-add.json",
            "release-please-config.json",
        ];
        for path in not_excluded {
            assert!(!is_excluded(path), "{path} must not be excluded");
        }
    }

    /// Live regression, against this checkout's own real tracked-file
    /// list: every file `git ls-files` names is classified (excluded or
    /// walked) without panicking, and the reviewer's two seeds land in
    /// the walked set while a representative excluded file does not --
    /// fix round 1, important finding 1's own remedy ("add a test that
    /// fails when a tracked path is neither walked nor excluded").
    /// Read-only: lists tracked files, writes nothing
    /// (`houserules.tests-clean-scratch-dirs`).
    #[test]
    fn every_real_tracked_file_is_classified_and_key_files_land_correctly() {
        let root = repo_root();
        let tracked = tracked_files(&root);
        assert!(
            tracked.len() > 50,
            "expected a real, populated tracked-file list, got {}",
            tracked.len()
        );
        for path in &tracked {
            let _ = is_excluded(path); // must not panic for any real tracked path
        }
        assert!(tracked.iter().any(|p| p == "CLAUDE.md"));
        assert!(!is_excluded("CLAUDE.md"));
        assert!(tracked.iter().any(|p| p == "tools/claude-session-start.sh"));
        assert!(!is_excluded("tools/claude-session-start.sh"));
        assert!(tracked.iter().any(|p| p == "knowledge/houserules.json"));
        assert!(is_excluded("knowledge/houserules.json"));
    }

    /// README's "no Node" negation is excepted, not flagged.
    #[test]
    fn readme_no_node_negation_is_excepted() {
        let hit = Hit {
            file: "README.md".to_string(),
            line: 68,
            text: "toolchain, no Node. Install it through any of these channels".to_string(),
        };
        let matched = exceptions()
            .iter()
            .any(|exception| (exception.matches)(&hit, ""));
        assert!(matched, "the \"no Node\" line must be excepted");
    }

    /// README's npm-publish ruling negation is excepted, not flagged.
    #[test]
    fn readme_will_not_publish_to_npm_is_excepted() {
        let hit = Hit {
            file: "README.md".to_string(),
            line: 313,
            text: "houserules will not publish to npm or any plugin marketplace".to_string(),
        };
        let matched = exceptions()
            .iter()
            .any(|exception| (exception.matches)(&hit, ""));
        assert!(
            matched,
            "the \"will not publish to npm\" line must be excepted"
        );
    }

    /// `runbook_release_please_section_lines` bounds exactly the
    /// "release-please's release-type" section: its own heading through
    /// the line before the next `## ` heading, nothing from the sections
    /// before or after.
    #[test]
    fn runbook_release_please_section_lines_stops_at_the_next_heading() {
        let text = "# Title\n\n## release-please's release-type: X\n\nnode line one\nnode line two\n\n## Next section\n\nnode line three (must NOT be exempt)\n";
        let exempt = runbook_release_please_section_lines(text);
        assert!(exempt.contains(&3)); // the heading itself
        assert!(exempt.contains(&5));
        assert!(exempt.contains(&6));
        assert!(!exempt.contains(&8)); // "## Next section"
        assert!(!exempt.contains(&10)); // past the next heading
    }

    /// A `node`/`package.json` line inside `docs/runbook.md`'s
    /// "release-please's release-type" section is excepted; the
    /// identical text elsewhere in the same file is not -- the exception
    /// is heading-bounded, not whole-file.
    #[test]
    fn runbook_release_please_section_exception_is_heading_bounded() {
        let file_text = "# houserules runbook\n\n## release-please's release-type: rust (HR-073)\n\nnames \"node\" here\n\n## Cutting a release\n\nnames \"node\" here too\n";
        let inside = Hit {
            file: "docs/runbook.md".to_string(),
            line: 5,
            text: "names \"node\" here".to_string(),
        };
        let outside = Hit {
            file: "docs/runbook.md".to_string(),
            line: 9,
            text: "names \"node\" here too".to_string(),
        };
        let exceptions = exceptions();
        assert!(
            exceptions
                .iter()
                .any(|exception| (exception.matches)(&inside, file_text)),
            "the release-please section's own line must be excepted"
        );
        assert!(
            !exceptions
                .iter()
                .any(|exception| (exception.matches)(&outside, file_text)),
            "a line outside the release-please section must NOT be excepted"
        );
    }

    /// `template/knowledge/security-hygiene.json` and `process.json` are
    /// whole-file excepted; a sibling template knowledge file is not.
    #[test]
    fn only_the_two_named_template_knowledge_files_are_excepted() {
        let excepted = Hit {
            file: "template/knowledge/security-hygiene.json".to_string(),
            line: 1,
            text: "pnpm add --save-exact".to_string(),
        };
        let not_excepted = Hit {
            file: "template/knowledge/quality.json".to_string(),
            line: 1,
            text: "pnpm add --save-exact".to_string(),
        };
        let exceptions = exceptions();
        assert!(exceptions.iter().any(|e| (e.matches)(&excepted, "")));
        assert!(!exceptions.iter().any(|e| (e.matches)(&not_excepted, "")));
    }

    /// Every file under `template/.claude/evals/` is excepted, by prefix.
    #[test]
    fn template_evals_subtree_is_excepted_by_prefix() {
        let hit = Hit {
            file: "template/.claude/evals/dependency-add.json".to_string(),
            line: 1,
            text: "commit a package.json".to_string(),
        };
        assert!(exceptions().iter().any(|e| (e.matches)(&hit, "")));
    }
}
