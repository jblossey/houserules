//! The permanent vacuous-exception gate (HR-115): every named exception
//! OR excluded-path-prefix a gate in this tree declares must excuse (or
//! exclude) at least one real, live hit -- one that excuses nothing is
//! exactly as wrong as an unexcused hit, and neither `residue-gate.rs`'s
//! nor `find-shell-tool-refs.rs`'s own report can show the difference on
//! its own: a hit-count grouping built only from the hits it actually
//! found never lists a label that found none, so a declared exception
//! matching zero real occurrences is invisible in both places at once
//! (batch 24 T3a fix round 1's `runbook_release_please_section_lines`,
//! orphaned by a heading rename with its own two unit tests still green
//! throughout, since neither recomputed the exception against a real
//! live run).
//!
//! # Scope: every exception list the tree carries
//!
//! Two DIFFERENT shapes today, both enumerated by an in-process scan of
//! `crates/houserules/src/bin/*.rs` (no subprocess spawned --
//! `houserules.platform-gated-tests`: an earlier version of this
//! enumeration spawned `grep`, ungated, on a crate CI builds and tests
//! on three OSes):
//!
//! - The exception-list function's own declaration (label, reason, a hit
//!   predicate, returning a `Vec<Exception>`): `other_bin_source_files`
//!   filtered for the two-halves pattern `exceptions_fn_pattern` builds
//!   finds exactly `residue-gate.rs` and `find-shell-tool-refs.rs`
//!   (`gates_is_exactly_the_trees_two_declared_exception_lists`), named
//!   in `GATES` below.
//! - A declared PATH-PREFIX exclusion array, `residue-gate.rs`'s own
//!   (paths never even walked, a coarser exclusion than a per-hit
//!   exception): the same in-process scan, searching for the substring
//!   `PREFIXES` this time, finds exactly one file, `residue-gate.rs`
//!   (`excluded_prefix_style_lists_are_exactly_residue_gates_own`) --
//!   brought into this gate's reach after a
//!   branch review found its `.superpowers/` entry matching zero tracked
//!   files (`.gitignore` already excludes that whole tree from
//!   `git ls-files`, so the entry excluded nothing, ever) while this
//!   gate itself, keyed only to the first shape, reported "0 unexpected
//!   vacuous".
//!
//! Neither search is a general Rust-source parse (`quality.principles`'
//! YAGNI: a structural scan for "every construct shaped like a
//! vacuousness-prone declared list" is real engineering with two known
//! instances to justify it); both are pattern searches over real file
//! text, checked against the corpus at test time so a THIRD instance of
//! either shape joining `src/bin/` fails the corresponding test's own
//! count instead of going unnoticed. `exceptions_fn_pattern` builds its
//! needle from two joined halves rather than one literal: written whole,
//! it would match this very file (and every other file quoting it, this
//! doc paragraph included), which is exactly the false extra hit the
//! test below exists to never see.
//!
//! # Design: read each gate's own printed counts, never a second copy of
//! its exceptions
//!
//! `residue-gate.rs` prints TWO sections, `find-shell-tool-refs.rs` one:
//! `-- N declared exception(s), by label --` (`declared_exception_
//! counts`, every `exceptions()` label with its real hit count for the
//! run just made, `0` included) and, `residue-gate.rs` only, `-- N
//! excluded prefix(es), by tracked-file hit count --`
//! (`excluded_prefix_counts`, every `EXCLUDED_PREFIXES` entry with its
//! real tracked-file count). This gate rebuilds each named binary fresh
//! (`cargo build --bin <name>`) and runs it, then parses whichever of
//! those sections its output carries (`parse_declared_counts`, given the
//! section's own header suffix) -- reading the source gate's own real
//! output, never a duplicated copy of its declared list that could
//! silently drift from it (`quality.gates-derive-their-scope`). This
//! package has no library target, so a `src/bin/*.rs` file cannot import
//! another's private list directly (`gen-goldens.rs`'s own module doc
//! has the fuller account of why every `src/bin/*.rs` file here keeps
//! its own copy of small helpers rather than sharing one); reading the
//! already-rebuilt gate's own stdout is this gate's answer to that
//! constraint, at the cost of one `cargo build` per named gate per run.
//! The exception-count section is REQUIRED (every `GATES` member prints
//! one, or this gate panics naming the format break); the excluded-
//! prefix section is OPTIONAL (only `residue-gate.rs` prints one today,
//! and a gate that never grows one is not an error).
//!
//! # Sanctioning a known defect
//!
//! `SANCTIONED` is empty on a clean tree: every gate's own declared
//! exceptions excuse at least one real, live hit right now, so nothing
//! needs sanctioning. A future fix round that finds a gate carrying a
//! newly-dead exception, but is not itself the round that removes it, may
//! add a `SanctionedVacuous` entry here (`gate`, `kind`, the exact
//! `label`, and `pointer`: the backlog id that owns the removal) so this
//! checker ships enforced without going red on an already-tracked defect.
//!
//! A sanction is never a standing exemption from this gate's own rule: it
//! is `stale` -- and fails the gate exactly like an unexpected vacuous
//! exception -- the moment either side of the fact it names stops being
//! true: the sanctioned label matches a real hit again (the fix shipped
//! and the count moved off zero, so the sanction now excuses something
//! that no longer needs excusing), or the label vanishes from its gate's
//! own declared list entirely (the fix removed the exception outright, so
//! the sanction now names nothing). Either way the stale entry is removed
//! from `SANCTIONED` in the same change: an exception list for THIS
//! checker would otherwise rot exactly the way this gate exists to catch
//! it rotting elsewhere -- an exception list for the exception checker
//! must itself be non-vacuous by construction.
//! `sanctioned_gates_are_declared` (test-only, no `main` step calls it)
//! pins one further way this list could rot unnoticed: a `SANCTIONED`
//! entry naming a gate outside `GATES` is never examined by `classify`
//! at all (its own per-gate loop filters on `s.gate == gate`), so a
//! renamed or removed gate would turn a real sanction into a silent dead
//! entry inside the very list this rule requires to be non-vacuous. One
//! test runs it against the real `SANCTIONED` (empty on a clean tree, so
//! that call alone proves nothing); a second, separate test proves it
//! rejects a synthetic out-of-`GATES` entry -- the same synthetic-
//! fixture pattern `classify`'s own tests use so the guard's own test
//! suite stays meaningful independent of whatever `SANCTIONED` currently
//! holds.
//!
//! Usage: `cargo run --quiet --bin vacuous-exception-gate`. Prints every
//! gate's own declared-exception (and, where present, excluded-prefix)
//! counts, then any unexpected vacuous entry and any stale sanction,
//! then a summary line; exits 1 if either list is non-empty, 0
//! otherwise.

use std::path::{Path, PathBuf};
use std::process::Command;

/// The gate binaries this checker inspects -- this module's own doc names
/// the enumeration that bounds the claim this is *every* exception list
/// the tree carries.
const GATES: [&str; 2] = ["residue-gate", "find-shell-tool-refs"];

/// Which of a gate's own printed sections a count was read from, so
/// every finding this checker files says which kind of list it names --
/// "residue-gate: crates/" alone would not say whether `crates/` is a
/// declared exception label or an excluded path prefix.
#[derive(Clone, Copy, PartialEq, Eq)]
enum ListKind {
    Exception,
    ExcludedPrefix,
}

impl ListKind {
    /// The header suffix `parse_declared_counts` looks for, and whether
    /// its absence from a gate's output is an error (every `GATES`
    /// member must print an exception-count section) or simply "this
    /// gate has none" (an excluded-prefix section is optional).
    fn header_suffix(self) -> &'static str {
        match self {
            ListKind::Exception => " declared exception(s), by label --",
            ListKind::ExcludedPrefix => " excluded prefix(es), by tracked-file hit count --",
        }
    }

    /// The noun a finding names this list's entries with.
    fn noun(self) -> &'static str {
        match self {
            ListKind::Exception => "exception",
            ListKind::ExcludedPrefix => "excluded prefix",
        }
    }
}

/// One exception (or excluded prefix) sanctioned to stay vacuous until a
/// named future task removes it -- `pointer` is the backlog id that owns
/// the removal, named so a reader (and `classify`'s own staleness check)
/// can find why.
struct SanctionedVacuous {
    gate: &'static str,
    kind: ListKind,
    label: &'static str,
    pointer: &'static str,
}

/// This checker's own exception list -- empty on a clean tree. This
/// module's own doc section "Sanctioning a known defect" has the full
/// account of when an entry belongs here and what makes one stale.
const SANCTIONED: [SanctionedVacuous; 0] = [];

/// This checkout's repository root, resolved at compile time -- every
/// other `src/bin/*.rs` file's own copy of this helper.
fn repo_root() -> PathBuf {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    root.canonicalize()
        .unwrap_or_else(|error| panic!("canonicalize {}: {error}", root.display()))
}

/// Rebuilds the `name` binary fresh (so this always reads what the CURRENT
/// source produces, never a stale artifact -- `gen-goldens.rs`'s own
/// `build_houserules` states the identical reasoning) and returns its
/// captured stdout from one run with no arguments.
fn run_gate(root: &Path, name: &str) -> String {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
    let status = Command::new(&cargo)
        .args(["build", "--quiet", "--bin", name])
        .current_dir(root)
        .status()
        .unwrap_or_else(|error| panic!("run cargo build --bin {name}: {error}"));
    assert!(status.success(), "cargo build --bin {name} failed");
    let suffix = std::env::consts::EXE_SUFFIX;
    let exe = root.join(format!("target/debug/{name}{suffix}"));
    let output = Command::new(&exe)
        .output()
        .unwrap_or_else(|error| panic!("run {}: {error}", exe.display()));
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// Parses one gate's own `-- N <noun>, by ... --` section (`header_suffix`
/// names which one) from its stdout: every following `<label>: <count>
/// hit(s)` line up to the first blank line, in printed order. `None` when
/// the header itself is absent. A label may itself contain `": "`
/// (several real labels do, e.g. `README.md: "no Node" toolchain
/// summary`), so each line is split on its LAST `": "` rather than its
/// first, which the fixed `" hit(s)"` suffix and all-digit count make
/// unambiguous.
fn parse_declared_counts(stdout: &str, header_suffix: &str) -> Option<Vec<(String, usize)>> {
    let mut lines = stdout.lines();
    let found_header = lines
        .by_ref()
        .any(|line| line.starts_with("-- ") && line.ends_with(header_suffix));
    if !found_header {
        return None;
    }
    let mut counts = Vec::new();
    for line in lines {
        if line.is_empty() {
            break;
        }
        let Some(rest) = line.strip_suffix(" hit(s)") else {
            break;
        };
        let Some(separator) = rest.rfind(": ") else {
            break;
        };
        let Ok(count) = rest[separator + 2..].parse::<usize>() else {
            break;
        };
        counts.push((rest[..separator].to_string(), count));
    }
    Some(counts)
}

/// One gate's classification against `SANCTIONED`: every declared label
/// with a zero count and no sanction (`unexpected_vacuous`, this gate's
/// own failure condition), every zero-count label that IS sanctioned
/// (`pending`, informational only), and every sanction that no longer
/// describes a real, current defect (`stale_sanctions`, this gate's
/// OTHER failure condition -- this module's own doc, "The sanctioned-
/// until-T4 exceptions", names the two ways a sanction goes stale).
struct GateReport {
    unexpected_vacuous: Vec<String>,
    stale_sanctions: Vec<String>,
    pending: Vec<String>,
}

/// Classifies `gate`'s own `counts` for list `kind`
/// (`parse_declared_counts`'s output) against `sanctioned` -- `main`
/// passes the production `SANCTIONED`; tests pass their own synthetic
/// list, so this stays a pure function of its arguments rather than
/// coupling every test to whatever `SANCTIONED` currently holds.
fn classify(
    gate: &'static str,
    kind: ListKind,
    counts: &[(String, usize)],
    sanctioned: &[SanctionedVacuous],
) -> GateReport {
    let mut unexpected_vacuous = Vec::new();
    let mut stale_sanctions = Vec::new();
    let mut pending = Vec::new();
    let noun = kind.noun();
    for (label, count) in counts {
        let sanction = sanctioned
            .iter()
            .find(|s| s.gate == gate && s.kind == kind && s.label == label);
        match (*count, sanction) {
            (0, None) => unexpected_vacuous.push(format!("{gate} {noun}: {label}")),
            (0, Some(s)) => pending.push(format!(
                "{gate} {noun}: {label} (sanctioned pending {})",
                s.pointer
            )),
            (n, Some(s)) => stale_sanctions.push(format!(
                "{gate} {noun}: {label} now matches {n} real hit(s) -- the {} sanction is \
                 stale, remove it",
                s.pointer
            )),
            (_, None) => {}
        }
    }
    for sanction in sanctioned
        .iter()
        .filter(|s| s.gate == gate && s.kind == kind)
    {
        if !counts.iter().any(|(label, _)| label == sanction.label) {
            stale_sanctions.push(format!(
                "{gate} {noun}: no longer declares \"{}\" -- the {} sanction is stale, remove it",
                sanction.label, sanction.pointer
            ));
        }
    }
    GateReport {
        unexpected_vacuous,
        stale_sanctions,
        pending,
    }
}

/// Runs every `GATES` binary fresh, classifies each one's own declared-
/// exception counts (required) and excluded-prefix counts (optional,
/// where printed) against `SANCTIONED`, prints all of it plus the
/// summary line, and exits 1 if any unexpected vacuous entry or any
/// stale sanction remains (0 otherwise) -- this module's own doc has the
/// full scope and design account.
fn main() {
    let root = repo_root();
    let mut unexpected_vacuous = Vec::new();
    let mut stale_sanctions = Vec::new();
    let mut pending = Vec::new();
    let mut total_declared = 0usize;

    for gate in GATES {
        let stdout = run_gate(&root, gate);

        let exception_counts = parse_declared_counts(&stdout, ListKind::Exception.header_suffix())
            .unwrap_or_else(|| {
                panic!(
                    "{gate}: no \"declared exception(s), by label\" section in its own \
                         output -- did its report format change?"
                )
            });
        total_declared += exception_counts.len();
        println!(
            "-- {gate}: {} declared exception(s) --",
            exception_counts.len()
        );
        for (label, count) in &exception_counts {
            println!("  {label}: {count} hit(s)");
        }
        let report = classify(gate, ListKind::Exception, &exception_counts, &SANCTIONED);
        unexpected_vacuous.extend(report.unexpected_vacuous);
        stale_sanctions.extend(report.stale_sanctions);
        pending.extend(report.pending);

        if let Some(prefix_counts) =
            parse_declared_counts(&stdout, ListKind::ExcludedPrefix.header_suffix())
        {
            total_declared += prefix_counts.len();
            println!("-- {gate}: {} excluded prefix(es) --", prefix_counts.len());
            for (label, count) in &prefix_counts {
                println!("  {label}: {count} hit(s)");
            }
            let report = classify(gate, ListKind::ExcludedPrefix, &prefix_counts, &SANCTIONED);
            unexpected_vacuous.extend(report.unexpected_vacuous);
            stale_sanctions.extend(report.stale_sanctions);
            pending.extend(report.pending);
        }
    }

    if !pending.is_empty() {
        println!(
            "\n-- {} sanctioned vacuous entry(ies) (pending removal) --",
            pending.len()
        );
        for entry in &pending {
            println!("  {entry}");
        }
    }
    if !unexpected_vacuous.is_empty() {
        println!(
            "\n-- {} unexpected vacuous entry(ies) --",
            unexpected_vacuous.len()
        );
        for entry in &unexpected_vacuous {
            println!("  {entry}");
        }
    }
    if !stale_sanctions.is_empty() {
        println!("\n-- {} stale sanction(s) --", stale_sanctions.len());
        for entry in &stale_sanctions {
            println!("  {entry}");
        }
    }

    println!(
        "\nsummary: {} gate(s), {} declared entry(ies), {} sanctioned pending, \
         {} unexpected vacuous, {} stale sanction(s)",
        GATES.len(),
        total_declared,
        pending.len(),
        unexpected_vacuous.len(),
        stale_sanctions.len(),
    );
    let failed = !unexpected_vacuous.is_empty() || !stale_sanctions.is_empty();
    std::process::exit(i32::from(failed));
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    /// The exception-list function's own declaration text, built from two
    /// joined halves so this file's OWN source never contains it whole --
    /// this module's own doc, "Scope: every exception list the tree
    /// carries", explains why a literal copy would defeat the enumeration
    /// below by matching itself.
    fn exceptions_fn_pattern() -> String {
        format!("{}{}", "fn exceptions", "()")
    }

    /// Every `.rs` file directly under `crates/houserules/src/bin/`, as
    /// (repo-relative path, file text) pairs -- an in-process directory
    /// listing, the replacement for a `grep -rl` subprocess spawn this
    /// module used to make (`houserules.platform-gated-tests`: an
    /// ungated spawn on a crate CI builds and tests on `windows-latest`
    /// and `macos-latest` too). `src/bin/` is flat (no subdirectories),
    /// so a plain `read_dir` is exactly `-r`'s own reach here; no
    /// `walkdir` needed for one directory level. Test-only: this
    /// module's own enumeration tests are its only callers.
    fn bin_source_files(root: &Path) -> Vec<(String, String)> {
        let dir = root.join("crates/houserules/src/bin");
        let entries =
            fs::read_dir(&dir).unwrap_or_else(|error| panic!("read {}: {error}", dir.display()));
        let mut files = Vec::new();
        for entry in entries {
            let entry = entry.unwrap_or_else(|error| {
                panic!("read a dir entry under {}: {error}", dir.display())
            });
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("rs") {
                continue;
            }
            let text = fs::read_to_string(&path)
                .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
            files.push((
                format!(
                    "crates/houserules/src/bin/{}",
                    entry.file_name().to_string_lossy()
                ),
                text,
            ));
        }
        files
    }

    /// `bin_source_files`, excluding THIS file: `vacuous-exception-
    /// gate.rs` is the checker, never a subject of its own enumeration,
    /// and its own module doc quotes both patterns the two tests below
    /// search for in plain prose (`fn exceptions()`, `PREFIXES`), which
    /// would otherwise self-match as a false third hit in each.
    fn other_bin_source_files(root: &Path) -> Vec<(String, String)> {
        bin_source_files(root)
            .into_iter()
            .filter(|(path, _)| !path.ends_with("vacuous-exception-gate.rs"))
            .collect()
    }

    /// `GATES` names exactly the tree's own two declared-exception-list
    /// definitions -- this module's own doc names the rerunnable, in-
    /// process enumeration this pins.
    #[test]
    fn gates_is_exactly_the_trees_two_declared_exception_lists() {
        let mut files: Vec<String> = other_bin_source_files(&repo_root())
            .into_iter()
            .filter(|(_, text)| text.contains(&exceptions_fn_pattern()))
            .map(|(path, _)| path)
            .collect();
        files.sort();
        assert_eq!(
            files,
            vec![
                "crates/houserules/src/bin/find-shell-tool-refs.rs".to_string(),
                "crates/houserules/src/bin/residue-gate.rs".to_string(),
            ]
        );
        assert_eq!(GATES.len(), files.len());
    }

    /// The excluded-path-prefix shape (`residue-gate.rs`'s own
    /// `EXCLUDED_PREFIXES`) appears in exactly one `src/bin/*.rs` file --
    /// the second enumeration this module's own doc names, proving there
    /// is no THIRD gate carrying this shape unchecked.
    #[test]
    fn excluded_prefix_style_lists_are_exactly_residue_gates_own() {
        let mut files: Vec<String> = other_bin_source_files(&repo_root())
            .into_iter()
            .filter(|(_, text)| text.contains("PREFIXES"))
            .map(|(path, _)| path)
            .collect();
        files.sort();
        assert_eq!(
            files,
            vec!["crates/houserules/src/bin/residue-gate.rs".to_string()]
        );
    }

    /// `true` when every entry in `sanctioned` names a gate present in
    /// `gates` -- module doc, "Sanctioning a known defect", has the full
    /// account of why a sanction naming a gate outside `gates` is a
    /// silent dead entry. Takes `sanctioned`/`gates` as parameters,
    /// mirroring `classify`'s own parameterization, so this stays
    /// checkable against a synthetic fixture independent of whatever the
    /// production `SANCTIONED` currently holds. Test-only: `main` never
    /// calls this (there is no runtime step that walks `SANCTIONED`
    /// looking for this shape of defect), so it lives here rather than
    /// at module scope, matching this file's own `bin_source_files` and
    /// `exceptions_fn_pattern`.
    fn sanctioned_gates_are_declared(sanctioned: &[SanctionedVacuous], gates: &[&str]) -> bool {
        sanctioned.iter().all(|s| gates.contains(&s.gate))
    }

    /// The production `SANCTIONED` (empty on a clean tree) trivially
    /// satisfies `sanctioned_gates_are_declared` -- this call alone
    /// proves nothing about the check's own correctness, which is why
    /// `sanctioned_gates_are_declared_rejects_a_gate_outside_gates`,
    /// below, exists.
    #[test]
    fn sanctioned_gates_are_all_declared_in_gates() {
        assert!(sanctioned_gates_are_declared(&SANCTIONED, &GATES));
    }

    /// A sanction naming a gate outside `GATES` is unreachable in
    /// `classify` (its own per-gate loop filters on `s.gate == gate`),
    /// which would let a renamed or removed gate turn a real sanction
    /// into a silent dead entry inside the exception list for the
    /// exception checker itself -- proven here against a synthetic
    /// fixture, since the production `SANCTIONED` is empty on a clean
    /// tree and cannot exercise this path on its own.
    #[test]
    fn sanctioned_gates_are_declared_rejects_a_gate_outside_gates() {
        let sanctioned = [SanctionedVacuous {
            gate: "not-a-real-gate",
            kind: ListKind::Exception,
            label: "whatever",
            pointer: "TEST-1",
        }];
        assert!(!sanctioned_gates_are_declared(&sanctioned, &GATES));
    }

    /// A well-formed declared-exception section parses in printed order,
    /// stopping at the blank line that follows it.
    #[test]
    fn parse_declared_counts_reads_the_section_in_order() {
        let stdout = "some earlier line\n\n\
            -- 3 declared exception(s), by label --\n\
            alpha: 1 hit(s)\n\
            beta: 0 hit(s)\n\
            \n\
            summary: ignored\n";
        assert_eq!(
            parse_declared_counts(stdout, ListKind::Exception.header_suffix()),
            Some(vec![("alpha".to_string(), 1), ("beta".to_string(), 0),])
        );
    }

    /// A label that itself contains `": "` (a real shape, e.g. `README.md:
    /// "no Node" toolchain summary`) still parses correctly: the split is
    /// on the LAST `": "`, immediately before the digits.
    #[test]
    fn parse_declared_counts_handles_a_label_containing_its_own_colon() {
        let stdout = "-- 1 declared exception(s), by label --\n\
            README.md: \"no Node\" toolchain summary: 1 hit(s)\n";
        assert_eq!(
            parse_declared_counts(stdout, ListKind::Exception.header_suffix()),
            Some(vec![(
                "README.md: \"no Node\" toolchain summary".to_string(),
                1
            )])
        );
    }

    /// Stdout with no declared-exception header at all is `None`, distinct
    /// from a header found with zero entries under it.
    #[test]
    fn parse_declared_counts_is_none_without_the_header() {
        assert_eq!(
            parse_declared_counts(
                "nothing relevant here\n",
                ListKind::Exception.header_suffix()
            ),
            None
        );
    }

    /// A header found with nothing under it (a gate declaring zero
    /// exceptions) is `Some(vec![])`, not `None`.
    #[test]
    fn parse_declared_counts_is_some_empty_for_a_header_with_no_entries() {
        let stdout = "-- 0 declared exception(s), by label --\n\nsummary: x\n";
        assert_eq!(
            parse_declared_counts(stdout, ListKind::Exception.header_suffix()),
            Some(Vec::new())
        );
    }

    /// The excluded-prefix header is a DIFFERENT suffix, parsed the same
    /// way once given its own header text.
    #[test]
    fn parse_declared_counts_reads_the_excluded_prefix_section_given_its_own_header() {
        let stdout = "-- 2 excluded prefix(es), by tracked-file hit count --\n\
            crates/: 40 hit(s)\n\
            .superpowers/: 0 hit(s)\n";
        assert_eq!(
            parse_declared_counts(stdout, ListKind::ExcludedPrefix.header_suffix()),
            Some(vec![
                ("crates/".to_string(), 40),
                (".superpowers/".to_string(), 0),
            ])
        );
    }

    /// A zero-count label with no matching sanction is unexpected -- this
    /// gate's own core failure condition.
    #[test]
    fn classify_flags_an_unsanctioned_zero_count_as_unexpected() {
        let counts = vec![("some-new-exception".to_string(), 0)];
        let report = classify("residue-gate", ListKind::Exception, &counts, &[]);
        assert_eq!(
            report.unexpected_vacuous,
            vec!["residue-gate exception: some-new-exception".to_string()]
        );
        assert!(report.stale_sanctions.is_empty());
        assert!(report.pending.is_empty());
    }

    /// The excluded-prefix kind is classified identically, and a zero
    /// count there is unexpected too -- the exact live incident
    /// (`.superpowers/`, HR-115's own review finding) this classification
    /// path exists to catch.
    #[test]
    fn classify_flags_an_unsanctioned_zero_count_excluded_prefix_as_unexpected() {
        let counts = vec![(".superpowers/".to_string(), 0)];
        let report = classify("residue-gate", ListKind::ExcludedPrefix, &counts, &[]);
        assert_eq!(
            report.unexpected_vacuous,
            vec!["residue-gate excluded prefix: .superpowers/".to_string()]
        );
    }

    /// Two synthetic sanctions, built locally rather than read from the
    /// production `SANCTIONED` -- empty on a clean tree, per this
    /// module's own doc -- so `classify`'s pending/stale logic stays
    /// under test independent of whether any real sanction currently
    /// exists.
    fn two_synthetic_sanctions() -> Vec<SanctionedVacuous> {
        vec![
            SanctionedVacuous {
                gate: "find-shell-tool-refs",
                kind: ListKind::Exception,
                label: "alpha",
                pointer: "TEST-1",
            },
            SanctionedVacuous {
                gate: "find-shell-tool-refs",
                kind: ListKind::Exception,
                label: "beta",
                pointer: "TEST-1",
            },
        ]
    }

    /// A zero-count label that IS sanctioned is `pending`, not a failure --
    /// `counts` here carries both synthetic sanctions at zero, neither
    /// omitted, so this does not also trip the "no longer declares"
    /// staleness check.
    #[test]
    fn classify_reports_a_sanctioned_zero_count_as_pending() {
        let sanctioned = two_synthetic_sanctions();
        let counts = vec![
            ("alpha".to_string(), 0),
            ("beta".to_string(), 0),
            (
                "crates/houserules/src/install.rs (RETIRED's own doc/test only)".to_string(),
                4,
            ),
        ];
        let report = classify(
            "find-shell-tool-refs",
            ListKind::Exception,
            &counts,
            &sanctioned,
        );
        assert!(report.unexpected_vacuous.is_empty());
        assert!(report.stale_sanctions.is_empty());
        assert_eq!(report.pending.len(), 2, "{:?}", report.pending);
        assert!(report.pending.iter().all(|entry| entry.contains("TEST-1")));
    }

    /// A sanctioned label that now matches a real hit is a stale sanction:
    /// the defect the sanction excused no longer holds. The OTHER
    /// sanctioned label stays at zero here, so it is `pending`, not
    /// stale -- the two are judged independently.
    #[test]
    fn classify_flags_a_sanctioned_label_with_a_real_hit_as_stale() {
        let sanctioned = two_synthetic_sanctions();
        let counts = vec![("alpha".to_string(), 2), ("beta".to_string(), 0)];
        let report = classify(
            "find-shell-tool-refs",
            ListKind::Exception,
            &counts,
            &sanctioned,
        );
        assert!(report.unexpected_vacuous.is_empty());
        assert_eq!(report.pending.len(), 1, "{:?}", report.pending);
        assert_eq!(
            report.stale_sanctions.len(),
            1,
            "{:?}",
            report.stale_sanctions
        );
        assert!(report.stale_sanctions[0].contains("2 real hit"));
    }

    /// A sanctioned label absent from the gate's own live counts entirely
    /// (the gate stopped declaring it) is also a stale sanction: the
    /// sanctioned list must shrink in the same change.
    #[test]
    fn classify_flags_a_sanction_whose_label_the_gate_no_longer_declares_as_stale() {
        let sanctioned = two_synthetic_sanctions();
        let counts: Vec<(String, usize)> = Vec::new();
        let report = classify(
            "find-shell-tool-refs",
            ListKind::Exception,
            &counts,
            &sanctioned,
        );
        assert!(report.unexpected_vacuous.is_empty());
        assert!(report.pending.is_empty());
        assert_eq!(
            report.stale_sanctions.len(),
            2,
            "{:?}",
            report.stale_sanctions
        );
        assert!(
            report
                .stale_sanctions
                .iter()
                .all(|entry| entry.contains("no longer declares"))
        );
    }

    /// A non-zero, unsanctioned count is unremarkable: it lands in none of
    /// the three lists.
    #[test]
    fn classify_ignores_a_healthy_nonzero_unsanctioned_count() {
        let counts = vec![("README.md: \"no Node\" toolchain summary".to_string(), 1)];
        let report = classify("residue-gate", ListKind::Exception, &counts, &[]);
        assert!(report.unexpected_vacuous.is_empty());
        assert!(report.stale_sanctions.is_empty());
        assert!(report.pending.is_empty());
    }

    /// The production `SANCTIONED` list is empty on a clean tree (this
    /// module's own doc, "Sanctioning a known defect"): every gate's own
    /// declared exceptions currently excuse at least one real hit, so
    /// nothing needs sanctioning.
    #[test]
    fn sanctioned_is_empty_on_a_clean_tree() {
        assert!(SANCTIONED.is_empty());
    }
}
