pub mod status_bar;
pub mod tabs;
pub mod transaction_detail;
pub mod transaction_list;

use ratatui::layout::{Constraint, Layout};
use ratatui::Frame;

use crate::app::state::{AppMode, AppState};

pub fn draw(f: &mut Frame, state: &AppState) {
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
