//! Lint: the production code writes a file directly in one place only.
//!
//! `init` and `update` print a `git add` line for the files that they wrote
//! (`install/next_line.rs`). The line reads the record of the writes
//! (`written.rs`), so a write that does not go through `written::write` leaves
//! its file out of the line (spec 20.1). This test reads every `.rs` file
//! under `crates/houserules/src`, found by a directory walk, and fails when a
//! production line other than the home holds the spelling of a write. The
//! failure names the file and the line.
//!
//! The spellings are the entries of `std::fs`, `std::io`, and `tempfile` that
//! put bytes or a new path on disk (`SPELLINGS`). Each file that holds one in
//! production is the home or an exemption with its reason (`EXEMPT`). A file
//! that is no part of `init` or `update` and writes on its own joins the list
//! with its reason; a file that is part of them calls `written::write`.
//!
//! Production lines are the lines of a file before its test module: the first
//! `#[cfg(test)]` that is followed by an inline module (`mod tests {`). A file
//! that a `#[cfg(test)] mod <name>;` declaration names is test code as a
//! whole. An item after an inline test module is a clippy warning
//! (`items_after_test_module`, style group, on by default; CI runs
//! `cargo clippy --all-targets -- -D warnings`), so a file ends with its test
//! module. A whole-line comment holds no spelling.
//!
//! The lint reads text, not syntax. These limits are pinned by
//! `the_limits_of_the_text_form_are_pinned`: a write through an alias
//! (`use std::fs as disk;`), a bare name from a grouped import
//! (`use std::fs::{self, write};`), and a command that writes (`cp`) escape
//! it. A spelling in a string literal or after code on a line is found
//! although it writes nothing. A `#[cfg(test)]` item before the test module
//! counts as production.
//!
//! The walk fails loudly on a directory or a file it cannot read, and on a
//! test-only module with no file (`quality.gates-derive-their-scope`).

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use walkdir::WalkDir;

/// The one source file that writes a file directly, as a `/`-separated path
/// below `src`.
const HOME: &str = "written.rs";

/// The spellings of a write that the lint looks for. `File::create` also
/// finds `File::create_new`, `.persist` also finds `.persist_noclobber`, and
/// `::symlink(` does not find `is_symlink()`.
const SPELLINGS: &[&str] = &[
    "fs::write",
    "fs::copy",
    "fs::rename",
    "fs::hard_link",
    "fs::soft_link",
    "File::create",
    "File::options",
    "OpenOptions",
    "io::copy",
    "NamedTempFile",
    "tempfile::Builder",
    ".persist",
    "::symlink(",
    "::symlink_file(",
    "::symlink_dir(",
];

/// The files that hold a spelling of a write in production and are no part of
/// `init` or `update`, each with its reason, as `/`-separated paths below
/// `src`. An entry that ends with `/` is a folder.
const EXEMPT: &[(&str, &str)] = &[
    (
        "backlog/commands.rs",
        "`set` rewrites the file of a backlog item",
    ),
    (
        "bin/",
        "separate binaries (gates and dev tools): they cannot call the crate's modules",
    ),
    ("capture.rs", "`capture` writes its output file"),
    ("report_facts.rs", "`report-facts` rewrites a report"),
    ("review/bundle.rs", "`bundle` writes its `--out` file"),
    ("review/ci.rs", "`ci` writes the workflow files"),
    (
        "review/evidence.rs",
        "`evidence` writes its `--json` and `--out` files",
    ),
    ("review/record/mod.rs", "`record` writes its `--out` file"),
    ("review/route.rs", "`route` writes its `--json` file"),
    ("rules/audit.rs", "`audit` writes its `--json` file"),
    (
        "update_check.rs",
        "the update check writes its cache file in the cache folder of the user",
    ),
];

/// A production line that holds the spelling of a write.
#[derive(Debug, PartialEq, Eq)]
struct WriteLine {
    /// The `/`-separated path of the file below `src`.
    path: String,
    /// The number of the line, counted from 1.
    line: usize,
    /// The text of the line, trimmed.
    text: String,
}

/// Every `.rs` file below `src` with its text, as `/`-separated paths below
/// `src`, sorted by path.
fn source_files(src: &Path) -> Vec<(String, String)> {
    let mut files = Vec::new();
    for entry in WalkDir::new(src).sort_by_file_name() {
        let entry = entry.unwrap_or_else(|error| panic!("walk {}: {error}", src.display()));
        if !entry.file_type().is_file() || entry.path().extension().is_none_or(|ext| ext != "rs") {
            continue;
        }
        let below = entry
            .path()
            .strip_prefix(src)
            .expect("a walked path lies below its root");
        let relative = below
            .components()
            .map(|part| part.as_os_str().to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join("/");
        let text = fs::read_to_string(entry.path())
            .unwrap_or_else(|error| panic!("read {}: {error}", entry.path().display()));
        files.push((relative, text));
    }
    files
}

/// The sources of the crate.
fn crate_sources() -> Vec<(String, String)> {
    source_files(&Path::new(env!("CARGO_MANIFEST_DIR")).join("src"))
}

/// `line` without a `pub` or `pub(crate)` prefix.
fn without_visibility(line: &str) -> &str {
    line.strip_prefix("pub(crate) ")
        .or_else(|| line.strip_prefix("pub "))
        .unwrap_or(line)
}

/// The name of the module that `line` declares as a file (`mod <name>;`).
fn declared_file_module(line: &str) -> Option<&str> {
    without_visibility(line)
        .strip_prefix("mod ")?
        .strip_suffix(';')
}

/// `true` when `line` opens an inline module (`mod <name> {`).
fn opens_inline_module(line: &str) -> bool {
    without_visibility(line)
        .strip_prefix("mod ")
        .is_some_and(|rest| rest.ends_with(" {"))
}

/// The folder that holds the child modules of the file `path`: the folder of
/// `mod.rs` and `main.rs`, else the folder named like the file. A
/// `/`-separated path below `src`, empty for `src` itself.
fn module_folder(path: &str) -> String {
    let (folder, file) = path.rsplit_once('/').unwrap_or(("", path));
    if file == "mod.rs" || file == "main.rs" {
        return folder.to_string();
    }
    let stem = file.strip_suffix(".rs").unwrap_or(file);
    if folder.is_empty() {
        stem.to_string()
    } else {
        format!("{folder}/{stem}")
    }
}

/// The files that a `#[cfg(test)] mod <name>;` declaration puts under test:
/// the file of the module and every file below its folder. A declaration with
/// no file is an error that names it.
fn test_only_files(files: &[(String, String)]) -> Result<BTreeSet<String>, String> {
    let mut marked = BTreeSet::new();
    for (path, text) in files {
        let lines: Vec<&str> = text.lines().collect();
        for pair in lines.windows(2) {
            if pair[0] != "#[cfg(test)]" {
                continue;
            }
            let Some(name) = declared_file_module(pair[1]) else {
                continue;
            };
            let folder = module_folder(path);
            let module = if folder.is_empty() {
                name.to_string()
            } else {
                format!("{folder}/{name}")
            };
            let (own, own_mod) = (format!("{module}.rs"), format!("{module}/mod.rs"));
            if !files
                .iter()
                .any(|(file, _)| *file == own || *file == own_mod)
            {
                return Err(format!(
                    "{path} declares the test-only module {name}, and neither {own} nor \
                     {own_mod} exists"
                ));
            }
            let below = format!("{module}/");
            marked.extend(
                files
                    .iter()
                    .map(|(file, _)| file)
                    .filter(|file| **file == own || file.starts_with(&below))
                    .cloned(),
            );
        }
    }
    Ok(marked)
}

/// The lines of a file before its test module, each with its number counted
/// from 1: the lines before the first `#[cfg(test)]` that is followed by an
/// inline module. A file with no test module is all production.
fn production_lines(text: &str) -> Vec<(usize, &str)> {
    let lines: Vec<&str> = text.lines().collect();
    let end = lines
        .windows(2)
        .position(|pair| pair[0] == "#[cfg(test)]" && opens_inline_module(pair[1]))
        .unwrap_or(lines.len());
    lines[..end]
        .iter()
        .enumerate()
        .map(|(index, line)| (index + 1, *line))
        .collect()
}

/// The spelling of a write that `line` holds, if any. A whole-line comment
/// holds none.
fn write_spelling(line: &str) -> Option<&'static str> {
    if line.trim_start().starts_with("//") {
        return None;
    }
    SPELLINGS
        .iter()
        .find(|spelling| line.contains(**spelling))
        .copied()
}

/// Every production line in `files` that holds the spelling of a write, in
/// the order of the files and lines. The files under test are skipped.
fn production_writes(files: &[(String, String)]) -> Result<Vec<WriteLine>, String> {
    let under_test = test_only_files(files)?;
    Ok(files
        .iter()
        .filter(|(path, _)| !under_test.contains(path))
        .flat_map(|(path, text)| {
            production_lines(text)
                .into_iter()
                .filter(|(_, line)| write_spelling(line).is_some())
                .map(|(number, line)| WriteLine {
                    path: path.clone(),
                    line: number,
                    text: line.trim().to_string(),
                })
        })
        .collect())
}

/// The production writes of the crate.
fn crate_writes() -> Vec<WriteLine> {
    production_writes(&crate_sources()).unwrap_or_else(|error| panic!("{error}"))
}

/// `true` when the exemption `entry` covers the file `path`.
fn exempts(entry: &str, path: &str) -> bool {
    if entry.ends_with('/') {
        path.starts_with(entry)
    } else {
        entry == path
    }
}

/// A production line outside the home and the exemptions that holds the
/// spelling of a write fails the lint, and the failure names its file and
/// line.
#[test]
fn no_production_line_but_the_home_writes_a_file_directly() {
    let offenders: Vec<String> = crate_writes()
        .into_iter()
        .filter(|write| {
            write.path != HOME && !EXEMPT.iter().any(|(entry, _)| exempts(entry, &write.path))
        })
        .map(|write| format!("{}:{}: {}", write.path, write.line, write.text))
        .collect();

    assert!(
        offenders.is_empty(),
        "these lines write a file without `written::write`; call it, or add the file to EXEMPT \
         with its reason when no command of init or update reaches it:\n{}",
        offenders.join("\n")
    );
}

/// The lint cannot pass for want of a subject: the home holds a spelling, and
/// every exemption still holds one (an exemption that holds none is dead and
/// goes).
#[test]
fn the_home_and_every_exemption_hold_a_spelling_of_a_write() {
    let writes = crate_writes();

    assert!(
        writes.iter().any(|write| write.path == HOME),
        "{HOME} holds no spelling of a write; the lint has no home to protect"
    );
    for (entry, reason) in EXEMPT {
        assert!(
            writes.iter().any(|write| exempts(entry, &write.path)),
            "{entry} is exempt ({reason}) but holds no spelling of a write; drop the exemption"
        );
    }
}

/// Each spelling is found on a line that holds it, and every entry of
/// `SPELLINGS` is the answer for at least one of the lines. A read, a
/// removal, a folder, and `written::write` hold none.
#[test]
fn every_spelling_of_a_write_is_found_and_a_read_is_not() {
    let found = [
        ("std::fs::write(&path, text)", "fs::write"),
        ("fs::copy(&from, &to)", "fs::copy"),
        ("std::fs::rename(&from, &to)", "fs::rename"),
        ("fs::hard_link(&from, &to)", "fs::hard_link"),
        ("std::fs::soft_link(&from, &to)", "fs::soft_link"),
        ("let file = File::create(&path)?;", "File::create"),
        ("let file = fs::File::create_new(&path)?;", "File::create"),
        ("let mut options = File::options();", "File::options"),
        ("let mut options = OpenOptions::new();", "OpenOptions"),
        ("io::copy(&mut reader, &mut file)", "io::copy"),
        ("let raw = NamedTempFile::new_in(dir)?;", "NamedTempFile"),
        ("let raw = tempfile::Builder::new();", "tempfile::Builder"),
        ("file.persist(out)", ".persist"),
        ("file.persist_noclobber(out)", ".persist"),
        ("std::os::unix::fs::symlink(&from, &to)", "::symlink("),
        (
            "std::os::windows::fs::symlink_file(&from, &to)",
            "::symlink_file(",
        ),
        (
            "std::os::windows::fs::symlink_dir(&from, &to)",
            "::symlink_dir(",
        ),
    ];
    for (line, spelling) in found {
        assert_eq!(write_spelling(line), Some(spelling), "{line}");
    }
    for spelling in SPELLINGS {
        assert!(
            found.iter().any(|(_, answer)| answer == spelling),
            "no line of this test holds {spelling}"
        );
    }

    for line in [
        "let text = fs::read_to_string(&path)?;",
        "let file = File::open(&path)?;",
        "fs::create_dir_all(&folder)?;",
        "fs::remove_file(&path)?;",
        "fs::remove_dir_all(&folder)?;",
        "let metadata = fs::metadata(&path)?;",
        "let metadata = fs::symlink_metadata(&path)?;",
        "metadata.file_type().is_symlink()",
        "crate::written::write(&path, content)?;",
        "let dir = tempfile::tempdir()?;",
        "write!(out, \"text\")?;",
        "// std::fs::write(&path, text)",
        "    /// Calls `std::fs::write`.",
        "//! See `fs::copy`.",
    ] {
        assert_eq!(write_spelling(line), None, "{line}");
    }
}

/// The limits of the text form: a write that the lint does not find, and a
/// line that it finds although nothing is written.
#[test]
fn the_limits_of_the_text_form_are_pinned() {
    for line in [
        "use std::fs as disk;",
        "disk::write(&path, text)?;",
        "use std::fs::{self, write};",
        "write(&path, text)?;",
        "Command::new(\"cp\").arg(&from).arg(&to)",
    ] {
        assert_eq!(write_spelling(line), None, "{line}");
    }
    for line in ["let message = \"fs::write failed\";", "run(); // fs::write"] {
        assert!(write_spelling(line).is_some(), "{line}");
    }
}

/// The production lines end before the first test module, whatever its
/// visibility, and not before a declaration of a file module or a test-only
/// function. Their numbers count from 1.
#[test]
fn production_lines_end_before_the_first_inline_test_module() {
    let numbered = |lines: Vec<(usize, &str)>| -> Vec<usize> {
        lines.into_iter().map(|(number, _)| number).collect()
    };
    let with_tests = "fn a() {}\n\n#[cfg(test)]\nmod tests {\n    fs::write(a, b);\n}\n";
    let with_scratch =
        "fn a() {}\n#[cfg(test)]\npub(crate) mod scratch {\n}\n#[cfg(test)]\nmod tests {\n}\n";
    let file_module = "#[cfg(test)]\nmod test_support;\nfn a() {}\n";
    let test_function = "#[cfg(test)]\nfn helper() {}\nfn a() {}\n";
    let no_tests = "fn a() {}\nfn b() {}\n";

    assert_eq!(numbered(production_lines(with_tests)), [1, 2]);
    assert_eq!(numbered(production_lines(with_scratch)), [1]);
    assert_eq!(numbered(production_lines(file_module)), [1, 2, 3]);
    assert_eq!(numbered(production_lines(test_function)), [1, 2, 3]);
    assert_eq!(numbered(production_lines(no_tests)), [1, 2]);
    assert_eq!(production_lines(with_tests)[0], (1, "fn a() {}"));
}

/// Builds the files of a fixture crate from the paths and texts.
fn fixture(files: &[(&str, &str)]) -> Vec<(String, String)> {
    files
        .iter()
        .map(|(path, text)| (path.to_string(), text.to_string()))
        .collect()
}

/// A file module that a `#[cfg(test)]` declaration names is test code with
/// every file below its folder. The declaration is read in `mod.rs`, in
/// `main.rs`, and in a file named like its folder; a module with no file is
/// an error that names it.
#[test]
fn a_file_module_declared_under_test_is_test_only_with_its_folder() {
    let files = fixture(&[
        ("main.rs", "#[cfg(test)]\nmod schema_pin;\nfn main() {}\n"),
        ("schema_pin.rs", ""),
        ("a/mod.rs", "#[cfg(test)]\npub(crate) mod fixtures;\n"),
        ("a/fixtures.rs", ""),
        (
            "b.rs",
            "#[cfg(test)]\nmod helper;\n#[cfg(test)]\nmod nested;\n",
        ),
        ("b/helper.rs", ""),
        ("b/nested/mod.rs", ""),
        ("b/nested/deeper.rs", ""),
        ("b/kept.rs", ""),
        ("c.rs", "mod declared_for_production;\n"),
        ("c/declared_for_production.rs", ""),
        ("d.rs", "mod tests {\n}\n"),
    ]);

    let marked = test_only_files(&files).expect("every declaration has a file");

    assert_eq!(
        marked.into_iter().collect::<Vec<_>>(),
        [
            "a/fixtures.rs",
            "b/helper.rs",
            "b/nested/deeper.rs",
            "b/nested/mod.rs",
            "schema_pin.rs",
        ]
    );
    let missing = fixture(&[("a/mod.rs", "#[cfg(test)]\nmod gone;\n")]);
    let error = test_only_files(&missing).expect_err("a declaration with no file");
    assert!(
        error.contains("a/mod.rs") && error.contains("gone"),
        "{error}"
    );
}

/// A write in a file under test or after the test module is no production
/// write; a write before it is, with its file and line.
#[test]
fn the_production_writes_of_a_fixture_crate_name_their_files_and_lines() {
    let files = fixture(&[
        (
            "main.rs",
            "#[cfg(test)]\nmod support;\nfn put() {\n    fs::write(a, b);\n}\n\
             #[cfg(test)]\nmod tests {\n    fn t() {\n        fs::write(a, b);\n    }\n}\n",
        ),
        ("support.rs", "fn s() { fs::write(a, b); }\n"),
        (
            "other.rs",
            "// fs::write(a, b)\nfn read() { fs::read(a); }\n",
        ),
    ]);

    let writes = production_writes(&files).expect("every declaration has a file");

    assert_eq!(
        writes,
        [WriteLine {
            path: "main.rs".to_string(),
            line: 4,
            text: "fs::write(a, b);".to_string(),
        }]
    );
}
