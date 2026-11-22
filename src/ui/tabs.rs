use opaline::names::tokens;
use ratatui::prelude::*;
use ratatui::widgets::Tabs;

use crate::app::state::AppState;

pub fn draw_tabs(buf: &mut Buffer, area: Rect, state: &AppState) {
    let muted = state.theme.color(tokens::TEXT_MUTED).into();
    let accent = state.theme.color(tokens::ACCENT_PRIMARY).into();

    let titles: Vec<Line> = state
        .tabs
        .iter()
        .map(|tab| &tab.account)
        .map(|acc| {
            Line::from(format!(
                " {} {} ",
                acc.display_name,
                acc.balance.format_signed()
            ))
        })
        .collect();

    let tabs = Tabs::new(titles)
        .select(state.active_tab)
        .style(Style::default().fg(muted))
        .highlight_style(Style::default().fg(accent).add_modifier(Modifier::BOLD))
        .divider(Span::raw("│"));

    tabs.render(area, buf);
}
