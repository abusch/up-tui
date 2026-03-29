pub mod status_bar;
pub mod tabs;
pub mod transaction_detail;
pub mod transaction_list;

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout};
use ratatui::style::Style;
use ratatui::widgets::{Block, Widget};

use crate::app::state::{AppMode, AppState};

pub fn draw(f: &mut Frame, state: &mut AppState) {
    let palette = state.palette();

    // Fill entire background with theme color
    Block::default()
        .style(Style::default().bg(palette.bg).fg(palette.fg))
        .render(f.area(), f.buffer_mut());

    let chunks = Layout::vertical([
        Constraint::Length(1), // tab bar
        Constraint::Fill(1),   // transaction list
        Constraint::Length(1), // status bar
    ])
    .split(f.area());

    // Record the list area height so key handlers can use it for page scrolling.
    // Subtract 3 for borders (2) and header row (1).
    state.list_height = chunks[1].height.saturating_sub(3);

    tabs::draw_tabs(f, chunks[0], state);
    transaction_list::draw_transaction_list(f, chunks[1], state);
    status_bar::draw_status_bar(f, chunks[2], state);

    if state.mode == AppMode::Detail {
        transaction_detail::draw_detail_overlay(f, state);
    }
}
