//! A scratch directory in which a script of a review unit runs with a stub `gh`
//! and a stub `curl` on its `PATH`. The stubs (`tests/fixtures/ci-units/`)
//! record every call and answer from files, so a test runs the script of a job
//! as the runner would, with no network and no host.
//!
//! The units run `jq` and `sh` on Linux images only, so this module is
//! Unix-only.

use std::ffi::OsStr;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// The directory of the stub programs and the route and request fixtures.
pub fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("ci-units")
}

/// The name of the reply file that a stub reads for a call: the method and the
/// path, with every character outside `A-Z`, `a-z`, and `0-9` replaced by an
/// underscore.
pub fn reply_name(method: &str, path: &str) -> String {
    format!("{method}{path}")
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character
            } else {
                '_'
            }
        })
        .collect()
}

/// A scratch directory with a `PATH` directory that holds the stubs.
pub struct Sandbox {
    /// The owner of every file of the sandbox; the directory goes with it.
    dir: tempfile::TempDir,
}

impl Sandbox {
    /// A sandbox with a stub `gh` and a stub `curl` in its `bin` directory.
    pub fn new() -> Self {
        let dir = tempfile::tempdir().expect("create the sandbox");
        let sandbox = Sandbox { dir };
        for folder in ["bin", "stub/replies", "stub/fail", "work", "temp", "home"] {
            fs::create_dir_all(sandbox.path().join(folder)).expect("create a sandbox folder");
        }
        sandbox.install_fixture("stub-gh", "gh");
        sandbox.install_fixture("stub-curl", "curl");
        fs::write(sandbox.log_path(), "").expect("create the call log");
        sandbox
    }

    /// The root of the sandbox.
    pub fn path(&self) -> &Path {
        self.dir.path()
    }

    /// The directory that holds the programs of the `PATH`.
    pub fn bin(&self) -> PathBuf {
        self.path().join("bin")
    }

    /// The working directory of a script, as a job's checkout is.
    pub fn work(&self) -> PathBuf {
        self.path().join("work")
    }

    /// The directory that stands for `RUNNER_TEMP`.
    pub fn temp(&self) -> PathBuf {
        self.path().join("temp")
    }

    /// The directory of the stub replies, `STUB_DIR`.
    pub fn stub_dir(&self) -> PathBuf {
        self.path().join("stub")
    }

    /// The call log of the stubs, `STUB_LOG`.
    pub fn log_path(&self) -> PathBuf {
        self.path().join("stub").join("calls.log")
    }

    /// Copies the fixture `fixture` into `bin` as `name` and makes it
    /// executable.
    fn install_fixture(&self, fixture: &str, name: &str) {
        let source = fixture_dir().join(fixture);
        self.install_program(name, &fs::read_to_string(&source).expect("read a fixture"));
    }

    /// Writes the executable program `name` into `bin`.
    pub fn install_program(&self, name: &str, text: &str) {
        let target = self.bin().join(name);
        fs::write(&target, text).expect("write a program");
        fs::set_permissions(&target, fs::Permissions::from_mode(0o755))
            .expect("make a program executable");
    }

    /// Makes `reply` the answer to the stub call `method` `path`.
    pub fn reply(&self, method: &str, path: &str, reply: &str) {
        fs::write(
            self.stub_dir()
                .join("replies")
                .join(reply_name(method, path)),
            reply,
        )
        .expect("write a reply");
    }

    /// Makes `reply` the answer to the stub call `method` `path` for the page
    /// `page` (2 or more) of a paginated read.
    pub fn reply_page(&self, method: &str, path: &str, page: usize, reply: &str) {
        fs::write(
            self.stub_dir()
                .join("replies")
                .join(format!("{}_page{page}", reply_name(method, path))),
            reply,
        )
        .expect("write a reply");
    }

    /// Makes the stub call `method` `path` fail.
    pub fn fail(&self, method: &str, path: &str) {
        fs::write(
            self.stub_dir().join("fail").join(reply_name(method, path)),
            "",
        )
        .expect("write a failure");
    }

    /// Makes every stub call but a read fail, as a read-only token does.
    pub fn refuse_writes(&self) {
        fs::write(self.stub_dir().join("refuse-writes"), "").expect("write the refusal");
    }

    /// The lines of the call log: one for each call of a stub.
    pub fn calls(&self) -> Vec<String> {
        fs::read_to_string(self.log_path())
            .expect("read the call log")
            .lines()
            .map(str::to_string)
            .collect()
    }

    /// The calls that are not a read: a stub call with a method other than GET.
    pub fn writes(&self) -> Vec<String> {
        self.calls()
            .into_iter()
            .filter(|call| {
                call.contains("--method POST")
                    || call.contains("--method PATCH")
                    || call.contains("--method PUT")
                    || call.contains("--method DELETE")
                    || call.contains("--request POST")
                    || call.contains("--request PUT")
                    || call.contains("--request PATCH")
                    || call.contains("--request DELETE")
            })
            .collect()
    }

    /// The body that the stub copied for the call with the 1-based `number`.
    pub fn input(&self, number: usize) -> String {
        fs::read_to_string(self.stub_dir().join(format!("input-{number}")))
            .unwrap_or_else(|error| panic!("read the body of call {number}: {error}"))
    }

    /// The 1-based number of the first call whose line holds `needle`.
    pub fn call_number(&self, needle: &str) -> usize {
        self.calls()
            .iter()
            .position(|call| call.contains(needle))
            .map(|index| index + 1)
            .unwrap_or_else(|| panic!("no call holds {needle}: {:?}", self.calls()))
    }

    /// Runs `script` with the shell `shell` (the program and its options), in
    /// the work directory, with exactly the environment `environment` and the
    /// variables that every script of a job gets: a `PATH` that starts with the
    /// stubs, a `HOME` of its own, `RUNNER_TEMP`, the hermetic settings of git,
    /// and the paths of the stubs. `arguments` follow the script.
    pub fn run<K: AsRef<OsStr>, V: AsRef<OsStr>>(
        &self,
        shell: &[&str],
        script: &str,
        arguments: &[&str],
        environment: impl IntoIterator<Item = (K, V)>,
    ) -> Output {
        let file = self.path().join("script.sh");
        fs::write(&file, script).expect("write the script");
        // A directory of the host that holds a houserules binary stays out of the
        // PATH, so that a script finds only the binary a test installs.
        let search_path = std::env::join_paths(
            std::iter::once(self.bin()).chain(
                std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
                    .filter(|directory| !directory.join("houserules").exists()),
            ),
        )
        .expect("join the PATH");
        let mut command = Command::new(shell[0]);
        command
            .args(&shell[1..])
            .arg(&file)
            .args(arguments)
            .current_dir(self.work())
            .env_clear()
            .env("PATH", search_path)
            .env("HOME", self.path().join("home"))
            .env("TMPDIR", self.temp())
            .env("RUNNER_TEMP", self.temp())
            .env("LC_ALL", "C")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("STUB_DIR", self.stub_dir())
            .env("STUB_LOG", self.log_path());
        for (name, value) in environment {
            command.env(name, value);
        }
        command.output().expect("run the script")
    }

    /// Runs `script` as a GitHub `shell: bash` step does:
    /// `bash --noprofile --norc -eo pipefail`.
    pub fn run_bash<K: AsRef<OsStr>, V: AsRef<OsStr>>(
        &self,
        script: &str,
        environment: impl IntoIterator<Item = (K, V)>,
    ) -> Output {
        self.run(
            &["bash", "--noprofile", "--norc", "-eo", "pipefail"],
            script,
            &[],
            environment,
        )
    }

    /// Runs `script` as a GitLab job script does, with `sh`.
    pub fn run_sh<K: AsRef<OsStr>, V: AsRef<OsStr>>(
        &self,
        script: &str,
        environment: impl IntoIterator<Item = (K, V)>,
    ) -> Output {
        self.run(&["sh"], script, &[], environment)
    }
}
