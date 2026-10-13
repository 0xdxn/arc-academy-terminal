//! First Keys: a mode for the youngest learners, before they can read.
//!
//! A grown-up starts it (`arct first-keys`). From then on the keyboard is
//! safe to bang on: no key quits, opens a menu or runs anything. Every
//! letter and number appears huge and is said out loud. It grows in three
//! stages:
//!
//! 1. Any key: press anything, see it big, hear its name.
//! 2. Find the letter: the screen shows one letter; finding it on the
//!    keyboard gets a cheer.
//! 3. Magic words: `ls`, `cd` and `pwd` on a tiny Arc Station. `ls` shows
//!    what is in the room, `cd` walks to the next room, `pwd` says where
//!    you are.
//!
//! Grown-ups leave with Ctrl+G and then the letters q u i t (or m u t e /
//! t a l k to turn the voice off and on). Speech only ever says fixed words
//! from this file, never anything typed.

use crate::theme::Theme;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Clear, Paragraph},
    Frame,
};

/// Letters pressed in stage 1 before "find the letter" starts
const ANY_KEY_PRESSES: u32 = 15;
/// Letters found in stage 2 before the magic words start
const LETTERS_TO_FIND: u32 = 8;
/// Letters to look for in stage 2: the ones the magic words need first
const FIND_ORDER: &[char] = &['L', 'S', 'C', 'D', 'P', 'W', 'A', 'M', 'O', 'T', 'B', 'E'];

/// The three magic words, in the order they are taught
const MAGIC_WORDS: &[&str] = &["ls", "cd", "pwd"];

/// What a key did, for the app
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyOutcome {
    /// Stay in First Keys
    Stay,
    /// A grown-up asked to leave
    Exit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    AnyKey,
    FindLetter,
    MagicWords,
}

/// One room of the tiny station: its name and the things `ls` shows
struct Room {
    name: &'static str,
    things: &'static [(&'static str, &'static [&'static str])],
}

const ROOMS: &[Room] = &[
    Room {
        name: "bunk",
        things: &[("bed", BED), ("teddy", TEDDY)],
    },
    Room {
        name: "bridge",
        things: &[("stars", STARS), ("screen", SCREEN)],
    },
    Room {
        name: "garden",
        things: &[("flower", FLOWER), ("tomato", TOMATO)],
    },
    Room {
        name: "cargo",
        things: &[("box", CRATE), ("ball", BALL)],
    },
    Room {
        name: "robot room",
        things: &[("robot", ROBOT)],
    },
];

const BED: &[&str] = &["        ", " ____   ", "|    |__", "|_______|", " |     | "];
const TEDDY: &[&str] = &[" ()_() ", " (o o) ", "( (_) )", " (   ) ", "  \" \"  "];
const STARS: &[&str] = &["  *    .  ", ".    *    ", "   .    * ", " *    .   ", "    *    ."];
const SCREEN: &[&str] = &[" _______ ", "|  ___  |", "| | o | |", "|  ---  |", "|_______|"];
const FLOWER: &[&str] = &["  .-.  ", " ( o ) ", "  '-'  ", "   |   ", " \\_|_/ "];
const TOMATO: &[&str] = &["  \\|/  ", " .---. ", "(     )", "(     )", " '---' "];
const CRATE: &[&str] = &[" _______ ", "|\\     /|", "| \\   / |", "| /   \\ |", "|/_____\\|"];
const BALL: &[&str] = &["  ___  ", " /   \\ ", "|  O  |", " \\___/ ", "       "];
const ROBOT: &[&str] = &["  [o_o]  ", "  /| |\\  ", " / |_| \\ ", "   | |   ", "  _| |_  "];

/// Says fixed words out loud. Tests swap in a recorder.
pub trait Speaker {
    fn say(&mut self, words: &str);
}

/// The system voice: `say` on macOS, `spd-say` or `espeak-ng` / `espeak`
/// on Linux. A new word stops the one still playing, so mashing keys
/// never piles up speech.
pub struct SystemVoice {
    program: Option<&'static str>,
    playing: Option<std::process::Child>,
}

impl SystemVoice {
    pub fn new() -> Self {
        let program = if std::env::var_os("ARCT_NO_SPEECH").is_some() {
            None
        } else if cfg!(target_os = "macos") {
            Some("/usr/bin/say").filter(|p| std::path::Path::new(p).exists())
        } else {
            ["spd-say", "espeak-ng", "espeak"].into_iter().find(|p| on_path(p))
        };
        Self { program, playing: None }
    }

    pub fn available(&self) -> bool {
        self.program.is_some()
    }
}

impl Default for SystemVoice {
    fn default() -> Self {
        Self::new()
    }
}

fn on_path(program: &str) -> bool {
    std::env::var_os("PATH")
        .is_some_and(|paths| std::env::split_paths(&paths).any(|dir| dir.join(program).is_file()))
}

impl Speaker for SystemVoice {
    fn say(&mut self, words: &str) {
        let Some(program) = self.program else { return };
        if let Some(mut child) = self.playing.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        // The words are an argument, never a shell line
        self.playing = std::process::Command::new(program)
            .arg(words)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .ok();
    }
}

impl Drop for SystemVoice {
    fn drop(&mut self) {
        if let Some(mut child) = self.playing.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

/// The First Keys screen and everything it remembers
pub struct FirstKeys {
    stage: Stage,
    /// The big character on screen
    shown: Option<char>,
    /// A line under it ("Find this letter!", "Yes!")
    message: String,
    presses: u32,
    found: u32,
    target: char,
    /// What has been typed toward a magic word
    word: String,
    room: usize,
    /// The things `ls` put on screen
    showing_room: bool,
    learned: Vec<&'static str>,
    /// Ctrl+G was pressed: the next letters may be a grown-up's command
    grown_up: Option<String>,
    muted: bool,
    /// Changes colour on each key, for fun
    tint: usize,
    speaker: Box<dyn Speaker + Send>,
}

impl FirstKeys {
    pub fn new(speaker: Box<dyn Speaker + Send>) -> Self {
        let mut keys = Self {
            stage: Stage::AnyKey,
            shown: None,
            message: "Press any key!".to_string(),
            presses: 0,
            found: 0,
            target: FIND_ORDER[0],
            word: String::new(),
            room: 0,
            showing_room: false,
            learned: Vec::new(),
            grown_up: None,
            muted: false,
            tint: 0,
            speaker,
        };
        keys.say("Press any key");
        keys
    }

    pub fn stage(&self) -> Stage {
        self.stage
    }

    /// Every magic word has been used: the First Keys badge
    pub fn finished(&self) -> bool {
        MAGIC_WORDS.iter().all(|w| self.learned.contains(w))
    }

    fn say(&mut self, words: &str) {
        if !self.muted {
            self.speaker.say(words);
        }
    }

    /// Handle one key. Nothing here can leave the mode except the
    /// grown-up sequence.
    pub fn handle_key(&mut self, key: KeyEvent) -> KeyOutcome {
        // Grown-ups: Ctrl+G, then a word
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            if key.code == KeyCode::Char('g') {
                self.grown_up = Some(String::new());
            }
            return KeyOutcome::Stay;
        }
        if let Some(typed) = self.grown_up.as_mut() {
            if let KeyCode::Char(c) = key.code {
                typed.push(c.to_ascii_lowercase());
                let typed = typed.clone();
                match typed.as_str() {
                    "quit" => return KeyOutcome::Exit,
                    "mute" => {
                        self.muted = true;
                        self.grown_up = None;
                        return KeyOutcome::Stay;
                    }
                    "talk" => {
                        self.muted = false;
                        self.grown_up = None;
                        return KeyOutcome::Stay;
                    }
                    t if ["quit", "mute", "talk"].iter().any(|w| w.starts_with(t)) => {
                        return KeyOutcome::Stay;
                    }
                    _ => self.grown_up = None,
                }
            } else {
                self.grown_up = None;
            }
        }

        self.tint = self.tint.wrapping_add(1);
        match self.stage {
            Stage::AnyKey => self.any_key(key.code),
            Stage::FindLetter => self.find_letter(key.code),
            Stage::MagicWords => self.magic_words(key.code),
        }
        KeyOutcome::Stay
    }

    fn any_key(&mut self, code: KeyCode) {
        let Some(c) = big_char(code) else {
            self.message = "Press a letter or a number!".to_string();
            return;
        };
        self.shown = Some(c);
        self.presses += 1;
        self.message = String::new();
        self.say(&c.to_string());
        if self.presses >= ANY_KEY_PRESSES {
            self.stage = Stage::FindLetter;
            self.target = FIND_ORDER[0];
            self.shown = Some(self.target);
            self.message = "Find this letter!".to_string();
            self.say(&format!("Can you find {}?", self.target));
        }
    }

    fn find_letter(&mut self, code: KeyCode) {
        let Some(c) = big_char(code) else { return };
        if c == self.target {
            self.found += 1;
            self.message = format!("Yes! That is {c}!");
            self.say(&format!("Yes! {c}!"));
            if self.found >= LETTERS_TO_FIND {
                self.stage = Stage::MagicWords;
                self.shown = None;
                self.message = String::new();
                self.say("Now the magic words");
                return;
            }
            self.target = FIND_ORDER[self.found as usize % FIND_ORDER.len()];
            self.shown = Some(self.target);
        } else {
            self.message = format!("That is {c}. Find {}!", self.target);
            self.say(&c.to_string());
        }
    }

    /// The magic word to learn next, or the first one again when all are known
    fn next_word(&self) -> &'static str {
        MAGIC_WORDS
            .iter()
            .find(|w| !self.learned.contains(w))
            .copied()
            .unwrap_or(MAGIC_WORDS[0])
    }

    fn magic_words(&mut self, code: KeyCode) {
        match code {
            KeyCode::Enter => {
                let word = std::mem::take(&mut self.word);
                self.run_word(&word);
            }
            KeyCode::Backspace => {
                self.word.pop();
            }
            KeyCode::Char(c) if c.is_ascii_alphanumeric() && self.word.len() < 6 => {
                let c = c.to_ascii_lowercase();
                self.word.push(c);
                self.shown = Some(c.to_ascii_uppercase());
                self.showing_room = false;
                self.message = String::new();
                self.say(&c.to_ascii_uppercase().to_string());
            }
            _ => {}
        }
    }

    fn run_word(&mut self, word: &str) {
        let room = ROOMS[self.room].name;
        match word {
            "ls" => {
                self.showing_room = true;
                self.message = format!("Here is what is in the {room}!");
                self.say(&format!("In the {room}"));
            }
            "cd" => {
                self.room = (self.room + 1) % ROOMS.len();
                self.showing_room = false;
                let room = ROOMS[self.room].name;
                self.message = format!("You walked to the {room}!");
                self.say(&format!("You walked to the {room}"));
            }
            "pwd" => {
                self.showing_room = false;
                self.message = format!("You are in the {room}.");
                self.say(&format!("You are in the {room}"));
            }
            _ => {
                let next = self.next_word();
                self.message = format!("Let's try {}", spaced(next));
                self.say("Let's try again");
                return;
            }
        }
        self.shown = None;
        let word = MAGIC_WORDS.iter().find(|w| **w == word).copied().unwrap_or("ls");
        if !self.learned.contains(&word) {
            self.learned.push(word);
            if self.finished() {
                self.message = "You know all the magic words! First Keys!".to_string();
                self.say("You know all the magic words. Hooray!");
            }
        }
    }

    pub fn render(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        frame.render_widget(Clear, area);
        frame.render_widget(Block::default().style(theme.style_block()), area);

        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // what to do
                Constraint::Min(8),    // the big letter or the room
                Constraint::Length(2), // message
                Constraint::Length(3), // magic word keys
                Constraint::Length(1), // grown-ups
            ])
            .split(area);

        let colours = [theme.accent, theme.success, theme.warning, theme.info];
        let tint = Style::default()
            .fg(colours[self.tint % colours.len()])
            .add_modifier(Modifier::BOLD);

        let prompt = match self.stage {
            Stage::AnyKey => "Press any key!".to_string(),
            Stage::FindLetter => format!("Find {} on the keyboard", self.target),
            Stage::MagicWords => format!("Type {}, then press Enter", spaced(self.next_word())),
        };
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(prompt, theme.style_title(true))))
                .alignment(Alignment::Center),
            rows[0].inner(&ratatui::layout::Margin { horizontal: 0, vertical: 1 }),
        );

        if self.showing_room {
            self.render_room(frame, rows[1], theme, tint);
        } else if let Some(c) = self.shown {
            let lines = big_glyph(c, rows[1])
                .into_iter()
                .map(|row| Line::from(Span::styled(row, tint)))
                .collect::<Vec<_>>();
            let height = lines.len() as u16;
            let top = rows[1].y + rows[1].height.saturating_sub(height) / 2;
            frame.render_widget(
                Paragraph::new(lines).alignment(Alignment::Center),
                Rect { y: top, height: height.min(rows[1].height), ..rows[1] },
            );
        } else {
            self.render_station(frame, rows[1], theme);
        }

        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(self.message.clone(), theme.style_accent())))
                .alignment(Alignment::Center),
            rows[2],
        );

        if self.stage == Stage::MagicWords {
            let mut spans = vec![Span::styled("  ", theme.style_normal())];
            for (i, c) in self.next_word().chars().enumerate() {
                let typed = self.word.chars().nth(i) == Some(c);
                let style = if typed { theme.style_success() } else { theme.style_normal() }
                    .add_modifier(Modifier::BOLD);
                spans.push(Span::styled(format!(" [ {} ] ", c.to_ascii_uppercase()), style));
            }
            spans.push(Span::styled(" [ Enter ] ", theme.style_secondary()));
            frame.render_widget(Paragraph::new(Line::from(spans)).alignment(Alignment::Center), rows[3]);
        }

        let grown_up = if self.grown_up.is_some() {
            "Grown-ups: now type quit, mute or talk"
        } else if self.muted {
            "Grown-ups: Ctrl+G then quit to leave, talk to turn the voice on"
        } else {
            "Grown-ups: Ctrl+G then quit to leave, mute to turn the voice off"
        };
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(grown_up, theme.style_dim()))).alignment(Alignment::Center),
            rows[4],
        );
    }

    /// The tiny station: the rooms in a row, the astronaut in this one
    fn render_station(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let mut names = Vec::new();
        let mut marker = Vec::new();
        for (i, room) in ROOMS.iter().enumerate() {
            let label = format!("[ {} ]", room.name);
            let style = if i == self.room { theme.style_accent().add_modifier(Modifier::BOLD) } else { theme.style_dim() };
            let width = label.chars().count();
            names.push(Span::styled(label, style));
            names.push(Span::raw("  "));
            let pad = width / 2;
            let mark = if i == self.room { "@" } else { " " };
            marker.push(Span::styled(format!("{:>w$}{:<rest$}", mark, "", w = pad + 1, rest = width + 1 - pad), theme.style_accent()));
        }
        let lines = vec![Line::from(marker), Line::from(names)];
        let top = area.y + area.height.saturating_sub(2) / 2;
        frame.render_widget(
            Paragraph::new(lines).alignment(Alignment::Center),
            Rect { y: top, height: 2.min(area.height), ..area },
        );
    }

    fn render_room(&self, frame: &mut Frame, area: Rect, theme: &Theme, tint: Style) {
        let room = &ROOMS[self.room];
        let height = room.things.iter().map(|(_, art)| art.len()).max().unwrap_or(0);
        let mut lines: Vec<Line> = Vec::new();
        for row in 0..height {
            let mut spans = Vec::new();
            for (_, art) in room.things {
                let width = art.iter().map(|l| l.chars().count()).max().unwrap_or(0);
                let text = art.get(row).copied().unwrap_or("");
                spans.push(Span::styled(format!("{text:<width$}     "), tint));
            }
            lines.push(Line::from(spans));
        }
        let mut names = Vec::new();
        for (name, art) in room.things {
            let width = art.iter().map(|l| l.chars().count()).max().unwrap_or(0);
            names.push(Span::styled(format!("{name:<width$}     "), theme.style_normal()));
        }
        lines.push(Line::from(""));
        lines.push(Line::from(names));
        let total = lines.len() as u16;
        let top = area.y + area.height.saturating_sub(total) / 2;
        frame.render_widget(
            Paragraph::new(lines).alignment(Alignment::Center),
            Rect { y: top, height: total.min(area.height), ..area },
        );
    }
}

/// "ls" -> "l s", so each letter reads as a key to press
fn spaced(word: &str) -> String {
    word.chars().map(|c| c.to_string()).collect::<Vec<_>>().join(" ")
}

/// The character a key shows big, if any
fn big_char(code: KeyCode) -> Option<char> {
    match code {
        KeyCode::Char(c) if c.is_ascii_alphanumeric() => Some(c.to_ascii_uppercase()),
        _ => None,
    }
}

/// 5x7 pictures of A-Z and 0-9
fn glyph(c: char) -> Option<[&'static str; 7]> {
    Some(match c {
        'A' => [".###.", "#...#", "#...#", "#####", "#...#", "#...#", "#...#"],
        'B' => ["####.", "#...#", "#...#", "####.", "#...#", "#...#", "####."],
        'C' => [".###.", "#...#", "#....", "#....", "#....", "#...#", ".###."],
        'D' => ["####.", "#...#", "#...#", "#...#", "#...#", "#...#", "####."],
        'E' => ["#####", "#....", "#....", "####.", "#....", "#....", "#####"],
        'F' => ["#####", "#....", "#....", "####.", "#....", "#....", "#...."],
        'G' => [".###.", "#...#", "#....", "#.###", "#...#", "#...#", ".###."],
        'H' => ["#...#", "#...#", "#...#", "#####", "#...#", "#...#", "#...#"],
        'I' => [".###.", "..#..", "..#..", "..#..", "..#..", "..#..", ".###."],
        'J' => ["..###", "...#.", "...#.", "...#.", "...#.", "#..#.", ".##.."],
        'K' => ["#...#", "#..#.", "#.#..", "##...", "#.#..", "#..#.", "#...#"],
        'L' => ["#....", "#....", "#....", "#....", "#....", "#....", "#####"],
        'M' => ["#...#", "##.##", "#.#.#", "#.#.#", "#...#", "#...#", "#...#"],
        'N' => ["#...#", "#...#", "##..#", "#.#.#", "#..##", "#...#", "#...#"],
        'O' => [".###.", "#...#", "#...#", "#...#", "#...#", "#...#", ".###."],
        'P' => ["####.", "#...#", "#...#", "####.", "#....", "#....", "#...."],
        'Q' => [".###.", "#...#", "#...#", "#...#", "#.#.#", "#..#.", ".##.#"],
        'R' => ["####.", "#...#", "#...#", "####.", "#.#..", "#..#.", "#...#"],
        'S' => [".####", "#....", "#....", ".###.", "....#", "....#", "####."],
        'T' => ["#####", "..#..", "..#..", "..#..", "..#..", "..#..", "..#.."],
        'U' => ["#...#", "#...#", "#...#", "#...#", "#...#", "#...#", ".###."],
        'V' => ["#...#", "#...#", "#...#", "#...#", "#...#", ".#.#.", "..#.."],
        'W' => ["#...#", "#...#", "#...#", "#.#.#", "#.#.#", "#.#.#", ".#.#."],
        'X' => ["#...#", "#...#", ".#.#.", "..#..", ".#.#.", "#...#", "#...#"],
        'Y' => ["#...#", "#...#", ".#.#.", "..#..", "..#..", "..#..", "..#.."],
        'Z' => ["#####", "....#", "...#.", "..#..", ".#...", "#....", "#####"],
        '0' => [".###.", "#...#", "#..##", "#.#.#", "##..#", "#...#", ".###."],
        '1' => ["..#..", ".##..", "..#..", "..#..", "..#..", "..#..", ".###."],
        '2' => [".###.", "#...#", "....#", "...#.", "..#..", ".#...", "#####"],
        '3' => ["####.", "....#", "....#", ".###.", "....#", "....#", "####."],
        '4' => ["...#.", "..##.", ".#.#.", "#..#.", "#####", "...#.", "...#."],
        '5' => ["#####", "#....", "####.", "....#", "....#", "#...#", ".###."],
        '6' => [".###.", "#....", "#....", "####.", "#...#", "#...#", ".###."],
        '7' => ["#####", "....#", "...#.", "..#..", ".#...", ".#...", ".#..."],
        '8' => [".###.", "#...#", "#...#", ".###.", "#...#", "#...#", ".###."],
        '9' => [".###.", "#...#", "#...#", ".####", "....#", "....#", ".###."],
        _ => return None,
    })
}

/// A character drawn as big as fits in `area`, as lines of text
fn big_glyph(c: char, area: Rect) -> Vec<String> {
    let Some(rows) = glyph(c) else { return vec![c.to_string()] };
    // Each pixel is `scale` rows tall and 2*scale columns wide (terminal
    // cells are about twice as tall as they are wide)
    let scale = (area.height / 7).min(area.width / 10).clamp(1, 4) as usize;
    let mut out = Vec::new();
    for row in rows {
        let line: String = row
            .chars()
            .map(|p| if p == '#' { "█".repeat(2 * scale) } else { " ".repeat(2 * scale) })
            .collect();
        for _ in 0..scale {
            out.push(line.clone());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    #[derive(Clone, Default)]
    struct Heard(Arc<Mutex<Vec<String>>>);

    impl Speaker for Heard {
        fn say(&mut self, words: &str) {
            self.0.lock().unwrap().push(words.to_string());
        }
    }

    fn keys() -> (FirstKeys, Heard) {
        let heard = Heard::default();
        (FirstKeys::new(Box::new(heard.clone())), heard)
    }

    fn press(k: &mut FirstKeys, code: KeyCode) -> KeyOutcome {
        k.handle_key(KeyEvent::new(code, KeyModifiers::NONE))
    }

    fn ctrl(k: &mut FirstKeys, c: char) -> KeyOutcome {
        k.handle_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL))
    }

    fn typed(k: &mut FirstKeys, text: &str) -> KeyOutcome {
        let mut last = KeyOutcome::Stay;
        for c in text.chars() {
            last = press(k, KeyCode::Char(c));
        }
        last
    }

    #[test]
    fn mashing_never_leaves() {
        let (mut k, _) = keys();
        for code in [
            KeyCode::Esc,
            KeyCode::Char('q'),
            KeyCode::Tab,
            KeyCode::Enter,
            KeyCode::F(1),
            KeyCode::Backspace,
            KeyCode::Up,
        ] {
            assert_eq!(press(&mut k, code), KeyOutcome::Stay);
        }
        for c in ['c', 'q', 'l', 'k', 'd', 'z'] {
            assert_eq!(ctrl(&mut k, c), KeyOutcome::Stay);
        }
        assert_eq!(typed(&mut k, "quit"), KeyOutcome::Stay, "quit without Ctrl+G is just letters");
        // Ctrl+G then a wrong letter cancels; Ctrl+G then quit leaves
        ctrl(&mut k, 'g');
        assert_eq!(typed(&mut k, "qx"), KeyOutcome::Stay);
        assert_eq!(typed(&mut k, "uit"), KeyOutcome::Stay);
        ctrl(&mut k, 'g');
        assert_eq!(typed(&mut k, "quit"), KeyOutcome::Exit);
    }

    #[test]
    fn letters_are_shown_and_said() {
        let (mut k, heard) = keys();
        press(&mut k, KeyCode::Char('b'));
        assert_eq!(k.shown, Some('B'));
        assert_eq!(heard.0.lock().unwrap().last().unwrap(), "B");
        press(&mut k, KeyCode::Char('7'));
        assert_eq!(k.shown, Some('7'));
        // keys without a letter say nothing new
        let before = heard.0.lock().unwrap().len();
        press(&mut k, KeyCode::Char('%'));
        assert_eq!(heard.0.lock().unwrap().len(), before);
    }

    #[test]
    fn mute_and_talk() {
        let (mut k, heard) = keys();
        ctrl(&mut k, 'g');
        typed(&mut k, "mute");
        let before = heard.0.lock().unwrap().len();
        press(&mut k, KeyCode::Char('a'));
        assert_eq!(heard.0.lock().unwrap().len(), before);
        ctrl(&mut k, 'g');
        typed(&mut k, "talk");
        press(&mut k, KeyCode::Char('a'));
        assert_eq!(heard.0.lock().unwrap().last().unwrap(), "A");
    }

    #[test]
    fn three_stages_end_with_the_magic_words() {
        let (mut k, _) = keys();
        for _ in 0..ANY_KEY_PRESSES {
            press(&mut k, KeyCode::Char('x'));
        }
        assert_eq!(k.stage(), Stage::FindLetter);
        // a wrong letter is fine; the right one counts
        press(&mut k, KeyCode::Char('z'));
        assert_eq!(k.found, 0);
        for _ in 0..LETTERS_TO_FIND {
            let target = k.target.to_ascii_lowercase();
            press(&mut k, KeyCode::Char(target));
        }
        assert_eq!(k.stage(), Stage::MagicWords);

        assert!(!k.finished());
        typed(&mut k, "ls");
        press(&mut k, KeyCode::Enter);
        assert!(k.showing_room);
        typed(&mut k, "cd");
        press(&mut k, KeyCode::Enter);
        assert_eq!(ROOMS[k.room].name, "bridge");
        typed(&mut k, "xyz");
        press(&mut k, KeyCode::Enter);
        assert!(k.message.contains("p w d"), "{}", k.message);
        typed(&mut k, "pwd");
        press(&mut k, KeyCode::Enter);
        assert!(k.finished());
    }

    #[test]
    fn every_letter_and_digit_has_a_picture() {
        for c in ('A'..='Z').chain('0'..='9') {
            let rows = glyph(c).unwrap_or_else(|| panic!("no glyph for {c}"));
            assert!(rows.iter().all(|r| r.len() == 5), "{c}");
        }
        let area = Rect::new(0, 0, 100, 30);
        let big = big_glyph('A', area);
        assert_eq!(big.len(), 7 * 4);
    }

    #[test]
    fn renders_at_small_and_large_sizes() {
        let theme = crate::theme::Theme::default();
        for (w, h) in [(80, 24), (120, 36), (200, 60)] {
            let backend = ratatui::backend::TestBackend::new(w, h);
            let mut terminal = ratatui::Terminal::new(backend).unwrap();
            let (mut k, _) = keys();
            press(&mut k, KeyCode::Char('l'));
            terminal.draw(|f| k.render(f, f.size(), &theme)).unwrap();
            let screen = format!("{:?}", terminal.backend().buffer());
            assert!(screen.contains("Grown-ups"), "{w}x{h}");
            assert!(screen.contains('█'), "{w}x{h}");
        }
    }
}
