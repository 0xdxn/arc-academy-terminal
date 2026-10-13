# Writing a lesson

A lesson is one TOML file. You don't need to know Rust. If you can write the steps down for a friend, you can write a lesson.

- Built-in lessons: [`crates/arct-core/lessons/`](../crates/arct-core/lessons/) (`explorer/` and `operator/`)
- Your own lessons: the `lessons` folder next to your config (`arct info` shows where). arct loads every `*.toml` file in it at startup; a file with the same `id` as a built-in replaces it.

## What makes a good arct lesson

1. **Teach, then try, then do it alone.** Show a command once, ask the learner to use it on something new without showing it again, and finish with a challenge that has no hints.
2. **Check outcomes, not keystrokes.** Prefer a `Task` step ("make a folder called `art`") over "type exactly `mkdir art`". Then `mkdir -p art`, or `cd` there first, all count.
3. **Hints are a ladder.** The first hint nudges, the last one gives the answer. They appear one per miss, never up front.
4. **Bring your own world.** Use `[[setup]]` to create the files the lesson needs. Never point at real system files like `/etc/passwd`: they don't exist in the sandbox, and in real-files mode the guard won't let commands reach them.
5. **Short sentences.** Explorer lessons are read by eight-year-olds. Operator lessons are read by tired adults. Both like short sentences.

## A complete example

```toml
id = "my-first-lesson"          # unique, lowercase, dashes
title = "Make a Treasure Map"
description = "Make folders and files to hide a treasure."
difficulty = "Beginner"         # Beginner | Intermediate | Advanced | Expert
estimated_minutes = 8
prerequisites = ["explorer-build-your-bunk"]   # ids of lessons to finish first
tags = ["explorer", "mkdir", "touch"]
track = "Explorer"              # Explorer | Operator (default Operator)

# Files the lesson starts with. Paths are relative to where the lesson starts.
[[setup]]
path = "island/README.txt"
contents = "A small island. Somewhere to hide things.\n"

# 1. Information: read, press Enter
[[steps]]
step_number = 1
title = "Pirate rules"
instruction = ""

[steps.step_type.Information]
content = """
Every treasure needs a hiding place.
  mkdir NAME    makes a folder
  touch NAME    makes an empty file"""

# 2. Task: any commands that reach the goal count
[[steps]]
step_number = 2
title = "Dig a cave"
instruction = "Make a folder called cave inside island."
hints = ["mkdir makes folders.", "Type: mkdir island/cave"]
solution = ["mkdir island/cave"]   # for the test suite, never shown

[steps.step_type.Task]
goal = "Make island/cave"
success_message = "A cave!"
checks = [{ DirExists = "island/cave" }]

# 3. Multiple choice: the learner types the number
[[steps]]
step_number = 3
title = "Quick check"
instruction = ""

[steps.step_type.MultipleChoice]
question = "Which command makes an empty file?"
options = ["mkdir", "touch", "ls"]
correct_index = 1               # counting from 0
explanation = "touch makes a file; mkdir makes a folder."

# 4. Challenge: no hints, no answer
[[steps]]
step_number = 4
title = "On your own"
instruction = "Hide treasure.txt inside the cave."
challenge = true
solution = ["touch island/cave/treasure.txt"]

[steps.step_type.Task]
goal = "Hide island/cave/treasure.txt"
success_message = "Treasure hidden. Arr."
checks = [{ FileExists = "island/cave/treasure.txt" }]
```

Keys in a step (`hints`, `challenge`, `solution`) go **before** its `[steps.step_type.…]` line; anything after that line belongs to the step type.

## Step types

| Type | The learner | Passes when |
|------|-------------|-------------|
| `Information` | reads, presses Enter | always |
| `Task` | uses any commands | every check holds |
| `CommandExercise` | types a command | it matches `expected_command` (see `validation`) **and** it worked |
| `MultipleChoice` | types a number | it's `correct_index` |
| `FillInBlank` | types the full command | the `{blank}` in `template` is filled with one of `correct_answers` |

`CommandExercise.validation` is one of `"Exact"`, `"CommandOnly"`, `"CommandAndFlags"`, `{ AnyOf = ["ls -a", "ls -la"] }` or `{ Regex = "^ls( -[al]+)?$" }`. Prefer `Task` when you can.

## Checks for `Task` steps

Paths are relative to where the lesson starts.

| Check | Holds when |
|-------|------------|
| `{ DirExists = "a/b" }` | the folder exists |
| `{ FileExists = "a/b.txt" }` | the file exists |
| `{ Missing = "junk.txt" }` | nothing is at that path |
| `{ FileContains = { path = "log.txt", text = "Day 2" } }` | the file contains the text |
| `{ CwdIs = "station/bridge" }` | the learner is in that folder (`""` is the start) |
| `{ OutputContains = "NOVA-7" }` | the last command printed the text |
| `{ Used = "grep" }` | the last command ran that program (anywhere in a pipe) |

When a check fails, the learner sees what's missing ("Not yet: there's no folder called 'cave'"), never the answer.

## What runs in the simulated sandbox

`pwd`, `ls` (`-a`, `-l`, `-h`, folders), `cd`, `cat`, `head`, `tail`, `wc`, `grep` (`-i -n -v -c -E`), `sort`, `uniq`, `cut`, `mkdir` (`-p`), `touch`, `rm` (`-r`), `mv`, `cp` (`-r`), `echo` (with `>` and `>>`), `chmod` (`+x`, `-x`, numbers), pipes into the text commands, and scripts (`sh file`, `bash file`, `./file` after `chmod +x`). Anything else is accepted by `CommandExercise` steps with a note that it only ran as a check; write those lessons for real-files mode.

## Test it

```sh
cargo test -p arct-tui playthrough
```

This plays every built-in lesson from the first step to the last through the same code a learner's Enter key runs. Task steps need a `solution`; other steps use their expected answer. If your lesson can't be finished, the test says which step and shows the output.

For your own lesson folder, start arct and play it: that's the real test.
