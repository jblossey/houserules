//! Quotes values for a POSIX shell: [`quote`] gives one word and [`join`] gives
//! a command line. A value that needs no quoting stays as it is.

use shlex::Quoter;

/// The quoter of both functions. A NUL byte passes through: no operating
/// system takes one in an argument or a path, but a value that a project wrote
/// into its store can hold one, and a printed line about that value reports it
/// instead of crashing. With `allow_nul(true)`, `shlex` returns no error at all
/// (docs.rs/shlex, `QuoteError`).
fn quoter() -> Quoter {
    Quoter::new().allow_nul(true)
}

/// Quotes `value` as one word of a POSIX shell command line. A value that
/// needs no quoting is returned as it is. A NUL byte in `value` stays in the
/// word, inside quotes: a shell cannot pass it on, but the line that prints the
/// word still names the value.
///
/// Every printed command that carries a project-controlled value uses this
/// function: a path, or a list that the store of a project built. A pack name,
/// a workflow name, a kit label, a repository slug, and a version match
/// patterns with no shell character, so a command that carries one needs no
/// call.
pub(crate) fn quote(value: &str) -> String {
    quoter()
        .quote(value)
        .expect("shlex returns no error when it allows a NUL byte")
        .into_owned()
}

/// Joins `argv` into one POSIX shell command line: each element is quoted
/// where it needs quoting (see [`quote`]), and one space separates the
/// elements.
pub(crate) fn join(argv: &[String]) -> String {
    quoter()
        .join(argv.iter().map(String::as_str))
        .expect("shlex returns no error when it allows a NUL byte")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_word_that_needs_no_quoting_stays_as_it_is() {
        assert_eq!(quote("plain"), "plain");
        assert_eq!(quote("review:wip"), "review:wip");
        assert_eq!(quote("/a/b-c_d.txt"), "/a/b-c_d.txt");
    }

    #[test]
    fn a_space_gets_single_quotes() {
        assert_eq!(quote("/a b/c"), "'/a b/c'");
    }

    #[test]
    fn a_comma_gets_single_quotes() {
        assert_eq!(
            quote("review:wip,partial:active"),
            "'review:wip,partial:active'"
        );
    }

    #[test]
    fn the_empty_word_is_a_pair_of_quotes() {
        assert_eq!(quote(""), "''");
    }

    #[test]
    fn join_quotes_each_element_where_it_needs_it() {
        let argv = ["sh", "-c", "a b"].map(String::from);
        assert_eq!(join(&argv), "sh -c 'a b'");
    }

    #[test]
    fn join_of_no_elements_is_empty() {
        assert_eq!(join(&[]), "");
    }

    #[test]
    fn a_nul_byte_stays_in_a_quoted_word() {
        assert_eq!(quote("a\0b"), "'a\0b'");
    }

    #[test]
    fn join_keeps_a_nul_byte_in_its_word() {
        let argv = ["sh", "a\0b"].map(String::from);
        assert_eq!(join(&argv), "sh 'a\0b'");
    }
}
