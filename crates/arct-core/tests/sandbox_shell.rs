//! The simulated sandbox's shell, run through arct-core with no user
//! interface, on both backends: every test runs once with the practice
//! files on disk and once in memory. The disk sandboxes live in private
//! temp folders this test makes; nothing else is touched.

use arct_core::{CommandAnalyzer, Marks, Sandbox, ShellState, VirtualFileSystem};

/// A shell with its practice files, as a host keeps them
struct Rig {
    shell: ShellState,
    vfs: VirtualFileSystem,
    analyzer: CommandAnalyzer,
    marks: Marks,
}

impl Rig {
    fn run(&mut self, line: &str) -> String {
        Sandbox::new(&mut self.shell, &mut self.vfs, &self.analyzer, &self.marks).run_line(line)
    }

    fn read(&self, path: &str) -> String {
        self.vfs.read_file(path).unwrap()
    }

    fn failed(&self, output: &str) -> bool {
        self.marks.reports_failure(output)
    }
}

fn rig(vfs: VirtualFileSystem) -> Rig {
    Rig {
        shell: ShellState::new(),
        vfs,
        analyzer: CommandAnalyzer::new(),
        marks: Marks::plain(),
    }
}

/// The same empty sandbox on disk and in memory
fn both(name: &str) -> [Rig; 2] {
    let session = format!("shell-{name}-{}", std::process::id());
    [
        rig(VirtualFileSystem::empty("sandbox-test", &session).unwrap()),
        rig(VirtualFileSystem::in_memory()),
    ]
}

#[test]
fn variables_expand_when_each_command_runs() {
    for mut app in both("vars") {
        let app = &mut app;
        assert_eq!(app.run("NAME=Ada; echo hi $NAME"), "hi Ada\n");
        assert_eq!(app.run("echo '$NAME' \"$NAME\""), "$NAME Ada\n");
        assert_eq!(app.run("echo $HOME"), "/lesson-home\n");
        // Not exported yet, so env doesn't show it
        assert!(!app.run("env").contains("NAME="));
        app.run("export NAME");
        assert!(app.run("env").contains("NAME=Ada"));
        assert_eq!(app.run("printenv NAME"), "Ada\n");
        app.run("unset NAME");
        assert_eq!(app.run("echo [$NAME]"), "[]\n");
        // A prefix assignment lasts for one command
        app.run("GREETING=hey env > env.txt");
        assert!(app.read("env.txt").contains("GREETING=hey"), "in that command's environment");
        assert_eq!(app.run("echo [$GREETING]"), "[]\n");
        assert_eq!(app.shell.var("HOME"), Some("/lesson-home"));
    }
}

#[test]
fn redirection_for_any_command() {
    for mut app in both("redirect") {
        let app = &mut app;
        app.run("mkdir logs");
        app.run("echo one > logs/a.txt");
        app.run("echo two >> logs/a.txt");
        assert_eq!(app.read("logs/a.txt"), "one\ntwo\n");

        let out = app.run("ls logs > list.txt");
        assert!(out.contains("Wrote list.txt"));
        assert_eq!(app.read("list.txt").trim(), "a.txt");

        // Errors stay on screen unless sent elsewhere
        let out = app.run("cat nothing.txt > out.txt");
        assert!(app.failed(&out));
        let out = app.run("cat nothing.txt 2> errors.txt");
        assert!(!app.failed(&out), "{out}");
        assert!(app.read("errors.txt").contains("nothing.txt"));
        let out = app.run("cat nothing.txt 2>/dev/null");
        assert!(!out.contains("nothing.txt"), "{out}");
        app.run("cat logs/a.txt nothing.txt > both.txt 2>&1");
        let both = app.read("both.txt");
        assert!(both.contains("one") && both.contains("nothing.txt"), "{both}");

        // Input from a file
        assert_eq!(app.run("wc -l < logs/a.txt"), "2\n");
        assert_eq!(app.run("sort -r < logs/a.txt"), "two\none\n");
        // A quoted > is just text
        assert_eq!(app.run("echo '>' x"), "> x\n");
    }
}

#[test]
fn lists_and_status() {
    for mut app in both("lists") {
        let app = &mut app;
        app.run("mkdir camp && cd camp && touch tent.txt");
        assert!(app.vfs.get_current_dir().ends_with("camp"));
        let out = app.run("cat missing.txt && echo never");
        assert!(!out.contains("never"));
        assert_eq!(app.run("echo $?"), "1\n");
        assert_eq!(app.shell.status(), 0, "echo worked");
        assert!(app.run("cat missing.txt || echo fallback").contains("fallback"));
        assert_eq!(app.run("echo $?"), "0\n");
        let out = app.run("ls &&");
        assert!(app.failed(&out));
    }
}

#[test]
fn pipes_tee_sed_tr() {
    for mut app in both("pipes") {
        let app = &mut app;
        app.run("echo 'b apple' > f.txt; echo 'a pear' >> f.txt");
        assert_eq!(app.run("cat f.txt | sort | tr a-z A-Z"), "A PEAR\nB APPLE\n");
        assert_eq!(app.run("cat f.txt | tee copy.txt | wc -l"), "2\n");
        assert_eq!(app.read("copy.txt"), "b apple\na pear\n");
        assert_eq!(app.run("sed 's/apple/plum/' f.txt"), "b plum\na pear\n");
        assert_eq!(app.run("tail -n +2 f.txt"), "a pear\n");
        assert_eq!(app.run("cat f.txt | tail -n +2"), "a pear\n");
        app.run("sed -i 's/pear/fig/' f.txt");
        assert_eq!(app.read("f.txt"), "b apple\na fig\n");
        assert_eq!(app.run("grep fig < f.txt"), "a fig\n");
        // Any command can follow a pipe (it just doesn't read the text)
        assert_eq!(app.run("cat f.txt | echo hi"), "hi\n");
    }
}

#[test]
fn globs_expand_against_the_sandbox() {
    for mut app in both("globs") {
        let app = &mut app;
        app.run("mkdir notes; touch notes/a.txt notes/b.txt notes/c.md notes/.secret.txt");
        assert_eq!(app.run("ls notes/*.txt"), app.run("ls notes/a.txt notes/b.txt"));
        assert_eq!(app.run("echo notes/.*"), "notes/.secret.txt\n");
        app.run("rm notes/*.txt");
        assert_eq!(app.run("find notes -type f"), "notes/.secret.txt\nnotes/c.md\n");
        // Quoted, find gets the pattern itself
        app.run("touch notes/d.md");
        assert_eq!(app.run("find notes -name '*.md'").lines().count(), 2);
        // No match: the word stays as typed (and the command says so)
        let out = app.run("cat *.nothing");
        assert!(app.failed(&out));
    }
}

#[test]
fn scripts_take_arguments_and_path_finds_them() {
    for mut app in both("scripts") {
        let app = &mut app;
        app.run("mkdir bin");
        app.run("echo 'echo Hello, $1! You gave me $# words.' > bin/greet");
        assert_eq!(app.run("sh bin/greet Ada Bo"), "Hello, Ada! You gave me 2 words.\n");
        // Not on PATH yet, and not runnable by path until chmod +x
        let out = app.run("greet Ada");
        assert!(app.failed(&out));
        assert!(app.run("./bin/greet").contains("permission denied"));
        app.run("chmod +x bin/greet");
        // (the rest of the mode follows the umask on disk)
        assert!(app.run("ls -l bin").starts_with("-rwx"));
        app.run("export PATH=$HOME/bin:$PATH");
        assert_eq!(app.run("greet Ada"), "Hello, Ada! You gave me 1 words.\n");
        assert_eq!(app.run("./bin/greet Bo"), "Hello, Bo! You gave me 1 words.\n");
        assert_eq!(app.run("which greet"), "/lesson-home/bin/greet\n");
        assert_eq!(app.run("type cd"), "cd is a shell builtin\n");

        // exit stops a script; if/for say they need a real shell
        app.run("echo 'echo one' > s.sh; echo 'exit' >> s.sh; echo 'echo two' >> s.sh");
        assert_eq!(app.run("sh s.sh"), "one\n");
        assert!(app.run("for x in a b").contains("real shell"));
    }
}

#[test]
fn shell_fidelity() {
    for mut app in both("fidelity") {
        let app = &mut app;
        // NAME=value cmd: in that command's environment only
        assert!(app.run("GREETING=hey env").contains("GREETING=hey"));
        assert!(!app.run("env").contains("GREETING"));
        // Scripts see exported variables only, and can't change ours
        app.run("echo 'echo [$SECRET] [$SHARED]; SHARED=changed' > s.sh");
        app.run("SECRET=x; export SHARED=y");
        assert_eq!(app.run("sh s.sh"), "[] [y]\n");
        assert_eq!(app.run("echo $SHARED"), "y\n");
        // Word splitting and empty values
        app.run("FILES='a.txt b.txt'; touch $FILES");
        assert!(app.vfs.is_file("b.txt"));
        // Hidden errors still set $?
        assert_eq!(app.run("cat nope 2>/dev/null; echo $?"), "1\n");
        // > empties its file before the command runs
        app.run("echo b > x; echo a >> x; sort x > x");
        assert_eq!(app.read("x"), "");
        // Several files into a folder, never over each other
        app.run("mkdir box; touch c d");
        app.run("mv c d box");
        assert!(app.vfs.is_file("box/d"));
        let out = app.run("cp a.txt b.txt x");
        assert!(app.failed(&out));
        // mkdir -p is fine when the folder exists; ls -a shows . and ..
        let out = app.run("mkdir -p box");
        assert!(!app.failed(&out));
        assert!(app.run("ls -a box").starts_with("./\n../\n"));
        assert!(!app.run("ls -A box").contains("./"));
        // ls -l shows real modes
        app.run("chmod 600 box/d");
        assert!(app.run("ls -l box").contains("-rw-------"));
        assert!(app.run("ls -l box/d").starts_with("-rw-------"));
    }
}

#[test]
fn guard_checks_the_expanded_command() {
    for mut app in both("guard") {
        let app = &mut app;
        app.run("touch keep.txt");
        let mut sandbox = Sandbox::new(&mut app.shell, &mut app.vfs, &app.analyzer, &app.marks);
        let out = sandbox.run_line("X=/; rm -rf $X");
        assert!(out.contains("Blocked"), "{out}");
        assert_eq!(sandbox.guard_refusals(), 1);
        assert!(app.vfs.is_file("keep.txt"));
    }
}

#[test]
fn archives_keys_du_diff() {
    for mut app in both("tools") {
        let app = &mut app;
        app.run("mkdir -p proj/src; echo hi > proj/src/a.txt");
        app.run("tar -czf proj.tar.gz proj");
        assert!(app.run("tar -tzf proj.tar.gz").contains("proj/src/a.txt"));
        app.run("mkdir out; tar -xzf proj.tar.gz -C out");
        assert_eq!(app.read("out/proj/src/a.txt"), "hi\n");
        app.run("tar czf two.tgz proj/src");
        assert!(app.run("tar tf two.tgz").contains("proj/src/a.txt"));

        let out = app.run("ssh-keygen -t ed25519 -C me@laptop -N ''");
        assert!(out.contains(".ssh/id_ed25519.pub"), "{out}");
        assert!(app.read(".ssh/id_ed25519.pub").ends_with("me@laptop\n"));
        assert!(app.run("ls -l .ssh").contains("-rw-------"));

        assert!(app.run("du -sh proj").ends_with("\tproj\n"));
        app.run("echo hi > a; echo ho > b");
        assert_eq!(app.run("diff a b"), "1c1\n< hi\n---\n> ho\n");
        // tar's f takes the rest of a bundle as the name; gzip -k keeps
        app.run("tar -cfz proj");
        assert!(app.vfs.exists("z"));
        app.run("gzip -k a");
        assert_eq!(app.read("a"), "hi\n");
        assert!(app.vfs.exists("a.gz"));
        // diff exits 1 when the files differ, grep when nothing matched
        assert!(!app.run("diff a b > /dev/null && echo same").contains("same"));
        assert!(app.run("cp a c; diff a c && echo same").contains("same"));
        assert!(app.run("grep zzz a || echo none").contains("none"));
        assert!(app.run("cat a | grep zzz || echo none").contains("none"));
        assert_eq!(app.run("grep hi a > /dev/null; echo $?"), "0\n");
    }
}

/// What the backends may print differently: the operating system's error
/// numbers. (Folder sizes in `ls -l` and the modes of new files differ
/// too: the disk reports its own, and follows the umask; the session
/// below doesn't show those.)
fn comparable(output: &str) -> String {
    let mut out = output.to_string();
    while let Some(start) = out.find(" (os error ") {
        match out[start..].find(')') {
            Some(end) => out.replace_range(start..start + end + 1, ""),
            None => break,
        }
    }
    out
}

/// One long session, typed into both backends: every line must print the
/// same and leave the same files
#[test]
fn disk_and_memory_print_the_same() {
    let lines = [
        "pwd",
        "ls",
        "ls -a",
        "mkdir -p station/bridge station/cargo",
        "mkdir station",
        "mkdir nowhere/deeper",
        "echo 'captain was here' > station/bridge/log.txt",
        "echo 'crate 7' > station/cargo/c7.txt; echo 'crate 12' > station/cargo/c12.txt",
        "printf x > nope",
        "echo > missing/folder/file.txt",
        "ls station",
        "ls station/cargo/*.txt",
        // New files on disk take the umask's mode; set it to compare
        "chmod 644 station/cargo/c7.txt station/cargo/c12.txt; ls -l station/cargo",
        "cat station/bridge/log.txt station/none.txt",
        "cat station",
        "cd station/cargo && pwd",
        "cd ../..; pwd",
        "cd nowhere",
        "cd station/bridge/log.txt",
        "touch .hidden visible",
        "ls -a",
        "ls -A",
        "cp station/bridge/log.txt station/cargo",
        "cp station station2",
        "cp -r station station2",
        "cp -r station station/inner",
        "find station2",
        "find . -name '*.txt' -type f",
        "find station -maxdepth 1",
        "mv visible station2/bridge",
        "mv station2/bridge station2/cargo",
        "mv station2 station2/cargo/deeper",
        "mv a b c",
        "rm station2",
        "rm -r station2",
        "rm -f no-such-file",
        "rm no-such-file",
        "ls",
        "echo one two three | wc -w",
        "head -n 1 station/cargo/c7.txt",
        "tail -1 station/cargo/c12.txt",
        "grep -n crate station/cargo/c7.txt station/cargo/c12.txt",
        "grep zzz station/cargo/c7.txt; echo $?",
        "sort station/cargo/c12.txt station/cargo/c7.txt | uniq",
        "du station",
        "du -s station",
        "du -sh .",
        "tar -cvf s.tar station",
        "tar -tf s.tar",
        "mkdir unpack; tar -xvf s.tar -C unpack",
        "gzip station/cargo/c7.txt; ls station/cargo",
        "gunzip station/cargo/c7.txt.gz; cat station/cargo/c7.txt",
        "gzip nope.txt",
        "cat s.tar > /dev/null",
        "echo 'echo hello $1' > hi.sh",
        "./hi.sh you",
        "chmod +x hi.sh; ./hi.sh you",
        "chmod 640 hi.sh; ls -l hi.sh",
        "chmod u+x hi.sh; ls -l hi.sh",
        "chmod g+w hi.sh",
        "sh hi.sh there",
        "mkdir -p tools; mv hi.sh tools/hi; chmod +x tools/hi",
        "PATH=$HOME/tools:$PATH; export PATH; hi path",
        "which hi ls nothing",
        "ssh-keygen -f keys/id -C me",
        "ssh-keygen -f keys/id -C me",
        "chmod 644 keys/id.pub; ls -l keys",
        "diff station/cargo/c7.txt station/cargo/c12.txt",
        "diff station/cargo/c7.txt",
        "sed -n '1p' station/bridge/log.txt",
        "cat ../../etc/hostname",
        "ls /lesson-home/station/../..",
        "rm -rf /",
        "tree",
        "frobnicate",
        "ping example.com",
    ];
    let session = format!("parity-{}", std::process::id());
    let mut disk = rig(VirtualFileSystem::new("parity", &session).unwrap());
    let memory_vfs = VirtualFileSystem::in_memory();
    memory_vfs.seed_generic().unwrap();
    let mut memory = rig(memory_vfs);
    for line in lines {
        let (a, b) = (disk.run(line), memory.run(line));
        assert_eq!(comparable(&a), comparable(&b), "the backends differ on: {line}");
        assert_eq!(disk.shell.status(), memory.shell.status(), "status differs on: {line}");
        assert_eq!(disk.vfs.get_current_dir(), memory.vfs.get_current_dir());
    }
    // And the same files are left behind, with the same contents
    let mut found = Vec::new();
    for rig in [&disk, &memory] {
        let paths = rig.vfs.find(".", &Default::default()).unwrap();
        let contents: Vec<(String, Option<Vec<u8>>)> = paths
            .iter()
            .filter(|p| !p.ends_with(".tar") && !p.ends_with(".gz"))
            .map(|p| (p.clone(), rig.vfs.is_file(p).then(|| rig.vfs.read_bytes(p).unwrap())))
            .collect();
        found.push(contents);
    }
    assert_eq!(found[0], found[1]);
}
