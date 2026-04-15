use opaline::names::tokens;
use ratatui::prelude::*;
use ratatui::widgets::Paragraph;

use crate::app::state::AppState;

pub fn draw_status_bar(buf: &mut Buffer, area: Rect, state: &AppState) {
    let info: Style = state.theme.color(tokens::INFO).into();
    let fg: Style = state.theme.color(tokens::TEXT_DIM).into();

    let spans = vec![
        Span::styled("Tab/l", info),
        // Span::styled("Tab/l", Style::default().fg(info)),
        Span::styled(": next  ", fg),
        Span::styled("S-Tab/h", info),
        Span::styled(": prev  ", fg),
        Span::styled("j/k", info),
        Span::styled(": navigate  ", fg),
        Span::styled("[/]", info),
        Span::styled(": prev/next day  ", fg),
        Span::styled("o", info),
        Span::styled(": detail  ", fg),
        Span::styled("r", info),
        Span::styled(": refresh  ", fg),
        Span::styled("t", info),
        Span::styled(": theme  ", fg),
        Span::styled("q", info),
        Span::styled(": quit", fg),
    ];

    let paragraph = Paragraph::new(Line::from(spans));

    paragraph.render(area, buf);
}
