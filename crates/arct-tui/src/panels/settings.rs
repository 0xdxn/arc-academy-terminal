//! Interactive settings panel for configuration

use crate::icons;
use crate::theme::Theme;
use anyhow::Result;
use ratatui::{
    layout::Rect,
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
    Frame,
};

/// Editable settings fields
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingField {
    UserName,
    AIEnabled,
    AIProvider,
    PracticeMode,
    SafeMode,
    Theme,
}

impl SettingField {
    /// Get the next field
    pub fn next(self) -> Self {
        match self {
            Self::UserName => Self::AIEnabled,
            Self::AIEnabled => Self::AIProvider,
            Self::AIProvider => Self::PracticeMode,
            Self::PracticeMode => Self::SafeMode,
            Self::SafeMode => Self::Theme,
            Self::Theme => Self::UserName,
        }
    }

    /// Get the previous field
    pub fn previous(self) -> Self {
        match self {
            Self::UserName => Self::Theme,
            Self::AIEnabled => Self::UserName,
            Self::AIProvider => Self::AIEnabled,
            Self::PracticeMode => Self::AIProvider,
            Self::SafeMode => Self::PracticeMode,
            Self::Theme => Self::SafeMode,
        }
    }

    /// Get field name for display
    pub fn name(&self) -> &str {
        match self {
            Self::UserName => "Name",
            Self::AIEnabled => "AI Assistant",
            Self::AIProvider => "AI Provider",
            Self::PracticeMode => "Lesson Practice Mode",
            Self::SafeMode => "Safe Mode",
            Self::Theme => "Theme",
        }
    }
}

/// Interactive settings panel
pub struct SettingsPanel {
    /// Currently selected field
    pub selected_field: SettingField,

    /// Whether we're in edit mode
    pub editing: bool,

    /// Edit buffer for current field
    pub edit_buffer: String,
}

impl SettingsPanel {
    pub fn new() -> Self {
        Self {
            selected_field: SettingField::UserName,
            editing: false,
            edit_buffer: String::new(),
        }
    }

    /// Move selection up
    pub fn previous_field(&mut self) {
        self.selected_field = self.selected_field.previous();
    }

    /// Move selection down
    pub fn next_field(&mut self) {
        self.selected_field = self.selected_field.next();
    }

    /// Start editing the selected field
    pub fn start_editing(&mut self, config: &arct_config::Config) {
        self.editing = true;
        self.edit_buffer = match self.selected_field {
            SettingField::UserName => {
                config.general.user_name.clone().unwrap_or_default()
            }
            SettingField::AIEnabled => {
                if config.ai.enabled { "true" } else { "false" }.to_string()
            }
            SettingField::AIProvider => {
                config.ai.provider.clone()
            }
            SettingField::PracticeMode => {
                config.lessons.practice_mode.clone()
            }
            SettingField::SafeMode => {
                if config.shell.safe_mode { "on" } else { "off" }.to_string()
            }
            SettingField::Theme => {
                config.theme.default_theme.clone()
            }
        };
    }

    /// Cancel editing
    pub fn cancel_editing(&mut self) {
        self.editing = false;
        self.edit_buffer.clear();
    }

    /// Save the current edit to config
    pub fn save_edit(&mut self, config: &mut arct_config::Config) -> Result<()> {
        match self.selected_field {
            SettingField::UserName => {
                if self.edit_buffer.is_empty() {
                    config.general.user_name = None;
                } else {
                    config.general.user_name = Some(self.edit_buffer.clone());
                }
            }
            SettingField::AIEnabled => {
                let value = self.edit_buffer.to_lowercase();
                let enabling = value == "true" || value == "yes" || value == "1" || value == "enabled";

                // If enabling AI and no provider is set, use smart defaults
                if enabling && !config.ai.enabled {
                    // Check if provider is unset or set to default anthropic
                    if config.ai.provider.is_empty() || config.ai.provider == "anthropic" {
                        // Try to detect Claude CLI first
                        if arct_ai::claude_cli::ClaudeCLIProvider::is_available() {
                            config.ai.provider = "claude-cli".to_string();
                            config.ai.model = Some("claude-sonnet-4".to_string());
                        } else {
                            // Fall back to local LLM
                            config.ai.provider = "local".to_string();
                            config.ai.endpoint = Some("http://localhost:11434".to_string());
                            config.ai.model = Some("llama3.2".to_string());
                        }
                    }
                }

                config.ai.enabled = enabling;
            }
            SettingField::AIProvider => {
                config.ai.provider = self.edit_buffer.clone();
            }
            SettingField::PracticeMode => {
                // Accept "real" (or "r") for real-filesystem practice;
                // anything else falls back to the safe simulated sandbox
                let value = self.edit_buffer.trim().to_lowercase();
                config.lessons.practice_mode = if value == "real" || value == "r" {
                    "real".to_string()
                } else {
                    "simulated".to_string()
                };
            }
            SettingField::SafeMode => {
                // Only an explicit "off" turns it off; anything else keeps it on
                let value = self.edit_buffer.trim().to_lowercase();
                config.shell.safe_mode = !(value == "off" || value == "false" || value == "0");
            }
            SettingField::Theme => {
                config.theme.default_theme = self.edit_buffer.clone();
            }
        }

        config.save()?;
        self.editing = false;
        self.edit_buffer.clear();
        Ok(())
    }

    /// Add character to edit buffer
    pub fn push_char(&mut self, c: char) {
        if self.editing {
            self.edit_buffer.push(c);
        }
    }

    /// Remove last character from edit buffer
    pub fn pop_char(&mut self) {
        if self.editing {
            self.edit_buffer.pop();
        }
    }

    /// Render the settings panel as a centered overlay
    pub fn render(&self, frame: &mut Frame, theme: &Theme, config: &arct_config::Config) {
        let area = frame.size();

        // Create centered modal
        let modal_width = 80.min(area.width - 4);
        let modal_height = 28.min(area.height - 4);

        let horizontal_margin = (area.width.saturating_sub(modal_width)) / 2;
        let vertical_margin = (area.height.saturating_sub(modal_height)) / 2;

        let modal_area = Rect {
            x: horizontal_margin,
            y: vertical_margin,
            width: modal_width,
            height: modal_height,
        };

        // Clear background
        frame.render_widget(Clear, modal_area);

        let title = if self.editing {
            format!(" Settings - Editing: {} ", self.selected_field.name())
        } else {
            " Settings - Use ↑↓ to navigate, Enter to edit ".to_string()
        };

        let block = Block::default()
            .title(title)
            .borders(Borders::ALL)
            .border_style(theme.style_border_focused())
            .style(theme.style_block());  // Set background for light themes

        let inner = block.inner(modal_area);
        frame.render_widget(block, modal_area);

        // Build settings display
        let user_name = config.general.user_name.as_deref().unwrap_or("Not set");
        let ai_status = if config.ai.enabled { "Enabled" } else { "Disabled" };
        let ai_provider = &config.ai.provider;
        let theme_name = &config.theme.default_theme;

        let mut lines = vec![
            Line::from(""),
            Line::from(vec![
                Span::styled("  User Profile", theme.style_header()),
            ]),
        ];

        // User Name field
        self.add_field_line(
            &mut lines,
            SettingField::UserName,
            "Name",
            if self.editing && self.selected_field == SettingField::UserName {
                &self.edit_buffer
            } else {
                user_name
            },
            theme,
        );

        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled("  AI Assistant", theme.style_header()),
        ]));

        // AI Enabled field
        self.add_field_line(
            &mut lines,
            SettingField::AIEnabled,
            "Status",
            if self.editing && self.selected_field == SettingField::AIEnabled {
                &self.edit_buffer
            } else {
                ai_status
            },
            theme,
        );

        // AI Provider field
        self.add_field_line(
            &mut lines,
            SettingField::AIProvider,
            "Provider",
            if self.editing && self.selected_field == SettingField::AIProvider {
                &self.edit_buffer
            } else {
                ai_provider
            },
            theme,
        );

        // Add helpful note about AI setup
        if config.ai.enabled {
            let note = match ai_provider.as_str() {
                "claude-cli" => format!("    {}Using Claude Code CLI - no API key needed", icons::hint().content),
                "anthropic" | "openai" => format!("    {}Set API key in config.toml", icons::hint().content),
                "local" => format!("    {}Make sure your local LLM server is running", icons::hint().content),
                "managed" => format!("    {}Using Arc Academy managed AI", icons::hint().content),
                _ => format!("    {}Invalid provider - press Enter to configure", icons::warning().content),
            };
            lines.push(Line::from(vec![
                Span::styled(note, theme.style_dim()),
            ]));
        } else {
            lines.push(Line::from(vec![
                Span::styled(format!("    {}Enable to use AI features (auto-configures provider)", icons::hint().content), theme.style_dim()),
            ]));
        }

        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled("  Lessons", theme.style_header()),
        ]));

        // Lesson practice mode field
        let practice_mode = &config.lessons.practice_mode;
        self.add_field_line(
            &mut lines,
            SettingField::PracticeMode,
            "Practice",
            if self.editing && self.selected_field == SettingField::PracticeMode {
                &self.edit_buffer
            } else {
                practice_mode
            },
            theme,
        );
        let practice_note = if config.lessons.is_real() {
            format!(
                "    {}Lesson commands run for real in ~/ArcAcademy/playground",
                icons::warning().content
            )
        } else {
            format!(
                "    {}\"simulated\" = safe sandbox, \"real\" = practice in ~/ArcAcademy/playground",
                icons::hint().content
            )
        };
        lines.push(Line::from(vec![Span::styled(practice_note, theme.style_dim())]));

        // Safe mode for the free shell
        let safe_value = if config.shell.safe_mode { "on" } else { "off" };
        self.add_field_line(
            &mut lines,
            SettingField::SafeMode,
            "Safe mode",
            if self.editing && self.selected_field == SettingField::SafeMode {
                &self.edit_buffer
            } else {
                safe_value
            },
            theme,
        );
        let safe_note = if config.shell.safe_mode {
            format!(
                "    {}The shell stays inside ~/ArcAcademy/home. Type \"off\" to use your real folders.",
                icons::hint().content
            )
        } else {
            format!(
                "    {}Off: commands run in your real folders. Only catastrophic ones are blocked.",
                icons::warning().content
            )
        };
        lines.push(Line::from(vec![Span::styled(safe_note, theme.style_dim())]));

        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled("  Appearance", theme.style_header()),
        ]));

        // Theme field
        self.add_field_line(
            &mut lines,
            SettingField::Theme,
            "Theme",
            if self.editing && self.selected_field == SettingField::Theme {
                &self.edit_buffer
            } else {
                theme_name
            },
            theme,
        );

        // List the valid theme names so typing one isn't a guessing game
        lines.push(Line::from(vec![Span::styled(
            format!(
                "    {}Themes: {}",
                icons::hint().content,
                Theme::all_names().join(", ")
            ),
            theme.style_dim(),
        )]));

        lines.push(Line::from(""));
        lines.push(Line::from(""));

        if self.editing {
            lines.push(Line::from(vec![
                Span::styled("  Press ", theme.style_dim()),
                Span::styled("Enter", theme.style_accent()),
                Span::styled(" to save, ", theme.style_dim()),
                Span::styled("Esc", theme.style_accent()),
                Span::styled(" to cancel", theme.style_dim()),
            ]));
        } else {
            lines.push(Line::from(vec![
                Span::styled("  Controls", theme.style_header()),
            ]));
            lines.push(Line::from(vec![
                Span::styled("    • Press ", theme.style_dim()),
                Span::styled("↑↓", theme.style_accent()),
                Span::styled(" to navigate fields", theme.style_dim()),
            ]));
            lines.push(Line::from(vec![
                Span::styled("    • Press ", theme.style_dim()),
                Span::styled("Enter", theme.style_accent()),
                Span::styled(" to edit selected field", theme.style_dim()),
            ]));
            lines.push(Line::from(vec![
                Span::styled("    • Press ", theme.style_dim()),
                Span::styled("Ctrl+T", theme.style_accent()),
                Span::styled(" to cycle themes", theme.style_dim()),
            ]));
            lines.push(Line::from(vec![
                Span::styled("    • Press ", theme.style_dim()),
                Span::styled("Ctrl+A", theme.style_accent()),
                Span::styled(" to toggle AI", theme.style_dim()),
            ]));
            lines.push(Line::from(""));
            lines.push(Line::from(vec![
                Span::styled("  Press ", theme.style_dim()),
                Span::styled("Esc", theme.style_accent()),
                Span::styled(" or ", theme.style_dim()),
                Span::styled("Ctrl+S", theme.style_accent()),
                Span::styled(" to close", theme.style_dim()),
            ]));
        }

        let paragraph = Paragraph::new(lines).wrap(Wrap { trim: false });
        frame.render_widget(paragraph, inner);
    }

    /// Add a field line with selection indicator
    fn add_field_line<'a>(
        &self,
        lines: &mut Vec<Line<'a>>,
        field: SettingField,
        label: &str,
        value: &'a str,
        theme: &Theme,
    ) {
        let is_selected = self.selected_field == field;
        let is_editing = self.editing && is_selected;

        let selector = if is_selected { "  ▶ " } else { "    " };

        let mut spans = vec![
            Span::styled(selector, theme.style_accent()),
            Span::styled(format!("{}: ", label), theme.style_dim()),
        ];

        if is_editing {
            spans.push(Span::styled(value, theme.style_accent()));
            spans.push(Span::styled("█", theme.style_accent()));
        } else if is_selected {
            spans.push(Span::styled(value, theme.style_success()));
        } else {
            spans.push(Span::styled(value, theme.style_normal()));
        }

        lines.push(Line::from(spans));
    }
}

impl Default for SettingsPanel {
    fn default() -> Self {
        Self::new()
    }
}
