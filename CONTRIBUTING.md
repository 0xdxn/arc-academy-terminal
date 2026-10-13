# Contributing to arct

Thanks for helping people learn the command line.

## Ways to help

- **Write a lesson.** The most valuable contribution, and no Rust needed: see [docs/WRITING-LESSONS.md](docs/WRITING-LESSONS.md).
- **Report confusion.** If a step, hint or message confused you or a learner you were helping, that's a bug. Open an issue and say what you expected.
- **Fix a bug or add a sandbox command.** The simulated sandbox lives in `crates/arct-tui/src/app.rs` (`execute_virtual_fs_command`); text filters are in `crates/arct-core/src/text_filter.rs`.
- **Translate lesson text.** Open an issue first so we can agree on the layout.

## Ground rules

- **Learners include children.** arct collects nothing: no accounts, analytics, crash reporting or network calls the learner didn't ask for. A change that sends data anywhere won't be merged. Lessons don't ask for personal information.
- **Safety first.** Changes to `crates/arct-core/src/guard.rs` or the shell need tests that try to break them.
- **Plain words.** Messages say what happened and what to do next. No jargon a learner hasn't met yet.

## Working on the code

```sh
cargo build
cargo test --workspace          # includes playing every built-in lesson to the end
cargo clippy --workspace
cargo run -p arct-cli           # try it
```

To try arct without touching your own progress, give it a scratch home:

```sh
HOME=$(mktemp -d) cargo run -p arct-cli
```

Keep pull requests focused: one lesson, one fix, or one feature. Say in the description how you tested it. For anything bigger than a small fix, open an issue first so we can agree on the shape before you spend time on it.

By contributing you agree that your work is licensed under GPL-2.0-only, like the rest of the project, and that you follow the [Code of Conduct](CODE_OF_CONDUCT.md).
