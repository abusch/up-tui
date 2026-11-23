use opaline::names::tokens;
use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Padding, Paragraph},
};

use crate::app::state::{AppState, TabState};

/// Rows taken by the bottom border.
const CHROME_HEIGHT: u16 = 1;
/// Columns taken by the horizontal padding.
const CHROME_WIDTH: u16 = 2;

/// Height of the header when drawn `width` columns wide: 0 if there is no account to
/// show (e.g. the accounts haven't loaded yet).
pub fn header_height(state: &AppState, width: u16) -> u16 {
    let Some(tab) = state.current_tab() else {
        return 0;
    };
    let lines = header_lines(tab, state, usize::from(width.saturating_sub(CHROME_WIDTH)));
    u16::try_from(lines.len())
        .unwrap_or(u16::MAX)
        .saturating_add(CHROME_HEIGHT)
}

/// The selected account's name, type, balance, and totals over the selected period,
/// above the transaction list.
pub fn draw_account_header(buf: &mut Buffer, area: Rect, state: &AppState) {
    let Some(tab) = state.current_tab() else {
        return;
    };
    let block = Block::default()
        .borders(Borders::BOTTOM)
        .border_style(state.theme.color(tokens::BORDER_UNFOCUSED))
        .padding(Padding::horizontal(1));
    let width = usize::from(block.inner(area).width);
    Paragraph::new(header_lines(tab, state, width))
        .block(block)
        .render(area, buf);
}

fn header_lines(tab: &TabState, state: &AppState, width: usize) -> Vec<Line<'static>> {
    let theme = &state.theme;
    let fg = |token| Style::default().fg(theme.color(token).into());
    let dim = fg(tokens::TEXT_DIM);
    let account = &tab.account;

    let title = Line::from(vec![
        Span::styled(
            account.display_name.clone(),
            fg(tokens::TEXT_PRIMARY).bold(),
        ),
        Span::styled(
            format!(
                "  {} · {}",
                type_name(&account.account_type),
                ownership_name(&account.ownership_type)
            ),
            dim,
        ),
    ]);

    let balance = vec![Span::styled(
        account.balance.format_signed(),
        fg(tokens::TEXT_PRIMARY).bold(),
    )];
    let period = Span::styled(format!("{}  ", state.period.name(state.today())), dim);
    // Stale totals stay visible while they're refreshed.
    let totals = match (tab.recent_totals(), tab.store.loading().is_some()) {
        (Some((money_in, money_out)), _) => vec![
            period,
            Span::styled("in ", dim),
            Span::styled(money_in.format_display(), fg(tokens::SUCCESS)),
            Span::styled("  out ", dim),
            // Without a sign: `format_display` would show no debits as "+$0.00".
            Span::styled(money_out.format_unsigned(), fg(tokens::TEXT_PRIMARY)),
        ],
        (None, true) => vec![period, Span::styled("loading…", dim)],
        // Failed to load (an error message was shown), or not requested yet.
        (None, false) => Vec::new(),
    };

    let mut lines = vec![title];
    if totals.is_empty() {
        lines.push(Line::from(balance));
    } else {
        lines.extend(super::right_aligned(balance, totals, width));
    }
    lines
}

/// A readable name for the API's account type.
fn type_name(account_type: &str) -> &str {
    match account_type {
        "TRANSACTIONAL" => "Spending account",
        "SAVER" => "Saver",
        "HOME_LOAN" => "Home loan",
        other => other,
    }
}

/// A readable name for the API's ownership type.
fn ownership_name(ownership_type: &str) -> &str {
    match ownership_type {
        "INDIVIDUAL" => "Individual",
        "JOINT" => "Joint (2Up)",
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::state::Period;
    use crate::app::state::tests::{account, money, state, txn};
    use crate::client::models::{Account, Transaction};

    fn render(state: &AppState, width: u16) -> Vec<String> {
        let height = header_height(state, width);
        let area = Rect::new(0, 0, width, height);
        let mut buf = Buffer::empty(area);
        draw_account_header(&mut buf, area, state);
        (0..height)
            .map(|y| {
                (0..width)
                    .map(|x| buf[(x, y)].symbol())
                    .collect::<String>()
                    .trim_end()
                    .to_string()
            })
            .collect()
    }

    /// A state with a saver account, over the last 30 days: this month's name changes.
    fn state_with_saver() -> AppState {
        let mut state = state();
        state.period = Period::Last30Days;
        state.set_accounts(vec![Account {
            display_name: "Holiday".to_string(),
            account_type: "SAVER".to_string(),
            ownership_type: "JOINT".to_string(),
            ..account("h", 240000)
        }]);
        state
    }

    #[test]
    fn shows_the_account_and_its_totals() {
        let mut state = state_with_saver();
        let with_amount = |id, cents| Transaction {
            amount: money(cents),
            ..txn(id, "2026-11-20T18:00:00Z")
        };
        state.tabs[0].set_transactions(vec![with_amount("a", 50000), with_amount("b", -1250)]);
        assert_eq!(
            render(&state, 60),
            [
                " Holiday  Saver · Joint (2Up)",
                " $2400.00             Last 30 days  in +$500.00  out $12.50",
                "────────────────────────────────────────────────────────────",
            ]
        );
    }

    #[test]
    fn totals_move_to_their_own_line_when_narrow() {
        let mut state = state_with_saver();
        state.tabs[0].set_transactions(Vec::new());
        assert_eq!(
            render(&state, 40),
            [
                " Holiday  Saver · Joint (2Up)",
                " $2400.00",
                " Last 30 days  in +$0.00  out $0.00",
                "────────────────────────────────────────",
            ]
        );
    }

    #[test]
    fn totals_show_when_they_are_loading_and_not_when_they_failed() {
        let mut state = state_with_saver();
        let since = "2026-11-01T00:00:00Z".parse().unwrap();
        let fetch = state.tabs[0].store.next_fetch(since, false).unwrap();
        assert_eq!(
            render(&state, 60)[1],
            " $2400.00                            Last 30 days  loading…"
        );
        state.tabs[0].store.load_failed(fetch.generation);
        assert_eq!(render(&state, 60)[1], " $2400.00");
    }

    #[test]
    fn nothing_without_an_account() {
        assert_eq!(header_height(&state(), 60), 0);
    }
}
