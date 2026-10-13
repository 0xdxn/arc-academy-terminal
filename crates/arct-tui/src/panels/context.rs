//! Context panel - shows system info and learning context

use crate::icons;
use crate::theme::Theme;
use arct_core::Context;
use ratatui::{
    layout::Rect,
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem},
    Frame,
};
use std::env;

/// Context panel
pub struct ContextPanel;

impl ContextPanel {
    pub fn new() -> Self {
        Self
    }

    /// Render the context panel
    pub fn render(
        &self,
        frame: &mut Frame,
        area: Rect,
        focused: bool,
        context: &Context,
        theme: &Theme,
        ai_enabled: bool,
        analytics_summary: Option<&crate::analytics::AnalyticsSummary>,
        lesson_mode: bool,
        virtual_fs: Option<&arct_core::VirtualFileSystem>,
        user_stats: &arct_core::UserStats,
        challenge_manager: &arct_core::ChallengeManager,
    ) {
        let border_style = if focused {
            theme.style_border_focused()
        } else {
            theme.style_border()
        };

        let block = Block::default()
            .title(format!(" {}Learning Environment ", icons::globe().content))
            .title_style(theme.style_title(focused))
            .borders(Borders::ALL)
            .border_style(border_style)
            .style(theme.style_block());  // Set background for light themes

        let mut items = Vec::new();

        // System Information
        items.push(ListItem::new(Line::from(vec![
            icons::laptop(),
            Span::styled("System Info", theme.style_header()),
        ])));
        items.push(ListItem::new(Line::from("")));

        // Operating System
        let os_info = if cfg!(target_os = "linux") {
            "Linux"
        } else if cfg!(target_os = "macos") {
            "macOS"
        } else if cfg!(target_os = "windows") {
            "Windows"
        } else {
            "Unknown"
        };
        items.push(ListItem::new(Line::from(vec![
            Span::raw("  OS: "),
            Span::styled(os_info, theme.style_accent()),
        ])));

        // Shell
        let shell = env::var("SHELL")
            .unwrap_or_else(|_| "Unknown".to_string())
            .split('/')
            .last()
            .unwrap_or("Unknown")
            .to_string();
        items.push(ListItem::new(Line::from(vec![
            Span::raw("  Shell: "),
            Span::styled(shell, theme.style_accent()),
        ])));

        // User
        let user = env::var("USER")
            .or_else(|_| env::var("USERNAME"))
            .unwrap_or_else(|_| "Unknown".to_string());
        items.push(ListItem::new(Line::from(vec![
            Span::raw("  User: "),
            Span::styled(user, theme.style_accent()),
        ])));

        // Current directory (shortened)
        let cwd = context.working_directory.to_string_lossy().to_string();
        let short_cwd = if cwd.len() > 25 {
            format!("...{}", &cwd[cwd.len() - 22..])
        } else {
            cwd
        };
        items.push(ListItem::new(Line::from(vec![
            Span::raw("  Dir: "),
            Span::styled(short_cwd, theme.style_info()),
        ])));

        items.push(ListItem::new(Line::from("")));

        // Directory Tree section (only in lesson mode)
        if lesson_mode {
            if let Some(vfs) = virtual_fs {
                items.push(ListItem::new(Line::from(vec![
                    icons::folder(),
                    Span::styled("Virtual Filesystem", theme.style_header()),
                ])));
                items.push(ListItem::new(Line::from("")));

                // Get the directory tree
                let tree = vfs.get_tree(3);  // Max depth of 3 levels
                let current_dir = vfs.get_current_dir().to_string_lossy().to_string();

                // Add current directory indicator
                items.push(ListItem::new(Line::from(vec![
                    Span::styled("  ", theme.style_normal()),
                    icons::location(),
                    Span::styled(current_dir, theme.style_accent()),
                ])));
                items.push(ListItem::new(Line::from("")));

                // Render tree
                items.push(ListItem::new(Line::from(vec![
                    Span::styled("  lesson-home/", theme.style_success()),
                ])));

                // Helper function to render tree nodes
                fn render_tree_items(
                    nodes: &[arct_core::TreeNode],
                    items: &mut Vec<ListItem>,
                    theme: &Theme,
                    _prefix: &str,
                    is_last_at_level: &[bool],
                ) {
                    for (i, node) in nodes.iter().enumerate() {
                        let is_last = i == nodes.len() - 1;

                        // Build the tree line prefix
                        let mut line_prefix = String::from("  ");
                        for &is_last_parent in is_last_at_level {
                            if is_last_parent {
                                line_prefix.push_str("    ");
                            } else {
                                line_prefix.push_str("│   ");
                            }
                        }

                        // Add the branch
                        if is_last {
                            line_prefix.push_str("└── ");
                        } else {
                            line_prefix.push_str("├── ");
                        }

                        // Add the node
                        let icon = if node.is_dir { icons::folder_glyph() } else { icons::file_glyph() };
                        let name_style = if node.is_current {
                            theme.style_accent()
                        } else if node.is_dir {
                            theme.style_info()
                        } else {
                            theme.style_dim()
                        };

                        let mut name = node.name.clone();
                        if node.is_dir {
                            name.push('/');
                        }
                        if node.is_current {
                            name.push_str(" ←");
                        }

                        items.push(ListItem::new(Line::from(vec![
                            Span::styled(line_prefix.clone(), theme.style_dim()),
                            Span::styled(icon, theme.style_normal()),
                            Span::raw(" "),
                            Span::styled(name, name_style),
                        ])));

                        // Recurse for children
                        if !node.children.is_empty() {
                            let mut new_is_last = is_last_at_level.to_vec();
                            new_is_last.push(is_last);
                            render_tree_items(&node.children, items, theme, "", &new_is_last);
                        }
                    }
                }

                render_tree_items(&tree, &mut items, theme, "", &[]);

                items.push(ListItem::new(Line::from("")));
                items.push(ListItem::new(Line::from(vec![
                    Span::styled("  ", theme.style_normal()),
                    icons::hint(),
                    Span::styled("Use cd, ls, pwd to navigate", theme.style_dim()),
                ])));
                items.push(ListItem::new(Line::from("")));
            }
        }

        // AI Assistant section
        items.push(ListItem::new(Line::from(vec![
            icons::ai(),
            Span::styled("AI Assistant", theme.style_header()),
        ])));
        items.push(ListItem::new(Line::from("")));

        if ai_enabled {
            items.push(ListItem::new(Line::from(vec![
                Span::styled("  Status: ", theme.style_normal()),
                Span::styled("Active", theme.style_success()),
            ])));
            items.push(ListItem::new(Line::from("")));
            items.push(ListItem::new(Line::from(vec![
                Span::styled("  Press ", theme.style_dim()),
                Span::styled("Ctrl+A", theme.style_accent()),
                Span::styled(" to ask questions", theme.style_dim()),
            ])));
            items.push(ListItem::new(Line::from(vec![
                Span::styled("  Get help with commands,", theme.style_dim()),
            ])));
            items.push(ListItem::new(Line::from(vec![
                Span::styled("  learn new skills, and more!", theme.style_dim()),
            ])));
        } else {
            items.push(ListItem::new(Line::from(vec![
                Span::styled("  Status: ", theme.style_normal()),
                Span::styled("Disabled", theme.style_dim()),
            ])));
            items.push(ListItem::new(Line::from("")));
            items.push(ListItem::new(Line::from(vec![
                Span::styled("  Press ", theme.style_dim()),
                Span::styled("Ctrl+S", theme.style_accent()),
                Span::styled(" to enable AI", theme.style_dim()),
            ])));
            items.push(ListItem::new(Line::from(vec![
                Span::styled("  in settings", theme.style_dim()),
            ])));
        }

        items.push(ListItem::new(Line::from("")));

        // Gamification Stats section
        items.push(ListItem::new(Line::from(vec![
            icons::celebration(),
            Span::styled("Your Progress", theme.style_header()),
        ])));
        items.push(ListItem::new(Line::from("")));

        // Learning days (rest days allowed; see streak.rs)
        items.push(ListItem::new(Line::from(vec![
            Span::raw("  "),
            Span::styled(
                crate::streak::short(user_stats.streak_status(arct_core::local_today())),
                theme.style_accent(),
            ),
        ])));

        // Lessons completed
        items.push(ListItem::new(Line::from(vec![
            Span::raw("  "),
            icons::lesson(),
            Span::raw(" Lessons: "),
            Span::styled(format!("{}", user_stats.lessons_completed.len()), theme.style_success()),
        ])));

        // Achievements unlocked
        items.push(ListItem::new(Line::from(vec![
            Span::raw("  "),
            icons::target(),
            Span::raw(" Achievements: "),
            Span::styled(format!("{}", user_stats.achievements.total_unlocked()), theme.style_info()),
        ])));

        // Today's challenge status
        let daily_status = if challenge_manager.is_daily_completed() {
            Span::styled("Complete!", theme.style_success())
        } else {
            Span::styled("Pending", theme.style_dim())
        };
        items.push(ListItem::new(Line::from(vec![
            Span::raw("  "),
            icons::lightning(),
            Span::raw(" Today's Challenge: "),
            daily_status,
        ])));

        items.push(ListItem::new(Line::from("")));

        // Hints for gamification features
        items.push(ListItem::new(Line::from(vec![
            Span::styled("  Press ", theme.style_dim()),
            Span::styled("Alt+A", theme.style_accent()),
            Span::styled(" for achievements", theme.style_dim()),
        ])));
        items.push(ListItem::new(Line::from(vec![
            Span::styled("  Press ", theme.style_dim()),
            Span::styled("Alt+P", theme.style_accent()),
            Span::styled(" for progress", theme.style_dim()),
        ])));
        items.push(ListItem::new(Line::from(vec![
            Span::styled("  Press ", theme.style_dim()),
            Span::styled("Alt+C", theme.style_accent()),
            Span::styled(" for challenges", theme.style_dim()),
        ])));

        items.push(ListItem::new(Line::from("")));

        // Learning Progress section (from analytics)
        if let Some(summary) = analytics_summary {
            items.push(ListItem::new(Line::from(vec![
                icons::target(),
                Span::styled("Learning Progress", theme.style_header()),
            ])));
            items.push(ListItem::new(Line::from("")));

            // Skill level
            items.push(ListItem::new(Line::from(vec![
                Span::styled("  Level: ", theme.style_normal()),
                Span::styled(&summary.skill_level, theme.style_accent()),
            ])));

            // Commands today
            items.push(ListItem::new(Line::from(vec![
                Span::styled("  Today: ", theme.style_normal()),
                Span::styled(format!("{} commands", summary.commands_today), theme.style_info()),
            ])));

            // Total commands
            items.push(ListItem::new(Line::from(vec![
                Span::styled("  Total: ", theme.style_normal()),
                Span::styled(format!("{} commands", summary.total_commands), theme.style_dim()),
            ])));

            // Unique commands learned
            items.push(ListItem::new(Line::from(vec![
                Span::styled("  Learned: ", theme.style_normal()),
                Span::styled(format!("{} unique", summary.unique_commands), theme.style_success()),
            ])));

            // Top commands (if any)
            if !summary.top_commands.is_empty() {
                items.push(ListItem::new(Line::from("")));
                items.push(ListItem::new(Line::from(vec![
                    Span::styled("  Most used:", theme.style_dim()),
                ])));
                for (cmd, count) in summary.top_commands.iter().take(3) {
                    let short_cmd = if cmd.len() > 12 {
                        format!("{}...", &cmd[..9])
                    } else {
                        cmd.clone()
                    };
                    items.push(ListItem::new(Line::from(vec![
                        Span::styled("    ", theme.style_normal()),
                        Span::styled(short_cmd, theme.style_accent()),
                        Span::styled(format!(" ({})", count), theme.style_dim()),
                    ])));
                }
            }

            items.push(ListItem::new(Line::from("")));
        }

        // Session info
        items.push(ListItem::new(Line::from(vec![
            Span::styled("Session", theme.style_header()),
        ])));
        items.push(ListItem::new(Line::from("")));

        // Show session file location
        if let Ok(path) = crate::persistence::get_session_file_path() {
            let path_str = path.to_string_lossy();
            let short_path = if path_str.len() > 22 {
                format!("...{}", &path_str[path_str.len() - 19..])
            } else {
                path_str.to_string()
            };
            items.push(ListItem::new(Line::from(vec![
                Span::styled("  ", theme.style_normal()),
                icons::folder(),
                Span::styled(short_path, theme.style_dim()),
            ])));
        }

        items.push(ListItem::new(Line::from("")));

        // Quick tips
        items.push(ListItem::new(Line::from(vec![
            icons::hint(),
            Span::styled("Quick Tips", theme.style_header()),
        ])));
        items.push(ListItem::new(Line::from("")));
        items.push(ListItem::new(Line::from(vec![
            Span::styled("  • Run commands to learn!", theme.style_success()),
        ])));
        items.push(ListItem::new(Line::from(vec![
            Span::styled("  • Tab to scroll output", theme.style_info()),
        ])));
        items.push(ListItem::new(Line::from(vec![
            Span::styled("  • Press ? for help", theme.style_warning()),
        ])));

        // Add spacing before footer
        let remaining_lines = area.height.saturating_sub(items.len() as u16 + 3);
        for _ in 0..remaining_lines {
            items.push(ListItem::new(Line::from("")));
        }

        // Footer with website
        items.push(ListItem::new(Line::from("")));
        items.push(ListItem::new(Line::from(vec![
            Span::styled("━━━━━━━━━━━━━━━", theme.style_dim()),
        ])));
        items.push(ListItem::new(Line::from(vec![
            Span::styled("  ", theme.style_accent()),
            Span::styled("arcacademy.sh", theme.style_accent()),
        ])));

        let list = List::new(items).block(block);

        frame.render_widget(list, area);
    }
}

impl Default for ContextPanel {
    fn default() -> Self {
        Self::new()
    }
}
