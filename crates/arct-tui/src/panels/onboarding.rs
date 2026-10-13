//! Onboarding wizard for first-run setup

use crate::theme::Theme;
use ratatui::{
    layout::Rect,
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
    Frame,
};

/// Onboarding wizard step
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OnboardingStep {
    Welcome,
    AskName,
    AskAI,
    AskAIProvider,
    Complete,
}

impl OnboardingStep {
    pub fn next(self) -> Self {
        match self {
            Self::Welcome => Self::AskName,
            Self::AskName => Self::AskAI,
            Self::AskAI => Self::AskAIProvider,
            Self::AskAIProvider => Self::Complete,
            Self::Complete => Self::Complete,
        }
    }

    pub fn previous(self) -> Self {
        match self {
            Self::Welcome => Self::Welcome,
            Self::AskName => Self::Welcome,
            Self::AskAI => Self::AskName,
            Self::AskAIProvider => Self::AskAI,
            Self::Complete => Self::AskAIProvider,
        }
    }
}

/// Onboarding wizard state
pub struct OnboardingWizard {
    pub step: OnboardingStep,
    pub user_name: String,
    pub ai_enabled: Option<bool>,
    pub ai_provider: Option<String>,
    pub selected_option: usize,
}

impl OnboardingWizard {
    pub fn new() -> Self {
        Self {
            step: OnboardingStep::Welcome,
            user_name: String::new(),
            ai_enabled: None,
            ai_provider: None,
            selected_option: 0,
        }
    }

    pub fn handle_char(&mut self, c: char) {
        if self.step == OnboardingStep::AskName {
            self.user_name.push(c);
        }
    }

    pub fn handle_backspace(&mut self) {
        if self.step == OnboardingStep::AskName {
            self.user_name.pop();
        }
    }

    pub fn handle_up(&mut self) {
        if self.selected_option > 0 {
            self.selected_option -= 1;
        }
    }

    pub fn handle_down(&mut self, max_options: usize) {
        if self.selected_option < max_options - 1 {
            self.selected_option += 1;
        }
    }

    /// Esc: go back one step (the first screen stays put)
    pub fn handle_back(&mut self) {
        if self.step != OnboardingStep::Welcome && self.step != OnboardingStep::Complete {
            self.step = self.step.previous();
            self.selected_option = 0;
        }
    }

    pub fn handle_enter(&mut self) {
        match self.step {
            OnboardingStep::Welcome => {
                self.step = self.step.next();
            }
            OnboardingStep::AskName => {
                // The name is optional and never leaves this computer
                self.user_name = self.user_name.trim().to_string();
                self.step = self.step.next();
                self.selected_option = 0;
            }
            OnboardingStep::AskAI => {
                // 0 = No (default), 1 = Yes
                match self.selected_option {
                    1 => {
                        self.ai_enabled = Some(true);
                        self.step = self.step.next();
                        self.selected_option = 0;
                    }
                    _ => {
                        self.ai_enabled = Some(false);
                        self.step = OnboardingStep::Complete;
                    }
                }
            }
            OnboardingStep::AskAIProvider => {
                // 0 = local model (Ollama), 1 = Claude Code CLI, 2 = own API key
                self.ai_provider = Some(
                    match self.selected_option {
                        1 => "claude-code",
                        2 => "own-key",
                        _ => "local",
                    }
                    .to_string(),
                );
                self.step = OnboardingStep::Complete;
            }
            OnboardingStep::Complete => {}
        }
    }

    /// Render the onboarding wizard as a centered modal
    pub fn render(&self, frame: &mut Frame, theme: &Theme) {
        let area = frame.size();

        // Create centered modal
        let modal_width = 70.min(area.width - 4);
        let modal_height = 20.min(area.height - 4);

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

        let block = Block::default()
            .title(" Arc Academy Terminal - Setup ")
            .borders(Borders::ALL)
            .border_style(theme.style_border_focused())
            .style(theme.style_block());  // Set background for light themes

        let inner = block.inner(modal_area);
        frame.render_widget(block, modal_area);

        // Render content based on step
        match self.step {
            OnboardingStep::Welcome => self.render_welcome(frame, inner, theme),
            OnboardingStep::AskName => self.render_ask_name(frame, inner, theme),
            OnboardingStep::AskAI => self.render_ask_ai(frame, inner, theme),
            OnboardingStep::AskAIProvider => self.render_ask_provider(frame, inner, theme),
            OnboardingStep::Complete => self.render_complete(frame, inner, theme),
        }
    }

    fn render_welcome(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let lines = vec![
            Line::from(""),
            Line::from(vec![
                Span::styled("  Welcome to ", theme.style_normal()),
                Span::styled("Arc Academy Terminal", theme.style_accent()),
                Span::styled("!", theme.style_normal()),
            ]),
            Line::from(""),
            Line::from(vec![
                Span::styled("  Learn shell commands interactively with AI-powered", theme.style_normal()),
            ]),
            Line::from(vec![
                Span::styled("  explanations and real-time help.", theme.style_normal()),
            ]),
            Line::from(""),
            Line::from(""),
            Line::from(vec![
                Span::styled("  Let's get you set up in just a few steps.", theme.style_dim()),
            ]),
            Line::from(""),
            Line::from(""),
            Line::from(""),
            Line::from(vec![
                Span::styled("  Press ", theme.style_dim()),
                Span::styled("Enter", theme.style_accent()),
                Span::styled(" to continue", theme.style_dim()),
            ]),
        ];

        let paragraph = Paragraph::new(lines).wrap(Wrap { trim: false });
        frame.render_widget(paragraph, area);
    }

    fn render_ask_name(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let lines = vec![
            Line::from(""),
            Line::from(vec![
                Span::styled("  What should arct call you?", theme.style_accent()),
            ]),
            Line::from(""),
            Line::from(vec![
                Span::styled("  A first name or a nickname. Optional; it stays on this computer.", theme.style_dim()),
            ]),
            Line::from(""),
            Line::from(""),
            Line::from(vec![
                Span::styled("  Name: ", theme.style_normal()),
                Span::styled(&self.user_name, theme.style_accent()),
                Span::styled("█", theme.style_accent()),
            ]),
            Line::from(""),
            Line::from(""),
            Line::from(""),
            Line::from(vec![
                Span::styled("  Press ", theme.style_dim()),
                Span::styled("Enter", theme.style_accent()),
                Span::styled(" to continue, ", theme.style_dim()),
                Span::styled("Esc", theme.style_accent()),
                Span::styled(" to go back", theme.style_dim()),
            ]),
        ];

        let paragraph = Paragraph::new(lines).wrap(Wrap { trim: false });
        frame.render_widget(paragraph, area);
    }

    fn render_ask_ai(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let options = vec![
            ("No AI, thanks", 0),
            ("Yes, I'll set up an AI assistant", 1),
        ];

        let mut lines = vec![
            Line::from(""),
            Line::from(vec![
                Span::styled("  Would you like to enable the AI assistant?", theme.style_accent()),
            ]),
            Line::from(""),
            Line::from(vec![
                Span::styled("  arct works fully without AI. If you turn it on, your questions", theme.style_dim()),
            ]),
            Line::from(vec![
                Span::styled("  go to the AI you choose. Kids: ask a grown-up first.", theme.style_dim()),
            ]),
            Line::from(vec![
                Span::styled("  You can change this any time in settings (Ctrl+S).", theme.style_dim()),
            ]),
            Line::from(""),
            Line::from(""),
        ];

        for (i, (option, _)) in options.iter().enumerate() {
            let prefix = if i == self.selected_option {
                Span::styled("  ▶ ", theme.style_accent())
            } else {
                Span::styled("    ", theme.style_dim())
            };

            let style = if i == self.selected_option {
                theme.style_accent()
            } else {
                theme.style_normal()
            };

            lines.push(Line::from(vec![prefix, Span::styled(*option, style)]));
        }

        lines.push(Line::from(""));
        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled("  Use ", theme.style_dim()),
            Span::styled("↑/↓", theme.style_accent()),
            Span::styled(" to select, ", theme.style_dim()),
            Span::styled("Enter", theme.style_accent()),
            Span::styled(" to continue", theme.style_dim()),
        ]));

        let paragraph = Paragraph::new(lines).wrap(Wrap { trim: false });
        frame.render_widget(paragraph, area);
    }

    fn render_ask_provider(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let options = vec![
            ("A local model with Ollama (stays on this computer)", 0),
            ("Claude Code, if it's installed and signed in", 1),
            ("My own API key (Anthropic or OpenAI)", 2),
        ];

        let mut lines = vec![
            Line::from(""),
            Line::from(vec![
                Span::styled("  How would you like to use AI?", theme.style_accent()),
            ]),
            Line::from(""),
            Line::from(""),
        ];

        for (i, (option, _)) in options.iter().enumerate() {
            let prefix = if i == self.selected_option {
                Span::styled("  ▶ ", theme.style_accent())
            } else {
                Span::styled("    ", theme.style_dim())
            };

            let style = if i == self.selected_option {
                theme.style_accent()
            } else {
                theme.style_normal()
            };

            lines.push(Line::from(vec![prefix, Span::styled(*option, style)]));
        }

        lines.push(Line::from(""));
        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled("  Only the local model keeps your questions on this computer.", theme.style_dim()),
        ]));
        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled("  Use ", theme.style_dim()),
            Span::styled("↑/↓", theme.style_accent()),
            Span::styled(" to select, ", theme.style_dim()),
            Span::styled("Enter", theme.style_accent()),
            Span::styled(" to continue", theme.style_dim()),
        ]));

        let paragraph = Paragraph::new(lines).wrap(Wrap { trim: false });
        frame.render_widget(paragraph, area);
    }

    fn render_complete(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let name = if !self.user_name.is_empty() {
            &self.user_name
        } else {
            "there"
        };

        let lines = vec![
            Line::from(""),
            Line::from(vec![
                Span::styled("  All set, ", theme.style_normal()),
                Span::styled(name, theme.style_accent()),
                Span::styled("!", theme.style_normal()),
            ]),
            Line::from(""),
            Line::from(""),
            Line::from(vec![
                Span::styled("  Quick tips:", theme.style_header()),
            ]),
            Line::from(vec![
                Span::styled("    • Press ", theme.style_dim()),
                Span::styled("?", theme.style_accent()),
                Span::styled(" for help anytime", theme.style_dim()),
            ]),
            Line::from(vec![
                Span::styled("    • Press ", theme.style_dim()),
                Span::styled("Ctrl+A", theme.style_accent()),
                Span::styled(" to toggle AI assistant", theme.style_dim()),
            ]),
            Line::from(vec![
                Span::styled("    • Press ", theme.style_dim()),
                Span::styled("Ctrl+S", theme.style_accent()),
                Span::styled(" to open settings", theme.style_dim()),
            ]),
            Line::from(vec![
                Span::styled("    • Press ", theme.style_dim()),
                Span::styled("Tab", theme.style_accent()),
                Span::styled(" to switch panels", theme.style_dim()),
            ]),
            Line::from(""),
            Line::from(""),
            Line::from(vec![
                Span::styled("  Press ", theme.style_dim()),
                Span::styled("Enter", theme.style_accent()),
                Span::styled(" to start learning!", theme.style_dim()),
            ]),
        ];

        let paragraph = Paragraph::new(lines).wrap(Wrap { trim: false });
        frame.render_widget(paragraph, area);
    }
}

impl Default for OnboardingWizard {
    fn default() -> Self {
        Self::new()
    }
}
