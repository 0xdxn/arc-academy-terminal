//! The operating system's sandbox around commands arct runs on real files.
//!
//! The safety guard reads a command before it runs and explains what is
//! dangerous; it can't see what a script, a program or a symlink does once
//! it runs. The jail can: the shell and everything it starts may write only
//! inside one folder (the lesson playground, or the free shell's
//! `~/ArcAcademy/home`), a private temp folder and the terminal devices, and
//! may not use the network. Reads stay open (lessons look at real system
//! files), except for the learner's keys and credentials.
//!
//! - macOS: `sandbox-exec` with the profile below.
//! - Linux: `bwrap` (bubblewrap): the whole filesystem read-only, the folder
//!   and the temp folder writable, no network.
//!
//! A jail is only used after it proves itself: [`Jail::for_root`] runs a
//! self-test (a write inside works, a write outside fails, nothing appears
//! outside) and returns `None` when that fails. Real-files lessons are only
//! offered with a working jail.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

/// The macOS sandbox profile. `ROOT`, `TMP` and `HOME` are passed with -D,
/// as canonical paths (sandbox rules match real paths, so /var/... has to
/// be /private/var/...).
pub const SEATBELT_PROFILE: &str = r#"(version 1)
(allow default)
(deny network*)
(deny file-write*)
(allow file-write*
    (subpath (param "ROOT"))
    (subpath (param "TMP"))
    (literal "/dev/null")
    (literal "/dev/zero")
    (literal "/dev/tty")
    (literal "/dev/stdout")
    (literal "/dev/stderr")
    (regex #"^/dev/fd/[0-9]+$")
    (regex #"^/dev/ttys[0-9]+$"))
(deny file-read* file-write*
    (subpath (string-append (param "HOME") "/.ssh"))
    (subpath (string-append (param "HOME") "/.gnupg"))
    (subpath (string-append (param "HOME") "/.aws"))
    (subpath (string-append (param "HOME") "/.arx"))
    (subpath (string-append (param "HOME") "/.config"))
    (subpath (string-append (param "HOME") "/Library/Keychains"))
    (subpath (string-append (param "HOME") "/Library/Cookies"))
    (subpath (string-append (param "HOME") "/Library/Mail"))
    (subpath (string-append (param "HOME") "/Library/Messages")))
"#;

const SANDBOX_EXEC: &str = "/usr/bin/sandbox-exec";

#[derive(Debug, Clone, PartialEq, Eq)]
enum Kind {
    Seatbelt,
    Bwrap(PathBuf),
}

/// A working jail for one folder
#[derive(Debug, Clone)]
pub struct Jail {
    kind: Kind,
    root: PathBuf,
    tmp: PathBuf,
    home: PathBuf,
}

impl Jail {
    /// A jail that lets commands write only inside `root`, proven by a
    /// self-test, or `None` when this system has no sandbox that passes it
    pub fn for_root(root: &Path) -> Option<Jail> {
        let root = root.canonicalize().ok()?;
        let tmp = Self::private_tmp()?;
        let home = dirs::home_dir().and_then(|h| h.canonicalize().ok()).unwrap_or_else(|| PathBuf::from("/nonexistent"));
        Self::candidates()
            .into_iter()
            .map(|kind| Jail { kind, root: root.clone(), tmp: tmp.clone(), home: home.clone() })
            .find(|jail| jail.self_test())
    }

    /// Which sandbox this is, for messages ("sandbox-exec", "bubblewrap")
    pub fn name(&self) -> &'static str {
        match self.kind {
            Kind::Seatbelt => "sandbox-exec",
            Kind::Bwrap(_) => "bubblewrap",
        }
    }

    /// The folder commands may write to
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The private temp folder, for the child's TMPDIR
    pub fn tmp(&self) -> &Path {
        &self.tmp
    }

    /// Program and arguments that run `sh -c <command>` inside the jail
    pub fn wrap(&self, command: &str) -> (OsString, Vec<OsString>) {
        let mut args: Vec<OsString> = Vec::new();
        let program: OsString = match &self.kind {
            Kind::Seatbelt => {
                for (name, value) in [("ROOT", &self.root), ("TMP", &self.tmp), ("HOME", &self.home)] {
                    args.push("-D".into());
                    let mut define = OsString::from(format!("{name}="));
                    define.push(value.as_os_str());
                    args.push(define);
                }
                args.push("-p".into());
                args.push(SEATBELT_PROFILE.into());
                SANDBOX_EXEC.into()
            }
            Kind::Bwrap(bwrap) => {
                for arg in ["--ro-bind", "/", "/", "--dev", "/dev", "--proc", "/proc"] {
                    args.push(arg.into());
                }
                for dir in [&self.root, &self.tmp] {
                    args.push("--bind".into());
                    args.push(dir.into());
                    args.push(dir.into());
                }
                for secret in [".ssh", ".gnupg", ".aws", ".arx", ".config"] {
                    let path = self.home.join(secret);
                    if path.exists() {
                        args.push("--tmpfs".into());
                        args.push(path.into());
                    }
                }
                for arg in ["--unshare-net", "--unshare-ipc", "--die-with-parent", "--new-session"] {
                    args.push(arg.into());
                }
                bwrap.into()
            }
        };
        args.push("/bin/sh".into());
        args.push("-c".into());
        args.push(command.into());
        (program, args)
    }

    /// A std Command for `sh -c <command>` inside the jail, with TMPDIR
    /// pointing at the private temp folder
    pub fn command(&self, command: &str) -> Command {
        let (program, args) = self.wrap(command);
        let mut cmd = Command::new(program);
        cmd.args(args).env("TMPDIR", &self.tmp);
        cmd
    }

    fn candidates() -> Vec<Kind> {
        let mut kinds = Vec::new();
        if cfg!(target_os = "macos") && Path::new(SANDBOX_EXEC).exists() {
            kinds.push(Kind::Seatbelt);
        }
        if cfg!(target_os = "linux") {
            for dir in ["/usr/bin", "/bin", "/usr/local/bin"] {
                let bwrap = Path::new(dir).join("bwrap");
                if bwrap.exists() {
                    kinds.push(Kind::Bwrap(bwrap));
                    break;
                }
            }
        }
        kinds
    }

    /// arct's own temp folder for jailed commands, private to this user
    fn private_tmp() -> Option<PathBuf> {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("arct-jail-{}", whoami()));
        std::fs::create_dir_all(&dir).ok()?;
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).ok()?;
        dir.canonicalize().ok()
    }

    /// Writes inside the root and the temp folder work; a write to a
    /// folder outside both fails and leaves nothing behind. The outside
    /// probe is a fresh empty folder made for this test, so a jail that
    /// doesn't hold only ever creates one empty file there.
    fn self_test(&self) -> bool {
        let Some(outside) = self.root.parent().map(|p| p.join(format!(".arct-jail-probe-{}", std::process::id()))) else {
            return false;
        };
        if std::fs::create_dir_all(&outside).is_err() {
            return false;
        }
        let inside = self.root.join(format!(".arct-jail-probe-{}", std::process::id()));
        let line = format!(
            "touch {} && touch \"$TMPDIR/probe\" && touch {} 2>/dev/null; echo done",
            sh_quote(&inside),
            sh_quote(&outside.join("escaped")),
        );
        let ran = self.command(&line).output();
        let wrote_inside = inside.exists();
        let escaped = outside.join("escaped").exists();
        let _ = std::fs::remove_file(&inside);
        let _ = std::fs::remove_file(self.tmp.join("probe"));
        let _ = std::fs::remove_file(outside.join("escaped"));
        let _ = std::fs::remove_dir(&outside);
        matches!(ran, Ok(out) if out.status.success() && String::from_utf8_lossy(&out.stdout).trim() == "done")
            && wrote_inside
            && !escaped
    }
}

fn whoami() -> String {
    std::env::var("USER").unwrap_or_else(|_| "user".to_string())
}

fn sh_quote(path: &Path) -> String {
    format!("'{}'", path.display().to_string().replace('\'', r"'\''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A playground root inside a scratch folder whose other contents stand
    /// in for "everything outside". Every probe below only creates or reads
    /// files in that scratch folder, so if a jail ever fails to hold, the
    /// worst outcome is a stray file in a temp folder (negjr11u).
    fn scratch(name: &str) -> (PathBuf, PathBuf) {
        let scratch = std::env::temp_dir()
            .join(format!("arct-jail-test-{}-{name}", std::process::id()))
            .canonicalize()
            .unwrap_or_else(|_| {
                let p = std::env::temp_dir().join(format!("arct-jail-test-{}-{name}", std::process::id()));
                std::fs::create_dir_all(&p).unwrap();
                p.canonicalize().unwrap()
            });
        let root = scratch.join("home/ArcAcademy/playground");
        std::fs::create_dir_all(root.join("lesson/sub")).unwrap();
        (scratch, root)
    }

    fn run(jail: &Jail, cwd: &Path, line: &str) -> std::process::Output {
        jail.command(line).current_dir(cwd).output().unwrap()
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn sandbox_exec_jail_is_found_and_proven() {
        let (scratch, root) = scratch("found");
        let jail = Jail::for_root(&root).expect("sandbox-exec passes its self-test on macOS");
        assert_eq!(jail.name(), "sandbox-exec");
        std::fs::remove_dir_all(&scratch).unwrap();
    }

    #[test]
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    fn writes_outside_the_root_fail_whatever_the_guard_misses() {
        let (scratch, root) = scratch("escapes");
        let jail = match Jail::for_root(&root) {
            Some(jail) => jail,
            None if cfg!(target_os = "macos") => panic!("macOS always has sandbox-exec"),
            None => {
                eprintln!("no jail on this system; skipping");
                return;
            }
        };
        let lesson = root.join("lesson");
        let sub = lesson.join("sub");
        let outside = scratch.join("home");

        // inside works
        let out = run(&jail, &lesson, "echo hi > inside.txt && cat inside.txt");
        assert_eq!(String::from_utf8_lossy(&out.stdout), "hi\n");

        // a script that climbs out (the guard reads only the typed line)
        std::fs::write(lesson.join("x.sh"), "cd ..; cd ..; touch escaped-by-script\n").unwrap();
        run(&jail, &lesson, "sh x.sh");
        assert!(!outside.join("ArcAcademy/escaped-by-script").exists());

        // bash 3.2 globs reaching .. (the guard counts .[.] as a name)
        run(&jail, &root, "touch .[.]/escaped-by-glob");
        assert!(!scratch.join("home/ArcAcademy/escaped-by-glob").exists());

        // a symlink chain out of the root, then a write through it
        run(&jail, &sub, "ln -s ../.. x; mv x ../..");
        run(&jail, &root, "touch x/escaped-by-link");
        assert!(!scratch.join("home/escaped-by-link").exists());
        std::os::unix::fs::symlink(&outside, root.join("y")).unwrap();
        run(&jail, &root, "touch y/escaped-by-made-link");
        assert!(!outside.join("escaped-by-made-link").exists());

        std::fs::remove_dir_all(&scratch).unwrap();
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn network_and_secrets_are_closed() {
        let (scratch, root) = scratch("net");
        let jail = Jail::for_root(&root).unwrap();
        // loopback only: a refused connection would say "refused", a jailed one isn't allowed to try
        let out = run(&jail, &root, "/usr/bin/nc -v -w 1 127.0.0.1 9 2>&1");
        let text = String::from_utf8_lossy(&out.stdout);
        assert!(text.contains("Operation not permitted"), "network should be denied, not refused: {text}");
        // the learner's keys can't be read, even though other reads are open
        let out = run(&jail, &root, "ls \"$HOME/.ssh\" >/dev/null 2>&1 && echo readable || echo closed; ls /usr/bin >/dev/null && echo reads-ok");
        let text = String::from_utf8_lossy(&out.stdout);
        if jail.home.join(".ssh").exists() {
            assert!(text.contains("closed"), "{text}");
        }
        assert!(text.contains("reads-ok"), "{text}");
        std::fs::remove_dir_all(&scratch).unwrap();
    }

    #[test]
    fn bwrap_arguments_bind_only_the_root_and_tmp() {
        let jail = Jail {
            kind: Kind::Bwrap(PathBuf::from("/usr/bin/bwrap")),
            root: PathBuf::from("/home/u/ArcAcademy/playground"),
            tmp: PathBuf::from("/tmp/arct-jail-u"),
            home: PathBuf::from("/nonexistent-home"),
        };
        let (program, args) = jail.wrap("ls");
        assert_eq!(program, "/usr/bin/bwrap");
        let args: Vec<String> = args.iter().map(|a| a.to_string_lossy().into_owned()).collect();
        let joined = args.join(" ");
        assert!(joined.starts_with("--ro-bind / / --dev /dev --proc /proc"), "{joined}");
        assert!(joined.contains("--bind /home/u/ArcAcademy/playground /home/u/ArcAcademy/playground"));
        assert!(joined.contains("--bind /tmp/arct-jail-u /tmp/arct-jail-u"));
        assert!(joined.contains("--unshare-net"));
        assert_eq!(args.iter().filter(|a| *a == "--bind").count(), 2);
        assert!(joined.ends_with("/bin/sh -c ls"));
    }
}
