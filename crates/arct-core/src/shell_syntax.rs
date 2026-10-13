//! Shell syntax for the simulated sandbox: quoting, variables, globs,
//! redirection, pipes and command lists.
//!
//! A small, honest subset of POSIX sh, enough for lessons to teach what a
//! real shell does with a line before any program runs. Parsing is pure:
//! variable values come from a lookup closure, and globs are only marked
//! here (the sandbox expands them against its own files).
//!
//! Not supported, and reported as such: command substitution (`$(...)`,
//! backticks), background jobs (`&`), here-documents, subshells.

/// One word after quoting and variable expansion
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Word {
    pub text: String,
    /// Contains an unquoted `*`, `?` or `[`: the sandbox may expand it
    pub glob: bool,
}

/// Where a stream goes, or comes from
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Redirect {
    /// `> file` / `>> file`
    Out { target: String, append: bool },
    /// `2> file` / `2>> file`
    Err { target: String, append: bool },
    /// `2>&1`: errors go wherever output goes
    ErrToOut,
    /// `< file`
    In(String),
}

/// One command in a pipeline
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Stage {
    /// `NAME=value` words before the command
    pub assignments: Vec<(String, String)>,
    pub words: Vec<Word>,
    pub redirects: Vec<Redirect>,
}

/// Commands joined by `|`
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Pipeline {
    pub stages: Vec<Stage>,
}

/// How a pipeline follows the one before it
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Joiner {
    /// First in the line, or after `;`
    Always,
    /// After `&&`: only if the previous one worked
    IfOk,
    /// After `||`: only if the previous one failed
    IfFailed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Token {
    Word(Word),
    Pipe,
    And,
    Or,
    Semi,
    Out(bool),
    Err(bool),
    ErrToOut,
    In,
}

/// Is this a valid shell variable name?
pub fn is_name(s: &str) -> bool {
    let mut chars = s.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Parse one line into pipelines joined by `;`, `&&` and `||`.
///
/// `lookup` returns a variable's value (`HOME`, `1`, `#`, `@`, `?` ...);
/// unknown variables expand to nothing, as in sh.
pub fn parse(
    line: &str,
    lookup: &dyn Fn(&str) -> Option<String>,
) -> Result<Vec<(Joiner, Pipeline)>, String> {
    let tokens = tokenize(line, lookup)?;
    let mut list: Vec<(Joiner, Pipeline)> = Vec::new();
    let mut joiner = Joiner::Always;
    let mut pipeline = Pipeline::default();
    let mut stage = Stage::default();
    let mut iter = tokens.into_iter().peekable();

    fn finish_stage(stage: &mut Stage, pipeline: &mut Pipeline, op: &str) -> Result<(), String> {
        if stage.words.is_empty() && stage.assignments.is_empty() {
            return Err(format!("syntax error near '{op}': a command is missing"));
        }
        pipeline.stages.push(std::mem::take(stage));
        Ok(())
    }

    while let Some(token) = iter.next() {
        match token {
            Token::Word(word) => {
                // NAME=value before the command name is an assignment
                if stage.words.is_empty() && !word.glob {
                    if let Some((name, value)) = word.text.split_once('=') {
                        if is_name(name) {
                            stage.assignments.push((name.to_string(), value.to_string()));
                            continue;
                        }
                    }
                }
                stage.words.push(word);
            }
            Token::Out(append) | Token::Err(append) => {
                let is_err = matches!(token, Token::Err(_));
                let Some(Token::Word(target)) = iter.next() else {
                    return Err("syntax error: a file name is missing after '>'".to_string());
                };
                stage.redirects.push(if is_err {
                    Redirect::Err { target: target.text, append }
                } else {
                    Redirect::Out { target: target.text, append }
                });
            }
            Token::ErrToOut => stage.redirects.push(Redirect::ErrToOut),
            Token::In => {
                let Some(Token::Word(target)) = iter.next() else {
                    return Err("syntax error: a file name is missing after '<'".to_string());
                };
                stage.redirects.push(Redirect::In(target.text));
            }
            Token::Pipe => finish_stage(&mut stage, &mut pipeline, "|")?,
            Token::And | Token::Or | Token::Semi => {
                let op = match token {
                    Token::And => "&&",
                    Token::Or => "||",
                    _ => ";",
                };
                if stage.words.is_empty() && stage.assignments.is_empty() {
                    // A trailing or doubled `;` is harmless; `&&`/`||` need both sides
                    if token != Token::Semi || !pipeline.stages.is_empty() {
                        return Err(format!("syntax error near '{op}': a command is missing"));
                    }
                } else {
                    finish_stage(&mut stage, &mut pipeline, op)?;
                }
                if !pipeline.stages.is_empty() {
                    list.push((joiner, std::mem::take(&mut pipeline)));
                } else if token != Token::Semi {
                    return Err(format!("syntax error near '{op}': a command is missing"));
                }
                joiner = match token {
                    Token::And => Joiner::IfOk,
                    Token::Or => Joiner::IfFailed,
                    _ => Joiner::Always,
                };
            }
        }
    }
    if !stage.words.is_empty() || !stage.assignments.is_empty() || !stage.redirects.is_empty() {
        finish_stage(&mut stage, &mut pipeline, "end of line")?;
    } else if !pipeline.stages.is_empty() {
        return Err("syntax error: a command is missing after '|'".to_string());
    }
    if !pipeline.stages.is_empty() {
        list.push((joiner, pipeline));
    } else if joiner != Joiner::Always {
        return Err("syntax error: a command is missing at the end of the line".to_string());
    }
    Ok(list)
}

// end_word! resets its state even after the last word
#[allow(unused_assignments)]
fn tokenize(line: &str, lookup: &dyn Fn(&str) -> Option<String>) -> Result<Vec<Token>, String> {
    let chars: Vec<char> = line.chars().collect();
    let mut tokens = Vec::new();
    let mut i = 0;
    // The word being built: text, has-unquoted-glob, and whether it has
    // started (so "" makes an empty word)
    let mut text = String::new();
    let mut glob = false;
    let mut started = false;

    macro_rules! end_word {
        () => {
            if started {
                tokens.push(Token::Word(Word { text: std::mem::take(&mut text), glob }));
                glob = false;
                started = false;
            }
        };
    }

    while i < chars.len() {
        let c = chars[i];
        match c {
            ' ' | '\t' => {
                end_word!();
                i += 1;
            }
            '#' if !started => break, // comment
            '\'' => {
                started = true;
                i += 1;
                let close = chars[i..].iter().position(|&ch| ch == '\'').ok_or("a ' quote is never closed")?;
                text.extend(&chars[i..i + close]);
                i += close + 1;
            }
            '"' => {
                started = true;
                i += 1;
                loop {
                    let Some(&ch) = chars.get(i) else {
                        return Err("a \" quote is never closed".to_string());
                    };
                    match ch {
                        '"' => {
                            i += 1;
                            break;
                        }
                        '\\' if matches!(chars.get(i + 1), Some('"' | '\\' | '$' | '`')) => {
                            text.push(chars[i + 1]);
                            i += 2;
                        }
                        '$' => {
                            let (value, used) = expand_dollar(&chars[i..], lookup)?;
                            text.push_str(&value);
                            i += used;
                        }
                        '`' => return Err(unsupported_substitution()),
                        _ => {
                            text.push(ch);
                            i += 1;
                        }
                    }
                }
            }
            '\\' => {
                started = true;
                if let Some(&next) = chars.get(i + 1) {
                    text.push(next);
                }
                i += 2;
            }
            '$' => {
                started = true;
                let (value, used) = expand_dollar(&chars[i..], lookup)?;
                text.push_str(&value);
                i += used;
            }
            '`' => return Err(unsupported_substitution()),
            '~' if !started => {
                // ~ and ~/path at the start of a word mean the home folder
                let next = chars.get(i + 1);
                if next.is_none() || matches!(next, Some('/' | ' ' | '\t' | ';' | '|' | '&')) {
                    text.push_str(&lookup("HOME").unwrap_or_else(|| "~".to_string()));
                } else {
                    text.push('~');
                }
                started = true;
                i += 1;
            }
            '|' => {
                end_word!();
                if chars.get(i + 1) == Some(&'|') {
                    tokens.push(Token::Or);
                    i += 2;
                } else {
                    tokens.push(Token::Pipe);
                    i += 1;
                }
            }
            '&' => {
                end_word!();
                if chars.get(i + 1) == Some(&'&') {
                    tokens.push(Token::And);
                    i += 2;
                } else {
                    return Err("running in the background with & isn't available in the sandbox".to_string());
                }
            }
            ';' => {
                end_word!();
                tokens.push(Token::Semi);
                i += 1;
            }
            '>' => {
                // A bare "2" right before > means the error stream
                let is_err = started && text == "2" && !glob && i > 0 && chars[i - 1] == '2';
                if is_err {
                    text.clear();
                    started = false;
                } else {
                    end_word!();
                }
                let append = chars.get(i + 1) == Some(&'>');
                i += if append { 2 } else { 1 };
                if is_err {
                    if !append && chars.get(i) == Some(&'&') && chars.get(i + 1) == Some(&'1') {
                        tokens.push(Token::ErrToOut);
                        i += 2;
                    } else {
                        tokens.push(Token::Err(append));
                    }
                } else {
                    tokens.push(Token::Out(append));
                }
            }
            '<' => {
                end_word!();
                tokens.push(Token::In);
                i += 1;
            }
            '*' | '?' | '[' => {
                started = true;
                glob = true;
                text.push(c);
                i += 1;
            }
            _ => {
                started = true;
                text.push(c);
                i += 1;
            }
        }
    }
    end_word!();
    Ok(tokens)
}

fn unsupported_substitution() -> String {
    "running a command inside another with $(...) or backticks isn't available in the sandbox".to_string()
}

/// Expand `$NAME`, `${NAME}`, `$1`, `$#`, `$@`, `$?` at the start of `chars`.
/// Returns the value and how many characters were used.
fn expand_dollar(
    chars: &[char],
    lookup: &dyn Fn(&str) -> Option<String>,
) -> Result<(String, usize), String> {
    match chars.get(1) {
        Some('(') => Err(unsupported_substitution()),
        Some('{') => {
            let close = chars
                .iter()
                .position(|&c| c == '}')
                .ok_or("a ${ is never closed with }")?;
            let name: String = chars[2..close].iter().collect();
            if !(is_name(&name) || name.chars().all(|c| c.is_ascii_digit()) && !name.is_empty()) {
                return Err(format!("${{{name}}}: bad variable name"));
            }
            Ok((lookup(&name).unwrap_or_default(), close + 1))
        }
        Some(&c) if c.is_ascii_digit() || matches!(c, '#' | '@' | '?' | '*') => {
            Ok((lookup(&c.to_string()).unwrap_or_default(), 2))
        }
        Some(&c) if c.is_ascii_alphabetic() || c == '_' => {
            let len = chars[1..]
                .iter()
                .take_while(|c| c.is_ascii_alphanumeric() || **c == '_')
                .count();
            let name: String = chars[1..1 + len].iter().collect();
            Ok((lookup(&name).unwrap_or_default(), 1 + len))
        }
        // A lone $ is just a dollar sign
        _ => Ok(("$".to_string(), 1)),
    }
}

/// Does `name` match the glob `pattern` the way the shell expands it?
/// Supports `*`, `?`, `[abc]`, `[a-z]` and `[!abc]`; a leading `.` must
/// be matched explicitly, so `*` skips hidden files.
pub fn glob_match(pattern: &str, name: &str) -> bool {
    if name.starts_with('.') && !pattern.starts_with('.') {
        return false;
    }
    fnmatch(pattern, name)
}

/// Pattern match without the shell's hidden-file rule (as `find -name` does)
pub fn fnmatch(pattern: &str, name: &str) -> bool {
    let p: Vec<char> = pattern.chars().collect();
    let n: Vec<char> = name.chars().collect();
    match_from(&p, &n)
}

fn match_from(p: &[char], n: &[char]) -> bool {
    match p.first() {
        None => n.is_empty(),
        Some('*') => (0..=n.len()).any(|skip| match_from(&p[1..], &n[skip..])),
        Some('?') => !n.is_empty() && match_from(&p[1..], &n[1..]),
        Some('[') => {
            let Some(close) = p.iter().skip(1).position(|&c| c == ']').map(|x| x + 1) else {
                return n.first() == Some(&'[') && match_from(&p[1..], &n[1..]);
            };
            let Some(&ch) = n.first() else { return false };
            let mut set = &p[1..close];
            let negate = matches!(set.first(), Some('!' | '^'));
            if negate {
                set = &set[1..];
            }
            let mut hit = false;
            let mut k = 0;
            while k < set.len() {
                if k + 2 < set.len() && set[k + 1] == '-' {
                    hit |= set[k] <= ch && ch <= set[k + 2];
                    k += 3;
                } else {
                    hit |= set[k] == ch;
                    k += 1;
                }
            }
            hit != negate && match_from(&p[close + 1..], &n[1..])
        }
        Some(&c) => n.first() == Some(&c) && match_from(&p[1..], &n[1..]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vars(name: &str) -> Option<String> {
        match name {
            "HOME" => Some("/lesson-home".into()),
            "NAME" => Some("Ada".into()),
            "1" => Some("first".into()),
            "#" => Some("1".into()),
            _ => None,
        }
    }

    fn one(line: &str) -> Stage {
        let list = parse(line, &vars).unwrap();
        assert_eq!(list.len(), 1, "{line}");
        assert_eq!(list[0].1.stages.len(), 1, "{line}");
        list[0].1.stages[0].clone()
    }

    fn texts(stage: &Stage) -> Vec<&str> {
        stage.words.iter().map(|w| w.text.as_str()).collect()
    }

    #[test]
    fn quotes_and_variables() {
        assert_eq!(texts(&one("echo $NAME")), ["echo", "Ada"]);
        assert_eq!(texts(&one("echo \"hi $NAME\"")), ["echo", "hi Ada"]);
        assert_eq!(texts(&one("echo '$NAME'")), ["echo", "$NAME"]);
        assert_eq!(texts(&one("echo ${NAME}s")), ["echo", "Adas"]);
        assert_eq!(texts(&one("echo $1 $#")), ["echo", "first", "1"]);
        assert_eq!(texts(&one("echo $MISSING end")), ["echo", "", "end"]);
        assert_eq!(texts(&one("echo \"\"")), ["echo", ""]);
        assert_eq!(texts(&one("echo a\\ b")), ["echo", "a b"]);
        assert_eq!(texts(&one("echo costs $5")), ["echo", "costs", ""]);
        assert_eq!(texts(&one("echo 5$")), ["echo", "5$"]);
    }

    #[test]
    fn home_tilde() {
        assert_eq!(texts(&one("cd ~")), ["cd", "/lesson-home"]);
        assert_eq!(texts(&one("ls ~/notes")), ["ls", "/lesson-home/notes"]);
        assert_eq!(texts(&one("echo a~b '~'")), ["echo", "a~b", "~"]);
    }

    #[test]
    fn globs_are_marked_only_when_unquoted() {
        let s = one("find . -name '*.txt' *.log");
        assert!(!s.words[3].glob);
        assert!(s.words[4].glob);
    }

    #[test]
    fn assignments() {
        let s = one("GREETING=hello");
        assert_eq!(s.assignments, [("GREETING".to_string(), "hello".to_string())]);
        assert!(s.words.is_empty());
        let s = one("LANG=C sort names.txt");
        assert_eq!(s.assignments.len(), 1);
        assert_eq!(texts(&s), ["sort", "names.txt"]);
        // Only before the command
        assert_eq!(texts(&one("echo A=1")), ["echo", "A=1"]);
    }

    #[test]
    fn redirections() {
        let s = one("ls > out.txt");
        assert_eq!(texts(&s), ["ls"]);
        assert_eq!(s.redirects, [Redirect::Out { target: "out.txt".into(), append: false }]);

        let s = one("echo hi>>log.txt");
        assert_eq!(texts(&s), ["echo", "hi"]);
        assert_eq!(s.redirects, [Redirect::Out { target: "log.txt".into(), append: true }]);

        let s = one("cat missing 2>/dev/null");
        assert_eq!(texts(&s), ["cat", "missing"]);
        assert_eq!(s.redirects, [Redirect::Err { target: "/dev/null".into(), append: false }]);

        let s = one("cat a b > all.txt 2>&1");
        assert_eq!(s.redirects.len(), 2);
        assert_eq!(s.redirects[1], Redirect::ErrToOut);

        let s = one("wc -l < names.txt");
        assert_eq!(texts(&s), ["wc", "-l"]);
        assert_eq!(s.redirects, [Redirect::In("names.txt".into())]);

        // "2" that isn't right before > is an ordinary word
        assert_eq!(texts(&one("echo 2 > f")), ["echo", "2"]);
        assert_eq!(texts(&one("echo a2> f")), ["echo", "a2"]);
    }

    #[test]
    fn pipes_and_lists() {
        let list = parse("cat log | grep ERROR | wc -l", &vars).unwrap();
        assert_eq!(list[0].1.stages.len(), 3);

        let list = parse("mkdir x && cd x; ls || echo empty", &vars).unwrap();
        let joiners: Vec<Joiner> = list.iter().map(|(j, _)| *j).collect();
        assert_eq!(joiners, [Joiner::Always, Joiner::IfOk, Joiner::Always, Joiner::IfFailed]);

        assert!(parse("ls;", &vars).is_ok());
        assert!(parse("ls && ", &vars).is_err());
        assert!(parse("| grep x", &vars).is_err());
        assert!(parse("ls |", &vars).is_err());
        assert!(parse("echo > ", &vars).is_err());
    }

    #[test]
    fn unsupported_syntax_says_so() {
        for line in ["echo $(date)", "echo `date`", "sleep 5 &", "echo 'open"] {
            assert!(parse(line, &vars).is_err(), "{line}");
        }
    }

    #[test]
    fn comments() {
        assert!(parse("# just a note", &vars).unwrap().is_empty());
        assert_eq!(texts(&one("echo hi # note")), ["echo", "hi"]);
        assert_eq!(texts(&one("echo a#b")), ["echo", "a#b"]);
    }

    #[test]
    fn glob_matching() {
        assert!(glob_match("*.txt", "notes.txt"));
        assert!(!glob_match("*.txt", "notes.md"));
        assert!(glob_match("log?.txt", "log1.txt"));
        assert!(!glob_match("log?.txt", "log10.txt"));
        assert!(glob_match("[abc]*", "beta"));
        assert!(glob_match("[a-c]*", "cat"));
        assert!(!glob_match("[!a-c]*", "cat"));
        assert!(!glob_match("*", ".hidden"));
        assert!(glob_match(".*", ".hidden"));
        assert!(glob_match("*", ""));
        assert!(fnmatch("*.txt", ".hidden.txt"));
    }
}
