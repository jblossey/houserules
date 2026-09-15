//! The update-available header (HR-134, spec 2026-09-15-batch-27 §3 T3):
//! the first stderr line a command MAY print, naming a newer release and
//! the command that installs it. [`maybe_print_header`] is the one call
//! `main` makes, before dispatch, for every command but `update` itself
//! (which acts on a newer release instead of announcing one).
//!
//! # Gating is one flat check
//!
//! [`header_disabled`] combines every opt-out -- stderr not a TTY, `CI`
//! set, [`NO_UPDATE_CHECK_ENV`] set, the `update` command itself -- into
//! one boolean that gates both the network refresh and the print
//! together. Spec section 1's own constraint ("no command ever blocks on
//! the network for the header") reads as a property of every invocation,
//! not only CI's: a script piping this binary's stdout should see
//! exactly the latency it saw before this header existed, so the network
//! check itself never runs unless stderr is already a place a human
//! could see the header on.
//!
//! # Receipt-free, by construction
//!
//! [`AxoupdaterSource`] never calls `load_receipt`: it builds a
//! `ReleaseSource` from this crate's own `CARGO_PKG_REPOSITORY` and hands
//! it to `AxoUpdater::set_release_source`, whose own doc comment names
//! exactly this use ("query the new version without actually performing
//! an upgrade") -- so mise and direct-download installs, which
//! `selfupdate.rs`'s receipt-gated phase never reaches, get this header
//! too (spec: "receipt-free by design").
//!
//! # Why this module owns a tiny tokio runtime
//!
//! `AxoUpdater::query_new_version` -- the one method that queries a
//! release without first requiring `current_version`/`install_prefix`
//! the way `is_update_needed`/`run` do -- has no `_sync` twin the
//! `blocking` feature provides for those two. `axoupdater`'s own
//! `is_update_needed_sync`/`run_sync` (`selfupdate.rs`'s doc has the
//! account) build exactly this shape of runtime internally; `tokio` is
//! already an unavoidable part of this binary's dependency tree because
//! `blocking` pulls it in with the `full` feature, so naming it directly
//! here to `block_on` one already-fetched future adds no new compiled
//! surface, only a name for surface that already exists.
//!
//! # Every failure refreshes the timestamp and stays silent
//!
//! [`refresh_if_stale`] treats a network error, a timeout, and "axoupdater
//! reported no release" identically: the cache's `checked_at_unix` moves
//! to now regardless, so a flaky release host costs at most one round
//! trip per [`CACHE_TTL`], never a retry storm -- and `latest_version`
//! carries over unchanged, so a transient failure never erases a real
//! update this run already knew about. [`read_cache`] folds a missing or
//! corrupt cache file into the same "start fresh" path
//! (`houserules.crash-paths-are-named`: a corrupt cache degrades this
//! header, it never breaks the command it hooks into).
//!
//! # Prerelease is checked twice
//!
//! `AxoUpdater`'s own `UpdateRequest::Latest` (this module's default)
//! already resolves to GitHub's `/releases/latest` endpoint or, failing
//! that, the newest release with `prerelease: false`
//! (`get_latest_stable_release`, axoupdater 0.10.2) -- a prerelease
//! should never reach the cache at all. [`is_update_available`] checks
//! `latest.pre.is_empty()` again anyway: a cache file is a plain JSON
//! file an adopter can hand-edit, and a defense that only lives in the
//! network layer would print a prerelease recommendation the moment
//! anything else ever populates that file.
//!
//! # Testability
//!
//! [`UpdateSource`] isolates the one network call behind a trait a fake
//! implements with no I/O (mirrors `selfupdate.rs`'s own `UpdateSource`);
//! [`header_disabled`], [`is_stale`], [`is_update_available`], and
//! [`header_line`] are pure functions taking their inputs as plain
//! parameters, so the full gating and comparison matrix is tested
//! without a real terminal, clock, or network call
//! (security-hygiene.verify-current-docs: no live network call belongs
//! in `cargo test`).

use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use axoupdater::{AxoUpdater, ReleaseSource, ReleaseSourceType, Version};
use serde::{Deserialize, Serialize};

/// Opt-out (any value, unset is the only "not set" state -- the same
/// convention `CI` and `NO_COLOR` use): disables the header and the
/// network check that would refresh its cache. Named in this crate's
/// README under "Updating".
const NO_UPDATE_CHECK_ENV: &str = "HOUSERULES_NO_UPDATE_CHECK";

/// How long a cached "latest version" measurement stays good before this
/// process pays for another network round trip.
const CACHE_TTL: Duration = Duration::from_secs(24 * 60 * 60);

/// How long the network check may block before this process gives up and
/// degrades to silence, same as any other failure -- spec section 1: "no
/// command ever blocks on the network for the header".
const NETWORK_TIMEOUT: Duration = Duration::from_secs(3);

/// The cache file's own name, nested under the platform cache dir's
/// `houserules` subdirectory (`cache_path`).
const CACHE_FILE_NAME: &str = "update-check.json";

/// The whole state this header persists across invocations: when this
/// host last checked (successfully or not), and the latest version that
/// check -- or an earlier one -- ever found. `latest_version` is `None`
/// until the first check that ever reaches a release succeeds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Cache {
    checked_at_unix: u64,
    latest_version: Option<String>,
}

/// This header's one dependency on the outside world: a real release
/// query. A fake implementation exercises [`refresh_if_stale`] with no
/// network or receipt-file access at all.
trait UpdateSource {
    /// Returns the latest stable release's version. `Err` covers every
    /// failure this module treats alike: a network error, a timeout, a
    /// malformed `CARGO_PKG_REPOSITORY`, or axoupdater reporting no
    /// release at all.
    fn latest_stable_version(&mut self) -> Result<Version, String>;
}

/// [`UpdateSource`] backed by the real `axoupdater` crate, queried
/// receipt-free (module doc, "Receipt-free, by construction").
struct AxoupdaterSource;

impl UpdateSource for AxoupdaterSource {
    fn latest_stable_version(&mut self) -> Result<Version, String> {
        let (owner, name) =
            github_owner_and_name(env!("CARGO_PKG_REPOSITORY")).ok_or_else(|| {
                format!(
                    "CARGO_PKG_REPOSITORY {:?} is not a github.com owner/name URL",
                    env!("CARGO_PKG_REPOSITORY")
                )
            })?;

        let mut updater = AxoUpdater::new_for(name);
        updater.set_release_source(ReleaseSource {
            release_type: ReleaseSourceType::GitHub,
            owner: owner.to_string(),
            name: name.to_string(),
            app_name: name.to_string(),
        });

        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|error| error.to_string())?;

        // `tokio::time::timeout`'s `Sleep::new_timeout` calls
        // `scheduler::Handle::current()` as soon as it is constructed (and
        // asserts the time driver is enabled) -- it does not register with
        // the timer driver until first poll, but acquiring the runtime
        // handle already needs an entered context. So it must still be
        // built inside the `async` block `block_on` drives -- passing it
        // as a plain argument expression builds it on the calling thread,
        // before any runtime context is entered, and panics ("there is no
        // reactor running").
        match runtime.block_on(async {
            tokio::time::timeout(NETWORK_TIMEOUT, updater.query_new_version()).await
        }) {
            Err(_elapsed) => Err("update check timed out".to_string()),
            Ok(Err(error)) => Err(error.to_string()),
            Ok(Ok(None)) => Err("axoupdater reported no release for a resolved query".to_string()),
            Ok(Ok(Some(version))) => Ok(version.clone()),
        }
    }
}

/// Splits a GitHub repository URL (`CARGO_PKG_REPOSITORY`'s own value:
/// `https://github.com/<owner>/<name>`) into its owner and name, the two
/// fields `ReleaseSource` needs. Reads the manifest's own `repository`
/// field rather than duplicating the name a second time in this module
/// (`quality.principles`: DRY) -- `None` for anything that is not a
/// `github.com` URL with both segments present.
fn github_owner_and_name(repository_url: &str) -> Option<(&str, &str)> {
    let path = repository_url
        .strip_prefix("https://github.com/")
        .map(|path| path.trim_end_matches('/'))?;
    let (owner, name) = path.split_once('/')?;
    if owner.is_empty() || name.is_empty() || name.contains('/') {
        return None;
    }
    Some((owner, name))
}

/// True when the header must not run at all this invocation -- neither
/// the network refresh nor the print. Pure so the full gating matrix
/// (process.tdd) is tested without touching stderr, the environment, or
/// the network.
fn header_disabled(
    is_update_command: bool,
    ci: bool,
    no_update_check: bool,
    stderr_is_tty: bool,
) -> bool {
    is_update_command || ci || no_update_check || !stderr_is_tty
}

/// True once `cache` is more than [`CACHE_TTL`] behind `now`, there is no
/// cache at all, or `cache` itself is dated after `now` -- the one
/// condition [`refresh_if_stale`] uses to decide whether to pay for a
/// network round trip. A future `checked_at_unix` (clock skew, or a
/// cache file an adopter can hand-edit) is not trustworthy evidence of a
/// recent check either, the same way an unparsable `latest_version`
/// cannot be trusted (`header_line`'s own doc) -- treating it as fresh
/// would saturate `now - checked_at_unix` to zero and suppress the
/// header forever.
fn is_stale(cache: Option<&Cache>, now: SystemTime) -> bool {
    let Some(cache) = cache else {
        return true;
    };
    let now_unix = now.duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
    if cache.checked_at_unix > now_unix {
        return true;
    }
    now_unix - cache.checked_at_unix >= CACHE_TTL.as_secs()
}

/// Whether `latest` is a real, adoptable update over `current`: strictly
/// newer by semver precedence, and not itself a prerelease. The
/// prerelease exclusion is defense in depth over the network layer's own
/// filtering (module doc, "Prerelease is checked twice").
fn is_update_available(current: &Version, latest: &Version) -> bool {
    latest > current && latest.pre.is_empty()
}

/// The header line spec 2026-09-15-batch-27 §3 T3 defines, or `None`
/// when `latest_str` is not a genuine update over `current_str` --
/// including when either fails to parse as semver, which a hand-edited
/// cache file can always produce (`quality.absence-is-designed`: no
/// header is the designed state for a value this module cannot trust).
fn header_line(current_str: &str, latest_str: &str) -> Option<String> {
    let current = Version::parse(current_str).ok()?;
    let latest = Version::parse(latest_str).ok()?;
    if !is_update_available(&current, &latest) {
        return None;
    }
    Some(format!(
        "houserules {current} -> {latest} available; run houserules update"
    ))
}

/// Reads the cache file at `path`. An absent file, an unreadable one, or
/// one that is not the JSON shape [`Cache`] expects all read as "no
/// cache" -- the caller reacts by treating the check as stale and
/// refreshing it (`houserules.crash-paths-are-named`: a corrupt cache
/// degrades, it never breaks a command).
fn read_cache(path: &Path) -> Option<Cache> {
    let contents = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&contents).ok()
}

/// Writes `cache` to `path`, creating its parent directory if needed.
/// The caller only ever discards this `Result`: an unwritable cache
/// degrades this header to running the network check every invocation
/// that reaches it, never to breaking the command it hooks into.
fn write_cache(path: &Path, cache: &Cache) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string(cache)?;
    std::fs::write(path, json)
}

/// Runs the network check when `cache` is stale, and returns the cache
/// this invocation should read the header from: unchanged when the check
/// does not run, freshly written when it does. Every failure still
/// refreshes `checked_at_unix` and keeps `latest_version` from `cache`
/// verbatim (module doc, "Every failure refreshes the timestamp and
/// stays silent"). The write itself is best-effort: `write_cache`'s
/// `Result` is discarded here too, for the same reason it names in its
/// own doc.
fn refresh_if_stale(
    cache: Option<Cache>,
    now: SystemTime,
    path: &Path,
    source: &mut impl UpdateSource,
) -> Option<Cache> {
    if !is_stale(cache.as_ref(), now) {
        return cache;
    }
    let now_unix = now.duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
    let latest_version = match source.latest_stable_version() {
        Ok(version) => Some(version.to_string()),
        Err(_) => cache.and_then(|cache| cache.latest_version),
    };
    let refreshed = Cache {
        checked_at_unix: now_unix,
        latest_version,
    };
    let _ = write_cache(path, &refreshed);
    Some(refreshed)
}

/// The cache file's real, platform-appropriate path -- `None` only when
/// this host exposes no cache/home directory at all (`dirs::cache_dir`
/// returning `None`), which degrades this header to running disabled
/// rather than erroring. Honors `XDG_CACHE_HOME` on Linux the same way
/// every other `dirs::cache_dir` caller does, which is also how a live
/// run or a test points this header at a scratch directory without a
/// dedicated override of its own.
fn cache_path() -> Option<PathBuf> {
    Some(dirs::cache_dir()?.join("houserules").join(CACHE_FILE_NAME))
}

/// Prints the update-available header on stderr, exactly as spec
/// 2026-09-15-batch-27 §3 T3 defines it. Called once from `main`, before
/// command dispatch, and nowhere else. `is_update_command` disables the
/// whole check unconditionally: `update` acts on a newer release instead
/// of announcing one, and the two commands sharing one network budget per
/// [`CACHE_TTL`] would otherwise mean an adopter who just ran `update`
/// pays for a second, redundant check on their very next command.
pub(crate) fn maybe_print_header(is_update_command: bool) {
    if header_disabled(
        is_update_command,
        std::env::var("CI").is_ok(),
        std::env::var(NO_UPDATE_CHECK_ENV).is_ok(),
        std::io::stderr().is_terminal(),
    ) {
        return;
    }

    let Some(path) = cache_path() else {
        return;
    };
    let cache = read_cache(&path);
    let cache = refresh_if_stale(cache, SystemTime::now(), &path, &mut AxoupdaterSource);
    let Some(latest_version) = cache.and_then(|cache| cache.latest_version) else {
        return;
    };

    if let Some(line) = header_line(env!("CARGO_PKG_VERSION"), &latest_version) {
        eprintln!("{line}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A scripted [`UpdateSource`]: never touches the network.
    struct FakeSource(Result<Version, String>);

    impl UpdateSource for FakeSource {
        fn latest_stable_version(&mut self) -> Result<Version, String> {
            self.0.clone()
        }
    }

    /// An [`UpdateSource`] whose call would fail any test relying on it:
    /// `refresh_if_stale` must never query the network when the cache is
    /// still fresh (mirrors `selfupdate.rs`'s own `found_elsewhere`
    /// fixture doc).
    struct PanicsIfQueried;

    impl UpdateSource for PanicsIfQueried {
        fn latest_stable_version(&mut self) -> Result<Version, String> {
            panic!("the network must not be queried when the cache is not stale");
        }
    }

    fn unix(seconds: u64) -> SystemTime {
        UNIX_EPOCH + Duration::from_secs(seconds)
    }

    fn version(text: &str) -> Version {
        Version::parse(text).expect("valid test version")
    }

    // Gating matrix: tty / CI / env / update-command.
    #[test]
    fn header_runs_when_nothing_disables_it() {
        assert!(!header_disabled(false, false, false, true));
    }

    #[test]
    fn header_disabled_when_stderr_is_not_a_tty() {
        assert!(header_disabled(false, false, false, false));
    }

    #[test]
    fn header_disabled_under_ci() {
        assert!(header_disabled(false, true, false, true));
    }

    #[test]
    fn header_disabled_by_the_opt_out_env_var() {
        assert!(header_disabled(false, false, true, true));
    }

    #[test]
    fn header_disabled_for_the_update_command_itself() {
        assert!(header_disabled(true, false, false, true));
    }

    // Comparison matrix: newer / equal / older / prerelease.
    #[test]
    fn header_line_announces_a_newer_stable_release() {
        let line = header_line("1.0.0", "1.1.0").expect("a newer release announces");
        assert_eq!(
            line,
            "houserules 1.0.0 -> 1.1.0 available; run houserules update"
        );
    }

    #[test]
    fn header_line_is_none_when_versions_are_equal() {
        assert_eq!(header_line("1.0.0", "1.0.0"), None);
    }

    #[test]
    fn header_line_is_none_when_the_cached_version_is_older() {
        assert_eq!(header_line("1.1.0", "1.0.0"), None);
    }

    #[test]
    fn header_line_is_none_when_the_cached_version_is_a_prerelease() {
        // 1.1.0-rc.1 outranks 1.0.0 by (major, minor, patch) alone, so a
        // naive `>` comparison would announce it; the prerelease guard
        // must still refuse it.
        assert_eq!(header_line("1.0.0", "1.1.0-rc.1"), None);
    }

    #[test]
    fn header_line_is_none_when_either_version_fails_to_parse() {
        assert_eq!(header_line("not-a-version", "1.1.0"), None);
        assert_eq!(header_line("1.0.0", "not-a-version"), None);
    }

    #[test]
    fn is_update_available_treats_a_higher_prerelease_as_not_available() {
        assert!(!is_update_available(
            &version("1.0.0"),
            &version("1.1.0-rc.1")
        ));
    }

    // Cache read/write/TTL.
    #[test]
    fn read_cache_returns_none_when_the_file_is_absent() {
        let dir = tempfile::tempdir().expect("tempdir");
        assert_eq!(read_cache(&dir.path().join("update-check.json")), None);
    }

    #[test]
    fn read_cache_returns_none_when_the_file_is_not_valid_json() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("update-check.json");
        std::fs::write(&path, b"not json").expect("write corrupt cache");
        assert_eq!(read_cache(&path), None);
    }

    #[test]
    fn read_cache_round_trips_a_written_cache() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("nested").join("update-check.json");
        let cache = Cache {
            checked_at_unix: 1_700_000_000,
            latest_version: Some("1.2.3".to_string()),
        };
        write_cache(&path, &cache).expect("write cache");
        assert_eq!(read_cache(&path), Some(cache));
    }

    #[test]
    fn is_stale_is_true_with_no_cache() {
        assert!(is_stale(None, unix(1_000_000)));
    }

    #[test]
    fn is_stale_is_false_within_the_ttl() {
        let cache = Cache {
            checked_at_unix: 1_000_000,
            latest_version: None,
        };
        assert!(!is_stale(Some(&cache), unix(1_000_000 + 60)));
    }

    #[test]
    fn is_stale_is_true_once_the_ttl_has_elapsed() {
        let cache = Cache {
            checked_at_unix: 1_000_000,
            latest_version: None,
        };
        assert!(is_stale(
            Some(&cache),
            unix(1_000_000 + CACHE_TTL.as_secs())
        ));
    }

    // Fix round 1, review Minor finding on update_check.rs:201-207: a
    // future-dated checked_at_unix (clock skew, or a hand edit) must not
    // saturate to "never stale" and permanently suppress the header.
    #[test]
    fn is_stale_is_true_when_checked_at_unix_is_in_the_future() {
        let cache = Cache {
            checked_at_unix: 4_102_444_800, // year 2100
            latest_version: None,
        };
        assert!(is_stale(Some(&cache), unix(1_000_000)));
    }

    // refresh_if_stale.
    #[test]
    fn refresh_if_stale_returns_the_existing_cache_unchanged_when_not_stale() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("update-check.json");
        let cache = Cache {
            checked_at_unix: 1_000_000,
            latest_version: Some("1.0.0".to_string()),
        };
        let result = refresh_if_stale(
            Some(cache.clone()),
            unix(1_000_000 + 60),
            &path,
            &mut PanicsIfQueried,
        );
        assert_eq!(result, Some(cache));
        assert!(!path.exists(), "a fresh cache must not be rewritten");
    }

    #[test]
    fn refresh_if_stale_writes_a_fresh_cache_on_a_successful_check() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("update-check.json");
        let mut source = FakeSource(Ok(version("2.0.0")));
        let result = refresh_if_stale(None, unix(500), &path, &mut source);
        let expected = Cache {
            checked_at_unix: 500,
            latest_version: Some("2.0.0".to_string()),
        };
        assert_eq!(result, Some(expected.clone()));
        assert_eq!(read_cache(&path), Some(expected));
    }

    #[test]
    fn refresh_if_stale_keeps_the_previous_latest_version_on_a_network_failure() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("update-check.json");
        let stale = Cache {
            checked_at_unix: 100,
            latest_version: Some("1.5.0".to_string()),
        };
        let mut source = FakeSource(Err("network unreachable".to_string()));
        let result = refresh_if_stale(
            Some(stale),
            unix(100 + CACHE_TTL.as_secs()),
            &path,
            &mut source,
        );
        let expected = Cache {
            checked_at_unix: 100 + CACHE_TTL.as_secs(),
            latest_version: Some("1.5.0".to_string()),
        };
        assert_eq!(result, Some(expected.clone()));
        assert_eq!(read_cache(&path), Some(expected));
    }

    #[test]
    fn refresh_if_stale_refreshes_the_timestamp_with_no_prior_cache_on_failure() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("update-check.json");
        let mut source = FakeSource(Err("network unreachable".to_string()));
        let result = refresh_if_stale(None, unix(42), &path, &mut source);
        let expected = Cache {
            checked_at_unix: 42,
            latest_version: None,
        };
        assert_eq!(result, Some(expected.clone()));
        assert_eq!(read_cache(&path), Some(expected));
    }

    // github_owner_and_name.
    #[test]
    fn github_owner_and_name_splits_a_well_formed_github_url() {
        assert_eq!(
            github_owner_and_name("https://github.com/jblossey/houserules"),
            Some(("jblossey", "houserules"))
        );
    }

    #[test]
    fn github_owner_and_name_is_none_for_a_non_github_url() {
        assert_eq!(
            github_owner_and_name("https://example.com/jblossey/houserules"),
            None
        );
    }

    #[test]
    fn github_owner_and_name_is_none_when_a_segment_is_missing() {
        assert_eq!(github_owner_and_name("https://github.com/jblossey"), None);
    }

    #[test]
    fn github_owner_and_name_reads_this_crates_own_repository_field() {
        // Guards the DRY claim in this function's own doc comment: this
        // project's manifest must keep naming a plain owner/name GitHub
        // URL for the network path to resolve at all.
        assert_eq!(
            github_owner_and_name(env!("CARGO_PKG_REPOSITORY")),
            Some(("jblossey", "houserules"))
        );
    }
}
