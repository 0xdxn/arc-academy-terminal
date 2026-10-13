//! Side panel: where you are, and what's around you
//!
//! Only what helps a learner: the folder commands run in, what is in it
//! (in lessons, the whole lesson world as a tree), and a one-line count
//! of progress. Hidden or shown with Ctrl+F.

use crate::theme::Theme;
use ratatui::{
    layout::Rect,
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
    Frame,
};
use std::path::Path;

/// Most folder entries listed for a real folder
const MAX_LISTED: usize = 40;

/// Side panel
pub struct ContextPanel;

/// What the side panel shows
pub struct SidebarData<'a> {
    /// The folder commands run in, as the prompt shows it
    pub folder: &'a str,
    /// The simulated lesson world, in a simulated lesson
    pub virtual_fs: Option<&'a arct_core::VirtualFileSystem>,
    /// The real folder behind `folder`, listed when there is no lesson world
    pub real_dir: Option<&'a Path>,
    /// A line or two of progress, shown at the bottom
    pub progress: Vec<String>,
}

impl ContextPanel {
    pub fn new() -> Self {
        Self
    }

    /// Render the side panel
    pub fn render(
        &self,
        frame: &mut Frame,
        area: Rect,
        focused: bool,
        theme: &Theme,
        data: &SidebarData,
    ) {
        let border_style = if focused {
            theme.style_border_focused()
        } else {
            theme.style_border()
        };
        let block = Block::default()
            .title(" Where you are ")
            .title_style(theme.style_title(focused))
            .borders(Borders::ALL)
            .border_style(border_style)
            .style(theme.style_block());
        let inner = block.inner(area);
        frame.render_widget(block, area);
        let progress_rows = data.progress.len() as u16;
        if inner.height < 3 + progress_rows || inner.width < 8 {
            return;
        }

        // Folder (wrapped, it can be long), then the progress lines at
        // the bottom; the listing gets the rows between
        let folder = Paragraph::new(Line::from(Span::styled(
            data.folder.to_string(),
            theme.style_accent(),
        )))
        .wrap(Wrap { trim: false });
        let folder_rows =
            (folder.line_count(inner.width) as u16).min(inner.height.saturating_sub(2));
        frame.render_widget(
            folder,
            Rect {
                height: folder_rows,
                ..inner
            },
        );

        let progress_y = inner.y + inner.height - progress_rows;
        frame.render_widget(
            Paragraph::new(
                data.progress
                    .iter()
                    .map(|p| Line::from(Span::styled(p.clone(), theme.style_dim())))
                    .collect::<Vec<_>>(),
            ),
            Rect {
                y: progress_y,
                height: progress_rows,
                ..inner
            },
        );

        let list_area = Rect {
            y: inner.y + folder_rows + 1,
            height: inner.height.saturating_sub(folder_rows + 2 + progress_rows),
            ..inner
        };
        if list_area.height == 0 {
            return;
        }

        let mut items = Vec::new();
        let mut current = None;
        if let Some(vfs) = data.virtual_fs {
            items.push(ListItem::new(Line::from(Span::styled(
                "lesson-home/",
                theme.style_info(),
            ))));
            render_tree_items(&vfs.get_tree(3), &mut items, theme, &mut current, &[]);
        } else if let Some(dir) = data.real_dir {
            items.push(ListItem::new(Line::from(Span::styled(
                "In this folder:",
                theme.style_header(),
            ))));
            let listing = list_folder(dir);
            if listing.is_empty() {
                items.push(ListItem::new(Line::from(Span::styled(
                    "  (empty)",
                    theme.style_dim(),
                ))));
            }
            for (name, is_dir) in listing {
                let style = if is_dir {
                    theme.style_info()
                } else {
                    theme.style_normal()
                };
                let glyph = if is_dir {
                    crate::icons::folder_glyph()
                } else {
                    crate::icons::file_glyph()
                };
                let shown = if is_dir { format!("{name}/") } else { name };
                items.push(ListItem::new(Line::from(vec![
                    Span::styled(format!("  {glyph} "), theme.style_dim()),
                    Span::styled(shown, style),
                ])));
            }
        }

        frame.render_widget(
            List::new(window(items, current, list_area.height as usize, theme)),
            list_area,
        );
    }
}

/// Fit `items` into `rows`, keeping the current folder in view and
/// saying how many rows are left out above and below
fn window<'a>(
    items: Vec<ListItem<'a>>,
    current: Option<usize>,
    rows: usize,
    theme: &Theme,
) -> Vec<ListItem<'a>> {
    let total = items.len();
    if total <= rows || rows < 3 {
        return items.into_iter().take(rows).collect();
    }
    let start = current
        .map(|c| c.saturating_sub(rows / 2))
        .unwrap_or(0)
        .min(total - rows);
    let end = start + rows;
    let mut shown: Vec<ListItem<'a>> = items.into_iter().skip(start).take(rows).collect();
    let note = |text: String| ListItem::new(Line::from(Span::styled(text, theme.style_dim())));
    if start > 0 {
        shown[0] = note(format!("  … {} above", start + 1));
    }
    if end < total {
        let last = shown.len() - 1;
        shown[last] = note(format!("  … {} more", total - end + 1));
    }
    shown
}

/// Visible names in a real folder, folders first, alphabetical
fn list_folder(dir: &Path) -> Vec<(String, bool)> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut names: Vec<(String, bool)> = entries
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            (!name.starts_with('.')).then(|| (name, e.path().is_dir()))
        })
        .take(MAX_LISTED * 4)
        .collect();
    names.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    names.truncate(MAX_LISTED);
    names
}

// Helper function to render tree nodes
fn render_tree_items(
    nodes: &[arct_core::TreeNode],
    items: &mut Vec<ListItem>,
    theme: &Theme,
    current: &mut Option<usize>,
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
        let icon = if node.is_dir {
            crate::icons::folder_glyph()
        } else {
            crate::icons::file_glyph()
        };
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
            *current = Some(items.len());
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
            render_tree_items(&node.children, items, theme, current, &new_is_last);
        }
    }
}

impl Default for ContextPanel {
    fn default() -> Self {
        Self::new()
    }
}
