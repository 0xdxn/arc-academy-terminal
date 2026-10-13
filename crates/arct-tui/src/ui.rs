//! UI rendering and layout

use crate::app::App;
use crate::footer::{self, FooterMode};
use crate::icons;
use crate::panels::{
    context::{ContextPanel, SidebarData},
    dashboard,
    explanation::ExplanationPanel,
    help::HelpPanel,
    PanelId,
};
use crate::theme::Theme;
use crate::transcript::Transcript;
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

    // First Keys owns the whole screen, at any size
    if let Some(keys) = app.first_keys.as_ref() {
        keys.render(frame, size, &app.theme);
        return;
    }

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
            Constraint::Length(1), // Header
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

/// Draw the header: one calm line, the name and the mode (keys live in
/// the footer)
fn draw_header(frame: &mut Frame, area: Rect, app: &App) {
    let width = area.width as usize;
    let title = if width >= 40 { " ARC ACADEMY TERMINAL" } else { " ARCT" };
    let mode = if app.lesson_mode {
        "  lessons"
    } else if app.ai_mode {
        "  AI assistant"
    } else {
        ""
    };
    let version = format!("v{} ", env!("CARGO_PKG_VERSION"));

    let used = title.chars().count() + mode.chars().count() + version.chars().count();
    let mut spans = vec![
        Span::styled(title, app.theme.style_accent().add_modifier(Modifier::BOLD)),
        Span::styled(mode, app.theme.style_secondary()),
    ];
    if width > used {
        spans.push(Span::raw(" ".repeat(width - used)));
        spans.push(Span::styled(version, app.theme.style_dim()));
    }

    frame.render_widget(
        Paragraph::new(Line::from(spans)).style(app.theme.style_block()),
        area,
    );
}

/// Side panel width for a terminal `width` columns wide (0: hidden)
pub fn sidebar_width(width: u16, visible: bool) -> u16 {
    if !visible || width < 80 {
        0
    } else if width >= 110 {
        30
    } else {
        26
    }
}

/// Rows the shell input box needs (the input line, plus any suggestions)
fn shell_height(completions: usize) -> u16 {
    match completions {
        0 => 3,
        n => 3 + 1 + n.min(5) as u16 + u16::from(n > 5),
    }
}

/// Height of the top panel (lesson step or explanation) in a right-hand
/// column `height` rows tall. A lesson step gets the rows its text needs,
/// leaving the transcript a few rows; a step longer than that scrolls.
fn top_panel_height(app: &App, width: u16, height: u16, shell: u16) -> u16 {
    if height < 15 {
        return 0;
    }
    let room = height.saturating_sub(shell);
    if app.lesson_mode {
        let Some(panel) = app.lesson_panel.as_ref() else {
            return 0;
        };
        let min_transcript = if panel.is_reading_step() { 4 } else { 8 };
        let cap = room.saturating_sub(min_transcript).max(5);
        panel.wanted_height(&app.theme, width).clamp(5, cap)
    } else {
        // An explanation gets the rows it needs, up to about a third
        let cap = if height < 25 { (room / 3).max(5) } else { height * 35 / 100 };
        match (&app.last_explanation, app.ai_mode) {
            (Some(exp), false) => ExplanationPanel::wanted_height(exp, &app.theme, width).clamp(5, cap),
            _ => cap,
        }
    }
}

/// Draw the main content area
fn draw_content(frame: &mut Frame, area: Rect, app: &mut App) {
    let side = sidebar_width(area.width, app.sidebar_visible);
    let main_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(side), Constraint::Min(0)])
        .split(area);

    // Remember panel geometry for mouse hit-testing (right side rects are
    // filled in below once the vertical split is computed)
    app.panel_rects = PanelRects {
        context: main_chunks[0],
        ..Default::default()
    };

    let folder = app.prompt_folder();

    // Left: where you are (only render if visible)
    if main_chunks[0].width > 0 {
        let real_dir = app.prompt_real_dir();
        let lessons_done = app.completed_lessons.len();
        let lessons_total = app.lesson_library.all().len();
        let progress = vec![
            format!("{lessons_done} of {lessons_total} lessons done"),
            crate::streak::short(app.user_stats.streak_status(arct_core::local_today())),
        ];
        let data = SidebarData {
            folder: &folder,
            virtual_fs: if app.lesson_mode { app.virtual_fs.as_ref() } else { None },
            real_dir: real_dir.as_deref(),
            progress,
        };
        ContextPanel::new().render(
            frame,
            main_chunks[0],
            app.active_panel == PanelId::Context,
            &app.theme,
            &data,
        );
    }

    // Right side: lesson step or explanation, the transcript, and the
    // input line at the bottom like a terminal
    let right = main_chunks[1];
    let shell = shell_height(if app.ai_mode { 0 } else { app.completion_suggestions.len() });
    let top = top_panel_height(app, right.width, right.height, shell);
    let right_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(top),
            Constraint::Min(0),
            Constraint::Length(shell),
        ])
        .split(right);

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
            app.explanation_max_scroll = explanation_panel.render(
                frame,
                right_chunks[0],
                app.active_panel == PanelId::Explanation,
                app.last_explanation.as_ref(),
                &app.theme,
                app.ai_mode,
                app.ai_response.as_deref(),
                app.explanation_scroll,
            );
        }
    }

    // Transcript (middle), or the welcome dashboard until the user's
    // first keypress
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
            &app.transcript,
            &app.theme,
        );
    }

    // Shell panel (bottom - classic terminal position)
    draw_shell_panel(
        frame,
        right_chunks[2],
        app.active_panel == PanelId::Shell,
        &folder,
        &app.command_buffer,
        &app.completion_suggestions,
        &app.theme,
        app.ai_mode,
        &app.ai_input_buffer,
        app.ai_loading,
    );
}

/// Draw the shell input box: the same folder prompt as the transcript
#[allow(clippy::too_many_arguments)]
fn draw_shell_panel(
    frame: &mut Frame,
    area: Rect,
    focused: bool,
    folder: &str,
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

    let title = if ai_mode { " AI Assistant " } else { " Shell " };

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
        let prompt = Span::styled("ask: ", theme.style_accent());
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
        let mut command_line = vec![
            Span::styled(folder.to_string(), theme.style_accent()),
            Span::styled(" $ ", theme.style_accent()),
            Span::styled(command, theme.style_normal()),
        ];

        // Add cursor if focused
        if focused {
            command_line.push(Span::styled("█", theme.style_accent()));
        }

        lines.push(Line::from(command_line));
    }

    // Add completion suggestions if any (only in shell mode)
    if !ai_mode && !completions.is_empty() {
        lines.push(Line::from(Span::styled("Suggestions (Tab):", theme.style_dim())));

        for completion in completions.iter().take(5) {
            lines.push(Line::from(vec![
                Span::styled("  ", theme.style_dim()),
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

/// Draw the transcript: every command with its output, newest at the
/// bottom, scrolled back when the learner has scrolled
fn draw_output_panel(
    frame: &mut Frame,
    area: Rect,
    focused: bool,
    transcript: &Transcript,
    theme: &Theme,
) {
    use ratatui::widgets::block::Title;

    let border_style = if focused {
        theme.style_border_focused()
    } else {
        theme.style_border()
    };

    let inner_width = area.width.saturating_sub(2) as usize;
    let inner_height = area.height.saturating_sub(2) as usize;

    let (rows, below) = if transcript.is_empty() {
        (
            vec![Line::from(vec![
                Span::styled("Type a command below and press ", theme.style_dim()),
                Span::styled("Enter", theme.style_accent()),
                Span::styled(". What you run, and what it prints, shows up here.", theme.style_dim()),
            ])],
            0,
        )
    } else {
        transcript.view(theme, inner_width, inner_height)
    };

    let mut block = Block::default()
        .title(" Terminal ")
        .title_style(theme.style_title(focused))
        .borders(Borders::ALL)
        .border_style(border_style)
        .style(theme.style_block());  // Set background for light themes
    if below > 0 {
        block = block.title(
            Title::from(Span::styled(
                format!(" {below} more line{} below: Ctrl+D or type to return ", if below == 1 { "" } else { "s" }),
                theme.style_accent(),
            ))
            .position(ratatui::widgets::block::Position::Bottom)
            .alignment(ratatui::layout::Alignment::Right),
        );
    }

    let wrap = transcript.is_empty();
    let mut paragraph = Paragraph::new(rows).block(block);
    if wrap {
        paragraph = paragraph.wrap(Wrap { trim: false });
    }
    frame.render_widget(paragraph, area);
}
