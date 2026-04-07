pub mod status_bar;
pub mod tabs;
pub mod transaction_detail;
pub mod transaction_list;

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout};
use ratatui::style::Style;
use ratatui::widgets::{Block, Widget};

use crate::app::state::AppState;

pub fn draw(f: &mut Frame, state: &mut AppState) {
    let palette = state.palette();

    // Fill entire background with theme color
    Block::default()
        .style(Style::default().bg(palette.bg).fg(palette.fg))
        .render(f.area(), f.buffer_mut());

    let chunks = Layout::vertical([
        Constraint::Length(1), // tab bar
        Constraint::Fill(1),   // main content
        Constraint::Length(1), // status bar
    ])
    .split(f.area());

    tabs::draw_tabs(f, chunks[0], state);

    if state.show_detail {
        // Split main content horizontally: transaction list (left) + detail pane (right)
        let content = Layout::horizontal([
            Constraint::Percentage(50),
            Constraint::Percentage(50),
        ])
        .split(chunks[1]);

        transaction_list::draw_transaction_list(f, content[0], state);
        transaction_detail::draw_detail_pane(f, content[1], state);
    } else {
        transaction_list::draw_transaction_list(f, chunks[1], state);
    }

    status_bar::draw_status_bar(f, chunks[2], state);
}
