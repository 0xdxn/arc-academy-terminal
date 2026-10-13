//! Printable certificate for a finished track, or for First Keys.
//!
//! Made on the learner's own computer from their saved progress: a single
//! HTML file with no script and nothing fetched (its own CSP forbids it),
//! meant to be opened in a browser and printed. Also available as plain
//! text. Issued only for a finished track, or once First Keys' magic words
//! have all been used.

use crate::streak::track_name;
use arct_core::{LessonLibrary, StepType, Track, TrackProgress, UserStats};
use chrono::{Local, NaiveDate};

/// Achievement that marks First Keys done: every magic word used
pub const FIRST_KEYS_ACHIEVEMENT: &str = "first_keys";

/// What a certificate is for
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Award {
    /// A finished lesson track
    Track(Track),
    /// First Keys, the pre-reader mode
    FirstKeys,
}

impl Award {
    /// Display name ("Explorer", "First Keys")
    pub fn name(self) -> &'static str {
        match self {
            Award::Track(track) => track_name(track),
            Award::FirstKeys => "First Keys",
        }
    }

    /// Name as typed on the command line and used in file names
    pub fn key(self) -> &'static str {
        match self {
            Award::Track(Track::Explorer) => "explorer",
            Award::Track(Track::Operator) => "operator",
            Award::FirstKeys => "first-keys",
        }
    }

    /// The certificate's corner label ("Explorer track", "First Keys")
    fn label(self) -> String {
        match self {
            Award::Track(track) => format!("{} track", track_name(track)),
            Award::FirstKeys => "First Keys".to_string(),
        }
    }

    /// Achievement whose unlock time dates the certificate
    fn achievement(self) -> &'static str {
        match self {
            Award::Track(Track::Explorer) => "station_crew",
            Award::Track(Track::Operator) => "operator",
            Award::FirstKeys => FIRST_KEYS_ACHIEVEMENT,
        }
    }
}

/// Everything a certificate shows
#[derive(Debug, Clone)]
pub struct Certificate {
    pub award: Award,
    /// The learner's name; `None` leaves a line to write it by hand
    pub name: Option<String>,
    /// Lesson titles in track order (for First Keys, what the child did)
    pub lessons: Vec<String>,
    /// Every lesson was finished without needing the answer
    pub mastered: bool,
    /// Shell commands the track's lessons use, in the order they appear
    pub commands: Vec<String>,
    /// When the track was finished (or today, if that wasn't recorded)
    pub date: NaiveDate,
    pub version: String,
}

/// Why a certificate can't be made yet
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotYet {
    pub award: Award,
    pub done: usize,
    pub total: usize,
    pub remaining: Vec<String>,
}

impl std::fmt::Display for NotYet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.award == Award::FirstKeys {
            return writeln!(
                f,
                "The First Keys certificate comes once the magic words ls, cd and pwd have all been used in First Keys (arct first-keys)."
            );
        }
        writeln!(
            f,
            "The {} certificate comes when the track is finished: {} of {} lessons done so far.",
            self.award.name(),
            self.done,
            self.total
        )?;
        if !self.remaining.is_empty() {
            writeln!(f, "Still to do:")?;
            for title in &self.remaining {
                writeln!(f, "  - {title}")?;
            }
        }
        Ok(())
    }
}

/// When an award was earned (or today, if that wasn't recorded)
fn award_date(stats: &UserStats, award: Award) -> NaiveDate {
    stats
        .achievements
        .unlock_timestamps
        .get(award.achievement())
        .map(|t| t.with_timezone(&Local).date_naive())
        .unwrap_or_else(arct_core::local_today)
}

/// Clean up a name; blank means "leave a line to write on"
fn clean_name(name: Option<String>) -> Option<String> {
    name.map(|n| n.trim().to_string()).filter(|n| !n.is_empty())
}

impl Certificate {
    /// Build the certificate for `track`, or say what's left
    pub fn for_track(
        stats: &UserStats,
        library: &LessonLibrary,
        track: Track,
        name: Option<String>,
    ) -> Result<Self, NotYet> {
        let progress: TrackProgress = stats.track_progress(library, track);
        if !progress.is_complete() {
            return Err(NotYet {
                award: Award::Track(track),
                done: progress.completed.len(),
                total: progress.total,
                remaining: progress.remaining,
            });
        }
        let award = Award::Track(track);
        Ok(Self {
            award,
            name: clean_name(name),
            mastered: progress.is_mastered(),
            lessons: progress.completed,
            commands: track_commands(library, track),
            date: award_date(stats, award),
            version: env!("CARGO_PKG_VERSION").to_string(),
        })
    }

    /// Build the First Keys certificate, once every magic word has been
    /// used. The name is never filled in from setup: the child in First
    /// Keys is rarely the person who set arct up, so a blank line is left
    /// unless a name is given.
    pub fn first_keys(stats: &UserStats, name: Option<String>) -> Result<Self, NotYet> {
        if !stats.achievements.is_unlocked(FIRST_KEYS_ACHIEVEMENT) {
            return Err(NotYet {
                award: Award::FirstKeys,
                done: 0,
                total: 1,
                remaining: Vec::new(),
            });
        }
        Ok(Self {
            award: Award::FirstKeys,
            name: clean_name(name),
            mastered: false,
            lessons: [
                "Pressed keys and heard their names",
                "Found letters on the keyboard",
                "Looked around a room with ls",
                "Walked to the next room with cd",
                "Asked where they are with pwd",
            ]
            .map(String::from)
            .to_vec(),
            commands: ["ls", "cd", "pwd"].map(String::from).to_vec(),
            date: award_date(stats, Award::FirstKeys),
            version: env!("CARGO_PKG_VERSION").to_string(),
        })
    }

    /// File name for this certificate
    pub fn file_name(&self) -> String {
        format!("arct-{}-certificate.html", self.award.key())
    }

    /// The line under the name, without a final full stop
    fn what(&self) -> String {
        let count = number_word(self.lessons.len());
        match self.award {
            Award::Track(Track::Explorer) => {
                format!("finished the Explorer track: {count} lessons aboard Arc Station")
            }
            Award::Track(Track::Operator) => {
                format!("finished the Operator track: {count} lessons at a working shell")
            }
            Award::FirstKeys => {
                "found their first keys: letters on the keyboard, and three magic words aboard Arc Station"
                    .to_string()
            }
        }
    }

    fn lessons_heading(&self) -> &'static str {
        match self.award {
            Award::Track(_) => "Lessons",
            Award::FirstKeys => "What they did",
        }
    }

    fn commands_heading(&self) -> &'static str {
        match self.award {
            Award::Track(_) => "Commands used",
            Award::FirstKeys => "Magic words",
        }
    }

    fn long_date(&self) -> String {
        self.date.format("%-d %B %Y").to_string()
    }

    /// Plain-text certificate
    pub fn to_text(&self) -> String {
        let rule = "-".repeat(60);
        let mut out = String::new();
        out.push_str(&format!("{rule}\n  ARC ACADEMY\n  Certificate of completion\n{rule}\n\n"));
        match &self.name {
            Some(name) => out.push_str(&format!("  {name}\n")),
            None => out.push_str("  ______________________________\n"),
        }
        out.push_str(&format!("  {}.\n\n", self.what()));
        for title in &self.lessons {
            out.push_str(&format!("    {title}\n"));
        }
        if !self.commands.is_empty() {
            out.push_str(&format!("\n  {}: {}\n", self.commands_heading(), self.commands.join("  ")));
        }
        if self.mastered {
            out.push_str("\n  Every lesson finished without needing the answer.\n");
        }
        out.push_str(&format!(
            "\n  {}\n  Signed: ______________________\n\n  Made by arct {} on this computer.\n{rule}\n",
            self.long_date(),
            self.version
        ));
        out
    }

    /// Printable HTML certificate (self-contained)
    pub fn to_html(&self) -> String {
        let track = self.award.name();
        let name = match &self.name {
            Some(name) => format!(r#"<p class="name">{}</p>"#, escape(name)),
            None => r#"<p class="name blank" aria-label="Name"></p>"#.to_string(),
        };
        let lessons: String = self
            .lessons
            .iter()
            .map(|t| format!("<li>{}</li>", escape(t)))
            .collect();
        let commands = if self.commands.is_empty() {
            String::new()
        } else {
            let list: String = self
                .commands
                .iter()
                .map(|c| format!("<code>{}</code>", escape(c)))
                .collect::<Vec<_>>()
                .join(" ");
            format!(
                r#"<section><h2>{}</h2><p class="commands">{list}</p></section>"#,
                self.commands_heading()
            )
        };
        let mastery = if self.mastered {
            r#"<p class="mastery">Every lesson finished without needing the answer.</p>"#
        } else {
            ""
        };
        let year = self.date.format("%Y");
        format!(
            r##"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta http-equiv="Content-Security-Policy" content="default-src 'none'; style-src 'unsafe-inline'">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Arc Academy certificate: {track}</title>
<style>
@page {{ size: landscape; margin: 0.4in; }}
:root {{ --ink: #1d1b18; --soft: #5c564c; --paper: #fbf8f1; --spot: #1f4e79; }}
* {{ box-sizing: border-box; }}
html {{ background: #e9e4d8; }}
body {{ margin: 0; padding: 32px 16px; color: var(--ink);
  font-family: "Iowan Old Style", "Palatino Linotype", Palatino, "Book Antiqua", Georgia, serif; }}
.sheet {{ position: relative; max-width: 10in; margin: 0 auto; background: var(--paper);
  padding: 0.55in 0.7in 0.5in; border: 1px solid #d8d0bf; }}
.sheet::before {{ content: ""; position: absolute; inset: 14px; border: 1.5px solid var(--spot); pointer-events: none; }}
header {{ display: flex; justify-content: space-between; align-items: baseline;
  font-variant: small-caps; letter-spacing: 0.12em; color: var(--spot); font-size: 15px; }}
h1 {{ font-weight: normal; font-size: 46px; line-height: 1.05; margin: 0.35in 0 0.2in; }}
.lead {{ font-size: 17px; color: var(--soft); margin: 0; }}
.name {{ font-size: 34px; margin: 6px 0 4px; min-height: 48px; border-bottom: 1px solid var(--ink); max-width: 6.2in; }}
.what {{ font-size: 19px; margin: 10px 0 0; }}
.body {{ display: grid; grid-template-columns: 1.1fr 1fr; gap: 0.45in; margin-top: 0.3in; }}
h2 {{ font-variant: small-caps; letter-spacing: 0.1em; font-weight: normal; font-size: 14px; color: var(--spot); margin: 0 0 6px; }}
ol {{ margin: 0; padding-left: 1.3em; columns: 2; column-gap: 0.3in; font-size: 14.5px; line-height: 1.5; }}
.commands {{ margin: 0; line-height: 2; }}
code {{ font-family: ui-monospace, "SF Mono", Menlo, Consolas, monospace; font-size: 13px; margin-right: 0.6em; white-space: nowrap; }}
.mastery {{ margin: 14px 0 0; font-style: italic; color: var(--spot); }}
footer {{ display: flex; align-items: flex-end; gap: 0.5in; margin-top: 0.4in; }}
.line {{ flex: 0 0 2.6in; border-top: 1px solid var(--ink); padding-top: 4px; font-size: 13px; color: var(--soft); }}
.line strong {{ display: block; font-weight: normal; font-size: 16px; color: var(--ink); margin-top: -30px; margin-bottom: 12px; }}
.seal {{ margin-left: auto; width: 1.25in; height: 1.25in; color: var(--spot); }}
.made {{ font-size: 11.5px; color: var(--soft); margin: 0.25in 0 0; }}
@media print {{
  html, body {{ background: none; padding: 0; }}
  .sheet {{ border: none; max-width: none; }}
}}
</style>
</head>
<body>
<main class="sheet">
<header><span>Arc Academy</span><span>{label}</span></header>
<h1>Certificate of completion</h1>
<p class="lead">This certifies that</p>
{name}
<p class="what">{what}.</p>
<div class="body">
<section><h2>{lessons_heading}</h2><ol>{lessons}</ol></section>
<div>{commands}{mastery}</div>
</div>
<footer>
<p class="line"><strong>{date}</strong>Date</p>
<p class="line"><strong>&nbsp;</strong>Signed</p>
<svg class="seal" viewBox="0 0 120 120" role="img" aria-label="Arc Academy {track} seal">
<defs><path id="ring" d="M60,60 m-44,0 a44,44 0 1,1 88,0 a44,44 0 1,1 -88,0"/></defs>
<circle cx="60" cy="60" r="57" fill="none" stroke="currentColor" stroke-width="2"/>
<circle cx="60" cy="60" r="33" fill="none" stroke="currentColor" stroke-width="1"/>
<text font-size="11" letter-spacing="2.2" fill="currentColor" font-family="ui-monospace, Menlo, monospace"><textPath href="#ring" textLength="270" lengthAdjust="spacing">ARC ACADEMY · {track_upper} · {year} ·</textPath></text>
<text x="60" y="66" text-anchor="middle" font-size="17" fill="currentColor" font-family="ui-monospace, Menlo, monospace">~/$</text>
</svg>
</footer>
<p class="made">Made by arct {version} on this computer from the learner's own progress.</p>
</main>
</body>
</html>
"##,
            track = track,
            track_upper = track.to_uppercase(),
            label = escape(&self.award.label()),
            name = name,
            what = escape(&self.what()),
            lessons_heading = self.lessons_heading(),
            lessons = lessons,
            commands = commands,
            mastery = mastery,
            date = escape(&self.long_date()),
            year = year,
            version = escape(&self.version),
        )
    }
}

/// Shell commands a track's lessons use: the first word of each command in
/// expected answers and solutions (including after pipes and `&&`)
pub fn track_commands(library: &LessonLibrary, track: Track) -> Vec<String> {
    let mut seen = Vec::<String>::new();
    for lesson in library.all().into_iter().filter(|l| l.track == track) {
        for step in &lesson.steps {
            let mut lines: Vec<&str> = step.solution.iter().map(String::as_str).collect();
            if let StepType::CommandExercise { expected_command, .. } = &step.step_type {
                lines.push(expected_command);
            }
            for line in lines {
                for part in line.split(['|', ';']).flat_map(|p| p.split("&&")) {
                    let Some(word) = part.split_whitespace().next() else { continue };
                    // Command names only: no paths like ./greet.sh, no VAR=value
                    let ok = word
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'));
                    if ok && !word.contains('=') && !seen.iter().any(|w| w == word) {
                        seen.push(word.to_string());
                    }
                }
            }
        }
    }
    seen
}

/// Escape text for HTML element content and attribute values
fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

/// "ten" for 10; digits past twenty
fn number_word(n: usize) -> String {
    const WORDS: [&str; 21] = [
        "zero", "one", "two", "three", "four", "five", "six", "seven", "eight", "nine", "ten",
        "eleven", "twelve", "thirteen", "fourteen", "fifteen", "sixteen", "seventeen", "eighteen",
        "nineteen", "twenty",
    ];
    WORDS.get(n).map(|w| w.to_string()).unwrap_or_else(|| n.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn finished(track: Track) -> (UserStats, LessonLibrary) {
        let library = LessonLibrary::new();
        let mut stats = UserStats::new();
        for lesson in library.all().into_iter().filter(|l| l.track == track) {
            stats.lessons_completed.insert(lesson.id.clone());
        }
        (stats, library)
    }

    #[test]
    fn unfinished_track_says_whats_left() {
        let library = LessonLibrary::new();
        let stats = UserStats::new();
        let not_yet = Certificate::for_track(&stats, &library, Track::Explorer, None).unwrap_err();
        assert_eq!(not_yet.done, 0);
        assert_eq!(not_yet.remaining.len(), not_yet.total);
        assert!(not_yet.to_string().contains("Still to do"));
    }

    #[test]
    fn finished_track_makes_a_self_contained_page() {
        let (stats, library) = finished(Track::Explorer);
        let cert =
            Certificate::for_track(&stats, &library, Track::Explorer, Some("Ada".into())).unwrap();
        let html = cert.to_html();
        assert!(html.contains("Ada"));
        assert!(html.contains("Explorer track"));
        assert!(html.contains("default-src 'none'"));
        // Nothing runs and nothing is fetched
        for banned in ["<script", "http://", "https://", "src=", "<link", "@import", "url("] {
            assert!(!html.contains(banned), "certificate contains {banned}");
        }
        assert!(!cert.mastered);
        assert!(cert.commands.contains(&"pwd".to_string()));
        assert!(cert.commands.iter().all(|c| !c.contains('/')), "{:?}", cert.commands);
        assert_eq!(cert.file_name(), "arct-explorer-certificate.html");
    }

    #[test]
    fn names_and_titles_are_escaped() {
        let (stats, library) = finished(Track::Explorer);
        let cert = Certificate::for_track(
            &stats,
            &library,
            Track::Explorer,
            Some("<img src=x onerror=alert(1)>".into()),
        )
        .unwrap();
        let html = cert.to_html();
        assert!(!html.contains("<img"));
        assert!(html.contains("&lt;img src=x onerror=alert(1)&gt;"));
    }

    #[test]
    fn blank_name_leaves_a_line_to_write_on() {
        let (stats, library) = finished(Track::Explorer);
        let cert =
            Certificate::for_track(&stats, &library, Track::Explorer, Some("  ".into())).unwrap();
        assert!(cert.name.is_none());
        assert!(cert.to_html().contains(r#"class="name blank""#));
        assert!(cert.to_text().contains("______"));
    }

    #[test]
    fn mastery_shows_when_every_lesson_was_mastered() {
        let (mut stats, library) = finished(Track::Explorer);
        let ids: Vec<String> = stats.lessons_completed.iter().cloned().collect();
        for id in ids {
            stats.record_lesson_mastered(id);
        }
        let cert = Certificate::for_track(&stats, &library, Track::Explorer, None).unwrap();
        assert!(cert.mastered);
        assert!(cert.to_text().contains("without needing the answer"));
    }

    #[test]
    fn first_keys_certificate_waits_for_the_magic_words() {
        let mut stats = UserStats::new();
        let not_yet = Certificate::first_keys(&stats, None).unwrap_err();
        assert_eq!(not_yet.award, Award::FirstKeys);
        assert!(not_yet.to_string().contains("arct first-keys"));

        stats.record_event("first_keys_complete");
        let cert = Certificate::first_keys(&stats, None).unwrap();
        assert!(cert.name.is_none(), "a blank line for the grown-up to write the name");
        assert_eq!(cert.file_name(), "arct-first-keys-certificate.html");
        assert_eq!(cert.commands, ["ls", "cd", "pwd"]);
        let html = cert.to_html();
        assert!(html.contains("First Keys"));
        assert!(html.contains("Magic words"));
        assert!(html.contains("found their first keys"));
        assert!(!html.contains("First Keys track"));
        for banned in ["<script", "http://", "https://", "src=", "<link", "@import", "url("] {
            assert!(!html.contains(banned), "certificate contains {banned}");
        }
        assert!(cert.to_text().contains("Magic words: ls  cd  pwd"));
    }

    #[test]
    fn numbers_read_as_words() {
        assert_eq!(number_word(10), "ten");
        assert_eq!(number_word(31), "31");
    }
}
