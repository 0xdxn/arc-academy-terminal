# Changelog

## [1.1.5] (2026-10-13)

### Lessons
- In a task step, commands that only look around (`ls`, `pwd`, `cd`, `cat`, `grep` and the like) don't count as wrong answers or use up hints. They still count when the step is about that command.
- Achievement popups size themselves to show the whole description.

### Development
- The test suite removes the temporary folders it creates.

## [1.1.4] (2026-10-13)

### Safety
- Commands in the sandbox see only basic session variables (`PATH`, `HOME`, `USER`, `LOGNAME`, `SHELL`, `TERM`, `LANG`, `TZ`, `LC_*`), and `$NAME` in real-files lessons and the safe free shell falls back to the same list. Variables you set yourself work as before.
- The sandbox also keeps out `.pypirc`, `.git-credentials`, Cargo credentials, `~/.local/share/keyrings`, `~/.password-store`, `~/.azure`, `~/.terraform.d` and `~/.mozilla`.

## [1.1.3] (2026-10-13)

- The test suite runs on machines without an OS sandbox; tests that need one say they were skipped. Nothing else changes from 1.1.2.

## [1.1.2] (2026-10-13)

- Release builds for all four platforms complete on Linux build machines as well. Nothing else changes from 1.1.1.

## [1.1.1] (2026-10-13)

### Install
- Prebuilt downloads for macOS (Apple silicon and Intel) and Linux (x86_64 and arm64); the installer checks each against `SHA256SUMS` before installing.
- The AI assistant connects over rustls on every platform; arct has no OpenSSL dependency.

Everything else is as in 1.1.0.

## [1.1.0] (2026-10-13)

### Safety
- Safe mode, on by default: the free shell runs inside `~/ArcAcademy/home` (created with starter files) behind the safety guard. Turn it off in settings; catastrophic commands are still refused everywhere.
- The guard checks every part of a chained command (`a && b`, `a; b`, `a | b`), refuses `rm -r ~` and `--no-preserve-root`, and keeps `~/...` paths inside the playground, `..` included.
- In the simulated sandbox, `rm -rf /` gets the safety lesson, and flags like `-rf` are always read as flags, never as file names.
- A lesson step only passes if its command actually worked.
- The guard also checks each command after variables and wildcards are filled in, so `X=/; rm -rf $X` is refused like `rm -rf /`. Redirecting into `/dev/null` is allowed; into a real device it is not.
- Real-files lessons can use `$`: arct fills in variables, `~` and wildcards itself, the guard checks the command with the real values and file names, and exactly that command runs, with nothing expanded a second time. Wildcards never match `.` or `..`. `$(...)` and backticks are still refused, and so is `$` inside `for`, `while` and `if`, whose values only exist as the shell runs them.
- Real-files lessons, and the free shell in safe mode, run inside the operating system's sandbox: `sandbox-exec` on macOS, bubblewrap (`bwrap`) on Linux. Commands can write only inside their own folder and a private temp folder, can't use the network, can't have programs running outside the sandbox act for them, and can't read the learner's keys and credentials. arct checks all of this works before using the sandbox; real-files lessons are only offered where it does, and safe mode says so when it is running without one.
- In safe mode the free shell works the same way: arct fills in `$`, `~` and wildcards against your Arc Academy home itself, the guard checks the result, and exactly that runs. Wildcards never match `.` or `..`.
- The guard reads quoted words the way the shell does, and checks paths after `=` (`of=...`, `--target-directory=...`) like any other path.

### Learning
- Explorer track: ten story lessons on Arc Station for younger learners, from `pwd` to a first script.
- Operator track: twenty new lessons, thirty in all. Dotfiles, wildcards, quoting, command lists, redirection, pipelines and `tee`, `find`, `sed`, `tr` and `cut`, `diff`, variables, the environment, scripts and their arguments, `PATH`, archives, `du`, SSH keys, and two capstones: Log Detective and Spring Cleaning.
- The simulated sandbox is now a small shell: quotes, variables and `export`, wildcards, redirection (`>`, `>>`, `2>`, `2>&1`, `<`, `/dev/null`), pipes into any text command, `&&`, `||` and `;`, scripts with arguments, and your own commands on `PATH`. New commands: `find`, `sed`, `tr`, `tee`, `diff`, `du`, `tar`, `gzip`, `gunzip`, `which`, `type`, `env`, and a practice-only `ssh-keygen`. `cp` and `mv` take several files into a folder; `ls -l` shows real permissions.
- In real-files lessons, variables and `export` carry from one command to the next.
- Lessons are TOML files, and steps can check outcomes (a folder exists, a file contains text, where you are) instead of exact keystrokes.
- Hints come one per miss; the answer only after the last hint; challenge steps give none.
- Practice and fill-in-the-blank steps check the learner's answer.
- Quiz answers are numbered from 1.
- Onboarding asks who's learning (Explorer or Operator); the lesson map groups by track and points at the next lesson.
- Sandbox: `ls` with folders, `-l` and `-h`; pipes into grep, head, tail, wc, sort, uniq and cut; scripts with `sh`, `bash` and `./`; `chmod +x` works.
- Operator lessons use their own practice files instead of system files; `top` is explained rather than run; the curl example fetches example.com.
- New badges: Safe Hands (the guard stopped you) and Station Crew (finish the Explorer track).
- Explorer lessons start with only their own files on Arc Station; Operator lessons also get the sample home folders.
- The lessons can run in a web browser: `crates/arct-web` is the same lesson engine compiled to WebAssembly, with the practice files in memory. It saves nothing and sends nothing. `scripts/build-web.sh` builds it, with a plain test page.

### The screen
- The output area is a transcript, like a terminal: each command shows the folder it ran in and the command, then its output, then any lesson feedback in quieter lines. Newest is at the bottom; scroll back with `Ctrl+U` / `Ctrl+D` or the mouse wheel, and typing brings you back to the end. The input line shows the same folder.
- Command errors show as the terminal prints them.
- Lesson steps show in full, with what to do next after the text. A step longer than the panel says how much more there is, and Enter reads on.
- The side panel shows where you are and what's in that folder (in lessons, the lesson world as a tree) and a line of progress, and `Ctrl+F` hides it. The header is a single line; keys live in the footer and the help.
- Keys that work in any terminal: `Ctrl+B` goes back a lesson step, `Ctrl+P` opens progress, `?` on an empty line or `F1` opens help. In a lesson `Ctrl+L` opens the lesson map, and `Ctrl+L` in the map leaves lessons. The `Alt` keys still work where the terminal sends them.

### Rewards
- Streaks count learning days, in your own time zone. Up to two rest days in between keep a run going; after a longer break a new run starts and your best one is kept. A day counts when you do something, not when you open the app.
- Mastery: a lesson finished without needing the answer (hints are fine). Each track has a badge for finishing it and one for mastering it: Station Crew and Station Ace, Operator and Senior Operator. The progress screen (`Ctrl+P`) shows where each track stands.
- `arct certificate` makes a printable certificate for a finished track: a single web page made on your computer, with no scripts and nothing downloaded, with lines for the date and a signature. `--text` prints it in the terminal instead.
- The Night Owl badge (learning after 10 PM) is gone, and the streak and time-of-day badges now follow your local clock.

### Setup and privacy
- The AI helper is off unless you choose it; onboarding says where questions go. The name is optional.
- Esc goes back during setup. Plain one-cell icons by default (Nerd Font icons optional), so panels line up in any terminal.
- The installer downloads a prebuilt binary and checks its SHA-256 sum, and asks before installing Rust.
- `ARCT_DATA_DIR` sets where progress is kept.

## [1.0.0] (2026-07-06)

First complete release: ten lessons, simulated and real-files practice, the safety guard, achievements, themes.
