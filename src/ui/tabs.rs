use ratatui::prelude::*;
use ratatui::widgets::Tabs;

use crate::app::state::AppState;

pub fn draw_tabs(buf: &mut Buffer, area: Rect, state: &AppState) {
    let palette = state.palette();

    let titles: Vec<Line> = state
        .accounts
        .iter()
        .map(|acc| {
            let balance = format_balance(&acc.balance.value);
            Line::from(format!(" {} {} ", acc.display_name, balance))
        })
        .collect();

    let tabs = Tabs::new(titles)
        .select(state.active_tab)
        .style(Style::default().fg(palette.muted))
        .highlight_style(
            Style::default()
                .fg(palette.accent)
                .add_modifier(Modifier::BOLD),
        )
        .divider(Span::raw("│"));

    tabs.render(area, buf);
}

fn format_balance(value: &str) -> String {
    if let Ok(v) = value.parse::<f64>() {
        format!("${:.2}", v)
    } else {
        format!("${}", value)
    }
}
