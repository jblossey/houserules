//! CLI-level pin for the update-available header (HR-134,
//! `update_check.rs`'s own module doc has the full account): stdout is a
//! JSON consumer's contract surface, and this header must never land on
//! it. Follows `check_commit.rs`'s structural pattern (a real subprocess
//! against this checkout's own repository, this file's own small copy of
//! the shared helpers) for the same reason that file's doc gives.
//!
//! Two tests split the contract because no single run can carry both
//! halves. `the_update_header_never_reaches_stdout_for_a_json_command`
//! runs with both streams piped, exactly how `cargo test` (and most
//! automation) invokes this binary: `header_disabled`'s own TTY gate
//! returns before `eprintln!` ever runs, so this pins the realistic,
//! header-never-fires case, on every platform. That gate is itself what a
//! regression could remove, which the mutation this file's own
//! `task-3-report.json` `tdd` entry discloses (temporarily dropping the
//! TTY term from `header_disabled`) proved by making this test fail with
//! the header on stderr.
//!
//! Fix round 1 (review, Important): that first test cannot tell stderr
//! from stdout on the shipped code, since the gate returns before either
//! stream is written to -- swapping `eprintln!` for `println!` at the
//! print site leaves it green. `the_update_header_prints_only_on_a_real_stderr_terminal`
//! (`#[cfg(unix)]`) closes that gap: it gives the child a real POSIX pty
//! as its stderr and an ordinary pipe as its stdout, so `is_terminal()`
//! is genuinely true and the header genuinely prints, landing on
//! whichever of the two streams the print site actually writes to. A
//! disclosed mutation (swapping that `eprintln!` for `println!`) is this
//! file's own second `tdd` entry: RED, the header appears on the piped
//! stdout instead of the pty; reverted, GREEN.

use std::path::{Path, PathBuf};
use std::process::Command;

/// A `Command` for the compiled `houserules` binary under test --
/// `check_commit.rs`'s own copy of this helper. Sets
/// `HOUSERULES_SKIP_SELF_UPDATE` so `update`'s self-update phase never
/// runs here, and removes `CI`/`HOUSERULES_NO_UPDATE_CHECK` so this
/// file's own header gate is exercised by the TTY term alone, not by
/// whatever the outer test runner's environment happens to carry.
fn houserules() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_houserules"));
    command
        .env("HOUSERULES_SKIP_SELF_UPDATE", "1")
        .env_remove("CI")
        .env_remove("HOUSERULES_NO_UPDATE_CHECK");
    command
}

/// This checkout's repository root -- `check_commit.rs`'s own copy.
fn repo_root() -> PathBuf {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    root.canonicalize()
        .unwrap_or_else(|error| panic!("canonicalize {}: {error}", root.display()))
}

/// A cache directory seeded so the header WOULD announce an update if the
/// TTY gate were the only thing standing in its way: a fresh timestamp
/// (so no network call is even attempted) naming a release far newer
/// than any real `CARGO_PKG_VERSION` this crate will ever reach.
///
/// Fix round 4 (CI red on macOS a second time, T3's own round-2 race
/// theory was not the cause): `dirs::cache_dir()` (`update_check.rs`'s
/// own `cache_path` doc) reads `XDG_CACHE_HOME`/`HOME` only on Linux;
/// with `HOME` set to `dir`, macOS resolves it to `dir/Library/Caches`
/// instead, never `dir/houserules` -- a layout this fixture never wrote
/// to. Both `stderr`-gated tests below open their TTY gate (the piped
/// test's TTY term is the only thing false there; the pty test's is
/// true), so on macOS `read_cache` found nothing, `is_stale` (correctly)
/// called it stale, and `the_update_header_prints_only_on_a_real_stderr_terminal`
/// made a real, live network call to GitHub from CI -- which is itself
/// the bug this fixture must not cause a test to trigger, quite apart
/// from the empty pty stderr that call's own real result (1.0.0 is not
/// newer than 1.0.0) then produced. Writing the doctored cache at both
/// layouts unconditionally, with no `cfg`, means whichever one
/// `dirs::cache_dir()` actually resolves to on the platform running this
/// test is always the one already seeded with a fresh timestamp, so the
/// network call this fixture exists to prevent is never made on any of
/// them; the layout the running platform does not use sits unread and
/// harmless.
fn scratch_cache_claiming_a_newer_release() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock after epoch")
        .as_secs();
    let seeded_cache = format!(r#"{{"checked_at_unix":{now},"latest_version":"999.0.0"}}"#);
    for layout in ["houserules", "Library/Caches/houserules"] {
        let cache_dir = dir.path().join(layout);
        std::fs::create_dir_all(&cache_dir).expect("create cache dir");
        std::fs::write(cache_dir.join("update-check.json"), &seeded_cache)
            .expect("write seeded cache");
    }
    dir
}

/// A JSON-emitting command (`get`) with a cache that would announce an
/// update if the header's TTY gate were bypassed must still produce
/// clean, header-free stdout -- and, since a real subprocess run this
/// way never has a TTY on either stream, header-free stderr too (a
/// header that fired here at all would mean the TTY gate broke, which is
/// itself the regression this test exists to catch, independent of
/// which stream it landed on). This test's own TTY gate is closed
/// (`stderr_is_tty` is false on a piped subprocess), so whether the
/// seeded cache is even found never changes its own outcome; it still
/// gets the fixture's cross-platform seeding (`scratch_cache_claiming_a_newer_release`'s
/// own doc) for free, and needs no separate one. Runs on every platform;
/// see this file's own module doc for why a second, unix-only test
/// carries the rest of the contract.
#[test]
fn the_update_header_never_reaches_stdout_for_a_json_command() {
    let cache = scratch_cache_claiming_a_newer_release();

    let output = houserules()
        .arg("get")
        .arg("houserules.pnpm-only")
        .current_dir(repo_root())
        .env("XDG_CACHE_HOME", cache.path())
        .env("HOME", cache.path())
        .output()
        .expect("run houserules get");

    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");

    assert!(output.status.success(), "stderr: {stderr}");
    assert!(
        stdout.trim_start().starts_with('['),
        "get's stdout must stay valid JSON: {stdout:?}"
    );
    assert!(
        !stdout.contains("available; run houserules update"),
        "the update header must never reach stdout: {stdout:?}"
    );
    assert!(
        !stderr.contains("available; run houserules update"),
        "a piped, non-tty run must print no header at all: {stderr:?}"
    );
}

/// A minimal POSIX pty allocator, `#[cfg(unix)]` only: the one way to
/// make `std::io::IsTerminal::is_terminal()` genuinely true for a stream
/// `cargo test`'s own harness never gives a real terminal
/// (`houserules.platform-gated-tests`: gated rather than attempted on
/// Windows, which has no POSIX pty and needs a ConPTY-based equivalent
/// this fix does not add). Uses the `libc` crate rather than hand-typed
/// flag values: `O_NOCTTY`'s own numeric value differs between Linux and
/// macOS, and getting it wrong here would silently build a pty that
/// still attaches as this test process's controlling terminal.
#[cfg(unix)]
mod pty {
    use std::ffi::CStr;
    use std::io;
    use std::os::fd::{FromRawFd, OwnedFd};
    use std::path::PathBuf;
    use std::sync::Mutex;

    /// Guards `libc::ptsname`, which is not reentrant -- this module has
    /// exactly one caller today, but the guard costs nothing and outlives
    /// a future second one.
    static PTSNAME_LOCK: Mutex<()> = Mutex::new(());

    /// One end of a fresh POSIX pty pair. `master` is this process's own
    /// end; `slave_path` is the path the child opens as its stderr.
    /// Nothing here emulates a real line-disciplined terminal beyond
    /// `is_terminal()` returning true for an fd opened from `slave_path`
    /// -- the one property this test needs.
    pub struct Pty {
        pub master: OwnedFd,
        pub slave_path: PathBuf,
    }

    /// Opens a pty pair via `posix_openpt`/`grantpt`/`unlockpt`/`ptsname`
    /// -- the same four-call sequence `man pty` documents.
    pub fn open() -> Pty {
        // SAFETY: each call's precondition is the previous call's
        // documented success; `ptsname`'s returned pointer is copied into
        // an owned `PathBuf` before the mutex guard (its validity window)
        // is released, and `master_fd` becomes an `OwnedFd` immediately
        // after the last fallible call on it.
        unsafe {
            let master_fd = libc::posix_openpt(libc::O_RDWR | libc::O_NOCTTY);
            assert!(
                master_fd >= 0,
                "posix_openpt: {}",
                io::Error::last_os_error()
            );
            assert_eq!(
                libc::grantpt(master_fd),
                0,
                "grantpt: {}",
                io::Error::last_os_error()
            );
            assert_eq!(
                libc::unlockpt(master_fd),
                0,
                "unlockpt: {}",
                io::Error::last_os_error()
            );
            let slave_path = {
                let _guard = PTSNAME_LOCK
                    .lock()
                    .unwrap_or_else(|poison| poison.into_inner());
                let name = libc::ptsname(master_fd);
                assert!(!name.is_null(), "ptsname: {}", io::Error::last_os_error());
                PathBuf::from(CStr::from_ptr(name).to_str().expect("ptsname is utf8"))
            };
            Pty {
                master: OwnedFd::from_raw_fd(master_fd),
                slave_path,
            }
        }
    }
}

/// Fix round 1, review Important finding on `tests/update_check.rs:75-102`:
/// gives the child a real pty as stderr and an ordinary pipe as stdout,
/// so `header_disabled`'s TTY gate genuinely opens and the header
/// genuinely prints -- landing on whichever stream the print call
/// actually targets, the property the piped test above cannot observe.
/// The doctored cache is the same shape; the assertions are the mirror
/// image of the piped test's (the header MUST appear, on stderr, and
/// MUST NOT appear on stdout).
///
/// Fix round 2 (CI red on macOS, T3 r2 review's own flagged risk): the
/// pty master must be read by a thread already blocked in `read()`
/// before the child's write ever happens, not only before `child.wait()`
/// -- on Darwin, a master read that STARTS after every slave-side fd has
/// already closed returns `EIO` immediately, discarding whatever the pty
/// driver was still holding, where Linux instead drains the buffered
/// bytes first and only then returns `EIO` (confirmed on this box: a
/// pty hangup reports as `EIO`, never `Ok(0)`, once the buffer is
/// drained). `get houserules.pnpm-only` is fast enough that the fix
/// round 1 shape (read the master inline, after spawning a separate
/// thread for stdout first) left a real window on a loaded CI runner: by
/// the time that inline read's first `read()` syscall ran, the child
/// could already have written its header line, exited, and closed its
/// own slave fd, so the master's first read landed in the post-close
/// state Darwin discards from. Spawning the pty reader thread as the
/// very first thing after `spawn`, reading in a loop that treats an
/// `EIO` exactly like `Ok(0)` (stop, keep whatever was already read),
/// closes that window on both platforms: the thread is normally already
/// parked in `read()` well before the child's own `execve` and `main`
/// even run, and on the rare loss race it still keeps every byte read
/// before the eventual close, on either platform's semantics. The loop
/// itself is not what protects already-read bytes -- `Read::read_to_end`
/// (which `read_to_string` calls) keeps its accumulated buffer on any
/// error per its own doc ("Any bytes which have already been read will
/// be appended to buf"), `ErrorKind::Interrupted` included (its own doc:
/// "should be retried"; confirmed on this box too: a pty fed bytes then
/// hung up leaves `read_to_string` returning `Err(EIO)` with every byte
/// already read still in the string). The loop exists only so this
/// thread's own `read()` call can start the instant the thread runs, and
/// so the eventual, expected `EIO` -- or a signal-interrupted read along
/// the way -- does not end the test.
#[cfg(unix)]
#[test]
fn the_update_header_prints_only_on_a_real_stderr_terminal() {
    use std::fs::File;
    use std::io::Read;
    use std::process::Stdio;

    let cache = scratch_cache_claiming_a_newer_release();
    let pty = pty::open();
    let slave = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&pty.slave_path)
        .unwrap_or_else(|error| panic!("open {}: {error}", pty.slave_path.display()));

    let mut command = houserules();
    command
        .arg("get")
        .arg("houserules.pnpm-only")
        .current_dir(repo_root())
        .env("XDG_CACHE_HOME", cache.path())
        .env("HOME", cache.path())
        .stdout(Stdio::piped())
        .stderr(Stdio::from(slave));

    let mut child = command.spawn().expect("spawn houserules");
    // The parent's own copy of the pty slave (opened above) must close
    // before the pty master can ever read EOF: `Stdio::from` gave the
    // child's dup'd copy to `command`, but `command` itself still owns
    // that copy until dropped, and the file object above already went
    // out of scope into `Stdio::from`, so dropping `command` now is what
    // actually closes the parent-side reference.
    drop(command);

    // Spawned first, before even touching stdout: every instruction
    // between `spawn` above and this thread's own first `read()` widens
    // the race this module doc describes, so nothing else runs first.
    let mut master = File::from(pty.master);
    let stderr_thread = std::thread::spawn(move || {
        let mut collected = Vec::new();
        let mut chunk = [0u8; 4096];
        loop {
            match master.read(&mut chunk) {
                Ok(0) => break,
                Ok(n) => collected.extend_from_slice(&chunk[..n]),
                // A signal-interrupted read is not a failure -- `Read::read`'s
                // own contract says it "should be retried"; `read_to_end`
                // (module doc) does this internally, so a hand-rolled loop
                // must too, or a bare signal turns a passing run red.
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                // Darwin's post-close EIO and Linux's own post-hangup EIO
                // (module doc) both mean "nothing more will ever arrive" --
                // the ordinary, expected way this pty ends once the child
                // has exited, not a failure to report.
                Err(error) if error.raw_os_error() == Some(libc::EIO) => break,
                Err(error) => panic!("read pty master: {error}"),
            }
        }
        collected
    });

    let mut stdout_pipe = child.stdout.take().expect("piped stdout");
    let stdout_thread = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = stdout_pipe.read_to_end(&mut buf);
        buf
    });

    let status = child.wait().expect("wait for houserules");
    let stderr_from_pty =
        String::from_utf8(stderr_thread.join().expect("join pty reader")).expect("utf8 stderr");
    let stdout =
        String::from_utf8(stdout_thread.join().expect("join stdout reader")).expect("utf8 stdout");

    assert!(status.success(), "stderr (pty): {stderr_from_pty}");
    assert!(
        stdout.trim_start().starts_with('['),
        "get's stdout must stay valid JSON: {stdout:?}"
    );
    assert!(
        !stdout.contains("available; run houserules update"),
        "the update header must never reach stdout: {stdout:?}"
    );
    let expected_header = format!(
        "houserules {} -> 999.0.0 available; run houserules update",
        env!("CARGO_PKG_VERSION")
    );
    assert!(
        stderr_from_pty.contains(&expected_header),
        "a real stderr terminal must show the header: {stderr_from_pty:?}"
    );
}
