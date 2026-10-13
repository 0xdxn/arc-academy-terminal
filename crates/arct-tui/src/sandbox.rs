//! The simulated sandbox's shell: what happens to a typed line before and
//! around each command. Variables, globs, redirection, pipes, `&&` / `||`
//! / `;`, scripts with arguments, and `PATH` lookups, all inside the
//! lesson's practice files. The commands themselves live in
//! `App::execute_virtual_fs_command` (files and text) and here (the ones
//! that need the shell's own state).

use crate::app::App;
use crate::icons;
use arct_core::shell_syntax::{self, Joiner, Pipeline, Redirect, Word};
use arct_core::text_filter;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// The sandbox shell's state: variables, script arguments, last status
#[derive(Debug, Clone)]
pub struct SandboxShell {
    vars: BTreeMap<String, String>,
    exported: BTreeSet<String>,
    /// `$1`, `$2`, ... inside a script
    args: Vec<String>,
    /// `$?`: 0 when the last command worked
    status: i32,
    /// How many scripts deep we are (scripts can't start scripts)
    depth: usize,
    /// Set by `exit` inside a script
    exiting: bool,
    /// Variables set in real-files mode (each command there is a new
    /// process, so the app carries them from one command to the next)
    real: BTreeMap<String, String>,
    /// Exit status a command chose that its printout can't show: diff
    /// finding differences, grep finding nothing (both exit 1 in sh)
    pending_status: Option<i32>,
}

impl Default for SandboxShell {
    fn default() -> Self {
        Self::new()
    }
}

impl SandboxShell {
    pub fn new() -> Self {
        let mut shell = Self {
            vars: BTreeMap::new(),
            exported: BTreeSet::new(),
            args: Vec::new(),
            status: 0,
            depth: 0,
            exiting: false,
            real: BTreeMap::new(),
            pending_status: None,
        };
        for (k, v) in [
            ("HOME", "/lesson-home"),
            ("USER", "learner"),
            ("SHELL", "/bin/sh"),
            ("PATH", "/usr/local/bin:/usr/bin:/bin"),
            ("LANG", "en_US.UTF-8"),
        ] {
            shell.vars.insert(k.to_string(), v.to_string());
            shell.exported.insert(k.to_string());
        }
        shell
    }

    fn lookup(&self, name: &str, pwd: &str) -> Option<String> {
        match name {
            "?" => Some(self.status.to_string()),
            "#" => Some(self.args.len().to_string()),
            "@" | "*" => Some(self.args.join(" ")),
            "0" => Some("sh".to_string()),
            "PWD" => Some(pwd.to_string()),
            n if n.chars().all(|c| c.is_ascii_digit()) => {
                let i: usize = n.parse().ok()?;
                self.args.get(i.checked_sub(1)?).cloned()
            }
            n => self.vars.get(n).cloned(),
        }
    }

    fn set(&mut self, name: &str, value: &str) {
        self.vars.insert(name.to_string(), value.to_string());
    }

    /// A variable's value in real-files mode: set in this lesson, or from
    /// the real environment (see [`fresh_shell_lookup`])
    fn real_lookup(&self, name: &str, pwd: &str) -> Option<String> {
        fresh_shell_lookup(name, pwd, &|n| self.real.get(n).cloned().or_else(|| std::env::var(n).ok()))
    }

    /// Real-files mode: [`expand_real_line`] with this lesson's variables
    pub(crate) fn expand_for_real(&self, line: &str, cwd: &Path, root: &Path) -> Result<Option<String>, String> {
        expand_real_line(line, cwd, root, &|n| self.real.get(n).cloned().or_else(|| std::env::var(n).ok()))
    }

    /// Real-files mode: handle a line that only sets or clears variables
    /// (`NAME=value`, `export NAME=value`, `unset NAME`). Returns `None`
    /// for any other line, which then runs in a real shell.
    pub(crate) fn real_assignments(&mut self, line: &str, pwd: &str) -> Option<String> {
        let real_value = |name: &str| std::env::var(name).ok();
        let lookup = |name: &str| match name {
            // a status from an earlier line isn't kept between real commands
            "?" => Some("0".to_string()),
            n => self.real_lookup(n, pwd),
        };
        let mut list = shell_syntax::parse(line, &lookup).ok()?;
        if list.len() != 1 || list[0].1.stages.len() != 1 {
            return None;
        }
        let stage = list.remove(0).1.stages.remove(0);
        if !stage.redirects.is_empty() {
            return None;
        }
        let words: Vec<&str> = stage.words.iter().map(|w| w.text.as_str()).collect();
        match words.split_first() {
            None => {
                for (name, value) in stage.assignments {
                    self.real.insert(name, value);
                }
                Some(String::new())
            }
            Some((&"export", rest)) if stage.assignments.is_empty() && !rest.is_empty() => {
                for word in rest {
                    match word.split_once('=') {
                        Some((name, value)) if shell_syntax::is_name(name) => {
                            self.real.insert(name.to_string(), value.to_string());
                        }
                        None if shell_syntax::is_name(word) => {
                            if let Some(value) = real_value(word) {
                                self.real.entry(word.to_string()).or_insert(value);
                            }
                        }
                        _ => return Some(error(format!("export: '{word}': not a valid name"))),
                    }
                }
                Some(String::new())
            }
            Some((&"unset", rest)) if stage.assignments.is_empty() => {
                for name in rest {
                    self.real.remove(*name);
                }
                Some(String::new())
            }
            _ => None,
        }
    }

    /// Variables set during a real-files lesson, for the next command's environment
    pub(crate) fn real_env(&self) -> impl Iterator<Item = (String, String)> + '_ {
        self.real.iter().map(|(k, v)| (k.clone(), v.clone()))
    }
}

/// A line for a real shell (real-files lessons, the safe-mode free shell),
/// expanded here (variables from `vars`, `~`, globs against the real
/// files under `root`) and written back fully quoted
/// (`shell_syntax::render`), so the guard checks the command as it
/// will really run and the shell expands nothing again. `Ok(None)` for
/// a line with `$(...)` or backticks, which the guard refuses as it
/// always has. Each part of a list is expanded after the ones before
/// it, as sh does: in `X=5; echo $X` the echo sees 5.
pub(crate) fn expand_real_line(
    line: &str,
    cwd: &Path,
    root: &Path,
    vars: &dyn Fn(&str) -> Option<String>,
) -> Result<Option<String>, String> {
    if line.contains("$(") || line.contains('`') {
        return Ok(None);
    }
    let pwd = cwd.display().to_string();
    // Variables set earlier in this line (None: unset)
    let mut local: BTreeMap<String, Option<String>> = BTreeMap::new();
    let mut list = Vec::new();
    for (joiner, part) in shell_syntax::split_list(line) {
        let lookup = |name: &str| match local.get(name) {
            Some(value) => value.clone(),
            None => fresh_shell_lookup(name, &pwd, vars),
        };
        let mut parsed = shell_syntax::parse(&part, &lookup)?;
        if parsed.is_empty() {
            if joiner != Joiner::Always {
                return Err("syntax error: a command is missing after && or ||".to_string());
            }
            continue;
        }
        let mut pipeline = parsed.remove(0).1;
        // Inside a loop or an if, the shell fills in $ as it goes, so
        // there is no value to check before it runs
        let opens_block = pipeline.stages.iter().any(|stage| {
            stage
                .words
                .first()
                .is_some_and(|w| KEYWORDS.contains(&w.text.as_str()) || matches!(w.text.as_str(), "{" | "}" | "!"))
        });
        // Shell grammar the parser doesn't model would reach sh quoted, as a
        // command name that doesn't exist: say so instead
        let grouping = pipeline.stages.iter().any(|stage| {
            stage.words.first().is_some_and(|w| {
                w.text.contains(['(', ')']) || matches!(w.text.as_str(), "{" | "}" | "!" | "[[" | "case" | "function")
            })
        });
        if grouping {
            return Err(
                "Grouping with ( ) or { }, functions, case, [[ and ! aren't available here: \
                 run the commands one at a time."
                    .to_string(),
            );
        }
        if opens_block && line.contains('$') {
            return Err(
                "arct fills in each $ before anything runs, so the safety guard can check \
                 the real values. Inside for, while and if, the shell fills them in as it \
                 goes, so they can't be checked here: type the values out instead."
                    .to_string(),
            );
        }
        for stage in &mut pipeline.stages {
            stage.words = stage
                .words
                .drain(..)
                .flat_map(|word| {
                    let matches = if word.glob { real_glob(&word.text, cwd, root) } else { Vec::new() };
                    if matches.is_empty() {
                        vec![Word { glob: false, ..word }]
                    } else {
                        matches.into_iter().map(|text| Word { text, glob: false }).collect()
                    }
                })
                .collect();
        }
        if let [stage] = pipeline.stages.as_slice() {
            let words: Vec<&str> = stage.words.iter().map(|w| w.text.as_str()).collect();
            match words.split_first() {
                None => {
                    for (name, value) in &stage.assignments {
                        local.insert(name.clone(), Some(value.clone()));
                    }
                }
                Some((&"export", rest)) => {
                    for word in rest {
                        if let Some((name, value)) = word.split_once('=') {
                            local.insert(name.to_string(), Some(value.to_string()));
                        }
                    }
                }
                Some((&"unset", rest)) => {
                    for name in rest {
                        local.insert(name.to_string(), None);
                    }
                }
                _ => {}
            }
        }
        list.push((joiner, pipeline));
    }
    Ok(Some(shell_syntax::render(&list)))
}

/// What `$name` means to a fresh `sh -c` running in `pwd`, with `vars`
/// for named variables. `$?` is left for the shell to fill in (see
/// `shell_syntax::STATUS_MARK`).
pub(crate) fn fresh_shell_lookup(name: &str, pwd: &str, vars: &dyn Fn(&str) -> Option<String>) -> Option<String> {
    match name {
        "?" => Some(shell_syntax::STATUS_MARK.to_string()),
        "#" => Some("0".to_string()),
        "@" | "*" => Some(String::new()),
        "0" => Some("sh".to_string()),
        "PWD" => Some(pwd.to_string()),
        n if n.chars().all(|c| c.is_ascii_digit()) => None,
        n => vars(n),
    }
}

/// Expand a glob against the real files, as sh does, except that it never
/// matches `.` or `..` (read_dir doesn't list them) and a leading dot only
/// matches when typed (`glob_match`). Relative patterns are relative to
/// `cwd`. A pattern whose fixed start already lies outside `root` isn't
/// expanded at all (the guard refuses it as typed), so this never lists a
/// folder outside the playground, and folders are compared by their real
/// path, so a symlink out of the playground isn't followed (the OS jail is
/// what holds against the rest). No match: empty, and the word stays as
/// typed, as in sh.
fn real_glob(pattern: &str, cwd: &Path, root: &Path) -> Vec<String> {
    let is_glob = |part: &str| part.contains(['*', '?', '[']);
    let absolute = pattern.starts_with('/');
    let parts: Vec<&str> = pattern.split('/').filter(|p| !p.is_empty()).collect();
    let fixed: Vec<&str> = parts.iter().copied().take_while(|p| !is_glob(p)).collect();
    let base = if absolute { PathBuf::from("/") } else { cwd.to_path_buf() };
    if !lexically_inside(&base.join(fixed.join("/")), root) {
        return Vec::new();
    }
    let join = |prefix: &str, part: &str| match prefix {
        "" => part.to_string(),
        "/" => format!("/{part}"),
        _ => format!("{prefix}/{part}"),
    };
    let on_disk = |text: &str| if absolute { PathBuf::from(text) } else { cwd.join(text) };
    let real_root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let really_inside = |dir: &Path| dir.canonicalize().is_ok_and(|real| real.starts_with(&real_root));
    let mut found: Vec<String> = vec![if absolute { "/".to_string() } else { String::new() }];
    for part in &parts {
        let mut next = Vec::new();
        for prefix in &found {
            if !is_glob(part) {
                next.push(join(prefix, part));
                continue;
            }
            let dir = if prefix.is_empty() { cwd.to_path_buf() } else { on_disk(prefix) };
            if !really_inside(&dir) {
                continue;
            }
            let Ok(entries) = std::fs::read_dir(&dir) else { continue };
            let mut names: Vec<String> = entries
                .filter_map(|e| e.ok()?.file_name().into_string().ok())
                .filter(|name| shell_syntax::glob_match(part, name))
                .collect();
            names.sort();
            next.extend(names.iter().map(|name| join(prefix, name)));
        }
        found = next;
    }
    let dirs_only = pattern.ends_with('/');
    found
        .into_iter()
        .filter(|text| std::fs::symlink_metadata(on_disk(text)).is_ok())
        .filter(|text| on_disk(text).parent().is_some_and(really_inside))
        .filter(|text| !dirs_only || on_disk(text).is_dir())
        .map(|text| if dirs_only { format!("{text}/") } else { text })
        .collect()
}

/// Is `path`, with `.` and `..` resolved by name only, inside `root`?
fn lexically_inside(path: &Path, root: &Path) -> bool {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::ParentDir => {
                if !out.pop() {
                    return false;
                }
            }
            std::path::Component::CurDir => {}
            other => out.push(other),
        }
    }
    out.starts_with(root)
}

/// Commands the sandbox runs, for `which` and `type`
const PROGRAMS: &[&str] = &[
    "ls", "cat", "head", "tail", "wc", "grep", "sort", "uniq", "cut", "sed", "tr", "mkdir",
    "touch", "rm", "mv", "cp", "chmod", "find", "du", "tar", "gzip", "gunzip", "diff", "tee",
    "env", "printenv", "which", "sh", "bash", "ssh-keygen",
];
const BUILTINS: &[&str] = &["cd", "echo", "pwd", "export", "unset", "type", "exit"];
const KEYWORDS: &[&str] = &["if", "then", "else", "elif", "fi", "for", "while", "until", "do", "done", "case", "esac", "function"];

enum Dest {
    Terminal,
    Null,
    File { path: String, append: bool },
}

impl Dest {
    fn of(target: &str, append: bool) -> Self {
        if target == "/dev/null" {
            Dest::Null
        } else {
            Dest::File { path: target.to_string(), append }
        }
    }
}

fn error(message: impl std::fmt::Display) -> String {
    format!("{}{}\n", icons::error().content, message)
}

/// Lines that are messages about the command (errors, warnings, hints,
/// confirmations) rather than its output
fn is_message(line: &str) -> bool {
    [icons::error(), icons::warning(), icons::hint(), icons::success()]
        .iter()
        .any(|icon| line.starts_with(icon.content.as_ref()))
}

fn is_error(line: &str) -> bool {
    line.starts_with(icons::error().content.as_ref()) || line.starts_with(icons::warning().content.as_ref())
}

/// Split a command's printout into (output, errors, notes)
fn split_streams(text: &str) -> (String, String, String) {
    let (mut out, mut err, mut notes) = (String::new(), String::new(), String::new());
    for line in text.split_inclusive('\n') {
        if is_error(line) {
            err.push_str(line);
        } else if is_message(line) {
            notes.push_str(line);
        } else {
            out.push_str(line);
        }
    }
    (out, err, notes)
}

impl App {
    /// Run one typed line in the simulated sandbox. `None` outside lesson
    /// mode or before the sandbox exists.
    pub(crate) fn run_sandbox_line(&mut self, line: &str) -> Option<String> {
        if !self.lesson_mode || self.virtual_fs.is_none() {
            return None;
        }
        let mut output = String::new();
        for (joiner, part) in shell_syntax::split_list(line) {
            if self.sandbox.exiting {
                break;
            }
            let run = match joiner {
                Joiner::Always => true,
                Joiner::IfOk => self.sandbox.status == 0,
                Joiner::IfFailed => self.sandbox.status != 0,
            };
            let pwd = self.virtual_fs.as_ref()?.get_current_dir().display().to_string();
            let shell = &self.sandbox;
            let parsed = shell_syntax::parse(&part, &|name| shell.lookup(name, &pwd));
            let pipeline = match parsed {
                Ok(list) if list.is_empty() && joiner != Joiner::Always => {
                    output.push_str(&error("syntax error: a command is missing after && or ||"));
                    self.sandbox.status = 2;
                    break;
                }
                Ok(list) if list.is_empty() => continue,
                Ok(mut list) => list.remove(0).1,
                Err(message) => {
                    output.push_str(&error(message));
                    self.sandbox.status = 2;
                    break;
                }
            };
            if !run {
                continue;
            }
            self.sandbox.pending_status = None;
            let text = self.run_sandbox_pipeline(&pipeline);
            let pending = self.sandbox.pending_status.take();
            self.sandbox.status = if Self::output_reports_failure(&text) { 1 } else { pending.unwrap_or(0) };
            output.push_str(&text);
        }
        Some(output)
    }

    fn expand_globs(&self, words: &[Word]) -> Vec<String> {
        let Some(vfs) = self.virtual_fs.as_ref() else {
            return words.iter().map(|w| w.text.clone()).collect();
        };
        let mut out = Vec::new();
        for word in words {
            let matches = if word.glob { vfs.glob(&word.text) } else { Vec::new() };
            if matches.is_empty() {
                out.push(word.text.clone());
            } else {
                out.extend(matches);
            }
        }
        out
    }

    fn run_sandbox_pipeline(&mut self, pipeline: &Pipeline) -> String {
        let mut terminal = String::new();
        let mut piped: Option<String> = None;
        let last = pipeline.stages.len() - 1;
        for (i, stage) in pipeline.stages.iter().enumerate() {
            if stage.words.is_empty() {
                for (name, value) in &stage.assignments {
                    self.sandbox.set(name, value);
                }
                continue;
            }
            let words = self.expand_globs(&stage.words);

            // The guard checks the command as it will actually run, after
            // variables and globs: `X=/; rm -rf $X` is still rm -rf /
            let expanded = shellwords::join(&words.iter().map(String::as_str).collect::<Vec<_>>());
            if let arct_core::GuardVerdict::Refuse { reason } =
                arct_core::PlaygroundGuard::check_catastrophic(&expanded)
            {
                self.record_feature_event("guard_refusal");
                terminal.push_str(&format!(
                    "{}Blocked. Nothing ran.\n\n{}\n",
                    icons::warning().content,
                    reason
                ));
                return terminal;
            }

            let mut stdin = piped.take();
            for redirect in &stage.redirects {
                if let Redirect::In(file) = redirect {
                    match self.virtual_fs.as_ref().map(|v| v.read_file(file)) {
                        Some(Ok(text)) => stdin = Some(text),
                        Some(Err(_)) | None => {
                            terminal.push_str(&error(format!("{file}: No such file or directory")));
                            return terminal;
                        }
                    }
                }
            }

            // NAME=value before a command applies to that command only
            // (and is in that command's environment, as if exported)
            let saved: Vec<(String, Option<String>, bool)> = stage
                .assignments
                .iter()
                .map(|(name, value)| {
                    let old = self.sandbox.vars.get(name).cloned();
                    let was_exported = !self.sandbox.exported.insert(name.clone());
                    self.sandbox.set(name, value);
                    (name.clone(), old, was_exported)
                })
                .collect();

            // > and 2> empty their file before the command runs (so
            // `sort x > x` leaves x empty, as in sh)
            for redirect in &stage.redirects {
                if let Redirect::Out { target, append: false } | Redirect::Err { target, append: false } = redirect {
                    if target != "/dev/null" {
                        if let Some(vfs) = self.virtual_fs.as_ref() {
                            let _ = vfs.write_file(target, "", false);
                        }
                    }
                }
            }
            // A pipeline's status is its last command's
            self.sandbox.pending_status = None;
            let printed = self.run_sandbox_stage(&words, stdin);
            match words[0].as_str() {
                "diff" if !printed.is_empty() && !Self::output_reports_failure(&printed) => {
                    self.sandbox.pending_status = Some(1)
                }
                "grep" if printed.trim().is_empty() || printed.starts_with("(no lines matched") => {
                    self.sandbox.pending_status = Some(1)
                }
                _ => {}
            }
            for (name, old, was_exported) in saved {
                if !was_exported {
                    self.sandbox.exported.remove(&name);
                }
                match old {
                    Some(value) => self.sandbox.set(&name, &value),
                    None => {
                        self.sandbox.vars.remove(&name);
                    }
                }
            }
            // Errors count even when 2> sends them away
            if Self::output_reports_failure(&printed) {
                self.sandbox.pending_status = Some(1);
            }

            // Last command, nothing redirected: show it exactly as printed
            if i == last && stage.redirects.iter().all(|r| matches!(r, Redirect::In(_))) {
                terminal.push_str(&printed);
                continue;
            }
            let (out, err, notes) = split_streams(&printed);
            let (out, err) = self.route(&stage.redirects, out, err, &mut terminal);
            terminal.push_str(&err);
            terminal.push_str(&notes);
            if i < last {
                piped = Some(out);
            } else {
                terminal.push_str(&out);
            }
        }
        terminal
    }

    /// Send output and errors where the redirections say. Returns what is
    /// left for the next command or the screen.
    fn route(&mut self, redirects: &[Redirect], out: String, err: String, terminal: &mut String) -> (String, String) {
        let mut out_dest = Dest::Terminal;
        let mut err_dest = Dest::Terminal;
        for redirect in redirects {
            match redirect {
                Redirect::Out { target, append } => out_dest = Dest::of(target, *append),
                Redirect::Err { target, append } => err_dest = Dest::of(target, *append),
                Redirect::ErrToOut => {
                    err_dest = match &out_dest {
                        Dest::Terminal => Dest::Terminal,
                        Dest::Null => Dest::Null,
                        Dest::File { path, append } => Dest::File { path: path.clone(), append: *append },
                    }
                }
                Redirect::In(_) => {}
            }
        }
        let same_file = matches!((&out_dest, &err_dest), (Dest::File { path: a, .. }, Dest::File { path: b, .. }) if a == b);
        let (mut keep_out, mut keep_err) = (String::new(), String::new());
        match out_dest {
            Dest::Terminal => keep_out = out.clone(),
            Dest::Null => {}
            Dest::File { ref path, append } => {
                let body = if same_file { format!("{out}{err}") } else { out.clone() };
                self.write_redirect(path, &body, append, terminal);
            }
        }
        match err_dest {
            Dest::Terminal => keep_err = err,
            Dest::Null => {}
            Dest::File { ref path, append } if !same_file => self.write_redirect(path, &err, append, terminal),
            Dest::File { .. } => {}
        }
        (keep_out, keep_err)
    }

    fn write_redirect(&mut self, path: &str, body: &str, append: bool, terminal: &mut String) {
        let Some(vfs) = self.virtual_fs.as_ref() else { return };
        match vfs.write_file(path, body, append) {
            Ok(()) => terminal.push_str(&format!(
                "{}{} {}\n",
                icons::success().content,
                if append { "Appended to" } else { "Wrote" },
                path
            )),
            Err(e) => terminal.push_str(&error(format!("{path}: {e}"))),
        }
    }

    /// Run one command of a pipeline
    fn run_sandbox_stage(&mut self, words: &[String], stdin: Option<String>) -> String {
        let program = words[0].as_str();
        let args = &words[1..];

        if KEYWORDS.contains(&program) {
            return error(format!(
                "{program}: the sandbox runs one command per line. if, for and while need a real shell (switch lessons to real files in settings)"
            ));
        }
        // A name with a slash in it is a path to a script, as in sh
        if matches!(program, "sh" | "bash") || program.contains('/') {
            return self.run_sandbox_script(words);
        }

        match program {
            "export" => return self.builtin_export(args),
            "unset" => {
                for name in args {
                    self.sandbox.vars.remove(name);
                    self.sandbox.exported.remove(name);
                }
                return String::new();
            }
            "env" | "printenv" if args.is_empty() => {
                return self
                    .sandbox
                    .exported
                    .iter()
                    .filter_map(|k| self.sandbox.vars.get(k).map(|v| format!("{k}={v}\n")))
                    .collect();
            }
            "printenv" => {
                return args
                    .iter()
                    .filter_map(|k| {
                        self.sandbox
                            .exported
                            .contains(k)
                            .then(|| self.sandbox.vars.get(k))
                            .flatten()
                            .map(|v| format!("{v}\n"))
                    })
                    .collect();
            }
            "which" => return self.builtin_which(args, false),
            "type" => return self.builtin_which(args, true),
            "exit" => {
                if self.sandbox.depth > 0 {
                    self.sandbox.exiting = true;
                    return String::new();
                }
                return format!("{}exit only ends scripts here; to leave a lesson press Ctrl+L\n", icons::hint().content);
            }
            "tee" => {
                let Some(text) = stdin else {
                    return error("tee needs something piped into it, like: ls | tee list.txt");
                };
                let append = args.iter().any(|a| a == "-a");
                let mut notes = String::new();
                for file in args.iter().filter(|a| !a.starts_with('-')) {
                    if let Some(Err(e)) = self.virtual_fs.as_ref().map(|v| v.write_file(file, &text, append)) {
                        notes.push_str(&error(format!("tee: {file}: {e}")));
                    }
                }
                return format!("{text}{notes}");
            }
            _ => {}
        }

        // Text from a pipe or `<` goes to filters that weren't given a file
        if let Some(text) = &stdin {
            let ops = text_filter::operands(program, args);
            let has_file = match program {
                "grep" | "sed" => ops.len() > 1,
                "tr" => false,
                _ => !ops.is_empty(),
            };
            if !has_file {
                if program == "cat" {
                    return text.clone();
                }
                if text_filter::FILTERS.contains(&program) {
                    return match text_filter::apply(program, args, text) {
                        Ok(out) => out,
                        Err(message) => error(message),
                    };
                }
            }
        }

        match program {
            "sed" => return self.sandbox_sed(args),
            "tr" => return error("tr reads from a pipe, like: cat names.txt | tr a-z A-Z"),
            "find" => return self.sandbox_find(args),
            "du" => return self.sandbox_du(args),
            "diff" => return self.sandbox_diff(args),
            "tar" => return self.sandbox_tar(args),
            "gzip" | "gunzip" => return self.sandbox_gzip(program, args),
            "ssh-keygen" => return self.sandbox_ssh_keygen(args),
            _ => {}
        }

        // A program found on PATH inside the practice files runs as a script
        if !program.contains('/') && !PROGRAMS.contains(&program) && !BUILTINS.contains(&program) {
            if let Some(path) = self.find_on_path(program) {
                let mut script = vec![path];
                script.extend(args.iter().cloned());
                return self.run_sandbox_script(&script);
            }
        }

        let line = shellwords::join(&words.iter().map(String::as_str).collect::<Vec<_>>());
        let cmd = match self.analyzer.parse(&line) {
            Ok(cmd) => cmd,
            Err(e) => return error(e),
        };
        self.execute_virtual_fs_command(&cmd)
            .unwrap_or_else(|| error(format!("{program}: not available in the sandbox")))
    }

    fn builtin_export(&mut self, args: &[String]) -> String {
        if args.is_empty() {
            return self
                .sandbox
                .exported
                .iter()
                .filter_map(|k| self.sandbox.vars.get(k).map(|v| format!("export {k}=\"{v}\"\n")))
                .collect();
        }
        let mut out = String::new();
        for arg in args {
            let (name, value) = match arg.split_once('=') {
                Some((name, value)) => (name, Some(value)),
                None => (arg.as_str(), None),
            };
            if !shell_syntax::is_name(name) {
                out.push_str(&error(format!("export: '{arg}': not a valid name")));
                continue;
            }
            if let Some(value) = value {
                self.sandbox.set(name, value);
            }
            self.sandbox.exported.insert(name.to_string());
        }
        out
    }

    /// Folders on PATH that live in the practice files, in order
    fn find_on_path(&self, program: &str) -> Option<String> {
        let vfs = self.virtual_fs.as_ref()?;
        let path = self.sandbox.vars.get("PATH")?;
        path.split(':')
            .filter(|dir| dir.starts_with("/lesson-home"))
            .map(|dir| format!("{}/{}", dir.trim_end_matches('/'), program))
            .find(|candidate| {
                vfs.resolve_path(candidate).ok().is_some_and(|real| {
                    #[cfg(unix)]
                    {
                        use std::os::unix::fs::PermissionsExt;
                        real.is_file()
                            && std::fs::metadata(&real).map(|m| m.permissions().mode() & 0o111 != 0).unwrap_or(false)
                    }
                    #[cfg(not(unix))]
                    {
                        real.is_file()
                    }
                })
            })
    }

    fn builtin_which(&self, args: &[String], verbose: bool) -> String {
        let mut out = String::new();
        for name in args.iter().filter(|a| !a.starts_with('-')) {
            let found = if BUILTINS.contains(&name.as_str()) {
                if verbose {
                    out.push_str(&format!("{name} is a shell builtin\n"));
                    continue;
                }
                None
            } else if let Some(path) = self.find_on_path(name) {
                Some(path)
            } else if PROGRAMS.contains(&name.as_str()) {
                Some(format!("/usr/bin/{name}"))
            } else {
                None
            };
            match (found, verbose) {
                (Some(path), true) => out.push_str(&format!("{name} is {path}\n")),
                (Some(path), false) => out.push_str(&format!("{path}\n")),
                (None, _) => out.push_str(&error(format!("{name}: not found on PATH"))),
            }
        }
        out
    }

    fn run_sandbox_script(&mut self, words: &[String]) -> String {
        let program = words[0].as_str();
        let (name, direct, args) = if matches!(program, "sh" | "bash") {
            let rest: Vec<&String> = words[1..].iter().filter(|a| !a.starts_with('-')).collect();
            match rest.split_first() {
                Some((file, args)) => ((*file).clone(), false, args.iter().map(|a| (*a).clone()).collect()),
                None => return error(format!("{program}: which script? Try: {program} hello.sh")),
            }
        } else {
            (program.trim_start_matches("./").to_string(), true, words[1..].to_vec())
        };
        if self.sandbox.depth > 0 {
            return error("scripts can't start other scripts in the sandbox");
        }
        let Some(vfs) = self.virtual_fs.as_ref() else { return String::new() };
        let real = match vfs.resolve_path(&name) {
            Ok(real) => real,
            Err(e) => return error(format!("{name}: {e}")),
        };
        if direct {
            #[cfg(unix)]
            let executable = {
                use std::os::unix::fs::PermissionsExt;
                std::fs::metadata(&real).map(|m| m.permissions().mode() & 0o111 != 0).unwrap_or(false)
            };
            #[cfg(not(unix))]
            let executable = true;
            if real.is_file() && !executable {
                return format!(
                    "{}{}: permission denied\n\n{}A file must be marked executable before it can run: chmod +x {}\n",
                    icons::error().content,
                    program,
                    icons::hint().content,
                    name
                );
            }
        }
        let script = match std::fs::read_to_string(&real) {
            Ok(text) => text,
            Err(_) => return error(format!("{name}: no such script")),
        };

        // A script is a new shell: it sees exported variables only, and
        // what it sets doesn't come back
        let saved_args = std::mem::replace(&mut self.sandbox.args, args);
        let saved_vars = self.sandbox.vars.clone();
        let saved_exported = self.sandbox.exported.clone();
        let exported = &self.sandbox.exported;
        self.sandbox.vars.retain(|k, _| exported.contains(k));
        self.sandbox.depth += 1;
        let mut output = String::new();
        for line in script.lines().map(str::trim) {
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some(out) = self.run_sandbox_line(line) {
                output.push_str(&out);
            }
            if self.sandbox.exiting {
                break;
            }
        }
        self.sandbox.depth -= 1;
        self.sandbox.exiting = false;
        self.sandbox.args = saved_args;
        self.sandbox.vars = saved_vars;
        self.sandbox.exported = saved_exported;
        if output.is_empty() {
            output.push_str("(the script ran and printed nothing)\n");
        }
        output
    }

    fn sandbox_sed(&mut self, args: &[String]) -> String {
        let in_place = args.iter().any(|a| a == "-i");
        let ops = text_filter::operands("sed", args);
        let Some((_, files)) = ops.split_first() else {
            return error("sed needs a script, like: sed 's/old/new/' notes.txt");
        };
        if files.is_empty() {
            return error("sed needs a file here, or text piped in: cat notes.txt | sed 's/old/new/'");
        }
        let filter_args: Vec<String> = args.iter().filter(|a| *a != "-i" && !files.contains(a)).cloned().collect();
        let mut out = String::new();
        for file in files {
            let Some(vfs) = self.virtual_fs.as_ref() else { break };
            let text = match vfs.read_file(file) {
                Ok(text) => text,
                Err(_) => {
                    out.push_str(&error(format!("sed: {file}: No such file or directory")));
                    continue;
                }
            };
            match text_filter::apply("sed", &filter_args, &text) {
                Ok(changed) if in_place => {
                    if let Err(e) = vfs.write_file(file, &changed, false) {
                        out.push_str(&error(format!("sed: {file}: {e}")));
                    }
                }
                Ok(changed) => out.push_str(&changed),
                Err(message) => return error(message),
            }
        }
        out
    }

    fn sandbox_find(&self, args: &[String]) -> String {
        let (paths, query) = match arct_core::virtual_fs_tools::FindQuery::parse(args) {
            Ok(parsed) => parsed,
            Err(message) => return error(message),
        };
        let Some(vfs) = self.virtual_fs.as_ref() else { return String::new() };
        let mut out = String::new();
        for path in paths {
            match vfs.find(&path, &query) {
                Ok(found) => found.iter().for_each(|f| {
                    out.push_str(f);
                    out.push('\n');
                }),
                Err(e) => out.push_str(&error(format!("find: {e}"))),
            }
        }
        out
    }

    fn sandbox_du(&self, args: &[String]) -> String {
        let summarize = args.iter().any(|a| a.starts_with('-') && a.contains('s'));
        let human = args.iter().any(|a| a.starts_with('-') && a.contains('h'));
        let mut paths: Vec<String> = args.iter().filter(|a| !a.starts_with('-')).cloned().collect();
        if paths.is_empty() {
            paths.push(".".to_string());
        }
        let size = |bytes: u64| {
            if human {
                match bytes {
                    b if b < 1024 => format!("{b}B"),
                    b if b < 1024 * 1024 => format!("{:.1}K", b as f64 / 1024.0),
                    b => format!("{:.1}M", b as f64 / (1024.0 * 1024.0)),
                }
            } else {
                // Like du: kilobytes, rounded up
                bytes.div_ceil(1024).max(u64::from(bytes > 0)).to_string()
            }
        };
        let Some(vfs) = self.virtual_fs.as_ref() else { return String::new() };
        let mut out = String::new();
        for path in paths {
            match vfs.disk_usage(&path, summarize) {
                Ok(rows) => rows.iter().for_each(|(p, b)| out.push_str(&format!("{}\t{}\n", size(*b), p))),
                Err(e) => out.push_str(&error(format!("du: {e}"))),
            }
        }
        out
    }

    fn sandbox_diff(&self, args: &[String]) -> String {
        let files: Vec<&String> = args.iter().filter(|a| !a.starts_with('-')).collect();
        let [a, b] = files[..] else {
            return error("diff compares two files, like: diff old.txt new.txt");
        };
        let Some(vfs) = self.virtual_fs.as_ref() else { return String::new() };
        let read = |f: &str| vfs.read_file(f).map_err(|_| error(format!("diff: {f}: No such file or directory")));
        match (read(a), read(b)) {
            (Ok(x), Ok(y)) => text_filter::diff(&x, &y),
            (Err(e), _) | (_, Err(e)) => e,
        }
    }

    fn sandbox_tar(&self, args: &[String]) -> String {
        let (mut create, mut extract, mut list, mut gzip, mut verbose) = (false, false, false, false, false);
        let mut archive: Option<String> = None;
        let mut dest = ".".to_string();
        let mut inputs = Vec::new();
        let mut iter = args.iter().enumerate();
        while let Some((i, arg)) = iter.next() {
            let bundle = arg.strip_prefix('-').filter(|s| !s.is_empty()).or_else(|| {
                (i == 0 && arg.chars().all(|c| "cxtzvf".contains(c))).then_some(arg.as_str())
            });
            let Some(letters) = bundle else {
                inputs.push(arg.clone());
                continue;
            };
            if letters == "C" {
                match iter.next() {
                    Some((_, d)) => dest = d.clone(),
                    None => return error("tar: -C needs a folder"),
                }
                continue;
            }
            let letters: Vec<char> = letters.chars().collect();
            for (k, &letter) in letters.iter().enumerate() {
                // In -cfz, f takes the rest of the bundle ("z") as the
                // archive's name, as in real tar: f goes last
                if letter == 'f' && k + 1 < letters.len() {
                    archive = Some(letters[k + 1..].iter().collect());
                    break;
                }
                match letter {
                    'c' => create = true,
                    'x' => extract = true,
                    't' => list = true,
                    'z' => gzip = true,
                    'v' => verbose = true,
                    'f' => match iter.next() {
                        Some((_, name)) => archive = Some(name.clone()),
                        None => return error("tar: f needs the archive's name, like: tar -czf notes.tar.gz notes"),
                    },
                    other => return error(format!("tar: the {other} option isn't available in the sandbox (try c, x, t, z, v, f)")),
                }
            }
        }
        let Some(archive) = archive else {
            return error("tar: name the archive with f, like: tar -czf notes.tar.gz notes");
        };
        let Some(vfs) = self.virtual_fs.as_ref() else { return String::new() };
        let result = match (create, extract, list) {
            (true, false, false) => vfs.tar_create(&archive, &inputs, gzip).map(|names| (names, verbose)),
            (false, true, false) => vfs.tar_extract(&archive, &dest).map(|names| (names, verbose)),
            (false, false, true) => vfs.tar_list(&archive).map(|names| (names, true)),
            _ => return error("tar: choose one of c (create), x (extract) or t (list)"),
        };
        match result {
            Ok((names, show)) if show => names.iter().map(|n| format!("{n}\n")).collect(),
            Ok(_) => String::new(),
            Err(e) => error(e),
        }
    }

    fn sandbox_gzip(&self, program: &str, args: &[String]) -> String {
        let unzip = program == "gunzip" || args.iter().any(|a| a == "-d");
        let keep = args.iter().any(|a| a == "-k" || a == "--keep");
        let files: Vec<&String> = args.iter().filter(|a| !a.starts_with('-')).collect();
        if files.is_empty() {
            return error(format!("{program}: name a file, like: {program} notes.txt{}", if unzip { ".gz" } else { "" }));
        }
        let Some(vfs) = self.virtual_fs.as_ref() else { return String::new() };
        let mut out = String::new();
        for file in files {
            // -k keeps the original: copy it back after the swap
            let backup = keep.then(|| vfs.resolve_path(file).ok().and_then(|p| std::fs::read(p).ok())).flatten();
            let result = if unzip { vfs.gunzip(file) } else { vfs.gzip(file) };
            match result {
                Ok(_) => {
                    if let (Some(bytes), Ok(path)) = (backup, vfs.resolve_path(file)) {
                        let _ = std::fs::write(path, bytes);
                    }
                }
                Err(e) => out.push_str(&error(e)),
            }
        }
        out
    }

    fn sandbox_ssh_keygen(&self, args: &[String]) -> String {
        let mut kind = "ed25519".to_string();
        let mut file: Option<String> = None;
        let mut comment = "learner@arc-academy".to_string();
        let mut iter = args.iter();
        while let Some(arg) = iter.next() {
            match arg.as_str() {
                "-t" => kind = iter.next().cloned().unwrap_or_default(),
                "-f" => file = iter.next().cloned(),
                "-C" => comment = iter.next().cloned().unwrap_or_default(),
                "-N" => {
                    iter.next();
                }
                "-q" => {}
                other => return error(format!("ssh-keygen: {other} isn't available in the sandbox (try -t, -f, -C, -N)")),
            }
        }
        if !matches!(kind.as_str(), "ed25519" | "rsa") {
            return error("ssh-keygen: -t takes ed25519 (recommended) or rsa");
        }
        let file = file.unwrap_or_else(|| format!("/lesson-home/.ssh/id_{kind}"));
        let Some(vfs) = self.virtual_fs.as_ref() else { return String::new() };
        match vfs.practice_ssh_key(&file, &kind, &comment) {
            Ok((private, public)) => format!(
                "Generating public/private {kind} key pair.\n\
                 Your identification has been saved in {private}\n\
                 Your public key has been saved in {public}\n\
                 {}This is a practice key made in the sandbox. It opens nothing.\n",
                icons::hint().content
            ),
            Err(e) => error(format!("ssh-keygen: {e}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An app in lesson mode with a fresh sandbox (data kept in a temp dir)
    fn app(name: &str) -> App {
        let dir = std::env::temp_dir().join(format!("arct-sandbox-{}-{name}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::env::set_var("ARCT_DATA_DIR", &dir);
        let config = dir.join("config.toml");
        std::fs::write(&config, "[general]\nsetup_complete = true\n").unwrap();
        let mut app = App::new_with_options(crate::RunOptions {
            config_path: Some(config),
            ..Default::default()
        })
        .unwrap();
        app.lesson_mode = true;
        app.virtual_fs = Some(arct_core::VirtualFileSystem::new("sandbox-test", name).unwrap());
        app
    }

    fn run(app: &mut App, line: &str) -> String {
        app.run_sandbox_line(line).unwrap()
    }

    fn read(app: &App, path: &str) -> String {
        app.virtual_fs.as_ref().unwrap().read_file(path).unwrap()
    }

    #[test]
    fn variables_expand_when_each_command_runs() {
        let mut app = app("vars");
        assert_eq!(run(&mut app, "NAME=Ada; echo hi $NAME"), "hi Ada\n");
        assert_eq!(run(&mut app, "echo '$NAME' \"$NAME\""), "$NAME Ada\n");
        assert_eq!(run(&mut app, "echo $HOME"), "/lesson-home\n");
        // Not exported yet, so env doesn't show it
        assert!(!run(&mut app, "env").contains("NAME="));
        run(&mut app, "export NAME");
        assert!(run(&mut app, "env").contains("NAME=Ada"));
        assert_eq!(run(&mut app, "printenv NAME"), "Ada\n");
        run(&mut app, "unset NAME");
        assert_eq!(run(&mut app, "echo [$NAME]"), "[]\n");
        // A prefix assignment lasts for one command
        run(&mut app, "GREETING=hey env > env.txt");
        assert!(read(&app, "env.txt").contains("GREETING=hey"), "in that command's environment");
        assert_eq!(run(&mut app, "echo [$GREETING]"), "[]\n");
    }

    #[test]
    fn redirection_for_any_command() {
        let mut app = app("redirect");
        run(&mut app, "mkdir logs");
        run(&mut app, "echo one > logs/a.txt");
        run(&mut app, "echo two >> logs/a.txt");
        assert_eq!(read(&app, "logs/a.txt"), "one\ntwo\n");

        let out = run(&mut app, "ls logs > list.txt");
        assert!(out.contains("Wrote list.txt"));
        assert_eq!(read(&app, "list.txt").trim(), "a.txt");

        // Errors stay on screen unless sent elsewhere
        let out = run(&mut app, "cat nothing.txt > out.txt");
        assert!(App::output_reports_failure(&out));
        let out = run(&mut app, "cat nothing.txt 2> errors.txt");
        assert!(!App::output_reports_failure(&out), "{out}");
        assert!(read(&app, "errors.txt").contains("nothing.txt"));
        let out = run(&mut app, "cat nothing.txt 2>/dev/null");
        assert!(!out.contains("nothing.txt"), "{out}");
        run(&mut app, "cat logs/a.txt nothing.txt > both.txt 2>&1");
        let both = read(&app, "both.txt");
        assert!(both.contains("one") && both.contains("nothing.txt"), "{both}");

        // Input from a file
        assert_eq!(run(&mut app, "wc -l < logs/a.txt"), "2\n");
        assert_eq!(run(&mut app, "sort -r < logs/a.txt"), "two\none\n");
        // A quoted > is just text
        assert_eq!(run(&mut app, "echo '>' x"), "> x\n");
    }

    #[test]
    fn lists_and_status() {
        let mut app = app("lists");
        run(&mut app, "mkdir camp && cd camp && touch tent.txt");
        assert!(app.virtual_fs.as_ref().unwrap().get_current_dir().ends_with("camp"));
        let out = run(&mut app, "cat missing.txt && echo never");
        assert!(!out.contains("never"));
        assert_eq!(run(&mut app, "echo $?"), "1\n");
        assert!(run(&mut app, "cat missing.txt || echo fallback").contains("fallback"));
        assert_eq!(run(&mut app, "echo $?"), "0\n");
        assert!(App::output_reports_failure(&run(&mut app, "ls &&")));
    }

    #[test]
    fn pipes_tee_sed_tr() {
        let mut app = app("pipes");
        run(&mut app, "echo 'b apple' > f.txt; echo 'a pear' >> f.txt");
        assert_eq!(run(&mut app, "cat f.txt | sort | tr a-z A-Z"), "A PEAR\nB APPLE\n");
        assert_eq!(run(&mut app, "cat f.txt | tee copy.txt | wc -l"), "2\n");
        assert_eq!(read(&app, "copy.txt"), "b apple\na pear\n");
        assert_eq!(run(&mut app, "sed 's/apple/plum/' f.txt"), "b plum\na pear\n");
        assert_eq!(run(&mut app, "tail -n +2 f.txt"), "a pear\n");
        assert_eq!(run(&mut app, "cat f.txt | tail -n +2"), "a pear\n");
        run(&mut app, "sed -i 's/pear/fig/' f.txt");
        assert_eq!(read(&app, "f.txt"), "b apple\na fig\n");
        assert_eq!(run(&mut app, "grep fig < f.txt"), "a fig\n");
        // Any command can follow a pipe (it just doesn't read the text)
        assert_eq!(run(&mut app, "cat f.txt | echo hi"), "hi\n");
    }

    #[test]
    fn globs_expand_against_the_sandbox() {
        let mut app = app("globs");
        run(&mut app, "mkdir notes; touch notes/a.txt notes/b.txt notes/c.md");
        assert_eq!(run(&mut app, "ls notes/*.txt"), run(&mut app, "ls notes/a.txt notes/b.txt"));
        run(&mut app, "rm notes/*.txt");
        assert_eq!(run(&mut app, "find notes -type f"), "notes/c.md\n");
        // Quoted, find gets the pattern itself
        run(&mut app, "touch notes/d.md");
        assert_eq!(run(&mut app, "find notes -name '*.md'").lines().count(), 2);
        // No match: the word stays as typed (and the command says so)
        assert!(App::output_reports_failure(&run(&mut app, "cat *.nothing")));
    }

    #[test]
    fn scripts_take_arguments_and_path_finds_them() {
        let mut app = app("scripts");
        run(&mut app, "mkdir bin");
        run(&mut app, "echo 'echo Hello, $1! You gave me $# words.' > bin/greet");
        assert_eq!(run(&mut app, "sh bin/greet Ada Bo"), "Hello, Ada! You gave me 2 words.\n");
        // Not on PATH yet
        assert!(App::output_reports_failure(&run(&mut app, "greet Ada")));
        run(&mut app, "chmod +x bin/greet");
        run(&mut app, "export PATH=$HOME/bin:$PATH");
        assert_eq!(run(&mut app, "greet Ada"), "Hello, Ada! You gave me 1 words.\n");
        assert_eq!(run(&mut app, "which greet"), "/lesson-home/bin/greet\n");
        assert_eq!(run(&mut app, "type cd"), "cd is a shell builtin\n");

        // exit stops a script; if/for say they need a real shell
        run(&mut app, "echo 'echo one' > s.sh; echo 'exit' >> s.sh; echo 'echo two' >> s.sh");
        assert_eq!(run(&mut app, "sh s.sh"), "one\n");
        assert!(run(&mut app, "for x in a b").contains("real shell"));
    }

    #[test]
    fn shell_fidelity() {
        let mut app = app("fidelity");
        // NAME=value cmd: in that command's environment only
        assert!(run(&mut app, "GREETING=hey env").contains("GREETING=hey"));
        assert!(!run(&mut app, "env").contains("GREETING"));
        // Scripts see exported variables only, and can't change ours
        run(&mut app, "echo 'echo [$SECRET] [$SHARED]; SHARED=changed' > s.sh");
        run(&mut app, "SECRET=x; export SHARED=y");
        assert_eq!(run(&mut app, "sh s.sh"), "[] [y]\n");
        assert_eq!(run(&mut app, "echo $SHARED"), "y\n");
        // Word splitting and empty values
        run(&mut app, "FILES='a.txt b.txt'; touch $FILES");
        assert!(app.virtual_fs.as_ref().unwrap().resolve_path("b.txt").unwrap().exists());
        // Hidden errors still set $?
        assert_eq!(run(&mut app, "cat nope 2>/dev/null; echo $?"), "1\n");
        // > empties its file before the command runs
        run(&mut app, "echo b > x; echo a >> x; sort x > x");
        assert_eq!(read(&app, "x"), "");
        // Several files into a folder, never over each other
        run(&mut app, "mkdir box; touch c d");
        run(&mut app, "mv c d box");
        assert!(app.virtual_fs.as_ref().unwrap().resolve_path("box/d").unwrap().exists());
        assert!(App::output_reports_failure(&run(&mut app, "cp a.txt b.txt x")));
        // mkdir -p is fine when the folder exists; ls -a shows . and ..
        assert!(!App::output_reports_failure(&run(&mut app, "mkdir -p box")));
        assert!(run(&mut app, "ls -a box").starts_with("./\n../\n"));
        assert!(!run(&mut app, "ls -A box").contains("./"));
        // ls -l shows real modes
        run(&mut app, "chmod 600 box/d");
        assert!(run(&mut app, "ls -l box").contains("-rw-------"));
    }

    #[test]
    fn guard_checks_the_expanded_command() {
        let mut app = app("guard");
        run(&mut app, "touch keep.txt");
        let out = run(&mut app, "X=/; rm -rf $X");
        assert!(out.contains("Blocked"), "{out}");
        assert!(app.virtual_fs.as_ref().unwrap().resolve_path("keep.txt").unwrap().exists());
    }

    #[test]
    fn real_mode_keeps_variables_between_commands() {
        let mut shell = SandboxShell::new();
        assert_eq!(shell.real_assignments("GREETING=hello", "/p"), Some(String::new()));
        assert_eq!(shell.real_assignments("export WHO=\"$GREETING world\"", "/p"), Some(String::new()));
        let env: BTreeMap<String, String> = shell.real_env().collect();
        assert_eq!(env["WHO"], "hello world");
        assert_eq!(shell.real_assignments("export PATH=/x/bin:$PATH", "/p").as_deref(), Some(""));
        assert!(shell.real.get("PATH").unwrap().starts_with("/x/bin:"));
        assert_eq!(shell.real_assignments("unset GREETING", "/p"), Some(String::new()));
        assert!(!shell.real.contains_key("GREETING"));
        // Anything else runs for real
        assert_eq!(shell.real_assignments("echo $WHO", "/p"), None);
        assert_eq!(shell.real_assignments("X=1 env", "/p"), None);
        assert_eq!(shell.real_assignments("export", "/p"), None);
    }

    #[test]
    fn real_mode_expands_dollar_before_the_guard() {
        let mut shell = SandboxShell::new();
        let pwd = "/home/u/ArcAcademy/playground/variables/tour";
        let root = Path::new("/home/u/ArcAcademy/playground");
        let expand = |shell: &SandboxShell, line: &str| shell.expand_for_real(line, Path::new(pwd), root);
        // Every line is rendered; substitution is left to the guard to refuse
        assert_eq!(expand(&shell, "ls -l").unwrap().as_deref(), Some("ls -l"));
        assert_eq!(expand(&shell, "for f in a b; do echo hi; done").unwrap().as_deref(), Some("for f in a b; do echo hi; done"));
        assert_eq!(expand(&shell, "echo $(date)"), Ok(None));
        assert_eq!(expand(&shell, "echo `date`"), Ok(None));

        shell.real.insert("VENUE".into(), "The Lantern Hall".into());
        assert_eq!(
            expand(&shell, "echo \"Tonight: $VENUE\"").unwrap().as_deref(),
            Some("echo 'Tonight: The Lantern Hall'")
        );
        // later parts see what earlier parts set, as in sh
        assert_eq!(
            expand(&shell, "BAND=Nightjar; echo $BAND && unset BAND; echo \"[$BAND]\"").unwrap().as_deref(),
            Some("BAND=Nightjar; echo Nightjar && unset BAND; echo '[]'")
        );
        assert_eq!(expand(&shell, "echo $PWD").unwrap().as_deref(), Some(&*format!("echo {pwd}")));
        assert_eq!(expand(&shell, "false; echo $?").unwrap().as_deref(), Some("false; echo \"$?\""));
        // script text in single quotes is kept as typed
        assert_eq!(
            expand(&shell, "echo 'echo \"$1: $2\"' > log.sh").unwrap().as_deref(),
            Some("echo 'echo \"$1: $2\"' > log.sh")
        );
        // loops and if fill in $ as they run: refused, with a reason
        for line in ["for x in a b; do echo $x; done", "if true; then echo $VENUE; fi", "while read l; do echo $l; done < f"] {
            assert!(expand(&shell, line).unwrap_err().contains("can't be checked"), "{line}");
        }
    }

    #[test]
    fn real_mode_guard_sees_what_variables_hold() {
        let root = std::path::Path::new("/home/u/ArcAcademy/playground");
        let cwd = root.join("variables");
        let guard = arct_core::PlaygroundGuard::new(root);
        let mut shell = SandboxShell::new();
        let checked = |shell: &SandboxShell, line: &str| {
            let line = shell.expand_for_real(line, &cwd, root).unwrap().unwrap();
            guard.check_expanded(&line, &cwd).is_allowed()
        };
        shell.real.insert("X".into(), "/".into());
        shell.real.insert("UP".into(), "../..".into());
        shell.real.insert("D".into(), "/dev/disk2".into());
        shell.real.insert("SEP".into(), ";".into());
        assert!(!checked(&shell, "rm -rf $X"));
        assert!(!checked(&shell, "cat $UP/secret"));
        assert!(!checked(&shell, "dd if=a of=$D"));
        assert!(!checked(&shell, "echo hi > $D"));
        assert!(!checked(&shell, "cat $HOME/notes"));
        assert!(!checked(&shell, "cat ~/notes $X"));
        assert!(!checked(&shell, "Y=/etc; cat $Y/hosts"));
        // a value is data, never a second command
        assert!(checked(&shell, "echo $SEP rm -rf"));
        assert!(checked(&shell, "Y=tour; ls $Y && echo \"$Y done\""));
        assert!(checked(&shell, "cd $PWD"));
    }

    #[tokio::test]
    async fn real_files_lesson_uses_variables_end_to_end() {
        let mut app = app("realfiles");
        let root = std::env::temp_dir().join(format!("arct-realfiles-{}", std::process::id()));
        let mut playground = arct_core::Playground::open(root.clone()).unwrap();
        let lesson = playground.enter_lesson("variables", &[]).unwrap();
        std::fs::create_dir_all(lesson.join("tour")).unwrap();
        app.playground = Some(playground);
        let real = async |app: &mut App, line: &str| {
            let cmd = app.analyzer.parse(line).unwrap();
            app.execute_playground_command(&cmd, line).await
        };

        assert_eq!(real(&mut app, "LIST=\"tour/encore songs.txt\"").await, "");
        real(&mut app, "echo \"Starlight Road\" > \"$LIST\"").await;
        assert_eq!(
            std::fs::read_to_string(lesson.join("tour/encore songs.txt")).unwrap(),
            "Starlight Road\n"
        );
        assert!(real(&mut app, "DAY=friday; echo \"Load in\" > tour/${DAY}_notes.txt; cat tour/$DAY\"_notes.txt\"")
            .await
            .contains("Load in"));
        real(&mut app, "cd tour").await;
        assert!(real(&mut app, "echo $PWD").await.contains("tour"));
        // the guard sees what the variable holds. This test reaches a real
        // shell, so it only ever probes with harmless, read-only commands:
        // if the guard regresses, ls runs, nothing else (destructive cases
        // are in real_mode_guard_sees_what_variables_hold, which runs nothing)
        real(&mut app, "X=/").await;
        let out = real(&mut app, "ls -d $X").await;
        assert!(out.contains("points outside"), "{out}");
        let out = real(&mut app, "ls -d $HOME/arct-no-such-probe").await;
        assert!(out.contains("points outside"), "{out}");
        let out = real(&mut app, "for f in *; do echo $f; done").await;
        assert!(out.contains("can't be checked"), "{out}");
        // globs come from the app, never from sh: no route to .. (read-only probes)
        real(&mut app, "cd ..").await;
        std::fs::write(lesson.join(".hidden"), "x").unwrap();
        let out = real(&mut app, "ls -d .* .?").await;
        assert!(out.contains(".hidden") && !out.contains("..\n") && !out.contains(".. "), "{out}");
        let out = real(&mut app, "ls .[.]/.[.]").await;
        assert!(out.contains("No such file"), "{out}");
        // and the OS jail holds where the guard can't see: a script that
        // climbs out (probe: an empty file in the temp folder)
        assert!(matches!(app.real_jail, Some(Some(_))), "real files run jailed");
        let escape = root.parent().unwrap().join(format!("arct-escape-probe-{}", std::process::id()));
        std::fs::write(lesson.join("x.sh"), format!("cd ..; cd ..; touch '{}'\n", escape.display())).unwrap();
        real(&mut app, "sh x.sh").await;
        assert!(!escape.exists(), "the script wrote outside the playground");

        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn network_lessons_stay_simulated_with_real_files_on() {
        let lessons = arct_core::LessonLibrary::new();
        let network = lessons.get("network-basics").expect("lesson exists");
        assert!(network.simulated_only);
        assert!(!lessons.get("variables").unwrap().simulated_only);
    }

    #[test]
    fn real_files_practice_needs_a_jail() {
        let mut app = app("nojail");
        app.real_jail = Some(None);
        app.toggle_practice_mode();
        assert_eq!(app.config.lessons.practice_mode, "simulated");
        assert!(app.last_output.contains("simulated sandbox"), "{}", app.last_output);
    }

    #[test]
    fn real_mode_globs_never_reach_dot_or_dotdot() {
        let root = std::env::temp_dir().join(format!("arct-globs-{}", std::process::id()));
        let lesson = root.join("lesson");
        std::fs::create_dir_all(lesson.join("sub")).unwrap();
        for f in ["a.txt", "b.txt", ".hidden"] {
            std::fs::write(lesson.join(f), "x").unwrap();
        }
        let shell = SandboxShell::new();
        let guard = arct_core::PlaygroundGuard::new(&root);
        let at = |cwd: &Path, line: &str| shell.expand_for_real(line, cwd, &root).unwrap().unwrap();
        let allowed = |cwd: &Path, line: &str| guard.check_expanded(&at(cwd, line), cwd).is_allowed();

        // bash 3.2 would turn these into .. and ../..; here they match
        // nothing (stay as typed, quoted) or only real hidden files
        assert_eq!(at(&lesson, "ls .[.]/.[.]"), "ls '.[.]/.[.]'");
        assert_eq!(at(&lesson, "ls -d .?"), "ls -d '.?'");
        assert_eq!(at(&lesson, "ls -d .*"), "ls -d .hidden");
        assert_eq!(at(&root, "chmod -R 000 .*"), "chmod -R 000 '.*'");
        assert_eq!(at(&lesson, "ls *.txt s*/"), "ls a.txt b.txt sub/");
        assert_eq!(at(&lesson, "cat 'a*'.txt"), "cat 'a*.txt'");
        // a .. typed out is still visible to the guard
        assert_eq!(at(&lesson.join("sub"), "ls ../*.txt"), "ls ../a.txt ../b.txt");
        assert!(allowed(&lesson.join("sub"), "ls ../*.txt"));
        // nothing outside the playground is listed, and the guard refuses it as typed
        assert_eq!(at(&root, "ls ../*"), "ls '../*'");
        assert!(!allowed(&root, "ls ../*"));
        assert_eq!(at(&lesson, "ls /etc/*"), "ls '/etc/*'");
        assert!(!allowed(&lesson, "ls /etc/*"));
        assert!(!allowed(&lesson, "ls ~/*"));
        // a symlink out of the playground isn't followed
        let outside = root.parent().unwrap().join(format!("arct-globs-outside-{}", std::process::id()));
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(outside.join("secret.txt"), "x").unwrap();
        std::os::unix::fs::symlink(&outside, lesson.join("x")).unwrap();
        assert_eq!(at(&lesson, "ls x/*"), "ls 'x/*'");
        assert_eq!(at(&lesson, "ls */secret.txt"), "ls '*/secret.txt'");
        std::fs::remove_dir_all(&outside).unwrap();

        std::fs::remove_dir_all(&root).unwrap();
    }

    #[tokio::test]
    async fn safe_free_shell_expands_and_globs_in_the_app() {
        // This reaches a real shell: read-only probes only (negjr11u)
        let mut app = app("freeshell");
        app.lesson_mode = false;
        app.config.shell.safe_mode = true;
        let root = std::env::temp_dir().join(format!("arct-freeshell-{}", std::process::id()));
        std::fs::create_dir_all(root.join("notes")).unwrap();
        std::fs::write(root.join(".hidden"), "x").unwrap();
        std::fs::write(root.join("notes/a.txt"), "alpha\n").unwrap();
        app.explore = Some(arct_core::Playground::open_with_display(root.clone(), "~/ArcAcademy/home").unwrap());
        app.environment_vars.insert("ARCT_TEST_WORD".into(), "alpha".into());
        let shell = async |app: &mut App, line: &str| {
            app.command_buffer = line.to_string();
            app.execute_command().await.unwrap();
            app.last_output.clone()
        };

        let out = shell(&mut app, "ls -d .* .?").await;
        assert!(out.contains(".hidden") && !out.contains("..\n") && !out.contains(".. "), "{out}");
        let out = shell(&mut app, "ls .[.]/.[.]").await;
        assert!(out.contains("No such file"), "{out}");
        assert!(shell(&mut app, "grep $ARCT_TEST_WORD notes/*.txt").await.contains("alpha"));
        assert!(shell(&mut app, "W=alpha; echo \"[$W]\"").await.contains("[alpha]"));
        assert!(shell(&mut app, "ls -d $HOME").await.contains("Blocked"));
        assert!(shell(&mut app, "ls -d ../*").await.contains("Blocked"));
        assert!(shell(&mut app, "echo $(date)").await.contains("Blocked"));
        assert!(shell(&mut app, "(ls)").await.contains("Blocked"));
        // the free shell runs jailed too (probe: an empty file in the temp folder)
        let escape = root.parent().unwrap().join(format!("arct-free-escape-probe-{}", std::process::id()));
        std::fs::write(root.join("x.sh"), format!("cd ..; touch '{}'\n", escape.display())).unwrap();
        shell(&mut app, "sh x.sh").await;
        assert!(!escape.exists(), "the free shell wrote outside its home");
        // what chmod -R 000 .* would have run here (not executed)
        let vars = |_: &str| None;
        assert_eq!(
            expand_real_line("chmod -R 000 .*", &root, &root, &vars).unwrap().as_deref(),
            Some("chmod -R 000 .hidden")
        );
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn real_mode_runs_exactly_the_expanded_line() {
        let dir = std::env::temp_dir().join(format!("arct-real-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut shell = SandboxShell::new();
        shell.real.insert("LIST".into(), "encore songs.txt".into());
        shell.real.insert("TRICK".into(), "$(touch pwned)".into());
        let line = shell
            .expand_for_real("echo \"Starlight Road\" > \"$LIST\"; echo $TRICK; X=5; echo \"$X\"", &dir, &dir)
            .unwrap()
            .unwrap();
        let out = std::process::Command::new("sh")
            .arg("-c")
            .arg(&line)
            .current_dir(&dir)
            .output()
            .unwrap();
        assert_eq!(String::from_utf8_lossy(&out.stdout), "$(touch pwned)\n5\n");
        assert_eq!(std::fs::read_to_string(dir.join("encore songs.txt")).unwrap(), "Starlight Road\n");
        assert!(!dir.join("pwned").exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn archives_keys_du_diff() {
        let mut app = app("tools");
        run(&mut app, "mkdir -p proj/src; echo hi > proj/src/a.txt");
        run(&mut app, "tar -czf proj.tar.gz proj");
        assert!(run(&mut app, "tar -tzf proj.tar.gz").contains("proj/src/a.txt"));
        run(&mut app, "mkdir out; tar -xzf proj.tar.gz -C out");
        assert_eq!(read(&app, "out/proj/src/a.txt"), "hi\n");
        run(&mut app, "tar czf two.tgz proj/src");
        assert!(run(&mut app, "tar tf two.tgz").contains("proj/src/a.txt"));

        let out = run(&mut app, "ssh-keygen -t ed25519 -C me@laptop -N ''");
        assert!(out.contains(".ssh/id_ed25519.pub"), "{out}");
        assert!(read(&app, ".ssh/id_ed25519.pub").ends_with("me@laptop\n"));

        assert!(run(&mut app, "du -sh proj").ends_with("\tproj\n"));
        run(&mut app, "echo hi > a; echo ho > b");
        assert_eq!(run(&mut app, "diff a b"), "1c1\n< hi\n---\n> ho\n");
        // tar's f takes the rest of a bundle as the name; gzip -k keeps
        run(&mut app, "tar -cfz proj");
        assert!(app.virtual_fs.as_ref().unwrap().resolve_path("z").unwrap().exists());
        run(&mut app, "gzip -k a");
        assert_eq!(read(&app, "a"), "hi\n");
        assert!(app.virtual_fs.as_ref().unwrap().resolve_path("a.gz").unwrap().exists());
        // diff exits 1 when the files differ, grep when nothing matched
        assert!(!run(&mut app, "diff a b > /dev/null && echo same").contains("same"));
        assert!(run(&mut app, "cp a c; diff a c && echo same").contains("same"));
        assert!(run(&mut app, "grep zzz a || echo none").contains("none"));
        assert!(run(&mut app, "cat a | grep zzz || echo none").contains("none"));
        assert_eq!(run(&mut app, "grep hi a > /dev/null; echo $?"), "0\n");
    }
}
