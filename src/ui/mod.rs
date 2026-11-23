pub mod account_header;
pub mod contrast;
pub mod header_bar;
pub mod help;
pub mod insights;
pub mod qr_code;
pub mod sidebar;
pub mod status_bar;
pub mod text;
pub mod transaction_detail;
pub mod transaction_list;

use opaline::ThemeSelector;
use opaline::names::tokens;
use ratatui::prelude::*;
use ratatui::widgets::{Block, Clear, Widget};

use crate::app::App;
use crate::app::state::{AppMode, View};

/// Minimum width of the detail pane, including its border and padding.
const DETAIL_MIN_WIDTH: u16 = 40;
/// Minimum width of the transaction list beside the detail pane, including its padding.
const LIST_MIN_WIDTH: u16 = 36;

/// Split `main` into the transaction list (left) and the detail pane (right), or give
/// it all to the pane (with no list) if they don't both fit at their minimum widths.
///
/// The pane takes a third of the screen's width (`screen_width`), not of what the
/// sidebar leaves, so that it stays wide enough to read; the list gets the rest,
/// including its right padding, which separates it from the pane's border.
fn split_list_and_detail(main: Rect, screen_width: u16) -> (Option<Rect>, Rect) {
    let detail_width = (screen_width / 3).max(DETAIL_MIN_WIDTH);
    if main.width < detail_width.saturating_add(LIST_MIN_WIDTH) {
        return (None, main);
    }
    let [list, detail] =
        Layout::horizontal([Constraint::Fill(1), Constraint::Length(detail_width)]).areas(main);
    (Some(list), detail)
}

impl Widget for &mut App {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let bg = self.state.theme.color(tokens::BG_BASE).into();
        let fg = self.state.theme.color(tokens::TEXT_PRIMARY).into();

        // Fill entire background with theme color
        Block::default()
            .style(Style::default().bg(bg).fg(fg))
            .render(area, buf);

        let [header_bar, main, status] = Layout::vertical([
            Constraint::Length(1),
            Constraint::Fill(1),
            Constraint::Length(1),
        ])
        .areas(area);
        header_bar::draw_header_bar(buf, header_bar, &self.state);
        // The help and QR code popups go between the header and status bars, leaving
        // the status bar's hints (on how to scroll or close them) visible.
        let popup_area = main;

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

        // The selected account's header, above the list and the detail pane.
        let header_height = account_header::header_height(&self.state, main.width);
        let [header, main] =
            Layout::vertical([Constraint::Length(header_height), Constraint::Fill(1)]).areas(main);
        account_header::draw_account_header(buf, header, &self.state);

        // A filtered list has a bar above it (and the detail pane), saying what it shows.
        let main = if self.state.view == View::Transactions && self.state.filter.is_some() {
            let [bar, rest] =
                Layout::vertical([Constraint::Length(1), Constraint::Fill(1)]).areas(main);
            transaction_list::draw_filter_bar(buf, bar, &self.state);
            rest
        } else {
            main
        };

        if self.state.view == View::Insights {
            insights::draw_insights(buf, main, &self.state);
        } else if self.state.show_detail {
            match split_list_and_detail(main, area.width) {
                (Some(list), detail) => {
                    transaction_list::draw_transaction_list(buf, list, &mut self.state);
                    transaction_detail::draw_detail_pane(buf, detail, &self.state, true);
                }
                (None, detail) => {
                    // The list isn't drawn, but j/k still move through it (and paging
                    // and loading more depend on how many transactions it would show).
                    self.state.page_size = transaction_list::page_size(main);
                    transaction_detail::draw_detail_pane(buf, detail, &self.state, false);
                }
            }
        } else {
            transaction_list::draw_transaction_list(buf, main, &mut self.state);
        }

        status_bar::draw_status_bar(buf, status, &self.state);

        if let AppMode::Help { ref mut scroll } = self.state.mode {
            help::draw_help(buf, popup_area, &self.state.theme, scroll);
        }
        if let AppMode::QrCode(ref popup) = self.state.mode {
            qr_code::draw_qr_code(buf, popup_area, area, &self.state.theme, popup);
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_detail_pane_takes_a_third_of_the_screen() {
        let main = Rect::new(30, 2, 120, 30);
        let (list, detail) = split_list_and_detail(main, 150);
        assert_eq!(list, Some(Rect::new(30, 2, 70, 30)));
        assert_eq!(detail, Rect::new(100, 2, 50, 30));
    }

    #[test]
    fn the_detail_pane_has_a_minimum_width() {
        let main = Rect::new(0, 2, 90, 30);
        let (list, detail) = split_list_and_detail(main, 90);
        assert_eq!(list, Some(Rect::new(0, 2, 90 - DETAIL_MIN_WIDTH, 30)));
        assert_eq!(detail.width, DETAIL_MIN_WIDTH);
    }

    #[test]
    fn only_the_detail_pane_is_shown_if_the_list_does_not_fit_beside_it() {
        let width = DETAIL_MIN_WIDTH + LIST_MIN_WIDTH;
        let main = Rect::new(0, 2, width - 1, 30);
        assert_eq!(split_list_and_detail(main, width - 1), (None, main));
        // With the sidebar taking some of the screen.
        let main = Rect::new(30, 2, width - 1, 30);
        assert_eq!(split_list_and_detail(main, width + 29), (None, main));
        let main = Rect::new(0, 2, width, 30);
        assert!(split_list_and_detail(main, width).0.is_some());
    }
}
