use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::app::state::AppState;

pub fn draw_status_bar(f: &mut Frame, area: Rect, state: &AppState) {
    let spans = if let Some(ref msg) = state.status_message {
        let color = if state.status_is_error {
            Color::Red
        } else {
            Color::Green
        };
        vec![Span::styled(msg.clone(), Style::default().fg(color))]
    } else {
        vec![
            Span::styled("Tab/l", Style::default().fg(Color::Yellow)),
            Span::raw(": next  "),
            Span::styled("S-Tab/h", Style::default().fg(Color::Yellow)),
            Span::raw(": prev  "),
            Span::styled("j/k", Style::default().fg(Color::Yellow)),
            Span::raw(": navigate  "),
            Span::styled("Enter", Style::default().fg(Color::Yellow)),
            Span::raw(": detail  "),
            Span::styled("r", Style::default().fg(Color::Yellow)),
            Span::raw(": refresh  "),
            Span::styled("g/G", Style::default().fg(Color::Yellow)),
            Span::raw(": top/bottom  "),
            Span::styled("q", Style::default().fg(Color::Yellow)),
            Span::raw(": quit"),
        ]
    };

    let paragraph = Paragraph::new(Line::from(spans))
        .block(Block::default().borders(Borders::ALL));

    f.render_widget(paragraph, area);
}
