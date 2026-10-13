# Changelog

## [1.1.0] (unreleased)

### Safety
- Safe mode, on by default: the free shell runs inside `~/ArcAcademy/home` (created with starter files) behind the safety guard. Turn it off in settings; catastrophic commands are still refused everywhere.
- The guard checks every part of a chained command (`a && b`, `a; b`, `a | b`), refuses `rm -r ~` and `--no-preserve-root`, and no longer lets `~/...` paths climb out with `..`.
- In the simulated sandbox, `rm -rf /` now gets the safety lesson instead of wiping the sandbox, and flags like `-rf` are never treated as file names.
- A lesson step only passes if its command actually worked.
- The guard also checks each command after variables and wildcards are filled in, so `X=/; rm -rf $X` is refused like `rm -rf /`. Redirecting into `/dev/null` is allowed; into a real device it is not.
- Real-files lessons can use `$`: arct fills in variables, `~` and wildcards itself, the guard checks the command with the real values and file names, and exactly that command runs, with nothing expanded a second time. Wildcards never match `.` or `..`. `$(...)` and backticks are still refused, and so is `$` inside `for`, `while` and `if`, whose values only exist as the shell runs them.
- Real-files lessons, and the free shell in safe mode, run inside the operating system's sandbox: `sandbox-exec` on macOS, bubblewrap (`bwrap`) on Linux. Commands can write only inside their own folder and a private temp folder, can't use the network, and can't read the learner's keys and credentials. arct checks the sandbox works before using it; real-files lessons are only offered where it does.
- In safe mode the free shell works the same way: arct fills in `$`, `~` and wildcards against your Arc Academy home itself, the guard checks the result, and exactly that runs. Wildcards never match `.` or `..`.
- The guard reads quoted words the way the shell does, and checks paths after `=` (`of=...`, `--target-directory=...`) like any other path.

### Learning
- Explorer track: ten story lessons on Arc Station for younger learners, from `pwd` to a first script.
- Operator track: twenty new lessons, thirty in all. Dotfiles, wildcards, quoting, command lists, redirection, pipelines and `tee`, `find`, `sed`, `tr` and `cut`, `diff`, variables, the environment, scripts and their arguments, `PATH`, archives, `du`, SSH keys, and two capstones: Log Detective and Spring Cleaning.
- The simulated sandbox is now a small shell: quotes, variables and `export`, wildcards, redirection (`>`, `>>`, `2>`, `2>&1`, `<`, `/dev/null`), pipes into any text command, `&&`, `||` and `;`, scripts with arguments, and your own commands on `PATH`. New commands: `find`, `sed`, `tr`, `tee`, `diff`, `du`, `tar`, `gzip`, `gunzip`, `which`, `type`, `env`, and a practice-only `ssh-keygen`. `cp` and `mv` take several files into a folder; `ls -l` shows real permissions.
- In real-files lessons, variables and `export` carry from one command to the next.
- Lessons are TOML files, and steps can check outcomes (a folder exists, a file contains text, where you are) instead of exact keystrokes.
- Hints come one per miss; the answer only after the last hint; challenge steps give none.
- Practice and fill-in-the-blank steps are now actually checked.
- Onboarding asks who's learning (Explorer or Operator); the lesson map groups by track and points at the next lesson.
- Sandbox: `ls` with folders, `-l` and `-h`; pipes into grep, head, tail, wc, sort, uniq and cut; scripts with `sh`, `bash` and `./`; `chmod +x` works.
- Operator lessons use their own practice files instead of system files; `top` is explained rather than run; the curl example fetches example.com.
- New badges: Safe Hands (the guard stopped you) and Station Crew (finish the Explorer track).
- Explorer lessons start with only their own files on Arc Station; Operator lessons also get the sample home folders.
- The lessons can run in a web browser: `crates/arct-web` is the same lesson engine compiled to WebAssembly, with the practice files in memory. It saves nothing and sends nothing. `scripts/build-web.sh` builds it, with a plain test page.

### Rewards
- Streaks count learning days, in your own time zone. Up to two rest days in between keep a run going; after a longer break a new run starts and your best one is kept. A day counts when you do something, not when you open the app.
- Mastery: a lesson finished without needing the answer (hints are fine). Each track has a badge for finishing it and one for mastering it: Station Crew and Station Ace, Operator and Senior Operator. The progress screen (`Alt+P`) shows where each track stands.
- `arct certificate` makes a printable certificate for a finished track: a single web page made on your computer, with no scripts and nothing downloaded, with lines for the date and a signature. `--text` prints it in the terminal instead.
- The Night Owl badge (learning after 10 PM) is gone, and the streak and time-of-day badges now follow your local clock.

### Setup and privacy
- The AI helper is off unless you choose it; onboarding says where questions go. The name is optional.
- Esc goes back during setup. Plain one-cell icons by default (Nerd Font icons optional), so panels line up in any terminal.
- The installer downloads a prebuilt binary and checks its SHA-256 sum; it no longer installs Rust without asking.
- `ARCT_DATA_DIR` sets where progress is kept.

## [1.0.0] (2026-07-06)

First complete release: ten lessons, simulated and real-files practice, the safety guard, achievements, themes.
