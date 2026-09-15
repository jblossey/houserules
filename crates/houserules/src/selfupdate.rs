//! The self-update phase `install::cmd_update` runs before its own repo
//! sync (HR-133, spec 2026-09-15-batch-27 §3 T2): keeps the INSTALLED
//! BINARY itself current, distinct from `install::update`'s payload sync
//! of a target repository's kit-owned files.
//!
//! # Channel branching
//!
//! Only the shell/PowerShell installer channel writes a cargo-dist
//! install receipt (`~/.config/houserules/houserules-receipt.json`, or
//! the platform equivalent `axoupdater::AxoUpdater::load_receipt` reads);
//! mise and a direct download never do. [`UpdateSource::load_receipt`]
//! reports which case applies, and [`decide`] turns that, plus
//! [`UpdateSource::receipt_ownership`] and the outcome of actually
//! applying an update, into one [`Outcome`] this module prints from and
//! re-execs on -- never the other way around, so every branch is a pure,
//! unit-tested decision with no I/O of its own.
//!
//! # A receipt names one install, not one binary
//!
//! A found receipt can still name an install this process is not itself
//! running from (a stale receipt from before the T1 `~/.cargo/bin` ->
//! `~/.local/bin` move, design.md 5.86, is exactly this). axoupdater
//! itself already declines to update in that case
//! (`AxoUpdater::check_receipt_is_for_this_executable`, which its own
//! `is_update_needed` checks before ever contacting GitHub) -- but
//! declining silently would read as "already current" to an adopter who
//! is anything but. [`UpdateSource::receipt_ownership`] calls that same
//! check explicitly, before [`UpdateSource::apply`] ever runs, so
//! [`Outcome::ReceiptElsewhere`] can say plainly that this command
//! cannot update the copy the user invoked, and name the way out.
//!
//! # Why this module is not `async`
//!
//! axoupdater's `blocking` feature (its own `is_update_needed_sync`/
//! `run_sync`) still depends on `tokio`: each call spins up a
//! single-threaded runtime internally and blocks on it. `tokio` is
//! therefore a real, if hidden, part of this binary's dependency tree;
//! nothing in this crate ever names it, since the sync API never
//! surfaces an `await` point to wrap.
//!
//! # Never a hard error
//!
//! Every branch this phase can take -- no receipt, a receipt for a
//! different install, a network or permissions failure, an explicit
//! opt-out -- degrades to at most one stderr line and always lets the
//! repo phase run on the current binary. Only a real update of the
//! binary this process is itself running from changes that:
//! [`run_before_repo_phase`] re-execs into the newly-replaced binary
//! rather than returning, since the old process image is stale the
//! moment that self-replace succeeds.
//!
//! # Testability
//!
//! [`UpdateSource`] isolates every network and receipt-file access
//! behind methods a fake can implement with no I/O at all, so [`decide`]
//! -- the branch this phase's whole contract rests on -- is tested
//! without ever touching GitHub (security-hygiene: no live network call
//! belongs in `cargo test`). [`stale_copies_on_path`] takes `PATH` and
//! the running binary's path as plain parameters for the same reason: a
//! test builds both without any real environment mutation.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use axoupdater::{AxoUpdater, AxoupdateError};

/// Set by [`run_before_repo_phase`]'s own re-exec, on the new process, so
/// that process's own self-update phase never fires again this
/// invocation (the loop guard the spec names): a fresh receipt check
/// costs a network round trip this run already paid for, and the new
/// binary is, by construction, the version that check would report.
const SELF_UPDATED_ENV: &str = "HOUSERULES_SELF_UPDATED";

/// Sanctioned opt-out (any value, unset is the only "not set" state --
/// the same convention `CI` and `NO_COLOR` use): skips the whole phase,
/// including the PATH scan, with no output at all.
const SKIP_ENV: &str = "HOUSERULES_SKIP_SELF_UPDATE";

/// One stderr line naming the channel-appropriate way to update the
/// binary itself, printed when no install receipt exists (mise or a
/// direct download).
const NO_RECEIPT_GUIDANCE: &str = "houserules: no shell-installer receipt found; run `mise up houserules` if installed via mise, or download the latest release from https://github.com/jblossey/houserules/releases/latest, to update the binary.";

/// Whether a receipt was found for [`UpdateSource::apply`] to act on.
enum ReceiptCheck {
    /// No install receipt exists for this app on this machine.
    NoReceipt,
    /// A receipt was loaded; the source is ready to check-and-apply.
    Found,
}

/// Whether a loaded receipt names the install this process is itself
/// running from -- axoupdater's own
/// `check_receipt_is_for_this_executable`, called explicitly so a
/// mismatch gets its own honest [`Outcome`] instead of being silently
/// folded into "already up to date".
enum ReceiptOwnership {
    /// The receipt's own install location is where this process runs
    /// from; safe to call [`UpdateSource::apply`].
    RunningFromIt,
    /// The receipt names a different install. `install_prefix` is that
    /// install's own recorded root, for naming in
    /// [`Outcome::ReceiptElsewhere`].
    Elsewhere { install_prefix: PathBuf },
}

/// What a successful self-replace changed -- the fields `decide` needs
/// to name the version transition. Reached only once
/// [`ReceiptOwnership::RunningFromIt`] has already confirmed the replace
/// lands under this process's own binary.
#[derive(Clone, Debug)]
struct Applied {
    /// The version the receipt recorded before the replace
    /// (`UpdateResult::old_version`). `None` only if axoupdater itself
    /// never resolved one.
    old_version: Option<String>,
    /// The version now installed.
    new_version: String,
}

/// The result [`decide`] reaches, and the one thing [`run_before_repo_phase`]
/// prints and acts from.
enum Outcome {
    /// No install receipt exists for this channel.
    NoReceipt,
    /// A receipt exists, names this process's own install, and the
    /// running binary is already the latest.
    UpToDate,
    /// A receipt exists, names this process's own install, and a newer
    /// release replaced the binary this process is itself running from.
    Updated { from: String, to: String },
    /// A receipt exists but names an install this process is not itself
    /// running from -- this command cannot update the copy the user
    /// invoked. `install_prefix` is that other install's own root.
    ReceiptElsewhere { install_prefix: PathBuf },
    /// Loading the receipt, determining its ownership, or applying the
    /// update failed.
    Failed(String),
}

/// The self-update phase's one dependency on the outside world: an install
/// receipt and a real release check/replace. Abstracted so [`decide`] is
/// tested with a fake that never reaches the network or the filesystem's
/// real receipt path (security-hygiene.verify-current-docs,
/// process.tdd: tests never touch the network).
trait UpdateSource {
    /// Loads whatever install receipt exists for this app. `Err` is a
    /// real failure (a receipt file present but unreadable/malformed);
    /// [`ReceiptCheck::NoReceipt`] is the ordinary, expected case for a
    /// non-shell-installer channel.
    fn load_receipt(&mut self) -> Result<ReceiptCheck, String>;

    /// Whether the receipt [`Self::load_receipt`] just loaded names the
    /// install this process is itself running from. Must be called only
    /// after `load_receipt` returns [`ReceiptCheck::Found`].
    fn receipt_ownership(&self) -> Result<ReceiptOwnership, String>;

    /// Checks the latest release against the loaded receipt and, if
    /// newer, replaces the binary on disk. Called only once
    /// [`Self::receipt_ownership`] has confirmed
    /// [`ReceiptOwnership::RunningFromIt`]. `Ok(None)` means the
    /// receipt's version is already current; `Ok(Some(applied))`
    /// describes the replace that succeeded.
    fn apply(&mut self) -> Result<Option<Applied>, String>;
}

/// [`UpdateSource`] backed by the real `axoupdater` crate: the GitHub
/// Releases backend, this app's own real install receipt.
struct AxoupdaterSource {
    updater: AxoUpdater,
}

impl AxoupdaterSource {
    /// An updater configured for this binary's own release repository.
    fn new() -> Self {
        Self {
            updater: AxoUpdater::new_for("houserules"),
        }
    }
}

impl UpdateSource for AxoupdaterSource {
    fn load_receipt(&mut self) -> Result<ReceiptCheck, String> {
        match self.updater.load_receipt() {
            Ok(_) => Ok(ReceiptCheck::Found),
            Err(AxoupdateError::NoReceipt { .. }) => Ok(ReceiptCheck::NoReceipt),
            Err(error) => Err(error.to_string()),
        }
    }

    fn receipt_ownership(&self) -> Result<ReceiptOwnership, String> {
        let matches = self
            .updater
            .check_receipt_is_for_this_executable()
            .map_err(|error| error.to_string())?;
        if matches {
            return Ok(ReceiptOwnership::RunningFromIt);
        }
        let install_prefix = self
            .updater
            .install_prefix_root()
            .map_err(|error| error.to_string())?
            .into_std_path_buf();
        Ok(ReceiptOwnership::Elsewhere { install_prefix })
    }

    fn apply(&mut self) -> Result<Option<Applied>, String> {
        self.updater
            .run_sync()
            .map(|result| {
                result.map(|update| Applied {
                    old_version: update.old_version.map(|version| version.to_string()),
                    new_version: update.new_version.to_string(),
                })
            })
            .map_err(|error| error.to_string())
    }
}

/// Turns a receipt check, its ownership, and (only once ownership
/// confirms this process's own install) an apply attempt into the one
/// [`Outcome`] the rest of this phase acts on.
fn decide(source: &mut impl UpdateSource) -> Outcome {
    match source.load_receipt() {
        Ok(ReceiptCheck::NoReceipt) => Outcome::NoReceipt,
        Err(message) => Outcome::Failed(message),
        Ok(ReceiptCheck::Found) => match source.receipt_ownership() {
            Err(message) => Outcome::Failed(message),
            Ok(ReceiptOwnership::Elsewhere { install_prefix }) => {
                Outcome::ReceiptElsewhere { install_prefix }
            }
            Ok(ReceiptOwnership::RunningFromIt) => match source.apply() {
                Ok(None) => Outcome::UpToDate,
                Ok(Some(applied)) => Outcome::Updated {
                    from: applied.old_version.unwrap_or_else(|| "unknown".to_string()),
                    to: applied.new_version,
                },
                Err(message) => Outcome::Failed(message),
            },
        },
    }
}

/// True when the self-update phase must not run at all this invocation:
/// an explicit opt-out, a CI environment, or the loop guard. Pure so the
/// truth table is tested without touching real environment variables.
fn phase_disabled(skip_requested: bool, ci: bool, already_self_updated: bool) -> bool {
    skip_requested || ci || already_self_updated
}

/// The `houserules` binary a receipt's `install_prefix` (a root, not a
/// binary path) actually holds: `prefix` itself for a flat layout
/// (`~/.local/bin`-shaped receipts, where `install_prefix` already is the
/// binary's own directory), or `prefix/bin` for a `CARGO_HOME`-shaped
/// one -- the same two candidates [`ReceiptOwnership`]'s own comparison
/// draws on, checked in the same order. Picks whichever candidate
/// actually exists; falls back to the `bin`-joined guess (the more
/// common layout, and the one a stale pre-HR-135 receipt -- this
/// function's whole reason to exist -- always uses) when neither has
/// been created yet, so a fixture in a test still gets a path to name.
fn resolve_runnable_binary(install_prefix: &Path) -> PathBuf {
    let exe_name = format!("houserules{}", std::env::consts::EXE_SUFFIX);
    let flat = install_prefix.join(&exe_name);
    if flat.exists() {
        return flat;
    }
    install_prefix.join("bin").join(exe_name)
}

/// The stderr line(s) this phase prints for `outcome`, in order, before
/// the PATH scan. Empty for [`Outcome::UpToDate`]: a self-updater that
/// has nothing to report says nothing, the same convention `rustup`/
/// `brew` use. The "checking for a newer release..." line precedes this
/// entirely (`run_before_repo_phase` prints it before `decide` even
/// runs), so every branch here names only its own outcome.
fn pre_scan_messages(outcome: &Outcome) -> Vec<String> {
    match outcome {
        Outcome::NoReceipt => vec![NO_RECEIPT_GUIDANCE.to_string()],
        Outcome::UpToDate => vec![],
        Outcome::Updated { from, to } => vec![format!("houserules: binary updated {from} -> {to}")],
        Outcome::ReceiptElsewhere { install_prefix } => vec![format!(
            "houserules: the install receipt at {prefix} is not for this binary; this command \
             cannot update the copy you just ran. Run houserules from {binary} to update that \
             install, or update this copy through its own channel (mise, or a fresh download).",
            prefix = install_prefix.display(),
            binary = resolve_runnable_binary(install_prefix).display()
        )],
        Outcome::Failed(message) => vec![format!(
            "houserules: self-update check failed ({message}); continuing without self-update."
        )],
    }
}

/// True when the PATH scan should run for `outcome`: only once this run
/// has confirmed, against this process's own install, that a newer
/// release does or does not exist. [`Outcome::ReceiptElsewhere`] already
/// names the one other install this run knows about directly, in its
/// own line; [`Outcome::NoReceipt`] and [`Outcome::Failed`] never learned
/// whether anything on `PATH` is actually behind. None of the three
/// needs a second, generic warning.
fn scan_applies(outcome: &Outcome) -> bool {
    matches!(outcome, Outcome::Updated { .. } | Outcome::UpToDate)
}

/// Scans `path_env` (a `PATH`-style string using this platform's
/// path-list separator) for `houserules` executables at a location other
/// than `current_exe`'s own canonical path. Each distinct stale copy is
/// reported once: two `PATH` entries that canonically resolve to the
/// same file (a duplicate entry, or a symlink), or to `current_exe`
/// itself, are never double-counted or reported as stale.
///
/// A `PATH` entry that does not exist, or a candidate this process
/// cannot stat, is silently absent for this scan (`PATH` commonly names
/// directories that do not exist; the scan is a convenience warning, not
/// a survey this process is entitled to fail over).
fn stale_copies_on_path(path_env: &str, current_exe: &Path) -> Vec<PathBuf> {
    let exe_name = format!("houserules{}", std::env::consts::EXE_SUFFIX);
    let current_canonical = current_exe
        .canonicalize()
        .unwrap_or_else(|_| current_exe.to_path_buf());

    let mut seen = HashSet::new();
    let mut stale = Vec::new();
    for dir in std::env::split_paths(path_env) {
        let candidate = dir.join(&exe_name);
        let Ok(canonical) = candidate.canonicalize() else {
            continue;
        };
        if canonical == current_canonical {
            continue;
        }
        if seen.insert(canonical) {
            stale.push(candidate);
        }
    }
    stale
}

/// The one warning line naming every other copy [`stale_copies_on_path`]
/// found on `PATH`, printed after the phase's own outcome line(s) and
/// before a re-exec (the T1 `~/.cargo/bin` -> `~/.local/bin` migration
/// story, design.md 5.86). Names the paths only -- what to do about one
/// is the reader's call, not an imperative this line hands out.
fn format_stale_copies_warning(paths: &[PathBuf]) -> String {
    let list = paths
        .iter()
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>()
        .join(", ");
    format!("houserules: also found on PATH, not the copy just checked: {list}")
}

/// Runs the self-update phase, called once from `install::cmd_update`
/// before its own repo sync. Reads its own environment, `PATH`, and the
/// running executable's path -- every decision this makes is delegated to
/// [`decide`]/[`phase_disabled`]/[`scan_applies`]/[`stale_copies_on_path`],
/// which is where this crate's tests exercise the actual logic (this
/// function itself runs a real network check and, on an update of its
/// own binary, never returns).
pub(crate) fn run_before_repo_phase() {
    if phase_disabled(
        std::env::var(SKIP_ENV).is_ok(),
        std::env::var("CI").is_ok(),
        std::env::var(SELF_UPDATED_ENV).is_ok(),
    ) {
        return;
    }

    // Captured before `decide` can self-replace: on Linux, `current_exe`
    // reads `/proc/self/exe`, which reports the path with " (deleted)"
    // appended once the file this process was loaded from is unlinked --
    // capture it before that can happen, since the path itself never
    // moves.
    let current_exe = std::env::current_exe().ok();

    eprintln!("houserules: checking for a newer release...");
    let mut source = AxoupdaterSource::new();
    let outcome = decide(&mut source);

    for line in pre_scan_messages(&outcome) {
        eprintln!("{line}");
    }

    if scan_applies(&outcome)
        && let (Some(exe), Ok(path_env)) = (current_exe.as_deref(), std::env::var("PATH"))
    {
        let stale = stale_copies_on_path(&path_env, exe);
        if !stale.is_empty() {
            eprintln!("{}", format_stale_copies_warning(&stale));
        }
    }

    // Updated is only ever produced once receipt_ownership has confirmed
    // RunningFromIt, which itself only holds when current_exe resolved
    // (axoupdater's own check reads it internally) -- so matching both
    // together here, rather than re-testing current_exe on its own,
    // keeps this the only path that can reach reexec_self, with no arm
    // for a precondition that cannot fail.
    if let (Outcome::Updated { .. }, Some(exe)) = (&outcome, current_exe.as_deref()) {
        eprintln!("houserules: re-running the updated binary...");
        reexec_self(exe);
    }
}

/// Re-executes `exe` (the running binary's own pre-replace path -- see
/// [`run_before_repo_phase`]'s own comment on why this must never be a
/// fresh `current_exe` call) with this process's original argv and
/// [`SELF_UPDATED_ENV`] set -- the loop guard [`phase_disabled`] reads on
/// the new process. Never returns: on Unix the new process image
/// replaces this one; on Windows, which has no such call, this process
/// exits with the child's own exit code.
fn reexec_self(exe: &Path) -> ! {
    let args: Vec<std::ffi::OsString> = std::env::args_os().skip(1).collect();
    let mut command = std::process::Command::new(exe);
    command.args(&args).env(SELF_UPDATED_ENV, "1");
    reexec(command)
}

/// Replaces this process's image with `command`'s program: `exec(3)`
/// never returns on success, so the only reachable return is the error
/// path.
#[cfg(unix)]
fn reexec(mut command: std::process::Command) -> ! {
    use std::os::unix::process::CommandExt;
    let error = command.exec();
    eprintln!("houserules: could not re-exec after self-update: {error}");
    std::process::exit(1);
}

/// Windows has no process-image-replacing exec; runs `command` as a
/// child and exits with its status code instead.
#[cfg(windows)]
fn reexec(mut command: std::process::Command) -> ! {
    match command.status() {
        Ok(status) => std::process::exit(status.code().unwrap_or(1)),
        Err(error) => {
            eprintln!("houserules: could not re-exec after self-update: {error}");
            std::process::exit(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A scripted [`UpdateSource`] double: never touches the network or a
    /// real receipt file.
    struct FakeSource {
        receipt: Result<ReceiptCheck, String>,
        ownership: Result<ReceiptOwnership, String>,
        apply: Result<Option<Applied>, String>,
    }

    impl UpdateSource for FakeSource {
        fn load_receipt(&mut self) -> Result<ReceiptCheck, String> {
            match &self.receipt {
                Ok(ReceiptCheck::Found) => Ok(ReceiptCheck::Found),
                Ok(ReceiptCheck::NoReceipt) => Ok(ReceiptCheck::NoReceipt),
                Err(message) => Err(message.clone()),
            }
        }

        fn receipt_ownership(&self) -> Result<ReceiptOwnership, String> {
            match &self.ownership {
                Ok(ReceiptOwnership::RunningFromIt) => Ok(ReceiptOwnership::RunningFromIt),
                Ok(ReceiptOwnership::Elsewhere { install_prefix }) => {
                    Ok(ReceiptOwnership::Elsewhere {
                        install_prefix: install_prefix.clone(),
                    })
                }
                Err(message) => Err(message.clone()),
            }
        }

        fn apply(&mut self) -> Result<Option<Applied>, String> {
            match &self.apply {
                Ok(value) => Ok(value.clone()),
                Err(message) => Err(message.clone()),
            }
        }
    }

    /// A found receipt naming this process's own install, ready to
    /// `apply`.
    fn found(apply: Result<Option<Applied>, String>) -> FakeSource {
        FakeSource {
            receipt: Ok(ReceiptCheck::Found),
            ownership: Ok(ReceiptOwnership::RunningFromIt),
            apply,
        }
    }

    /// A found receipt naming a different install. `apply` is an `Err`
    /// that would fail any test relying on it, since `decide` must never
    /// call it for this ownership: a test that still gets `Failed`
    /// instead of `ReceiptElsewhere` has found a real regression, not a
    /// fixture gap.
    fn found_elsewhere(install_prefix: PathBuf) -> FakeSource {
        FakeSource {
            receipt: Ok(ReceiptCheck::Found),
            ownership: Ok(ReceiptOwnership::Elsewhere { install_prefix }),
            apply: Err("apply must not be called for ReceiptOwnership::Elsewhere".to_string()),
        }
    }

    fn no_receipt() -> FakeSource {
        FakeSource {
            receipt: Ok(ReceiptCheck::NoReceipt),
            ownership: Ok(ReceiptOwnership::RunningFromIt),
            apply: Ok(None),
        }
    }

    fn receipt_load_failed(message: &str) -> FakeSource {
        FakeSource {
            receipt: Err(message.to_string()),
            ownership: Ok(ReceiptOwnership::RunningFromIt),
            apply: Ok(None),
        }
    }

    fn ownership_check_failed(message: &str) -> FakeSource {
        FakeSource {
            receipt: Ok(ReceiptCheck::Found),
            ownership: Err(message.to_string()),
            apply: Ok(None),
        }
    }

    /// This platform's `houserules` executable filename -- test-only copy
    /// of the production scan's own naming, kept here rather than
    /// exported so the fixtures below build a real candidate the same
    /// way the scan looks for one.
    fn exe_name() -> String {
        format!("houserules{}", std::env::consts::EXE_SUFFIX)
    }

    // (c) no-receipt guidance.
    #[test]
    fn decide_reports_no_receipt_when_the_source_has_none() {
        let mut source = no_receipt();
        assert!(matches!(decide(&mut source), Outcome::NoReceipt));
    }

    #[test]
    fn no_receipt_outcome_prints_exactly_the_guidance_line() {
        assert_eq!(
            pre_scan_messages(&Outcome::NoReceipt),
            vec![NO_RECEIPT_GUIDANCE.to_string()]
        );
    }

    #[test]
    fn no_receipt_guidance_names_mise_and_the_releases_page() {
        assert!(NO_RECEIPT_GUIDANCE.contains("mise up houserules"));
        assert!(NO_RECEIPT_GUIDANCE.contains("releases/latest"));
    }

    // A receipt present, for this install, but already at the latest
    // version: prints nothing.
    #[test]
    fn decide_reports_up_to_date_when_apply_finds_no_newer_release() {
        let mut source = found(Ok(None));
        assert!(matches!(decide(&mut source), Outcome::UpToDate));
    }

    #[test]
    fn up_to_date_outcome_prints_nothing() {
        assert!(pre_scan_messages(&Outcome::UpToDate).is_empty());
    }

    #[test]
    fn decide_reports_updated_using_the_receipts_old_version_when_running_from_the_receipts_own_install()
     {
        let mut source = found(Ok(Some(Applied {
            old_version: Some("1.0.0".to_string()),
            new_version: "1.1.0".to_string(),
        })));
        match decide(&mut source) {
            Outcome::Updated { from, to } => {
                assert_eq!(from, "1.0.0");
                assert_eq!(to, "1.1.0");
            }
            _ => panic!("expected Updated"),
        }
    }

    // The real bug a live run found (round 3): a receipt naming a
    // different install must decline honestly, and must never reach
    // apply -- axoupdater's own gate would otherwise make this
    // indistinguishable from UpToDate.
    #[test]
    fn decide_declines_without_calling_apply_when_the_receipt_names_a_different_install() {
        let mut source = found_elsewhere(PathBuf::from("/old/install"));
        match decide(&mut source) {
            Outcome::ReceiptElsewhere { install_prefix } => {
                assert_eq!(install_prefix, PathBuf::from("/old/install"));
            }
            other => panic!(
                "expected ReceiptElsewhere without calling apply, got {}",
                outcome_name(&other)
            ),
        }
    }

    #[test]
    fn decide_reports_failed_when_the_ownership_check_errors() {
        let mut source = ownership_check_failed("current_exe unavailable");
        match decide(&mut source) {
            Outcome::Failed(message) => assert_eq!(message, "current_exe unavailable"),
            other => panic!("expected Failed, got {}", outcome_name(&other)),
        }
    }

    #[test]
    fn decide_uses_a_designed_token_when_the_replaced_versions_old_value_is_unknown() {
        let mut source = found(Ok(Some(Applied {
            old_version: None,
            new_version: "1.1.0".to_string(),
        })));
        let from = match decide(&mut source) {
            Outcome::Updated { from, .. } => from,
            other => panic!("expected Updated, got {}", outcome_name(&other)),
        };
        assert_eq!(from, "unknown");
    }

    fn outcome_name(outcome: &Outcome) -> &'static str {
        match outcome {
            Outcome::NoReceipt => "NoReceipt",
            Outcome::UpToDate => "UpToDate",
            Outcome::Updated { .. } => "Updated",
            Outcome::ReceiptElsewhere { .. } => "ReceiptElsewhere",
            Outcome::Failed(_) => "Failed",
        }
    }

    #[test]
    fn updated_outcome_prints_only_the_version_transition() {
        let outcome = Outcome::Updated {
            from: "1.0.0".to_string(),
            to: "1.1.0".to_string(),
        };
        let lines = pre_scan_messages(&outcome);
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert!(lines[0].contains("1.0.0 -> 1.1.0"), "{lines:?}");
    }

    #[test]
    fn receipt_elsewhere_outcome_names_the_prefix_and_the_way_out() {
        // Nothing under /old/install actually exists in this test, so
        // neither the flat nor the bin-joined candidate can be found: the
        // "receipt at ..." half still names the bare root exactly as
        // received, but the actionable half must resolve to the
        // bin-joined path (the more common of the two install layouts,
        // and the one a cargo-home receipt -- the population this line
        // serves -- actually uses), never the bare root a reader could
        // not run anything from.
        let outcome = Outcome::ReceiptElsewhere {
            install_prefix: PathBuf::from("/old/install"),
        };
        let lines = pre_scan_messages(&outcome);
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert!(
            lines[0].contains("receipt at /old/install "),
            "the receipt-at half must name the bare root: {lines:?}"
        );
        assert!(
            lines[0].contains("cannot update the copy"),
            "must say this command cannot update the copy the user invoked: {lines:?}"
        );
        let bin_joined = Path::new("/old/install").join("bin").join(exe_name());
        assert!(
            lines[0].contains(&format!("Run houserules from {}", bin_joined.display())),
            "must name the bin-joined runnable path, not the bare root, as the way out: {lines:?}"
        );
        assert!(
            lines[0].contains("own channel"),
            "must name updating this copy through its own channel as the other way out: {lines:?}"
        );
    }

    // (d) failure-degrades-to-warning.
    #[test]
    fn decide_reports_failed_when_the_receipt_load_errors() {
        let mut source = receipt_load_failed("permission denied");
        match decide(&mut source) {
            Outcome::Failed(message) => assert_eq!(message, "permission denied"),
            _ => panic!("expected Failed"),
        }
    }

    #[test]
    fn decide_reports_failed_when_apply_errors_after_a_receipt_is_found() {
        let mut source = found(Err("network unreachable".to_string()));
        match decide(&mut source) {
            Outcome::Failed(message) => assert_eq!(message, "network unreachable"),
            _ => panic!("expected Failed"),
        }
    }

    #[test]
    fn failed_outcome_prints_exactly_one_warning_line_naming_the_error() {
        let outcome = Outcome::Failed("network unreachable".to_string());
        let lines = pre_scan_messages(&outcome);
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert!(lines[0].contains("network unreachable"), "{lines:?}");
        assert!(lines[0].starts_with("houserules: "), "{lines:?}");
    }

    // (b) CI/env skips, (a) the loop guard.
    #[test]
    fn phase_runs_when_nothing_opts_out() {
        assert!(!phase_disabled(false, false, false));
    }

    #[test]
    fn phase_disabled_by_the_skip_env_var() {
        assert!(phase_disabled(true, false, false));
    }

    #[test]
    fn phase_disabled_under_ci() {
        assert!(phase_disabled(false, true, false));
    }

    #[test]
    fn phase_disabled_by_the_loop_guard() {
        assert!(phase_disabled(false, false, true));
    }

    // (3) the PATH scan runs only once this run has checked this
    // process's own install -- never on NoReceipt, ReceiptElsewhere or
    // Failed.
    #[test]
    fn path_scan_runs_after_updated_or_up_to_date() {
        assert!(scan_applies(&Outcome::Updated {
            from: "1.0.0".to_string(),
            to: "1.1.0".to_string(),
        }));
        assert!(scan_applies(&Outcome::UpToDate));
    }

    #[test]
    fn path_scan_never_runs_on_no_receipt_elsewhere_or_failed() {
        assert!(!scan_applies(&Outcome::NoReceipt));
        assert!(!scan_applies(&Outcome::ReceiptElsewhere {
            install_prefix: PathBuf::from("/old"),
        }));
        assert!(!scan_applies(&Outcome::Failed(
            "network unreachable".to_string()
        )));
    }

    // (e) the PATH-scan dedupe and warning text.
    #[test]
    fn stale_scan_finds_nothing_when_only_the_running_binary_is_on_path() {
        let dir = tempfile::tempdir().expect("tempdir");
        let exe = dir.path().join(exe_name());
        std::fs::write(&exe, b"binary").expect("write exe");
        let path_env = dir.path().display().to_string();
        assert!(stale_copies_on_path(&path_env, &exe).is_empty());
    }

    #[test]
    fn stale_scan_reports_a_different_houserules_on_path() {
        let dir = tempfile::tempdir().expect("tempdir");
        let running = dir.path().join("running");
        std::fs::create_dir(&running).expect("mkdir running");
        let running_exe = running.join(exe_name());
        std::fs::write(&running_exe, b"new").expect("write running");

        let stale_dir = dir.path().join("stale");
        std::fs::create_dir(&stale_dir).expect("mkdir stale");
        let stale_exe = stale_dir.join(exe_name());
        std::fs::write(&stale_exe, b"old").expect("write stale");

        let path_env = std::env::join_paths([&running, &stale_dir])
            .expect("join_paths")
            .into_string()
            .expect("utf8 PATH");

        let found = stale_copies_on_path(&path_env, &running_exe);
        assert_eq!(found, vec![stale_exe]);
    }

    #[test]
    fn stale_scan_dedupes_a_path_entry_that_repeats() {
        let dir = tempfile::tempdir().expect("tempdir");
        let running = dir.path().join("running");
        std::fs::create_dir(&running).expect("mkdir running");
        let running_exe = running.join(exe_name());
        std::fs::write(&running_exe, b"new").expect("write running");

        let stale_dir = dir.path().join("stale");
        std::fs::create_dir(&stale_dir).expect("mkdir stale");
        let stale_exe = stale_dir.join(exe_name());
        std::fs::write(&stale_exe, b"old").expect("write stale");

        // The same directory listed twice on PATH -- a duplicate rc-file
        // append is the realistic cause, not a symlink.
        let path_env = std::env::join_paths([&stale_dir, &stale_dir])
            .expect("join_paths")
            .into_string()
            .expect("utf8 PATH");

        let found = stale_copies_on_path(&path_env, &running_exe);
        assert_eq!(found, vec![stale_exe]);
    }

    #[test]
    fn stale_scan_skips_a_path_entry_that_does_not_exist() {
        let dir = tempfile::tempdir().expect("tempdir");
        let running_exe = dir.path().join(exe_name());
        std::fs::write(&running_exe, b"new").expect("write running");
        let missing = dir.path().join("does-not-exist");

        let path_env = std::env::join_paths([&missing])
            .expect("join_paths")
            .into_string()
            .expect("utf8 PATH");

        assert!(stale_copies_on_path(&path_env, &running_exe).is_empty());
    }

    #[test]
    fn stale_copies_warning_names_every_path_in_one_line_with_no_delete_imperative() {
        let paths = vec![
            PathBuf::from("/old/houserules"),
            PathBuf::from("/other/houserules"),
        ];
        let warning = format_stale_copies_warning(&paths);
        assert!(warning.lines().count() == 1, "{warning:?}");
        assert!(warning.contains("/old/houserules"), "{warning:?}");
        assert!(warning.contains("/other/houserules"), "{warning:?}");
        assert!(!warning.contains("delete"), "{warning:?}");
    }
}
