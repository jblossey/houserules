//! The install step of both units: the version, the minimum version, and the sources of the binary.

use super::*;

/// The text of an installer that writes a binary, which prints
/// `houserules <version>`, into the directory that the installer is told to use.
fn installer(version: &str) -> String {
    format!(
        "mkdir -p \"$HOUSERULES_UNMANAGED_INSTALL\"\n\
         printf '#!/bin/sh\\necho \"houserules {version}\"\\n' > \"$HOUSERULES_UNMANAGED_INSTALL/houserules\"\n\
         chmod +x \"$HOUSERULES_UNMANAGED_INSTALL/houserules\"\n"
    )
}

/// The URL path of the installer of the release `version`.
fn installer_path(version: &str) -> String {
    format!("/jblossey/houserules/releases/download/v{version}/houserules-installer.sh")
}

/// A program that prints `houserules <version>` for `--version` and records its
/// other calls.
fn binary(version: &str) -> String {
    format!(
        "#!/bin/sh\nif [ \"$1\" = --version ]; then echo 'houserules {version}'; exit 0; fi\n\
         echo \"$*\" >> \"$STUB_DIR/houserules.log\"\n"
    )
}

/// Runs the install step of the GitHub job `role` with the source `source`, the
/// version `version`, and the extra step variables `extra`.
fn github_install(
    sandbox: &Sandbox,
    role: &str,
    source: &str,
    version: &str,
    extra: &[(&str, &str)],
) -> Output {
    let unit = github();
    let github_path = text(&sandbox.temp().join("github_path"));
    let mut pairs = vec![
        ("HOUSERULES_SOURCE", source),
        ("HOUSERULES_VERSION", version),
        ("GITHUB_PATH", github_path.as_str()),
    ];
    pairs.extend_from_slice(extra);
    sandbox.run_bash(
        &unit.step_script(role, "Install houserules"),
        env_of(&unit, role, &pairs),
    )
}

/// Spec 10.2: a job that installs `release` with no `version` fails with one
/// line and downloads nothing.
#[test]
fn a_release_install_without_a_version_fails_with_the_line_of_spec_10_2() {
    for role in ["gates", "floor", "record"] {
        let sandbox = Sandbox::new();
        let output = github_install(&sandbox, role, "release", "", &[]);
        assert_eq!(output.status.code(), Some(1), "{role}: {}", stdout(&output));
        assert_eq!(
            stdout(&output),
            format!("::error::{VERSION_REQUIRED}\n"),
            "{role}"
        );
        assert!(sandbox.calls().is_empty(), "{role}: nothing was downloaded");
    }
}

/// Spec 10.6: a release install downloads the installer of the version, puts
/// the binary on the PATH of later steps, and checks the minimum version.
#[test]
fn a_release_install_runs_the_installer_of_the_version() {
    for role in ["gates", "floor", "record"] {
        let sandbox = Sandbox::new();
        sandbox.reply("GET", &installer_path("2.0.0"), &installer("2.0.0"));
        let output = github_install(&sandbox, role, "release", "2.0.0", &[]);
        assert_eq!(output.status.code(), Some(0), "{role}: {}", stdout(&output));
        let calls = sandbox.calls();
        assert_eq!(calls.len(), 1, "{role}: {calls:?}");
        assert!(
            calls[0].ends_with("https://github.com/jblossey/houserules/releases/download/v2.0.0/houserules-installer.sh"),
            "{role}: {calls:?}"
        );
        let bin = sandbox.temp().join("houserules-bin");
        assert!(
            bin.join("houserules").is_file(),
            "{role}: the binary is installed"
        );
        let github_path =
            fs::read_to_string(sandbox.temp().join("github_path")).expect("GITHUB_PATH");
        assert_eq!(github_path, format!("{}\n", text(&bin)), "{role}");
    }
}

/// Spec 10.6: a `release` binary older than the minimum fails with the line
/// of the spec; the same or a newer binary passes, the comparison is numeric,
/// and a pre-release of the minimum is older.
#[test]
fn a_release_binary_older_than_the_minimum_fails_with_the_line_of_spec_10_6() {
    for (found, passes) in [
        ("1.4.0", false),
        ("1.99.99", false),
        ("2.0.0-rc.1", false),
        ("2.0.0", true),
        ("2.0.1", true),
        ("2.1.0", true),
        ("3.0.0-rc.1", true),
        ("10.0.0", true),
    ] {
        let sandbox = Sandbox::new();
        sandbox.reply("GET", &installer_path(found), &installer(found));
        let output = github_install(&sandbox, "gates", "release", found, &[]);
        if passes {
            assert_eq!(
                output.status.code(),
                Some(0),
                "{found}: {}",
                stdout(&output)
            );
            assert_eq!(stdout(&output), "", "{found}");
        } else {
            assert_eq!(output.status.code(), Some(1), "{found}");
            assert_eq!(
                stdout(&output),
                format!(
                    "::error::houserules {found} is older than 2.0.0, which this unit needs; raise the version input\n"
                ),
                "{found}"
            );
        }
    }
}

/// A version that is no release version never reaches the URL.
#[test]
fn a_version_that_is_no_release_version_is_refused_before_the_download() {
    for version in ["1.0", "v2.0.0", "2.0.0;echo", "latest", "2.0.0 "] {
        let sandbox = Sandbox::new();
        let output = github_install(&sandbox, "gates", "release", version, &[]);
        assert_eq!(output.status.code(), Some(1), "{version:?}");
        assert!(
            stdout(&output).contains("is not a release version such as MAJOR.MINOR.PATCH"),
            "{version:?}: {}",
            stdout(&output)
        );
        assert!(
            sandbox.calls().is_empty(),
            "{version:?}: nothing was downloaded"
        );
    }
}

/// Spec 10.6: `install: none`, `floor-install: runner:<path>`, and
/// `floor-install: default-branch` bring a binary that names no release, so they
/// run no minimum check and download nothing.
#[test]
fn none_runner_and_default_branch_run_no_minimum_check() {
    // install none: the setup command put an old binary on the PATH.
    let sandbox = Sandbox::new();
    sandbox.install_program("houserules", &binary("1.4.0"));
    let output = github_install(&sandbox, "gates", "none", "", &[]);
    assert_eq!(output.status.code(), Some(0), "{}", stdout(&output));
    assert_eq!(stdout(&output), "");
    assert!(sandbox.calls().is_empty());

    // floor-install runner:<path>: an executable of the runner image.
    let sandbox = Sandbox::new();
    let runner = sandbox.path().join("runner-image");
    fs::create_dir_all(&runner).expect("create the runner folder");
    let program = runner.join("houserules");
    fs::write(&program, binary("1.4.0")).expect("write the runner binary");
    fs::set_permissions(&program, fs::Permissions::from_mode(0o755)).expect("make it executable");
    let source = format!("runner:{}", text(&program));
    let output = github_install(&sandbox, "floor", &source, "", &[]);
    assert_eq!(output.status.code(), Some(0), "{}", stdout(&output));
    assert_eq!(stdout(&output), "");
    assert!(sandbox.calls().is_empty());
    let link = sandbox.temp().join("houserules-bin").join("houserules");
    assert_eq!(fs::read_link(&link).expect("a link"), program);

    // floor-install default-branch: cargo builds the default branch.
    let sandbox = Sandbox::new();
    let source_dir = sandbox.path().join("default-branch");
    fs::create_dir_all(source_dir.join("crates").join("houserules")).expect("create the crate");
    fs::write(
        source_dir
            .join("crates")
            .join("houserules")
            .join("Cargo.toml"),
        "",
    )
    .expect("write Cargo.toml");
    sandbox.install_program(
        "cargo",
        "#!/bin/sh\necho \"$*\" >> \"$STUB_DIR/cargo.log\"\nroot=\nwhile [ $# -gt 0 ]; do case $1 in --root) root=$2; shift 2;; *) shift;; esac; done\n\
         mkdir -p \"$root/bin\"\nprintf '#!/bin/sh\\necho \"houserules 1.4.0\"\\n' > \"$root/bin/houserules\"\nchmod +x \"$root/bin/houserules\"\n",
    );
    let default_src = text(&source_dir);
    let output = github_install(
        &sandbox,
        "floor",
        "default-branch",
        "",
        &[("HOUSERULES_DEFAULT_SRC", default_src.as_str())],
    );
    assert_eq!(output.status.code(), Some(0), "{}", stdout(&output));
    assert_eq!(stdout(&output), "");
    let cargo = fs::read_to_string(sandbox.stub_dir().join("cargo.log")).expect("the cargo log");
    assert_eq!(
        cargo.trim(),
        format!(
            "install --locked --path {default_src}/crates/houserules --bin houserules --root {}",
            text(&sandbox.temp().join("houserules-bin-build"))
        )
    );
    assert!(sandbox.calls().is_empty(), "no download");
    let github_path = fs::read_to_string(sandbox.temp().join("github_path")).expect("GITHUB_PATH");
    assert!(
        github_path.contains("houserules-bin-build/bin"),
        "{github_path}"
    );
}

/// Design.md 242, 246(3), 247: the floor never takes a binary that the request
/// built or a relative path, and each source that is no source is refused with
/// its line.
#[test]
fn the_install_step_refuses_each_source_that_is_no_source() {
    for (role, source, line) in [
        (
            "floor",
            "none",
            "floor-install must not be none, because the floor must not run a binary that the request built",
        ),
        (
            "floor",
            "runner:relative/houserules",
            "floor-install runner:<path> needs an absolute path",
        ),
        (
            "floor",
            "runner:/no/such/houserules",
            "floor-install runner:/no/such/houserules names no executable file on this runner",
        ),
        (
            "floor",
            "default-branch",
            "the default branch holds no crates/houserules, so floor-install default-branch cannot build it",
        ),
        (
            "floor",
            "bogus",
            "floor-install must be release, default-branch, or runner:<path>",
        ),
        ("gates", "default-branch", "install must be release or none"),
        (
            "gates",
            "runner:/bin/true",
            "install must be release or none",
        ),
        ("gates", "bogus", "install must be release or none"),
    ] {
        let sandbox = Sandbox::new();
        let output = github_install(&sandbox, role, source, "2.0.0", &[]);
        assert_eq!(output.status.code(), Some(1), "{role} {source}");
        assert_eq!(
            stdout(&output),
            format!("::error::houserules: {line}\n"),
            "{role} {source}"
        );
        assert!(
            sandbox.calls().is_empty(),
            "{role} {source}: nothing was downloaded"
        );
    }
}

/// The GitLab gates script installs the release of its own version line, runs
/// no minimum check (the unit is the release), and runs the checks.
#[test]
fn the_gitlab_gates_script_installs_the_release_of_its_version_line() {
    let unit = gitlab("root");
    let version = unit
        .variable("gates", "HOUSERULES_VERSION")
        .expect("the version line");
    let sandbox = Sandbox::new();
    sandbox.reply("GET", &installer_path(&version), &installer("0.1.0"));
    let env = env_of(
        &unit,
        "gates",
        &[
            ("HOUSERULES_DIR", "."),
            ("HOUSERULES_SOURCE", "release"),
            ("HOUSERULES_TRACKER", ""),
            ("HOUSERULES_SETUP", ""),
            ("CI_PIPELINE_SOURCE", "push"),
        ],
    );
    let output = sandbox.run_sh(&unit.job_script("gates"), env);
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}{}",
        stdout(&output),
        stderr(&output)
    );
    let calls = sandbox.calls();
    assert_eq!(calls.len(), 1, "{calls:?}");
    assert!(
        calls[0].ends_with(&format!(
            "/releases/download/v{version}/houserules-installer.sh"
        )),
        "{calls:?}"
    );
}

/// The GitLab scripts run the setup command in gates and refuse a missing binary
/// with `install: none`; the floor script refuses `none` before any other step.
#[test]
fn the_gitlab_scripts_run_setup_require_houserules_and_refuse_a_floor_of_none() {
    let unit = gitlab("root");
    let base = [
        ("HOUSERULES_DIR", "."),
        ("HOUSERULES_TRACKER", ""),
        ("CI_PIPELINE_SOURCE", "push"),
    ];

    // install none and a setup command that puts the binary in place.
    let sandbox = Sandbox::new();
    let setup = format!(
        "mkdir -p {bin} && printf '#!/bin/sh\\necho \"$*\" >> \"$STUB_DIR/houserules.log\"\\n' > {bin}/houserules && chmod +x {bin}/houserules && echo setup-ran > \"$PWD/setup.txt\"",
        bin = text(&sandbox.path().join("setup-bin"))
    );
    let mut pairs = base.to_vec();
    pairs.extend([
        ("HOUSERULES_SOURCE", "none"),
        ("HOUSERULES_SETUP", setup.as_str()),
    ]);
    let script = format!(
        "PATH=\"{}:$PATH\"\n{}",
        text(&sandbox.path().join("setup-bin")),
        unit.job_script("gates")
    );
    let output = sandbox.run_sh(&script, env_of(&unit, "gates", &pairs));
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}{}",
        stdout(&output),
        stderr(&output)
    );
    assert_eq!(
        fs::read_to_string(sandbox.work().join("setup.txt")).unwrap(),
        "setup-ran\n"
    );
    let log =
        fs::read_to_string(sandbox.stub_dir().join("houserules.log")).expect("the houserules log");
    assert_eq!(log, "check-knowledge --dir .\ncheck-backlog --dir .\n");

    // install none and no binary: the job fails with one line.
    let sandbox = Sandbox::new();
    let mut pairs = base.to_vec();
    pairs.extend([("HOUSERULES_SOURCE", "none"), ("HOUSERULES_SETUP", "")]);
    let output = sandbox.run_sh(&unit.job_script("gates"), env_of(&unit, "gates", &pairs));
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        stdout(&output),
        "houserules: houserules is not on the PATH, so with install none the setup command must put it there\n"
    );

    // the floor takes no none.
    let sandbox = Sandbox::new();
    sandbox.install_program("houserules", &binary("9.9.9"));
    let output = sandbox.run_sh(
        &unit.job_script("floor"),
        env_of(
            &unit,
            "floor",
            &[("HOUSERULES_DIR", "."), ("HOUSERULES_SOURCE", "none")],
        ),
    );
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        stdout(&output),
        "houserules: floor-install must not be none, because the floor must not run a binary that the request built\n"
    );
    assert!(sandbox.calls().is_empty());
}
