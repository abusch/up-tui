pub mod status_bar;
pub mod tabs;
pub mod transaction_detail;
pub mod transaction_list;

use ratatui::layout::{Constraint, Layout};
use ratatui::style::Style;
use ratatui::widgets::{Block, Widget};
use ratatui::Frame;

use crate::app::state::{AppMode, AppState};

pub fn draw(f: &mut Frame, state: &AppState) {
    let palette = state.palette();

    // Fill entire background with theme color
    Block::default()
        .style(Style::default().bg(palette.bg).fg(palette.fg))
        .render(f.area(), f.buffer_mut());

    let chunks = Layout::vertical([
        Constraint::Length(3),  // tab bar
        Constraint::Fill(1),   // transaction list
        Constraint::Length(3), // status bar
    ])
    .split(f.area());

    tabs::draw_tabs(f, chunks[0], state);
    transaction_list::draw_transaction_list(f, chunks[1], state);
    status_bar::draw_status_bar(f, chunks[2], state);

    if state.mode == AppMode::Detail {
        transaction_detail::draw_detail_overlay(f, state);
    }
}
