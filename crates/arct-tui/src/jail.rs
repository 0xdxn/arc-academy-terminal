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
//! Neither may the jailed shell ask a program running outside the jail to
//! do the work for it: on Linux the per-user service manager and session
//! bus (their sockets under /run and /tmp aren't there, and there's no
//! shared process table); on macOS launchd, LaunchServices (`open`) and
//! Apple Events (`osascript`), whose services the profile can't look up.
//!
//! A jail is only used after it proves itself: [`Jail::for_root`] runs a
//! self-test (a write inside works, a write outside fails, and asking the
//! platform's service manager to write outside fails too) and returns
//! `None` when any of that fails. Real-files lessons are only offered with
//! a working jail.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Keys, credentials and private stores under the learner's home that
/// jailed commands may not read. One list for both platforms: the macOS
/// profile denies each, bubblewrap covers each that exists (paths that
/// don't exist on a platform are simply absent there).
pub const SECRET_PATHS: &[&str] = &[
    ".ssh",
    ".gnupg",
    ".aws",
    ".arx",
    ".config",
    ".netrc",
    ".docker",
    ".kube",
    ".npmrc",
    "Library/Keychains",
    "Library/Cookies",
    "Library/Mail",
    "Library/Messages",
    "Library/Application Support",
    "Library/Group Containers",
];

/// The services a jailed command may look up on macOS. Everything else is
/// refused, so launchd's job services, LaunchServices, Apple Events, the
/// pasteboard and the rest are unreachable by construction, not by a list
/// of denies. These answer "who is uid 501" (ls -l, whoami), list
/// processes (pkill) and take logging.
const MACH_SERVICES: &[&str] = &[
    "com.apple.system.opendirectoryd.libinfo",
    "com.apple.system.opendirectoryd.membership",
    "com.apple.sysmond",
    "com.apple.system.notification_center",
    "com.apple.system.logger",
    "com.apple.logd",
    "com.apple.diagnosticd",
];

/// Programs whose whole job is to have something outside the jail act:
/// refused outright on macOS (the service denies above already stop them;
/// this says so plainly and holds if a lookup rule ever changes)
const DELEGATING_PROGRAMS: &[&str] = &[
    "/bin/launchctl",
    "/usr/bin/open",
    "/usr/bin/osascript",
    "/usr/bin/at",
    "/usr/bin/batch",
    "/usr/bin/crontab",
    "/usr/bin/shortcuts",
    "/usr/bin/automator",
];

/// Environment variables that point a child at a session bus, display or
/// agent outside the jail. Removed from every jailed command.
const SESSION_ENV: &[&str] = &[
    "DBUS_SESSION_BUS_ADDRESS",
    "XDG_RUNTIME_DIR",
    "DISPLAY",
    "WAYLAND_DISPLAY",
    "SSH_AUTH_SOCK",
    "GPG_AGENT_INFO",
    "TMUX",
    "TMUX_PANE",
];

/// The macOS sandbox profile. `ROOT`, `TMP` and `HOME` are passed with -D,
/// as canonical paths (sandbox rules match real paths, so /var/... has to
/// be /private/var/...). Only the fixed lists above go into its text.
pub fn seatbelt_profile() -> String {
    let mut profile = String::from(
        r#"(version 1)
(allow default)
(deny network*)
(deny job-creation)
(deny lsopen)
(deny appleevent-send)
(deny mach-lookup)
(allow mach-lookup"#,
    );
    for service in MACH_SERVICES {
        profile.push_str(&format!("\n    (global-name \"{service}\")"));
    }
    profile.push_str(")\n(deny process-exec");
    for program in DELEGATING_PROGRAMS {
        profile.push_str(&format!("\n    (literal \"{program}\")"));
    }
    profile.push_str(
        r#")
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
(deny file-read* file-write*"#,
    );
    for secret in SECRET_PATHS {
        profile.push_str(&format!("\n    (subpath (string-append (param \"HOME\") \"/{secret}\"))"));
    }
    profile.push_str(")\n");
    profile
}

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
                args.push(profile_text().into());
                SANDBOX_EXEC.into()
            }
            Kind::Bwrap(bwrap) => {
                for arg in ["--ro-bind", "/", "/", "--dev", "/dev", "--proc", "/proc"] {
                    args.push(arg.into());
                }
                // Empty /run and /tmp: the sockets of everything running
                // outside (the user's service manager and session bus under
                // /run/user, the system bus, X11, tmux, ssh agents) aren't
                // there to connect to. The root and temp folder are bound
                // back in after, even when they live under /tmp.
                for dir in ["/run", "/tmp"] {
                    if weak_for_test() {
                        break;
                    }
                    args.push("--tmpfs".into());
                    args.push(dir.into());
                }
                if let Some(runtime) = std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from) {
                    if runtime.is_absolute() && !runtime.starts_with("/run") && !runtime.starts_with("/tmp") {
                        args.push("--tmpfs".into());
                        args.push(runtime.into());
                    }
                }
                for dir in [&self.root, &self.tmp] {
                    args.push("--bind".into());
                    args.push(dir.into());
                    args.push(dir.into());
                }
                for secret in SECRET_PATHS {
                    let path = self.home.join(secret);
                    match std::fs::symlink_metadata(&path) {
                        Ok(meta) if meta.is_dir() => {
                            args.push("--tmpfs".into());
                            args.push(path.into());
                        }
                        // A file (.netrc) or a link: an empty file over it
                        Ok(_) => {
                            args.push("--ro-bind".into());
                            args.push("/dev/null".into());
                            args.push(path.into());
                        }
                        Err(_) => {}
                    }
                }
                for arg in [
                    "--unshare-net",
                    "--unshare-ipc",
                    "--unshare-pid",
                    "--die-with-parent",
                    "--new-session",
                ] {
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
    /// pointing at the private temp folder and nothing in the environment
    /// that leads to a session bus, display or agent outside
    pub fn command(&self, command: &str) -> Command {
        let (program, args) = self.wrap(command);
        let mut cmd = Command::new(program);
        cmd.args(args).env("TMPDIR", &self.tmp);
        for name in SESSION_ENV {
            cmd.env_remove(name);
        }
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

    /// arct's own temp folder for jailed commands, private to this user.
    /// On a shared machine another user could make the usual name first
    /// (as a link to somewhere of theirs), so it's only used when it is a
    /// real folder owned by this user and closed to everyone else; failing
    /// that a fresh random name is made; failing that, no jail.
    fn private_tmp() -> Option<PathBuf> {
        let base = std::env::temp_dir();
        let usual = base.join(format!("arct-jail-{}", whoami()));
        if let Some(dir) = Self::own_private_dir(&usual) {
            return Some(dir);
        }
        for _ in 0..4 {
            let fresh = base.join(format!("arct-jail-{}", uuid::Uuid::new_v4().simple()));
            if let Some(dir) = Self::own_private_dir(&fresh) {
                return Some(dir);
            }
        }
        None
    }

    /// `dir`, made if missing (never through a link), if it is a real
    /// folder owned by this user with mode 0700
    fn own_private_dir(dir: &Path) -> Option<PathBuf> {
        use std::os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt};
        // create, not create_all: an existing name (folder or link) is left alone
        let _ = std::fs::DirBuilder::new().mode(0o700).create(dir);
        let meta = std::fs::symlink_metadata(dir).ok()?;
        // SAFETY: getuid has no preconditions and cannot fail
        let uid = unsafe { libc::getuid() };
        if !meta.file_type().is_dir() || meta.uid() != uid || meta.permissions().mode() & 0o777 != 0o700 {
            return None;
        }
        dir.canonicalize().ok()
    }

    /// Writes inside the root and the temp folder work; a write to a
    /// folder outside both fails and leaves nothing behind, whether the
    /// shell writes itself or asks the platform's service manager to (and,
    /// on macOS, LaunchServices can't be reached). The outside probe is a
    /// fresh empty folder made for this test, so a jail that doesn't hold
    /// only ever creates one empty file there.
    fn self_test(&self) -> bool {
        let Some(outside) = self.root.parent().map(|p| p.join(format!(".arct-jail-probe-{}", std::process::id()))) else {
            return false;
        };
        if std::fs::create_dir_all(&outside).is_err() {
            return false;
        }
        let inside = self.root.join(format!(".arct-jail-probe-{}", std::process::id()));
        let label = format!("cc.arclabs.arct.jail-probe.{}", std::process::id());
        let line = format!(
            "touch {} && touch \"$TMPDIR/probe\" && touch {} 2>/dev/null; {} echo done",
            sh_quote(&inside),
            sh_quote(&outside.join("escaped")),
            self.delegation_probe(&outside.join("delegated"), &label),
        );
        let ran = self.command(&line).output();
        let wrote_inside = inside.exists();
        let escaped = outside.join("escaped").exists();
        let delegated = outside.join("delegated").exists();
        let stdout = ran.as_ref().map(|out| String::from_utf8_lossy(&out.stdout).into_owned()).unwrap_or_default();
        let reached = stdout.lines().any(|l| l.starts_with("REACHED"));
        if matches!(self.kind, Kind::Seatbelt) && (delegated || reached) {
            // The jail let a job through to launchd: take it back out
            let _ = Command::new("/bin/launchctl").args(["remove", &label]).output();
        }
        let _ = std::fs::remove_file(&inside);
        let _ = std::fs::remove_file(self.tmp.join("probe"));
        let _ = std::fs::remove_file(outside.join("escaped"));
        let _ = std::fs::remove_file(outside.join("delegated"));
        let _ = std::fs::remove_dir(&outside);
        matches!(&ran, Ok(out) if out.status.success())
            && stdout.lines().last() == Some("done")
            && wrote_inside
            && !escaped
            && !delegated
            && !reached
    }

    /// Shell that asks the platform's service manager to create `target`
    /// (one empty file in the probe folder), and prints a REACHED line if
    /// a service outside the jail answered. Harmless if the jail fails:
    /// the most that happens is that empty file.
    fn delegation_probe(&self, target: &Path, label: &str) -> String {
        let target = sh_quote(target);
        match self.kind {
            // launchd runs a submitted job outside any sandbox; LaunchServices
            // answers lsappinfo with the front app's ASN when it is reachable
            Kind::Seatbelt => format!(
                "/bin/launchctl submit -l {label} -- /usr/bin/touch {target} >/dev/null 2>&1 && echo REACHED launchd; \
                 /usr/bin/lsappinfo front 2>/dev/null | grep -q ASN: && echo REACHED launchservices; \
                 sleep 0.3;"
            ),
            // The user's service manager on its bus socket, asked by path so
            // the probe tests the socket, not just the cleared environment
            Kind::Bwrap(_) => format!(
                "uid=$(id -u); test -S /run/user/$uid/bus && echo REACHED session-bus; \
                 DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/$uid/bus timeout 5 systemd-run --user --quiet --collect -- /usr/bin/touch {target} >/dev/null 2>&1 && echo REACHED systemd; \
                 sleep 0.3;"
            ),
        }
    }
}

/// The profile wrap() uses: the real one, except in tests that check a
/// weakened jail fails its self-test
fn profile_text() -> String {
    #[cfg(test)]
    if weak_for_test() {
        return "(version 1)\n(allow default)\n(deny network*)\n(deny file-write*)\n(allow file-write* (subpath (param \"ROOT\")) (subpath (param \"TMP\")) (literal \"/dev/null\"))\n".to_string();
    }
    seatbelt_profile()
}

#[cfg(test)]
thread_local! {
    static WEAK_FOR_TEST: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Tests only: build the jail without its delegation walls
fn weak_for_test() -> bool {
    #[cfg(test)]
    {
        WEAK_FOR_TEST.with(|w| w.get())
    }
    #[cfg(not(test))]
    {
        false
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
        assert!(
            joined.starts_with("--ro-bind / / --dev /dev --proc /proc --tmpfs /run --tmpfs /tmp "),
            "{joined}"
        );
        // the root and temp folder come back in after /tmp is emptied
        let tmpfs_tmp = joined.find("--tmpfs /tmp").unwrap();
        assert!(joined.find("--bind /tmp/arct-jail-u /tmp/arct-jail-u").unwrap() > tmpfs_tmp);
        assert!(joined.contains("--bind /home/u/ArcAcademy/playground /home/u/ArcAcademy/playground"));
        for flag in ["--unshare-net", "--unshare-ipc", "--unshare-pid", "--die-with-parent", "--new-session"] {
            assert!(joined.contains(flag), "{flag} missing: {joined}");
        }
        assert_eq!(args.iter().filter(|a| *a == "--bind").count(), 2);
        assert!(joined.ends_with("/bin/sh -c ls"));
    }

    #[test]
    fn both_platforms_cover_the_same_secrets() {
        let (scratch, root) = scratch("secrets");
        let home = scratch.join("fake-home");
        for secret in SECRET_PATHS {
            let path = home.join(secret);
            if secret.ends_with("rc") {
                std::fs::create_dir_all(path.parent().unwrap()).unwrap();
                std::fs::write(&path, "").unwrap();
            } else {
                std::fs::create_dir_all(&path).unwrap();
            }
        }
        let jail = Jail {
            kind: Kind::Bwrap(PathBuf::from("/usr/bin/bwrap")),
            root,
            tmp: PathBuf::from("/tmp/arct-jail-u"),
            home: home.clone(),
        };
        let (_, args) = jail.wrap("ls");
        let profile = seatbelt_profile();
        for secret in SECRET_PATHS {
            assert!(profile.contains(&format!("\"/{secret}\"")), "macOS profile misses {secret}");
            let path = home.join(secret).into_os_string();
            assert!(args.contains(&path), "bubblewrap misses {secret}");
        }
        for must in [".netrc", ".docker", ".kube", ".npmrc", "Library/Application Support", "Library/Group Containers"] {
            assert!(SECRET_PATHS.contains(&must), "{must}");
        }
        std::fs::remove_dir_all(&scratch).unwrap();
    }

    #[test]
    fn jailed_commands_get_no_session_bus_display_or_agent() {
        let jail = Jail {
            kind: Kind::Bwrap(PathBuf::from("/usr/bin/bwrap")),
            root: PathBuf::from("/r"),
            tmp: PathBuf::from("/t"),
            home: PathBuf::from("/h"),
        };
        let cmd = jail.command("true");
        let removed: Vec<_> = cmd.get_envs().filter(|(_, v)| v.is_none()).map(|(k, _)| k.to_owned()).collect();
        for name in ["DBUS_SESSION_BUS_ADDRESS", "XDG_RUNTIME_DIR", "DISPLAY", "SSH_AUTH_SOCK", "TMUX"] {
            assert!(removed.iter().any(|k| k == name), "{name} not removed");
        }
    }

    #[test]
    fn private_tmp_refuses_a_name_someone_else_made() {
        let (scratch, _) = scratch("tmpname");
        // a link made in advance, the way another user on the machine could
        use std::os::unix::fs::PermissionsExt;
        let target = scratch.join("theirs");
        std::fs::create_dir_all(&target).unwrap();
        std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o755)).unwrap();
        let link = scratch.join("arct-jail-guess");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        assert!(Jail::own_private_dir(&link).is_none());
        // an open folder isn't private either
        let open = scratch.join("open");
        std::fs::create_dir_all(&open).unwrap();
        std::fs::set_permissions(&open, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert!(Jail::own_private_dir(&open).is_none());
        // the link target's mode was left alone
        assert_eq!(std::fs::metadata(&target).unwrap().permissions().mode() & 0o777, 0o755);
        // a fresh name is made private
        let fresh = Jail::own_private_dir(&scratch.join("fresh")).unwrap();
        assert_eq!(std::fs::metadata(&fresh).unwrap().permissions().mode() & 0o777, 0o700);
        assert!(Jail::private_tmp().is_some());
        std::fs::remove_dir_all(&scratch).unwrap();
    }

    /// A jail without its delegation walls must fail the self-test, so a
    /// regression is refused rather than offered. The probes are harmless:
    /// read-only lookups and, at most, one empty file in the probe folder.
    #[test]
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    fn self_test_refuses_a_jail_that_lets_services_act_for_it() {
        let (scratch, root) = scratch("weak");
        let strong = Jail::for_root(&root);
        if strong.is_none() && cfg!(target_os = "linux") {
            eprintln!("no jail on this system; skipping");
            std::fs::remove_dir_all(&scratch).unwrap();
            return;
        }
        let strong = strong.expect("macOS always has sandbox-exec");
        assert!(strong.self_test());
        WEAK_FOR_TEST.with(|w| w.set(true));
        let weak_passed = strong.self_test();
        WEAK_FOR_TEST.with(|w| w.set(false));
        let has_bus = cfg!(target_os = "linux")
            && std::path::Path::new(&format!("/run/user/{}/bus", unsafe { libc::getuid() })).exists();
        if cfg!(target_os = "macos") || has_bus {
            assert!(!weak_passed, "a jail with open service paths passed its self-test");
        }
        std::fs::remove_dir_all(&scratch).unwrap();
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn macos_services_that_act_outside_are_unreachable() {
        let (scratch, root) = scratch("services");
        let jail = Jail::for_root(&root).unwrap();
        // read-only questions; none of these can change anything
        let out = run(&jail, &root, "/usr/bin/lsappinfo front 2>&1; /bin/launchctl version 2>&1; /usr/bin/osascript -e 1 2>&1; /usr/bin/open -h 2>&1; whoami");
        let text = String::from_utf8_lossy(&out.stdout);
        assert!(!text.contains("ASN:"), "LaunchServices answered: {text}");
        assert_eq!(text.matches("Operation not permitted").count(), 3, "{text}");
        // ordinary lesson commands still work, user names included
        assert!(text.trim_end().ends_with(&whoami()), "{text}");
        std::fs::remove_dir_all(&scratch).unwrap();
    }
}
