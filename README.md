# arct

Learn the command line by using it. arct (Arc Academy Terminal) is a terminal app with guided lessons, an explanation for every command you type, and a safety guard that turns dangerous commands into lessons instead of disasters.

It works for a ten-year-old meeting a terminal for the first time and for an adult who wants to finally feel at home in one.

- **Two tracks.** *Explorer* is a story for younger learners (about 8 to 12): you're the newest crew member on Arc Station, and the station computer only understands typed commands. *Operator* is the grown-up path: navigation, files, permissions, processes, pipes, packages, networking and git.
- **You learn by doing.** Most steps check what actually happened (is the folder there? does the file say the right thing?), so any route that works counts. Hints arrive one at a time when you miss, and the answer only after the last hint. Each lesson ends with a challenge that has no hints at all.
- **Safe by default.** The free shell runs inside `~/ArcAcademy/home`, and a guard keeps commands there. `rm -rf /` and friends are refused with an explanation of what they would have done. Lessons run in a simulated sandbox, or on real files in `~/ArcAcademy/playground` if you choose.
- **Rewards that measure skill.** Levels and badges earned by finishing lessons and challenges, not by grinding; a badge for mastering each track without needing the answers; streaks that allow rest days; and a printable certificate when a track is done (`arct certificate`).
- **Nothing leaves your computer.** No accounts. Progress is stored locally. Local-only usage stats are off unless you turn them on. The AI helper is optional and off by default; if you turn it on, your questions go to the AI you choose.

## Install

```sh
curl -fsSL https://arcacademy.sh/install.sh | sh
```

The installer downloads a prebuilt `arct` for macOS or Linux, checks it against the published SHA-256 sums, and puts it in `~/.local/bin`. It doesn't need admin rights and doesn't install anything else. Read it first if you like: it's short.

With Rust installed you can also use cargo:

```sh
cargo install --locked arct-cli
```

Or build from source:

```sh
git clone https://github.com/0xdxn/arc-academy-terminal.git
cd arc-academy-terminal
cargo install --locked --path crates/arct-cli
```

## Start

```sh
arct                      # start the app
arct explain "ls -lah"    # explain a command without starting the app
arct info                 # where arct keeps its settings and progress
```

The first run asks three things: a name to call you (optional), who's learning (Explorer or Operator), and whether you want an AI helper (no, by default). Then press `Ctrl+L` for the lesson map.

## Keys

| Key | Does |
|-----|------|
| `Ctrl+L` | Lesson map |
| `Ctrl+K` | Command palette: every feature, searchable |
| `Ctrl+S` | Settings (track, safe mode, practice mode, AI, theme) |
| `Alt+←` / `Alt+R` | Previous lesson step / restart the lesson |
| `Alt+A` / `Alt+P` / `Alt+C` | Badges / progress / challenges |
| `Ctrl+T` | Next theme |
| `?` | Help |
| `q` or `Ctrl+C` | Quit |

The bar at the bottom always shows the keys that matter where you are.

## Settings

Settings live in `config.toml`; `arct info` prints the path. The ones people change:

```toml
[lessons]
track = "explorer"           # or "operator": which track the map shows first
practice_mode = "simulated"  # or "real": lessons on real files in ~/ArcAcademy/playground

[shell]
safe_mode = true             # false: the shell uses your real folders (catastrophic commands are still refused)

[theme]
nerd_fonts = false           # true if your terminal font has Nerd Font icons

[ai]
enabled = false              # optional helper; see docs for providers
```

`ARCT_DATA_DIR` moves where progress is stored (handy for a USB stick or a shared classroom machine).

## Write a lesson

Lessons are TOML files, not code. The built-in ones live in [`crates/arct-core/lessons/`](crates/arct-core/lessons/), and you can add your own to the `lessons` folder next to your config. [docs/WRITING-LESSONS.md](docs/WRITING-LESSONS.md) walks through the format, with a complete example.

## Contributing

New lessons, fixes, translations of the lesson text, and reports of anything confusing are all welcome. Start with [CONTRIBUTING.md](CONTRIBUTING.md). Security problems: see [SECURITY.md](SECURITY.md).

## License

GPL-2.0-only. See [LICENSE](LICENSE).
