# Changelog

## [1.1.0] (unreleased)

### Safety
- Safe mode, on by default: the free shell runs inside `~/ArcAcademy/home` (created with starter files) behind the safety guard. Turn it off in settings; catastrophic commands are still refused everywhere.
- The guard checks every part of a chained command (`a && b`, `a; b`, `a | b`), refuses `rm -r ~` and `--no-preserve-root`, and no longer lets `~/...` paths climb out with `..`.
- In the simulated sandbox, `rm -rf /` now gets the safety lesson instead of wiping the sandbox, and flags like `-rf` are never treated as file names.
- A lesson step only passes if its command actually worked.

### Learning
- Explorer track: ten story lessons on Arc Station for younger learners, from `pwd` to a first script.
- Lessons are TOML files, and steps can check outcomes (a folder exists, a file contains text, where you are) instead of exact keystrokes.
- Hints come one per miss; the answer only after the last hint; challenge steps give none.
- Practice and fill-in-the-blank steps are now actually checked.
- Onboarding asks who's learning (Explorer or Operator); the lesson map groups by track and points at the next lesson.
- Sandbox: `ls` with folders, `-l` and `-h`; pipes into grep, head, tail, wc, sort, uniq and cut; scripts with `sh`, `bash` and `./`; `chmod +x` works.
- Operator lessons use their own practice files instead of system files; `top` is explained rather than run; the curl example fetches example.com.
- New badges: Safe Hands (the guard stopped you) and Station Crew (finish the Explorer track).

### Setup and privacy
- The AI helper is off unless you choose it; onboarding says where questions go. The name is optional.
- Esc goes back during setup. Plain one-cell icons by default (Nerd Font icons optional), so panels line up in any terminal.
- The installer downloads a prebuilt binary and checks its SHA-256 sum; it no longer installs Rust without asking.
- `ARCT_DATA_DIR` sets where progress is kept.

## [1.0.0] (2026-07-06)

First complete release: ten lessons, simulated and real-files practice, the safety guard, achievements, themes.
