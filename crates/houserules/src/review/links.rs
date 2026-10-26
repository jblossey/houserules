//! The links that the commits of a range carry in their trailers (batch 38, spec
//! 5.1): an `Intent: <source>:<ref>` trailer names an intent, and a trailer whose
//! key `review.sessions.trailers` lists names a session of an agent tool.
//!
//! [`read`] lists the commits of the range, oldest first, and reads the trailers
//! of each message with `rules::trailers::parse`, which reads them as git does
//! (`git interpret-trailers --parse`). A key matches without regard to its case,
//! as git matches trailer keys. The trailers of one commit come in the order of
//! its message. A trailer never fails the command: an `Intent:` value that is no
//! `<source>:<ref>` is kept with the line that says so (spec 5.2), and the
//! callers print it as an unresolved intent.
//!
//! The listing reads the commits through `review::git`, so it holds only the
//! text of the commits that git has. It reads them as NUL-separated records
//! (`git log -z`), because a message may hold any other byte. The settings that
//! change the text of `git log` and that the call pins are
//! `i18n.logOutputEncoding` (the call asks for `--encoding=UTF-8`),
//! `log.showSignature` (the call asks for `--no-show-signature`), and the
//! replace refs (the runner reads no replaced object). No option turns off the
//! grafts file of a shallow clone: a range in a shallow clone lists the commits
//! that the clone holds.

use std::path::Path;

use super::git;
use super::intent::IntentRef;
use super::text::code_span;
pub(crate) use crate::published::evidence::Session;
use crate::rules::trailers;

/// The trailer key that names an intent.
const INTENT_KEY: &str = "Intent";

/// What the trailers of a range hold: the intents that its commits name and the
/// sessions that they cite.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Links {
    /// The `Intent:` trailers, oldest commit first.
    pub(crate) intents: Vec<TrailerIntent>,
    /// The session trailers, oldest commit first.
    pub(crate) sessions: Vec<Session>,
}

/// One `Intent:` trailer of a commit.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct TrailerIntent {
    /// The full sha of the commit that holds the trailer.
    pub(crate) commit: String,
    /// The value of the trailer, as `rules::trailers::parse` gives it.
    pub(crate) value: String,
    /// The intent that the value names, or the line of an intent that is no
    /// `<source>:<ref>`. Whether the source is an adapter is the resolver's
    /// question.
    pub(crate) named: Result<IntentRef, String>,
}

impl Links {
    /// The intents for the resolver: `named` (the flag or the bundle line) when
    /// there is one, then the intent of each `Intent:` trailer, oldest commit
    /// first. The resolver counts each `<source>:<ref>` once.
    pub(crate) fn names(&self, named: Option<IntentRef>) -> Vec<IntentRef> {
        let mut names: Vec<IntentRef> = named.into_iter().collect();
        names.extend(self.intents.iter().map(TrailerIntent::intent_ref));
        names
    }
}

impl TrailerIntent {
    /// The intent that the trailer names for the resolver: the value as it is
    /// written, or for a value that is no `<source>:<ref>` its split at the first
    /// colon, which the resolver turns into an unresolved intent.
    pub(crate) fn intent_ref(&self) -> IntentRef {
        match &self.named {
            Ok(named) => named.clone(),
            Err(_) => IntentRef::split(&self.value),
        }
    }
}

/// The links of the commits of the range `base` to `head`, both full commit ids,
/// `base` being git's empty tree for a range from the start of the history, whose
/// range holds every commit of `head`. `keys` are the trailer keys of the
/// sessions (`review.sessions.trailers`). The commits come oldest first, in
/// topological order. A failure of git, or a message whose trailers git cannot
/// read, is the `Err` with its line.
pub(crate) fn read(root: &Path, base: &str, head: &str, keys: &[String]) -> Result<Links, String> {
    let range = if base == git::empty_tree(root)? {
        head.to_string()
    } else {
        format!("{base}..{head}")
    };
    let log = git::run(
        root,
        &[
            "log",
            "-z",
            "--no-show-signature",
            "--encoding=UTF-8",
            "--topo-order",
            "--reverse",
            "--format=%H%n%B",
            &range,
            "--",
        ],
    )?;
    let mut links = Links {
        intents: Vec::new(),
        sessions: Vec::new(),
    };
    for (commit, message) in records(&log)? {
        for (key, value) in trailers::parse(root, message)? {
            if trailers::is_key(&key, INTENT_KEY) {
                links.intents.push(TrailerIntent {
                    commit: commit.to_string(),
                    named: IntentRef::parse(&value),
                    value,
                });
            } else if keys.iter().any(|wanted| trailers::is_key(&key, wanted)) {
                links.sessions.push(Session {
                    commit: commit.to_string(),
                    trailer: key,
                    value,
                });
            }
        }
    }
    Ok(links)
}

/// The start of the line for a record of the listing that holds no line end
/// after its sha: the line goes on with `: ` and the record.
const NO_MESSAGE_LINE: &str = "git log printed a commit with no line end after its sha";

/// The `(sha, message)` of each commit in `log`, the text of `git log -z
/// --format=%H%n%B`: the sha is the first line of a record, and the message
/// is the rest of it. git ends each record with NUL, which a message never
/// holds (git refuses to commit one), so no byte of a message ends a record.
/// A record with no line end after its sha is the `Err` of [`NO_MESSAGE_LINE`]
/// and the record, printed as a code span.
fn records(log: &str) -> Result<Vec<(&str, &str)>, String> {
    log.split('\0')
        .filter(|record| !record.is_empty())
        .map(|record| {
            record
                .split_once('\n')
                .ok_or_else(|| format!("{NO_MESSAGE_LINE}: {}", code_span(record)))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::review::git::scratch::Repo;

    /// The keys of the session trailers `names`.
    fn keys(names: &[&str]) -> Vec<String> {
        names.iter().map(|name| (*name).to_string()).collect()
    }

    /// The line of an `Intent:` value that is no `<source>:<ref>`.
    const MALFORMED: &str = "an intent is written <source>:<ref>";

    #[test]
    fn the_trailers_of_a_range_come_oldest_commit_first_and_in_message_order() {
        let repo = Repo::new();
        let base = repo.commit("chore: base\n\nEntire-Checkpoint: cp-base");
        let first = repo.commit("docs: one\n\nIntent: file:a.md\nentire-checkpoint: cp-1");
        let second = repo.commit(
            "docs: two\n\nINTENT: oops\nSession-Id: s-2\nIntent: openspec:x\nReviewed-by: me",
        );

        let links = read(
            repo.root(),
            &base,
            &second,
            &keys(&["Entire-Checkpoint", "Session-Id"]),
        )
        .expect("read");

        assert_eq!(
            links.intents,
            [
                TrailerIntent {
                    commit: first.clone(),
                    value: "file:a.md".to_string(),
                    named: IntentRef::parse("file:a.md"),
                },
                TrailerIntent {
                    commit: second.clone(),
                    value: "oops".to_string(),
                    named: Err(MALFORMED.to_string()),
                },
                TrailerIntent {
                    commit: second.clone(),
                    value: "openspec:x".to_string(),
                    named: IntentRef::parse("openspec:x"),
                },
            ]
        );
        let session = |commit: &str, trailer: &str, value: &str| Session {
            commit: commit.to_string(),
            trailer: trailer.to_string(),
            value: value.to_string(),
        };
        assert_eq!(
            links.sessions,
            [
                session(&first, "entire-checkpoint", "cp-1"),
                session(&second, "Session-Id", "s-2"),
            ]
        );
    }

    #[test]
    fn the_names_for_the_resolver_start_with_the_flag_and_follow_the_trailers() {
        let trailer = |value: &str| TrailerIntent {
            commit: String::new(),
            value: value.to_string(),
            named: IntentRef::parse(value),
        };
        let links = Links {
            intents: vec![trailer("file:a.md"), trailer("oops")],
            sessions: Vec::new(),
        };
        let flag = IntentRef::parse("file:flag.md").expect("an intent");

        assert_eq!(
            links.names(Some(flag.clone())),
            [
                flag,
                IntentRef::parse("file:a.md").expect("an intent"),
                IntentRef::split("oops")
            ]
        );
        assert_eq!(links.names(None).len(), 2);
    }

    #[test]
    fn a_malformed_value_is_named_by_its_split() {
        let trailer = |value: &str| TrailerIntent {
            commit: String::new(),
            value: value.to_string(),
            named: IntentRef::parse(value),
        };

        assert_eq!(
            trailer("file:a.md").intent_ref(),
            IntentRef::parse("file:a.md").expect("an intent")
        );
        assert_eq!(trailer("oops").intent_ref(), IntentRef::split("oops"));
        assert_eq!(trailer("file:").intent_ref(), IntentRef::split("file:"));
    }

    #[test]
    fn the_range_leaves_out_the_commits_of_the_base_and_an_empty_tree_reads_them_all() {
        let repo = Repo::new();
        let root = repo.commit("chore: base\n\nEntire-Checkpoint: cp-root");
        let head = repo.commit("docs: edit\n\nEntire-Checkpoint: cp-head");
        let keys = keys(&["Entire-Checkpoint"]);
        let values = |links: Links| -> Vec<String> {
            links
                .sessions
                .into_iter()
                .map(|session| session.value)
                .collect()
        };

        let after_root = read(repo.root(), &root, &head, &keys).expect("read");
        let empty = git::empty_tree(repo.root()).expect("the empty tree");
        let from_the_start = read(repo.root(), &empty, &head, &keys).expect("read");

        assert_eq!(values(after_root), ["cp-head"]);
        assert_eq!(values(from_the_start), ["cp-root", "cp-head"]);
    }

    #[test]
    fn a_message_is_read_as_utf_8_whatever_the_log_encoding_of_the_repository() {
        let repo = Repo::new();
        let base = repo.commit("chore: base");
        repo.git(&["config", "i18n.logOutputEncoding", "ISO-8859-1"]);
        let head = repo.commit("docs: edit\n\nEntire-Checkpoint: caf\u{e9}");

        let links = read(repo.root(), &base, &head, &keys(&["Entire-Checkpoint"])).expect("read");

        assert_eq!(links.sessions[0].value, "caf\u{e9}");
    }

    /// A message may hold any byte but NUL, the separator of the records of the
    /// listing, so the record separator `0x1e` cuts no record, wherever a message
    /// holds it.
    #[test]
    fn a_message_that_holds_a_separator_byte_keeps_its_trailers() {
        let repo = Repo::new();
        let base = repo.commit("chore: base");
        let middle = repo.commit(
            "docs: middle\n\nbody \u{1e} here\n\nIntent: file:middle.md\nEntire-Checkpoint: cp-middle",
        );
        let line_start = repo.commit(
            "docs: line start\n\n\u{1e}at the start of a line\n\nIntent: file:start.md\nEntire-Checkpoint: cp-start",
        );
        let subject = repo.commit("\u{1e}docs: subject\n\nIntent: file:subject.md");
        let in_value = repo.commit("docs: value\n\nEntire-Checkpoint: a\u{1e}b");

        let links =
            read(repo.root(), &base, &in_value, &keys(&["Entire-Checkpoint"])).expect("read");

        let intent = |commit: &String, value: &str| TrailerIntent {
            commit: commit.clone(),
            value: value.to_string(),
            named: IntentRef::parse(value),
        };
        assert_eq!(
            links.intents,
            [
                intent(&middle, "file:middle.md"),
                intent(&line_start, "file:start.md"),
                intent(&subject, "file:subject.md"),
            ]
        );
        let session = |commit: &String, value: &str| Session {
            commit: commit.clone(),
            trailer: "Entire-Checkpoint".to_string(),
            value: value.to_string(),
        };
        assert_eq!(
            links.sessions,
            [
                session(&middle, "cp-middle"),
                session(&line_start, "cp-start"),
                session(&in_value, "a\u{1e}b"),
            ]
        );
    }

    #[test]
    fn the_records_of_a_log_end_at_nul_and_nowhere_else() {
        let log = "aaa\nsubject\n\nbody \u{1e}\n\0bbb\n\0ccc\n\0";

        assert_eq!(
            records(log),
            Ok(vec![
                ("aaa", "subject\n\nbody \u{1e}\n"),
                ("bbb", ""),
                ("ccc", "")
            ])
        );
        assert_eq!(records(""), Ok(Vec::new()));
    }

    #[test]
    fn a_record_with_no_line_end_after_its_sha_is_one_line_that_names_the_record() {
        let error = records("aaa\nsubject\n\0bbb").expect_err("no line end after bbb");
        let escaped = records("aaa\nsubject\n\0b\u{1b}b").expect_err("no line end after b<ESC>b");

        assert_eq!(
            error,
            "git log printed a commit with no line end after its sha: `bbb`"
        );
        assert_eq!(
            escaped,
            "git log printed a commit with no line end after its sha: `\"b\\u001bb\"`"
        );
        assert!(!error.contains('\n'), "{error}");
        assert!(!escaped.chars().any(char::is_control), "{escaped:?}");
    }

    #[test]
    fn a_range_that_cannot_be_read_is_one_line() {
        let repo = Repo::new();
        let head = repo.commit("chore: base");

        let error = read(repo.root(), &"0".repeat(40), &head, &keys(&[])).expect_err("a bad base");

        assert!(!error.contains('\n'), "{error}");
        assert!(!error.is_empty());
    }
}
