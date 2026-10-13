//! Explanation panel - shows command breakdowns and learning content

use crate::icons;
use crate::theme::Theme;
use arct_core::Explanation;
use ratatui::{
    layout::Rect,
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
    Frame,
};

/// Explanation panel
pub struct ExplanationPanel;

impl ExplanationPanel {
    pub fn new() -> Self {
        Self
    }

    /// Render the explanation panel
    pub fn render(
        &self,
        frame: &mut Frame,
        area: Rect,
        focused: bool,
        explanation: Option<&Explanation>,
        theme: &Theme,
        ai_mode: bool,
        ai_response: Option<&str>,
    ) {
        let border_style = if focused {
            theme.style_border_focused()
        } else {
            theme.style_border()
        };

        let title = if ai_mode {
            format!(" {}AI Assistant ", icons::ai().content)
        } else {
            format!(" {}Learning ", icons::learning().content)
        };

        let block = Block::default()
            .title(title)
            .title_style(theme.style_title(focused))
            .borders(Borders::ALL)
            .border_style(border_style)
            .style(theme.style_block());  // Set background for light themes

        if ai_mode {
            // AI mode - show AI response
            if let Some(response) = ai_response {
                let mut items = Vec::new();

                items.push(ListItem::new(Line::from(vec![
                    icons::ai(),
                    Span::styled("AI Response:", theme.style_header()),
                ])));
                items.push(ListItem::new(Line::from("")));

                // Split response into lines and render
                for line in response.lines() {
                    items.push(ListItem::new(Line::from(vec![
                        Span::styled(line, theme.style_normal()),
                    ])));
                }

                let list = List::new(items).block(block);
                frame.render_widget(list, area);
            } else {
                // No AI response yet
                let paragraph = Paragraph::new(vec![
                    Line::from(""),
                    Line::from(vec![
                        icons::ai(),
                        Span::styled("AI Assistant Active", theme.style_accent()),
                    ]),
                    Line::from(""),
                    Line::from(vec![
                        Span::styled("Ask me anything about shell commands!", theme.style_normal()),
                    ]),
                    Line::from(""),
                    Line::from(vec![
                        Span::styled("Press ", theme.style_dim()),
                        Span::styled("Ctrl+A", theme.style_accent()),
                        Span::styled(" to return to shell mode", theme.style_dim()),
                    ]),
                ])
                .block(block)
                .wrap(Wrap { trim: false });

                frame.render_widget(paragraph, area);
            }
        } else if let Some(exp) = explanation {
            let mut items = Vec::new();

            // Summary
            items.push(ListItem::new(Line::from(vec![
                icons::learning(),
                Span::styled(&exp.summary, theme.style_accent()),
            ])));
            items.push(ListItem::new(Line::from("")));

            // Detailed description (plain English explanation)
            if !exp.description.is_empty() {
                // Word wrap the description for better readability
                let max_width = (area.width.saturating_sub(4)) as usize; // Account for padding
                let words: Vec<&str> = exp.description.split_whitespace().collect();
                let mut current_line = String::new();

                for word in words {
                    if current_line.len() + word.len() + 1 > max_width {
                        if !current_line.is_empty() {
                            items.push(ListItem::new(Line::from(vec![
                                Span::styled(current_line.clone(), theme.style_normal()),
                            ])));
                            current_line.clear();
                        }
                    }
                    if !current_line.is_empty() {
                        current_line.push(' ');
                    }
                    current_line.push_str(word);
                }

                if !current_line.is_empty() {
                    items.push(ListItem::new(Line::from(vec![
                        Span::styled(current_line, theme.style_normal()),
                    ])));
                }

                items.push(ListItem::new(Line::from("")));
            }

            // Flag explanations
            if !exp.flag_explanations.is_empty() {
                items.push(ListItem::new(Line::from(vec![
                    Span::styled("", theme.style_warning()),
                    Span::styled("Flag Explanations:", theme.style_header()),
                ])));

                for flag_exp in &exp.flag_explanations {
                    items.push(ListItem::new(Line::from(vec![
                        Span::raw("  "),
                        Span::styled(&flag_exp.flag, theme.style_accent()),
                        Span::raw(" → "),
                        Span::styled(&flag_exp.description, theme.style_normal()),
                    ])));
                }
                items.push(ListItem::new(Line::from("")));
            }

            // Warnings
            if !exp.warnings.is_empty() {
                items.push(ListItem::new(Line::from(vec![
                    icons::warning(),
                    Span::styled("Warnings:", theme.style_warning()),
                ])));

                for warning in &exp.warnings {
                    items.push(ListItem::new(Line::from(vec![
                        Span::raw("  • "),
                        Span::styled(&warning.message, theme.style_error()),
                    ])));
                }
                items.push(ListItem::new(Line::from("")));
            }

            // Tips
            if !exp.tips.is_empty() {
                items.push(ListItem::new(Line::from(vec![
                    icons::hint(),
                    Span::styled("Tips:", theme.style_header()),
                ])));

                for tip in &exp.tips {
                    items.push(ListItem::new(Line::from(vec![
                        Span::raw("  "),
                        Span::styled(&tip.title, theme.style_accent()),
                    ])));
                    items.push(ListItem::new(Line::from(vec![
                        Span::raw("    "),
                        Span::styled(&tip.content, theme.style_dim()),
                    ])));
                }
                items.push(ListItem::new(Line::from("")));
            }

            // Related commands
            if !exp.related_commands.is_empty() {
                items.push(ListItem::new(Line::from(vec![
                    Span::styled("", theme.style_success()),
                    Span::styled("Related: ", theme.style_secondary()),
                    Span::styled(exp.related_commands.join(", "), theme.style_dim()),
                ])));
            }

            let list = List::new(items).block(block);
            frame.render_widget(list, area);
        } else {
            // No explanation yet
            let paragraph = Paragraph::new(vec![
                Line::from(""),
                Line::from(vec![
                    icons::welcome(),
                    Span::styled("Welcome to Arc Academy Terminal!", theme.style_accent()),
                ]),
                Line::from(""),
                Line::from(vec![
                    Span::styled("Type a command below and press ", theme.style_normal()),
                    Span::styled("Enter", theme.style_accent()),
                    Span::styled(" to see it explained.", theme.style_normal()),
                ]),
                Line::from(""),
                Line::from(vec![
                    Span::styled("Try: ", theme.style_dim()),
                    Span::styled("ls -lah", theme.style_accent()),
                ]),
                Line::from(""),
                Line::from(vec![
                    Span::styled("Press ", theme.style_dim()),
                    Span::styled("?", theme.style_accent()),
                    Span::styled(" for help", theme.style_dim()),
                ]),
            ])
            .block(block)
            .wrap(Wrap { trim: false });

            frame.render_widget(paragraph, area);
        }
    }
}

impl Default for ExplanationPanel {
    fn default() -> Self {
        Self::new()
    }
}
