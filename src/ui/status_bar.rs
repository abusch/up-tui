use opaline::names::tokens;
use ratatui::prelude::*;
use ratatui::widgets::{Block, Padding, Paragraph};

use crate::app::state::{AppState, View};

/// Key hints in normal mode (and behind the theme picker, which shows its own).
const NORMAL_HINTS: &[(&str, &str)] = &[
    ("h/l", "account"),
    ("j/k", "move"),
    ("[/]", "day"),
    ("Enter", "detail"),
    ("i", "insights"),
    ("a", "sidebar"),
    ("r", "refresh"),
    ("t", "theme"),
    ("q", "quit"),
];

/// Key hints in a filtered transaction list, opened from the Insights view.
const FILTERED_HINTS: &[(&str, &str)] = &[
    ("Esc", "back to insights"),
    ("h/l", "account"),
    ("j/k", "move"),
    ("[/]", "day"),
    ("Enter", "detail"),
    ("r", "refresh"),
    ("q", "quit"),
];

/// Key hints in the Insights view, listing spending by root category.
const INSIGHTS_HINTS: &[(&str, &str)] = &[
    ("h/l", "account"),
    ("j/k", "category"),
    ("Enter", "breakdown"),
    ("i", "transactions"),
    ("a", "sidebar"),
    ("r", "refresh"),
    ("t", "theme"),
    ("q", "quit"),
];

/// Key hints in the Insights view, showing a root category's breakdown.
const BREAKDOWN_HINTS: &[(&str, &str)] = &[
    ("h/l", "account"),
    ("j/k", "subcategory"),
    ("Enter", "transactions"),
    ("Esc", "back"),
    ("i", "transactions"),
    ("a", "sidebar"),
    ("r", "refresh"),
    ("t", "theme"),
    ("q", "quit"),
];

pub fn draw_status_bar(buf: &mut Buffer, area: Rect, state: &AppState) {
    let info: Style = state.theme.color(tokens::INFO).into();
    let fg: Style = state.theme.color(tokens::TEXT_DIM).into();

    let hints = match state.view {
        View::Transactions if state.filter.is_some() => FILTERED_HINTS,
        View::Transactions => NORMAL_HINTS,
        View::Insights if state.insights.breakdown => BREAKDOWN_HINTS,
        View::Insights => INSIGHTS_HINTS,
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

    // The same background as the header bar, so the two frame the screen.
    let bg = state.theme.color(tokens::BG_HIGHLIGHT);
    Paragraph::new(Line::from(spans))
        .block(Block::new().padding(Padding::horizontal(1)))
        .style(Style::default().bg(bg.into()))
        .render(area, buf);
}
