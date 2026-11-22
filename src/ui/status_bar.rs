use opaline::names::tokens;
use ratatui::prelude::*;
use ratatui::widgets::Paragraph;

use crate::app::state::{AppMode, AppState};

/// Key hints in normal mode (and behind the theme picker, which shows its own).
const NORMAL_HINTS: &[(&str, &str)] = &[
    ("h/l", "account"),
    ("j/k", "move"),
    ("[/]", "day"),
    ("Enter", "open"),
    ("o", "detail"),
    ("s", "sidebar"),
    ("r", "refresh"),
    ("t", "theme"),
    ("q", "quit"),
];

/// Key hints while the detail popup is open.
const DETAIL_HINTS: &[(&str, &str)] = &[
    ("j/k", "prev/next transaction"),
    ("[/]", "prev/next day"),
    ("Esc/Enter/q", "close"),
];

pub fn draw_status_bar(buf: &mut Buffer, area: Rect, state: &AppState) {
    let info: Style = state.theme.color(tokens::INFO).into();
    let fg: Style = state.theme.color(tokens::TEXT_DIM).into();

    let hints = match state.mode {
        AppMode::Detail => DETAIL_HINTS,
        AppMode::Normal | AppMode::Theme(_) => NORMAL_HINTS,
    };
    let spans: Vec<Span> = hints
        .iter()
        .enumerate()
        .flat_map(|(i, (key, action))| {
            let sep = if i + 1 < hints.len() { "  " } else { "" };
            [
                Span::styled(*key, info),
                Span::styled(format!(": {action}{sep}"), fg),
            ]
        })
        .collect();

    Paragraph::new(Line::from(spans)).render(area, buf);
}
