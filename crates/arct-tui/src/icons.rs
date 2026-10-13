//! Icon constants and helpers.
//!
//! By default every icon is a plain symbol one terminal cell wide, so panels
//! line up in any terminal and font. Nerd Font glyphs are opt-in
//! (`theme.nerd_fonts = true`) because without a patched font they render as
//! empty boxes.

use ratatui::style::{Color, Style};
use ratatui::text::Span;
use std::sync::atomic::{AtomicBool, Ordering};

static NERD_FONTS: AtomicBool = AtomicBool::new(false);

/// Choose Nerd Font glyphs (true) or plain symbols (false, the default).
pub fn set_nerd_fonts(enabled: bool) {
    NERD_FONTS.store(enabled, Ordering::Relaxed);
}

fn pick(nerd: &'static str, plain: &'static str) -> &'static str {
    if NERD_FONTS.load(Ordering::Relaxed) {
        nerd
    } else {
        plain
    }
}

/// Folder glyph for file trees
pub fn folder_glyph() -> &'static str {
    pick(FOLDER, "▸")
}

/// File glyph for file trees
pub fn file_glyph() -> &'static str {
    pick(FILE, "·")
}

/// Timer glyph
pub fn timer_glyph() -> &'static str {
    pick(TIMER, "◷")
}

/// Application branding icons
pub const TERMINAL: &str = "\u{f489}";  //  - Terminal icon
pub const LIGHTNING: &str = "\u{f0e7}"; //  - Lightning bolt
pub const SHELL: &str = "\u{f489}";     //  - Shell prompt

/// Mode indicators
pub const LESSON: &str = "\u{f02d}";    //  - Book icon for lesson mode
pub const AI: &str = "\u{f544}";        //  - Robot head (cooler version)
pub const LEARNING: &str = "\u{f19d}";  //  - Graduation cap

/// Status indicators
pub const SUCCESS: &str = "\u{f058}";   //  - Check circle (filled)
pub const ERROR: &str = "\u{f057}";     //  - Times circle (filled)
pub const WARNING: &str = "\u{f071}";   //  - Exclamation triangle
pub const INFO: &str = "\u{f05a}";      //  - Info circle
pub const HINT: &str = "\u{f0eb}";      //  - Lightbulb
pub const LOADING: &str = "\u{f110}";   //  - Spinner/loading
pub const TIMER: &str = "\u{f017}";     //  - Clock

/// File system icons
pub const FOLDER: &str = "\u{f07c}";    //  - Open folder
pub const FILE: &str = "\u{f15b}";      //  - File
pub const FOLDER_OPEN: &str = "\u{f07c}"; //  - Open folder
pub const LOCATION: &str = "\u{f3c5}";  //  - Pin/location marker

/// Learning/Education icons
pub const NOTE: &str = "\u{f040}";      //  - Pencil/note
pub const QUESTION: &str = "\u{f059}";  //  - Question mark circle
pub const TARGET: &str = "\u{f140}";    //  - Target/goal
pub const BEGINNER: &str = "\u{f06c}";  //  - Leaf/seedling
pub const CELEBRATION: &str = "\u{f091}"; //  - Trophy

/// System/Environment icons
pub const GLOBE: &str = "\u{f0ac}";     //  - Globe/world
pub const LAPTOP: &str = "\u{f109}";    //  - Laptop
pub const OUTPUT: &str = "\u{f120}";    //  - Terminal window

/// UI Elements
pub const WELCOME: &str = "\u{f2b5}";   //  - Hand wave
pub const ARROW_RIGHT: &str = "\u{f061}"; //  - Arrow right
pub const BULLET: &str = "\u{f111}";    //  - Circle bullet

/// Unicode symbols (widely supported even without Nerd Fonts)
pub const CHECK_LIGHT: &str = "\u{2713}"; // ✓ - Light checkmark
pub const CROSS_LIGHT: &str = "\u{2717}"; // ✗ - Light cross
pub const ARROW_LEFT: &str = "\u{2190}";  // ← - Left arrow
pub const TRIANGLE_RIGHT: &str = "\u{25b6}"; // ▶ - Right triangle

// ============================================================================
// COLORED ICON HELPERS
// ============================================================================
// These functions return Span objects with icons styled in contextually
// appropriate colors that stand out tastefully from surrounding text.

/// Success icon in green
pub fn success() -> Span<'static> {
    Span::styled(format!("{}  ", pick(SUCCESS, "✓")), Style::default().fg(Color::Rgb(46, 204, 113)))
}

/// Error icon in red
pub fn error() -> Span<'static> {
    Span::styled(format!("{}  ", pick(ERROR, "✗")), Style::default().fg(Color::Rgb(231, 76, 60)))
}

/// Warning icon in yellow/orange
pub fn warning() -> Span<'static> {
    Span::styled(format!("{}  ", pick(WARNING, "!")), Style::default().fg(Color::Rgb(241, 196, 15)))
}

/// Info icon in blue
pub fn info() -> Span<'static> {
    Span::styled(format!("{}  ", pick(INFO, "i")), Style::default().fg(Color::Rgb(52, 152, 219)))
}

/// Hint/tip icon in purple/cyan
pub fn hint() -> Span<'static> {
    Span::styled(format!("{}  ", pick(HINT, "*")), Style::default().fg(Color::Rgb(155, 89, 182)))
}

/// AI assistant icon in cyan/green
pub fn ai() -> Span<'static> {
    Span::styled(format!("{}  ", pick(AI, "?")), Style::default().fg(Color::Rgb(26, 188, 156)))
}

/// Lesson icon in blue
pub fn lesson() -> Span<'static> {
    Span::styled(format!("{}  ", pick(LESSON, "§")), Style::default().fg(Color::Rgb(52, 152, 219)))
}

/// Learning icon in purple
pub fn learning() -> Span<'static> {
    Span::styled(format!("{}  ", pick(LEARNING, "^")), Style::default().fg(Color::Rgb(142, 68, 173)))
}

/// Folder icon in yellow/gold
pub fn folder() -> Span<'static> {
    Span::styled(format!("{}  ", pick(FOLDER, "▸")), Style::default().fg(Color::Rgb(241, 196, 15)))
}

/// File icon in light gray
pub fn file() -> Span<'static> {
    Span::styled(format!("{}  ", pick(FILE, "·")), Style::default().fg(Color::Rgb(189, 195, 199)))
}

/// Location/pin icon in orange/red
pub fn location() -> Span<'static> {
    Span::styled(format!("{}  ", pick(LOCATION, "@")), Style::default().fg(Color::Rgb(230, 126, 34)))
}

/// Target/goal icon in orange
pub fn target() -> Span<'static> {
    Span::styled(format!("{}  ", pick(TARGET, "◎")), Style::default().fg(Color::Rgb(230, 126, 34)))
}

/// Celebration icon in gold
pub fn celebration() -> Span<'static> {
    Span::styled(format!("{}  ", pick(CELEBRATION, "★")), Style::default().fg(Color::Rgb(241, 196, 15)))
}

/// Welcome icon in cyan (using star instead of handshake)
pub fn welcome() -> Span<'static> {
    Span::styled("  ", Style::default().fg(Color::Rgb(26, 188, 156)))
}

/// Lightning/power icon in bright yellow
pub fn lightning() -> Span<'static> {
    Span::styled(format!("{}  ", pick(LIGHTNING, "+")), Style::default().fg(Color::Rgb(241, 196, 15)))
}

/// Question icon in blue
pub fn question() -> Span<'static> {
    Span::styled(format!("{}  ", pick(QUESTION, "?")), Style::default().fg(Color::Rgb(52, 152, 219)))
}

/// Note/pencil icon in cyan
pub fn note() -> Span<'static> {
    Span::styled(format!("{}  ", pick(NOTE, "-")), Style::default().fg(Color::Rgb(26, 188, 156)))
}

/// Loading spinner in cyan
pub fn loading() -> Span<'static> {
    Span::styled(format!("{}  ", pick(LOADING, "…")), Style::default().fg(Color::Rgb(52, 152, 219)))
}

/// Globe icon in green
pub fn globe() -> Span<'static> {
    Span::styled(format!("{}  ", pick(GLOBE, "#")), Style::default().fg(Color::Rgb(46, 204, 113)))
}

/// Laptop icon in blue
pub fn laptop() -> Span<'static> {
    Span::styled(format!("{}  ", pick(LAPTOP, "#")), Style::default().fg(Color::Rgb(52, 152, 219)))
}

/// Shell/terminal icon in orange (Arc Academy brand color)
pub fn shell() -> Span<'static> {
    Span::styled(format!("{}  ", pick(SHELL, "$")), Style::default().fg(Color::Rgb(255, 140, 0)))
}

/// Output panel icon in gray
pub fn output() -> Span<'static> {
    Span::styled(format!("{}  ", pick(OUTPUT, ">")), Style::default().fg(Color::Rgb(149, 165, 166)))
}

/// Beginner level icon in light green
pub fn beginner() -> Span<'static> {
    Span::styled(format!("{}  ", pick(BEGINNER, "+")), Style::default().fg(Color::Rgb(46, 204, 113)))
}
