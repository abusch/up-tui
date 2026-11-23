use opaline::names::tokens;
use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Padding, Paragraph},
};

use super::contrast::readable;
use crate::app::categories::Categories;
use crate::app::insights::spending_by_root;
use crate::app::state::{AppState, TabState};

/// Rows taken by the bottom border.
const CHROME_HEIGHT: u16 = 1;
/// Columns taken by the horizontal padding.
const CHROME_WIDTH: u16 = 2;
/// Columns taken by the spending bar, which stand for all the spending in the period.
const BAR_WIDTH: u16 = 30;
/// The spending bar's cells: a lower three-quarters block, thinner than a full block
/// but thick enough to show the colours.
const BAR_SYMBOL: &str = "▆";

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
    // On the panel background, like the sidebar and the detail pane, framing the list.
    // Not `BG_ELEVATED`/`BG_ACTIVE`: in most themes they are the selection's colour,
    // so the header would look like a selected row. The border separates it from the
    // list in themes where `BG_PANEL` is the same colour as `BG_BASE`.
    let block = Block::default()
        .borders(Borders::BOTTOM)
        .border_style(state.theme.color(tokens::BORDER_UNFOCUSED))
        .style(Style::default().bg(state.theme.color(tokens::BG_PANEL).into()))
        .padding(Padding::horizontal(1));
    let width = usize::from(block.inner(area).width);
    Paragraph::new(header_lines(tab, state, width))
        .block(block)
        .render(area, buf);
}

fn header_lines(tab: &TabState, state: &AppState, width: usize) -> Vec<Line<'static>> {
    let theme = &state.theme;
    let fg = |token| Style::default().fg(theme.color(token).into());
    let dim = Style::default().fg(readable(theme, tokens::TEXT_DIM, tokens::BG_PANEL));
    let account = &tab.account;

    let title = vec![
        Span::styled(
            account.display_name.clone(),
            fg(tokens::ACCENT_PRIMARY).bold(),
        ),
        Span::styled(
            format!(
                "  {} · {}",
                type_name(&account.account_type),
                ownership_name(&account.ownership_type)
            ),
            dim,
        ),
    ];

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

    // Above the totals, on the title's line if it fits.
    let mut lines = match spending_bar(tab, state, width) {
        Some(bar) => super::right_aligned(title, bar, width),
        None => vec![Line::from(title)],
    };
    if totals.is_empty() {
        lines.push(Line::from(balance));
    } else {
        lines.extend(super::right_aligned(balance, totals, width));
    }
    lines
}

/// The spending over the period (as in the Insights view) as a bar `BAR_WIDTH`
/// columns wide (or `width`, if narrower), split between the root categories in their
/// colours, always in the same order, then the rest (uncategorised, or in a root
/// category without a colour) in the uncategorised colour. `None` if the period's
/// transactions aren't loaded, or there is no spending.
fn spending_bar(tab: &TabState, state: &AppState, width: usize) -> Option<Vec<Span<'static>>> {
    let roots = spending_by_root(tab.store.recent()?);
    let mut segments: Vec<_> = Categories::coloured_roots()
        .map(|id| {
            let total = roots
                .iter()
                .find(|r| r.category.as_deref() == Some(id))
                .map_or(0, |r| r.total);
            (state.category_color(Some(id)), total)
        })
        .collect();
    let other = roots
        .iter()
        .filter(|r| !Categories::coloured_roots().any(|id| r.category.as_deref() == Some(id)))
        .fold(0_i64, |sum, r| sum.saturating_add(r.total));
    segments.push((state.category_color(None), other));

    let width = BAR_WIDTH.min(u16::try_from(width).unwrap_or(u16::MAX));
    let totals: Vec<i64> = segments.iter().map(|&(_, total)| total).collect();
    let cells = split_cells(&totals, width)?;
    Some(
        segments
            .iter()
            .zip(cells)
            .filter(|&(_, n)| n > 0)
            .map(|(&(color, _), n)| Span::styled(BAR_SYMBOL.repeat(n), Style::default().fg(color)))
            .collect(),
    )
}

/// `width` cells split between `amounts` (negative ones count as 0) in proportion,
/// adding up to exactly `width`: each gets its share rounded down, and the cells left
/// go to the largest remainders (the first, on a tie). `None` if they add up to 0.
fn split_cells(amounts: &[i64], width: u16) -> Option<Vec<usize>> {
    let amounts: Vec<u128> = amounts
        .iter()
        .map(|&a| u128::from(a.max(0).unsigned_abs()))
        .collect();
    let total: u128 = amounts.iter().sum();
    if total == 0 {
        return None;
    }
    let width = u128::from(width);
    let mut cells: Vec<u128> = amounts.iter().map(|a| a * width / total).collect();
    let left = width - cells.iter().sum::<u128>();
    let mut by_remainder: Vec<usize> = (0..amounts.len()).collect();
    // Stable, so ties keep their order.
    by_remainder.sort_by_key(|&i| std::cmp::Reverse(amounts[i] * width % total));
    for &i in by_remainder.iter().take(usize::try_from(left).unwrap_or(0)) {
        cells[i] += 1;
    }
    Some(
        cells
            .into_iter()
            .map(|n| usize::try_from(n).unwrap_or_default())
            .collect(),
    )
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
            render(&state, 70),
            [
                format!(
                    " Holiday  Saver · Joint (2Up){}{}",
                    " ".repeat(10),
                    BAR_SYMBOL.repeat(30)
                ),
                " $2400.00                       Last 30 days  in +$500.00  out $12.50".to_string(),
                "─".repeat(70),
            ]
        );
        // Without room beside the title, the bar goes on a line of its own.
        assert_eq!(render(&state, 60)[1], format!(" {}", BAR_SYMBOL.repeat(30)));
    }

    #[test]
    fn the_bar_splits_the_spending_by_root_category_in_a_fixed_order() {
        let mut state = state_with_saver();
        let spent = |id, cents, parent: Option<&str>| Transaction {
            amount: money(cents),
            category: parent.map(|p| format!("{p}-child")),
            parent_category: parent.map(str::to_owned),
            ..txn(id, "2026-11-20T18:00:00Z")
        };
        let mut transfer = spent("t", -90000, None);
        transfer.transfer_account = Some("other".to_string());
        state.tabs[0].set_transactions(vec![
            spent("a", -1000, None),
            spent("b", -1000, Some("transport")),
            spent("c", -3000, Some("good-life")),
            // Not spending: a credit and a transfer.
            spent("d", 5000, Some("home")),
            transfer,
        ]);
        let area = Rect::new(0, 0, 70, header_height(&state, 70));
        let mut buf = Buffer::empty(area);
        draw_account_header(&mut buf, area, &state);
        let colors: Vec<_> = (39..69).map(|x| buf[(x, 0)].fg).collect();
        let color = |category| state.category_color(category);
        let expected: Vec<_> = [
            (color(Some("good-life")), 18),
            (color(Some("transport")), 6),
            (color(None), 6),
        ]
        .into_iter()
        .flat_map(|(c, n)| std::iter::repeat_n(c, n))
        .collect();
        assert_eq!(colors, expected);
        assert_eq!(buf[(38, 0)].symbol(), " ");
    }

    #[test]
    fn no_bar_without_spending() {
        let mut state = state_with_saver();
        state.tabs[0].set_transactions(vec![Transaction {
            amount: money(50000),
            ..txn("a", "2026-11-20T18:00:00Z")
        }]);
        assert_eq!(render(&state, 70)[0], " Holiday  Saver · Joint (2Up)");
    }

    #[test]
    fn cells_are_split_in_proportion_and_add_up_to_the_width() {
        assert_eq!(split_cells(&[1, 1, 1], 10), Some(vec![4, 3, 3]));
        assert_eq!(split_cells(&[0, 5, 0, 15], 8), Some(vec![0, 2, 0, 6]));
        // 0.5 cell is rounded up when it has the largest remainder, 0.4 isn't.
        assert_eq!(split_cells(&[1, 95, 4], 10), Some(vec![0, 10, 0]));
        assert_eq!(split_cells(&[5, 90, 5], 10), Some(vec![1, 9, 0]));
        assert_eq!(split_cells(&[i64::MAX, i64::MAX], 30), Some(vec![15, 15]));
        assert_eq!(split_cells(&[0, -5], 10), None);
        assert_eq!(split_cells(&[], 10), None);
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
