//! The QR code popup: a transaction's deep link, to scan with a phone to open the
//! transaction in Up's app.

use opaline::Theme;
use opaline::names::tokens;
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, Padding, Paragraph};
use tui_qrcode::{Colors, QrCodeWidget};

use super::contrast::readable;
use crate::app::state::QrCodePopup;

/// The QR code's colours. These are fixed rather than taken from the theme: phone
/// cameras read dark modules on a light background (with a light border, the "quiet
/// zone") most reliably, and a dark theme's colours would invert it.
const DARK: Color = Color::Rgb(0, 0, 0);
const LIGHT: Color = Color::Rgb(255, 255, 255);

/// Draw the popup centred in `area`, part of a `screen` that may not be big enough for
/// it (it then says how big the screen must be).
pub fn draw_qr_code(
    buf: &mut Buffer,
    area: Rect,
    screen: Rect,
    theme: &Theme,
    popup: &QrCodePopup,
) {
    let dim = Style::default().fg(readable(theme, tokens::TEXT_DIM, tokens::BG_ELEVATED));
    let block = Block::new()
        .title(Line::from(vec![
            Span::raw(" "),
            Span::styled(
                popup.description.as_str(),
                Style::from(theme.color(tokens::TEXT_PRIMARY)).bold(),
            ),
            Span::raw(" "),
        ]))
        .borders(Borders::ALL)
        // Like the help and theme picker popups.
        .border_style(theme.color(tokens::ACCENT_PRIMARY))
        .style(
            Style::default()
                .fg(theme.color(tokens::TEXT_PRIMARY).into())
                .bg(theme.color(tokens::BG_ELEVATED).into()),
        )
        .padding(Padding::horizontal(2));

    // Each module is one column wide and half a row high, with the quiet zone around it.
    let qr = QrCodeWidget::new(popup.qr_code.clone())
        .colors(Colors::Normal)
        .style(Style::default().fg(DARK).bg(LIGHT));
    let qr_size = qr.size(Rect::default());
    let hint = Line::styled("Scan to open it in Up", dim).centered();
    let hint_width = u16::try_from(hint.width()).unwrap_or(u16::MAX);

    // The border and padding around the code, and a line below it for the hint.
    let (frame_width, frame_height) = (2 + 4, 2 + 1);
    let width = qr_size.width.max(hint_width).saturating_add(frame_width);
    let height = qr_size.height.saturating_add(frame_height);

    if width > area.width || height > area.height {
        // A cropped code can't be scanned: ask for room instead, including what the
        // rest of the screen takes (the header and status bars).
        let screen_width = width.saturating_add(screen.width.saturating_sub(area.width));
        let screen_height = height.saturating_add(screen.height.saturating_sub(area.height));
        let message = Line::styled(
            format!("Make the window at least {screen_width}×{screen_height} to show the QR code"),
            dim,
        )
        .centered();
        let width = u16::try_from(message.width())
            .unwrap_or(u16::MAX)
            .saturating_add(frame_width)
            .min(area.width);
        let popup_area = area.centered(Constraint::Length(width), Constraint::Length(3));
        Clear.render(popup_area, buf);
        Paragraph::new(message).block(block).render(popup_area, buf);
        return;
    }

    let popup_area = area.centered(Constraint::Length(width), Constraint::Length(height));
    Clear.render(popup_area, buf);
    let inner = block.inner(popup_area);
    block.render(popup_area, buf);
    let [code, hint_area] =
        Layout::vertical([Constraint::Length(qr_size.height), Constraint::Length(1)]).areas(inner);
    qr.render(
        code.centered_horizontally(Constraint::Length(qr_size.width)),
        buf,
    );
    hint.render(hint_area, buf);
}
