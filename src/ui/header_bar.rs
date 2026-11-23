use opaline::names::tokens;
use ratatui::prelude::*;
use ratatui::widgets::Paragraph;
use unicode_width::UnicodeWidthStr;

use super::contrast::readable;
use super::transaction_list::truncate_to_width;
use crate::app::state::{AppState, MessageKind};

/// The app's name and version, at the left of the header bar.
const NAME: &str = env!("CARGO_PKG_NAME");
const VERSION: &str = concat!("v", env!("CARGO_PKG_VERSION"));

/// The header bar at the top of the screen: the app's name and version on the left,
/// and the current status message (if any) right-aligned in the rest of the width.
pub fn draw_header_bar(buf: &mut Buffer, area: Rect, state: &AppState) {
    Paragraph::new(header_line(state, usize::from(area.width)))
        .style(Style::default().bg(state.theme.color(tokens::BG_HIGHLIGHT).into()))
        .render(area, buf);
}

fn header_line(state: &AppState, width: usize) -> Line<'static> {
    let theme = &state.theme;
    let fg = |token| Style::default().fg(theme.color(token).into());
    let mut spans = vec![
        Span::styled(format!(" {NAME}"), fg(tokens::ACCENT_PRIMARY).bold()),
        Span::styled(
            format!(" {VERSION}"),
            Style::default().fg(readable(theme, tokens::TEXT_DIM, tokens::BG_HIGHLIGHT)),
        ),
    ];
    let used: usize = spans.iter().map(|s| s.content.width()).sum();

    if let Some(message) = &state.message {
        let (marker, marker_token, text_token) = match message.kind {
            MessageKind::Info => ("●", tokens::INFO, tokens::TEXT_SECONDARY),
            MessageKind::Success => ("✓", tokens::SUCCESS, tokens::TEXT_SECONDARY),
            MessageKind::Error => ("✗", tokens::ERROR, tokens::ERROR),
        };
        // The marker, a space, the text and a space at the right edge, at least two
        // columns away from the name.
        let room = width.saturating_sub(used + 2 + marker.width() + 2);
        if room > 0 {
            let text = truncate_to_width(&message.text, room);
            let gap = width - used - marker.width() - 1 - text.width() - 1;
            spans.push(Span::raw(" ".repeat(gap)));
            spans.push(Span::styled(marker, fg(marker_token)));
            spans.push(Span::styled(format!(" {text} "), fg(text_token)));
        }
    }
    Line::from(spans)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::state::Message;
    use crate::app::state::tests::state;

    fn text(line: &Line) -> String {
        line.spans.iter().map(|s| s.content.as_ref()).collect()
    }

    fn with_message(text: &str, kind: MessageKind) -> AppState {
        let mut state = state();
        state.message = Some(Message {
            text: text.to_string(),
            kind,
        });
        state
    }

    #[test]
    fn shows_the_name_and_version() {
        let line = header_line(&state(), 40);
        assert_eq!(text(&line), format!(" up-tui {VERSION}"));
    }

    #[test]
    fn the_message_is_right_aligned() {
        let state = with_message("Accounts loaded", MessageKind::Success);
        let line = text(&header_line(&state, 50));
        assert_eq!(line.width(), 50);
        assert!(line.starts_with(" up-tui v"));
        assert!(line.ends_with("  ✓ Accounts loaded "), "{line:?}");
    }

    #[test]
    fn a_long_message_is_truncated() {
        let state = with_message(
            "Failed to load accounts: connection refused",
            MessageKind::Error,
        );
        let line = header_line(&state, 40);
        let text = text(&line);
        assert_eq!(text.width(), 40);
        // Truncated to leave the usual two-column gap after the name.
        assert!(
            text.contains(&format!("{VERSION}  ✗ Failed to load")),
            "{text:?}"
        );
        assert!(text.ends_with("… "), "{text:?}");
        let error = Some(state.theme.color(tokens::ERROR).into());
        assert_eq!(line.spans.last().unwrap().style.fg, error);
    }

    #[test]
    fn no_room_for_the_message() {
        let state = with_message("Accounts loaded", MessageKind::Info);
        let line = text(&header_line(&state, 12));
        assert!(line.starts_with(" up-tui"));
        assert!(!line.contains("Accounts"));
    }
}
