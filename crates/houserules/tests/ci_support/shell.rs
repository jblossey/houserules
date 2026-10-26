//! A small lexer for shell text: the unquoted words of a script, and the two questions
//! the trust rules ask of it (does it run a command, does it turn on a trace).

/// The unquoted words of the shell text `script`: a word with any quoted part,
/// a word in a comment, and the text of a quote are left out. A separator ends a
/// word: white space, `;`, `|`, `&`, parentheses, braces, and a backtick.
pub fn bare_words(script: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut quoted = false;
    let mut quote: Option<char> = None;
    let mut chars = script.chars().peekable();
    let flush = |word: &mut String, quoted: &mut bool, words: &mut Vec<String>| {
        if !word.is_empty() && !*quoted {
            words.push(std::mem::take(word));
        }
        word.clear();
        *quoted = false;
    };
    while let Some(character) = chars.next() {
        match quote {
            Some('\'') => {
                if character == '\'' {
                    quote = None;
                }
            }
            Some(_) => match character {
                '\\' => {
                    chars.next();
                }
                '"' => quote = None,
                _ => {}
            },
            None => match character {
                '\'' | '"' => {
                    quote = Some(character);
                    quoted = true;
                }
                '\\' => {
                    chars.next();
                    quoted = true;
                }
                '#' if word.is_empty() => {
                    for skipped in chars.by_ref() {
                        if skipped == '\n' {
                            break;
                        }
                    }
                }
                character
                    if character.is_whitespace()
                        || matches!(character, ';' | '|' | '&' | '(' | ')' | '{' | '}' | '`') =>
                {
                    flush(&mut word, &mut quoted, &mut words);
                }
                character => word.push(character),
            },
        }
    }
    flush(&mut word, &mut quoted, &mut words);
    words
}

/// `true` when the shell text `script` holds `command` as an unquoted word,
/// bare or as the last segment of a path. A message inside quotes that names
/// the command is no invocation.
pub fn invokes(script: &str, command: &str) -> bool {
    let suffix = format!("/{command}");
    bare_words(script)
        .iter()
        .any(|word| word == command || word.ends_with(&suffix))
}

/// `true` when the shell text `script` turns on a shell trace: a `set`, `bash`,
/// or `sh` with a flag group that holds `x`, or with `-o xtrace`.
pub fn traces_the_shell(script: &str) -> bool {
    let words = bare_words(script);
    words.iter().enumerate().any(|(index, word)| {
        if !matches!(word.as_str(), "set" | "bash" | "sh") {
            return false;
        }
        let mut rest = words[index + 1..].iter();
        while let Some(option) = rest.next() {
            if option == "xtrace" || option == "-x" {
                return true;
            }
            if option == "-o" {
                if rest.next().is_some_and(|name| name == "xtrace") {
                    return true;
                }
                continue;
            }
            let is_flag_group = option.starts_with('-') && !option.starts_with("--");
            if !is_flag_group {
                return false;
            }
            if option.contains('x') {
                return true;
            }
        }
        false
    })
}
