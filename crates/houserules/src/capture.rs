//! `houserules capture`: runs a command, records its output under a header,
//! and reads such a file back.
//!
//! A capture file holds the [`Header`] as one JSON object on line 1. The output
//! of the command follows the first LF. The output is UTF-8 and ends where the
//! command's output ends: a command that wrote nothing leaves the header alone,
//! with no LF after it.
//!
//! `capture` streams the output of the command into a temporary file in the
//! directory of the capture, converts it to UTF-8 while it writes a second
//! temporary file that starts with the header, and persists that file at the
//! path of the capture. A reader never sees a partial capture. A run that ends
//! with an error removes both temporary files; a `capture` that a signal kills
//! can leave a file that starts with `.tmp` in that directory.

use std::ffi::OsString;
use std::fmt;
use std::fs;
use std::io::{self, BufWriter, Read, Seek, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, ExitStatus, Stdio};

use jiff::Timestamp;
use serde::de::DeserializeOwned;
use serde_json::{Map, Value};
use tempfile::NamedTempFile;

pub(crate) use crate::published::capture::Header;

/// The `schema` value that `capture` writes. It equals the schema id of
/// [`Header`], `published::id::<Header>()`.
pub(crate) const SCHEMA: &str = "houserules/capture@1.0.0";

/// The most bytes of command output that `capture` holds in memory at once
/// while it converts the output to UTF-8.
const CONVERT_BUFFER_LEN: usize = 64 * 1024;

/// A capture file read back: its header and its bytes.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Capture {
    /// The parsed header.
    pub(crate) header: Header,
    /// The whole file, header line included.
    pub(crate) bytes: Vec<u8>,
}

impl Capture {
    /// The output of the command: the bytes after the first LF. It is empty
    /// when the file is the header alone.
    pub(crate) fn output(&self) -> &[u8] {
        match self.bytes.iter().position(|byte| *byte == b'\n') {
            Some(at) => &self.bytes[at + 1..],
            None => &[],
        }
    }

    /// The lowercase hex SHA-256 of the whole file.
    pub(crate) fn sha256(&self) -> String {
        crate::baseline::hash(&self.bytes)
    }
}

/// Why [`read`] gave no capture.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum ReadError {
    /// No file exists at the path.
    Missing,
    /// The file exists and the operating system refused the read; the text is
    /// the operating system's own error.
    Unreadable(String),
    /// The file is no capture; the text is the reason, one of four.
    NoCapture(String),
}

/// Reads the capture file at `path`.
///
/// The first line must parse as a JSON object that holds every key of the
/// [`Header`], each with a value of the field's type, and whose `schema` is
/// `houserules/capture@1.x.y`. A key whose value has the wrong type counts as
/// missing. The `argv` of a capture holds no NUL byte, which no operating
/// system accepts in an argument; an `argv` with one counts as missing.
pub(crate) fn read(path: &Path) -> Result<Capture, ReadError> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Err(ReadError::Missing),
        Err(error) => return Err(ReadError::Unreadable(error.to_string())),
    };
    let header = parse_header(&bytes).map_err(ReadError::NoCapture)?;
    Ok(Capture { header, bytes })
}

/// The header on line 1 of `bytes`, or the reason that `bytes` is no capture.
pub(crate) fn parse_header(bytes: &[u8]) -> Result<Header, String> {
    if bytes.is_empty() {
        return Err("it is empty".to_string());
    }
    let first_line = bytes
        .split(|byte| *byte == b'\n')
        .next()
        .unwrap_or_default();
    let Ok(Value::Object(mut keys)) = serde_json::from_slice(first_line) else {
        return Err("its first line is no JSON object".to_string());
    };
    let schema: String = take_key(&mut keys, "schema")?;
    if !is_schema_of_major_1(&schema) {
        return Err(format!("its schema is {schema}, not houserules/capture@1"));
    }
    let header = Header {
        schema,
        argv: take_key(&mut keys, "argv")?,
        cwd: take_key(&mut keys, "cwd")?,
        start: take_key(&mut keys, "start")?,
        end: take_key(&mut keys, "end")?,
        exit: take_key(&mut keys, "exit")?,
        signal: take_key(&mut keys, "signal")?,
        head: take_key(&mut keys, "head")?,
        dirty: take_key(&mut keys, "dirty")?,
        lossy: take_key(&mut keys, "lossy")?,
    };
    if header.argv.iter().any(|argument| argument.contains('\0')) {
        return Err(lacks("argv"));
    }
    Ok(header)
}

/// The reason for a key that the header lacks.
fn lacks(key: &str) -> String {
    format!("its header lacks {key}")
}

/// Removes `key` from `keys` and reads its value as a `T`; a missing key and a
/// value of another type both give the reason that the header lacks the key.
fn take_key<T: DeserializeOwned>(keys: &mut Map<String, Value>, key: &str) -> Result<T, String> {
    keys.remove(key)
        .and_then(|value| serde_json::from_value(value).ok())
        .ok_or_else(|| lacks(key))
}

/// Whether `schema` is `houserules/capture@1.x.y`, with `x` and `y` numbers.
fn is_schema_of_major_1(schema: &str) -> bool {
    schema
        .strip_prefix("houserules/capture@1.")
        .is_some_and(|version| {
            let numbers: Vec<&str> = version.split('.').collect();
            numbers.len() == 2
                && numbers
                    .iter()
                    .all(|number| !number.is_empty() && number.bytes().all(|b| b.is_ascii_digit()))
        })
}

/// Copies `raw` to `sink` as UTF-8 and returns the number of invalid byte
/// sequences, each replaced with one U+FFFD, as `String::from_utf8_lossy`
/// replaces them. It holds at most `buffer_len` bytes at a time; a sequence
/// that a read cuts in two stays whole. `buffer_len` is at least 4.
fn convert_lossy(
    mut raw: impl Read,
    buffer_len: usize,
    mut sink: impl FnMut(&[u8]) -> io::Result<()>,
) -> io::Result<u64> {
    const REPLACEMENT: &[u8] = "\u{fffd}".as_bytes();
    let mut buffer = vec![0; buffer_len];
    let mut held = 0;
    let mut lossy = 0;
    loop {
        let read = match raw.read(&mut buffer[held..]) {
            Ok(read) => read,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        };
        let at_end = read == 0;
        let filled = held + read;
        // An invalid sequence at the end of the buffer may be a sequence that
        // the next read completes; it waits for that read.
        let waiting = if at_end {
            0
        } else {
            buffer[..filled]
                .utf8_chunks()
                .last()
                .map_or(0, |chunk| chunk.invalid().len())
        };
        let ready = filled - waiting;
        for chunk in buffer[..ready].utf8_chunks() {
            if !chunk.valid().is_empty() {
                sink(chunk.valid().as_bytes())?;
            }
            if !chunk.invalid().is_empty() {
                sink(REPLACEMENT)?;
                lossy += 1;
            }
        }
        if at_end {
            return Ok(lossy);
        }
        buffer.copy_within(ready..filled, 0);
        held = waiting;
    }
}

/// The text of `error` as the operating system gives it. `tempfile` adds
/// ` at path "<temporary file>"` to the errors of a temporary file, and the
/// name of a temporary file says nothing to the person who reads the line.
/// `report-facts` prints the errors of its own temporary file the same way.
pub(crate) fn os_error_text(error: &io::Error) -> String {
    let text = error.to_string();
    match text.rfind(" at path \"") {
        Some(at) => text[..at].to_string(),
        None => text,
    }
}

/// A reason that `capture` stops. Its text follows `capture: ` on stderr.
#[derive(Debug)]
enum CaptureError {
    /// `--out` exists and `--force` is absent.
    Exists { file: String },
    /// The directory of `--out` does not exist.
    NoDirectory { dir: String },
    /// Argument `index` of the command is not UTF-8.
    ArgumentNotUtf8 { index: usize },
    /// The current directory is not UTF-8.
    CurrentDirNotUtf8,
    /// The operating system cannot name the current directory.
    CurrentDirUnreadable(io::Error),
    /// The `--out` path is not UTF-8.
    OutNotUtf8,
    /// The command did not start.
    CannotStart { program: String, error: io::Error },
    /// The operating system gave no exit status for the command.
    CannotWait { program: String, error: io::Error },
    /// The capture file or a temporary file could not be written; `error` is
    /// the text of the operating system's error (see [`os_error_text`]).
    CannotWrite { file: String, error: String },
}

impl fmt::Display for CaptureError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Exists { file } => write!(formatter, "{file} exists; pass --force to replace it"),
            Self::NoDirectory { dir } => write!(formatter, "{dir} does not exist"),
            Self::ArgumentNotUtf8 { index } => write!(formatter, "argument {index} is not UTF-8"),
            Self::CurrentDirNotUtf8 => write!(formatter, "the current directory is not UTF-8"),
            Self::CurrentDirUnreadable(error) => {
                write!(formatter, "could not read the current directory: {error}")
            }
            Self::OutNotUtf8 => write!(formatter, "the --out path is not UTF-8"),
            Self::CannotStart { program, error } => {
                write!(formatter, "could not start {program}: {error}")
            }
            Self::CannotWait { program, error } => {
                write!(formatter, "could not wait for {program}: {error}")
            }
            Self::CannotWrite { file, error } => {
                write!(formatter, "could not write {file}: {error}")
            }
        }
    }
}

/// How a command ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Ended {
    /// The command exited with this code.
    Exit(i32),
    /// This signal ended the command.
    Signal(i32),
}

impl Ended {
    /// How `status` ended the command.
    fn of(status: ExitStatus) -> Self {
        #[cfg(unix)]
        let signal = std::os::unix::process::ExitStatusExt::signal(&status);
        #[cfg(not(unix))]
        let signal = None::<i32>;
        match (status.code(), signal) {
            (Some(code), _) => Self::Exit(code),
            (None, Some(signal)) => Self::Signal(signal),
            (None, None) => unreachable!("an exit status holds a code or a signal"),
        }
    }

    /// The exit status of `capture` itself: the code, or 128 plus the signal.
    fn process_code(self) -> i32 {
        match self {
            Self::Exit(code) => code,
            Self::Signal(signal) => 128 + signal,
        }
    }
}

/// The command and its arguments as UTF-8 strings.
fn utf8_argv(command: Vec<OsString>) -> Result<Vec<String>, CaptureError> {
    command
        .into_iter()
        .enumerate()
        .map(|(index, argument)| {
            argument
                .into_string()
                .map_err(|_| CaptureError::ArgumentNotUtf8 { index })
        })
        .collect()
}

/// The text of a `git` run in `dir` that succeeded, or `None`. `git` runs with
/// `LC_ALL=C` and without the optional index refresh, so it writes nothing.
fn git_output(dir: &Path, args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .args(args)
        .env("LC_ALL", "C")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .current_dir(dir)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8(output.stdout).ok()
}

/// The full sha of HEAD and whether the work tree is dirty, both read at
/// `dir`. Untracked files count whatever `status.showUntrackedFiles` says. Both
/// are `None` unless `dir` is inside a git work tree that has a commit and
/// `git status` succeeds: `dirty` is `None` exactly when `head` is.
fn git_facts(dir: &Path) -> (Option<String>, Option<bool>) {
    let inside = git_output(dir, &["rev-parse", "--is-inside-work-tree"])
        .is_some_and(|text| text.trim() == "true");
    if !inside {
        return (None, None);
    }
    let Some(head) = git_output(dir, &["rev-parse", "HEAD"])
        .map(|text| text.trim().to_string())
        .filter(|sha| !sha.is_empty())
    else {
        return (None, None);
    };
    match git_output(dir, &["status", "--porcelain", "--untracked-files=normal"]) {
        Some(status) => (Some(head), Some(!status.is_empty())),
        None => (None, None),
    }
}

/// The current time as RFC 3339 in UTC with nine fractional digits.
fn now() -> String {
    format!("{:.9}", Timestamp::now())
}

/// A finished run: how it ended and the line that `capture` prints.
struct Finished {
    ended: Ended,
    line: String,
}

/// The setup tests, the run, and the write of `out`.
fn run(out: &Path, force: bool, command: Vec<OsString>) -> Result<Finished, CaptureError> {
    let argv = utf8_argv(command)?;
    let cwd = std::env::current_dir().map_err(CaptureError::CurrentDirUnreadable)?;
    let cwd_text = cwd.to_str().ok_or(CaptureError::CurrentDirNotUtf8)?;
    let out_text = out.to_str().ok_or(CaptureError::OutNotUtf8)?;
    let dir = out
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    if !dir.exists() {
        return Err(CaptureError::NoDirectory {
            dir: dir.display().to_string(),
        });
    }
    if !force && fs::symlink_metadata(out).is_ok() {
        return Err(CaptureError::Exists {
            file: out_text.to_string(),
        });
    }
    let (head, dirty) = git_facts(&cwd);
    let cannot_write = |error: io::Error| CaptureError::CannotWrite {
        file: out_text.to_string(),
        error: os_error_text(&error),
    };
    let mut raw = NamedTempFile::new_in(dir).map_err(cannot_write)?;

    let (program, arguments) = argv.split_first().expect("clap requires a command");
    let cannot_start = |error| CaptureError::CannotStart {
        program: program.clone(),
        error,
    };
    let (mut reader, writer) = io::pipe().map_err(cannot_start)?;
    let mut process = Command::new(program);
    process
        .args(arguments)
        .stdin(Stdio::null())
        .stdout(writer.try_clone().map_err(cannot_start)?)
        .stderr(writer);
    let start = now();
    let mut child = process.spawn().map_err(cannot_start)?;
    // The command holds the write end of the pipe; the reader sees the end of
    // the output once the child and this process have both closed it.
    drop(process);
    if let Err(error) = io::copy(&mut reader, &mut raw) {
        // The child may block on a pipe that nobody reads; end it first.
        let _ = child.kill();
        let _ = child.wait();
        return Err(cannot_write(error));
    }
    let status = child.wait().map_err(|error| CaptureError::CannotWait {
        program: program.clone(),
        error,
    })?;
    let end = now();

    let ended = Ended::of(status);
    let (exit, signal) = match ended {
        Ended::Exit(code) => (Some(code), None),
        Ended::Signal(signal) => (None, Some(signal)),
    };
    let lossy = convert_lossy(
        rewound(&mut raw).map_err(cannot_write)?,
        CONVERT_BUFFER_LEN,
        |_| Ok(()),
    )
    .map_err(cannot_write)?;
    let header = Header {
        schema: SCHEMA.to_string(),
        argv,
        cwd: cwd_text.to_string(),
        start,
        end,
        exit,
        signal,
        head,
        dirty,
        lossy,
    };
    let mut file = new_capture_file(dir).map_err(cannot_write)?;
    write_capture(&mut file, &header, &mut raw).map_err(cannot_write)?;
    let persisted = if force {
        file.persist(out)
    } else {
        file.persist_noclobber(out)
    };
    persisted.map_err(|failure| {
        if !force && failure.error.kind() == io::ErrorKind::AlreadyExists {
            CaptureError::Exists {
                file: out_text.to_string(),
            }
        } else {
            cannot_write(failure.error)
        }
    })?;
    let line = match ended {
        Ended::Exit(code) => format!("{out_text}: exit {code}"),
        Ended::Signal(signal) => format!("{out_text}: ended by signal {signal}"),
    };
    Ok(Finished { ended, line })
}

/// The temporary file that becomes the capture. On Unix it gets the mode of a
/// file that a shell redirect creates, 0666 less the umask of the process; the
/// temporary file of `tempfile` alone is 0600.
#[cfg(unix)]
fn new_capture_file(dir: &Path) -> io::Result<NamedTempFile> {
    use std::os::unix::fs::PermissionsExt;

    tempfile::Builder::new()
        .permissions(fs::Permissions::from_mode(0o666))
        .tempfile_in(dir)
}

/// The temporary file that becomes the capture.
#[cfg(not(unix))]
fn new_capture_file(dir: &Path) -> io::Result<NamedTempFile> {
    NamedTempFile::new_in(dir)
}

/// `raw`, read again from its start.
fn rewound(raw: &mut NamedTempFile) -> io::Result<&mut NamedTempFile> {
    raw.rewind()?;
    Ok(raw)
}

/// Writes `header`, then a LF and the output when `raw` holds any, to `file`.
fn write_capture(
    file: &mut NamedTempFile,
    header: &Header,
    raw: &mut NamedTempFile,
) -> io::Result<()> {
    let mut writer = BufWriter::new(file);
    serde_json::to_writer(&mut writer, header)?;
    if raw.as_file().metadata()?.len() > 0 {
        writer.write_all(b"\n")?;
        convert_lossy(rewound(raw)?, CONVERT_BUFFER_LEN, |bytes| {
            writer.write_all(bytes)
        })?;
    }
    writer.flush()
}

/// Runs `command` and writes its capture to `out`, then ends the process with
/// the exit code of the command, or 128 plus the signal that ended it.
/// A setup error prints one line on stderr and returns exit status 2 with no
/// file written and no command started.
pub(crate) fn cmd_capture(out: PathBuf, force: bool, command: Vec<OsString>) -> ExitCode {
    match run(&out, force, command) {
        Ok(finished) => {
            println!("{}", finished.line);
            std::process::exit(finished.ended.process_code())
        }
        Err(error) => {
            eprintln!("capture: {error}");
            ExitCode::from(2)
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    /// A reader that hands out at most `step` bytes per call.
    struct Trickle<'a> {
        rest: &'a [u8],
        step: usize,
    }

    impl Read for Trickle<'_> {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            let len = self.step.min(buf.len()).min(self.rest.len());
            buf[..len].copy_from_slice(&self.rest[..len]);
            self.rest = &self.rest[len..];
            Ok(len)
        }
    }

    /// What `convert_lossy` writes and counts for `input`.
    fn convert(input: &[u8], buffer_len: usize, step: usize) -> (Vec<u8>, u64) {
        let mut written = Vec::new();
        let lossy = convert_lossy(Trickle { rest: input, step }, buffer_len, |bytes| {
            written.extend_from_slice(bytes);
            Ok(())
        })
        .expect("convert");
        (written, lossy)
    }

    /// The same result in one piece: `String::from_utf8_lossy` and the count
    /// of the invalid sequences.
    fn whole(input: &[u8]) -> (Vec<u8>, u64) {
        let lossy = input
            .utf8_chunks()
            .filter(|chunk| !chunk.invalid().is_empty())
            .count() as u64;
        (
            String::from_utf8_lossy(input).into_owned().into_bytes(),
            lossy,
        )
    }

    #[test]
    fn valid_utf8_and_cr_pass_unchanged() {
        let input = "a\r\nb\r\u{20ac}\n".as_bytes();
        assert_eq!(convert(input, 8, 3), (input.to_vec(), 0));
    }

    #[test]
    fn an_empty_input_gives_nothing() {
        assert_eq!(convert(b"", 8, 3), (Vec::new(), 0));
    }

    #[test]
    fn an_invalid_byte_is_one_replacement() {
        assert_eq!(
            convert(b"a\xffb", 8, 8),
            ("a\u{fffd}b".as_bytes().to_vec(), 1)
        );
    }

    #[test]
    fn a_sequence_cut_at_the_end_of_the_input_is_one_replacement() {
        assert_eq!(
            convert(b"ab\xe2\x82", 8, 8),
            ("ab\u{fffd}".as_bytes().to_vec(), 1)
        );
    }

    #[test]
    fn a_sequence_cut_by_a_read_stays_whole() {
        let input = "x\u{20ac}y\u{20ac}".as_bytes();
        for step in 1..=input.len() {
            assert_eq!(convert(input, 4, step), (input.to_vec(), 0), "step {step}");
        }
    }

    #[test]
    fn any_buffer_and_any_read_size_give_the_whole_input_result() {
        let alphabet: [u8; 13] = [
            0x41, 0xc3, 0xa9, 0xe2, 0x82, 0xac, 0xf0, 0x9f, 0x98, 0x80, 0xff, 0x0d, 0x0a,
        ];
        let mut state: u32 = 0x2545_f491;
        for _ in 0..300 {
            let mut input = Vec::new();
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            for _ in 0..(state % 48) {
                state ^= state << 13;
                state ^= state >> 17;
                state ^= state << 5;
                input.push(alphabet[state as usize % alphabet.len()]);
            }
            for buffer_len in [4, 5, 7, 16] {
                for step in [1, 2, 3, 100] {
                    assert_eq!(
                        convert(&input, buffer_len, step),
                        whole(&input),
                        "input {input:?}, buffer {buffer_len}, step {step}"
                    );
                }
            }
        }
    }

    #[test]
    fn the_written_schema_value_is_the_published_id_of_the_header() {
        assert_eq!(SCHEMA, crate::published::id::<Header>());
    }

    /// A valid header.
    fn header() -> Header {
        Header {
            schema: SCHEMA.to_string(),
            argv: vec!["echo".to_string(), "a b".to_string()],
            cwd: "/work".to_string(),
            start: "2026-10-23T00:00:00.000000000Z".to_string(),
            end: "2026-10-23T00:00:01.000000000Z".to_string(),
            exit: Some(0),
            signal: None,
            head: None,
            dirty: None,
            lossy: 0,
        }
    }

    /// Writes `bytes` to a new file in `dir` and reads it as a capture.
    fn read_bytes(dir: &tempfile::TempDir, bytes: &[u8]) -> Result<Capture, ReadError> {
        let path = dir.path().join("capture.txt");
        fs::write(&path, bytes).expect("write the file");
        read(&path)
    }

    fn file_of(header: &str, output: &str) -> Vec<u8> {
        format!("{header}\n{output}").into_bytes()
    }

    #[test]
    fn a_capture_reads_back_with_its_header_and_output() {
        let dir = tempfile::tempdir().expect("tempdir");
        let text = serde_json::to_string(&header()).expect("serialize");
        let bytes = file_of(&text, "line 1\nline 2\n");
        let capture = read_bytes(&dir, &bytes).expect("a capture");
        assert_eq!(capture.header, header());
        assert_eq!(capture.output(), b"line 1\nline 2\n");
        assert_eq!(capture.bytes, bytes);
    }

    #[test]
    fn a_header_alone_has_an_empty_output() {
        let dir = tempfile::tempdir().expect("tempdir");
        let text = serde_json::to_string(&header()).expect("serialize");
        let capture = read_bytes(&dir, text.as_bytes()).expect("a capture");
        assert_eq!(capture.output(), b"");
    }

    #[test]
    fn the_output_may_hold_further_json_lines() {
        let dir = tempfile::tempdir().expect("tempdir");
        let text = serde_json::to_string(&header()).expect("serialize");
        let capture = read_bytes(&dir, &file_of(&text, "{\"a\":1}\n")).expect("a capture");
        assert_eq!(capture.output(), b"{\"a\":1}\n");
    }

    #[test]
    fn the_sha256_is_the_hash_of_the_whole_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let text = serde_json::to_string(&header()).expect("serialize");
        let bytes = file_of(&text, "x");
        let capture = read_bytes(&dir, &bytes).expect("a capture");
        assert_eq!(capture.sha256(), crate::baseline::hash(&bytes));
    }

    #[test]
    fn an_empty_file_is_no_capture() {
        let dir = tempfile::tempdir().expect("tempdir");
        assert_eq!(
            read_bytes(&dir, b""),
            Err(ReadError::NoCapture("it is empty".to_string()))
        );
    }

    #[test]
    fn a_first_line_that_is_no_json_object_is_no_capture() {
        let dir = tempfile::tempdir().expect("tempdir");
        for bytes in [
            &b"plain text\nmore"[..],
            b"[1,2]\n",
            b"\"x\"\n",
            b"\n{}\n",
            b"\xff\xfe\n",
        ] {
            assert_eq!(
                read_bytes(&dir, bytes),
                Err(ReadError::NoCapture(
                    "its first line is no JSON object".to_string()
                )),
                "{bytes:?}"
            );
        }
    }

    #[test]
    fn a_header_without_a_key_is_named_by_that_key() {
        let dir = tempfile::tempdir().expect("tempdir");
        let full: serde_json::Value = serde_json::to_value(header()).expect("value");
        for key in full.as_object().expect("object").keys() {
            let mut cut = full.clone();
            cut.as_object_mut().expect("object").remove(key);
            let bytes = file_of(&cut.to_string(), "out");
            assert_eq!(
                read_bytes(&dir, &bytes),
                Err(ReadError::NoCapture(format!("its header lacks {key}"))),
                "without {key}"
            );
        }
    }

    #[test]
    fn a_key_with_the_wrong_type_counts_as_missing() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut value: serde_json::Value = serde_json::to_value(header()).expect("value");
        value["exit"] = serde_json::json!("0");
        assert_eq!(
            read_bytes(&dir, &file_of(&value.to_string(), "")),
            Err(ReadError::NoCapture("its header lacks exit".to_string()))
        );
    }

    #[test]
    fn an_argv_with_a_nul_byte_counts_as_missing() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut value: serde_json::Value = serde_json::to_value(header()).expect("value");
        value["argv"] = serde_json::json!(["a\u{0}b"]);
        assert_eq!(
            read_bytes(&dir, &file_of(&value.to_string(), "")),
            Err(ReadError::NoCapture("its header lacks argv".to_string()))
        );
    }

    #[test]
    fn a_schema_of_another_major_is_named() {
        let dir = tempfile::tempdir().expect("tempdir");
        for schema in [
            "houserules/capture@2.0.0",
            "houserules/capture@10.0.0",
            "houserules/capture@1.0",
            "houserules/capture@1.x.0",
            "other/capture@1.0.0",
            "",
        ] {
            let mut value: serde_json::Value = serde_json::to_value(header()).expect("value");
            value["schema"] = serde_json::json!(schema);
            assert_eq!(
                read_bytes(&dir, &file_of(&value.to_string(), "")),
                Err(ReadError::NoCapture(format!(
                    "its schema is {schema}, not houserules/capture@1"
                ))),
                "schema {schema:?}"
            );
        }
    }

    #[test]
    fn a_schema_of_another_major_is_named_before_any_other_key_is_judged() {
        let dir = tempfile::tempdir().expect("tempdir");
        let schema_reason = Err(ReadError::NoCapture(
            "its schema is houserules/capture@2.0.0, not houserules/capture@1".to_string(),
        ));
        for header in [
            r#"{"schema":"houserules/capture@2.0.0"}"#,
            r#"{"schema":"houserules/capture@2.0.0","argv":5}"#,
        ] {
            assert_eq!(
                read_bytes(&dir, &file_of(header, "out")),
                schema_reason,
                "{header}"
            );
        }
    }

    #[test]
    fn the_wait_failure_line_names_the_program_and_the_operating_systems_error() {
        let error = CaptureError::CannotWait {
            program: "sh".to_string(),
            error: io::Error::from_raw_os_error(10),
        };
        assert_eq!(
            error.to_string(),
            format!(
                "could not wait for sh: {}",
                io::Error::from_raw_os_error(10)
            )
        );
    }

    #[test]
    fn a_later_minor_or_patch_of_major_1_is_read() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut value: serde_json::Value = serde_json::to_value(header()).expect("value");
        value["schema"] = serde_json::json!("houserules/capture@1.12.3");
        value["extra"] = serde_json::json!(true);
        let capture = read_bytes(&dir, &file_of(&value.to_string(), "")).expect("a capture");
        assert_eq!(capture.header.schema, "houserules/capture@1.12.3");
    }

    #[test]
    fn the_error_of_a_temporary_file_loses_its_path_suffix() {
        let dir = tempfile::tempdir().expect("tempdir");
        let missing = dir.path().join("missing");
        let wrapped = NamedTempFile::new_in(&missing).expect_err("the directory is missing");
        let plain = fs::File::create(missing.join("probe")).expect_err("the directory is missing");
        assert!(
            wrapped.to_string().contains(" at path "),
            "tempfile no longer adds the path: {wrapped}"
        );
        assert_eq!(os_error_text(&wrapped), plain.to_string());
        assert_eq!(os_error_text(&plain), plain.to_string());
    }

    #[test]
    fn a_missing_file_is_missing() {
        let dir = tempfile::tempdir().expect("tempdir");
        assert_eq!(read(&dir.path().join("none.txt")), Err(ReadError::Missing));
    }

    #[test]
    fn a_directory_is_unreadable_with_the_operating_systems_error() {
        let dir = tempfile::tempdir().expect("tempdir");
        let expected = fs::read(dir.path())
            .expect_err("a directory cannot be read as a file")
            .to_string();
        assert_eq!(read(dir.path()), Err(ReadError::Unreadable(expected)));
    }
}
