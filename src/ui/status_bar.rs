use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::app::state::AppState;

pub fn draw_status_bar(f: &mut Frame, area: Rect, state: &AppState) {
    let palette = state.palette();

    let spans = if let Some(ref msg) = state.status_message {
        let color = if state.status_is_error {
            palette.error
        } else {
            palette.success
        };
        vec![Span::styled(msg.clone(), Style::default().fg(color))]
    } else {
        vec![
            Span::styled("Tab/l", Style::default().fg(palette.info)),
            Span::styled(": next  ", Style::default().fg(palette.fg)),
            Span::styled("S-Tab/h", Style::default().fg(palette.info)),
            Span::styled(": prev  ", Style::default().fg(palette.fg)),
            Span::styled("j/k", Style::default().fg(palette.info)),
            Span::styled(": navigate  ", Style::default().fg(palette.fg)),
            Span::styled("[/]", Style::default().fg(palette.info)),
            Span::styled(": prev/next day  ", Style::default().fg(palette.fg)),
            Span::styled("o", Style::default().fg(palette.info)),
            Span::styled(": detail  ", Style::default().fg(palette.fg)),
            Span::styled("r", Style::default().fg(palette.info)),
            Span::styled(": refresh  ", Style::default().fg(palette.fg)),
            Span::styled("t/T", Style::default().fg(palette.info)),
            Span::styled(": theme  ", Style::default().fg(palette.fg)),
            Span::styled("q", Style::default().fg(palette.info)),
            Span::styled(": quit", Style::default().fg(palette.fg)),
        ]
    };

    let paragraph = Paragraph::new(Line::from(spans));

    f.render_widget(paragraph, area);
}
