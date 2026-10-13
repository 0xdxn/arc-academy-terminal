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

    /// The explanation of a command, as lines (wrapped when rendered)
    pub fn lines(exp: &Explanation, theme: &Theme) -> Vec<Line<'static>> {
        let mut lines = vec![Line::from(vec![
            icons::learning(),
            Span::styled(exp.summary.clone(), theme.style_accent()),
        ])];

        // Detailed description (plain English explanation)
        if !exp.description.is_empty() {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(exp.description.clone(), theme.style_normal())));
        }

        if !exp.flag_explanations.is_empty() {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled("Flags:", theme.style_header())));
            for flag_exp in &exp.flag_explanations {
                lines.push(Line::from(vec![
                    Span::raw("  "),
                    Span::styled(flag_exp.flag.clone(), theme.style_accent()),
                    Span::raw(" → "),
                    Span::styled(flag_exp.description.clone(), theme.style_normal()),
                ]));
            }
        }

        if !exp.warnings.is_empty() {
            lines.push(Line::from(""));
            lines.push(Line::from(vec![
                icons::warning(),
                Span::styled("Warnings:", theme.style_warning()),
            ]));
            for warning in &exp.warnings {
                lines.push(Line::from(vec![
                    Span::raw("  • "),
                    Span::styled(warning.message.clone(), theme.style_error()),
                ]));
            }
        }

        if !exp.tips.is_empty() {
            lines.push(Line::from(""));
            lines.push(Line::from(vec![icons::hint(), Span::styled("Tips:", theme.style_header())]));
            for tip in &exp.tips {
                lines.push(Line::from(vec![
                    Span::raw("  "),
                    Span::styled(tip.title.clone(), theme.style_accent()),
                ]));
                lines.push(Line::from(vec![
                    Span::raw("    "),
                    Span::styled(tip.content.clone(), theme.style_dim()),
                ]));
            }
        }

        if !exp.related_commands.is_empty() {
            lines.push(Line::from(""));
            lines.push(Line::from(vec![
                Span::styled("Related: ", theme.style_secondary()),
                Span::styled(exp.related_commands.join(", "), theme.style_dim()),
            ]));
        }
        lines
    }

    /// Rows (borders included) the explanation needs at `width` columns
    pub fn wanted_height(exp: &Explanation, theme: &Theme, width: u16) -> u16 {
        let rows = Paragraph::new(Self::lines(exp, theme))
            .wrap(Wrap { trim: false })
            .line_count(width.saturating_sub(2));
        (rows as u16).saturating_add(2)
    }

    /// Draw `lines` into `inner` scrolled down `scroll` rows; when they
    /// don't fit, the last row says how much more there is. Returns the
    /// furthest it can scroll.
    fn render_scrolled(
        frame: &mut Frame,
        inner: Rect,
        lines: Vec<Line<'static>>,
        scroll: usize,
        theme: &Theme,
    ) -> usize {
        if inner.height == 0 || inner.width == 0 {
            return 0;
        }
        let paragraph = Paragraph::new(lines).wrap(Wrap { trim: false });
        let total = paragraph.line_count(inner.width);
        let rows = inner.height as usize;
        if total <= rows {
            frame.render_widget(paragraph, inner);
            return 0;
        }
        let view = rows.saturating_sub(1).max(1);
        let max_scroll = total - view;
        let scroll = scroll.min(max_scroll);
        let below = total - view - scroll;
        frame.render_widget(
            paragraph.scroll((scroll as u16, 0)),
            Rect { height: view as u16, ..inner },
        );
        let cue = if below > 0 {
            format!(
                "▼ {below} more line{} below: scroll with the wheel, or Tab to here and press ↓",
                if below == 1 { "" } else { "s" }
            )
        } else {
            "▲ more above: scroll up to reread".to_string()
        };
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(cue, theme.style_accent()))),
            Rect { y: inner.y + view as u16, height: 1, ..inner },
        );
        max_scroll
    }

    /// Render the explanation panel. `scroll` moves a long explanation
    /// down; returns the furthest it can scroll.
    #[allow(clippy::too_many_arguments)]
    pub fn render(
        &self,
        frame: &mut Frame,
        area: Rect,
        focused: bool,
        explanation: Option<&Explanation>,
        theme: &Theme,
        ai_mode: bool,
        ai_response: Option<&str>,
        scroll: usize,
    ) -> usize {
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
            let inner = block.inner(area);
            frame.render_widget(block, area);
            return Self::render_scrolled(frame, inner, Self::lines(exp, theme), scroll, theme);
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
        0
    }
}

impl Default for ExplanationPanel {
    fn default() -> Self {
        Self::new()
    }
}
