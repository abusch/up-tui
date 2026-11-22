pub mod status_bar;
pub mod tabs;
pub mod transaction_detail;
pub mod transaction_list;

use opaline::ThemeSelector;
use opaline::names::tokens;
use ratatui::layout::Spacing;
use ratatui::prelude::*;
use ratatui::widgets::{Block, Clear, Widget};

use crate::app::App;
use crate::app::state::AppMode;

impl Widget for &mut App {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let bg = self.state.theme.color(tokens::BG_BASE).into();
        let fg = self.state.theme.color(tokens::TEXT_PRIMARY).into();

        // Fill entire background with theme color
        Block::default()
            .style(Style::default().bg(bg).fg(fg))
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
            let content = Layout::horizontal([Constraint::Fill(2), Constraint::Fill(1)])
                .spacing(Spacing::Overlap(1))
                .split(chunks[1]);

            transaction_list::draw_transaction_list(buf, content[0], &mut self.state);
            transaction_detail::draw_detail_pane(buf, content[1], &self.state);
        } else {
            transaction_list::draw_transaction_list(buf, chunks[1], &mut self.state);
        }

        status_bar::draw_status_bar(buf, chunks[2], &self.state);

        self.render_toasts(area, buf);

        if let AppMode::Theme(ref mut state) = self.state.mode {
            let centered_area = area.centered(Constraint::Ratio(2, 3), Constraint::Ratio(3, 4));
            Clear.render(centered_area, buf);
            Block::new()
                .style(Style::default().bg(bg))
                .render(centered_area, buf);
            ThemeSelector::new()
                .title("Theme")
                .render(centered_area, buf, state);
        }
    }
}
