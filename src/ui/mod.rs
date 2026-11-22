pub mod sidebar;
pub mod status_bar;
pub mod toast;
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

        let [main, status] =
            Layout::vertical([Constraint::Fill(1), Constraint::Length(1)]).areas(area);

        // Account sidebar on the left, if shown.
        let main = if self.state.show_sidebar {
            let width = sidebar::sidebar_width(&self.state, main.width);
            let [sidebar_area, rest] =
                Layout::horizontal([Constraint::Length(width), Constraint::Fill(1)]).areas(main);
            sidebar::draw_sidebar(buf, sidebar_area, &self.state);
            rest
        } else {
            main
        };

        if self.state.show_detail {
            // Split the rest horizontally: transaction list (left) + detail pane (right).
            // The pane takes a third of the screen's width, not of what the sidebar
            // leaves, so that it stays wide enough to read; the list gets the rest.
            let [list, detail] =
                Layout::horizontal([Constraint::Fill(1), Constraint::Length(area.width / 3)])
                    .spacing(Spacing::Overlap(1))
                    .areas(main);

            transaction_list::draw_transaction_list(buf, list, &mut self.state);
            transaction_detail::draw_detail_pane(buf, detail, &self.state);
        } else {
            transaction_list::draw_transaction_list(buf, main, &mut self.state);
        }

        status_bar::draw_status_bar(buf, status, &self.state);

        if let AppMode::Detail = self.state.mode {
            transaction_detail::draw_detail_popup(buf, area, &self.state);
        }

        self.render_toasts(area, buf);

        if let AppMode::Theme(ref mut state) = self.state.mode {
            let centered_area = area.centered(Constraint::Ratio(2, 3), Constraint::Ratio(3, 4));
            Clear.render(centered_area, buf);
            // The picker sets only foreground colours, so give it a popup background.
            let popup_bg = self.state.theme.color(tokens::BG_ELEVATED).into();
            Block::new()
                .style(Style::default().bg(popup_bg).fg(fg))
                .render(centered_area, buf);
            ThemeSelector::new()
                .title("Theme")
                .render(centered_area, buf, state);
        }
    }
}
