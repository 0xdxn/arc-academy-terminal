//! The simulated sandbox's shell, with no user interface: what happens to
//! a typed line before and around each command. Variables, globs,
//! redirection, pipes, `&&` / `||` / `;`, scripts with arguments, and
//! `PATH` lookups, all inside the lesson's practice files
//! ([`VirtualFileSystem`], on disk or in memory). The file and text
//! commands themselves are in `sandbox_commands.rs`.
//!
//! The terminal app and the browser both run lessons through this. A
//! [`Sandbox`] borrows the state it works on (the shell's variables, the
//! practice files) for one line at a time, so the host keeps owning them.

use crate::command::CommandAnalyzer;
use crate::guard::{GuardVerdict, PlaygroundGuard};
use crate::shell_syntax::{self, Joiner, Pipeline, Redirect, Word};
use crate::text_filter;
use crate::virtual_fs::VirtualFileSystem;
use std::collections::{BTreeMap, BTreeSet};

/// The marks at the start of message lines: errors, warnings, hints and
/// confirmations, as opposed to a command's own output. The host picks
/// them (the terminal app can use Nerd Font glyphs); each includes the
/// space after it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Marks {
    pub success: String,
    pub error: String,
    pub warning: String,
    pub info: String,
    pub hint: String,
    pub folder: String,
    pub file: String,
}

impl Marks {
    /// Plain one-cell symbols, the terminal app's default
    pub fn plain() -> Self {
        Self {
            success: "✓  ".to_string(),
            error: "✗  ".to_string(),
            warning: "!  ".to_string(),
            info: "i  ".to_string(),
            hint: "*  ".to_string(),
            folder: "▸  ".to_string(),
            file: "·  ".to_string(),
        }
    }

    /// Whether output reports a failure: an error line, a guard refusal,
    /// or a timeout
    pub fn reports_failure(&self, output: &str) -> bool {
        output.lines().any(|line| {
            line.starts_with(self.error.as_str())
                || line.starts_with(self.warning.as_str())
                || line.contains("timed out")
        })
    }

    /// Lines that are messages about the command (errors, warnings, hints,
    /// confirmations) rather than its output
    fn is_message(&self, line: &str) -> bool {
        [&self.error, &self.warning, &self.hint, &self.success]
            .iter()
            .any(|mark| line.starts_with(mark.as_str()))
    }

    fn is_error(&self, line: &str) -> bool {
        line.starts_with(self.error.as_str()) || line.starts_with(self.warning.as_str())
    }

    /// Split a command's printout into (output, errors, notes)
    fn split_streams(&self, text: &str) -> (String, String, String) {
        let (mut out, mut err, mut notes) = (String::new(), String::new(), String::new());
        for line in text.split_inclusive('\n') {
            if self.is_error(line) {
                err.push_str(line);
            } else if self.is_message(line) {
                notes.push_str(line);
            } else {
                out.push_str(line);
            }
        }
        (out, err, notes)
    }

    /// An error line
    pub fn error(&self, message: impl std::fmt::Display) -> String {
        format!("{}{}\n", self.error, message)
    }
}

impl Default for Marks {
    fn default() -> Self {
        Self::plain()
    }
}

/// The sandbox shell's state: variables, script arguments, last status
#[derive(Debug, Clone)]
pub struct ShellState {
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
    /// Exit status a command chose that its printout can't show: diff
    /// finding differences, grep finding nothing (both exit 1 in sh)
    pending_status: Option<i32>,
}

impl Default for ShellState {
    fn default() -> Self {
        Self::new()
    }
}

impl ShellState {
    pub fn new() -> Self {
        let mut shell = Self {
            vars: BTreeMap::new(),
            exported: BTreeSet::new(),
            args: Vec::new(),
            status: 0,
            depth: 0,
            exiting: false,
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

    /// `$?`: the last command's status
    pub fn status(&self) -> i32 {
        self.status
    }

    /// A shell variable's value
    pub fn var(&self, name: &str) -> Option<&str> {
        self.vars.get(name).map(String::as_str)
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
}

/// Commands the sandbox runs, for `which` and `type`
pub const PROGRAMS: &[&str] = &[
    "ls", "cat", "head", "tail", "wc", "grep", "sort", "uniq", "cut", "sed", "tr", "mkdir",
    "touch", "rm", "mv", "cp", "chmod", "find", "du", "tar", "gzip", "gunzip", "diff", "tee",
    "env", "printenv", "which", "sh", "bash", "ssh-keygen",
];
/// Builtins of the sandbox shell
pub const BUILTINS: &[&str] = &["cd", "echo", "pwd", "export", "unset", "type", "exit"];
/// Shell keywords (the sandbox runs one command per line, so none of these)
pub const KEYWORDS: &[&str] = &["if", "then", "else", "elif", "fi", "for", "while", "until", "do", "done", "case", "esac", "function"];

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

/// The simulated shell at work on one host's state: run typed lines with
/// [`Sandbox::run_line`]
pub struct Sandbox<'a> {
    pub(crate) shell: &'a mut ShellState,
    pub(crate) vfs: &'a mut VirtualFileSystem,
    pub(crate) analyzer: &'a CommandAnalyzer,
    pub(crate) marks: &'a Marks,
    guard_refusals: usize,
}

impl<'a> Sandbox<'a> {
    pub fn new(
        shell: &'a mut ShellState,
        vfs: &'a mut VirtualFileSystem,
        analyzer: &'a CommandAnalyzer,
        marks: &'a Marks,
    ) -> Self {
        Self {
            shell,
            vfs,
            analyzer,
            marks,
            guard_refusals: 0,
        }
    }

    /// How many commands the safety guard refused while this sandbox ran
    pub fn guard_refusals(&self) -> usize {
        self.guard_refusals
    }

    pub(crate) fn error(&self, message: impl std::fmt::Display) -> String {
        self.marks.error(message)
    }

    /// Run one typed line: everything it printed, errors included
    pub fn run_line(&mut self, line: &str) -> String {
        let mut output = String::new();
        for (joiner, part) in shell_syntax::split_list(line) {
            if self.shell.exiting {
                break;
            }
            let run = match joiner {
                Joiner::Always => true,
                Joiner::IfOk => self.shell.status == 0,
                Joiner::IfFailed => self.shell.status != 0,
            };
            let pwd = self.vfs.get_current_dir().display().to_string();
            let shell = &self.shell;
            let parsed = shell_syntax::parse(&part, &|name| shell.lookup(name, &pwd));
            let pipeline = match parsed {
                Ok(list) if list.is_empty() && joiner != Joiner::Always => {
                    output.push_str(&self.error("syntax error: a command is missing after && or ||"));
                    self.shell.status = 2;
                    break;
                }
                Ok(list) if list.is_empty() => continue,
                Ok(mut list) => list.remove(0).1,
                Err(message) => {
                    output.push_str(&self.error(message));
                    self.shell.status = 2;
                    break;
                }
            };
            if !run {
                continue;
            }
            self.shell.pending_status = None;
            let text = self.run_pipeline(&pipeline);
            let pending = self.shell.pending_status.take();
            self.shell.status = if self.marks.reports_failure(&text) { 1 } else { pending.unwrap_or(0) };
            output.push_str(&text);
        }
        output
    }

    fn expand_globs(&self, words: &[Word]) -> Vec<String> {
        let mut out = Vec::new();
        for word in words {
            let matches = if word.glob { self.vfs.glob(&word.text) } else { Vec::new() };
            if matches.is_empty() {
                out.push(word.text.clone());
            } else {
                out.extend(matches);
            }
        }
        out
    }

    fn run_pipeline(&mut self, pipeline: &Pipeline) -> String {
        let mut terminal = String::new();
        let mut piped: Option<String> = None;
        let last = pipeline.stages.len() - 1;
        for (i, stage) in pipeline.stages.iter().enumerate() {
            if stage.words.is_empty() {
                for (name, value) in &stage.assignments {
                    self.shell.set(name, value);
                }
                continue;
            }
            let words = self.expand_globs(&stage.words);

            // The guard checks the command as it will actually run, after
            // variables and globs: `X=/; rm -rf $X` is still rm -rf /
            let expanded = shellwords::join(&words.iter().map(String::as_str).collect::<Vec<_>>());
            if let GuardVerdict::Refuse { reason } = PlaygroundGuard::check_catastrophic(&expanded) {
                self.guard_refusals += 1;
                terminal.push_str(&format!("{}Blocked. Nothing ran.\n\n{}\n", self.marks.warning, reason));
                return terminal;
            }

            let mut stdin = piped.take();
            for redirect in &stage.redirects {
                if let Redirect::In(file) = redirect {
                    match self.vfs.read_file(file) {
                        Ok(text) => stdin = Some(text),
                        Err(_) => {
                            terminal.push_str(&self.error(format!("{file}: No such file or directory")));
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
                    let old = self.shell.vars.get(name).cloned();
                    let was_exported = !self.shell.exported.insert(name.clone());
                    self.shell.set(name, value);
                    (name.clone(), old, was_exported)
                })
                .collect();

            // > and 2> empty their file before the command runs (so
            // `sort x > x` leaves x empty, as in sh)
            for redirect in &stage.redirects {
                if let Redirect::Out { target, append: false } | Redirect::Err { target, append: false } = redirect {
                    if target != "/dev/null" {
                        let _ = self.vfs.write_file(target, "", false);
                    }
                }
            }
            // A pipeline's status is its last command's
            self.shell.pending_status = None;
            let printed = self.run_stage(&words, stdin);
            match words[0].as_str() {
                "diff" if !printed.is_empty() && !self.marks.reports_failure(&printed) => {
                    self.shell.pending_status = Some(1)
                }
                "grep" if printed.trim().is_empty() || printed.starts_with("(no lines matched") => {
                    self.shell.pending_status = Some(1)
                }
                _ => {}
            }
            for (name, old, was_exported) in saved {
                if !was_exported {
                    self.shell.exported.remove(&name);
                }
                match old {
                    Some(value) => self.shell.set(&name, &value),
                    None => {
                        self.shell.vars.remove(&name);
                    }
                }
            }
            // Errors count even when 2> sends them away
            if self.marks.reports_failure(&printed) {
                self.shell.pending_status = Some(1);
            }

            // Last command, nothing redirected: show it exactly as printed
            if i == last && stage.redirects.iter().all(|r| matches!(r, Redirect::In(_))) {
                terminal.push_str(&printed);
                continue;
            }
            let (out, err, notes) = self.marks.split_streams(&printed);
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
        match self.vfs.write_file(path, body, append) {
            Ok(()) => terminal.push_str(&format!(
                "{}{} {}\n",
                self.marks.success,
                if append { "Appended to" } else { "Wrote" },
                path
            )),
            Err(e) => terminal.push_str(&self.error(format!("{path}: {e}"))),
        }
    }

    /// Run one command of a pipeline
    fn run_stage(&mut self, words: &[String], stdin: Option<String>) -> String {
        let program = words[0].as_str();
        let args = &words[1..];

        if KEYWORDS.contains(&program) {
            return self.error(format!(
                "{program}: the sandbox runs one command per line. if, for and while need a real shell (switch lessons to real files in settings)"
            ));
        }
        // A name with a slash in it is a path to a script, as in sh
        if matches!(program, "sh" | "bash") || program.contains('/') {
            return self.run_script(words);
        }

        match program {
            "export" => return self.builtin_export(args),
            "unset" => {
                for name in args {
                    self.shell.vars.remove(name);
                    self.shell.exported.remove(name);
                }
                return String::new();
            }
            "env" | "printenv" if args.is_empty() => {
                return self
                    .shell
                    .exported
                    .iter()
                    .filter_map(|k| self.shell.vars.get(k).map(|v| format!("{k}={v}\n")))
                    .collect();
            }
            "printenv" => {
                return args
                    .iter()
                    .filter_map(|k| {
                        self.shell
                            .exported
                            .contains(k)
                            .then(|| self.shell.vars.get(k))
                            .flatten()
                            .map(|v| format!("{v}\n"))
                    })
                    .collect();
            }
            "which" => return self.builtin_which(args, false),
            "type" => return self.builtin_which(args, true),
            "exit" => {
                if self.shell.depth > 0 {
                    self.shell.exiting = true;
                    return String::new();
                }
                return format!("{}exit only ends scripts here; to leave a lesson press Ctrl+L\n", self.marks.hint);
            }
            "tee" => {
                let Some(text) = stdin else {
                    return self.error("tee needs something piped into it, like: ls | tee list.txt");
                };
                let append = args.iter().any(|a| a == "-a");
                let mut notes = String::new();
                for file in args.iter().filter(|a| !a.starts_with('-')) {
                    if let Err(e) = self.vfs.write_file(file, &text, append) {
                        notes.push_str(&self.error(format!("tee: {file}: {e}")));
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
                        Err(message) => self.error(message),
                    };
                }
            }
        }

        match program {
            "sed" => return self.sandbox_sed(args),
            "tr" => return self.error("tr reads from a pipe, like: cat names.txt | tr a-z A-Z"),
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
                return self.run_script(&script);
            }
        }

        let line = shellwords::join(&words.iter().map(String::as_str).collect::<Vec<_>>());
        let cmd = match self.analyzer.parse(&line) {
            Ok(cmd) => cmd,
            Err(e) => return self.error(e),
        };
        self.run_command(&cmd)
    }

    fn builtin_export(&mut self, args: &[String]) -> String {
        if args.is_empty() {
            return self
                .shell
                .exported
                .iter()
                .filter_map(|k| self.shell.vars.get(k).map(|v| format!("export {k}=\"{v}\"\n")))
                .collect();
        }
        let mut out = String::new();
        for arg in args {
            let (name, value) = match arg.split_once('=') {
                Some((name, value)) => (name, Some(value)),
                None => (arg.as_str(), None),
            };
            if !shell_syntax::is_name(name) {
                out.push_str(&self.error(format!("export: '{arg}': not a valid name")));
                continue;
            }
            if let Some(value) = value {
                self.shell.set(name, value);
            }
            self.shell.exported.insert(name.to_string());
        }
        out
    }

    /// Folders on PATH that live in the practice files, in order
    fn find_on_path(&self, program: &str) -> Option<String> {
        let path = self.shell.vars.get("PATH")?;
        path.split(':')
            .filter(|dir| dir.starts_with("/lesson-home"))
            .map(|dir| format!("{}/{}", dir.trim_end_matches('/'), program))
            .find(|candidate| {
                // A platform without modes counts every file as runnable
                self.vfs
                    .metadata(candidate)
                    .is_some_and(|m| !m.is_dir && m.mode.is_none_or(|bits| bits & 0o111 != 0))
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
                (None, _) => out.push_str(&self.error(format!("{name}: not found on PATH"))),
            }
        }
        out
    }

    fn run_script(&mut self, words: &[String]) -> String {
        let program = words[0].as_str();
        let (name, direct, args) = if matches!(program, "sh" | "bash") {
            let rest: Vec<&String> = words[1..].iter().filter(|a| !a.starts_with('-')).collect();
            match rest.split_first() {
                Some((file, args)) => ((*file).clone(), false, args.iter().map(|a| (*a).clone()).collect()),
                None => return self.error(format!("{program}: which script? Try: {program} hello.sh")),
            }
        } else {
            (program.trim_start_matches("./").to_string(), true, words[1..].to_vec())
        };
        if self.shell.depth > 0 {
            return self.error("scripts can't start other scripts in the sandbox");
        }
        if let Err(e) = self.vfs.resolve_path(&name) {
            return self.error(format!("{name}: {e}"));
        }
        if direct {
            let meta = self.vfs.metadata(&name);
            // A platform without modes counts every file as runnable
            let executable = meta.and_then(|m| m.mode).is_none_or(|bits| bits & 0o111 != 0);
            if meta.is_some_and(|m| !m.is_dir) && !executable {
                return format!(
                    "{}{}: permission denied\n\n{}A file must be marked executable before it can run: chmod +x {}\n",
                    self.marks.error, program, self.marks.hint, name
                );
            }
        }
        let script = match self.vfs.read_file(&name) {
            Ok(text) => text,
            Err(_) => return self.error(format!("{name}: no such script")),
        };

        // A script is a new shell: it sees exported variables only, and
        // what it sets doesn't come back
        let saved_args = std::mem::replace(&mut self.shell.args, args);
        let saved_vars = self.shell.vars.clone();
        let saved_exported = self.shell.exported.clone();
        let exported = &self.shell.exported;
        self.shell.vars.retain(|k, _| exported.contains(k));
        self.shell.depth += 1;
        let mut output = String::new();
        for line in script.lines().map(str::trim) {
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            output.push_str(&self.run_line(line));
            if self.shell.exiting {
                break;
            }
        }
        self.shell.depth -= 1;
        self.shell.exiting = false;
        self.shell.args = saved_args;
        self.shell.vars = saved_vars;
        self.shell.exported = saved_exported;
        if output.is_empty() {
            output.push_str("(the script ran and printed nothing)\n");
        }
        output
    }

    fn sandbox_sed(&mut self, args: &[String]) -> String {
        let in_place = args.iter().any(|a| a == "-i");
        let ops = text_filter::operands("sed", args);
        let Some((_, files)) = ops.split_first() else {
            return self.error("sed needs a script, like: sed 's/old/new/' notes.txt");
        };
        if files.is_empty() {
            return self.error("sed needs a file here, or text piped in: cat notes.txt | sed 's/old/new/'");
        }
        let filter_args: Vec<String> = args.iter().filter(|a| *a != "-i" && !files.contains(a)).cloned().collect();
        let mut out = String::new();
        for file in files {
            let text = match self.vfs.read_file(file) {
                Ok(text) => text,
                Err(_) => {
                    out.push_str(&self.error(format!("sed: {file}: No such file or directory")));
                    continue;
                }
            };
            match text_filter::apply("sed", &filter_args, &text) {
                Ok(changed) if in_place => {
                    if let Err(e) = self.vfs.write_file(file, &changed, false) {
                        out.push_str(&self.error(format!("sed: {file}: {e}")));
                    }
                }
                Ok(changed) => out.push_str(&changed),
                Err(message) => return self.error(message),
            }
        }
        out
    }

    fn sandbox_find(&self, args: &[String]) -> String {
        let (paths, query) = match crate::virtual_fs_tools::FindQuery::parse(args) {
            Ok(parsed) => parsed,
            Err(message) => return self.error(message),
        };
        let mut out = String::new();
        for path in paths {
            match self.vfs.find(&path, &query) {
                Ok(found) => found.iter().for_each(|f| {
                    out.push_str(f);
                    out.push('\n');
                }),
                Err(e) => out.push_str(&self.error(format!("find: {e}"))),
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
        let mut out = String::new();
        for path in paths {
            match self.vfs.disk_usage(&path, summarize) {
                Ok(rows) => rows.iter().for_each(|(p, b)| out.push_str(&format!("{}\t{}\n", size(*b), p))),
                Err(e) => out.push_str(&self.error(format!("du: {e}"))),
            }
        }
        out
    }

    fn sandbox_diff(&self, args: &[String]) -> String {
        let files: Vec<&String> = args.iter().filter(|a| !a.starts_with('-')).collect();
        let [a, b] = files[..] else {
            return self.error("diff compares two files, like: diff old.txt new.txt");
        };
        let read = |f: &str| self.vfs.read_file(f).map_err(|_| self.error(format!("diff: {f}: No such file or directory")));
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
                    None => return self.error("tar: -C needs a folder"),
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
                        None => return self.error("tar: f needs the archive's name, like: tar -czf notes.tar.gz notes"),
                    },
                    other => return self.error(format!("tar: the {other} option isn't available in the sandbox (try c, x, t, z, v, f)")),
                }
            }
        }
        let Some(archive) = archive else {
            return self.error("tar: name the archive with f, like: tar -czf notes.tar.gz notes");
        };
        let result = match (create, extract, list) {
            (true, false, false) => self.vfs.tar_create(&archive, &inputs, gzip).map(|names| (names, verbose)),
            (false, true, false) => self.vfs.tar_extract(&archive, &dest).map(|names| (names, verbose)),
            (false, false, true) => self.vfs.tar_list(&archive).map(|names| (names, true)),
            _ => return self.error("tar: choose one of c (create), x (extract) or t (list)"),
        };
        match result {
            Ok((names, show)) if show => names.iter().map(|n| format!("{n}\n")).collect(),
            Ok(_) => String::new(),
            Err(e) => self.error(e),
        }
    }

    fn sandbox_gzip(&self, program: &str, args: &[String]) -> String {
        let unzip = program == "gunzip" || args.iter().any(|a| a == "-d");
        let keep = args.iter().any(|a| a == "-k" || a == "--keep");
        let files: Vec<&String> = args.iter().filter(|a| !a.starts_with('-')).collect();
        if files.is_empty() {
            return self.error(format!("{program}: name a file, like: {program} notes.txt{}", if unzip { ".gz" } else { "" }));
        }
        let mut out = String::new();
        for file in files {
            // -k keeps the original: copy it back after the swap
            let backup = keep.then(|| self.vfs.read_bytes(file).ok()).flatten();
            let result = if unzip { self.vfs.gunzip(file) } else { self.vfs.gzip(file) };
            match result {
                Ok(_) => {
                    if let Some(bytes) = backup {
                        let _ = self.vfs.write_bytes(file, &bytes);
                    }
                }
                Err(e) => out.push_str(&self.error(e)),
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
                other => return self.error(format!("ssh-keygen: {other} isn't available in the sandbox (try -t, -f, -C, -N)")),
            }
        }
        if !matches!(kind.as_str(), "ed25519" | "rsa") {
            return self.error("ssh-keygen: -t takes ed25519 (recommended) or rsa");
        }
        let file = file.unwrap_or_else(|| format!("/lesson-home/.ssh/id_{kind}"));
        match self.vfs.practice_ssh_key(&file, &kind, &comment) {
            Ok((private, public)) => format!(
                "Generating public/private {kind} key pair.\n\
                 Your identification has been saved in {private}\n\
                 Your public key has been saved in {public}\n\
                 {}This is a practice key made in the sandbox. It opens nothing.\n",
                self.marks.hint
            ),
            Err(e) => self.error(format!("ssh-keygen: {e}")),
        }
    }
}
