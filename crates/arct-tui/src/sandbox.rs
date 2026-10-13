//! The sandbox shell in the terminal app. The simulated sandbox's shell
//! (variables, globs, redirection, pipes, `&&` / `||` / `;`, scripts and
//! the commands themselves) is `arct_core::sandbox`, shared with the
//! browser; the app runs typed lines through it here. Real-files mode
//! lives here too: variables carried from one real command to the next,
//! and lines expanded by the app before a real shell runs them.

use crate::app::App;
use crate::icons;
use arct_core::sandbox::{Marks, Sandbox, ShellState, KEYWORDS};
use arct_core::shell_syntax::{self, Joiner, Word};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// The sandbox shell's state: the simulated shell's variables, script
/// arguments and last status, and the variables set in real-files mode
#[derive(Debug, Clone)]
pub struct SandboxShell {
    /// The simulated sandbox's shell state (see `arct_core::sandbox`)
    pub(crate) shell: ShellState,
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
        Self {
            shell: ShellState::new(),
            real: BTreeMap::new(),
        }
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

fn error(message: impl std::fmt::Display) -> String {
    format!("{}{}\n", icons::error().content, message)
}

/// The app's icons as the sandbox's message marks (plain symbols, or Nerd
/// Font glyphs when those are on)
pub(crate) fn marks() -> Marks {
    Marks {
        success: icons::success().content.to_string(),
        error: icons::error().content.to_string(),
        warning: icons::warning().content.to_string(),
        info: icons::info().content.to_string(),
        hint: icons::hint().content.to_string(),
        folder: icons::folder().content.to_string(),
        file: icons::file().content.to_string(),
    }
}

impl App {
    /// Run one typed line in the simulated sandbox. `None` outside lesson
    /// mode or before the sandbox exists.
    pub(crate) fn run_sandbox_line(&mut self, line: &str) -> Option<String> {
        if !self.lesson_mode {
            return None;
        }
        let marks = marks();
        let vfs = self.virtual_fs.as_mut()?;
        let mut sandbox = Sandbox::new(&mut self.sandbox.shell, vfs, &self.analyzer, &marks);
        let output = sandbox.run_line(line);
        let refusals = sandbox.guard_refusals();
        for _ in 0..refusals {
            self.record_feature_event("guard_refusal");
        }
        Some(output)
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
