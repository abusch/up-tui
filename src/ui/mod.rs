pub mod status_bar;
pub mod tabs;
pub mod transaction_detail;
pub mod transaction_list;

use ratatui::prelude::*;
use ratatui::widgets::{Block, Widget};

use crate::app::App;

impl Widget for &mut App {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let palette = self.state.palette();

        // Fill entire background with theme color
        Block::default()
            .style(Style::default().bg(palette.bg).fg(palette.fg))
            .render(area, buf);

        let chunks = Layout::vertical([
            Constraint::Length(1), // tab bar
            Constraint::Fill(1),   // main content
            Constraint::Length(1), // status bar
        ])
        .split(area);

        tabs::draw_tabs(buf, chunks[0], &self.state);

        if self.state.show_detail {
            // Split main content horizontally: transaction list (left) + detail pane (right)
            let content =
                Layout::horizontal([Constraint::Fill(1), Constraint::Min(25)]).split(chunks[1]);

            transaction_list::draw_transaction_list(buf, content[0], &mut self.state);
            transaction_detail::draw_detail_pane(buf, content[1], &self.state);
        } else {
            transaction_list::draw_transaction_list(buf, chunks[1], &mut self.state);
        }

        status_bar::draw_status_bar(buf, chunks[2], &self.state);

        if self.toaster.has_toast() {
            self.toaster.set_area(area);
            self.toaster.render(area, buf);
        }
    }
}
