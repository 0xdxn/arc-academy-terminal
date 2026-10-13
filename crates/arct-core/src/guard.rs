//! Safety guard for real-filesystem lesson practice ("Playground" mode).
//!
//! When a lesson runs in real practice mode, commands execute against the
//! real shell inside `~/ArcAcademy/playground`. This guard sits in front of
//! that shell and refuses:
//!
//! 1. Commands that reference paths outside the playground (absolute paths,
//!    `~`, or enough `..` to climb out given the tracked working directory).
//! 2. A small denylist of catastrophic patterns (`rm -rf /`, `mkfs`,
//!    `dd of=/dev/...`, fork bombs, `chmod -R 777 /`, redirects into `/dev`,
//!    and anything run via `sudo`) — each refusal explains WHY the pattern
//!    is dangerous, because that is itself a teaching moment.
//! 3. Constructs we cannot reason about safely (command substitution,
//!    environment-variable expansion, unbalanced quotes). The heuristics are
//!    deliberately conservative: when in doubt, refuse politely.
//!
//! The same guard protects the free shell when `shell.safe_mode` is on (the
//! default), rooted at `~/ArcAcademy/home`. With safe mode off, the free shell
//! still runs every command through [`PlaygroundGuard::check_catastrophic`].

use std::path::{Component, Path, PathBuf};

/// Human-readable location of the playground, used in refusal messages.
pub const PLAYGROUND_DISPLAY: &str = "~/ArcAcademy/playground";

/// Outcome of checking a command against the playground guard.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GuardVerdict {
    /// Command is allowed to run in the real shell.
    Allow,
    /// Command is refused; `reason` is a friendly, novice-facing explanation.
    Refuse { reason: String },
}

impl GuardVerdict {
    pub fn is_allowed(&self) -> bool {
        matches!(self, GuardVerdict::Allow)
    }

    fn refuse(reason: impl Into<String>) -> Self {
        GuardVerdict::Refuse {
            reason: reason.into(),
        }
    }
}

/// Guard that keeps real-mode lesson commands inside the playground.
pub struct PlaygroundGuard {
    /// Absolute path of the playground root on the real filesystem.
    root: PathBuf,
    /// How the root is shown to the learner in refusal messages.
    display: String,
}

/// What a sed script can reach beyond the text it edits
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SedReach {
    /// Only commands that edit, print or delete lines
    Harmless,
    /// w, W, r, R or e somewhere (or s///w, s///e): files or programs
    FilesOrPrograms,
    /// Not understood: left to the ordinary path check
    Unknown,
}

/// Classify a sed script by reading it the way sed does: addresses
/// (a number, $, /regex/ or \cregexc, a range, !), then a command. Only
/// a short list of commands counts as harmless.
fn sed_script_reach(script: &str) -> SedReach {
    let chars: Vec<char> = script.chars().collect();
    let mut i = 0;
    // Read a delimited part (regex or replacement) ending at an unescaped
    // `delim`; returns the index after it
    let read_part = |mut i: usize, delim: char| -> Option<usize> {
        while i < chars.len() {
            match chars[i] {
                '\\' => i += 2,
                c if c == delim => return Some(i + 1),
                '\n' => return None,
                _ => i += 1,
            }
        }
        None
    };
    let read_address = |i: usize| -> Option<usize> {
        match chars.get(i) {
            Some(c) if c.is_ascii_digit() => {
                let mut j = i;
                while chars.get(j).is_some_and(|c| c.is_ascii_digit()) {
                    j += 1;
                }
                Some(j)
            }
            Some('$') => Some(i + 1),
            Some('/') => read_part(i + 1, '/'),
            Some('\\') => {
                let delim = *chars.get(i + 1)?;
                read_part(i + 2, delim)
            }
            _ => Some(i),
        }
    };
    let mut reach = SedReach::Harmless;
    loop {
        while chars.get(i).is_some_and(|c| c.is_whitespace() || *c == ';') {
            i += 1;
        }
        if i >= chars.len() {
            return reach;
        }
        let Some(after) = read_address(i) else { return SedReach::Unknown };
        i = after;
        if chars.get(i) == Some(&',') {
            let Some(after) = read_address(i + 1) else { return SedReach::Unknown };
            if after == i + 1 {
                return SedReach::Unknown;
            }
            i = after;
        }
        while chars.get(i).is_some_and(|c| c.is_whitespace() || *c == '!') {
            i += 1;
        }
        let Some(&command) = chars.get(i) else { return SedReach::Unknown };
        i += 1;
        match command {
            'w' | 'W' | 'r' | 'R' | 'e' => return SedReach::FilesOrPrograms,
            'd' | 'D' | 'p' | 'P' | 'n' | 'N' | 'q' | 'Q' | '=' | 'g' | 'G' | 'h' | 'H'
            | 'x' | 'l' | 'z' | '{' | '}' => {}
            's' | 'y' => {
                let Some(&delim) = chars.get(i) else { return SedReach::Unknown };
                if delim == '\\' || delim == '\n' || delim.is_whitespace() {
                    return SedReach::Unknown;
                }
                let Some(after) = read_part(i + 1, delim) else { return SedReach::Unknown };
                let Some(after) = read_part(after, delim) else { return SedReach::Unknown };
                i = after;
                if command == 's' {
                    while let Some(&flag) = chars.get(i) {
                        match flag {
                            'w' | 'e' => return SedReach::FilesOrPrograms,
                            'g' | 'p' | 'i' | 'I' | 'm' | 'M' => i += 1,
                            c if c.is_ascii_digit() => i += 1,
                            _ => break,
                        }
                    }
                }
            }
            _ => reach = SedReach::Unknown,
        }
        if reach == SedReach::Unknown {
            return reach;
        }
        while chars.get(i).is_some_and(|c| *c == ' ' || *c == '\t') {
            i += 1;
        }
        match chars.get(i) {
            None => return reach,
            Some(';') | Some('\n') | Some('}') => {}
            Some(_) => return SedReach::Unknown,
        }
    }
}

impl PlaygroundGuard {
    /// Create a guard for the given playground root (absolute path).
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self::with_display(root, PLAYGROUND_DISPLAY)
    }

    /// Create a guard whose refusal messages name the root as `display`
    /// (e.g. `~/ArcAcademy/home`).
    pub fn with_display(root: impl Into<PathBuf>, display: impl Into<String>) -> Self {
        Self {
            root: root.into(),
            display: display.into(),
        }
    }

    /// Check only the catastrophic-pattern denylist, with no containment.
    /// Used by the free shell when safe mode is off: commands may touch the
    /// whole filesystem, but never the handful that destroy it.
    pub fn check_catastrophic(command: &str) -> GuardVerdict {
        let trimmed = command.trim();
        if trimmed.is_empty() {
            return GuardVerdict::Allow;
        }
        Self::check_denylist(trimmed).unwrap_or(GuardVerdict::Allow)
    }

    /// Check a command line before it is sent to the real shell.
    ///
    /// `cwd` is the tracked working directory for the practice session; it
    /// must be inside the playground root. The check is purely lexical — it
    /// never touches the filesystem.
    pub fn check(&self, command: &str, cwd: &Path) -> GuardVerdict {
        self.check_line(command, cwd, false)
    }

    /// Check a line the app has already expanded and quoted itself (see
    /// `shell_syntax::render`): every word is a literal, and the only `$`
    /// left is inside single quotes or the shell's own `"$?"`. Such a line
    /// shows exactly what will run, so `$` is no reason to refuse it.
    /// Command substitution is still refused.
    pub fn check_expanded(&self, command: &str, cwd: &Path) -> GuardVerdict {
        // Trust the contract only after checking it
        if !Self::dollars_are_rendered(command) {
            return GuardVerdict::refuse(
                "That command still has a '$' the shell would fill in, so it can't be \
                 checked safely.",
            );
        }
        self.check_line(command, cwd, true)
    }

    /// Every `$` sits inside single quotes, or is exactly `"$?"`, and the
    /// only backslash is the `\'` that writes a quote inside a quoted word
    fn dollars_are_rendered(command: &str) -> bool {
        let mut rest = command;
        let mut quoted = false;
        while let Some(c) = rest.chars().next() {
            if quoted {
                quoted = c != '\'';
            } else if c == '\'' {
                quoted = true;
            } else if rest.starts_with("\\'") {
                // the quote inside a quoted word, as in 'it'\''s'
                rest = &rest[2..];
                continue;
            } else if c == '\\' {
                // nothing else in a rendered line is escaped
                return false;
            } else if rest.starts_with("\"$?\"") {
                rest = &rest[4..];
                continue;
            } else if c == '$' {
                return false;
            }
            rest = &rest[c.len_utf8()..];
        }
        true
    }

    fn check_line(&self, command: &str, cwd: &Path, expanded: bool) -> GuardVerdict {
        let trimmed = command.trim();
        if trimmed.is_empty() {
            return GuardVerdict::Allow;
        }

        // --- 1. Catastrophic-pattern denylist (checked first so the user
        // gets the specific teaching-moment explanation, not a generic
        // path message) ---
        if let Some(verdict) = Self::check_denylist(trimmed) {
            return verdict;
        }

        // --- 2. Constructs we can't reason about safely ---
        if trimmed.contains('`') || trimmed.contains("$(") {
            return GuardVerdict::refuse(
                "Command substitution (backticks or $(...)) runs a hidden inner command, \
                 so there's no way to check where it will reach. Type the command \
                 directly instead.",
            );
        }
        if trimmed.contains('$') && !expanded {
            return GuardVerdict::refuse(
                "Environment variables like $HOME or $PATH can expand to locations outside \
                 the folder you're practising in, so commands using '$' are blocked. Try \
                 typing the actual path or value instead.",
            );
        }

        // --- 3. Tokenize (handles quoted paths like "cat '/etc/passwd'") ---
        let tokens = match shellwords::split(trimmed) {
            Ok(tokens) => tokens,
            Err(_) => {
                return GuardVerdict::refuse(
                    "That command has unbalanced quotes, so it can't be checked safely. \
                     Close the quote and try again.",
                );
            }
        };

        // --- 4. Per-token path containment ---
        // A sed script such as '/debug/d' starts with '/' but names no file.
        // Scripts made only of commands that can't reach a file or program
        // are left out of the path check; scripts with w, r or e (which do)
        // are refused; anything not understood is checked as a path.
        let mut sed_scripts = std::collections::HashSet::new();
        for (index, script) in Self::sed_script_tokens(&tokens) {
            match sed_script_reach(script) {
                SedReach::Harmless => {
                    sed_scripts.insert(index);
                }
                SedReach::FilesOrPrograms => {
                    return GuardVerdict::refuse(
                        "sed's w, r and e commands write files, read files or run programs \
                         from inside the script, where the guard can't follow them, so \
                         they're turned off here. Redirect sed's output with > instead.",
                    );
                }
                SedReach::Unknown => {}
            }
        }
        let tokens: Vec<String> = tokens
            .into_iter()
            .enumerate()
            .filter(|(index, _)| !sed_scripts.contains(index))
            .map(|(_, token)| token)
            .collect();
        let mut total_climb: usize = 0;
        // Strip redirect prefixes so `>/etc/passwd` or `2>/tmp/x` are
        // analyzed as paths, and check what follows `=` too, so
        // `of=/dev/disk2` or `--output=/etc/x` is seen as the path it names
        let candidates = tokens.iter().flat_map(|raw_token| {
            let digits_stripped = raw_token.trim_start_matches(|c: char| c.is_ascii_digit());
            let token = if digits_stripped.starts_with(['<', '>', '&']) {
                digits_stripped.trim_start_matches(['<', '>', '&'])
            } else {
                raw_token.as_str()
            };
            let value = token.split_once('=').map(|(_, value)| value);
            std::iter::once(token).chain(value)
        });
        for token in candidates {
            if token.is_empty() {
                continue;
            }

            // Tilde expansion points at the home directory. `~/<root>/...` is
            // allowed only if it still resolves inside the root after `..`.
            if token.contains('~') {
                let inside = self
                    .display
                    .strip_prefix("~/")
                    .and_then(|_| token.strip_prefix(self.display.as_str()))
                    .filter(|rest| rest.is_empty() || rest.starts_with('/'))
                    .and_then(|rest| {
                        let joined = format!("{}{}", self.root.display(), rest);
                        Self::lexical_normalize(Path::new(&joined))
                    })
                    .is_some_and(|normalized| normalized.starts_with(&self.root));
                if inside {
                    continue;
                }
                return GuardVerdict::refuse(format!(
                    "Commands here stay inside {} — '~' points at your real home \
                     directory. Try a relative path instead.",
                    self.display
                ));
            }

            // Absolute paths must stay inside the playground root
            if token.starts_with('/') {
                match Self::lexical_normalize(Path::new(token)) {
                    Some(normalized) if normalized.starts_with(&self.root) => continue,
                    _ => {
                        return GuardVerdict::refuse(format!(
                            "Commands here stay inside {} — '{token}' points outside \
                             it. Try a relative path instead.",
                            self.display
                        ));
                    }
                }
            }

            // Relative paths: accumulate the deepest upward climb (`..`)
            total_climb += Self::upward_climb(Path::new(token));
        }

        // --- 5. `..` escape budget given the tracked cwd depth ---
        let depth = cwd
            .strip_prefix(&self.root)
            .map(|rel| rel.components().count())
            .unwrap_or(0);
        if total_climb > depth {
            return GuardVerdict::refuse(format!(
                "That command uses enough '..' to climb out of {}. Commands here stay \
                 inside it — try a relative path.",
                self.display
            ));
        }

        GuardVerdict::Allow
    }

    /// Denylist of catastrophic patterns, refused regardless of paths.
    /// Each refusal explains WHY the pattern is dangerous.
    fn check_denylist(command: &str) -> Option<GuardVerdict> {
        // Fork bomb: :(){ :|:& };:  (whitespace-insensitive)
        let squashed: String = command.chars().filter(|c| !c.is_whitespace()).collect();
        if squashed.contains(":(){") || squashed.contains(":|:&") {
            return Some(GuardVerdict::refuse(
                "That's a fork bomb — a tiny function that endlessly launches copies of \
                 itself until the computer runs out of memory and freezes. It looks like \
                 line noise, which is exactly why people get tricked into running it.",
            ));
        }

        // Commands in a chain, split two ways: at every ; | & (catches a
        // quote that hides a split, like `true';' sudo x`), and only where
        // the shell splits (catches a quoted ';' that would otherwise make
        // the segment fail to tokenize, like `rm -rf ~ ';'`)
        let mut segments: Vec<String> = command
            .split([';', '|', '&'])
            .map(str::to_string)
            .collect();
        segments.extend(Self::split_unquoted(command));

        // sudo anywhere in the pipeline/sequence
        for segment in &segments {
            let first = shellwords::split(segment)
                .ok()
                .and_then(|t| t.into_iter().next())
                .or_else(|| segment.split_whitespace().next().map(str::to_string));
            if first.as_deref() == Some("sudo") {
                return Some(GuardVerdict::refuse(
                    "'sudo' runs a command with full administrator power, where a single \
                     typo can change or delete critical system files. arct never needs \
                     admin rights, so sudo is blocked here; when you truly need it, use \
                     your own terminal and read the command twice first.",
                ));
            }
        }

        // Every word, as typed and as the shell reads it after quotes, so
        // 'of=/dev/disk2' is seen as of=/dev/disk2
        let words: Vec<String> = command
            .split_whitespace()
            .map(str::to_string)
            .chain(segments.iter().filter_map(|s| shellwords::split(s).ok()).flatten())
            .collect();

        // mkfs formats disks
        if words.iter().any(|t| t == "mkfs" || t.starts_with("mkfs.")) {
            return Some(GuardVerdict::refuse(
                "'mkfs' formats a disk partition, instantly erasing every file on it. \
                 It's used to prepare brand-new drives — never something to practice with.",
            ));
        }

        // dd writing to a raw device
        if words.iter().any(|t| t.starts_with("of=/dev/")) {
            return Some(GuardVerdict::refuse(
                "'dd' writing to a /dev device bypasses the filesystem and overwrites the \
                 raw disk underneath your files — it can wipe an entire drive in seconds \
                 with no confirmation and no undo.",
            ));
        }

        // Redirecting output into a device (e.g. `> /dev/sda`). /dev/null,
        // the "throw it away" file, and the terminal's own streams are fine
        const HARMLESS: &[&str] = &["/dev/null", "/dev/stdout", "/dev/stderr", "/dev/tty"];
        let into_device =
            regex::Regex::new(r#">>?\s*['"]?(/dev/[^\s;|&<>'"]*)"#).expect("valid pattern");
        if into_device
            .captures_iter(command)
            .any(|c| !HARMLESS.contains(&c.get(1).map_or("", |m| m.as_str())))
        {
            return Some(GuardVerdict::refuse(
                "Redirecting output into /dev writes bytes directly to a hardware device. \
                 Aimed at a disk like /dev/sda, that corrupts the filesystem and destroys \
                 data immediately.",
            ));
        }

        // rm -rf / chmod -R 777 — checked in every segment of a chain like
        // `cd / && rm -rf *` or `true; rm -rf ~`
        for segment in &segments {
            if let Some(verdict) = Self::check_segment(segment.trim()) {
                return Some(verdict);
            }
        }

        None
    }

    /// Split a line at the `;`, `|` and `&` the shell would act on: not
    /// inside quotes, not after a backslash
    fn split_unquoted(command: &str) -> Vec<String> {
        let mut segments = vec![String::new()];
        let mut quote: Option<char> = None;
        let mut chars = command.chars();
        while let Some(c) = chars.next() {
            match (quote, c) {
                (None, '\'' | '"') => quote = Some(c),
                (Some(q), _) if c == q => quote = None,
                (None | Some('"'), '\\') => {
                    segments.last_mut().expect("never empty").push(c);
                    if let Some(next) = chars.next() {
                        segments.last_mut().expect("never empty").push(next);
                    }
                    continue;
                }
                (None, ';' | '|' | '&') => {
                    segments.push(String::new());
                    continue;
                }
                _ => {}
            }
            segments.last_mut().expect("never empty").push(c);
        }
        segments
    }

    /// Destructive-target checks for one simple command (no `;`/`|`/`&`).
    fn check_segment(segment: &str) -> Option<GuardVerdict> {
        let tokens = shellwords::split(segment).ok()?;
        let program = tokens.first().map(String::as_str)?;
        let is_root = |t: &str| matches!(t, "/" | "/*");
        let is_home = |t: &str| {
            matches!(
                t.trim_end_matches('/'),
                "~" | "~/*" | "$HOME" | "${HOME}" | "$HOME/*" | "${HOME}/*"
            )
        };

        if program == "rm" {
            let mut recursive = false;
            let mut no_preserve_root = false;
            let mut targets_root = false;
            let mut targets_home = false;
            for t in &tokens[1..] {
                if t == "--no-preserve-root" {
                    no_preserve_root = true;
                } else if t == "--recursive" {
                    recursive = true;
                } else if t.starts_with('-') && !t.starts_with("--") {
                    recursive |= t.contains('r') || t.contains('R');
                } else if is_root(t) {
                    targets_root = true;
                } else if is_home(t) {
                    targets_home = true;
                }
            }
            if recursive && (targets_root || no_preserve_root) {
                return Some(GuardVerdict::refuse(
                    "'rm -rf /' recursively force-deletes EVERYTHING starting from the \
                     root of the filesystem — the operating system, your applications, \
                     and all your files — with no confirmation and no undo. This is the \
                     single most destructive command on a Unix system.",
                ));
            }
            if recursive && targets_home {
                return Some(GuardVerdict::refuse(
                    "'rm -r ~' deletes your whole home folder: every document, photo, \
                     project and setting you own, with no trash can and no undo. To \
                     clean up, delete one named folder at a time and run 'ls' first to \
                     see what's inside.",
                ));
            }
        }

        if program == "chmod" {
            let recursive = tokens.iter().any(|t| t == "-R" || t == "-r");
            let mode_777 = tokens.iter().any(|t| t == "777");
            let targets_root = tokens.iter().any(|t| is_root(t));
            if recursive && mode_777 && targets_root {
                return Some(GuardVerdict::refuse(
                    "'chmod -R 777 /' makes every file on the system readable, writable \
                     and executable by everyone — a massive security hole that also \
                     breaks programs that refuse to run with unsafe permissions.",
                ));
            }
        }

        None
    }

    /// Lexically resolve `.`/`..` in an absolute path (no filesystem access).
    /// Returns None if the path climbs above `/`.
    /// Indices of the tokens that are sed scripts: after `-e`, in
    /// `--expression=`, or else sed's first argument that isn't an option.
    /// sed must be the command (first word, or after ; | & && ||).
    fn sed_script_tokens(tokens: &[String]) -> Vec<(usize, &str)> {
        let ends_command = |t: &str| t.ends_with([';', '|', '&']);
        let mut found = Vec::new();
        let mut i = 0;
        while i < tokens.len() {
            let at_command = i == 0 || ends_command(&tokens[i - 1]);
            if !(at_command && tokens[i] == "sed") {
                i += 1;
                continue;
            }
            let mut j = i + 1;
            let mut saw_expression = false;
            let mut first_operand = None;
            while j < tokens.len() {
                let t = tokens[j].as_str();
                let last = ends_command(t);
                if t == "-e" || t == "--expression" {
                    if let Some(next) = tokens.get(j + 1) {
                        found.push((j + 1, next.as_str()));
                    }
                    saw_expression = true;
                    j += 2;
                    continue;
                }
                if let Some(script) = t.strip_prefix("--expression=") {
                    // the path check still sees the whole token; the script
                    // inside it is classified for w, r and e
                    if matches!(sed_script_reach(script), SedReach::FilesOrPrograms) {
                        found.push((j, script));
                    }
                    saw_expression = true;
                } else if t == "-f" || t == "--file" {
                    j += 2; // a script file is a path: checked as one
                    continue;
                } else if !t.starts_with('-') && first_operand.is_none() {
                    first_operand = Some(j);
                }
                j += 1;
                if last {
                    break;
                }
            }
            if let (false, Some(k)) = (saw_expression, first_operand) {
                found.push((k, tokens[k].as_str()));
            }
            i = j;
        }
        found
    }

    fn lexical_normalize(path: &Path) -> Option<PathBuf> {
        let mut out = PathBuf::from("/");
        for component in path.components() {
            match component {
                Component::RootDir | Component::Prefix(_) | Component::CurDir => {}
                Component::ParentDir => {
                    if !out.pop() {
                        return None;
                    }
                }
                Component::Normal(part) => out.push(part),
            }
        }
        Some(out)
    }

    /// Deepest upward climb of a relative path: how many levels above the
    /// starting directory it can reach (e.g. `a/../../b` climbs 1, `../..`
    /// climbs 2). Summed across tokens, this is a conservative bound on how
    /// far a command can escape.
    fn upward_climb(path: &Path) -> usize {
        let mut depth: isize = 0;
        let mut min_depth: isize = 0;
        for component in path.components() {
            match component {
                Component::ParentDir => {
                    depth -= 1;
                    min_depth = min_depth.min(depth);
                }
                Component::Normal(_) => depth += 1,
                _ => {}
            }
        }
        (-min_depth).max(0) as usize
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn guard() -> PlaygroundGuard {
        PlaygroundGuard::new("/home/user/ArcAcademy/playground")
    }

    /// cwd = playground/<lesson-id> (depth 1)
    fn lesson_cwd() -> PathBuf {
        PathBuf::from("/home/user/ArcAcademy/playground/file-mgmt")
    }

    /// cwd = playground root (depth 0)
    fn root_cwd() -> PathBuf {
        PathBuf::from("/home/user/ArcAcademy/playground")
    }

    fn assert_allowed(cmd: &str, cwd: &Path) {
        let verdict = guard().check(cmd, cwd);
        assert!(verdict.is_allowed(), "expected Allow for {cmd:?}, got {verdict:?}");
    }

    fn assert_refused(cmd: &str, cwd: &Path) -> String {
        match guard().check(cmd, cwd) {
            GuardVerdict::Refuse { reason } => reason,
            GuardVerdict::Allow => panic!("expected Refuse for {cmd:?}, got Allow"),
        }
    }

    // --- sed scripts are scripts, not paths ---

    #[test]
    fn sed_scripts_that_only_edit_text_are_not_paths() {
        let cwd = lesson_cwd();
        for cmd in [
            "sed '/debug/d' app.log",
            "sed -n '/error/p' app.log",
            "sed -e '/a/d' -e 's/x/y/g' notes.txt",
            "sed '1,/start/d;s|/usr|/opt|g' notes.txt",
            "sed -i.bak '/^#/d' notes.txt",
            "grep x f | sed '/debug/d'",
        ] {
            assert_allowed(cmd, &cwd);
        }
        // the same line as the real-files path renders it
        assert!(guard().check_expanded("sed '/debug/d' 'app.log'", &cwd).is_allowed());
    }

    #[test]
    fn sed_commands_that_reach_files_or_programs_are_refused() {
        let cwd = lesson_cwd();
        for cmd in [
            "sed '/x/w out.txt' notes.txt",
            "sed 's/a/b/w /tmp/x' notes.txt",
            "sed 's/a/date/e' notes.txt",
            "sed '1e date' notes.txt",
            "sed '/x/r /etc/passwd' notes.txt",
            "ls; sed -n '2R /etc/hosts' notes.txt",
            "sed -e 's/a/b/' -e 'W copy' notes.txt",
            "sed --expression='w out' notes.txt",
        ] {
            let reason = assert_refused(cmd, &cwd);
            assert!(reason.contains("w, r and e"), "{cmd}: {reason}");
        }
    }

    #[test]
    fn files_given_to_sed_are_still_checked() {
        let cwd = lesson_cwd();
        assert_refused("sed '/x/d' /etc/passwd", &cwd);
        assert_refused("sed -f /etc/script.sed notes.txt", &cwd);
        assert_refused("sed -i '/x/d' ../../../etc/hosts", &cwd);
        // a script the guard doesn't understand is checked as a path
        assert_refused("sed '/x/{w /etc/y}' notes.txt", &cwd);
        // only sed's script is spared: other commands' /words are paths
        assert_refused("cat /debug/d", &cwd);
        assert_refused("echo hi; cat /etc/passwd '/debug/d'", &cwd);
    }

    // --- Everyday lesson commands pass through untouched ---

    #[test]
    fn test_allows_common_lesson_commands() {
        let cwd = lesson_cwd();
        for cmd in [
            "pwd",
            "ls",
            "ls -lh",
            "ls -a",
            "mkdir practice",
            "touch test.txt",
            "cp test.txt test-backup.txt",
            "mv test-backup.txt backup.txt",
            "rm -i test.txt",
            "cat notes.txt",
            "grep http server.log",
            "grep -i SSH server.log",
            "head -n 5 server.log",
            "tail server.log",
            "wc -l access.log",
            "echo hello > greeting.txt",
            "echo more >> greeting.txt",
            "cut -d':' -f1 fruits.txt | sort | uniq -c",
            "ls -l | grep txt",
            "cat \"my file.txt\"",
        ] {
            assert_allowed(cmd, &cwd);
        }
    }

    #[test]
    fn test_allows_dotdot_within_budget() {
        // depth 1: one level up is still inside the playground
        assert_allowed("cd ..", &lesson_cwd());
        assert_allowed("cat ../notes.txt", &lesson_cwd());
        assert_allowed("ls ..", &lesson_cwd());
        // depth 2: two independent climbs of one level each
        let deep = lesson_cwd().join("sub");
        assert_allowed("cd .. && cat ../notes.txt", &deep);
    }

    #[test]
    fn test_allows_absolute_paths_inside_playground() {
        assert_allowed(
            "cat /home/user/ArcAcademy/playground/file-mgmt/notes.txt",
            &lesson_cwd(),
        );
        assert_allowed("ls ~/ArcAcademy/playground/file-mgmt", &lesson_cwd());
    }

    // --- Path escapes are refused with a friendly message ---

    #[test]
    fn test_refuses_absolute_paths_outside_playground() {
        let reason = assert_refused("cat /etc/passwd", &lesson_cwd());
        assert!(reason.contains("~/ArcAcademy/playground"), "reason: {reason}");
        assert_refused("ls /", &lesson_cwd());
        assert_refused("rm /tmp/somefile", &lesson_cwd());
        assert_refused("echo x > /tmp/evil", &lesson_cwd());
    }

    #[test]
    fn test_refuses_quoted_absolute_paths() {
        assert_refused("cat \"/etc/passwd\"", &lesson_cwd());
        assert_refused("cat '/etc/shadow'", &lesson_cwd());
    }

    #[test]
    fn test_refuses_absolute_path_inside_root_that_climbs_out() {
        // Lexically inside the playground prefix, but the .. escapes it
        assert_refused(
            "cat /home/user/ArcAcademy/playground/../../.ssh/id_rsa",
            &lesson_cwd(),
        );
    }

    #[test]
    fn test_refuses_tilde_outside_playground() {
        let reason = assert_refused("ls ~", &lesson_cwd());
        assert!(reason.contains("~/ArcAcademy/playground"));
        assert_refused("cd ~/Documents", &lesson_cwd());
        assert_refused("cat ~/.bashrc", &lesson_cwd());
        assert_refused("rm -rf ~", &lesson_cwd());
    }

    #[test]
    fn test_refuses_dotdot_escape() {
        // depth 0: any climb escapes
        assert_refused("cd ..", &root_cwd());
        assert_refused("cat ../secrets.txt", &root_cwd());
        // depth 1: two levels climbs out
        assert_refused("cat ../../outside.txt", &lesson_cwd());
        // combined climb across a compound command
        assert_refused("cd .. && rm -rf ../important", &lesson_cwd());
        // sneaky mixed form
        assert_refused("cat foo/../../../etc/hostname", &lesson_cwd());
    }

    #[test]
    fn test_refuses_env_var_tricks() {
        let reason = assert_refused("cat $HOME/.ssh/id_rsa", &lesson_cwd());
        assert!(reason.to_lowercase().contains("environment"));
        assert_refused("cd $HOME", &lesson_cwd());
        assert_refused("ls ${HOME}", &lesson_cwd());
        assert_refused("echo $PATH", &lesson_cwd());
    }

    #[test]
    fn test_refuses_command_substitution() {
        assert_refused("cat $(echo /etc/passwd)", &lesson_cwd());
        assert_refused("cat `echo /etc/passwd`", &lesson_cwd());
    }

    #[test]
    fn test_refuses_unbalanced_quotes() {
        assert_refused("cat \"unclosed", &lesson_cwd());
    }

    // --- Catastrophic denylist, refused regardless of path rules ---

    #[test]
    fn test_refuses_rm_rf_root_with_explanation() {
        let reason = assert_refused("rm -rf /", &lesson_cwd());
        assert!(reason.contains("no undo"), "reason: {reason}");
        assert_refused("rm -fr /", &lesson_cwd());
        assert_refused("rm -r -f /", &lesson_cwd());
        assert_refused("rm -rf /*", &lesson_cwd());
        assert_refused("rm --recursive --force /", &lesson_cwd());
    }

    #[test]
    fn test_refuses_mkfs() {
        let reason = assert_refused("mkfs /dev/sda1", &lesson_cwd());
        assert!(reason.contains("format"), "reason: {reason}");
        assert_refused("mkfs.ext4 /dev/sda1", &lesson_cwd());
    }

    #[test]
    fn test_refuses_dd_to_device() {
        let reason = assert_refused("dd if=image.iso of=/dev/sda", &lesson_cwd());
        assert!(reason.contains("raw disk"), "reason: {reason}");
    }

    #[test]
    fn test_refuses_fork_bomb() {
        let reason = assert_refused(":(){ :|:& };:", &lesson_cwd());
        assert!(reason.contains("fork bomb"), "reason: {reason}");
        assert_refused(":(){:|:&};:", &lesson_cwd());
    }

    #[test]
    fn test_refuses_chmod_777_root() {
        let reason = assert_refused("chmod -R 777 /", &lesson_cwd());
        assert!(reason.contains("security"), "reason: {reason}");
    }

    #[test]
    fn test_refuses_redirect_to_dev() {
        let reason = assert_refused("echo garbage > /dev/sda", &lesson_cwd());
        assert!(reason.contains("/dev"), "reason: {reason}");
        assert_refused("cat file >/dev/sdb1", &lesson_cwd());
        assert_refused("ls > /dev/null; echo x >> /dev/sda", &lesson_cwd());
    }

    #[test]
    fn test_allows_dev_null() {
        for ok in ["cat missing.txt 2>/dev/null", "ls > /dev/null", "grep x f >/dev/null 2>&1"] {
            assert!(
                matches!(PlaygroundGuard::check_catastrophic(ok), GuardVerdict::Allow),
                "{ok} should be allowed"
            );
        }
    }

    #[test]
    fn test_refuses_sudo_anything() {
        let reason = assert_refused("sudo ls", &lesson_cwd());
        assert!(reason.contains("administrator"), "reason: {reason}");
        assert_refused("sudo rm -rf /tmp", &lesson_cwd());
        assert_refused("sudo apt install cowsay", &lesson_cwd());
        // sudo hidden behind a separator
        assert_refused("ls; sudo whoami", &lesson_cwd());
        assert_refused("ls && sudo whoami", &lesson_cwd());
        assert_refused("ls | sudo tee /etc/x", &lesson_cwd());
    }

    #[test]
    fn test_empty_command_is_allowed() {
        assert_allowed("", &lesson_cwd());
        assert_allowed("   ", &lesson_cwd());
    }

    #[test]
    fn test_refuses_tilde_path_that_climbs_out() {
        let reason = assert_refused(
            "cat ~/ArcAcademy/playground/../../.ssh/id_rsa",
            &lesson_cwd(),
        );
        assert!(reason.contains("~/ArcAcademy/playground"), "reason: {reason}");
    }

    #[test]
    fn test_allows_tilde_path_inside_root() {
        assert_allowed("ls ~/ArcAcademy/playground", &lesson_cwd());
        assert_allowed("cat ~/ArcAcademy/playground/nav-basics/notes.txt", &lesson_cwd());
    }

    #[test]
    fn test_refuses_tilde_lookalike_prefix() {
        assert_refused("ls ~/ArcAcademy/playground-evil", &lesson_cwd());
    }

    #[test]
    fn test_refuses_destruction_inside_a_chain() {
        let reason = assert_refused("cd docs && rm -rf /", &lesson_cwd());
        assert!(reason.contains("no undo"), "reason: {reason}");
        assert!(!PlaygroundGuard::check_catastrophic("true; rm -r /*").is_allowed());
    }

    #[test]
    fn test_refuses_recursive_delete_of_home() {
        for cmd in ["rm -rf ~", "rm -r ~/", "rm -rf $HOME", "rm -fr ~/*", "rm -R ${HOME}"] {
            let verdict = PlaygroundGuard::check_catastrophic(cmd);
            match verdict {
                GuardVerdict::Refuse { reason } => {
                    assert!(reason.contains("home folder"), "{cmd}: {reason}")
                }
                GuardVerdict::Allow => panic!("{cmd} should be refused"),
            }
        }
    }

    #[test]
    fn test_refuses_rm_r_root_without_force() {
        assert!(!PlaygroundGuard::check_catastrophic("rm -r /").is_allowed());
        assert!(!PlaygroundGuard::check_catastrophic("rm -rf --no-preserve-root /").is_allowed());
    }

    #[test]
    fn test_catastrophic_check_allows_everyday_commands() {
        for cmd in [
            "ls -la /etc",
            "rm notes.txt",
            "rm -r build",
            "cat ~/.zshrc",
            "echo $HOME",
            "grep -r TODO .",
            "chmod 644 index.html",
        ] {
            assert!(
                PlaygroundGuard::check_catastrophic(cmd).is_allowed(),
                "{cmd} should be allowed"
            );
        }
    }

    #[test]
    fn test_messages_name_the_configured_root() {
        let guard = PlaygroundGuard::with_display(root_cwd(), "~/ArcAcademy/home");
        match guard.check("cat /etc/hosts", &root_cwd()) {
            GuardVerdict::Refuse { reason } => {
                assert!(reason.contains("~/ArcAcademy/home"), "reason: {reason}")
            }
            GuardVerdict::Allow => panic!("should be refused"),
        }
    }

    #[test]
    fn test_quotes_do_not_hide_catastrophic_commands() {
        for cmd in [
            // a quoted ; or | used to make the segment fail to tokenize
            "rm -rf ~ ';'",
            "rm -rf / '|'",
            "rm -rf \"$HOME\" '&&'",
            // quoting the program or the target
            "'sudo' ls",
            "true; \"sudo\" ls",
            "dd if=x 'of=/dev/disk2'",
            "echo x > '/dev/disk2'",
            "echo x >> \"/dev/sda\"",
            "'mkfs.ext4' disk.img",
        ] {
            assert!(
                !PlaygroundGuard::check_catastrophic(cmd).is_allowed(),
                "{cmd} should be refused"
            );
        }
        // quoted ; and | in ordinary commands are still fine
        for cmd in ["grep -E 'a|b' notes.txt", "echo 'one; two'", "echo x > /dev/null"] {
            assert!(PlaygroundGuard::check_catastrophic(cmd).is_allowed(), "{cmd} should be allowed");
        }
    }

    #[test]
    fn test_refuses_paths_after_equals() {
        let cwd = lesson_cwd();
        assert_refused("dd if=notes.txt of=/etc/x", &cwd);
        assert_refused("cp notes.txt --target-directory=../..", &cwd);
        assert_refused("tar --file=~/stuff.tar -c notes", &cwd);
        assert_allowed("ls --color=auto", &cwd);
        assert_allowed("sort --output=sorted.txt notes.txt", &cwd);
    }

    #[test]
    fn test_expanded_lines_may_carry_quoted_dollars() {
        let cwd = lesson_cwd();
        // the app's own rendering: $ only inside single quotes, or "$?"
        let line = "echo 'echo $1' > 'greet.sh'";
        assert!(!guard().check(line, &cwd).is_allowed());
        assert!(guard().check_expanded(line, &cwd).is_allowed());
        assert!(guard().check_expanded("false; echo 'status '\"$?\"", &cwd).is_allowed());
        // still contained, still no command substitution
        assert!(!guard().check_expanded("cat '/etc/hosts'", &cwd).is_allowed());
        assert!(!guard().check_expanded("echo '$(date)'", &cwd).is_allowed());
        assert!(!guard().check_expanded("rm -rf '/'", &cwd).is_allowed());
    }

    #[test]
    fn test_expanded_lines_must_keep_the_contract() {
        let cwd = lesson_cwd();
        // hand-made lines a render bug could produce: a $ the shell would fill in
        for line in [
            "echo $HOME",
            "echo \"$HOME\"",
            "echo 'a'$X'b'",
            "echo \\$HOME",
            "echo \"$?x\"",
            "echo \"$? \"",
            "cat ${F}",
            "echo 'ok' $",
        ] {
            assert!(!guard().check_expanded(line, &cwd).is_allowed(), "{line} should be refused");
        }
        for line in ["echo '$HOME'", "echo 'it'\\''s $5'", "false; echo \"$?\"", "echo 'a'\"$?\"'b'"] {
            assert!(guard().check_expanded(line, &cwd).is_allowed(), "{line} should be allowed");
        }
    }
}
