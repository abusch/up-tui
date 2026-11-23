//! The help screen: a popup listing all the key bindings, opened with `?`. The status
//! bar only hints at the most useful ones.

use opaline::Theme;
use opaline::names::tokens;
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, Padding, Paragraph};
use unicode_width::UnicodeWidthStr;

/// The key bindings, by section: a heading, and the keys and what they do. Keep this
/// in sync with the README.
const SECTIONS: &[(&str, &[(&str, &str)])] = &[
    (
        "General",
        &[
            ("l / Tab", "Next account"),
            ("h / Shift+Tab", "Previous account"),
            ("i", "Switch between transactions and Insights"),
            ("p", "Switch between this month and the last 30 days"),
            ("a", "Toggle the account sidebar"),
            ("r", "Refresh accounts and transactions"),
            ("t", "Open the theme picker"),
            ("?", "Show or hide this help"),
            ("q / Ctrl+C", "Quit"),
        ],
    ),
    (
        "Transactions",
        &[
            ("j / Down", "Next transaction"),
            ("k / Up", "Previous transaction"),
            ("Ctrl+D / Ctrl+U", "Page down / up"),
            ("g / G", "Jump to the top / bottom"),
            ("[ / ]", "Jump to the day above / below"),
            ("Enter", "Toggle the detail pane"),
            ("Esc", "Back to Insights, from a category's transactions"),
        ],
    ),
    (
        "Insights",
        &[
            ("j / k", "Select a category"),
            (
                "Enter",
                "Show its breakdown, or a subcategory's transactions",
            ),
            ("Esc", "Back to the categories, from a breakdown"),
        ],
    ),
    (
        "Theme picker",
        &[
            ("Up / Down", "Select a theme, previewing it"),
            ("Type", "Filter the themes"),
            ("Enter", "Apply the theme (until the app exits)"),
            ("Esc", "Close without changing the theme"),
        ],
    ),
];

/// Columns of space between the keys and their actions.
const KEY_GAP: usize = 2;

/// Draw the help popup centred in `area`, scrolled down `scroll` lines. `scroll` is
/// clamped so that the last line is at the bottom at most, as how far it can scroll
/// depends on the popup's size.
pub fn draw_help(buf: &mut Buffer, area: Rect, theme: &Theme, scroll: &mut u16) {
    let lines = help_lines(theme);
    let content_width = lines.iter().map(Line::width).max().unwrap_or(0);
    let content_height = lines.len();

    let block = Block::new()
        .title(Line::from(vec![
            Span::raw(" "),
            Span::styled(
                "Keys",
                Style::from(theme.color(tokens::TEXT_PRIMARY)).bold(),
            ),
            Span::raw(" "),
        ]))
        .borders(Borders::ALL)
        // Like the theme picker's popup.
        .border_style(theme.color(tokens::ACCENT_PRIMARY))
        .style(
            Style::default()
                .fg(theme.color(tokens::TEXT_PRIMARY).into())
                .bg(theme.color(tokens::BG_ELEVATED).into()),
        )
        .padding(Padding::symmetric(2, 1));

    // The border and padding around the content.
    let (frame_width, frame_height) = (2 + 4, 2 + 2);
    let width = to_u16(content_width + frame_width).min(area.width);
    let height = to_u16(content_height + frame_height).min(area.height);
    let popup = area.centered(Constraint::Length(width), Constraint::Length(height));

    let visible = usize::from(block.inner(popup).height);
    *scroll = (*scroll).min(to_u16(content_height.saturating_sub(visible)));

    Clear.render(popup, buf);
    Paragraph::new(lines)
        .block(block)
        .scroll((*scroll, 0))
        .render(popup, buf);
}

/// The help screen's content: each section's heading, then its keys (in a column as
/// wide as the widest key) and what they do, with a blank line between sections.
fn help_lines(theme: &Theme) -> Vec<Line<'static>> {
    let fg = |token| Style::from(theme.color(token));
    // As in the detail pane and the status bar.
    let heading = fg(tokens::TEXT_DIM).bold();
    let key = fg(tokens::INFO);
    let action = fg(tokens::TEXT_PRIMARY);

    let key_width = SECTIONS
        .iter()
        .flat_map(|(_, keys)| keys.iter().map(|(k, _)| k.width()))
        .max()
        .unwrap_or(0);

    let mut lines = Vec::new();
    for (i, (title, keys)) in SECTIONS.iter().enumerate() {
        if i > 0 {
            lines.push(Line::default());
        }
        lines.push(Line::styled(*title, heading));
        lines.extend(keys.iter().map(|(k, a)| {
            Line::from(vec![
                Span::styled(format!("{k:key_width$}{:KEY_GAP$}", ""), key),
                Span::styled(*a, action),
            ])
        }));
    }
    lines
}

fn to_u16(n: usize) -> u16 {
    u16::try_from(n).unwrap_or(u16::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render(area: Rect, scroll: &mut u16) -> Buffer {
        let mut buf = Buffer::empty(area);
        draw_help(&mut buf, area, &opaline::current(), scroll);
        buf
    }

    fn content_height() -> usize {
        help_lines(&opaline::current()).len()
    }

    #[test]
    fn the_popup_fits_its_content_if_it_can() {
        let mut scroll = 5;
        render(Rect::new(0, 0, 120, 60), &mut scroll);
        assert_eq!(scroll, 0, "there's nothing to scroll");
    }

    #[test]
    fn the_scroll_stops_at_the_last_line() {
        let mut scroll = u16::MAX;
        // Borders and padding take 4 rows.
        render(Rect::new(0, 0, 120, 20), &mut scroll);
        assert_eq!(usize::from(scroll), content_height() - 16);
    }

    #[test]
    fn keys_line_up_in_a_column() {
        let lines = help_lines(&opaline::current());
        let action_columns: Vec<usize> = lines
            .iter()
            .filter(|line| line.spans.len() == 2)
            .map(|line| line.spans[0].width())
            .collect();
        assert!(action_columns.len() > 10);
        assert!(action_columns.iter().all(|&c| c == action_columns[0]));
    }
}
