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

    /// Real-files mode: handle a line that only sets or clears variables
    /// (`NAME=value`, `export NAME=value`, `unset NAME`). Returns `None`
    /// for any other line, which then runs in a real shell.
    pub(crate) fn real_assignments(&mut self, line: &str) -> Option<String> {
        let real_value = |name: &str| std::env::var(name).ok();
        let lookup = |name: &str| self.real.get(name).cloned().or_else(|| real_value(name));
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
            let text = self.run_sandbox_pipeline(&pipeline);
            self.sandbox.status = i32::from(Self::output_reports_failure(&text));
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
            let saved: Vec<(String, Option<String>)> = stage
                .assignments
                .iter()
                .map(|(name, value)| {
                    let old = self.sandbox.vars.get(name).cloned();
                    self.sandbox.set(name, value);
                    (name.clone(), old)
                })
                .collect();
            let printed = self.run_sandbox_stage(&words, stdin);
            for (name, old) in saved {
                match old {
                    Some(value) => self.sandbox.set(&name, &value),
                    None => {
                        self.sandbox.vars.remove(&name);
                    }
                }
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

        let saved_args = std::mem::replace(&mut self.sandbox.args, args);
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
            for letter in letters.chars() {
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
        let files: Vec<&String> = args.iter().filter(|a| !a.starts_with('-')).collect();
        if files.is_empty() {
            return error(format!("{program}: name a file, like: {program} notes.txt{}", if unzip { ".gz" } else { "" }));
        }
        let Some(vfs) = self.virtual_fs.as_ref() else { return String::new() };
        let mut out = String::new();
        for file in files {
            let result = if unzip { vfs.gunzip(file) } else { vfs.gzip(file) };
            if let Err(e) = result {
                out.push_str(&error(e));
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
        assert!(!read(&app, "env.txt").contains("GREETING"), "not exported");
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
        assert_eq!(shell.real_assignments("GREETING=hello"), Some(String::new()));
        assert_eq!(shell.real_assignments("export WHO=\"$GREETING world\""), Some(String::new()));
        let env: BTreeMap<String, String> = shell.real_env().collect();
        assert_eq!(env["WHO"], "hello world");
        assert_eq!(shell.real_assignments("export PATH=/x/bin:$PATH").as_deref(), Some(""));
        assert!(shell.real.get("PATH").unwrap().starts_with("/x/bin:"));
        assert_eq!(shell.real_assignments("unset GREETING"), Some(String::new()));
        assert!(!shell.real.contains_key("GREETING"));
        // Anything else runs for real
        assert_eq!(shell.real_assignments("echo $WHO"), None);
        assert_eq!(shell.real_assignments("X=1 env"), None);
        assert_eq!(shell.real_assignments("export"), None);
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
    }
}
