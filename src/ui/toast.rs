use opaline::names::tokens;
use ratatui::{
    prelude::*,
    symbols,
    widgets::{Block, Borders, Clear, Padding, Paragraph},
};
use ratatui_toaster::ToastType;

/// Draw a toast in `area` (as laid out by the toast engine), in theme colours.
///
/// This replaces ratatui-toaster's own rendering, whose colours are hardcoded.
pub fn draw_toast(
    buf: &mut Buffer,
    area: Rect,
    message: &str,
    typ: ToastType,
    theme: &opaline::Theme,
) {
    let accent = theme.color(match typ {
        ToastType::Info => tokens::INFO,
        ToastType::Success => tokens::SUCCESS,
        ToastType::Warning => tokens::WARNING,
        ToastType::Error => tokens::ERROR,
    });
    let style = Style::default()
        .fg(theme.color(tokens::TEXT_PRIMARY).into())
        .bg(theme.color(tokens::BG_ELEVATED).into());

    Clear.render(area, buf);
    Paragraph::new(message)
        .style(style)
        .block(
            Block::default()
                .borders(Borders::LEFT | Borders::RIGHT)
                .border_set(symbols::border::QUADRANT_OUTSIDE)
                .border_style(accent)
                .padding(Padding::uniform(1)),
        )
        .render(area, buf);
}
