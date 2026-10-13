//! Main application state and logic

use crate::events::{Action, Event, EventHandler, key_to_action};
use crate::icons;
use crate::panels::PanelId;
use crate::shell::ShellExecutor;
use crate::theme::Theme;
use crate::ui;
use anyhow::Result;
use arct_core::{CommandAnalyzer, Context, ContextDetector, Educator, Session};
use crossterm::{
    event::{KeyCode, KeyEvent, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::{Backend, CrosstermBackend},
    Terminal,
};
use std::collections::HashMap;
use std::io;

/// Main application state
pub struct App {
    /// Whether the application should quit
    pub should_quit: bool,

    /// Currently active panel
    pub active_panel: PanelId,

    /// User session
    pub session: Session,

    /// Current context (project detection, etc.)
    pub context: Context,

    /// Command analyzer
    pub analyzer: CommandAnalyzer,

    /// Educator for explanations
    pub educator: Educator,

    /// Current theme
    pub theme: Theme,

    /// Event handler
    event_handler: EventHandler,

    /// Show help overlay
    pub show_help: bool,

    /// Command buffer for shell input
    pub command_buffer: String,

    /// Last command explanation
    pub last_explanation: Option<arct_core::Explanation>,

    /// Shell executor
    shell_executor: ShellExecutor,

    /// Last command output
    pub last_output: String,

    /// Output panel scroll offset
    pub output_scroll: usize,

    /// Command history
    command_history: Vec<String>,

    /// Current position in history (0 = most recent, None = not browsing)
    history_position: Option<usize>,

    /// Environment variables set by export command
    pub environment_vars: HashMap<String, String>,

    /// Command aliases (name -> command)
    pub aliases: HashMap<String, String>,

    /// Application configuration
    pub config: arct_config::Config,

    /// Autocompleter
    autocompleter: crate::autocomplete::Autocompleter,

    /// Current completion suggestions (shown below shell input)
    pub completion_suggestions: Vec<String>,

    /// AI assistant provider (if enabled)
    ai_provider: Option<Box<dyn arct_ai::AIProvider>>,

    /// AI conversation history
    pub ai_conversation: Vec<arct_ai::Message>,

    /// AI input buffer (when in AI mode)
    pub ai_input_buffer: String,

    /// Last AI response
    pub ai_response: Option<String>,

    /// AI loading state
    pub ai_loading: bool,

    /// AI mode enabled (toggle between shell and AI)
    pub ai_mode: bool,

    /// Onboarding wizard (shown on first run)
    pub onboarding: Option<crate::panels::onboarding::OnboardingWizard>,

    /// Settings panel (interactive)
    pub settings_panel: Option<crate::panels::settings::SettingsPanel>,

    /// Analytics tracker
    pub analytics: Option<crate::analytics::Analytics>,

    /// Current session ID
    session_id: String,

    /// Lesson panel for interactive lessons
    pub lesson_panel: Option<crate::panels::lesson::LessonPanel>,

    /// Lesson mode enabled (toggle between explanation and lesson)
    pub lesson_mode: bool,

    /// Virtual filesystem for lesson sandboxing
    pub virtual_fs: Option<arct_core::VirtualFileSystem>,

    /// The sandbox shell's variables and status (see sandbox.rs)
    pub(crate) sandbox: crate::sandbox::SandboxShell,

    /// Real-filesystem practice playground (~/ArcAcademy/playground),
    /// used when lessons.practice_mode = "real"
    pub playground: Option<arct_core::Playground>,

    /// Safe-mode home for the free shell (~/ArcAcademy/home), opened on
    /// first use when shell.safe_mode is on
    pub explore: Option<arct_core::Playground>,

    /// Lesson menu for selecting lessons
    pub lesson_menu: Option<crate::panels::lesson_menu::LessonMenuPanel>,

    /// Completed lesson IDs
    pub completed_lessons: std::collections::HashSet<String>,

    /// User statistics and progress tracking
    pub user_stats: arct_core::UserStats,

    /// Challenge manager for daily/weekly challenges
    pub challenge_manager: arct_core::ChallengeManager,

    /// Recommendation engine for suggesting lessons
    pub recommendation_engine: arct_core::RecommendationEngine,

    /// Achievements panel
    pub achievements_panel: Option<crate::panels::achievements::AchievementsPanel>,

    /// Progress panel
    pub progress_panel: Option<crate::panels::progress::ProgressPanel>,

    /// Challenges panel
    pub challenges_panel: Option<crate::panels::challenges::ChallengesPanel>,

    /// Pending notifications (achievement unlocks, challenge completions...)
    pub pending_notifications: Vec<crate::panels::notification::NotificationPanel>,

    /// Currently showing notification popup
    pub showing_notification: Option<crate::panels::notification::NotificationPanel>,

    /// Lesson library (built-ins merged with user lesson packs)
    pub lesson_library: arct_core::LessonLibrary,

    /// Per-lesson step-level resume state (persisted)
    pub lesson_progress: HashMap<String, crate::persistence::LessonResumeState>,

    /// Command palette overlay (Ctrl+K)
    pub command_palette: Option<crate::panels::command_palette::CommandPalette>,

    /// Panels visited this session (for the "visit_all_panels" achievement)
    visited_panels: std::collections::HashSet<&'static str>,

    /// Local-only telemetry sink (None when telemetry is disabled)
    telemetry: Option<arct_telemetry::Telemetry>,

    /// When the app started (for the SessionEnded telemetry event)
    app_started: std::time::Instant,

    /// Last time the session-time heartbeat folded elapsed time into stats
    last_time_update: std::time::Instant,

    /// Welcome dashboard is showing (dismissed on first keypress/click)
    pub show_dashboard: bool,

    /// Ticks since the current notification appeared (drives the brief
    /// celebratory border animation)
    pub notification_ticks: u32,

    /// Panel geometry captured by the last render (mouse hit-testing)
    pub panel_rects: crate::ui::PanelRects,
}

/// Panels that count towards the "visit_all_panels" achievement
const ALL_VISITABLE_PANELS: [&str; 6] = [
    "achievements",
    "progress",
    "challenges",
    "settings",
    "lesson_menu",
    "help",
];

/// How the safe-mode home is shown to the learner
const EXPLORE_DISPLAY: &str = "~/ArcAcademy/home";

/// Starter files written the first time the safe-mode home is created
const EXPLORE_STARTER_FILES: &[(&str, &str)] = &[
    (
        "README.txt",
        "Welcome to your Arc Academy home.\n\n\
         This folder is real: files you make here are saved on your computer.\n\
         Safe mode keeps every command inside this folder, so explore freely.\n\n\
         Try: ls, cd notes, cat todo.txt, mkdir art, touch art/robot.txt\n",
    ),
    ("notes/todo.txt", "- learn ls\n- learn cd\n- make a folder\n"),
    ("notes/ideas.txt", "A robot that waters the plants.\nA map of the stars.\n"),
    ("projects/hello.sh", "#!/bin/sh\necho \"Hello from your first script!\"\n"),
    ("photos/README.txt", "Pretend this folder is full of photos.\n"),
    (".secret.txt", "You found a hidden file! Files starting with a dot hide from plain ls.\nTry ls -a.\n"),
];

impl App {
    /// Create a new application
    pub fn new() -> Result<Self> {
        Self::new_with_options(crate::RunOptions::default())
    }

    /// Create a new application with CLI overrides (config path, theme)
    pub fn new_with_options(options: crate::RunOptions) -> Result<Self> {
        let session = Session::new();
        let working_dir = session.state.working_directory.clone();
        let context = ContextDetector::detect(&working_dir)?;

        // Load configuration (an explicit --config path is an error if unusable)
        let config = match &options.config_path {
            Some(path) => arct_config::Config::load_from(path)?,
            None => arct_config::Config::load().unwrap_or_else(|e| {
                tracing::warn!("Failed to load config, using defaults: {}", e);
                arct_config::Config::default()
            }),
        };

        crate::icons::set_nerd_fonts(config.theme.nerd_fonts);

        // Load session data (history, stats, progress) from disk
        let session_data = match crate::persistence::load_session() {
            Ok(data) => {
                tracing::info!("Loaded session with {} commands, {} completed lessons",
                    data.command_history.len(),
                    data.completed_lessons.len()
                );
                data
            }
            Err(e) => {
                tracing::warn!("Failed to load session: {}", e);
                crate::persistence::SessionData::new()
            }
        };

        let command_history = session_data.command_history;

        // Load aliases and environment variables from config
        let aliases = config.shell.aliases.clone();
        let environment_vars = config.shell.environment.clone();

        // Select theme: CLI --theme override takes precedence over config
        let theme = match &options.theme {
            Some(name) => crate::resolve_theme(name)?,
            None => Theme::from_name(&config.theme.default_theme),
        };

        // Initialize AI provider if enabled
        let ai_provider = if config.ai.enabled {
            match Self::create_ai_provider(&config.ai) {
                Ok(provider) => {
                    tracing::info!("AI provider initialized: {}", provider.name());
                    Some(provider)
                }
                Err(e) => {
                    tracing::warn!("Failed to initialize AI provider: {}", e);
                    None
                }
            }
        } else {
            None
        };

        // Check if first run (show onboarding)
        let onboarding = if !config.general.setup_complete {
            Some(crate::panels::onboarding::OnboardingWizard::new())
        } else {
            None
        };

        // Returning users see the welcome dashboard until their first key
        let show_dashboard = onboarding.is_none();

        // Create welcome message for returning users
        let welcome_message = if config.general.setup_complete {
            let name = config.general.user_name.as_deref().unwrap_or("there");
            let mut msg = format!("{}Welcome back, {}!\n\n", icons::welcome().content, name);

            // Add quick tips
            msg.push_str("Quick reminders:\n");
            if config.ai.enabled {
                msg.push_str("  • Press Ctrl+A to ask the AI for help\n");
            }
            msg.push_str("  • Press ? for help\n");
            msg.push_str("  • Press Ctrl+S for settings\n");
            msg.push_str("  • Tab to autocomplete commands\n\n");
            msg.push_str("Start typing a command to begin!\n");
            msg
        } else {
            String::new()
        };

        // Load user stats from session. Opening the app doesn't count as a
        // learning day (doing something does); only end a run whose rest
        // days have run out, so no screen shows a run that already ended.
        let mut user_stats = session_data.user_stats;
        user_stats.settle_streak(arct_core::local_today());
        // session_start is #[serde(skip)] and deserializes to the epoch;
        // reset it so the first update_session_time() doesn't count decades
        user_stats.session_start = chrono::Utc::now();

        // Load challenge manager from session
        let mut challenge_manager = session_data.challenge_manager;
        // Generate today's challenges (will use cached if same day/week)
        challenge_manager.get_daily_challenge();
        challenge_manager.get_weekly_challenge();

        // Load completed lessons from session
        let completed_lessons = session_data.completed_lessons;

        // Build the lesson library: built-ins merged with user lesson packs
        // from ~/.config/arct/lessons/*.toml
        let mut lesson_library = arct_core::LessonLibrary::new();
        if let Some(config_dir) = dirs::config_dir() {
            let user_lessons_dir = config_dir.join("arct").join("lessons");
            match lesson_library.load_from_dir(&user_lessons_dir) {
                Ok(0) => {}
                Ok(count) => tracing::info!(
                    "Loaded {} user lesson(s) from {}",
                    count,
                    user_lessons_dir.display()
                ),
                Err(e) => tracing::warn!(
                    "Failed to load user lessons from {}: {}",
                    user_lessons_dir.display(),
                    e
                ),
            }
        }

        // Local-only telemetry, strictly opt-in (config default is OFF)
        let telemetry = if config.telemetry.enabled {
            match arct_telemetry::Telemetry::new(true) {
                Ok(t) => Some(t),
                Err(e) => {
                    tracing::warn!("Failed to initialize telemetry: {}", e);
                    None
                }
            }
        } else {
            None
        };

        Ok(Self {
            should_quit: false,
            active_panel: PanelId::Shell,
            session,
            context,
            analyzer: CommandAnalyzer::new(),
            educator: Educator::new(),
            theme,
            event_handler: EventHandler::new(),
            show_help: false,
            command_buffer: String::new(),
            last_explanation: None,
            shell_executor: ShellExecutor::new()?,
            last_output: welcome_message,
            output_scroll: 0,
            command_history,
            history_position: None,
            environment_vars,
            aliases,
            config,
            autocompleter: crate::autocomplete::Autocompleter::new(),
            completion_suggestions: Vec::new(),
            ai_provider,
            ai_conversation: Vec::new(),
            ai_input_buffer: String::new(),
            ai_response: None,
            ai_loading: false,
            ai_mode: false,
            onboarding,
            settings_panel: None,
            analytics: crate::analytics::Analytics::new().ok(),
            session_id: uuid::Uuid::new_v4().to_string(),
            lesson_panel: Self::initialize_lesson_panel(),
            lesson_mode: false,
            virtual_fs: None,
            sandbox: crate::sandbox::SandboxShell::new(),
            playground: None,
            explore: None,
            lesson_menu: None,
            completed_lessons,
            user_stats,
            challenge_manager,
            recommendation_engine: arct_core::RecommendationEngine::with_library(
                lesson_library.clone(),
            ),
            achievements_panel: None,
            progress_panel: None,
            challenges_panel: None,
            pending_notifications: Vec::new(),
            showing_notification: None,
            lesson_library,
            lesson_progress: session_data.lesson_progress,
            command_palette: None,
            visited_panels: std::collections::HashSet::new(),
            telemetry,
            app_started: std::time::Instant::now(),
            last_time_update: std::time::Instant::now(),
            // Returning users land on the welcome dashboard (streak first!);
            // first-run users go through onboarding instead
            show_dashboard,
            notification_ticks: 0,
            panel_rects: crate::ui::PanelRects::default(),
        })
    }

    /// Initialize empty lesson panel (user will select lesson from menu)
    fn initialize_lesson_panel() -> Option<crate::panels::lesson::LessonPanel> {
        Some(crate::panels::lesson::LessonPanel::new())
    }

    /// Create AI provider from configuration
    fn create_ai_provider(config: &arct_config::AIConfig) -> Result<Box<dyn arct_ai::AIProvider>> {
        let ai_config = match config.provider.as_str() {
            "anthropic" => {
                let api_key = config.api_key.clone()
                    .ok_or_else(|| anyhow::anyhow!("Anthropic API key not set"))?;
                let model = config.model.clone()
                    .unwrap_or_else(|| "claude-3-5-sonnet-20241022".to_string());
                arct_ai::AIConfig::Anthropic { api_key, model }
            }
            "openai" => {
                let api_key = config.api_key.clone()
                    .ok_or_else(|| anyhow::anyhow!("OpenAI API key not set"))?;
                let model = config.model.clone()
                    .unwrap_or_else(|| "gpt-4-turbo-preview".to_string());
                arct_ai::AIConfig::OpenAI { api_key, model }
            }
            "local" => {
                let endpoint = config.endpoint.clone()
                    .unwrap_or_else(|| "http://localhost:11434".to_string());
                let model = config.model.clone();
                arct_ai::AIConfig::Local { endpoint, model }
            }
            "managed" => {
                let auth_token = config.api_key.clone()
                    .ok_or_else(|| anyhow::anyhow!("Managed API token not set"))?;
                arct_ai::AIConfig::Managed { auth_token }
            }
            "claude-cli" => {
                // Claude Code CLI - no API key needed
                let model = config.model.clone();
                arct_ai::AIConfig::ClaudeCLI { model }
            }
            _ => arct_ai::AIConfig::Disabled,
        };

        arct_ai::AIFactory::create(&ai_config)
            .map_err(|e| anyhow::anyhow!("Failed to create AI provider: {}", e))
    }

    /// Show ASCII art splash screen
    fn show_splash_screen() -> Result<()> {
        use crossterm::{
            cursor,
            style::{Color, Print, SetForegroundColor, ResetColor},
            terminal::{Clear, ClearType},
        };
        use std::io::Write;

        let mut stdout = io::stdout();

        // Clear screen
        execute!(stdout, Clear(ClearType::All), cursor::MoveTo(0, 0))?;

        // ASCII art logo
        let logo = r#"
 ▄▄▄       ██▀███   ▄████▄      ▄▄▄       ▄████▄   ▄▄▄      ▓█████▄ ▓█████  ███▄ ▄███▓▓██   ██▓
▒████▄    ▓██ ▒ ██▒▒██▀ ▀█     ▒████▄    ▒██▀ ▀█  ▒████▄    ▒██▀ ██▌▓█   ▀ ▓██▒▀█▀ ██▒ ▒██  ██▒
▒██  ▀█▄  ▓██ ░▄█ ▒▒▓█    ▄    ▒██  ▀█▄  ▒▓█    ▄ ▒██  ▀█▄  ░██   █▌▒███   ▓██    ▓██░  ▒██ ██░
░██▄▄▄▄██ ▒██▀▀█▄  ▒▓▓▄ ▄██▒   ░██▄▄▄▄██ ▒▓▓▄ ▄██▒░██▄▄▄▄██ ░▓█▄   ▌▒▓█  ▄ ▒██    ▒██   ░ ▐██▓░
 ▓█   ▓██▒░██▓ ▒██▒▒ ▓███▀ ░    ▓█   ▓██▒▒ ▓███▀ ░ ▓█   ▓██▒░▒████▓ ░▒████▒▒██▒   ░██▒  ░ ██▒▓░
 ▒▒   ▓▒█░░ ▒▓ ░▒▓░░ ░▒ ▒  ░    ▒▒   ▓▒█░░ ░▒ ▒  ░ ▒▒   ▓▒█░ ▒▒▓  ▒ ░░ ▒░ ░░ ▒░   ░  ░   ██▒▒▒
  ▒   ▒▒ ░  ░▒ ░ ▒░  ░  ▒        ▒   ▒▒ ░  ░  ▒     ▒   ▒▒ ░ ░ ▒  ▒  ░ ░  ░░  ░      ░ ▓██ ░▒░
  ░   ▒     ░░   ░ ░             ░   ▒   ░          ░   ▒    ░ ░  ░    ░   ░      ░    ▒ ▒ ░░
      ░  ░   ░     ░ ░               ░  ░░ ░            ░  ░   ░       ░  ░       ░    ░ ░
                   ░                     ░                   ░                         ░ ░
"#;

        let tagline = "Λ° Learn Shell Commands Interactively with AI";
        let version = format!("v{}", env!("CARGO_PKG_VERSION"));

        // Get terminal size for centering
        let (width, height) = crossterm::terminal::size()?;
        let start_row = (height / 2).saturating_sub(7); // Center vertically

        // Calculate logo width (longest line is ~92 chars)
        let logo_width = 92;
        let logo_col = (width / 2).saturating_sub(logo_width / 2);

        // Print logo with orange color, centered
        for (i, line) in logo.lines().enumerate() {
            let row = start_row + i as u16;
            execute!(
                stdout,
                cursor::MoveTo(logo_col, row),
                SetForegroundColor(Color::Rgb { r: 255, g: 140, b: 0 }), // Arc Academy Orange
                Print(line),
                ResetColor
            )?;
        }

        // Print tagline centered below logo
        let tagline_row = start_row + 11;
        let tagline_col = (width / 2).saturating_sub((tagline.len() / 2) as u16);
        execute!(
            stdout,
            cursor::MoveTo(tagline_col, tagline_row),
            SetForegroundColor(Color::White),
            Print(tagline),
            ResetColor
        )?;

        // Print version
        let version_row = tagline_row + 1;
        let version_col = (width / 2).saturating_sub((version.len() / 2) as u16);
        execute!(
            stdout,
            cursor::MoveTo(version_col, version_row),
            SetForegroundColor(Color::DarkGrey),
            Print(version),
            ResetColor
        )?;

        stdout.flush()?;

        // Pause for 1.5 seconds
        std::thread::sleep(std::time::Duration::from_millis(1500));

        // Clear screen before entering TUI
        execute!(stdout, Clear(ClearType::All), cursor::MoveTo(0, 0))?;

        Ok(())
    }

    /// Run the application
    pub async fn run(&mut self) -> Result<()> {
        // Show splash screen
        Self::show_splash_screen()?;

        // Safe mode: start the shell in ~/ArcAcademy/home
        if self.config.shell.safe_mode {
            if let Err(e) = self.ensure_explore() {
                tracing::warn!("Could not open ~/ArcAcademy/home: {}", e);
            }
        }

        // Panic safety: restore the terminal (raw mode, alternate screen,
        // mouse capture) before the default panic handler prints, so a crash
        // never leaves the user's terminal in a broken state
        let default_hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            let _ = disable_raw_mode();
            let _ = execute!(
                io::stdout(),
                crossterm::event::DisableMouseCapture,
                LeaveAlternateScreen
            );
            default_hook(info);
        }));

        // Setup terminal (mouse capture: click to focus, wheel to scroll)
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(
            stdout,
            EnterAlternateScreen,
            crossterm::event::EnableMouseCapture
        )?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;

        // Start event handler
        self.event_handler.start().await;

        // Telemetry: session start (local-only, opt-in)
        self.telemetry_record(arct_telemetry::TelemetryEvent::SessionStarted {
            session_id: self.session_id.clone(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            os: std::env::consts::OS.to_string(),
            arch: std::env::consts::ARCH.to_string(),
        });

        // Main loop
        let result = self.main_loop(&mut terminal).await;

        // Fold the remaining session time into "Time Invested", then save
        self.user_stats.update_session_time();
        self.save_session();

        // Telemetry: session end
        self.telemetry_record(arct_telemetry::TelemetryEvent::SessionEnded {
            session_id: self.session_id.clone(),
            duration_ms: self.app_started.elapsed().as_millis() as u64,
        });

        // Restore terminal (mouse capture must be disabled on exit)
        disable_raw_mode()?;
        execute!(
            terminal.backend_mut(),
            crossterm::event::DisableMouseCapture,
            LeaveAlternateScreen
        )?;
        terminal.show_cursor()?;

        result
    }

    /// Main application loop
    async fn main_loop<B: Backend>(&mut self, terminal: &mut Terminal<B>) -> Result<()> {
        loop {
            // Draw UI
            terminal.draw(|f| ui::draw(f, self))?;

            // Handle events; a closed channel means the event source is gone,
            // so exit instead of spinning
            match self.event_handler.next().await {
                Some(event) => self.handle_event(event).await?,
                None => {
                    tracing::warn!("Event channel closed, exiting main loop");
                    break;
                }
            }

            // Check for quit
            if self.should_quit {
                break;
            }
        }

        Ok(())
    }

    /// Handle an event
    async fn handle_event(&mut self, event: Event) -> Result<()> {
        match event {
            Event::Key(key) => {
                // The welcome dashboard dismisses on the first keypress;
                // the key still does whatever it normally does
                self.show_dashboard = false;

                // If onboarding is active, handle onboarding events
                if self.onboarding.is_some() {
                    return self.handle_onboarding_event(key).await;
                }

                // If settings panel is open, handle settings events
                if self.settings_panel.is_some() {
                    return self.handle_settings_event(key).await;
                }

                // If the command palette is open, it consumes all keys
                if let Some(ref mut palette) = self.command_palette {
                    use crate::panels::command_palette::PaletteOutcome;
                    match palette.handle_key(key) {
                        PaletteOutcome::Pending => {}
                        PaletteOutcome::Close => {
                            self.command_palette = None;
                        }
                        PaletteOutcome::Execute(command) => {
                            self.command_palette = None;
                            self.execute_palette_command(command).await?;
                        }
                    }
                    return Ok(());
                }

                // If the achievements panel is open, ↑/↓ (or j/k) scroll it
                if let Some(ref mut panel) = self.achievements_panel {
                    match key.code {
                        KeyCode::Up | KeyCode::Char('k')
                            if key.modifiers == KeyModifiers::NONE =>
                        {
                            panel.scroll_up();
                            return Ok(());
                        }
                        KeyCode::Down | KeyCode::Char('j')
                            if key.modifiers == KeyModifiers::NONE =>
                        {
                            panel.scroll_down();
                            return Ok(());
                        }
                        _ => {}
                    }
                }

                // If lesson menu is open, handle menu events
                if let Some(ref mut menu) = self.lesson_menu {
                    match key.code {
                        KeyCode::Up | KeyCode::Char('k') => {
                            menu.select_previous();
                            return Ok(());
                        }
                        KeyCode::Down | KeyCode::Char('j') => {
                            menu.select_next();
                            return Ok(());
                        }
                        KeyCode::Char(c) if c.is_ascii_digit() => {
                            if let Some(digit) = c.to_digit(10) {
                                // Map 1-9 to lessons 1-9, and 0 to lesson 10
                                let lesson_num = if digit == 0 { 10 } else { digit as usize };
                                menu.select_by_number(lesson_num);
                            }
                            return Ok(());
                        }
                        KeyCode::Enter => {
                            self.activate_selected_lesson();
                            return Ok(());
                        }
                        KeyCode::Esc | KeyCode::Char('q') => {
                            self.lesson_menu = None;
                            return Ok(());
                        }
                        _ => {}
                    }
                }

                // If in AI mode and in Shell panel, handle AI input
                if self.ai_mode && self.active_panel == PanelId::Shell && !self.show_help {
                    match key.code {
                        KeyCode::Char(c) if key.modifiers == KeyModifiers::NONE || key.modifiers == KeyModifiers::SHIFT => {
                            self.ai_input_buffer.push(c);
                            return Ok(());
                        }
                        KeyCode::Backspace => {
                            self.ai_input_buffer.pop();
                            return Ok(());
                        }
                        KeyCode::Enter => {
                            if !self.ai_input_buffer.is_empty() {
                                let question = self.ai_input_buffer.clone();
                                self.ai_input_buffer.clear();
                                self.ask_ai(question).await?;
                            }
                            return Ok(());
                        }
                        _ => {}
                    }
                }

                // If in shell panel and not a special action, handle as text input or history
                if self.active_panel == PanelId::Shell && !self.show_help && !self.ai_mode {
                    match key.code {
                        KeyCode::Char(c) if key.modifiers == KeyModifiers::NONE || key.modifiers == KeyModifiers::SHIFT => {
                            self.command_buffer.push(c);
                            // Reset history position when typing
                            self.history_position = None;
                            // Clear completion suggestions when typing
                            self.completion_suggestions.clear();
                            return Ok(());
                        }
                        KeyCode::Backspace => {
                            self.command_buffer.pop();
                            // Reset history position when editing
                            self.history_position = None;
                            // Clear completion suggestions when editing
                            self.completion_suggestions.clear();
                            return Ok(());
                        }
                        KeyCode::Tab if key.modifiers == KeyModifiers::NONE => {
                            // Tab in shell panel triggers autocomplete only if there's input
                            // Otherwise, fall through to panel cycling
                            if !self.command_buffer.is_empty() {
                                self.handle_autocomplete()?;
                                return Ok(());
                            }
                            // Empty buffer - let Tab fall through to cycle panels
                        }
                        KeyCode::Up => {
                            // Navigate backward in history (older commands)
                            self.history_previous();
                            return Ok(());
                        }
                        KeyCode::Down => {
                            // Navigate forward in history (newer commands)
                            self.history_next();
                            return Ok(());
                        }
                        _ => {}
                    }
                }

                // Handle as action
                let action = key_to_action(key);
                self.handle_action(action).await?;
            }
            Event::Mouse(mouse) => {
                self.handle_mouse(mouse).await?;
            }
            Event::Resize(_, _) => {
                // Terminal will automatically redraw on next iteration
            }
            Event::Tick => {
                // Drive the notification popup's brief border animation
                if self.showing_notification.is_some() {
                    self.notification_ticks = self.notification_ticks.saturating_add(1);
                }

                // Session-time heartbeat: fold elapsed time into stats every
                // ~30s so the Progress panel's "Time Invested" accumulates
                // even if the app exits uncleanly
                if self.last_time_update.elapsed() >= std::time::Duration::from_secs(30) {
                    self.user_stats.update_session_time();
                    self.last_time_update = std::time::Instant::now();
                    self.save_session();
                }
            }
            Event::Quit => {
                self.should_quit = true;
            }
        }

        Ok(())
    }

    /// Handle a mouse event: click to focus panels / select rows, wheel to
    /// scroll. Purely additive — every mouse action has a keyboard twin.
    async fn handle_mouse(&mut self, mouse: crossterm::event::MouseEvent) -> Result<()> {
        use crossterm::event::{MouseButton, MouseEventKind};

        match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                let (col, row) = (mouse.column, mouse.row);

                // A click also dismisses transient surfaces, like a keypress
                if self.showing_notification.is_some() {
                    self.dismiss_notification();
                    return Ok(());
                }

                // Command palette: click selects, click-on-selected runs
                if let Some(ref mut palette) = self.command_palette {
                    use crate::panels::command_palette::PaletteOutcome;
                    if let PaletteOutcome::Execute(command) = palette.click_at(col, row) {
                        self.command_palette = None;
                        self.execute_palette_command(command).await?;
                    }
                    return Ok(());
                }

                // Lesson menu: click selects, click-on-selected starts
                if self.lesson_menu.is_some() {
                    let activate = self
                        .lesson_menu
                        .as_mut()
                        .map(|menu| menu.click_at(col, row))
                        .unwrap_or(false);
                    if activate {
                        self.activate_selected_lesson();
                    }
                    return Ok(());
                }

                // Other overlays capture the click (Esc closes them)
                if self.onboarding.is_some()
                    || self.show_help
                    || self.settings_panel.is_some()
                    || self.achievements_panel.is_some()
                    || self.progress_panel.is_some()
                    || self.challenges_panel.is_some()
                {
                    return Ok(());
                }

                // Plain click: dismiss the dashboard and focus the panel hit
                self.show_dashboard = false;
                if let Some(panel) = self.panel_rects.panel_at(col, row) {
                    self.active_panel = panel;
                    if panel == PanelId::Context {
                        self.record_feature_event("use_context");
                    }
                }
            }
            MouseEventKind::ScrollUp | MouseEventKind::ScrollDown => {
                let down = matches!(mouse.kind, MouseEventKind::ScrollDown);

                if let Some(ref mut palette) = self.command_palette {
                    palette.scroll_selection(down);
                } else if let Some(ref mut menu) = self.lesson_menu {
                    if down {
                        menu.select_next();
                    } else {
                        menu.select_previous();
                    }
                } else if let Some(ref mut panel) = self.achievements_panel {
                    if down {
                        panel.scroll_down();
                    } else {
                        panel.scroll_up();
                    }
                } else if down {
                    // Wheel over the main view scrolls the output panel
                    let total_lines = self.last_output.lines().count();
                    if self.output_scroll < total_lines.saturating_sub(1) {
                        self.output_scroll += 1;
                    }
                } else {
                    self.output_scroll = self.output_scroll.saturating_sub(1);
                }
            }
            _ => {}
        }

        Ok(())
    }

    /// Start (or resume) the lesson currently selected in the lesson menu.
    /// Locked lessons don't start — the menu shows what unlocks them instead.
    fn activate_selected_lesson(&mut self) {
        let Some(lesson) = self
            .lesson_menu
            .as_ref()
            .and_then(|menu| menu.get_selected_lesson())
        else {
            return;
        };

        // Locked lessons are selectable but not startable
        use crate::panels::lesson_menu::{lesson_state, LessonState};
        if lesson_state(&lesson, &self.completed_lessons) == LessonState::Locked {
            let missing = self
                .lesson_menu
                .as_ref()
                .map(|menu| {
                    menu.missing_prerequisite_titles(&lesson, &self.completed_lessons)
                        .join(", ")
                })
                .unwrap_or_default();
            if let Some(ref mut menu) = self.lesson_menu {
                menu.set_status(format!("Locked: complete {} first", missing));
            }
            return;
        }

        // Load selected lesson (resuming at a saved step if it was left mid-way)
        let lesson_id = lesson.id.clone();
        let resume_state = self.lesson_progress.get(&lesson_id).cloned();

        let mut loaded = false;
        if let Some(ref mut panel) = self.lesson_panel {
            panel.load_lesson(lesson);
            self.lesson_menu = None;
            loaded = true;

            match resume_state {
                Some(state) if state.current_step_index > 0 => {
                    let resumed_at =
                        panel.resume_at(state.current_step_index, state.completed_steps);
                    self.last_output = format!(
                        "{}Lesson loaded — resumed at step {}.\n\nUse Alt+R to restart from the beginning, Alt+Left to go back a step.\n",
                        icons::lesson().content,
                        resumed_at + 1
                    );
                }
                _ => {
                    self.last_output = format!(
                        "{}Lesson loaded! Follow the instructions in the lesson panel.\n",
                        icons::lesson().content
                    );
                }
            }
        }

        if loaded {
            // Materialize starter files: real playground dir (real mode)
            // or seeded sandbox (simulated)
            self.prepare_lesson_environment();
        }
    }

    /// Handle an action
    async fn handle_action(&mut self, action: Action) -> Result<()> {
        match action {
            Action::Quit => {
                if self.show_help {
                    self.show_help = false;
                } else {
                    self.should_quit = true;
                }
            }
            Action::NextPanel => {
                self.active_panel = self.active_panel.next();
                // Don't reset scroll - users may want to keep reading output
                if self.active_panel == PanelId::Context {
                    self.record_feature_event("use_context");
                }
            }
            Action::PreviousPanel => {
                self.active_panel = self.active_panel.previous();
                // Don't reset scroll - users may want to keep reading output
                if self.active_panel == PanelId::Context {
                    self.record_feature_event("use_context");
                }
            }
            Action::ScrollUp => {
                // Only scroll when Output panel is focused
                if self.active_panel == PanelId::Output {
                    self.output_scroll = self.output_scroll.saturating_sub(1);
                }
            }
            Action::ScrollDown => {
                // Only scroll when Output panel is focused
                if self.active_panel == PanelId::Output {
                    let total_lines = self.last_output.lines().count();
                    if self.output_scroll < total_lines.saturating_sub(1) {
                        self.output_scroll += 1;
                    }
                }
            }
            Action::PageUp => {
                // Only scroll when Output panel is focused
                if self.active_panel == PanelId::Output {
                    self.output_scroll = self.output_scroll.saturating_sub(10);
                }
            }
            Action::PageDown => {
                // Only scroll when Output panel is focused
                if self.active_panel == PanelId::Output {
                    let total_lines = self.last_output.lines().count();
                    self.output_scroll = (self.output_scroll + 10).min(total_lines.saturating_sub(1));
                }
            }
            Action::ScrollOutputUp => {
                // Always scroll output regardless of which panel is focused
                self.output_scroll = self.output_scroll.saturating_sub(1);
            }
            Action::ScrollOutputDown => {
                // Always scroll output regardless of which panel is focused
                let total_lines = self.last_output.lines().count();
                if self.output_scroll < total_lines.saturating_sub(1) {
                    self.output_scroll += 1;
                }
            }
            Action::Help => {
                self.show_help = !self.show_help;
                if self.show_help {
                    self.mark_panel_visited("help");
                }
            }
            Action::ToggleTheme => {
                let from = self.theme.name.clone();
                self.theme = self.theme.cycle_next();
                let to = self.theme.name.clone();
                self.record_feature_event("change_theme");
                self.telemetry_record(arct_telemetry::TelemetryEvent::ThemeChanged { from, to });
            }
            Action::ToggleAI => {
                self.toggle_ai_mode();
            }
            Action::ToggleSettings => {
                if self.settings_panel.is_some() {
                    self.settings_panel = None;
                } else {
                    self.settings_panel = Some(crate::panels::settings::SettingsPanel::new());
                    self.mark_panel_visited("settings");
                }
            }
            Action::ToggleLesson => {
                self.toggle_lesson_mode();
            }
            Action::ShowLessonMenu => {
                if self.lesson_mode {
                    // Toggle menu on/off
                    if self.lesson_menu.is_some() {
                        self.lesson_menu = None;
                    } else {
                        self.lesson_menu = Some(
                            crate::panels::lesson_menu::LessonMenuPanel::for_learner(
                                self.lesson_library.clone(),
                                self.preferred_track(),
                                &self.completed_lessons,
                            ),
                        );
                        self.mark_panel_visited("lesson_menu");
                    }
                } else {
                    // Outside lesson mode, 'm' enters lesson mode, which
                    // opens the lesson map automatically
                    self.toggle_lesson_mode();
                }
            }
            Action::LessonPreviousStep => {
                self.lesson_previous_step();
            }
            Action::LessonRestart => {
                self.lesson_restart();
            }
            Action::TogglePracticeMode => {
                self.toggle_practice_mode();
            }
            Action::ResetPlayground => {
                self.reset_playground();
            }
            Action::CommandPalette => {
                if self.command_palette.is_some() {
                    self.command_palette = None;
                } else {
                    self.command_palette =
                        Some(crate::panels::command_palette::CommandPalette::new());
                    self.telemetry_record(arct_telemetry::TelemetryEvent::FeatureUsed {
                        feature: "command_palette".to_string(),
                        context: None,
                    });
                }
            }
            Action::Escape => {
                if self.showing_notification.is_some() {
                    self.dismiss_notification();
                } else if self.show_help {
                    self.show_help = false;
                } else if self.achievements_panel.is_some() {
                    self.achievements_panel = None;
                } else if self.progress_panel.is_some() {
                    self.progress_panel = None;
                } else if self.challenges_panel.is_some() {
                    self.challenges_panel = None;
                } else if self.settings_panel.is_some() {
                    self.settings_panel = None;
                } else if self.lesson_menu.is_some() {
                    self.lesson_menu = None;
                } else if self.ai_mode {
                    self.ai_mode = false;
                }
            }
            Action::Enter => {
                // Enter closes a popup; if a command is waiting underneath,
                // it runs too, so a fast typist never loses a command
                let had_notification = self.showing_notification.is_some();
                if had_notification {
                    self.dismiss_notification();
                }
                if !self.ai_mode && (!had_notification || !self.command_buffer.is_empty()) {
                    self.execute_command().await?;
                }
            }
            Action::ShowAchievements => {
                self.toggle_achievements_panel();
            }
            Action::ShowProgress => {
                self.toggle_progress_panel();
            }
            Action::ShowChallenges => {
                self.toggle_challenges_panel();
            }
            Action::DismissNotification => {
                self.dismiss_notification();
            }
            _ => {}
        }

        Ok(())
    }

    /// Helper: Check if a flag exists in the command
    fn has_flag(cmd: &arct_core::Command, flag: &str) -> bool {
        cmd.flags.iter().any(|f| {
            f.raw == flag ||
            f.short == Some(flag.chars().nth(1).unwrap_or(' ')) ||
            f.long.as_ref().map(|l| l == flag.trim_start_matches("--")).unwrap_or(false)
        })
    }

    /// Whether command output reports a failure: an error line, a guard
    /// refusal, a timeout, or a non-zero exit from the real shell
    pub(crate) fn output_reports_failure(output: &str) -> bool {
        let error = icons::error().content;
        let warning = icons::warning().content;
        output.lines().any(|line| {
            line.starts_with(error.as_ref())
                || line.starts_with(warning.as_ref())
                || line.contains("timed out")
        })
    }

    /// `ls -l` style permissions (drwxr-xr-x) for a sandbox path, from the
    /// file's real mode bits, so chmod changes show up
    fn mode_string(real: &std::path::Path) -> String {
        let Ok(meta) = std::fs::symlink_metadata(real) else {
            return "----------".to_string();
        };
        #[cfg(unix)]
        let bits = {
            use std::os::unix::fs::PermissionsExt;
            meta.permissions().mode()
        };
        #[cfg(not(unix))]
        let bits: u32 = if meta.is_dir() { 0o755 } else { 0o644 };
        let mut out = String::from(if meta.is_dir() { "d" } else { "-" });
        for shift in [6, 3, 0] {
            let part = (bits >> shift) & 0o7;
            out.push(if part & 4 != 0 { 'r' } else { '-' });
            out.push(if part & 2 != 0 { 'w' } else { '-' });
            out.push(if part & 1 != 0 { 'x' } else { '-' });
        }
        out
    }

    /// Apply a chmod mode to a sandbox file for the modes lessons rely on:
    /// octal numbers and adding/removing execute (`+x`, `u+x`, `a-x`...).
    /// Returns false for modes it leaves simulated.
    #[cfg(unix)]
    fn apply_sandbox_mode(real: &std::path::Path, mode: &str) -> bool {
        use std::os::unix::fs::PermissionsExt;
        let Ok(meta) = std::fs::metadata(real) else {
            return false;
        };
        let current = meta.permissions().mode() & 0o7777;
        let new_mode = if mode.chars().all(|c| c.is_digit(8)) && (3..=4).contains(&mode.len()) {
            u32::from_str_radix(mode, 8).ok()
        } else if let Some((who, perm)) = mode.split_once(['+', '-']) {
            if perm != "x" || !who.chars().all(|c| "ugoa".contains(c)) {
                None
            } else {
                let mut bits = 0;
                if who.is_empty() || who.contains('a') || who.contains('u') {
                    bits |= 0o100;
                }
                if who.is_empty() || who.contains('a') || who.contains('g') {
                    bits |= 0o010;
                }
                if who.is_empty() || who.contains('a') || who.contains('o') {
                    bits |= 0o001;
                }
                Some(if mode.contains('+') { current | bits } else { current & !bits })
            }
        } else {
            None
        };
        match new_mode {
            // Never lock the learner out of their own sandbox file
            Some(m) => std::fs::set_permissions(real, std::fs::Permissions::from_mode(m | 0o600)).is_ok(),
            None => false,
        }
    }

    #[cfg(not(unix))]
    fn apply_sandbox_mode(_real: &std::path::Path, _mode: &str) -> bool {
        false
    }

    /// Execute command against virtual filesystem and return output
    pub(crate) fn execute_virtual_fs_command(&mut self, cmd: &arct_core::Command) -> Option<String> {
        if !self.lesson_mode {
            return None;
        }

        // Pipes, redirection, variables and scripts are handled before
        // this point by the sandbox shell (sandbox.rs)
        let vfs = self.virtual_fs.as_mut()?;
        let program = cmd.program.as_str();

        // Operands are the non-flag arguments: `rm -rf notes` removes
        // "notes", never a file called "-rf"
        let operands: Vec<String> = cmd
            .args
            .iter()
            .filter(|a| !(a.starts_with('-') && a.len() > 1))
            .cloned()
            .collect();

        match program {
            "pwd" => {
                let current = vfs.get_current_dir().display().to_string();
                Some(format!("{}\n", current))
            }

            "ls" => {
                let show_hidden = Self::has_flag(cmd, "-a") || Self::has_flag(cmd, "--all");
                let long = Self::has_flag(cmd, "-l");
                let human = Self::has_flag(cmd, "-h");
                let size_text = |bytes: u64| -> String {
                    if human && bytes >= 1024 {
                        format!("{:.1}K", bytes as f64 / 1024.0)
                    } else {
                        bytes.to_string()
                    }
                };

                let targets: Vec<Option<&str>> = if operands.is_empty() {
                    vec![None]
                } else {
                    operands.iter().map(|o| Some(o.as_str())).collect()
                };
                let mut output = String::new();
                for target in &targets {
                    // `ls file` prints the file itself
                    if let Some(name) = target {
                        match vfs.resolve_path(name) {
                            Ok(real) if real.is_file() => {
                                let size = std::fs::metadata(&real).map(|m| m.len()).unwrap_or(0);
                                if long {
                                    output.push_str(&format!("{}  {:>6}  {}\n", Self::mode_string(&real), size_text(size), name));
                                } else {
                                    output.push_str(&format!("{}\n", name));
                                }
                                continue;
                            }
                            Ok(real) if !real.exists() => {
                                output.push_str(&format!(
                                    "{}ls: {}: No such file or directory\n",
                                    icons::error().content,
                                    name
                                ));
                                continue;
                            }
                            Err(e) => {
                                output.push_str(&format!("{}ls: {}\n", icons::error().content, e));
                                continue;
                            }
                            _ => {}
                        }
                    }
                    if targets.len() > 1 {
                        output.push_str(&format!("{}:\n", target.unwrap_or(".")));
                    }
                    match vfs.list_directory(*target) {
                        Ok(mut entries) => {
                            if !show_hidden {
                                entries.retain(|e| !e.name.starts_with('.'));
                            }
                            // -a also lists . (this folder) and .. (the one above); -A doesn't
                            if show_hidden && !Self::has_flag(cmd, "-A") {
                                for dot in ["..", "."] {
                                    entries.insert(0, arct_core::DirEntry { name: dot.to_string(), is_dir: true, size: 0 });
                                }
                            }
                            if entries.is_empty() {
                                output.push_str("(empty folder)\n");
                            }
                            for entry in entries {
                                let shown = if entry.is_dir {
                                    format!("{}/", entry.name)
                                } else {
                                    entry.name.clone()
                                };
                                if long {
                                    let real = vfs
                                        .resolve_path(&match target {
                                            Some(dir) => format!("{}/{}", dir.trim_end_matches('/'), entry.name),
                                            None => entry.name.clone(),
                                        })
                                        .unwrap_or_default();
                                    let mode = Self::mode_string(&real);
                                    output.push_str(&format!("{}  {:>6}  {}\n", mode, size_text(entry.size), shown));
                                } else {
                                    output.push_str(&format!("{}\n", shown));
                                }
                            }
                        }
                        Err(e) => output.push_str(&format!("{}ls: {}\n", icons::error().content, e)),
                    }
                }
                Some(output)
            }

            "cd" => {
                let target = cmd.args.first().map(|s| s.as_str()).unwrap_or("~");
                match vfs.change_directory(target) {
                    Ok(new_path) => {
                        Some(format!("{}Changed to: {}\n", icons::folder().content, new_path))
                    }
                    Err(e) => Some(format!("{}cd: {}\n", icons::error().content, e)),
                }
            }

            "cat" => {
                if operands.is_empty() {
                    return Some(format!("{}cat: missing file argument\n", icons::error().content));
                }

                let mut output = String::new();
                for file in &operands {
                    match vfs.read_file(file) {
                        Ok(content) => output.push_str(&content),
                        Err(e) => output.push_str(&format!("{}cat: {}\n", icons::error().content, e)),
                    }
                }
                Some(output)
            }

            "mkdir" => {
                if operands.is_empty() {
                    return Some(format!("{}mkdir: missing directory name\n", icons::error().content));
                }

                let parents = Self::has_flag(cmd, "-p") || Self::has_flag(cmd, "--parents");
                let mut output = String::new();

                for dir in &operands {
                    match vfs.create_directory(dir, parents) {
                        Ok(_) => output.push_str(&format!("{}Created directory: {}\n", icons::folder().content, dir)),
                        Err(e) => output.push_str(&format!("{}mkdir: {}\n", icons::error().content, e)),
                    }
                }
                Some(output)
            }

            "touch" => {
                if operands.is_empty() {
                    return Some(format!("{}touch: missing file argument\n", icons::error().content));
                }

                let mut output = String::new();
                for file in &operands {
                    match vfs.touch_file(file) {
                        Ok(_) => output.push_str(&format!("{}Created/updated file: {}\n", icons::file().content, file)),
                        Err(e) => output.push_str(&format!("{}touch: {}\n", icons::error().content, e)),
                    }
                }
                Some(output)
            }

            "rm" => {
                if operands.is_empty() {
                    return Some(format!("{}rm: missing file argument\n", icons::error().content));
                }

                let recursive = Self::has_flag(cmd, "-r") || Self::has_flag(cmd, "-R") || Self::has_flag(cmd, "--recursive");
                let force = Self::has_flag(cmd, "-f") || Self::has_flag(cmd, "--force");
                let mut output = String::new();

                for item in &operands {
                    match vfs.remove(item, recursive, force) {
                        Ok(_) => output.push_str(&format!("{}Removed: {}\n", icons::success().content, item)),
                        Err(e) => output.push_str(&format!("{}rm: {}\n", icons::error().content, e)),
                    }
                }
                Some(output)
            }

            "mv" => {
                if operands.len() < 2 {
                    return Some(format!("{}mv: missing source or destination\n", icons::error().content));
                }

                let (sources, dest) = operands.split_at(operands.len() - 1);
                let dest = &dest[0];
                // Several files go into a folder, never over each other
                if sources.len() > 1 && !vfs.resolve_path(dest).is_ok_and(|p| p.is_dir()) {
                    return Some(format!("{}mv: '{}' is not a folder (moving several files needs a folder at the end)\n", icons::error().content, dest));
                }
                let mut output = String::new();
                for source in sources {
                    match vfs.move_item(source, dest) {
                        Ok(_) => output.push_str(&format!("{}Moved {} to {}\n", icons::success().content, source, dest)),
                        Err(e) => output.push_str(&format!("{}mv: {}\n", icons::error().content, e)),
                    }
                }
                Some(output)
            }

            "cp" => {
                if operands.len() < 2 {
                    return Some(format!("{}cp: missing source or destination\n", icons::error().content));
                }

                let recursive = Self::has_flag(cmd, "-r") || Self::has_flag(cmd, "-R") || Self::has_flag(cmd, "--recursive");
                let (sources, dest) = operands.split_at(operands.len() - 1);
                let dest = &dest[0];
                // Several files go into a folder, never over each other
                if sources.len() > 1 && !vfs.resolve_path(dest).is_ok_and(|p| p.is_dir()) {
                    return Some(format!("{}cp: '{}' is not a folder (copying several files needs a folder at the end)\n", icons::error().content, dest));
                }
                let mut output = String::new();
                for source in sources {
                    match vfs.copy(source, dest, recursive) {
                        Ok(_) => output.push_str(&format!("{}Copied {} to {}\n", icons::success().content, source, dest)),
                        Err(e) => output.push_str(&format!("{}cp: {}\n", icons::error().content, e)),
                    }
                }
                Some(output)
            }

            "grep" => {
                // Same grep as after a pipe (-i -n -v -c -E), applied per file
                let flags: Vec<String> =
                    cmd.args.iter().filter(|a| a.starts_with('-') && a.len() > 1).cloned().collect();
                let positional: Vec<&String> =
                    cmd.args.iter().filter(|a| !(a.starts_with('-') && a.len() > 1)).collect();
                let Some(pattern) = positional.first() else {
                    return Some(format!("{}grep: usage: grep [-i] [-n] PATTERN FILE...\n", icons::error().content));
                };
                let files = &positional[1..];
                if files.is_empty() {
                    return Some(format!("{}(lesson sandbox) grep needs a file here — try: grep {} <file>\n", icons::hint().content, pattern));
                }
                let mut grep_args = flags.clone();
                grep_args.push((*pattern).clone());

                let mut output = String::new();
                let multiple = files.len() > 1;
                for file in files {
                    let content = match vfs.read_file(file) {
                        Ok(content) => content,
                        Err(e) => {
                            output.push_str(&format!("{}grep: {}\n", icons::error().content, e));
                            continue;
                        }
                    };
                    match arct_core::text_filter::apply("grep", &grep_args, &content) {
                        Ok(found) => {
                            for line in found.lines() {
                                if multiple {
                                    output.push_str(&format!("{}:", file));
                                }
                                output.push_str(line);
                                output.push('\n');
                            }
                        }
                        Err(message) => output.push_str(&format!("{}{}\n", icons::error().content, message)),
                    }
                }
                if output.is_empty() {
                    output = format!("(no lines matched '{}')\n", pattern);
                }
                Some(output)
            }

            "head" | "tail" => {
                let (count, files) = Self::parse_line_count_args(&cmd.args);
                if files.is_empty() {
                    return Some(format!("{}(lesson sandbox) {} needs a file here — try: {} -n {} <file>\n", icons::hint().content, program, program, count));
                }

                let mut output = String::new();
                let multiple = files.len() > 1;
                for file in &files {
                    let result = if program == "head" {
                        vfs.head_file(file, count)
                    } else if arct_core::text_filter::tail_from_start(&cmd.args) {
                        // tail -n +N: from line N on
                        let flags: Vec<String> =
                            cmd.args.iter().filter(|a| !files.contains(a)).cloned().collect();
                        vfs.read_file(file).and_then(|text| {
                            arct_core::text_filter::apply("tail", &flags, &text)
                                .map_err(|e| anyhow::anyhow!(e))
                        })
                    } else {
                        vfs.tail_file(file, count)
                    };
                    match result {
                        Ok(content) => {
                            if multiple {
                                output.push_str(&format!("==> {} <==\n", file));
                            }
                            output.push_str(&content);
                        }
                        Err(e) => output.push_str(&format!("{}{}: {}\n", icons::error().content, program, e)),
                    }
                }
                Some(output)
            }

            "wc" => {
                let want_lines = Self::has_flag(cmd, "-l") || Self::has_flag(cmd, "--lines");
                let want_words = Self::has_flag(cmd, "-w") || Self::has_flag(cmd, "--words");
                let want_bytes = Self::has_flag(cmd, "-c") || Self::has_flag(cmd, "--bytes");
                let want_all = !want_lines && !want_words && !want_bytes;

                let files: Vec<&String> = cmd.args.iter().filter(|a| !a.starts_with('-')).collect();
                if files.is_empty() {
                    return Some(format!("{}(lesson sandbox) wc needs a file here — try: wc -l <file>\n", icons::hint().content));
                }

                let mut output = String::new();
                for file in &files {
                    match vfs.wc_file(file) {
                        Ok((lines, words, bytes)) => {
                            let mut parts = Vec::new();
                            if want_all || want_lines {
                                parts.push(format!("{:8}", lines));
                            }
                            if want_all || want_words {
                                parts.push(format!("{:8}", words));
                            }
                            if want_all || want_bytes {
                                parts.push(format!("{:8}", bytes));
                            }
                            output.push_str(&format!("{} {}\n", parts.join(""), file));
                        }
                        Err(e) => output.push_str(&format!("{}wc: {}\n", icons::error().content, e)),
                    }
                }
                Some(output)
            }

            "echo" => {
                // Redirection (> and >>) is applied by the sandbox shell
                let no_newline = cmd.args.first().is_some_and(|a| a == "-n");
                let words = if no_newline { &cmd.args[1..] } else { &cmd.args[..] };
                let mut text = words.join(" ");
                if !no_newline {
                    text.push('\n');
                }
                Some(text)
            }

            "chmod" => {
                let positional: Vec<&String> = cmd.args.iter().filter(|a| !a.starts_with('-') || a.chars().skip(1).all(|c| "rwxugoa+-=,0123456789".contains(c))).collect();
                // chmod modes like "-w" also start with '-'; treat the first
                // positional as the mode and the rest as files
                let Some(mode) = positional.first() else {
                    return Some(format!("{}chmod: missing operand\n  Usage: chmod MODE FILE...\n", icons::error().content));
                };
                let files = &positional[1..];
                if files.is_empty() {
                    return Some(format!("{}chmod: missing file operand after '{}'\n", icons::error().content, mode));
                }

                let mut output = String::new();
                for file in files {
                    match vfs.resolve_path(file) {
                        Ok(real) if real.exists() => {
                            if Self::apply_sandbox_mode(&real, mode) {
                                output.push_str(&format!(
                                    "{}Mode of '{}' set to '{}'\n",
                                    icons::success().content, file, mode
                                ));
                            } else {
                                output.push_str(&format!(
                                    "{}Mode of '{}' set to '{}' (simulated: the sandbox only applies +x, -x and number modes)\n",
                                    icons::success().content, file, mode
                                ));
                            }
                        }
                        Ok(_) => output.push_str(&format!("{}chmod: cannot access '{}': No such file or directory\n", icons::error().content, file)),
                        Err(e) => output.push_str(&format!("{}chmod: {}\n", icons::error().content, e)),
                    }
                }
                Some(output)
            }

            // Common commands lessons mention but that can't be meaningfully
            // simulated against a sandbox filesystem: return an instructional
            // message instead of silent nothing (the lesson step still
            // validates the command syntax)
            "ps" | "top" | "htop" | "kill" | "killall" | "git" | "apt" | "apt-get" | "yum"
            | "dnf" | "brew" | "ping" | "curl" | "wget" | "ssh" | "scp" | "sudo" | "man"
            | "df" | "free" | "uname" | "whoami" | "zip" | "unzip"
            | "nano" | "vim" | "vi" | "less" | "more" | "netstat" | "ifconfig" | "ip" | "ss"
            | "lsof" | "systemctl" | "journalctl"
            | "awk" | "ln" | "python" | "python3" | "pip"
            | "npm" | "node" => {
                Some(format!(
                    "(lesson sandbox) '{}' isn't simulated here — this step checks your command syntax.\n",
                    program
                ))
            }

            // Commands that don't manipulate the filesystem
            "sort" | "uniq" | "cut" => {
                let files = arct_core::text_filter::operands(program, &cmd.args);
                if files.is_empty() {
                    return Some(format!(
                        "{}{} needs a file here, or use it after a pipe: cat file.txt | {}\n",
                        icons::error().content,
                        program,
                        program
                    ));
                }
                let mut input = String::new();
                for file in &files {
                    match vfs.read_file(file) {
                        Ok(content) => input.push_str(&content),
                        Err(e) => {
                            return Some(format!("{}{}: {}\n", icons::error().content, program, e))
                        }
                    }
                }
                let flags: Vec<String> =
                    cmd.args.iter().filter(|a| !files.contains(a)).cloned().collect();
                Some(match arct_core::text_filter::apply(program, &flags, &input) {
                    Ok(out) => out,
                    Err(message) => format!("{}{}\n", icons::error().content, message),
                })
            }

            // A real command the sandbox doesn't run: say so, and let the
            // step check what was typed
            other if cmd.category != arct_core::CommandCategory::Unknown => Some(format!(
                "{}{} doesn't run in the simulated sandbox, so this only checks what you typed. \
                 To see real output, switch lesson practice to real files in settings (Ctrl+S).\n",
                icons::info().content,
                other
            )),

            // Not a command at all (a typo, or a script that isn't on PATH)
            other => Some(format!("{}{}: command not found\n", icons::error().content, other)),
        }
    }

    /// Parse `head`/`tail` style arguments: `-n N`, `-nN`, `-N`, plus file
    /// operands. Returns (line count, files); the count defaults to 10.
    fn parse_line_count_args(args: &[String]) -> (usize, Vec<String>) {
        let mut count = 10usize;
        let mut files = Vec::new();

        let mut iter = args.iter().peekable();
        while let Some(arg) = iter.next() {
            if arg == "-n" || arg == "--lines" {
                if let Some(n) = iter.peek().and_then(|v| v.parse::<usize>().ok()) {
                    count = n;
                    iter.next();
                }
            } else if let Some(n) = arg
                .strip_prefix("-n")
                .or_else(|| arg.strip_prefix("--lines="))
                .and_then(|v| v.parse::<usize>().ok())
            {
                count = n;
            } else if let Some(n) = arg.strip_prefix('-').and_then(|v| v.parse::<usize>().ok()) {
                // Classic `head -5` form
                count = n;
            } else if !arg.starts_with('-') {
                files.push(arg.clone());
            }
        }

        (count, files)
    }

    /// Execute the current command
    async fn execute_command(&mut self) -> Result<()> {
        // In lesson mode, allow empty commands (for Information steps that just need Enter)
        if self.command_buffer.is_empty() && !self.lesson_mode {
            return Ok(());
        }

        let mut command_str = self.command_buffer.clone();

        // If in lesson mode with empty command, skip parsing and go to validation
        if self.lesson_mode && command_str.is_empty() {
            // Extract lesson completion info outside the borrow scope
            let mut lesson_completed_info: Option<(String, arct_core::Difficulty, u32, u64, usize, bool)> =
                None;
            let mut step_advanced = false;

            if let Some(ref mut lesson_panel) = self.lesson_panel {
                let validation = lesson_panel.validate_current_step(&command_str);

                if validation.is_success() {
                    // Success! Move to next step
                    self.last_output = format!("{}{}\n\nMoving to next step...\n",
                        icons::success().content,
                        match &validation {
                            arct_core::ValidationResult::Success { message } => message,
                            _ => "Success!",
                        }
                    );

                    if !lesson_panel.next_step() {
                        // Lesson complete! Extract info for later processing
                        if let Some(lesson) = lesson_panel.current_lesson.as_ref() {
                            lesson_completed_info = Some((
                                lesson.id.clone(),
                                lesson.difficulty,
                                lesson.estimated_minutes,
                                lesson_panel.elapsed_seconds(),
                                lesson_panel.wrong_answers(),
                                lesson_panel.mastered_run(),
                            ));
                        }
                        self.last_output.push_str(&format!("\n{}Congratulations! You've completed this lesson!\n\nPress Ctrl+L to exit lesson mode or 'm' to select another lesson.\n", icons::celebration().content));
                    } else {
                        step_advanced = true;
                    }
                } else {
                    // Information steps should always succeed with Enter
                    self.last_output = "Press Enter to continue...\n".to_string();
                }

                self.command_buffer.clear();
                self.add_to_history(command_str.clone());
            }

            // Persist step-level position for resume
            if step_advanced {
                self.save_lesson_progress();
            }

            // Process lesson completion outside the borrow scope
            if let Some((lesson_id, difficulty, estimated, elapsed, wrong, mastered)) =
                lesson_completed_info
            {
                self.record_lesson_completion(lesson_id, difficulty, estimated, elapsed, wrong, mastered);
            }

            return Ok(());
        }

        // Parse command
        let cmd = self.analyzer.parse(&command_str)?;

        // If in lesson mode, execute (real playground or virtual FS) and validate
        if self.lesson_mode {
            // Execute the command in the active practice environment:
            // real shell inside ~/ArcAcademy/playground (guard-checked) when
            // practice_mode = "real", simulated virtual filesystem otherwise
            let mut guard_refused = false;
            // A quiz answer ("1") is not a command: don't run it
            let answering = self.lesson_panel.as_ref().is_some_and(|p| p.expects_answer());
            let vfs_output = if answering {
                None
            } else if self.practice_mode_is_real() {
                Some(self.execute_playground_command(&cmd, &command_str).await)
            } else if let arct_core::GuardVerdict::Refuse { reason } =
                arct_core::PlaygroundGuard::check_catastrophic(&command_str)
            {
                guard_refused = true;
                Some(format!(
                    "{}Blocked. Nothing ran.\n\n{}\n",
                    icons::warning().content,
                    reason
                ))
            } else {
                self.run_sandbox_line(&command_str)
            };
            if guard_refused {
                self.record_feature_event("guard_refusal");
            }
            let command_failed = vfs_output
                .as_deref()
                .map(|out| Self::output_reports_failure(out))
                .unwrap_or(false);

            // Extract lesson completion info outside the borrow scope
            let mut lesson_completed_info: Option<(String, arct_core::Difficulty, u32, u64, usize, bool)> =
                None;
            let mut step_advanced = false;

            let practice_real = self.practice_mode_is_real();
            if let Some(ref mut lesson_panel) = self.lesson_panel {
                // Task steps check the practice environment itself
                let world: Option<&dyn arct_core::PracticeWorld> = if practice_real {
                    self.playground
                        .as_ref()
                        .map(|p| p as &dyn arct_core::PracticeWorld)
                } else {
                    self.virtual_fs
                        .as_ref()
                        .map(|v| v as &dyn arct_core::PracticeWorld)
                };
                let validation = lesson_panel.validate_step_with(
                    &command_str,
                    vfs_output.as_deref().unwrap_or(""),
                    command_failed,
                    world,
                );

                // Build output: virtual FS output FIRST, then validation feedback
                let mut output = String::new();

                // Show the command output from virtual filesystem
                if let Some(vfs_out) = vfs_output {
                    output.push_str(&vfs_out);
                    output.push('\n');
                }

                // Then show validation feedback
                if validation.is_success() {
                    // Success! Move to next step
                    output.push_str(&format!("{}{}\n\n",
                        icons::success().content,
                        match &validation {
                            arct_core::ValidationResult::Success { message } => message,
                            _ => "Correct!",
                        }
                    ));

                    if !lesson_panel.next_step() {
                        // Lesson complete! Extract info for later processing
                        if let Some(lesson) = lesson_panel.current_lesson.as_ref() {
                            lesson_completed_info = Some((
                                lesson.id.clone(),
                                lesson.difficulty,
                                lesson.estimated_minutes,
                                lesson_panel.elapsed_seconds(),
                                lesson_panel.wrong_answers(),
                                lesson_panel.mastered_run(),
                            ));
                        }
                        output.push_str(&format!("{}Congratulations! You've completed this lesson!\n\nPress Ctrl+L to exit lesson mode or 'm' to select another lesson.\n", icons::celebration().content));
                    } else {
                        step_advanced = true;
                        output.push_str("Moving to next step...\n");
                    }
                } else {
                    // Show validation failure
                    output.push_str(&match validation {
                        arct_core::ValidationResult::Failure { message, hint } => {
                            let mut fail_output = format!("{}{}\n", icons::error().content, message);
                            if let Some(h) = hint {
                                fail_output.push_str(&format!("\n{}Hint: {}\n", icons::hint().content, h));
                            }
                            fail_output.push_str("\nTry again!\n");
                            fail_output
                        }
                        arct_core::ValidationResult::Partial { message, progress } => {
                            format!("{}{} ({:.0}% correct)\n\nKeep trying!\n", icons::warning().content, message, progress)
                        }
                        _ => "Try again!\n".to_string(),
                    });
                }

                self.last_output = output;
                self.command_buffer.clear();
                self.add_to_history(command_str.clone());
            }

            // Persist step-level position for resume
            if step_advanced {
                self.save_lesson_progress();
            }

            // Lesson-mode commands count towards challenges too
            self.check_challenges_for_command(&command_str);

            // Telemetry: command name only (no arguments — privacy)
            let command_name = command_str
                .split_whitespace()
                .next()
                .unwrap_or("")
                .to_string();
            if !command_name.is_empty() {
                self.telemetry_record(arct_telemetry::TelemetryEvent::CommandExecuted {
                    command: command_name,
                    success: true,
                    duration_ms: 0,
                });
            }

            // Process lesson completion outside the borrow scope
            if let Some((lesson_id, difficulty, estimated, elapsed, wrong, mastered)) =
                lesson_completed_info
            {
                self.record_lesson_completion(lesson_id, difficulty, estimated, elapsed, wrong, mastered);
            }

            return Ok(());
        }

        // Generate explanation
        let explanation = self.educator.explain(&cmd)?;
        self.last_explanation = Some(explanation);

        // Check if this is a shell builtin command
        match cmd.program.as_str() {
            "cd" => {
                // Add to history before clearing buffer
                self.add_to_history(command_str.clone());
                if self.config.shell.safe_mode {
                    self.handle_safe_cd(&cmd);
                } else {
                    self.handle_cd_command(&cmd)?;
                }
                self.command_buffer.clear();
                return Ok(());
            }
            "history" => {
                // Add to history before clearing buffer
                self.add_to_history(command_str.clone());
                self.handle_history_command(&cmd)?;
                self.command_buffer.clear();
                return Ok(());
            }
            "export" => {
                // Add to history before clearing buffer
                self.add_to_history(command_str.clone());
                self.handle_export_command(&cmd)?;
                self.command_buffer.clear();
                return Ok(());
            }
            "alias" => {
                // Add to history before clearing buffer
                self.add_to_history(command_str.clone());
                self.handle_alias_command(&cmd)?;
                self.command_buffer.clear();
                return Ok(());
            }
            _ => {
                // Check if the command is an alias and expand it
                if let Some(aliased_command) = self.aliases.get(cmd.program.as_str()) {
                    // Replace the alias with the full command
                    let args_str = if !cmd.args.is_empty() {
                        format!(" {}", cmd.args.join(" "))
                    } else {
                        String::new()
                    };
                    command_str = format!("{}{}", aliased_command, args_str);
                }
            }
        }

        // Safety guard, checked after alias expansion so an alias can't hide
        // a destructive command. Safe mode keeps the free shell inside
        // ~/ArcAcademy/home; with it off, catastrophic patterns are still
        // refused everywhere.
        let (verdict, run_cwd) = if self.config.shell.safe_mode {
            if let Err(e) = self.ensure_explore() {
                self.last_output = format!(
                    "{}Could not open your Arc Academy home: {}\n",
                    icons::error().content,
                    e
                );
                self.command_buffer.clear();
                return Ok(());
            }
            let home = self.explore.as_ref().expect("ensured above");
            let guard = arct_core::PlaygroundGuard::with_display(home.root(), EXPLORE_DISPLAY);
            (
                guard.check(&command_str, home.cwd()),
                Some(home.cwd().to_path_buf()),
            )
        } else {
            (arct_core::PlaygroundGuard::check_catastrophic(&command_str), None)
        };
        if let arct_core::GuardVerdict::Refuse { reason } = verdict {
            self.last_output = format!(
                "{}Blocked. Nothing ran.\n\n{}\n\n{}The safety guard turns dangerous commands into lessons.\n",
                icons::warning().content,
                reason,
                icons::hint().content
            );
            self.output_scroll = 0;
            self.add_to_history(command_str.clone());
            self.command_buffer.clear();
            self.record_feature_event("guard_refusal");
            return Ok(());
        }

        // Show executing status
        self.last_output = format!("{}Executing: {}\n", icons::loading().content, command_str);

        // Execute the command for real with timeout
        let start_time = std::time::Instant::now();

        // The executor enforces the timeout itself and kills the child
        // process if it fires (configurable via general.command_timeout)
        let timeout_duration = std::time::Duration::from_secs(self.config.general.command_timeout);
        let env_vars = self.environment_vars.clone();
        let output_result = self
            .shell_executor
            .execute(command_str.clone(), env_vars, timeout_duration, run_cwd)
            .await;

        let output = match output_result {
            Ok(output) => output,
            Err(e) => match e.downcast_ref::<crate::shell::CommandTimeout>() {
                Some(timeout) => timeout.to_string(),
                None => format!("{}Error: {}", icons::error().content, e),
            },
        };

        let duration = start_time.elapsed();

        // Store output
        self.last_output = output.clone();

        // Determine if command was successful
        let success = !Self::output_reports_failure(&output);

        // Reset scroll to top for new output
        self.output_scroll = 0;

        // Record in session
        self.session.record_command(
            command_str.clone(),
            Some(0),
            Some(duration.as_millis() as u64),
        );

        // Track in analytics database
        if let Some(ref analytics) = self.analytics {
            let working_dir = self.session.state.working_directory.to_string_lossy().to_string();
            let _ = analytics.record_command(
                &command_str,
                success,
                &working_dir,
                &self.session_id,
            );
        }

        // Add to history
        self.add_to_history(command_str.clone());

        // Track command use for stats and achievements
        self.record_command_for_stats(&command_str);

        // Check daily/weekly challenges after successful commands
        if success {
            self.check_challenges_for_command(&command_str);
        }

        self.check_and_unlock_achievements();

        // Telemetry: command name only (no arguments — privacy)
        let command_name = command_str
            .split_whitespace()
            .next()
            .unwrap_or("")
            .to_string();
        if !command_name.is_empty() {
            self.telemetry_record(arct_telemetry::TelemetryEvent::CommandExecuted {
                command: command_name,
                success,
                duration_ms: duration.as_millis() as u64,
            });
        }

        // Clear buffer
        self.command_buffer.clear();

        Ok(())
    }

    /// Handle cd command specially (it's a shell builtin)
    fn handle_cd_command(&mut self, cmd: &arct_core::Command) -> Result<()> {
        use std::path::PathBuf;

        // If in lesson mode, use virtual filesystem
        if self.lesson_mode {
            if let Some(ref mut vfs) = self.virtual_fs {
                let target_str = if cmd.args.is_empty() {
                    "~"
                } else {
                    &cmd.args[0]
                };

                match vfs.change_directory(target_str) {
                    Ok(new_path) => {
                        self.last_output = format!(
                            "{}Changed directory to:\n  {}\n\n{}You're in the virtual lesson filesystem\n",
                            icons::success().content, new_path, icons::hint().content
                        );
                        self.output_scroll = 0;
                        return Ok(());
                    }
                    Err(e) => {
                        self.last_output = format!("{}cd: {}\n", icons::error().content, e);
                        self.output_scroll = 0;
                        return Ok(());
                    }
                }
            }
        }

        // Normal mode - use real filesystem
        // Determine target directory
        let target = if cmd.args.is_empty() {
            // cd with no args goes to home directory
            dirs::home_dir().ok_or_else(|| anyhow::anyhow!("Could not find home directory"))?
        } else {
            let target_str = &cmd.args[0];

            // Expand ~ to home directory
            if target_str == "~" {
                dirs::home_dir().ok_or_else(|| anyhow::anyhow!("Could not find home directory"))?
            } else if target_str.starts_with("~/") {
                let home = dirs::home_dir().ok_or_else(|| anyhow::anyhow!("Could not find home directory"))?;
                home.join(&target_str[2..])
            } else {
                PathBuf::from(target_str)
            }
        };

        // Try to change directory
        match std::env::set_current_dir(&target) {
            Ok(_) => {
                // Update session working directory
                self.session.state.working_directory = std::env::current_dir()?;

                // Update context
                self.update_context()?;

                // Show success message
                let new_dir = std::env::current_dir()?;
                self.last_output = format!(
                    "{}Changed directory to:\n  {}\n",
                    icons::success().content, new_dir.display()
                );

                // Reset scroll
                self.output_scroll = 0;

                // Record in session
                self.session.record_command(
                    format!("cd {}", cmd.args.join(" ")),
                    Some(0),
                    Some(0),
                );

                Ok(())
            }
            Err(e) => {
                // Show error message
                self.last_output = format!(
                    "{}cd: {}\n  Cannot change to: {}\n",
                    icons::error().content, e, target.display()
                );
                self.output_scroll = 0;

                // Record in session as failed
                self.session.record_command(
                    format!("cd {}", cmd.args.join(" ")),
                    Some(1),
                    Some(0),
                );

                Ok(())
            }
        }
    }

    /// Update context (e.g., when directory changes)
    pub fn update_context(&mut self) -> Result<()> {
        let working_dir = &self.session.state.working_directory;
        self.context = ContextDetector::detect(working_dir)?;
        Ok(())
    }

    /// Navigate to previous command in history (Up arrow)
    fn history_previous(&mut self) {
        if self.command_history.is_empty() {
            return;
        }

        match self.history_position {
            None => {
                // Start browsing history from most recent
                self.history_position = Some(0);
                self.command_buffer = self.command_history[0].clone();
            }
            Some(pos) => {
                // Move to older command if possible
                if pos < self.command_history.len() - 1 {
                    let new_pos = pos + 1;
                    self.history_position = Some(new_pos);
                    self.command_buffer = self.command_history[new_pos].clone();
                }
            }
        }
    }

    /// Navigate to next command in history (Down arrow)
    fn history_next(&mut self) {
        match self.history_position {
            None => {
                // Not browsing history, do nothing
            }
            Some(0) => {
                // At most recent, clear buffer
                self.history_position = None;
                self.command_buffer.clear();
            }
            Some(pos) => {
                // Move to newer command
                let new_pos = pos - 1;
                self.history_position = Some(new_pos);
                self.command_buffer = self.command_history[new_pos].clone();
            }
        }
    }

    /// Add a command to history
    fn add_to_history(&mut self, command: String) {
        if command.trim().is_empty() {
            return;
        }

        // Don't add duplicate of most recent command
        if let Some(last) = self.command_history.first() {
            if last == &command {
                return;
            }
        }

        // Add to beginning (most recent first)
        self.command_history.insert(0, command);

        // Limit history size to 1000 commands
        if self.command_history.len() > 1000 {
            self.command_history.truncate(1000);
        }

        // Save all progress to disk
        self.save_session();
    }

    /// Save all session data (history, stats, progress) to disk
    fn save_session(&self) {
        let session_data = crate::persistence::SessionData {
            command_history: self.command_history.clone(),
            last_updated: chrono::Local::now().to_rfc3339(),
            user_stats: self.user_stats.clone(),
            completed_lessons: self.completed_lessons.clone(),
            challenge_manager: self.challenge_manager.clone(),
            lesson_progress: self.lesson_progress.clone(),
        };

        if let Err(e) = crate::persistence::save_session(&session_data) {
            tracing::warn!("Failed to save session: {}", e);
        }
    }

    /// Handle history command (show command history)
    fn handle_history_command(&mut self, cmd: &arct_core::Command) -> Result<()> {
        // Parse optional argument for number of commands to show
        let limit = if cmd.args.is_empty() {
            50 // Default: show last 50 commands
        } else {
            cmd.args[0].parse::<usize>().unwrap_or(50)
        };

        if self.command_history.is_empty() {
            self.last_output = "No commands in history yet.\n".to_string();
        } else {
            let mut output = String::new();
            let total = self.command_history.len();

            // Show commands in reverse chronological order (oldest to newest on screen)
            // but numbered from oldest to newest (like bash)
            for (i, cmd) in self.command_history.iter().rev().enumerate().take(limit) {
                let index = total - self.command_history.len() + i + 1;
                output.push_str(&format!("{:5}  {}\n", index, cmd));
            }

            self.last_output = output;
        }

        // Reset scroll
        self.output_scroll = 0;

        // Record in session
        self.session.record_command(
            format!("history {}", if cmd.args.is_empty() { String::new() } else { cmd.args.join(" ") }),
            Some(0),
            Some(0),
        );

        Ok(())
    }

    /// Handle export command (set environment variables)
    fn handle_export_command(&mut self, cmd: &arct_core::Command) -> Result<()> {
        if cmd.args.is_empty() {
            // No arguments - show all exported variables
            if self.environment_vars.is_empty() {
                self.last_output = "No environment variables set.\n".to_string();
            } else {
                let mut output = String::new();
                output.push_str("Exported environment variables:\n\n");
                let mut vars: Vec<_> = self.environment_vars.iter().collect();
                vars.sort_by_key(|(k, _)| *k);
                for (key, value) in vars {
                    output.push_str(&format!("  {}={}\n", key, value));
                }
                self.last_output = output;
            }
        } else {
            // Parse VAR=value format
            for arg in &cmd.args {
                if let Some((key, value)) = arg.split_once('=') {
                    let key = key.trim().to_string();
                    let value = value.trim().to_string();

                    // Remove quotes if present
                    let value = if (value.starts_with('"') && value.ends_with('"')) ||
                                   (value.starts_with('\'') && value.ends_with('\'')) {
                        value[1..value.len()-1].to_string()
                    } else {
                        value
                    };

                    self.environment_vars.insert(key.clone(), value.clone());
                    self.last_output = format!("{}Exported: {}={}\n", icons::success().content, key, value);

                    // Save to config
                    self.config.shell.environment = self.environment_vars.clone();
                    if let Err(e) = self.config.save() {
                        tracing::warn!("Failed to save config: {}", e);
                    }
                } else {
                    self.last_output = format!("{}Invalid export syntax: {}\n  Usage: export VAR=value\n", icons::error().content, arg);
                    break;
                }
            }
        }

        // Reset scroll
        self.output_scroll = 0;

        // Record in session
        self.session.record_command(
            format!("export {}", cmd.args.join(" ")),
            Some(0),
            Some(0),
        );

        Ok(())
    }

    /// Handle alias command (create command shortcuts)
    fn handle_alias_command(&mut self, cmd: &arct_core::Command) -> Result<()> {
        if cmd.args.is_empty() {
            // No arguments - show all aliases
            if self.aliases.is_empty() {
                self.last_output = "No aliases defined.\n".to_string();
            } else {
                let mut output = String::new();
                output.push_str("Defined aliases:\n\n");
                let mut aliases: Vec<_> = self.aliases.iter().collect();
                aliases.sort_by_key(|(k, _)| *k);
                for (name, command) in aliases {
                    output.push_str(&format!("  {}='{}'\n", name, command));
                }
                self.last_output = output;
            }
        } else {
            // Parse name=command format
            let arg = cmd.args.join(" ");
            if let Some((name, command)) = arg.split_once('=') {
                let name = name.trim().to_string();
                let command = command.trim().to_string();

                // Remove quotes if present
                let command = if (command.starts_with('"') && command.ends_with('"')) ||
                                 (command.starts_with('\'') && command.ends_with('\'')) {
                    command[1..command.len()-1].to_string()
                } else {
                    command
                };

                self.aliases.insert(name.clone(), command.clone());
                self.last_output = format!("{}Alias created: {}='{}'\n", icons::success().content, name, command);

                // Save to config
                self.config.shell.aliases = self.aliases.clone();
                if let Err(e) = self.config.save() {
                    tracing::warn!("Failed to save config: {}", e);
                }
            } else {
                self.last_output = format!("{}Invalid alias syntax: {}\n  Usage: alias name='command'\n", icons::error().content, arg);
            }
        }

        // Reset scroll
        self.output_scroll = 0;

        // Record in session
        self.session.record_command(
            format!("alias {}", cmd.args.join(" ")),
            Some(0),
            Some(0),
        );

        Ok(())
    }

    /// Handle Tab key autocompletion
    fn handle_autocomplete(&mut self) -> Result<()> {
        if self.command_buffer.is_empty() {
            return Ok(());
        }

        // Get completion results
        let working_dir = &self.session.state.working_directory;
        let result = self.autocompleter.complete(&self.command_buffer, working_dir)?;

        // If there's a unique completion or common prefix, apply it
        if !result.common_prefix.is_empty() && result.common_prefix != self.command_buffer {
            // Update the buffer with the common prefix
            // We need to replace the last token with the completion
            let tokens: Vec<&str> = self.command_buffer.split_whitespace().collect();

            if tokens.is_empty() {
                self.command_buffer = result.common_prefix.clone();
            } else if tokens.len() == 1 && !self.command_buffer.ends_with(' ') {
                // Completing first token (command)
                self.command_buffer = result.common_prefix.clone();
            } else {
                // Completing a path - replace last token
                let last_token = tokens.last().unwrap_or(&"");
                if let Some(idx) = self.command_buffer.rfind(last_token) {
                    self.command_buffer.truncate(idx);
                    self.command_buffer.push_str(&result.common_prefix);
                }
            }
        }

        // Store suggestions for display (limit to 10)
        self.completion_suggestions = result.completions.into_iter().take(10).collect();

        Ok(())
    }

    /// Toggle AI assistant mode
    pub fn toggle_ai_mode(&mut self) {
        if self.ai_provider.is_some() {
            self.ai_mode = !self.ai_mode;
            if self.ai_mode {
                // Clear AI input when entering AI mode
                self.ai_input_buffer.clear();
                self.ai_loading = false;
                self.record_feature_event("use_ai");
            }
        } else {
            self.last_output = format!("{}AI is not enabled. Configure it in ~/.config/arct/config.toml\n", icons::error().content);
        }
    }

    /// Toggle lesson mode
    pub fn toggle_lesson_mode(&mut self) {
        self.lesson_mode = !self.lesson_mode;

        if self.lesson_mode {
            if self.practice_mode_is_real() {
                // Real-filesystem practice: open the playground
                match self.ensure_playground() {
                    Ok(()) => {
                        let mut msg = format!(
                            "{}Lesson mode activated in REAL practice mode.\n\nCommands run for real inside ~/ArcAcademy/playground.\n\nPress Ctrl+L again to return to normal mode.\n",
                            icons::lesson().content
                        );
                        if let Some(intro) = self.real_mode_intro_if_needed() {
                            msg.push('\n');
                            msg.push_str(&intro);
                        }
                        self.last_output = msg;
                    }
                    Err(e) => {
                        self.last_output = format!(
                            "{}Failed to open the practice playground: {}\n",
                            icons::error().content,
                            e
                        );
                        self.lesson_mode = false;
                        return;
                    }
                }
            } else {
                // Simulated practice: initialize the virtual filesystem
                match arct_core::VirtualFileSystem::new("nav-basics", &self.session_id) {
                    Ok(vfs) => {
                        self.virtual_fs = Some(vfs);
                        self.sandbox = crate::sandbox::SandboxShell::new();
                        self.last_output = format!("{}Lesson mode activated! You're now in a safe virtual filesystem.\n\nPress Ctrl+L again to return to normal mode.\n\nNavigate through lessons using the Learning panel on the right.\n", icons::lesson().content);
                    }
                    Err(e) => {
                        self.last_output = format!("{}Failed to initialize lesson environment: {}\n", icons::error().content, e);
                        self.lesson_mode = false;
                        return;
                    }
                }
            }

            // Initialize lesson panel if not already done
            if self.lesson_panel.is_none() {
                self.lesson_panel = Self::initialize_lesson_panel();
            }

            // Show lesson menu to let user choose a lesson
            if self.lesson_menu.is_none() {
                self.lesson_menu = Some(crate::panels::lesson_menu::LessonMenuPanel::for_learner(
                    self.lesson_library.clone(),
                    self.preferred_track(),
                    &self.completed_lessons,
                ));
                self.mark_panel_visited("lesson_menu");
            }
        } else {
            // Clean up virtual filesystem (playground dirs are real files the
            // learner owns — they persist until an explicit reset)
            self.virtual_fs = None;
            self.last_output = format!("{}Lesson mode deactivated. Back to normal shell mode and real filesystem.\n", icons::learning().content);
        }
    }

    /// Whether lesson practice runs on the real filesystem playground
    fn practice_mode_is_real(&self) -> bool {
        self.config.lessons.is_real()
    }

    /// Default playground root: ~/ArcAcademy/playground
    fn playground_root() -> Result<std::path::PathBuf> {
        dirs::home_dir()
            .map(|home| home.join("ArcAcademy").join("playground"))
            .ok_or_else(|| anyhow::anyhow!("Could not find home directory"))
    }

    /// The learner's track from settings (Explorer unless set to operator)
    fn preferred_track(&self) -> arct_core::Track {
        if self.config.lessons.track.eq_ignore_ascii_case("operator") {
            arct_core::Track::Operator
        } else {
            arct_core::Track::Explorer
        }
    }

    /// Safe-mode home root: ~/ArcAcademy/home
    fn explore_root() -> Result<std::path::PathBuf> {
        dirs::home_dir()
            .map(|home| home.join("ArcAcademy").join("home"))
            .ok_or_else(|| anyhow::anyhow!("Could not find home directory"))
    }

    /// Open the safe-mode home on demand (idempotent). The first time, it is
    /// created with a few starter files so there is something to explore.
    fn ensure_explore(&mut self) -> Result<()> {
        if self.explore.is_none() {
            let root = Self::explore_root()?;
            if !root.exists() {
                std::fs::create_dir_all(&root)?;
                for (path, contents) in EXPLORE_STARTER_FILES {
                    let file = root.join(path);
                    if let Some(parent) = file.parent() {
                        std::fs::create_dir_all(parent)?;
                    }
                    std::fs::write(file, contents)?;
                }
            }
            let home = arct_core::Playground::open_with_display(root, EXPLORE_DISPLAY)?;
            self.session.state.working_directory = home.cwd().to_path_buf();
            self.explore = Some(home);
            self.update_context()?;
        }
        Ok(())
    }

    /// `cd` in safe mode: move within ~/ArcAcademy/home, never out of it
    fn handle_safe_cd(&mut self, cmd: &arct_core::Command) {
        self.output_scroll = 0;
        if let Err(e) = self.ensure_explore() {
            self.last_output = format!(
                "{}Could not open your Arc Academy home: {}\n",
                icons::error().content,
                e
            );
            return;
        }
        let target = cmd.args.first().map(String::as_str).unwrap_or("");
        let home = self.explore.as_mut().expect("ensured above");
        let result = home.change_directory(target);
        let cwd = home.cwd().to_path_buf();
        match result {
            Ok(new_path) => {
                self.session.state.working_directory = cwd;
                let _ = self.update_context();
                self.last_output = format!(
                    "{}Changed directory to:\n  {}\n",
                    icons::success().content,
                    new_path
                );
                self.session
                    .record_command(format!("cd {}", cmd.args.join(" ")), Some(0), Some(0));
            }
            Err(e) => {
                self.last_output = format!(
                    "{}cd: {}\n\n{}Safe mode keeps you inside {}. Adults can turn it off in settings (shell.safe_mode).\n",
                    icons::error().content,
                    e,
                    icons::hint().content,
                    EXPLORE_DISPLAY
                );
                self.session
                    .record_command(format!("cd {}", cmd.args.join(" ")), Some(1), Some(0));
            }
        }
    }

    /// Open the playground on demand (idempotent)
    fn ensure_playground(&mut self) -> Result<()> {
        if self.playground.is_none() {
            let root = Self::playground_root()?;
            self.playground = Some(arct_core::Playground::open(root)?);
        }
        Ok(())
    }

    /// One-time explainer shown on first entry into real practice mode.
    /// Returns the explainer text and persists the "already shown" flag.
    fn real_mode_intro_if_needed(&mut self) -> Option<String> {
        if self.config.lessons.real_mode_intro_shown {
            return None;
        }
        self.config.lessons.real_mode_intro_shown = true;
        if let Err(e) = self.config.save() {
            tracing::warn!("Failed to save config: {}", e);
        }
        Some(format!(
            "{}First time in REAL practice mode — here's what that means:\n\n\
             • The playground is a real folder on your computer: ~/ArcAcademy/playground\n\
             • Each lesson gets its own subfolder with starter files, and your session starts there\n\
             • Commands run in the real shell — files you create, change, or delete are REAL\n\
             • Guardrails keep commands inside the playground and block dangerous patterns\n\
             • \"Reset Lesson Playground\" in the command palette (Ctrl+K) restores the starter files\n\n\
             Everything outside ~/ArcAcademy/playground stays untouched. Have fun!\n",
            icons::info().content
        ))
    }

    /// Prepare the practice environment for the currently loaded lesson:
    /// materialize starter files in the playground (real mode) or seed them
    /// into a fresh virtual filesystem (simulated mode).
    fn prepare_lesson_environment(&mut self) {
        let lesson_info = self
            .lesson_panel
            .as_ref()
            .and_then(|p| p.current_lesson.as_ref())
            .map(|l| (l.id.clone(), l.setup.clone()));

        if self.practice_mode_is_real() {
            // Each lesson starts with no variables left over from the last
            self.sandbox = crate::sandbox::SandboxShell::new();
            if let Err(e) = self.ensure_playground() {
                self.last_output.push_str(&format!(
                    "{}Failed to open the practice playground: {}\n",
                    icons::error().content,
                    e
                ));
                return;
            }
            if let Some((lesson_id, setup)) = lesson_info {
                let result = self
                    .playground
                    .as_mut()
                    .expect("playground exists after ensure_playground")
                    .enter_lesson(&lesson_id, &setup);
                match result {
                    Ok(_) => {
                        let cwd = self
                            .playground
                            .as_ref()
                            .map(|p| p.display_cwd())
                            .unwrap_or_default();
                        self.last_output.push_str(&format!(
                            "\n{}REAL practice mode: working in {} (starter files ready).\n",
                            icons::folder().content,
                            cwd
                        ));
                    }
                    Err(e) => {
                        self.last_output.push_str(&format!(
                            "{}Failed to set up the lesson playground: {}\n",
                            icons::error().content,
                            e
                        ));
                    }
                }
            }
        } else {
            // Drop any previous sandbox BEFORE creating the new one: Drop
            // cleans up its temp root, which may be the same path when the
            // same lesson is re-entered in the same session
            self.virtual_fs = None;

            let lesson_id = lesson_info
                .as_ref()
                .map(|(id, _)| id.as_str())
                .unwrap_or("lesson");
            match arct_core::VirtualFileSystem::new(lesson_id, &self.session_id) {
                Ok(vfs) => {
                    if let Some((_, setup)) = &lesson_info {
                        if let Err(e) = vfs.seed_setup(setup) {
                            tracing::warn!("Failed to seed lesson setup files: {}", e);
                        }
                    }
                    self.virtual_fs = Some(vfs);
                    self.sandbox = crate::sandbox::SandboxShell::new();
                }
                Err(e) => {
                    tracing::warn!("Failed to initialize lesson sandbox: {}", e);
                }
            }
        }
    }

    /// Toggle lesson practice between the simulated sandbox and the real
    /// filesystem playground ("Toggle Real-Filesystem Practice")
    fn toggle_practice_mode(&mut self) {
        let to_real = !self.practice_mode_is_real();
        self.config.lessons.practice_mode = if to_real { "real" } else { "simulated" }.to_string();
        if let Err(e) = self.config.save() {
            tracing::warn!("Failed to save config: {}", e);
        }
        self.announce_practice_mode();
        self.record_feature_event("toggle_practice_mode");
    }

    /// Announce the active practice mode (and show the one-time real-mode
    /// explainer), then re-prepare the current lesson's environment
    fn announce_practice_mode(&mut self) {
        if self.practice_mode_is_real() {
            self.last_output = format!(
                "{}Real-filesystem practice is ON.\n\nLesson commands now run for real inside ~/ArcAcademy/playground.\n",
                icons::warning().content
            );
            if let Some(intro) = self.real_mode_intro_if_needed() {
                self.last_output.push('\n');
                self.last_output.push_str(&intro);
            }
        } else {
            self.last_output = format!(
                "{}Real-filesystem practice is OFF.\n\nLesson commands run in the safe simulated sandbox again.\n",
                icons::success().content
            );
        }
        self.output_scroll = 0;

        // Rebuild the environment for the mode we just switched into
        if self.lesson_mode {
            self.prepare_lesson_environment();
        }
    }

    /// Wipe and re-materialize the current lesson's playground directory
    /// ("Reset Lesson Playground"). Deletion is confined to
    /// ~/ArcAcademy/playground with canonical-prefix verification.
    fn reset_playground(&mut self) {
        if !self.lesson_mode || !self.practice_mode_is_real() {
            self.last_output = format!(
                "{}Playground reset applies to lessons in real practice mode. Enable it with \"Toggle Real-Filesystem Practice\" (Ctrl+K) and start a lesson first.\n",
                icons::hint().content
            );
            self.output_scroll = 0;
            return;
        }

        // Make sure the lesson's playground is registered (e.g. after a
        // mid-lesson mode switch), then reset it
        if self.playground.as_ref().map(|p| p.has_lesson()) != Some(true) {
            self.prepare_lesson_environment();
        }

        let result = self.playground.as_mut().map(|p| p.reset_lesson());
        self.last_output = match result {
            Some(Ok(_)) => {
                let cwd = self
                    .playground
                    .as_ref()
                    .map(|p| p.display_cwd())
                    .unwrap_or_default();
                format!(
                    "{}Playground reset — starter files restored in {}.\n",
                    icons::success().content,
                    cwd
                )
            }
            Some(Err(e)) => format!(
                "{}Could not reset the playground: {} (load a lesson first).\n",
                icons::warning().content,
                e
            ),
            None => format!(
                "{}The playground isn't open yet — start a lesson first.\n",
                icons::hint().content
            ),
        };
        self.output_scroll = 0;
    }

    /// Execute a lesson command against the real playground (practice_mode =
    /// "real"): guard-check it, handle the cd builtin with persistent cwd,
    /// and run everything else through the real shell executor.
    async fn execute_playground_command(
        &mut self,
        cmd: &arct_core::Command,
        command_str: &str,
    ) -> String {
        if let Err(e) = self.ensure_playground() {
            return format!(
                "{}The practice playground is unavailable: {}\n",
                icons::error().content,
                e
            );
        }

        // Safety guard: playground containment + catastrophic-pattern denylist
        let (verdict, cwd) = {
            let playground = self.playground.as_ref().expect("ensured above");
            let guard = arct_core::PlaygroundGuard::new(playground.root());
            (
                guard.check(command_str, playground.cwd()),
                playground.cwd().to_path_buf(),
            )
        };
        if let arct_core::GuardVerdict::Refuse { reason } = verdict {
            return format!("{}{}\n", icons::warning().content, reason);
        }

        // cd builtin with per-session cwd persistence (simple `cd` only;
        // compound commands run in the shell where cd affects a subshell)
        let is_simple = !command_str.contains(['|', ';', '&', '<', '>']);
        if cmd.program == "cd" && is_simple {
            let target = cmd.args.first().map(String::as_str).unwrap_or("");
            let playground = self.playground.as_mut().expect("ensured above");
            return match playground.change_directory(target) {
                Ok(new_path) => format!(
                    "{}Changed directory to:\n  {}\n",
                    icons::success().content,
                    new_path
                ),
                Err(e) => format!("{}cd: {}\n", icons::error().content, e),
            };
        }

        // Each command runs in a fresh shell, so variables set in a lesson
        // (NAME=value, export, unset) are kept here and handed to every
        // later command, for this lesson only
        if is_simple {
            if let Some(out) = self.sandbox.real_assignments(command_str) {
                return out;
            }
        }

        // Real execution, pinned to the tracked playground cwd, with the
        // same timeout-kill behavior as the normal shell
        let timeout = std::time::Duration::from_secs(self.config.general.command_timeout);
        let mut env_vars = self.environment_vars.clone();
        env_vars.extend(self.sandbox.real_env());
        match self
            .shell_executor
            .execute(command_str.to_string(), env_vars, timeout, Some(cwd))
            .await
        {
            Ok(output) => output,
            Err(e) => match e.downcast_ref::<crate::shell::CommandTimeout>() {
                Some(timeout) => timeout.to_string(),
                None => format!("{}Error: {}", icons::error().content, e),
            },
        }
    }

    /// Ask the AI assistant a question
    pub async fn ask_ai(&mut self, question: String) -> Result<()> {
        if self.ai_provider.is_none() {
            return Ok(());
        }

        if question.trim().is_empty() {
            return Ok(());
        }

        self.ai_loading = true;

        // Add user message to conversation
        self.ai_conversation.push(arct_ai::Message::user(question.clone()));

        // Build conversation with system prompt
        let user_name = self.config.general.user_name.as_deref().unwrap_or("there");
        let system_prompt = format!(
            "You are an AI teaching assistant integrated into Arc Academy Terminal, \
             an interactive terminal learning application. Your role is to help users \
             learn shell commands and terminal skills.\n\n\
             You're helping {}, so address them by name occasionally to make the \
             interaction personal and engaging.\n\n\
             Guidelines:\n\
             - Teach shell commands with clear, executable examples\n\
             - Explain concepts in beginner-friendly language\n\
             - Provide commands the user can type themselves in the terminal\n\
             - Keep responses concise (3-4 sentences or a short example)\n\
             - Focus on common Linux/Unix commands (bash, grep, find, etc.)\n\
             - Suggest safer alternatives when appropriate\n\
             - You are NOT Claude Code - you cannot execute commands or use tools\n\
             - You are a teaching assistant helping someone learn the terminal\n\
             - Be encouraging and supportive in your teaching approach",
            user_name
        );

        let mut messages = vec![
            arct_ai::Message::system(system_prompt),
        ];
        messages.extend(self.ai_conversation.clone());

        // Get response from AI
        // SAFETY: ai_provider is guaranteed to be Some when ai_mode is true
        // because toggle_ai_mode() checks ai_provider.is_some() before enabling
        let provider = self.ai_provider.as_ref()
            .expect("BUG: ai_provider must exist when ai_mode is true - this is a logic error");
        let response = provider.complete(&messages, None).await;

        self.ai_loading = false;

        match response {
            Ok(ai_response) => {
                // Strip markdown formatting for terminal display
                let cleaned_content = Self::strip_markdown(&ai_response.content);

                // Add assistant response to conversation
                self.ai_conversation.push(arct_ai::Message::assistant(ai_response.content.clone()));
                self.ai_response = Some(cleaned_content);
                Ok(())
            }
            Err(e) => {
                self.ai_response = Some(format!("{}Error: {}", icons::error().content, e));
                Err(anyhow::anyhow!("AI request failed: {}", e))
            }
        }
    }

    /// Clear AI conversation
    pub fn clear_ai_conversation(&mut self) {
        self.ai_conversation.clear();
        self.ai_response = None;
        self.ai_input_buffer.clear();
    }

    /// Strip markdown formatting from text for plain terminal display
    fn strip_markdown(text: &str) -> String {
        let mut result = String::new();
        let mut in_code_block = false;
        let mut skip_line = false;

        for line in text.lines() {
            // Toggle code block state
            if line.trim().starts_with("```") {
                in_code_block = !in_code_block;
                skip_line = true;
            }

            if skip_line {
                skip_line = false;
                continue;
            }

            // Clean the line
            let mut cleaned = line.to_string();

            // Remove headers (## Header -> Header)
            if cleaned.trim_start().starts_with('#') {
                cleaned = cleaned.trim_start().trim_start_matches('#').trim().to_string();
            }

            // Remove bold/italic markers
            cleaned = cleaned.replace("**", "").replace("*", "");

            // Remove inline code backticks (but not the content)
            cleaned = cleaned.replace('`', "");

            // Remove list markers (- item -> item, but keep indentation)
            if let Some(stripped) = cleaned.trim_start().strip_prefix("- ") {
                let indent = cleaned.len() - cleaned.trim_start().len();
                cleaned = format!("{}{}", " ".repeat(indent), stripped);
            }

            result.push_str(&cleaned);
            result.push('\n');
        }

        result.trim_end().to_string()
    }

    /// Handle onboarding wizard events
    async fn handle_onboarding_event(&mut self, key: KeyEvent) -> Result<()> {
        if let Some(wizard) = self.onboarding.as_mut() {
            match key.code {
                KeyCode::Char(c) if key.modifiers == KeyModifiers::NONE || key.modifiers == KeyModifiers::SHIFT => {
                    wizard.handle_char(c);
                }
                KeyCode::Backspace => {
                    wizard.handle_backspace();
                }
                KeyCode::Up => {
                    wizard.handle_up();
                }
                KeyCode::Esc => {
                    wizard.handle_back();
                }
                KeyCode::Down => {
                    let max_options = match wizard.step {
                        crate::panels::onboarding::OnboardingStep::AskAI => 2,
                        crate::panels::onboarding::OnboardingStep::AskTrack => 2,
                        crate::panels::onboarding::OnboardingStep::AskAIProvider => 3,
                        _ => 1,
                    };
                    wizard.handle_down(max_options);
                }
                KeyCode::Enter => {
                    wizard.handle_enter();

                    // Check if onboarding is complete
                    if wizard.step == crate::panels::onboarding::OnboardingStep::Complete {
                        // Save settings to config
                        if let Some(wizard) = self.onboarding.take() {
                            self.complete_onboarding(wizard).await?;
                        }
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }

    /// Complete onboarding and save settings
    async fn complete_onboarding(&mut self, wizard: crate::panels::onboarding::OnboardingWizard) -> Result<()> {
        // Update config with onboarding results
        if !wizard.user_name.is_empty() {
            self.config.general.user_name = Some(wizard.user_name.clone());
        }

        if let Some(track) = &wizard.track {
            self.config.lessons.track = track.clone();
        }

        if let Some(ai_enabled) = wizard.ai_enabled {
            self.config.ai.enabled = ai_enabled;

            if ai_enabled {
                // Configure AI provider based on user selection
                match wizard.ai_provider.as_deref() {
                    Some("claude-code") => {
                        // Uses the signed-in Claude Code CLI; no API key here
                        self.config.ai.provider = "claude-cli".to_string();
                    }
                    Some("own-key") => {
                        // Needs a key before it can work: stays off until set
                        self.config.ai.provider = "anthropic".to_string();
                        if self.config.ai.api_key.is_none() {
                            self.config.ai.enabled = false;
                        }
                    }
                    _ => {
                        // Local model via Ollama: questions stay on this machine
                        self.config.ai.provider = "local".to_string();
                        self.config.ai.endpoint = Some("http://localhost:11434".to_string());
                        self.config.ai.model = Some("llama3.2".to_string());
                    }
                }
            }
        }

        // Mark setup as complete
        self.config.general.setup_complete = true;

        // Save config
        self.config.save()?;

        // Reinitialize AI provider with new settings
        if self.config.ai.enabled {
            match Self::create_ai_provider(&self.config.ai) {
                Ok(provider) => {
                    self.ai_provider = Some(provider);
                }
                Err(e) => {
                    // Log error but don't fail onboarding
                    self.last_output = format!("{}AI provider initialization failed: {}\n", icons::warning().content, e);
                }
            }
        }

        // Remove onboarding
        self.onboarding = None;

        // Show greeting in output
        let name = self
            .config
            .general
            .user_name
            .as_deref()
            .filter(|n| !n.is_empty())
            .unwrap_or("explorer");
        let mut welcome_msg = format!(
            "{}Welcome, {}!\n\n\
             You're all set to start learning shell commands!\n\n",
            icons::celebration().content, name
        );

        if wizard.ai_provider.as_deref() == Some("own-key") && !self.config.ai.enabled {
            welcome_msg.push_str(&format!(
                "{}AI stays off until you add a key: set ARCT_AI_API_KEY, or put\n\
                 api_key in your config (arct info shows where), then turn AI on\n\
                 in settings (Ctrl+S).\n\n",
                icons::note().content
            ));
        }

        // Add provider-specific setup notes if AI is enabled
        if self.config.ai.enabled {
            match self.config.ai.provider.as_str() {
                "claude-cli" => {
                    welcome_msg.push_str(
                        &format!("{}Using Claude Code CLI - your Max subscription is ready!\n\
                         Press Ctrl+A to ask Claude for help.\n\n", icons::ai().content)
                    );
                }
                "anthropic" | "openai" => {
                    welcome_msg.push_str(
                        &format!("{}To use AI features, set your API key:\n\
                         export ARCT_AI_API_KEY=\"your-api-key-here\"\n\n", icons::note().content)
                    );
                }
                "local" => {
                    welcome_msg.push_str(
                        &format!("{}Using local LLM - make sure your server is running!\n\n", icons::info().content)
                    );
                }
                _ => {}
            }
        }

        welcome_msg.push_str("Press ? for help, or just start typing commands.\n");
        self.last_output = welcome_msg;

        Ok(())
    }

    /// Handle settings panel events
    async fn handle_settings_event(&mut self, key: KeyEvent) -> Result<()> {
        // Determine what action to take
        let (action, selected_field) = {
            let panel = match self.settings_panel.as_ref() {
                Some(p) => p,
                None => return Ok(()),
            };

            let action = if panel.editing {
                // In edit mode
                match key.code {
                    KeyCode::Char(c) if key.modifiers == KeyModifiers::NONE || key.modifiers == KeyModifiers::SHIFT => {
                        SettingsAction::PushChar(c)
                    }
                    KeyCode::Backspace => SettingsAction::PopChar,
                    KeyCode::Enter => SettingsAction::SaveEdit,
                    KeyCode::Esc => SettingsAction::CancelEdit,
                    _ => SettingsAction::None,
                }
            } else {
                // In navigation mode
                match key.code {
                    KeyCode::Up | KeyCode::Char('k') if key.modifiers == KeyModifiers::NONE => {
                        SettingsAction::PreviousField
                    }
                    KeyCode::Down | KeyCode::Char('j') if key.modifiers == KeyModifiers::NONE => {
                        SettingsAction::NextField
                    }
                    KeyCode::Enter => SettingsAction::StartEdit,
                    // Esc always closes (consistent with every other overlay);
                    // Ctrl+S toggles the panel shut too
                    KeyCode::Esc => SettingsAction::Close,
                    KeyCode::Char('s') if key.modifiers == KeyModifiers::CONTROL => {
                        SettingsAction::Close
                    }
                    _ => SettingsAction::None,
                }
            };

            (action, panel.selected_field)
        };

        // Execute the action
        match action {
            SettingsAction::PushChar(c) => {
                if let Some(panel) = self.settings_panel.as_mut() {
                    panel.push_char(c);
                }
            }
            SettingsAction::PopChar => {
                if let Some(panel) = self.settings_panel.as_mut() {
                    panel.pop_char();
                }
            }
            SettingsAction::SaveEdit => {
                let mut theme_changed = false;
                let mut practice_mode_changed = false;
                let practice_mode_before = self.config.lessons.practice_mode.clone();
                if let Some(panel) = self.settings_panel.as_mut() {
                    panel.save_edit(&mut self.config)?;

                    // If practice mode was changed, announce it and rebuild
                    // the lesson environment for the new mode
                    if selected_field == crate::panels::settings::SettingField::PracticeMode
                        && self.config.lessons.practice_mode != practice_mode_before
                    {
                        practice_mode_changed = true;
                    }

                    // Safe mode toggled: move the shell into or out of
                    // ~/ArcAcademy/home
                    if selected_field == crate::panels::settings::SettingField::SafeMode {
                        if self.config.shell.safe_mode {
                            self.explore = None;
                            let _ = self.ensure_explore();
                        } else {
                            self.explore = None;
                            if let Ok(dir) = std::env::current_dir() {
                                self.session.state.working_directory = dir;
                            }
                            let _ = self.update_context();
                        }
                    }

                    // If theme was changed, reload it
                    if selected_field == crate::panels::settings::SettingField::Theme {
                        self.theme = Theme::from_name(&self.config.theme.default_theme);
                        theme_changed = true;
                    }

                    // If AI was toggled, reload provider
                    if selected_field == crate::panels::settings::SettingField::AIEnabled {
                        if self.config.ai.enabled {
                            // Try to initialize AI provider
                            match Self::create_ai_provider(&self.config.ai) {
                                Ok(provider) => {
                                    self.ai_provider = Some(provider);
                                }
                                Err(e) => {
                                    tracing::warn!("Failed to initialize AI provider: {}", e);
                                    self.ai_provider = None;
                                }
                            }
                        } else {
                            self.ai_provider = None;
                            self.ai_mode = false;
                        }
                    }
                }

                if theme_changed {
                    self.record_feature_event("change_theme");
                }
                if practice_mode_changed {
                    self.announce_practice_mode();
                }
            }
            SettingsAction::CancelEdit => {
                if let Some(panel) = self.settings_panel.as_mut() {
                    panel.cancel_editing();
                }
            }
            SettingsAction::PreviousField => {
                if let Some(panel) = self.settings_panel.as_mut() {
                    panel.previous_field();
                }
            }
            SettingsAction::NextField => {
                if let Some(panel) = self.settings_panel.as_mut() {
                    panel.next_field();
                }
            }
            SettingsAction::StartEdit => {
                if let Some(panel) = self.settings_panel.as_mut() {
                    panel.start_editing(&self.config);
                }
            }
            SettingsAction::Close => {
                self.settings_panel = None;
            }
            SettingsAction::None => {}
        }

        Ok(())
    }

    /// Toggle achievements panel
    pub fn toggle_achievements_panel(&mut self) {
        if self.achievements_panel.is_some() {
            self.achievements_panel = None;
        } else {
            self.achievements_panel = Some(crate::panels::achievements::AchievementsPanel::new());
            self.mark_panel_visited("achievements");
        }
    }

    /// Toggle progress panel
    pub fn toggle_progress_panel(&mut self) {
        if self.progress_panel.is_some() {
            self.progress_panel = None;
        } else {
            self.progress_panel = Some(crate::panels::progress::ProgressPanel::new());
            self.mark_panel_visited("progress");
        }
    }

    /// Toggle challenges panel
    pub fn toggle_challenges_panel(&mut self) {
        if self.challenges_panel.is_some() {
            self.challenges_panel = None;
        } else {
            self.challenges_panel = Some(crate::panels::challenges::ChallengesPanel::new());
            self.mark_panel_visited("challenges");
        }
    }

    /// Record that a panel was visited this session; fires the
    /// "visit_all_panels" event once every visitable panel has been opened
    fn mark_panel_visited(&mut self, panel: &'static str) {
        if self.visited_panels.insert(panel)
            && ALL_VISITABLE_PANELS
                .iter()
                .all(|p| self.visited_panels.contains(p))
        {
            self.record_feature_event("visit_all_panels");
        }
    }

    /// Record a one-shot feature-usage event, surface any newly unlocked
    /// achievements through the notification queue, and persist
    pub(crate) fn record_feature_event(&mut self, key: &str) {
        let newly_unlocked = self.user_stats.record_event(key);
        if !newly_unlocked.is_empty() {
            for achievement in &newly_unlocked {
                self.queue_notification(
                    crate::panels::notification::NotificationPanel::achievement(achievement),
                );
            }
            self.save_session();
        }
        self.telemetry_record(arct_telemetry::TelemetryEvent::FeatureUsed {
            feature: key.to_string(),
            context: None,
        });
    }

    /// Record a telemetry event if telemetry is enabled (local-only)
    fn telemetry_record(&self, event: arct_telemetry::TelemetryEvent) {
        if let Some(ref telemetry) = self.telemetry {
            if let Err(e) = telemetry.record(event) {
                tracing::warn!("Failed to record telemetry event: {}", e);
            }
        }
    }

    /// Check the executed command against active daily/weekly challenges.
    /// Surfaces completed challenges AND any achievements unlocked as a side
    /// effect (e.g. "challenge_accepted") through the notification queue.
    fn check_challenges_for_command(&mut self, command: &str) {
        // Snapshot unlocked achievements: check_command routes the
        // "first_challenge" event into UserStats internally, so newly
        // unlocked achievements have to be diffed, not returned
        let unlocked_before = self.user_stats.achievements.unlocked.clone();

        let completed = self
            .challenge_manager
            .check_command(command, &mut self.user_stats);

        if let Some(ref challenge) = completed {
            self.queue_notification(
                crate::panels::notification::NotificationPanel::challenge(challenge),
            );
            self.telemetry_record(arct_telemetry::TelemetryEvent::FeatureUsed {
                feature: "challenge_completed".to_string(),
                context: None,
            });
        }

        // Surface achievements unlocked inside check_command
        for achievement in arct_core::all_achievements() {
            if self.user_stats.achievements.is_unlocked(&achievement.id)
                && !unlocked_before.contains(&achievement.id)
            {
                self.queue_notification(
                    crate::panels::notification::NotificationPanel::achievement(&achievement),
                );
            }
        }

        if completed.is_some() {
            self.save_session();
        }
    }

    /// Check for newly unlocked achievements and queue notifications
    pub fn check_and_unlock_achievements(&mut self) {
        let newly_unlocked = self
            .user_stats
            .check_achievements_with_library(&self.lesson_library);

        if !newly_unlocked.is_empty() {
            for achievement in &newly_unlocked {
                self.queue_notification(
                    crate::panels::notification::NotificationPanel::achievement(achievement),
                );
            }
            self.save_session();
        }
    }

    /// Queue a notification popup (shown immediately if none is showing)
    fn queue_notification(&mut self, notification: crate::panels::notification::NotificationPanel) {
        if self.showing_notification.is_none() {
            self.showing_notification = Some(notification);
            // Restart the celebratory border animation
            self.notification_ticks = 0;
        } else {
            self.pending_notifications.push(notification);
        }
    }

    /// Dismiss the current notification and show the next queued one, if any
    fn dismiss_notification(&mut self) {
        self.showing_notification = None;

        if !self.pending_notifications.is_empty() {
            self.showing_notification = Some(self.pending_notifications.remove(0));
            self.notification_ticks = 0;
        }
    }

    /// Record command execution for stats tracking
    pub fn record_command_for_stats(&mut self, command: &str) {
        // Extract just the command name (first word)
        let command_name = command.split_whitespace().next().unwrap_or("");
        if !command_name.is_empty() {
            self.user_stats.record_command_use(command_name.to_string());
            self.user_stats.update_streak();
        }
    }

    /// Record lesson completion and check for achievements.
    ///
    /// `estimated_minutes` / `elapsed_secs` / `wrong_answers` feed the
    /// "speed_lesson" and "perfect_lesson" achievement events; `mastered`
    /// is true when the run finished without the answer being revealed.
    pub fn record_lesson_completion(
        &mut self,
        lesson_id: String,
        difficulty: arct_core::Difficulty,
        estimated_minutes: u32,
        elapsed_secs: u64,
        wrong_answers: usize,
        mastered: bool,
    ) {
        self.user_stats.update_streak();
        if mastered {
            self.user_stats.record_lesson_mastered(lesson_id.clone());
        }
        // Record in stats. Explorer lessons count toward lessons completed
        // and their own track badge, not the Operator difficulty badges.
        let is_explorer = self
            .lesson_library
            .get(&lesson_id)
            .is_some_and(|l| l.track == arct_core::Track::Explorer);
        if is_explorer {
            self.user_stats.lessons_completed.insert(lesson_id.clone());
        } else {
            self.user_stats
                .record_lesson_completion(lesson_id.clone(), difficulty);
        }

        // Add to completed lessons set and clear any step-level resume state
        self.completed_lessons.insert(lesson_id.clone());
        self.lesson_progress.remove(&lesson_id);

        // Celebration banner in the output area — completing a lesson
        // should feel like a moment
        let lesson_title = self
            .lesson_library
            .get(&lesson_id)
            .map(|l| l.title.clone())
            .unwrap_or_else(|| lesson_id.clone());
        let xp = crate::level::lesson_xp(difficulty);
        self.last_output = format!(
            "{}\n{}",
            crate::celebrate::banner("LESSON COMPLETE!", &lesson_title, xp),
            self.last_output
        );
        self.output_scroll = 0;

        // Track badges: finished every lesson, and finished them all
        // without needing the answer
        let track = if is_explorer {
            arct_core::Track::Explorer
        } else {
            arct_core::Track::Operator
        };
        let progress = self.user_stats.track_progress(&self.lesson_library, track);
        let key = crate::streak::track_name(track).to_lowercase();
        let first_time = !self
            .user_stats
            .completed_challenges
            .contains(&format!("{key}_complete"));
        if progress.is_complete() {
            self.record_feature_event(&format!("{key}_complete"));
        }
        if progress.is_complete() && first_time {
            self.last_output.push_str(&format!(
                "\nYou finished the {} track. Print your certificate with: arct certificate --track {key}\n",
                crate::streak::track_name(track)
            ));
        }
        if progress.is_mastered() {
            self.record_feature_event(&format!("{key}_mastered"));
        }

        // Speed / perfect lesson events
        if elapsed_secs < u64::from(estimated_minutes) * 60 {
            self.record_feature_event("speed_lesson");
        }
        if wrong_answers == 0 {
            self.record_feature_event("perfect_lesson");
        }

        // Check for newly unlocked achievements (uses the full library so
        // per-difficulty totals include user lesson packs)
        self.check_and_unlock_achievements();
        self.save_session();

        self.telemetry_record(arct_telemetry::TelemetryEvent::FeatureUsed {
            feature: "lesson_completed".to_string(),
            context: None,
        });

        // Recommend what to learn next
        let recommendations = self.recommendation_engine.get_recommendations(
            &self.completed_lessons,
            &self.user_stats,
            1,
        );
        if let Some(rec) = recommendations.first() {
            self.last_output.push_str(&format!(
                "\n{}Next up: {} — press m for the menu\n",
                icons::target().content,
                rec.lesson.title
            ));
        }
    }

    /// Persist the current lesson's step position for step-level resume
    fn save_lesson_progress(&mut self) {
        let state = self.lesson_panel.as_ref().and_then(|panel| {
            panel.current_lesson.as_ref().map(|lesson| {
                (
                    lesson.id.clone(),
                    crate::persistence::LessonResumeState {
                        current_step_index: panel.current_step_index(),
                        completed_steps: panel.completed_steps().to_vec(),
                    },
                )
            })
        });

        if let Some((lesson_id, state)) = state {
            self.lesson_progress.insert(lesson_id, state);
            self.save_session();
        }
    }

    /// Go back one lesson step (Alt+Left in lesson mode)
    fn lesson_previous_step(&mut self) {
        if !self.lesson_mode {
            return;
        }

        let moved = match self.lesson_panel.as_mut() {
            Some(panel) if panel.current_lesson.is_some() => {
                if panel.current_step_index() > 0 {
                    panel.previous_step();
                    Some(panel.current_step_index())
                } else {
                    None
                }
            }
            _ => return,
        };

        match moved {
            Some(step_index) => {
                self.last_output = format!(
                    "{}Went back to step {}.\n",
                    icons::lesson().content,
                    step_index + 1
                );
                self.save_lesson_progress();
            }
            None => {
                self.last_output = format!(
                    "{}Already at the first step of this lesson.\n",
                    icons::hint().content
                );
            }
        }
    }

    /// Restart the current lesson from step 1 (Alt+R in lesson mode)
    fn lesson_restart(&mut self) {
        if !self.lesson_mode {
            return;
        }

        let lesson_id = match self.lesson_panel.as_mut() {
            Some(panel) if panel.current_lesson.is_some() => {
                panel.restart();
                panel.current_lesson.as_ref().map(|l| l.id.clone())
            }
            _ => return,
        };

        if let Some(lesson_id) = lesson_id {
            self.lesson_progress.remove(&lesson_id);
            self.save_session();
            self.last_output = format!(
                "{}Lesson restarted from step 1.\n",
                icons::lesson().content
            );

            // In real practice mode, restarting also wipes and re-materializes
            // the lesson's playground directory (safe: deletion is confined to
            // ~/ArcAcademy/playground with canonical-prefix verification)
            if self.practice_mode_is_real() {
                if self.playground.as_ref().map(|p| p.has_lesson()) != Some(true) {
                    self.prepare_lesson_environment();
                }
                match self.playground.as_mut().map(|p| p.reset_lesson()) {
                    Some(Ok(_)) => {
                        self.last_output.push_str(&format!(
                            "{}Playground wiped — starter files restored.\n",
                            icons::success().content
                        ));
                    }
                    Some(Err(e)) => {
                        self.last_output.push_str(&format!(
                            "{}Could not reset the playground: {}\n",
                            icons::warning().content,
                            e
                        ));
                    }
                    None => {}
                }
            }
        }
    }

    /// Execute a command selected from the command palette.
    ///
    /// Everything routes through the normal `Action` dispatch so behavior
    /// stays in one place; the only special cases are direct theme selection
    /// and opening the lesson menu from outside lesson mode.
    async fn execute_palette_command(
        &mut self,
        command: crate::panels::command_palette::PaletteCommand,
    ) -> Result<()> {
        use crate::panels::command_palette::PaletteCommand;

        match command {
            PaletteCommand::SetTheme(name) => {
                let from = self.theme.name.clone();
                self.theme = Theme::from_name(name);
                self.record_feature_event("change_theme");
                self.telemetry_record(arct_telemetry::TelemetryEvent::ThemeChanged {
                    from,
                    to: name.to_string(),
                });
                Ok(())
            }
            PaletteCommand::Action(Action::ShowLessonMenu) if !self.lesson_mode => {
                // The lesson menu only exists in lesson mode; entering lesson
                // mode opens the menu automatically
                self.toggle_lesson_mode();
                Ok(())
            }
            PaletteCommand::Action(action) => self.handle_action(action).await,
        }
    }
}

/// Actions that can be performed in the settings panel
enum SettingsAction {
    PushChar(char),
    PopChar,
    SaveEdit,
    CancelEdit,
    PreviousField,
    NextField,
    StartEdit,
    Close,
    None,
}

#[cfg(test)]
mod playthrough_tests {
    use super::*;
    use arct_core::{CommandValidation, LessonStep, StepType};

    /// What a learner would type to finish a step: the lesson's own
    /// solution, or the expected answer for steps that have one
    fn step_inputs(lesson_id: &str, step: &LessonStep) -> Vec<String> {
        if !step.solution.is_empty() {
            return step.solution.clone();
        }
        match &step.step_type {
            StepType::CommandExercise {
                expected_command,
                validation,
                ..
            } => match validation {
                CommandValidation::AnyOf(options) => vec![options[0].clone()],
                _ => vec![expected_command.clone()],
            },
            StepType::MultipleChoice { correct_index, .. } => vec![correct_index.to_string()],
            StepType::FillInBlank {
                template,
                correct_answers,
                ..
            } => {
                let answer = &correct_answers[0];
                let filled = match (template.find('{'), template.find('}')) {
                    (Some(a), Some(b)) if a < b => {
                        format!("{}{}{}", &template[..a], answer, &template[b + 1..])
                    }
                    _ => answer.clone(),
                };
                vec![filled]
            }
            StepType::Information { .. } => vec![String::new()],
            StepType::Practice { .. } | StepType::Task { .. } => panic!(
                "{lesson_id} step {} '{}' is a task and needs a `solution`",
                step.step_number, step.title
            ),
        }
    }

    /// Plays every built-in lesson, start to finish, in the simulated
    /// sandbox, through the same code path a learner's Enter key takes
    #[tokio::test]
    async fn every_builtin_lesson_can_be_completed() {
        let dir = std::env::temp_dir().join(format!("arct-playthrough-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::env::set_var("ARCT_DATA_DIR", &dir);
        let config = dir.join("config.toml");
        std::fs::write(
            &config,
            "[general]\nsetup_complete = true\n[lessons]\npractice_mode = \"simulated\"\n",
        )
        .unwrap();

        let library = arct_core::LessonLibrary::new();
        for lesson in library.all() {
            let mut app = App::new_with_options(crate::RunOptions {
                config_path: Some(config.clone()),
                ..Default::default()
            })
            .unwrap();
            app.lesson_mode = true;
            app.lesson_panel
                .as_mut()
                .expect("lesson panel")
                .load_lesson(lesson.clone());
            app.prepare_lesson_environment();

            for step in &lesson.steps {
                let before = app.lesson_panel.as_ref().unwrap().current_step_index();
                for input in step_inputs(&lesson.id, step) {
                    app.command_buffer = input;
                    app.execute_command().await.unwrap();
                }
                let after = app.lesson_panel.as_ref().unwrap().current_step_index();
                let finished = app.completed_lessons.contains(&lesson.id);
                assert!(
                    after == before + 1 || finished,
                    "{} step {} '{}' did not pass.\n--- output ---\n{}",
                    lesson.id,
                    step.step_number,
                    step.title,
                    app.last_output
                );
            }
            assert!(
                app.completed_lessons.contains(&lesson.id),
                "{} was not recorded as completed",
                lesson.id
            );
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
