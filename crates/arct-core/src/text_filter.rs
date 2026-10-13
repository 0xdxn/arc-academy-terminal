//! Text filters for the simulated sandbox: the commands that usually sit
//! on the right of a pipe (`grep`, `head`, `tail`, `wc`, `sort`, `uniq`,
//! `cut`), applied to text in memory. Pure functions, so the sandbox can
//! run `cat log | grep ERROR | sort | uniq -c` without a real shell.

/// The programs [`apply`] understands
pub const FILTERS: &[&str] = &["grep", "head", "tail", "wc", "sort", "uniq", "cut"];

/// Run one filter over `input`. `args` are the words after the program
/// name (flags and operands, as typed). Returns `Err` with a short,
/// learner-facing message for bad usage.
pub fn apply(program: &str, args: &[String], input: &str) -> Result<String, String> {
    let lines: Vec<&str> = input.lines().collect();
    let out: Vec<String> = match program {
        "grep" => grep(args, &lines)?,
        "head" => {
            let n = line_count(args)?;
            lines.iter().take(n).map(|l| l.to_string()).collect()
        }
        "tail" => {
            let n = line_count(args)?;
            let skip = lines.len().saturating_sub(n);
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
