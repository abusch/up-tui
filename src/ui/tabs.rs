use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Tabs};

use crate::app::state::AppState;

pub fn draw_tabs(f: &mut Frame, area: Rect, state: &AppState) {
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
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Accounts ")
                .style(Style::default().fg(palette.fg).bg(palette.bg)),
        )
        .select(state.active_tab)
        .style(Style::default().fg(palette.muted))
        .highlight_style(
            Style::default()
                .fg(palette.accent)
                .add_modifier(Modifier::BOLD),
        )
        .divider(Span::raw("│"));

    f.render_widget(tabs, area);
}

fn format_balance(value: &str) -> String {
    if let Ok(v) = value.parse::<f64>() {
        format!("${:.2}", v)
    } else {
        format!("${}", value)
    }
}
