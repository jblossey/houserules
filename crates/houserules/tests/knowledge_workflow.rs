//! Pins the install step of the seeded CI workflow,
//! `template/.github/workflows/knowledge.yml`: it installs the houserules
//! release that `.houserules.json` records, not the latest release.
//!
//! The kit files of an install stay at the version of its last
//! `houserules update`. A latest binary can run a check that those files
//! do not satisfy yet, so the gate would turn red on a release the install
//! never chose (HR-154, docs/design.md 5.88). The step has three
//! outcomes: it downloads the installer of the recorded version; it falls
//! back to the latest release, with a notice that names the reason, when
//! the stamp does not exist or has no line the read matches (see
//! `NO_VERSION_LINE`); it fails with a named line when the recorded version
//! cannot be downloaded or is no release version.
//!
//! The step reads the version with `sed`, a POSIX tool every runner has
//! (`houserules.payload-runs-on-builtins`: no JSON tool). `houserules`
//! writes `.houserules.json` itself through `serde_json`'s pretty printer
//! (`emit` in `src/emit.rs`), so the top-level `version` key is the one
//! line that starts with exactly two spaces and a quote; every nested key
//! sits deeper. The baselines map holds paths and entry ids as keys, so no
//! nested key is named `version`; the stamp `init` writes and this
//! repository's own stamp each hold exactly one line that starts with two
//! spaces and `"version"`, and `step_reads_the_stamp_that_init_writes`
//! feeds the stamp `init` writes to the step. A stamp in another layout,
//! such as a minified or tab-indented one, has no line the read matches
//! and falls back to the latest release, even when it records a version.
//!
//! The script uses three bash-only constructs, `[[ string =~ pattern ]]`,
//! `${variable//pattern/text}`, and `${variable:offset:length}`; the
//! step's `shell: bash` provides them. The `sed` forms are POSIX: `-n`, the
//! `s` command with the `p` flag, `\(` `\)` groups, and `[^"]` in a basic
//! regular expression
//! (<https://pubs.opengroup.org/onlinepubs/9799919799/utilities/sed.html>,
//! <https://pubs.opengroup.org/onlinepubs/9799919799/basedefs/V1_chap09.html>).
//!
//! This crate has no YAML dependency. `workflow_reader` reads the workflow
//! by indentation, which is enough for the fixed shape the file keeps.
//! The run tests execute the step's own script under the shell Actions
//! uses for `shell: bash` (`bash --noprofile --norc -eo pipefail {0}`, per
//! `houserules.actions-default-shell-lacks-pipefail`) against a stub
//! `curl`, so the decisions are proven by the consumer's run and not by a
//! string match. Their `PATH` holds only the stub directory, which carries
//! `curl`, `sh`, and `sed`: no `jq`, and nothing else the step could lean
//! on. The run tests are `#[cfg(unix)]`: a Windows build compiles the
//! shape pins only. A Unix host without `bash`, `sh`, or `sed` fails the
//! run tests with a message that names the tool; it never skips them.

mod workflow_reader;

use std::fs;
#[cfg(unix)]
use std::path::{Path, PathBuf};

#[cfg(unix)]
use workflow_reader::step_script;
use workflow_reader::{position_of_step_running, repo_root, step_key, step_scalar, workflow_steps};

/// The workflow path, relative to the repository root.
const WORKFLOW: &str = "template/.github/workflows/knowledge.yml";

/// The text that identifies the install step: both installer URLs end
/// with it.
const INSTALLER: &str = "houserules-installer.sh";

/// The installer URL of the latest release.
const LATEST_URL: &str =
    "https://github.com/jblossey/houserules/releases/latest/download/houserules-installer.sh";

/// The installer URL of the release that `.houserules.json` records, with
/// the shell variable the step fills.
const VERSIONED_URL_TEMPLATE: &str =
    "https://github.com/jblossey/houserules/releases/download/v$version/houserules-installer.sh";

/// The text of the workflow.
fn read_workflow() -> String {
    fs::read_to_string(repo_root().join(WORKFLOW))
        .unwrap_or_else(|error| panic!("read {WORKFLOW}: {error}"))
}

/// The one step whose `run:` holds `command`, as its raw lines.
fn step_running<'a>(steps: &[Vec<&'a str>], command: &str) -> Vec<&'a str> {
    steps[position_of_step_running(steps, command)].clone()
}

/// The install step is one `run:` step that names `shell: bash` and loads
/// no action. A `run` step with no `shell` key runs without `pipefail`
/// (`houserules.actions-default-shell-lacks-pipefail`), and the one
/// `uses:` line of the workflow stays the checkout.
#[test]
fn install_step_is_one_bash_run_step_without_an_action() {
    let raw = read_workflow();
    let steps = workflow_steps(&raw);
    let install = step_running(&steps, INSTALLER);
    assert_eq!(
        step_scalar(&install, "shell").as_deref(),
        Some("bash"),
        "the install step needs shell: bash"
    );
    let action = step_key(&install, "uses").map(|(action, _)| action);
    assert_eq!(action, None, "the install step loads no action");
    let uses: Vec<&str> = raw
        .lines()
        .filter(|line| line.contains("uses:"))
        .map(str::trim)
        .collect();
    assert_eq!(
        uses.len(),
        1,
        "the workflow loads one action, the checkout: {uses:?}"
    );
}

/// The first `MAJOR.MINOR.PATCH` literal in `text`: three runs of digits
/// joined by single dots, whatever the widths of the runs.
fn version_literal(text: &str) -> Option<&str> {
    let literal = regress::Regex::new(r"[0-9]+(\.[0-9]+){2}").expect("the pattern compiles");
    literal.find(text).map(|found| &text[found.range()])
}

/// The scan finds a version literal of any width and flags nothing that is
/// no version: the TLS option `--tlsv1.2` has two runs, the release-version
/// pattern of the step has its digit classes split by brackets, and a run
/// of dots has no digits.
#[test]
fn version_literal_scan_finds_a_run_of_any_width() {
    let found = [
        ("# pinned to 1.1.0", "1.1.0"),
        ("# pinned to 1.10.0", "1.10.0"),
        ("v2.12.3", "2.12.3"),
        ("10.0.10", "10.0.10"),
        ("see 123.456.789.0", "123.456.789"),
    ];
    for (text, literal) in found {
        assert_eq!(version_literal(text), Some(literal), "text {text:?}");
    }
    let not_found = [
        "curl --proto '=https' --tlsv1.2 -LsSf",
        "release_version='^[0-9]+\\.[0-9]+\\.[0-9]+(-[0-9A-Za-z-]+)?$'",
        "such as MAJOR.MINOR.PATCH",
        "1.2",
        "...",
        "1..2..3",
    ];
    for text in not_found {
        assert_eq!(version_literal(text), None, "text {text:?}");
    }
}

/// The workflow text holds no `MAJOR.MINOR.PATCH` literal: a release
/// rewrites nothing in `template/`, because the step reads the version
/// from `.houserules.json` when it runs. The release-version pattern line
/// of the step does not trip the scan: its digit classes are split by
/// brackets and quantifiers, so no run of digits has two dots after it.
#[test]
fn workflow_holds_no_version_literal() {
    let raw = read_workflow();
    assert_eq!(
        version_literal(&raw),
        None,
        "the workflow holds a version literal"
    );
}

/// The step carries both installer URL forms: the release the stamp
/// records and the latest release it falls back to.
#[test]
fn install_step_names_the_versioned_and_the_latest_installer_url() {
    let raw = read_workflow();
    let steps = workflow_steps(&raw);
    let install = step_running(&steps, INSTALLER);
    let run = step_scalar(&install, "run").expect("the install step has a run: key");
    for url in [LATEST_URL, VERSIONED_URL_TEMPLATE] {
        assert!(run.contains(url), "the install step lacks {url:?}");
    }
}

/// What one run of the install step left behind.
#[cfg(unix)]
struct InstallRun {
    /// Whether the script exited zero.
    succeeded: bool,
    /// Everything the script printed to stdout, where Actions reads its
    /// `::notice::` and `::error::` commands.
    stdout: String,
    /// One entry per `curl` call: its arguments joined with spaces.
    curl_calls: Vec<String>,
    /// Whether `jq` resolves on the `PATH` the script ran with.
    jq_resolves: bool,
}

#[cfg(unix)]
impl InstallRun {
    /// The stdout lines that are Actions workflow commands.
    fn commands(&self) -> Vec<&str> {
        self.stdout
            .lines()
            .filter(|line| line.starts_with("::"))
            .collect()
    }

    /// Whether the stub installer ran: the stub `curl` hands back a script
    /// that prints this line.
    fn installer_ran(&self) -> bool {
        self.stdout
            .lines()
            .any(|line| line == STUB_INSTALLER_OUTPUT)
    }
}

/// The line the stub installer prints when `sh` runs it.
#[cfg(unix)]
const STUB_INSTALLER_OUTPUT: &str = "stub installer ran";

/// The reason the step gives when the stamp does not exist.
#[cfg(unix)]
const NO_STAMP: &str = "does not exist";

/// The reason the step gives when the stamp has no line the read matches:
/// the read takes the top-level `version` from the line that starts with
/// two spaces and `"version": "`, the layout `houserules` writes, and a
/// non-empty value.
#[cfg(unix)]
const NO_VERSION_LINE: &str = "has no line that starts with two spaces and \"version\": \"<non-empty text>\", the layout houserules writes";

/// The notice the step prints when it falls back to the latest release.
/// `reason` completes the sentence that starts with `.houserules.json`.
#[cfg(unix)]
fn fallback_notice(reason: &str) -> String {
    format!(
        "::notice::.houserules.json {reason}, so this run installs the latest houserules release. \
         Run houserules update to write the version. \
         If it reports an error in the file, fix the file first."
    )
}

/// The installer URL of the release `version`.
#[cfg(unix)]
fn versioned_url(version: &str) -> String {
    format!("https://github.com/jblossey/houserules/releases/download/v{version}/{INSTALLER}")
}

/// A stamp as `houserules` writes one: `serde_json`'s pretty printer, keys
/// in insertion order, a closing newline. `value` is any JSON value.
#[cfg(unix)]
fn pretty(value: &serde_json::Value) -> String {
    format!(
        "{}\n",
        serde_json::to_string_pretty(value).expect("a JSON value serializes")
    )
}

/// The stamp `init` writes, with `version` as the recorded version: the id
/// prefix first, then the version, then a baselines map.
#[cfg(unix)]
fn stamp_with(version: &str) -> String {
    pretty(&serde_json::json!({
        "idPrefix": "WI",
        "version": version,
        "baselines": { "tools/claude-session-start.sh": "0123abcd" },
    }))
}

/// The full path of the tool `name` on this process's own `PATH`. Panics
/// with a message that names the tool when it is missing, so a run test
/// never passes without the tool it needs.
#[cfg(unix)]
fn host_tool(name: &str) -> PathBuf {
    let path = std::env::var_os("PATH").unwrap_or_default();
    std::env::split_paths(&path)
        .map(|dir| dir.join(name))
        .find(|candidate| candidate.is_file())
        .unwrap_or_else(|| {
            panic!("{name} is not on PATH. The install step tests need it; install it to run them.")
        })
}

/// Runs the install step's own script in the checkout `work_dir`.
///
/// `curl_exit` is the exit status of the stub `curl`: `"0"` hands back a
/// stub installer, any other value fails like a refused download. The
/// script's `PATH` holds only a scratch directory with the stub `curl` and
/// links to the host's `sh` and `sed`.
#[cfg(unix)]
fn run_install_step_in(work_dir: &Path, curl_exit: &str) -> InstallRun {
    use std::os::unix::fs::{PermissionsExt, symlink};
    use std::process::Command;

    const STUB_CURL: &str = r#"#!/bin/sh
echo "$*" >> "$CURL_LOG"
if [ "$STUB_CURL_EXIT" != 0 ]; then
  echo "curl: ($STUB_CURL_EXIT) stub failure" >&2
  exit "$STUB_CURL_EXIT"
fi
echo 'echo stub installer ran'
"#;

    let raw = read_workflow();
    let steps = workflow_steps(&raw);
    let script = step_script(&step_running(&steps, INSTALLER));

    let scratch = tempfile::TempDir::new().expect("create scratch dir");
    let bin_dir = scratch.path().join("bin");
    fs::create_dir(&bin_dir).expect("create stub bin dir");
    let stub = bin_dir.join("curl");
    fs::write(&stub, STUB_CURL).expect("write stub curl");
    fs::set_permissions(&stub, fs::Permissions::from_mode(0o755)).expect("make stub curl runnable");
    let sh = host_tool("sh");
    symlink(&sh, bin_dir.join("sh")).expect("link sh into the stub bin dir");
    symlink(host_tool("sed"), bin_dir.join("sed")).expect("link sed into the stub bin dir");
    let script_path = scratch.path().join("install.sh");
    fs::write(&script_path, script).expect("write the install script");
    let log_path = scratch.path().join("curl.log");
    let path = std::env::join_paths([&bin_dir]).expect("join PATH");

    let jq_resolves = Command::new(&sh)
        .args(["-c", "command -v jq"])
        .env("PATH", &path)
        .output()
        .expect("probe for jq")
        .status
        .success();
    let output = Command::new(host_tool("bash"))
        .args(["--noprofile", "--norc", "-eo", "pipefail"])
        .arg(&script_path)
        .current_dir(work_dir)
        .env("PATH", &path)
        .env("CURL_LOG", &log_path)
        .env("STUB_CURL_EXIT", curl_exit)
        .output()
        .expect("run the install script under bash");
    let curl_calls = fs::read_to_string(&log_path)
        .unwrap_or_default()
        .lines()
        .map(str::to_string)
        .collect();
    InstallRun {
        succeeded: output.status.success(),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        curl_calls,
        jq_resolves,
    }
}

/// Runs the install step in a scratch checkout whose `.houserules.json`
/// holds `stamp`, or in one with no stamp when `stamp` is `None`.
#[cfg(unix)]
fn run_install_step(stamp: Option<&str>, curl_exit: &str) -> InstallRun {
    let checkout = tempfile::TempDir::new().expect("create the scratch checkout");
    if let Some(stamp) = stamp {
        fs::write(checkout.path().join(".houserules.json"), stamp).expect("write the stamp");
    }
    run_install_step_in(checkout.path(), curl_exit)
}

/// The `curl` arguments of the step's one download, for `url`: the options
/// the step has always used, then the URL.
#[cfg(unix)]
fn curl_call(url: &str) -> String {
    format!("--proto =https --tlsv1.2 -LsSf {url}")
}

/// A stamp that records a release version downloads that release's
/// installer, runs it, and prints no workflow command, whatever the key
/// order of the stamp or its line endings. A semver prerelease version
/// counts as a release version.
#[cfg(unix)]
#[test]
fn recorded_version_installs_that_release_without_a_notice() {
    for version in ["1.1.0", "0.3.0", "1.2.0-rc.1", "1.2.0-alpha.1.beta"] {
        let version_first = pretty(&serde_json::json!({
            "version": version,
            "idPrefix": "HR",
            "overrides": ["backlog/items/general.json"],
            "baselines": { "process.tdd": "0123abcd" },
        }));
        let version_last = pretty(&serde_json::json!({
            "idPrefix": "WI",
            "baselines": {},
            "version": version,
        }));
        let stamps = [
            ("version second, as init writes it", stamp_with(version)),
            ("version first", version_first),
            ("version last, no comma", version_last),
            (
                "carriage returns",
                stamp_with(version).replace('\n', "\r\n"),
            ),
        ];
        for (layout, stamp) in stamps {
            let run = run_install_step(Some(&stamp), "0");
            let label = format!("version {version}, {layout}");
            assert!(run.succeeded, "{label}: {}", run.stdout);
            assert_eq!(
                run.curl_calls,
                [curl_call(&versioned_url(version))],
                "{label}"
            );
            assert!(run.installer_ran(), "{label}: {}", run.stdout);
            assert_eq!(run.commands(), Vec::<&str>::new(), "{label}");
        }
    }
}

/// The step reads the stamp that `init` writes, in a real git repository:
/// the writer's output fed to the step that consumes it. The step asks for
/// the installer of this crate's own version, the one `init` stamps.
#[cfg(unix)]
#[test]
fn step_reads_the_stamp_that_init_writes() {
    use std::process::Command;

    let checkout = tempfile::TempDir::new().expect("create the scratch repository");
    let git = Command::new("git")
        .args(["init", "-q"])
        .current_dir(checkout.path())
        .status()
        .expect("run git init");
    assert!(git.success(), "git init failed");
    let init = Command::new(env!("CARGO_BIN_EXE_houserules"))
        .args(["init", "--dir"])
        .arg(checkout.path())
        .output()
        .expect("run houserules init");
    assert!(
        init.status.success(),
        "init: {}",
        String::from_utf8_lossy(&init.stderr)
    );
    let version = env!("CARGO_PKG_VERSION");
    let run = run_install_step_in(checkout.path(), "0");
    assert!(run.succeeded, "{}", run.stdout);
    assert_eq!(run.curl_calls, [curl_call(&versioned_url(version))]);
    assert_eq!(run.commands(), Vec::<&str>::new());
}

/// A `version` key below the top level is never the version. A stamp whose
/// nested `version` comes before the top-level one still installs the
/// top-level version; a stamp with a nested `version` only has no line the
/// read matches and falls back to the latest release.
#[cfg(unix)]
#[test]
fn a_version_key_deeper_in_the_stamp_is_not_the_version() {
    let nested_first = pretty(&serde_json::json!({
        "baselines": { "version": "9.9.9" },
        "idPrefix": "WI",
        "version": "1.1.0",
    }));
    let run = run_install_step(Some(&nested_first), "0");
    assert!(run.succeeded, "{}", run.stdout);
    assert_eq!(run.curl_calls, [curl_call(&versioned_url("1.1.0"))]);
    assert_eq!(run.commands(), Vec::<&str>::new());

    let nested_only = pretty(&serde_json::json!({
        "idPrefix": "WI",
        "adopter": { "notes": { "version": "9.9.9" }, "version": "8.8.8" },
        "baselines": { "version": "7.7.7" },
    }));
    let run = run_install_step(Some(&nested_only), "0");
    assert!(run.succeeded, "{}", run.stdout);
    assert_eq!(run.curl_calls, [curl_call(LATEST_URL)]);
    assert_eq!(run.commands(), [fallback_notice(NO_VERSION_LINE)]);
}

/// A stamp that does not exist, or has no line the read matches, falls back
/// to the latest release and prints one notice that names the reason and
/// the way out. The step reads lines, so a stamp that is no JSON, no
/// object, or a valid stamp in a layout `houserules` does not write
/// (minified, tab-indented) has no such line, and so has a `version` that
/// is null, a number, or empty.
#[cfg(unix)]
#[test]
fn unusable_stamp_installs_the_latest_release_with_a_notice() {
    let cases: [(&str, Option<String>, &str); 11] = [
        ("no stamp", None, NO_STAMP),
        (
            "no version key",
            Some(pretty(
                &serde_json::json!({ "idPrefix": "WI", "baselines": {} }),
            )),
            NO_VERSION_LINE,
        ),
        (
            "null version",
            Some(pretty(
                &serde_json::json!({ "idPrefix": "WI", "version": null }),
            )),
            NO_VERSION_LINE,
        ),
        (
            "numeric version",
            Some(pretty(
                &serde_json::json!({ "idPrefix": "WI", "version": 5 }),
            )),
            NO_VERSION_LINE,
        ),
        ("empty version", Some(stamp_with("")), NO_VERSION_LINE),
        ("empty file", Some(String::new()), NO_VERSION_LINE),
        ("not JSON", Some("not json".to_string()), NO_VERSION_LINE),
        ("JSON array", Some("[1]".to_string()), NO_VERSION_LINE),
        (
            "JSON string",
            Some(r#""1.1.0""#.to_string()),
            NO_VERSION_LINE,
        ),
        (
            "minified stamp",
            Some(r#"{"idPrefix":"WI","version":"1.1.0"}"#.to_string()),
            NO_VERSION_LINE,
        ),
        (
            "version on a tab-indented line",
            Some("{\n\t\"version\": \"1.1.0\"\n}\n".to_string()),
            NO_VERSION_LINE,
        ),
    ];
    for (label, stamp, reason) in cases {
        let run = run_install_step(stamp.as_deref(), "0");
        assert!(run.succeeded, "{label}: {}", run.stdout);
        assert_eq!(run.curl_calls, [curl_call(LATEST_URL)], "{label}");
        assert!(run.installer_ran(), "{label}: {}", run.stdout);
        assert_eq!(run.commands(), [fallback_notice(reason)], "{label}");
    }
}

/// A recorded version with no such release fails the step: curl fails, the
/// installer never runs, and one `::error::` line names the version, the
/// URL, curl's exit status, the causes, and the ways out.
#[cfg(unix)]
#[test]
fn recorded_version_with_no_release_fails_with_a_named_error() {
    let run = run_install_step(Some(&stamp_with("9.9.9")), "22");
    let url = versioned_url("9.9.9");
    assert!(!run.succeeded, "{}", run.stdout);
    assert_eq!(run.curl_calls, [curl_call(&url)]);
    assert!(!run.installer_ran(), "{}", run.stdout);
    assert_eq!(
        run.commands(),
        [format!(
            "::error::Cannot download {url} (curl exit 22). \
             Release v9.9.9 does not exist, has no installer, or GitHub was unreachable. \
             Wait for the release, run houserules update on a released binary, or re-run the job."
        )]
    );
}

/// The fallback download failing is a failure too, with a line that names
/// the URL and the exit status and does not blame a recorded version.
#[cfg(unix)]
#[test]
fn latest_download_failure_fails_with_a_named_error() {
    let run = run_install_step(None, "22");
    assert!(!run.succeeded, "{}", run.stdout);
    assert_eq!(run.curl_calls, [curl_call(LATEST_URL)]);
    assert!(!run.installer_ran(), "{}", run.stdout);
    assert_eq!(
        run.commands(),
        [
            fallback_notice(NO_STAMP),
            format!(
                "::error::Cannot download {LATEST_URL} (curl exit 22). \
                 GitHub was unreachable or has no latest release. Re-run the job."
            ),
        ]
    );
}

/// The `::error::` line for a recorded version that is no release version:
/// `shown` is the value the step prints.
#[cfg(unix)]
fn rejected_version_error(shown: &str) -> String {
    format!(
        "::error::.houserules.json records version '{shown}', \
         which is not a release version such as MAJOR.MINOR.PATCH. \
         Fix the version, or run houserules update to restamp it."
    )
}

/// A version string that is no release version never reaches a URL: no
/// download starts, the step fails, and one `::error::` line shows the
/// value cut to 40 characters with every character outside digits,
/// letters, `.`, `+`, and `-` replaced by `?`. The line stays one line,
/// so a value from a pull request cannot start a workflow command. The
/// escaped cases are what a JSON writer makes of a quote or a newline: the
/// step reads the text of the line and never unescapes it.
#[cfg(unix)]
#[test]
fn version_that_is_no_release_version_is_rejected_before_any_download() {
    let long = "a".repeat(100);
    let cases: [(&str, &str); 14] = [
        ("1.0.0-..", "1.0.0-.."),
        ("1.0.0-.", "1.0.0-."),
        ("1.0.0-a..b", "1.0.0-a..b"),
        ("1.0.0-rc.", "1.0.0-rc."),
        ("1.1.0+build", "1.1.0+build"),
        ("1.1", "1.1"),
        ("v1.1.0", "v1.1.0"),
        ("latest", "latest"),
        ("1.1.0/../../x", "1.1.0?..?..?x"),
        ("1.1.0?x=1", "1.1.0?x?1"),
        ("1.1.0 ", "1.1.0?"),
        ("1.1.0\"x", "1.1.0?"),
        ("1.1.0\n::error::injected", "1.1.0?n??error??injected"),
        (long.as_str(), &long[..40]),
    ];
    for (version, shown) in cases {
        let run = run_install_step(Some(&stamp_with(version)), "0");
        assert!(!run.succeeded, "version {version:?}: {}", run.stdout);
        assert_eq!(run.curl_calls, Vec::<String>::new(), "version {version:?}");
        assert!(!run.installer_ran(), "version {version:?}");
        assert_eq!(
            run.commands(),
            [rejected_version_error(shown)],
            "version {version:?}"
        );
        assert_eq!(
            run.stdout.lines().count(),
            1,
            "version {version:?}: {}",
            run.stdout
        );
    }
}

/// Two top-level `version` lines make one value with a real newline in it.
/// The step rejects it, and the one error line stays one line.
#[cfg(unix)]
#[test]
fn two_version_lines_are_rejected_as_one_value_on_one_line() {
    let stamp = "{\n  \"version\": \"1.1.0\",\n  \"version\": \"::error::second\"\n}\n";
    let run = run_install_step(Some(stamp), "0");
    assert!(!run.succeeded, "{}", run.stdout);
    assert_eq!(run.curl_calls, Vec::<String>::new());
    assert_eq!(
        run.commands(),
        [rejected_version_error("1.1.0???error??second")]
    );
    assert_eq!(run.stdout.lines().count(), 1, "{}", run.stdout);
}

/// The step needs no `jq`: on a `PATH` where `jq` does not resolve, a
/// stamp that records a version still installs that release.
#[cfg(unix)]
#[test]
fn install_runs_on_a_path_without_jq() {
    let run = run_install_step(Some(&stamp_with("1.1.0")), "0");
    assert!(
        !run.jq_resolves,
        "the test PATH must not resolve jq, or this test proves nothing"
    );
    assert!(run.succeeded, "{}", run.stdout);
    assert_eq!(run.curl_calls, [curl_call(&versioned_url("1.1.0"))]);
    assert_eq!(run.commands(), Vec::<&str>::new());
}
