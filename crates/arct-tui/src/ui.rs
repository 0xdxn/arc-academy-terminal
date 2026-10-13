//! UI rendering and layout

use crate::app::App;
use crate::footer::{self, FooterMode};
use crate::icons;
use crate::panels::{
    context::ContextPanel,
    dashboard,
    explanation::ExplanationPanel,
    help::HelpPanel,
    PanelId,
};
use crate::theme::Theme;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::Modifier,
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame,
};

/// Minimum terminal dimensions for usable display
const MIN_WIDTH: u16 = 40;
const MIN_HEIGHT: u16 = 12;

/// Screen rectangles of the main panels, captured every frame so mouse
/// clicks can be hit-tested against the live layout
#[derive(Debug, Clone, Copy, Default)]
pub struct PanelRects {
    pub context: Rect,
    pub explanation: Rect,
    pub output: Rect,
    pub shell: Rect,
}

impl PanelRects {
    /// Which panel (if any) contains the given screen position
    pub fn panel_at(&self, column: u16, row: u16) -> Option<PanelId> {
        let hit = |r: Rect| {
            r.width > 0
                && r.height > 0
                && column >= r.x
                && column < r.x + r.width
                && row >= r.y
                && row < r.y + r.height
        };
        if hit(self.shell) {
            Some(PanelId::Shell)
        } else if hit(self.output) {
            Some(PanelId::Output)
        } else if hit(self.explanation) {
            Some(PanelId::Explanation)
        } else if hit(self.context) {
            Some(PanelId::Context)
        } else {
            None
        }
    }
}

/// Derive the footer hint mode from the current app state,
/// mirroring the input-capture priority order in `App::handle_event`
pub fn footer_mode(app: &App) -> FooterMode {
    if app.showing_notification.is_some() {
        FooterMode::Notification
    } else if app.command_palette.is_some() {
        FooterMode::Palette
    } else if app.settings_panel.is_some() {
        FooterMode::Settings
    } else if app.lesson_menu.is_some() {
        FooterMode::LessonMenu
    } else if app.show_help
        || app.achievements_panel.is_some()
        || app.progress_panel.is_some()
        || app.challenges_panel.is_some()
    {
        FooterMode::Overlay
    } else if app.lesson_mode {
        FooterMode::Lesson
    } else if app.ai_mode {
        FooterMode::Ai
    } else if app.show_dashboard {
        FooterMode::Dashboard
    } else {
        FooterMode::Normal
    }
}

/// Main UI drawing function
pub fn draw(frame: &mut Frame, app: &mut App) {
    let size = frame.size();

    // Check minimum terminal size
    if size.width < MIN_WIDTH || size.height < MIN_HEIGHT {
        let msg = format!(
            "Terminal too small ({} x {})\nMinimum: {} x {}",
            size.width, size.height, MIN_WIDTH, MIN_HEIGHT
        );
        let paragraph = Paragraph::new(msg)
            .style(app.theme.style_warning())
            .wrap(Wrap { trim: false });
        frame.render_widget(paragraph, size);
        return;
    }

    // Main layout: header + content + footer hint bar
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Header
            Constraint::Min(0),    // Content
            Constraint::Length(1), // Footer hint bar
        ])
        .split(size);

    // Draw header
    draw_header(frame, chunks[0], app);

    // Draw main content
    draw_content(frame, chunks[1], app);

    // Persistent, context-sensitive footer hint bar
    footer::render(frame, chunks[2], &app.theme, footer_mode(app));

    // Draw onboarding wizard if active (highest priority)
    if let Some(ref wizard) = app.onboarding {
        wizard.render(frame, &app.theme);
        return;  // Don't render anything else
    }

    // Draw help overlay if active
    if app.show_help {
        let help_panel = HelpPanel::new();
        help_panel.render(frame, &app.theme);
        return;  // Don't render other overlays
    }

    // Draw settings panel if active
    if let Some(ref panel) = app.settings_panel {
        panel.render(frame, &app.theme, &app.config);
    }

    // Draw lesson menu if active
    if let Some(ref mut menu) = app.lesson_menu {
        menu.render(
            frame,
            &app.theme,
            &app.completed_lessons,
            &app.user_stats,
            &app.recommendation_engine,
            &app.lesson_progress,
        );
    }

    // Draw gamification panels if active
    if let Some(ref mut panel) = app.achievements_panel {
        panel.render(frame, &app.theme, &app.user_stats.achievements);
    }

    if let Some(ref panel) = app.progress_panel {
        let activity = app
            .analytics
            .as_ref()
            .and_then(|a| a.get_daily_activity(14).ok());
        panel.render(
            frame,
            &app.theme,
            &app.user_stats,
            &app.lesson_library,
            activity.as_deref(),
        );
    }

    if let Some(ref panel) = app.challenges_panel {
        panel.render(frame, &app.theme, &app.challenge_manager);
    }

    // Command palette overlays everything except notifications
    if let Some(ref mut palette) = app.command_palette {
        palette.render(frame, &app.theme);
    }

    // Notification popups have highest priority (render last)
    if let Some(ref notification) = app.showing_notification {
        notification.render(frame, &app.theme, app.notification_ticks);
    }
}

/// Draw the header
fn draw_header(frame: &mut Frame, area: Rect, app: &App) {
    let width = area.width as usize;

    // Responsive title - shorter on narrow terminals
    let title = if width >= 60 {
        format!("  {}ARC ACADEMY TERMINAL ", icons::lightning().content)
    } else if width >= 40 {
        format!(" {}ARCT ", icons::lightning().content)
    } else {
        format!("{}A", icons::lightning().content)
    };

    let mode_indicator = if app.lesson_mode {
        if width >= 50 { format!(" {}LESSON ", icons::lesson().content) } else { String::new() }
    } else if app.ai_mode {
        if width >= 50 { format!(" {}AI ", icons::ai().content) } else { String::new() }
    } else {
        String::new()
    };

    // Responsive version info
    let version = if width >= 80 {
        format!("v{} | {} ", env!("CARGO_PKG_VERSION"), app.theme.name)
    } else if width >= 50 {
        format!("v{} ", env!("CARGO_PKG_VERSION"))
    } else {
        String::new()
    };

    // Responsive help text - progressively shorter
    let help_text = if width >= 120 {
        " [? help] [^L lessons] [^A AI] [^T theme] [q quit] "
    } else if width >= 80 {
        " [?] [^L] [^A] [^T] [q] "
    } else if width >= 50 {
        " [? help] "
    } else {
        ""
    };

    let title_len = title.chars().count();
    let version_len = version.chars().count();
    let mode_len = mode_indicator.chars().count();
    let help_len = help_text.chars().count();

    let used_width = title_len + mode_len + version_len + help_len;
    let padding = width.saturating_sub(used_width + 2); // +2 for borders

    let mut spans = vec![
        Span::styled(&title, app.theme.style_accent().add_modifier(Modifier::BOLD)),
    ];

    if !mode_indicator.is_empty() {
        spans.push(Span::styled(&mode_indicator, app.theme.style_success().add_modifier(Modifier::BOLD)));
    }

    if !version.is_empty() {
        spans.push(Span::styled(&version, app.theme.style_info()));
    }

    if padding > 0 {
        spans.push(Span::raw(" ".repeat(padding)));
    }

    if !help_text.is_empty() {
        spans.push(Span::styled(help_text, app.theme.style_dim()));
    }

    let header_text = Line::from(spans);

    let header = Paragraph::new(header_text)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(app.theme.style_border_focused())
                .style(app.theme.style_block()),  // Set background for light themes
        );

    frame.render_widget(header, area);
}

/// Draw the main content area
fn draw_content(frame: &mut Frame, area: Rect, app: &mut App) {
    let width = area.width;

    // Responsive layout: hide context panel on very narrow terminals
    let main_chunks = if width < 60 {
        // Single column - no context panel
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Length(0), // Hide context panel
                Constraint::Min(0),    // Shell + Explanation takes all
            ])
            .split(area)
    } else if width < 100 {
        // Narrower split for medium terminals
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Min(20),   // Context panel min 20
                Constraint::Min(40),   // Shell + Explanation min 40
            ])
            .split(area)
    } else {
        // Standard split for wide terminals
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(30), // Context panel
                Constraint::Percentage(70), // Shell + Explanation
            ])
            .split(area)
    };

    // Remember panel geometry for mouse hit-testing (right side rects are
    // filled in below once the vertical split is computed)
    app.panel_rects = PanelRects {
        context: main_chunks[0],
        ..Default::default()
    };

    // Left: Context panel (only render if visible)
    if main_chunks[0].width > 0 {
        let context_panel = ContextPanel::new();

        // Get analytics summary if available
        let analytics_summary = app.analytics.as_ref()
            .and_then(|a| a.get_summary().ok());

        context_panel.render(
            frame,
            main_chunks[0],
            app.active_panel == PanelId::Context,
            &app.context,
            &app.theme,
            app.config.ai.enabled,
            analytics_summary.as_ref(),
            app.lesson_mode,
            app.virtual_fs.as_ref(),
            &app.user_stats,
            &app.challenge_manager,
        );
    }

    // Right side: Classic terminal layout (Explanation → Output → Shell at bottom)
    // This follows traditional terminal UX where input is at the bottom
    let height = main_chunks[1].height;
    let right_chunks = if height < 15 {
        // Very short terminal - minimal layout (hide explanation)
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(0),  // Hide explanation
                Constraint::Min(0),     // Everything else to output
                Constraint::Length(3),  // Shell input at bottom
            ])
            .split(main_chunks[1])
    } else if height < 25 {
        // Short terminal - compact layout
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(5),     // Explanation (smaller but visible)
                Constraint::Percentage(50), // Command output
                Constraint::Length(3),  // Shell input at bottom
            ])
            .split(main_chunks[1])
    } else {
        // Standard layout - explanation prominent for learning
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Percentage(35), // Explanation (learning focus)
                Constraint::Percentage(40), // Command output
                Constraint::Length(3),  // Shell input at bottom
            ])
            .split(main_chunks[1])
    };

    app.panel_rects.explanation = right_chunks[0];
    app.panel_rects.output = right_chunks[1];
    app.panel_rects.shell = right_chunks[2];

    // Explanation panel OR Lesson panel (top - high visibility for learning)
    if right_chunks[0].height > 0 {
        if app.lesson_mode {
            // Lesson mode - show interactive lessons
            if let Some(ref lesson_panel) = app.lesson_panel {
                lesson_panel.render(
                    frame,
                    right_chunks[0],
                    app.active_panel == PanelId::Explanation,
                    &app.theme,
                    app.config.lessons.is_real(),
                );
            }
        } else {
            // Normal mode - show explanations
            let explanation_panel = ExplanationPanel::new();
            explanation_panel.render(
                frame,
                right_chunks[0],
                app.active_panel == PanelId::Explanation,
                app.last_explanation.as_ref(),
                &app.theme,
                app.ai_mode,
                app.ai_response.as_deref(),
            );
        }
    }

    // Output panel (middle - shows command results), or the welcome
    // dashboard until the user's first keypress
    if app.show_dashboard {
        let data = dashboard::DashboardData {
            user_name: app.config.general.user_name.clone(),
            streak: app.user_stats.streak_status(arct_core::local_today()),
            level: crate::level::level_info(&app.user_stats),
            lessons_completed: app.completed_lessons.len(),
            lessons_total: app.lesson_library.all().len(),
            daily_challenge: Some(app.challenge_manager.get_daily_challenge().title),
            next_lesson: app
                .recommendation_engine
                .get_recommendations(&app.completed_lessons, &app.user_stats, 1)
                .first()
                .map(|r| r.lesson.title.clone()),
        };
        dashboard::render(frame, right_chunks[1], &app.theme, &data);
    } else {
        draw_output_panel(
            frame,
            right_chunks[1],
            app.active_panel == PanelId::Output,
            &app.last_output,
            app.output_scroll,
            &app.theme,
        );
    }

    // Shell panel (bottom - classic terminal position)
    draw_shell_panel(
        frame,
        right_chunks[2],
        app.active_panel == PanelId::Shell,
        &app.command_buffer,
        &app.completion_suggestions,
        &app.theme,
        app.ai_mode,
        &app.ai_input_buffer,
        app.ai_loading,
    );
}

/// Draw the shell panel
fn draw_shell_panel(
    frame: &mut Frame,
    area: Rect,
    focused: bool,
    command: &str,
    completions: &[String],
    theme: &Theme,
    ai_mode: bool,
    ai_input: &str,
    ai_loading: bool,
) {
    let border_style = if focused {
        theme.style_border_focused()
    } else {
        theme.style_border()
    };

    let title = if ai_mode {
        if focused {
            format!(" {}AI Assistant (Active - Ctrl+A to exit, Enter to ask) ", icons::ai().content)
        } else {
            format!(" {}AI Assistant ", icons::ai().content)
        }
    } else if focused {
        format!(" {}Shell (Active - Ctrl+A for AI, Tab to complete, ↑↓ for history) ", icons::shell().content)
    } else {
        format!(" {}Shell ", icons::shell().content)
    };

    let block = Block::default()
        .title(title)
        .title_style(theme.style_title(focused))
        .borders(Borders::ALL)
        .border_style(border_style)
        .style(theme.style_block());  // Set background for light themes

    // Build lines for the panel
    let mut lines = Vec::new();

    // First line: prompt and command (or AI input)
    if ai_mode {
        let prompt = icons::ai();
        let input_text = if ai_loading {
            Span::styled(format!("{}Thinking...", icons::loading().content), theme.style_dim())
        } else {
            Span::styled(ai_input, theme.style_normal())
        };

        let mut input_line = vec![prompt, input_text];

        // Add cursor if focused and not loading
        if focused && !ai_loading {
            input_line.push(Span::styled("█", theme.style_accent()));
        }

        lines.push(Line::from(input_line));
    } else {
        let prompt = Span::styled("$ ", theme.style_accent());
        let command_text = Span::styled(command, theme.style_normal());

        let mut command_line = vec![prompt, command_text];

        // Add cursor if focused
        if focused {
            command_line.push(Span::styled("█", theme.style_accent()));
        }

        lines.push(Line::from(command_line));
    }

    // Add completion suggestions if any (only in shell mode)
    if !ai_mode && !completions.is_empty() {
        lines.push(Line::from(""));  // Empty line
        lines.push(Line::from(vec![
            icons::hint(),
            Span::styled("Suggestions:", theme.style_dim()),
        ]));

        for completion in completions.iter().take(5) {
            lines.push(Line::from(vec![
                Span::styled("  • ", theme.style_dim()),
                Span::styled(completion, theme.style_success()),
            ]));
        }

        if completions.len() > 5 {
            lines.push(Line::from(vec![
                Span::styled(format!("  ...and {} more", completions.len() - 5), theme.style_dim()),
            ]));
        }
    }

    let paragraph = Paragraph::new(lines)
        .block(block)
        .wrap(Wrap { trim: false });

    frame.render_widget(paragraph, area);
}

/// Draw the output panel
fn draw_output_panel(
    frame: &mut Frame,
    area: Rect,
    focused: bool,
    output: &str,
    scroll_offset: usize,
    theme: &Theme,
) {
    let border_style = if focused {
        theme.style_border_focused()
    } else {
        theme.style_border()
    };

    let title = if focused {
        format!(" {}Output (↑↓ scroll) ", icons::output().content)
    } else {
        format!(" {}Output (^↑↓ scroll) ", icons::output().content)
    };

    let block = Block::default()
        .title(title)
        .title_style(theme.style_title(focused))
        .borders(Borders::ALL)
        .border_style(border_style)
        .style(theme.style_block());  // Set background for light themes

    let inner_height = area.height.saturating_sub(2) as usize; // Subtract borders

    let text = if output.is_empty() {
        vec![
            Line::from(""),
            Line::from(vec![
                Span::styled("  Ready to execute commands!", theme.style_dim()),
            ]),
            Line::from(""),
            Line::from(vec![
                Span::styled("  Type a command above and press ", theme.style_dim()),
                Span::styled("Enter", theme.style_accent()),
            ]),
        ]
    } else {
        // Parse ANSI codes for colored output!
        let all_lines: Vec<Line> = crate::ansi::parse_ansi(output);
        let total_lines = all_lines.len();

        // Show scroll indicator if there are more lines than can fit
        let mut visible_lines: Vec<Line> = all_lines
            .into_iter()
            .skip(scroll_offset)
            .take(inner_height)
            .collect();

        // Add scroll indicator at bottom if not at end
        if scroll_offset + inner_height < total_lines {
            let remaining = total_lines - (scroll_offset + inner_height);
            visible_lines.push(Line::from(vec![
                Span::styled(
                    format!("  ↓ {} more lines (press ↓ or j to scroll)", remaining),
                    theme.style_dim(),
                ),
            ]));
        }

        // Add scroll indicator at top if not at beginning
        if scroll_offset > 0 {
            visible_lines.insert(0, Line::from(vec![
                Span::styled(
                    format!("  ↑ {} lines above (press ↑ or k to scroll)", scroll_offset),
                    theme.style_dim(),
                ),
            ]));
        }

        visible_lines
    };

    let paragraph = Paragraph::new(text)
        .block(block)
        .wrap(Wrap { trim: false });

    frame.render_widget(paragraph, area);
}
