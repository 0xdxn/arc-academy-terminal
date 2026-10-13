//! The simulated sandbox's file and text commands (`ls`, `cat`, `mkdir`,
//! `cp`, `grep`, `chmod`, ...), run against the practice files. Pipes,
//! redirection, variables and scripts are handled before this point by
//! the sandbox shell (`sandbox.rs`).

use crate::command::{Command, CommandCategory};
use crate::sandbox::Sandbox;
use crate::text_filter;
use crate::virtual_fs::{DirEntry, Meta, VirtualFileSystem};

impl Sandbox<'_> {
    /// Run one parsed command against the practice files and return what
    /// it printed
    pub fn run_command(&mut self, cmd: &Command) -> String {
        self.command_output(cmd)
            .unwrap_or_else(|| self.error(format!("{}: not available in the sandbox", cmd.program)))
    }

    fn command_output(&mut self, cmd: &Command) -> Option<String> {
        let marks = self.marks;
        let vfs: &mut VirtualFileSystem = self.vfs;
        let program = cmd.program.as_str();

        // Operands are the non-flag arguments: `rm -rf notes` removes
        // "notes", never a file called "-rf"
        let operands: Vec<String> = cmd
            .args
            .iter()
            .filter(|a| !(a.starts_with('-') && a.len() > 1))
            .cloned()
            .collect();

        match program {
            "pwd" => {
                let current = vfs.get_current_dir().display().to_string();
                Some(format!("{}\n", current))
            }

            "ls" => {
                let show_hidden = has_flag(cmd, "-a") || has_flag(cmd, "--all");
                let long = has_flag(cmd, "-l");
                let human = has_flag(cmd, "-h");
                let size_text = |bytes: u64| -> String {
                    if human && bytes >= 1024 {
                        format!("{:.1}K", bytes as f64 / 1024.0)
                    } else {
                        bytes.to_string()
                    }
                };

                let targets: Vec<Option<&str>> = if operands.is_empty() {
                    vec![None]
                } else {
                    operands.iter().map(|o| Some(o.as_str())).collect()
                };
                let mut output = String::new();
                for target in &targets {
                    // `ls file` prints the file itself
                    if let Some(name) = target {
                        match vfs.resolve_path(name).map(|_| vfs.metadata(name)) {
                            Ok(Some(meta)) if !meta.is_dir => {
                                let size = meta.len;
                                if long {
                                    output.push_str(&format!("{}  {:>6}  {}\n", mode_string(Some(meta)), size_text(size), name));
                                } else {
                                    output.push_str(&format!("{}\n", name));
                                }
                                continue;
                            }
                            Ok(None) => {
                                output.push_str(&format!(
                                    "{}ls: {}: No such file or directory\n",
                                    marks.error,
                                    name
                                ));
                                continue;
                            }
                            Err(e) => {
                                output.push_str(&format!("{}ls: {}\n", marks.error, e));
                                continue;
                            }
                            _ => {}
                        }
                    }
                    if targets.len() > 1 {
                        output.push_str(&format!("{}:\n", target.unwrap_or(".")));
                    }
                    match vfs.list_directory(*target) {
                        Ok(mut entries) => {
                            if !show_hidden {
                                entries.retain(|e| !e.name.starts_with('.'));
                            }
                            // -a also lists . (this folder) and .. (the one above); -A doesn't
                            if show_hidden && !has_flag(cmd, "-A") {
                                for dot in ["..", "."] {
                                    entries.insert(0, DirEntry { name: dot.to_string(), is_dir: true, size: 0 });
                                }
                            }
                            if entries.is_empty() {
                                output.push_str("(empty folder)\n");
                            }
                            for entry in entries {
                                let shown = if entry.is_dir {
                                    format!("{}/", entry.name)
                                } else {
                                    entry.name.clone()
                                };
                                if long {
                                    let meta = vfs.metadata(&match target {
                                        Some(dir) => format!("{}/{}", dir.trim_end_matches('/'), entry.name),
                                        None => entry.name.clone(),
                                    });
                                    let mode = mode_string(meta);
                                    output.push_str(&format!("{}  {:>6}  {}\n", mode, size_text(entry.size), shown));
                                } else {
                                    output.push_str(&format!("{}\n", shown));
                                }
                            }
                        }
                        Err(e) => output.push_str(&format!("{}ls: {}\n", marks.error, e)),
                    }
                }
                Some(output)
            }

            "cd" => {
                let target = cmd.args.first().map(|s| s.as_str()).unwrap_or("~");
                match vfs.change_directory(target) {
                    Ok(new_path) => {
                        Some(format!("{}Changed to: {}\n", marks.folder, new_path))
                    }
                    Err(e) => Some(format!("{}cd: {}\n", marks.error, e)),
                }
            }

            "cat" => {
                if operands.is_empty() {
                    return Some(format!("{}cat: missing file argument\n", marks.error));
                }

                let mut output = String::new();
                for file in &operands {
                    match vfs.read_file(file) {
                        Ok(content) => output.push_str(&content),
                        Err(e) => output.push_str(&format!("{}cat: {}\n", marks.error, e)),
                    }
                }
                Some(output)
            }

            "mkdir" => {
                if operands.is_empty() {
                    return Some(format!("{}mkdir: missing directory name\n", marks.error));
                }

                let parents = has_flag(cmd, "-p") || has_flag(cmd, "--parents");
                let mut output = String::new();

                for dir in &operands {
                    match vfs.create_directory(dir, parents) {
                        Ok(_) => output.push_str(&format!("{}Created directory: {}\n", marks.folder, dir)),
                        Err(e) => output.push_str(&format!("{}mkdir: {}\n", marks.error, e)),
                    }
                }
                Some(output)
            }

            "touch" => {
                if operands.is_empty() {
                    return Some(format!("{}touch: missing file argument\n", marks.error));
                }

                let mut output = String::new();
                for file in &operands {
                    match vfs.touch_file(file) {
                        Ok(_) => output.push_str(&format!("{}Created/updated file: {}\n", marks.file, file)),
                        Err(e) => output.push_str(&format!("{}touch: {}\n", marks.error, e)),
                    }
                }
                Some(output)
            }

            "rm" => {
                if operands.is_empty() {
                    return Some(format!("{}rm: missing file argument\n", marks.error));
                }

                let recursive = has_flag(cmd, "-r") || has_flag(cmd, "-R") || has_flag(cmd, "--recursive");
                let force = has_flag(cmd, "-f") || has_flag(cmd, "--force");
                let mut output = String::new();

                for item in &operands {
                    match vfs.remove(item, recursive, force) {
                        Ok(_) => output.push_str(&format!("{}Removed: {}\n", marks.success, item)),
                        Err(e) => output.push_str(&format!("{}rm: {}\n", marks.error, e)),
                    }
                }
                Some(output)
            }

            "mv" => {
                if operands.len() < 2 {
                    return Some(format!("{}mv: missing source or destination\n", marks.error));
                }

                let (sources, dest) = operands.split_at(operands.len() - 1);
                let dest = &dest[0];
                // Several files go into a folder, never over each other
                if sources.len() > 1 && !vfs.is_dir(dest) {
                    return Some(format!("{}mv: '{}' is not a folder (moving several files needs a folder at the end)\n", marks.error, dest));
                }
                let mut output = String::new();
                for source in sources {
                    match vfs.move_item(source, dest) {
                        Ok(_) => output.push_str(&format!("{}Moved {} to {}\n", marks.success, source, dest)),
                        Err(e) => output.push_str(&format!("{}mv: {}\n", marks.error, e)),
                    }
                }
                Some(output)
            }

            "cp" => {
                if operands.len() < 2 {
                    return Some(format!("{}cp: missing source or destination\n", marks.error));
                }

                let recursive = has_flag(cmd, "-r") || has_flag(cmd, "-R") || has_flag(cmd, "--recursive");
                let (sources, dest) = operands.split_at(operands.len() - 1);
                let dest = &dest[0];
                // Several files go into a folder, never over each other
                if sources.len() > 1 && !vfs.is_dir(dest) {
                    return Some(format!("{}cp: '{}' is not a folder (copying several files needs a folder at the end)\n", marks.error, dest));
                }
                let mut output = String::new();
                for source in sources {
                    match vfs.copy(source, dest, recursive) {
                        Ok(_) => output.push_str(&format!("{}Copied {} to {}\n", marks.success, source, dest)),
                        Err(e) => output.push_str(&format!("{}cp: {}\n", marks.error, e)),
                    }
                }
                Some(output)
            }

            "grep" => {
                // Same grep as after a pipe (-i -n -v -c -E), applied per file
                let flags: Vec<String> =
                    cmd.args.iter().filter(|a| a.starts_with('-') && a.len() > 1).cloned().collect();
                let positional: Vec<&String> =
                    cmd.args.iter().filter(|a| !(a.starts_with('-') && a.len() > 1)).collect();
                let Some(pattern) = positional.first() else {
                    return Some(format!("{}grep: usage: grep [-i] [-n] PATTERN FILE...\n", marks.error));
                };
                let files = &positional[1..];
                if files.is_empty() {
                    return Some(format!("{}(lesson sandbox) grep needs a file here — try: grep {} <file>\n", marks.hint, pattern));
                }
                let mut grep_args = flags.clone();
                grep_args.push((*pattern).clone());

                let mut output = String::new();
                let multiple = files.len() > 1;
                for file in files {
                    let content = match vfs.read_file(file) {
                        Ok(content) => content,
                        Err(e) => {
                            output.push_str(&format!("{}grep: {}\n", marks.error, e));
                            continue;
                        }
                    };
                    match text_filter::apply("grep", &grep_args, &content) {
                        Ok(found) => {
                            for line in found.lines() {
                                if multiple {
                                    output.push_str(&format!("{}:", file));
                                }
                                output.push_str(line);
                                output.push('\n');
                            }
                        }
                        Err(message) => output.push_str(&format!("{}{}\n", marks.error, message)),
                    }
                }
                if output.is_empty() {
                    output = format!("(no lines matched '{}')\n", pattern);
                }
                Some(output)
            }

            "head" | "tail" => {
                let (count, files) = parse_line_count_args(&cmd.args);
                if files.is_empty() {
                    return Some(format!("{}(lesson sandbox) {} needs a file here — try: {} -n {} <file>\n", marks.hint, program, program, count));
                }

                let mut output = String::new();
                let multiple = files.len() > 1;
                for file in &files {
                    let result = if program == "head" {
                        vfs.head_file(file, count)
                    } else if text_filter::tail_from_start(&cmd.args) {
                        // tail -n +N: from line N on
                        let flags: Vec<String> =
                            cmd.args.iter().filter(|a| !files.contains(a)).cloned().collect();
                        vfs.read_file(file).and_then(|text| {
                            text_filter::apply("tail", &flags, &text)
                                .map_err(|e| anyhow::anyhow!(e))
                        })
                    } else {
                        vfs.tail_file(file, count)
                    };
                    match result {
                        Ok(content) => {
                            if multiple {
                                output.push_str(&format!("==> {} <==\n", file));
                            }
                            output.push_str(&content);
                        }
                        Err(e) => output.push_str(&format!("{}{}: {}\n", marks.error, program, e)),
                    }
                }
                Some(output)
            }

            "wc" => {
                let want_lines = has_flag(cmd, "-l") || has_flag(cmd, "--lines");
                let want_words = has_flag(cmd, "-w") || has_flag(cmd, "--words");
                let want_bytes = has_flag(cmd, "-c") || has_flag(cmd, "--bytes");
                let want_all = !want_lines && !want_words && !want_bytes;

                let files: Vec<&String> = cmd.args.iter().filter(|a| !a.starts_with('-')).collect();
                if files.is_empty() {
                    return Some(format!("{}(lesson sandbox) wc needs a file here — try: wc -l <file>\n", marks.hint));
                }

                let mut output = String::new();
                for file in &files {
                    match vfs.wc_file(file) {
                        Ok((lines, words, bytes)) => {
                            let mut parts = Vec::new();
                            if want_all || want_lines {
                                parts.push(format!("{:8}", lines));
                            }
                            if want_all || want_words {
                                parts.push(format!("{:8}", words));
                            }
                            if want_all || want_bytes {
                                parts.push(format!("{:8}", bytes));
                            }
                            output.push_str(&format!("{} {}\n", parts.join(""), file));
                        }
                        Err(e) => output.push_str(&format!("{}wc: {}\n", marks.error, e)),
                    }
                }
                Some(output)
            }

            "echo" => {
                // Redirection (> and >>) is applied by the sandbox shell
                let no_newline = cmd.args.first().is_some_and(|a| a == "-n");
                let words = if no_newline { &cmd.args[1..] } else { &cmd.args[..] };
                let mut text = words.join(" ");
                if !no_newline {
                    text.push('\n');
                }
                Some(text)
            }

            "chmod" => {
                let positional: Vec<&String> = cmd.args.iter().filter(|a| !a.starts_with('-') || a.chars().skip(1).all(|c| "rwxugoa+-=,0123456789".contains(c))).collect();
                // chmod modes like "-w" also start with '-'; treat the first
                // positional as the mode and the rest as files
                let Some(mode) = positional.first() else {
                    return Some(format!("{}chmod: missing operand\n  Usage: chmod MODE FILE...\n", marks.error));
                };
                let files = &positional[1..];
                if files.is_empty() {
                    return Some(format!("{}chmod: missing file operand after '{}'\n", marks.error, mode));
                }

                let mut output = String::new();
                for file in files {
                    match vfs.resolve_path(file).map(|_| vfs.metadata(file)) {
                        Ok(Some(meta)) => {
                            if apply_mode(vfs, file, meta, mode) {
                                output.push_str(&format!(
                                    "{}Mode of '{}' set to '{}'\n",
                                    marks.success, file, mode
                                ));
                            } else {
                                output.push_str(&format!(
                                    "{}Mode of '{}' set to '{}' (simulated: the sandbox only applies +x, -x and number modes)\n",
                                    marks.success, file, mode
                                ));
                            }
                        }
                        Ok(None) => output.push_str(&format!("{}chmod: cannot access '{}': No such file or directory\n", marks.error, file)),
                        Err(e) => output.push_str(&format!("{}chmod: {}\n", marks.error, e)),
                    }
                }
                Some(output)
            }

            // Common commands lessons mention but that can't be meaningfully
            // simulated against a sandbox filesystem: return an instructional
            // message instead of silent nothing (the lesson step still
            // validates the command syntax)
            "ps" | "top" | "htop" | "kill" | "killall" | "git" | "apt" | "apt-get" | "yum"
            | "dnf" | "brew" | "ping" | "curl" | "wget" | "ssh" | "scp" | "sudo" | "man"
            | "df" | "free" | "uname" | "whoami" | "zip" | "unzip"
            | "nano" | "vim" | "vi" | "less" | "more" | "netstat" | "ifconfig" | "ip" | "ss"
            | "lsof" | "systemctl" | "journalctl"
            | "awk" | "ln" | "python" | "python3" | "pip"
            | "npm" | "node" => {
                Some(format!(
                    "(lesson sandbox) '{}' isn't simulated here — this step checks your command syntax.\n",
                    program
                ))
            }

            // Commands that don't manipulate the filesystem
            "sort" | "uniq" | "cut" => {
                let files = text_filter::operands(program, &cmd.args);
                if files.is_empty() {
                    return Some(format!(
                        "{}{} needs a file here, or use it after a pipe: cat file.txt | {}\n",
                        marks.error,
                        program,
                        program
                    ));
                }
                let mut input = String::new();
                for file in &files {
                    match vfs.read_file(file) {
                        Ok(content) => input.push_str(&content),
                        Err(e) => {
                            return Some(format!("{}{}: {}\n", marks.error, program, e))
                        }
                    }
                }
                let flags: Vec<String> =
                    cmd.args.iter().filter(|a| !files.contains(a)).cloned().collect();
                Some(match text_filter::apply(program, &flags, &input) {
                    Ok(out) => out,
                    Err(message) => format!("{}{}\n", marks.error, message),
                })
            }

            // A real command the sandbox doesn't run: say so, and let the
            // step check what was typed
            other if cmd.category != CommandCategory::Unknown => Some(format!(
                "{}{} doesn't run in the simulated sandbox, so this only checks what you typed. \
                 To see real output, switch lesson practice to real files in settings (Ctrl+S).\n",
                marks.info,
                other
            )),

            // Not a command at all (a typo, or a script that isn't on PATH)
            other => Some(format!("{}{}: command not found\n", marks.error, other)),
        }
    }
}

/// Helper: Check if a flag exists in the command
fn has_flag(cmd: &Command, flag: &str) -> bool {
    cmd.flags.iter().any(|f| {
        f.raw == flag ||
        f.short == Some(flag.chars().nth(1).unwrap_or(' ')) ||
        f.long.as_ref().map(|l| l == flag.trim_start_matches("--")).unwrap_or(false)
    })
}

/// `ls -l` style permissions (drwxr-xr-x) for a sandbox path, from its
/// mode bits, so chmod changes show up
fn mode_string(meta: Option<Meta>) -> String {
    let Some(meta) = meta else {
        return "----------".to_string();
    };
    // A platform without modes shows the usual ones
    let bits = meta.mode.unwrap_or(if meta.is_dir { 0o755 } else { 0o644 });
    let mut out = String::from(if meta.is_dir { "d" } else { "-" });
    for shift in [6, 3, 0] {
        let part = (bits >> shift) & 0o7;
        out.push(if part & 4 != 0 { 'r' } else { '-' });
        out.push(if part & 2 != 0 { 'w' } else { '-' });
        out.push(if part & 1 != 0 { 'x' } else { '-' });
    }
    out
}

/// Apply a chmod mode to a sandbox file for the modes lessons rely on:
/// octal numbers and adding/removing execute (`+x`, `u+x`, `a-x`...).
/// Returns false for modes it leaves simulated (and everywhere on a
/// platform without modes).
fn apply_mode(vfs: &VirtualFileSystem, path: &str, meta: Meta, mode: &str) -> bool {
    let Some(current) = meta.mode else {
        return false;
    };
    let current = current & 0o7777;
    let new_mode = if mode.chars().all(|c| c.is_digit(8)) && (3..=4).contains(&mode.len()) {
        u32::from_str_radix(mode, 8).ok()
    } else if let Some((who, perm)) = mode.split_once(['+', '-']) {
        if perm != "x" || !who.chars().all(|c| "ugoa".contains(c)) {
            None
        } else {
            let mut bits = 0;
            if who.is_empty() || who.contains('a') || who.contains('u') {
                bits |= 0o100;
            }
            if who.is_empty() || who.contains('a') || who.contains('g') {
                bits |= 0o010;
            }
            if who.is_empty() || who.contains('a') || who.contains('o') {
                bits |= 0o001;
            }
            Some(if mode.contains('+') { current | bits } else { current & !bits })
        }
    } else {
        None
    };
    match new_mode {
        // Never lock the learner out of their own sandbox file
        Some(m) => vfs.set_mode(path, m | 0o600).is_ok(),
        None => false,
    }
}

/// Parse `head`/`tail` style arguments: `-n N`, `-nN`, `-N`, plus file
/// operands. Returns (line count, files); the count defaults to 10.
fn parse_line_count_args(args: &[String]) -> (usize, Vec<String>) {
    let mut count = 10usize;
    let mut files = Vec::new();

    let mut iter = args.iter().peekable();
    while let Some(arg) = iter.next() {
        if arg == "-n" || arg == "--lines" {
            if let Some(n) = iter.peek().and_then(|v| v.parse::<usize>().ok()) {
                count = n;
                iter.next();
            }
        } else if let Some(n) = arg
            .strip_prefix("-n")
            .or_else(|| arg.strip_prefix("--lines="))
            .and_then(|v| v.parse::<usize>().ok())
        {
            count = n;
        } else if let Some(n) = arg.strip_prefix('-').and_then(|v| v.parse::<usize>().ok()) {
            // Classic `head -5` form
            count = n;
        } else if !arg.starts_with('-') {
            files.push(arg.clone());
        }
    }

    (count, files)
}
