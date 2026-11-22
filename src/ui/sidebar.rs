use opaline::names::tokens;
use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Padding, Paragraph},
};
use unicode_width::UnicodeWidthStr;

use super::transaction_list::truncate_to_width;
use crate::app::state::AppState;
use crate::client::models::{Account, MoneyObject};

/// Drawn before the selected account, like the transaction list's selection marker.
/// Other rows are indented by the same width.
const MARKER: &str = "▎ ";
const MARKER_WIDTH: usize = 2;
/// Minimum space between an account's name and its balance.
const MIN_GAP: usize = 2;
/// Columns taken by the right border and the horizontal padding.
const CHROME_WIDTH: u16 = 3;
const MIN_WIDTH: u16 = 20;
const MAX_WIDTH: u16 = 40;

/// Width of the sidebar: enough for the longest account row, within limits, and at
/// most a third of the `available` width.
pub fn sidebar_width(state: &AppState, available: u16) -> u16 {
    let content = state
        .tabs
        .iter()
        .map(|tab| (tab.account.display_name.as_str(), &tab.account.balance))
        .chain(total_balance(state).as_ref().map(|total| ("Total", total)))
        .map(|(name, balance)| {
            MARKER_WIDTH + name.width() + MIN_GAP + balance.format_signed().width()
        })
        .max()
        .unwrap_or(0);
    u16::try_from(content)
        .unwrap_or(u16::MAX)
        .saturating_add(CHROME_WIDTH)
        .clamp(MIN_WIDTH, MAX_WIDTH)
        .min(available / 3)
}

/// The account list: accounts grouped by type with their balances, the active one
/// highlighted, and the total balance.
pub fn draw_sidebar(buf: &mut Buffer, area: Rect, state: &AppState) {
    let theme = &state.theme;
    let fg = |token| Style::default().fg(theme.color(token).into());

    // Like the detail pane: on the panel background, with a border separating it from
    // the list (in some themes `BG_PANEL` is the same colour as `BG_BASE`).
    let block = Block::default()
        .title(" Accounts ")
        .title_style(theme.color(tokens::ACCENT_PRIMARY))
        .borders(Borders::RIGHT)
        .border_style(theme.color(tokens::BORDER_UNFOCUSED))
        .style(fg(tokens::TEXT_PRIMARY).bg(theme.color(tokens::BG_PANEL).into()))
        .padding(Padding::uniform(1));
    let width = usize::from(block.inner(area).width);

    let heading_style = fg(tokens::TEXT_DIM).add_modifier(Modifier::BOLD);
    let selected_style = Style::default()
        .bg(theme.color(tokens::BG_SELECTION).into())
        .add_modifier(Modifier::BOLD);
    let marker_style = fg(tokens::ACCENT_PRIMARY);

    let mut lines = Vec::new();
    let mut group = None;
    for (i, tab) in state.tabs.iter().enumerate() {
        let account = &tab.account;
        let account_group = group_name(account);
        if group != Some(account_group) {
            if group.is_some() {
                lines.push(Line::default());
            }
            group = Some(account_group);
            let heading = format!("{} ", account_group.to_uppercase());
            let fill = width.saturating_sub(MARKER_WIDTH);
            lines.push(Line::styled(
                format!("{:MARKER_WIDTH$}{heading:┄<fill$}", ""),
                heading_style,
            ));
        }

        let mut row = account_row(&account.display_name, &account.balance, width);
        if i == state.active_tab {
            row.spans[0] = Span::styled(MARKER, marker_style);
            row = row.style(selected_style);
        }
        lines.push(row);
    }

    if let Some(total) = total_balance(state) {
        lines.push(Line::default());
        lines.push(account_row("Total", &total, width).style(fg(tokens::TEXT_SECONDARY).bold()));
    }

    Paragraph::new(lines).block(block).render(area, buf);
}

/// The heading of the group an account is shown in.
fn group_name(account: &Account) -> &str {
    match account.account_type.as_str() {
        "TRANSACTIONAL" => "Spending",
        "SAVER" => "Savers",
        "HOME_LOAN" => "Home loans",
        other => other,
    }
}

/// The total of all balances, if there's more than one account and they can be added
/// up (they're all in the same currency).
fn total_balance(state: &AppState) -> Option<MoneyObject> {
    if state.tabs.len() < 2 {
        return None;
    }
    MoneyObject::sum(state.tabs.iter().map(|tab| &tab.account.balance))
}

/// A row with the name on the left (truncated if needed) and the balance on the right,
/// filling `width` so that a background colour covers the whole row. The first span
/// is the marker column, left blank.
fn account_row(name: &str, balance: &MoneyObject, width: usize) -> Line<'static> {
    let balance = balance.format_signed();
    let name_width = width.saturating_sub(MARKER_WIDTH + MIN_GAP + balance.width());
    let name = truncate_to_width(name, name_width);
    let gap = width.saturating_sub(MARKER_WIDTH + name.width() + balance.width());
    Line::from(vec![
        Span::raw(" ".repeat(MARKER_WIDTH)),
        Span::raw(name),
        Span::raw(" ".repeat(gap)),
        Span::raw(balance),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::state::tests::{account, state};

    fn typed(id: &str, account_type: &str, cents: i64) -> Account {
        Account {
            account_type: account_type.to_string(),
            ..account(id, cents)
        }
    }

    fn render(state: &AppState, width: u16, height: u16) -> Vec<String> {
        text(&render_buffer(state, width, height))
    }

    fn render_buffer(state: &AppState, width: u16, height: u16) -> Buffer {
        let area = Rect::new(0, 0, width, height);
        let mut buf = Buffer::empty(area);
        draw_sidebar(&mut buf, area, state);
        buf
    }

    fn text(buf: &Buffer) -> Vec<String> {
        let Rect { width, height, .. } = buf.area;
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

    #[test]
    fn accounts_are_grouped_with_a_total() {
        let mut state = state();
        state.set_accounts(vec![
            typed("Holiday", "SAVER", 240000),
            typed("Spending", "TRANSACTIONAL", 123456),
        ]);
        state.active_tab = 1;
        let lines = render(&state, 32, 11);
        assert_eq!(
            lines,
            [
                " Accounts                      │",
                "                               │",
                "   SPENDING ┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄ │",
                "   Account Spending   $1234.56 │",
                "                               │",
                "   SAVERS ┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄ │",
                " ▎ Account Holiday    $2400.00 │",
                "                               │",
                "   Total              $3634.56 │",
                "                               │",
                "                               │",
            ]
        );
    }

    #[test]
    fn active_account_is_highlighted() {
        let mut state = state();
        state.set_accounts(vec![account("a", 100), account("b", 100)]);
        state.active_tab = 1;
        let buf = render_buffer(&state, 24, 8);
        let accent = Color::from(state.theme.color(tokens::ACCENT_PRIMARY));
        let selection = Color::from(state.theme.color(tokens::BG_SELECTION));
        // Row 3 is account "a", row 4 account "b"; column 1 is the marker column.
        assert_eq!(buf[(1, 4)].symbol(), "▎");
        assert_eq!(buf[(1, 4)].fg, accent);
        // The selection background covers the whole row, up to the padding.
        assert!((1..22).all(|x| buf[(x, 4)].bg == selection));
        assert!((1..22).all(|x| buf[(x, 3)].bg != selection));
    }

    #[test]
    fn long_names_are_truncated() {
        let mut state = state();
        state.set_accounts(vec![typed("A very long saver name", "SAVER", 100)]);
        let lines = render(&state, 24, 6);
        assert_eq!(lines[3], " ▎ Account A v…  $1.00 │");
    }

    #[test]
    fn width_fits_the_content_within_limits() {
        let named = |name: &str| Account {
            display_name: name.to_string(),
            ..account(name, 100)
        };
        let mut state = state();
        state.set_accounts(vec![named("A")]);
        assert_eq!(sidebar_width(&state, 200), MIN_WIDTH);

        state.set_accounts(vec![named("A"), named("Fourteen chars")]);
        // Marker, name, gap, balance ("$1.00"), then the border and padding.
        assert_eq!(sidebar_width(&state, 200), 2 + 14 + 2 + 5 + 3);
        // At most a third of the screen, even if that's less than the minimum.
        assert_eq!(sidebar_width(&state, 45), 15);

        state.set_accounts(vec![named(&"x".repeat(60))]);
        assert_eq!(sidebar_width(&state, 200), MAX_WIDTH);
    }
}
