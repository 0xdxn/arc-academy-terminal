//! Text filters for the simulated sandbox: the commands that usually sit
//! on the right of a pipe (`grep`, `head`, `tail`, `wc`, `sort`, `uniq`,
//! `cut`, `sed`, `tr`), applied to text in memory, plus a line `diff`. Pure functions, so the sandbox can
//! run `cat log | grep ERROR | sort | uniq -c` without a real shell.

/// The programs [`apply`] understands
pub const FILTERS: &[&str] = &["grep", "head", "tail", "wc", "sort", "uniq", "cut", "sed", "tr"];

/// Run one filter over `input`. `args` are the words after the program
/// name (flags and operands, as typed). Returns `Err` with a short,
/// learner-facing message for bad usage.
pub fn apply(program: &str, args: &[String], input: &str) -> Result<String, String> {
    if program == "tr" {
        return tr(args, input);
    }
    let lines: Vec<&str> = input.lines().collect();
    let out: Vec<String> = match program {
        "grep" => grep(args, &lines)?,
        "head" => {
            let n = line_count(args)?;
            lines.iter().take(n).map(|l| l.to_string()).collect()
        }
        "tail" => {
            let n = line_count(args)?;
            // tail -n +N starts at line N (skips a header, for example)
            let skip = if tail_from_start(args) {
                n.saturating_sub(1)
            } else {
                lines.len().saturating_sub(n)
            };
            lines.iter().skip(skip).map(|l| l.to_string()).collect()
        }
        "wc" => {
            let words: usize = lines.iter().map(|l| l.split_whitespace().count()).sum();
            let only_lines = args.iter().any(|a| a == "-l");
            let only_words = args.iter().any(|a| a == "-w");
            vec![if only_lines {
                lines.len().to_string()
            } else if only_words {
                words.to_string()
            } else {
                format!("{} {} {}", lines.len(), words, input.len())
            }]
        }
        "sort" => {
            let numeric = args.iter().any(|a| a.starts_with('-') && a.contains('n'));
            let reverse = args.iter().any(|a| a.starts_with('-') && a.contains('r'));
            let mut v: Vec<String> = lines.iter().map(|l| l.to_string()).collect();
            if numeric {
                let key = |s: &String| {
                    s.split_whitespace()
                        .next()
                        .and_then(|w| w.parse::<f64>().ok())
                        .unwrap_or(0.0)
                };
                v.sort_by(|a, b| key(a).partial_cmp(&key(b)).unwrap_or(std::cmp::Ordering::Equal));
            } else {
                v.sort();
            }
            if reverse {
                v.reverse();
            }
            v
        }
        "uniq" => {
            let count = args.iter().any(|a| a == "-c");
            let mut v: Vec<(usize, String)> = Vec::new();
            for line in lines {
                match v.last_mut() {
                    Some((n, last)) if last == line => *n += 1,
                    _ => v.push((1, line.to_string())),
                }
            }
            v.into_iter()
                .map(|(n, l)| if count { format!("{n:>7} {l}") } else { l })
                .collect()
        }
        "cut" => cut(args, &lines)?,
        "sed" => sed(args, &lines)?,
        other => return Err(format!("{other} isn't available in the sandbox")),
    };
    let mut text = out.join("\n");
    if !text.is_empty() {
        text.push('\n');
    }
    Ok(text)
}

/// Operands (non-flag words), skipping values that belong to flags like
/// `-n 5`, `-d :` and `-f 1`
pub fn operands(program: &str, args: &[String]) -> Vec<String> {
    let takes_value: &[&str] = match program {
        "head" | "tail" => &["-n"],
        "cut" => &["-d", "-f"],
        "tr" => &[],
        _ => &[],
    };
    let mut out = Vec::new();
    let mut skip = false;
    for a in args {
        if skip {
            skip = false;
            continue;
        }
        if takes_value.contains(&a.as_str()) {
            skip = true;
            continue;
        }
        // grep's first operand is its pattern
        if a.starts_with('-') && a.len() > 1 {
            continue;
        }
        out.push(a.clone());
    }
    out
}

/// `tail -n +N` / `tail -n+N` / `tail +N`: count from the start, not the end
pub fn tail_from_start(args: &[String]) -> bool {
    let mut prev_n = false;
    for a in args {
        if (prev_n && a.starts_with('+')) || a.starts_with("-n+") || (a.starts_with('+') && a.len() > 1) {
            return true;
        }
        prev_n = a == "-n";
    }
    false
}

fn line_count(args: &[String]) -> Result<usize, String> {
    let mut iter = args.iter();
    while let Some(a) = iter.next() {
        if a == "-n" {
            return iter
                .next()
                .and_then(|v| v.parse().ok())
                .ok_or_else(|| "-n needs a number, like -n 5".to_string());
        }
        if let Some(n) = a.strip_prefix("-n").or_else(|| a.strip_prefix('-')) {
            if let Ok(n) = n.parse() {
                return Ok(n);
            }
        }
    }
    Ok(10)
}

/// The script operand of a sed command (the first non-flag word)
pub fn sed_script(args: &[String]) -> Option<&String> {
    args.iter().find(|a| !(a.starts_with('-') && a.len() > 1))
}

/// sed: `s/old/new/` (flags `g`, `i`), `/pattern/d`, and `-n '/pattern/p'`
fn sed(args: &[String], lines: &[&str]) -> Result<Vec<String>, String> {
    let quiet = args.iter().any(|a| a == "-n");
    for a in args.iter().filter(|a| a.starts_with('-') && a.len() > 1) {
        if !matches!(a.as_str(), "-n" | "-E" | "-r" | "-i") {
            return Err(format!("sed: the {a} flag isn't available in the sandbox"));
        }
    }
    let script = sed_script(args).ok_or("sed needs a script, like: sed 's/old/new/'")?;
    let usage = || format!("sed: '{script}' isn't a script the sandbox understands (try s/old/new/ or /word/d)");

    if let Some(body) = script.strip_prefix('s') {
        let delim = body.chars().next().ok_or_else(usage)?;
        let parts: Vec<&str> = body[delim.len_utf8()..].split(delim).collect();
        if parts.len() != 3 {
            return Err(usage());
        }
        let (pattern, replacement, flags) = (parts[0], parts[1], parts[2]);
        let global = flags.contains('g');
        let print = flags.contains('p');
        if flags.chars().any(|c| !"gipI".contains(c)) {
            return Err(usage());
        }
        let re = regex::RegexBuilder::new(pattern)
            .case_insensitive(flags.contains('i') || flags.contains('I'))
            .build()
            .map_err(|_| format!("sed: '{pattern}' isn't a pattern sed understands"))?;
        // sed's & and \1 become the regex crate's ${0} and ${1}
        let mut rep = String::new();
        let mut chars = replacement.chars().peekable();
        while let Some(c) = chars.next() {
            match c {
                '&' => rep.push_str("${0}"),
                '\\' => match chars.next() {
                    Some(d) if d.is_ascii_digit() => rep.push_str(&format!("${{{d}}}")),
                    Some('n') => rep.push('\n'),
                    Some(other) => rep.push(other),
                    None => {}
                },
                '$' => rep.push_str("$$"),
                other => rep.push(other),
            }
        }
        let mut out = Vec::new();
        for line in lines {
            let changed = re.is_match(line);
            let new = if global {
                re.replace_all(line, rep.as_str()).into_owned()
            } else {
                re.replace(line, rep.as_str()).into_owned()
            };
            if !quiet {
                out.push(new.clone());
            }
            if print && changed {
                out.push(new);
            }
        }
        return Ok(out);
    }

    // /pattern/d or /pattern/p
    let (pattern, command) = script
        .strip_prefix('/')
        .and_then(|rest| rest.rsplit_once('/'))
        .ok_or_else(usage)?;
    let re = regex::Regex::new(pattern)
        .map_err(|_| format!("sed: '{pattern}' isn't a pattern sed understands"))?;
    let mut out = Vec::new();
    for line in lines {
        let hit = re.is_match(line);
        match command {
            "d" => {
                if !hit && !quiet {
                    out.push(line.to_string());
                }
            }
            "p" => {
                if !quiet {
                    out.push(line.to_string());
                }
                if hit {
                    out.push(line.to_string());
                }
            }
            _ => return Err(usage()),
        }
    }
    Ok(out)
}

/// Expand a tr set: ranges like a-z, and \n \t escapes
fn tr_set(set: &str) -> Vec<char> {
    let mut raw = Vec::new();
    let mut chars = set.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('n') => raw.push('\n'),
                Some('t') => raw.push('\t'),
                Some(other) => raw.push(other),
                None => raw.push('\\'),
            }
        } else {
            raw.push(c);
        }
    }
    let mut out = Vec::new();
    let mut i = 0;
    while i < raw.len() {
        if i + 2 < raw.len() && raw[i + 1] == '-' && raw[i] <= raw[i + 2] {
            out.extend(raw[i]..=raw[i + 2]);
            i += 3;
        } else {
            out.push(raw[i]);
            i += 1;
        }
    }
    out
}

/// tr: `tr a-z A-Z` translates, `tr -d SET` deletes
fn tr(args: &[String], input: &str) -> Result<String, String> {
    let delete = args.iter().any(|a| a == "-d");
    let sets: Vec<&String> = args.iter().filter(|a| !(a.starts_with('-') && a.len() > 1)).collect();
    if delete {
        let set = tr_set(sets.first().ok_or("tr -d needs the characters to delete, like: tr -d ','")?);
        return Ok(input.chars().filter(|c| !set.contains(c)).collect());
    }
    let (Some(from), Some(to)) = (sets.first(), sets.get(1)) else {
        return Err("tr needs two sets, like: tr a-z A-Z".to_string());
    };
    let (from, to) = (tr_set(from), tr_set(to));
    if to.is_empty() {
        return Err("tr needs two sets, like: tr a-z A-Z".to_string());
    }
    Ok(input
        .chars()
        .map(|c| match from.iter().position(|&f| f == c) {
            Some(i) => *to.get(i).unwrap_or(to.last().unwrap()),
            None => c,
        })
        .collect())
}

/// Compare two texts line by line, in diff's classic format
/// (`2c2`, `< old`, `---`, `> new`). Empty when they match.
pub fn diff(old: &str, new: &str) -> String {
    let a: Vec<&str> = old.lines().collect();
    let b: Vec<&str> = new.lines().collect();
    // Longest common subsequence table
    let mut lcs = vec![vec![0usize; b.len() + 1]; a.len() + 1];
    for i in (0..a.len()).rev() {
        for j in (0..b.len()).rev() {
            lcs[i][j] = if a[i] == b[j] {
                lcs[i + 1][j + 1] + 1
            } else {
                lcs[i + 1][j].max(lcs[i][j + 1])
            };
        }
    }
    // A run of `len` (at least 1) lines starting at line `start`
    let range = |start: usize, len: usize| {
        if len == 1 {
            start.to_string()
        } else {
            format!("{},{}", start, start + len - 1)
        }
    };
    let mut out = String::new();
    let (mut i, mut j) = (0, 0);
    while i < a.len() || j < b.len() {
        if i < a.len() && j < b.len() && a[i] == b[j] {
            i += 1;
            j += 1;
            continue;
        }
        let (si, sj) = (i, j);
        while i < a.len() || j < b.len() {
            if i < a.len() && j < b.len() && a[i] == b[j] {
                break;
            }
            if j < b.len() && (i == a.len() || lcs[i][j + 1] >= lcs[i + 1][j]) {
                j += 1;
            } else {
                i += 1;
            }
        }
        let (dels, adds) = (i - si, j - sj);
        let header = match (dels, adds) {
            (0, _) => format!("{}a{}", si, range(sj + 1, adds)),
            (_, 0) => format!("{}d{}", range(si + 1, dels), sj),
            _ => format!("{}c{}", range(si + 1, dels), range(sj + 1, adds)),
        };
        out.push_str(&header);
        out.push('\n');
        for line in &a[si..i] {
            out.push_str(&format!("< {line}\n"));
        }
        if dels > 0 && adds > 0 {
            out.push_str("---\n");
        }
        for line in &b[sj..j] {
            out.push_str(&format!("> {line}\n"));
        }
    }
    out
}

fn grep(args: &[String], lines: &[&str]) -> Result<Vec<String>, String> {
    let mut ignore_case = false;
    let mut numbers = false;
    let mut invert = false;
    let mut count = false;
    let mut pattern: Option<&str> = None;
    for a in args {
        if a.starts_with('-') && a.len() > 1 && pattern.is_none() {
            for c in a.chars().skip(1) {
                match c {
                    'i' => ignore_case = true,
                    'n' => numbers = true,
                    'v' => invert = true,
                    'c' => count = true,
                    'E' => {}
                    other => return Err(format!("grep: the -{other} flag isn't available in the sandbox")),
                }
            }
        } else if pattern.is_none() {
            pattern = Some(a);
        }
    }
    let pattern = pattern.ok_or("grep needs a word to search for, like: grep ERROR")?;
    let re = regex::RegexBuilder::new(pattern)
        .case_insensitive(ignore_case)
        .build()
        .map_err(|_| format!("grep: '{pattern}' isn't a pattern grep understands"))?;
    let hits: Vec<String> = lines
        .iter()
        .enumerate()
        .filter(|(_, l)| re.is_match(l) != invert)
        .map(|(i, l)| if numbers { format!("{}:{}", i + 1, l) } else { l.to_string() })
        .collect();
    Ok(if count { vec![hits.len().to_string()] } else { hits })
}

fn cut(args: &[String], lines: &[&str]) -> Result<Vec<String>, String> {
    let mut delim = "\t".to_string();
    let mut fields: Option<Vec<usize>> = None;
    let mut iter = args.iter();
    while let Some(a) = iter.next() {
        let (flag, inline) = if a.starts_with("-d") || a.starts_with("-f") {
            (&a[..2], &a[2..])
        } else {
            continue;
        };
        let value = if inline.is_empty() {
            iter.next().cloned().unwrap_or_default()
        } else {
            inline.to_string()
        };
        if flag == "-d" {
            delim = value.trim_matches(|c| c == '\'' || c == '"').to_string();
        } else {
            fields = Some(
                value
                    .split(',')
                    .filter_map(|f| f.parse::<usize>().ok())
                    .filter(|f| *f > 0)
                    .collect(),
            );
        }
    }
    let fields = fields.ok_or("cut needs -f to pick fields, like: cut -d: -f1")?;
    if delim.is_empty() {
        return Err("cut -d needs a character, like -d:".to_string());
    }
    Ok(lines
        .iter()
        .map(|l| {
            let parts: Vec<&str> = l.split(delim.as_str()).collect();
            fields
                .iter()
                .filter_map(|f| parts.get(f - 1).copied())
                .collect::<Vec<_>>()
                .join(&delim)
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(program: &str, args: &[&str], input: &str) -> String {
        let args: Vec<String> = args.iter().map(|s| s.to_string()).collect();
        apply(program, &args, input).unwrap()
    }

    const LOG: &str = "a INFO ok\nb ERROR bad\nc INFO ok\nd error worse\n";

    #[test]
    fn tail_plus_starts_at_a_line() {
        let text = "header\na\nb\nc\n";
        assert_eq!(run("tail", &["-n", "+2"], text), "a\nb\nc\n");
        assert_eq!(run("tail", &["-n+3"], text), "b\nc\n");
        assert_eq!(run("tail", &["-n", "2"], text), "b\nc\n");
    }

    #[test]
    fn sed_substitutes_and_filters() {
        let text = "cat sat\ncat ran\ndog sat\n";
        assert_eq!(run("sed", &["s/cat/dog/"], text), "dog sat\ndog ran\ndog sat\n");
        assert_eq!(run("sed", &["s/a/o/g"], "banana\n"), "bonono\n");
        assert_eq!(run("sed", &["s/a/o/"], "banana\n"), "bonana\n");
        assert_eq!(run("sed", &["s/CAT/x/i"], "cat\n"), "x\n");
        assert_eq!(run("sed", &["s|/tmp|/home|"], "/tmp/a\n"), "/home/a\n");
        assert_eq!(run("sed", &["s/[0-9]+/<&>/"], "day 12\n"), "day <12>\n");
        assert_eq!(run("sed", &["s/(\\w+) (\\w+)/\\2 \\1/"], "hello world\n"), "world hello\n");
        assert_eq!(run("sed", &["s/a/$5/"], "a\n"), "$5\n");
        assert_eq!(run("sed", &["/dog/d"], text), "cat sat\ncat ran\n");
        assert_eq!(run("sed", &["-n", "/ran/p"], text), "cat ran\n");
        assert!(apply("sed", &["y/a/b/".to_string()], text).is_err());
        assert!(apply("sed", &["-z".to_string(), "s/a/b/".to_string()], text).is_err());
    }

    #[test]
    fn tr_translates_and_deletes() {
        assert_eq!(run("tr", &["a-z", "A-Z"], "hi there\n"), "HI THERE\n");
        assert_eq!(run("tr", &["-d", ","], "a,b,c\n"), "abc\n");
        assert_eq!(run("tr", &[" ", "\\n"], "a b\n"), "a\nb\n");
        assert!(apply("tr", &["a".to_string()], "x").is_err());
    }

    #[test]
    fn diff_classic_format() {
        assert_eq!(diff("a\nb\n", "a\nb\n"), "");
        assert_eq!(diff("a\nb\nc\n", "a\nB\nc\n"), "2c2\n< b\n---\n> B\n");
        assert_eq!(diff("a\nc\n", "a\nb\nc\n"), "1a2\n> b\n");
        assert_eq!(diff("a\nb\nc\n", "a\nc\n"), "2d1\n< b\n");
        assert_eq!(diff("", "x\ny\n"), "0a1,2\n> x\n> y\n");
        assert_eq!(diff("a\nb\nc\nd\n", "a\nX\nY\nd\n"), "2,3c2,3\n< b\n< c\n---\n> X\n> Y\n");
    }

    #[test]
    fn grep_flags() {
        assert_eq!(run("grep", &["ERROR"], LOG), "b ERROR bad\n");
        assert_eq!(run("grep", &["-i", "error"], LOG), "b ERROR bad\nd error worse\n");
        assert_eq!(run("grep", &["-n", "ERROR"], LOG), "2:b ERROR bad\n");
        assert_eq!(run("grep", &["-c", "INFO"], LOG), "2\n");
        assert_eq!(run("grep", &["-v", "INFO"], LOG), "b ERROR bad\nd error worse\n");
        assert_eq!(run("grep", &["-E", "^[ab]"], LOG), "a INFO ok\nb ERROR bad\n");
    }

    #[test]
    fn head_tail_wc() {
        assert_eq!(run("head", &["-n", "1"], LOG), "a INFO ok\n");
        assert_eq!(run("head", &["-2"], LOG), "a INFO ok\nb ERROR bad\n");
        assert_eq!(run("tail", &["-n", "1"], LOG), "d error worse\n");
        assert_eq!(run("wc", &["-l"], LOG), "4\n");
    }

    #[test]
    fn sort_uniq_cut() {
        let fruit = "banana\napple\nbanana\napple\napple\n";
        assert_eq!(run("sort", &[], fruit), "apple\napple\napple\nbanana\nbanana\n");
        let sorted = run("sort", &[], fruit);
        assert_eq!(run("uniq", &["-c"], &sorted), "      3 apple\n      2 banana\n");
        assert_eq!(run("cut", &["-d:", "-f1"], "alice:87\nbob:62\n"), "alice\nbob\n");
        assert_eq!(run("cut", &["-d", ":", "-f", "2"], "alice:87\n"), "87\n");
        assert_eq!(run("sort", &["-rn"], "2\n10\n1\n"), "10\n2\n1\n");
    }

    #[test]
    fn operands_skip_flag_values() {
        let args: Vec<String> = ["-n", "5", "log.txt"].iter().map(|s| s.to_string()).collect();
        assert_eq!(operands("head", &args), vec!["log.txt"]);
        let args: Vec<String> = ["-d", ":", "-f", "1", "s.txt"].iter().map(|s| s.to_string()).collect();
        assert_eq!(operands("cut", &args), vec!["s.txt"]);
    }

    #[test]
    fn bad_usage_explains() {
        assert!(apply("grep", &[], "x").unwrap_err().contains("needs a word"));
        assert!(apply("cut", &["-d:".into()], "x").unwrap_err().contains("-f"));
    }
}
