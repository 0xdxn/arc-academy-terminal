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
        if trimmed.contains('$') {
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
        let mut total_climb: usize = 0;
        for raw_token in &tokens {
            // Strip redirect prefixes so `>/etc/passwd` or `2>/tmp/x` are
            // analyzed as paths
            let token = {
                let digits_stripped = raw_token.trim_start_matches(|c: char| c.is_ascii_digit());
                if digits_stripped.starts_with(['<', '>', '&']) {
                    digits_stripped.trim_start_matches(['<', '>', '&'])
                } else {
                    raw_token.as_str()
                }
            };
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

        // sudo anywhere in the pipeline/sequence
        for segment in command.split(|c| c == ';' || c == '|' || c == '&') {
            if segment.split_whitespace().next() == Some("sudo") {
                return Some(GuardVerdict::refuse(
                    "'sudo' runs a command with full administrator power, where a single \
                     typo can change or delete critical system files. arct never needs \
                     admin rights, so sudo is blocked here; when you truly need it, use \
                     your own terminal and read the command twice first.",
                ));
            }
        }

        // mkfs formats disks
        if command
            .split_whitespace()
            .any(|t| t == "mkfs" || t.starts_with("mkfs."))
        {
            return Some(GuardVerdict::refuse(
                "'mkfs' formats a disk partition, instantly erasing every file on it. \
                 It's used to prepare brand-new drives — never something to practice with.",
            ));
        }

        // dd writing to a raw device
        if command.split_whitespace().any(|t| t.starts_with("of=/dev/")) {
            return Some(GuardVerdict::refuse(
                "'dd' writing to a /dev device bypasses the filesystem and overwrites the \
                 raw disk underneath your files — it can wipe an entire drive in seconds \
                 with no confirmation and no undo.",
            ));
        }

        // Redirecting output into a device (e.g. `> /dev/sda`). /dev/null,
        // the "throw it away" file, and the terminal's own streams are fine
        const HARMLESS: &[&str] = &["/dev/null", "/dev/stdout", "/dev/stderr", "/dev/tty"];
        let into_device = regex::Regex::new(r">>?\s*(/dev/[^\s;|&<>]*)").expect("valid pattern");
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
        for segment in command.split(|c| c == ';' || c == '|' || c == '&') {
            if let Some(verdict) = Self::check_segment(segment.trim()) {
                return Some(verdict);
            }
        }

        None
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
}
