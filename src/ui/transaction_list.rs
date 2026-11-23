use opaline::names::tokens;
use ratatui::{
    prelude::*,
    widgets::{Block, HighlightSpacing, List, ListItem, Padding, Paragraph},
};

use unicode_width::UnicodeWidthStr;

use crate::app::state::AppState;
use crate::app::store::LoadKind;
use crate::client::models::{MoneyObject, Transaction};

use super::contrast::readable;
use super::text::{
    SELECTION_MARKER, SELECTION_MARKER_WIDTH, right_aligned_if_room, truncate_to_width,
};
use super::transaction_detail::format_day;

/// Marks a transaction's category, in its root category's colour: a square, which
/// lines up with the text beside it (unlike the account header's `▆`), and shows more
/// colour than a dot without being as heavy as a full block.
const CATEGORY_SYMBOL: &str = "■";

/// The list's block, without its style.
fn list_block() -> Block<'static> {
    Block::default().padding(Padding::uniform(1))
}

/// How many transactions fit in a list drawn in `area`, for Ctrl+D/Ctrl+U paging and
/// for deciding when to load the next page. Each transaction item is 2 rows tall; the
/// day headers (and the blank rows above them) are ignored, so this is an upper bound.
pub fn page_size(area: Rect) -> usize {
    usize::from(list_block().inner(area).height / 2)
}

pub fn draw_transaction_list(buf: &mut Buffer, area: Rect, state: &mut AppState) {
    let fg = state.theme.color(tokens::TEXT_PRIMARY).into();
    let bg = state.theme.color(tokens::BG_BASE).into();
    let accent = state.theme.color(tokens::ACCENT_PRIMARY);
    let selection = state.theme.color(tokens::BG_SELECTION).into();

    let base_style = Style::default().fg(fg).bg(bg);
    let mut block = list_block().style(base_style);
    let inner = block.inner(area);
    state.page_size = page_size(area);

    // No title, as it's clearly a list of transactions (and the account header names
    // the account), except for the loading status, if any.
    let (status, transactions) = list_status(state);
    if let Some(status) = status {
        block = block.title(format!(" {status} ")).title_style(accent);
    }

    let transactions = match transactions {
        Some(txns) if !txns.is_empty() => txns,
        loaded => {
            block.render(area, buf);
            if loaded.is_some() {
                let dim =
                    Style::default().fg(readable(&state.theme, tokens::TEXT_DIM, tokens::BG_BASE));
                Paragraph::new(Line::styled("No transactions.", dim)).render(inner, buf);
            }
            return;
        }
    };

    // Less the horizontal padding and the selection marker.
    let inner_width = usize::from(area.width).saturating_sub(2 + SELECTION_MARKER_WIDTH);
    let tz = state.tz.clone();
    let today = state.today();
    // A band across the list, so a day stands out from its transactions. In the few
    // themes where the panel background is the base one, the blank row above still
    // sets it apart.
    let day_style = Style::default()
        .fg(state.theme.color(tokens::TEXT_SECONDARY).into())
        .bg(state.theme.color(tokens::BG_PANEL).into());

    // Build list items with a header for each day, on a band and set apart by a blank
    // row above it (rather than a rule, which is noisier). Track which list index
    // corresponds to each transaction so we can map the selection to the right row.
    let mut items: Vec<ListItem> = Vec::new();
    let mut txn_to_list_index: Vec<usize> = Vec::new();
    let mut last_date: Option<jiff::civil::Date> = None;
    let selected_id = state.selected_transaction().map(|txn| &txn.id);

    for txn in transactions {
        let zdt = txn.created_at.to_zoned_datetime(tz.clone());
        let date = zdt.date();

        if last_date != Some(date) {
            if last_date.is_some() {
                items.push(ListItem::new(Line::default()));
            }
            last_date = Some(date);
            items.push(ListItem::new(format_day(date, today)).style(day_style));
        }

        txn_to_list_index.push(items.len());
        // The selected row's dim text is on the selection's background.
        let row_bg = if selected_id == Some(&txn.id) {
            tokens::BG_SELECTION
        } else {
            tokens::BG_BASE
        };
        items.push(build_transaction_item(
            txn,
            &zdt,
            inner_width,
            state,
            row_bg,
        ));
    }

    let list = List::new(items)
        .block(block)
        .highlight_style(Style::default().bg(selection).add_modifier(Modifier::BOLD))
        .highlight_symbol(Line::styled(SELECTION_MARKER, accent))
        .highlight_spacing(HighlightSpacing::Always)
        .repeat_highlight_symbol(true);

    let Some((_, selection)) = state.list_mut() else {
        return;
    };
    let list_index = txn_to_list_index
        .get(selection.selected)
        .copied()
        .unwrap_or(0);
    selection.list_state.select(Some(list_index));
    StatefulWidget::render(list, area, buf, &mut selection.list_state);
}

/// The list's loading status, if it's loading, and the transactions it shows (the
/// filter's, or all the active tab's loaded ones), if they're loaded.
fn list_status(state: &AppState) -> (Option<&'static str>, Option<Vec<&Transaction>>) {
    let Some(tab) = state.current_tab() else {
        return (None, None);
    };
    // A filter shows the period's transactions, which are only there once loaded.
    let transactions = match &state.filter {
        Some(filter) => tab.store.recent().map(|_| filter.transactions(tab)),
        None => tab
            .store
            .transactions()
            .map(|txns| txns.iter().collect::<Vec<_>>()),
    };
    let status = match (tab.store.loading(), &transactions) {
        (None, _) => None,
        (Some(_), None) => Some("Loading…"),
        (Some(LoadKind::Replace), Some(_)) => Some("Refreshing…"),
        (Some(LoadKind::Append), Some(_)) => Some("Loading more…"),
    };
    (status, transactions)
}

/// The bar above a filtered list (and the detail pane): the category (in its colour)
/// and period it shows, and, on the right, how many transactions there are and their
/// total.
pub fn draw_filter_bar(buf: &mut Buffer, area: Rect, state: &AppState) {
    let (Some(filter), Some(tab)) = (&state.filter, state.current_tab()) else {
        return;
    };
    let theme = &state.theme;
    let fg = |token| Style::default().fg(theme.color(token).into());
    let category = filter.category.as_deref();

    let left = vec![
        Span::styled(" Spending on ", fg(tokens::TEXT_SECONDARY)),
        Span::styled(
            match category {
                Some(id) => state.category_path(Some(state.categories.root(id)), id),
                None => state.category_label(None).to_string(),
            },
            Style::default()
                .fg(state.category_color(category))
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(" {}", state.period.phrase(state.today())),
            fg(tokens::TEXT_SECONDARY),
        ),
    ];
    let txns = filter.transactions(tab);
    let zero = tab.account.balance.zero_like();
    let total = MoneyObject::sum(std::iter::once(&zero).chain(txns.iter().map(|t| &t.amount)));
    let count = match txns.len() {
        1 => "1 transaction".to_string(),
        n => format!("{n} transactions"),
    };
    let right = match total {
        Some(total) => format!("{count}, {} ", total.format_unsigned()),
        None => format!("{count} "),
    };

    let dim = readable(theme, tokens::TEXT_DIM, tokens::BG_HIGHLIGHT);
    let right = vec![Span::styled(right, Style::default().fg(dim))];
    Paragraph::new(right_aligned_if_room(left, right, usize::from(area.width)))
        .style(Style::default().bg(theme.color(tokens::BG_HIGHLIGHT).into()))
        .render(area, buf);
}

fn build_transaction_item<'a>(
    txn: &Transaction,
    zdt: &jiff::ZonedDateTime,
    inner_width: usize,
    state: &AppState,
    bg: &str,
) -> ListItem<'a> {
    let palette = &state.theme;
    let time = zdt.strftime("%H:%M").to_string();
    let amount = txn.amount.format_display();
    let amount_width = amount.width();
    // Leave at least one column between the description and the amount.
    let max_desc_width = inner_width.saturating_sub(amount_width + 1);
    let truncated_desc = truncate_to_width(&txn.description, max_desc_width);
    // Right-align the amount, measuring in terminal columns (not bytes or chars).
    let padding = inner_width.saturating_sub(truncated_desc.width() + amount_width);

    // Debits in the normal text colour, credits highlighted (as in the detail pane).
    let amount_style = if txn.amount.is_negative() {
        palette.color(tokens::TEXT_PRIMARY)
    } else {
        palette.color(tokens::SUCCESS)
    };

    let line1 = Line::from(vec![
        Span::styled(truncated_desc, palette.color(tokens::TEXT_PRIMARY)),
        Span::raw(" ".repeat(padding)),
        Span::styled(amount, amount_style),
    ]);
    // The category (if any) after a block in its root category's colour, under the
    // description, and the time on the right, under the amount. The blocks line up
    // down the list, to scan by; the name in that colour (red for Transport) would be
    // loud.
    let dim = readable(palette, tokens::TEXT_DIM, bg);
    let mut line2 = Vec::new();
    if let Some(id) = txn.category.as_deref().or(txn.parent_category.as_deref()) {
        let color = state.category_color(txn.root_category());
        // Leave at least one column before the time.
        let name_width = inner_width.saturating_sub(time.width() + 1 + 2);
        line2.push(Span::styled(CATEGORY_SYMBOL, color));
        line2.push(Span::raw(" "));
        line2.push(Span::styled(
            truncate_to_width(state.category_name(id), name_width),
            dim,
        ));
    }
    let left_width: usize = line2.iter().map(|span| span.content.width()).sum();
    line2.push(Span::raw(
        " ".repeat(inner_width.saturating_sub(left_width + time.width())),
    ));
    line2.push(Span::styled(time, dim));
    let line2 = Line::from(line2);

    ListItem::new(Text::from(vec![line1, line2]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::categories::{Categories, tests::named};
    use crate::app::state::{
        CategoryFilter, Period, TabState,
        tests::{account, money, txn},
    };

    #[test]
    fn the_selected_rows_dim_text_is_readable_on_the_selection() {
        // Nord's dim text is all but invisible on its selection.
        let mut state = AppState::new(Some("nord"));
        state.tz = jiff::tz::TimeZone::UTC;
        let mut tab = TabState::new(account("a", 0));
        tab.set_transactions(vec![
            txn("t0", "2020-11-20T18:00:00Z"),
            txn("t1", "2020-11-20T09:30:00Z"),
        ]);
        state.tabs.push(tab);

        let area = Rect::new(0, 0, 40, 10);
        let mut buf = Buffer::empty(area);
        draw_transaction_list(&mut buf, area, &mut state);
        // Rows 3 and 5 end with the times of the selected and the other transaction.
        assert_eq!(buf[(34, 3)].symbol(), "1");
        let on = |bg| readable(&state.theme, tokens::TEXT_DIM, bg);
        assert_eq!(buf[(34, 3)].fg, on(tokens::BG_SELECTION));
        assert_eq!(buf[(34, 5)].fg, on(tokens::BG_BASE));
        assert_ne!(on(tokens::BG_SELECTION), on(tokens::BG_BASE));
    }

    #[test]
    fn selected_row_is_marked_in_the_accent_colour() {
        let mut state = AppState::new(None);
        state.tz = jiff::tz::TimeZone::UTC;
        let mut tab = TabState::new(account("a", 0));
        tab.set_transactions(vec![txn("t0", "2026-11-20T18:00:00Z")]);
        state.tabs.push(tab);

        let area = Rect::new(0, 0, 40, 10);
        let mut buf = Buffer::empty(area);
        draw_transaction_list(&mut buf, area, &mut state);

        // Row 1 is the top padding, row 2 the day header, row 3 the transaction.
        let marker = &buf[(1, 3)];
        assert_eq!(marker.symbol(), "▎");
        assert_eq!(
            marker.fg,
            Color::from(state.theme.color(tokens::ACCENT_PRIMARY))
        );
    }

    #[test]
    fn days_have_a_header_band_with_a_blank_row_above_all_but_the_first() {
        let mut state = AppState::new(None);
        state.tz = jiff::tz::TimeZone::UTC;
        let mut tab = TabState::new(account("a", 0));
        tab.set_transactions(vec![
            txn("t0", "2020-11-20T18:00:00Z"),
            txn("t1", "2019-11-19T09:00:00Z"),
        ]);
        state.tabs.push(tab);

        let area = Rect::new(0, 0, 40, 10);
        let mut buf = Buffer::empty(area);
        draw_transaction_list(&mut buf, area, &mut state);
        let rows: Vec<String> = (1..8)
            .map(|y| {
                let row: String = (1..area.width).map(|x| buf[(x, y)].symbol()).collect();
                row.trim_end().to_string()
            })
            .collect();
        assert_eq!(rows[0], "  Fri 20 Nov 2020");
        // The band spans the list's width, including the selection marker's column.
        let panel = Color::from(state.theme.color(tokens::BG_PANEL));
        assert!((1..area.width - 1).all(|x| buf[(x, 1)].bg == panel));
        assert_ne!(buf[(1, 2)].bg, panel);
        assert!(rows[1].contains("Transaction t0"));
        assert_eq!(rows[3], "");
        assert_eq!(rows[4], "  Tue 19 Nov 2019");
        assert!(rows[5].contains("Transaction t1"));
    }

    #[test]
    fn the_second_row_shows_the_category_with_a_block_and_the_time_on_the_right() {
        let mut state = AppState::new(None);
        state.tz = jiff::tz::TimeZone::UTC;
        state.categories = Categories::new([
            named("home", "Home", None),
            named("groceries", "Groceries", Some("home")),
        ]);
        let mut tab = TabState::new(account("a", 0));
        tab.set_transactions(vec![
            Transaction {
                category: Some("groceries".to_string()),
                parent_category: Some("home".to_string()),
                ..txn("t0", "2020-11-20T18:00:00Z")
            },
            txn("t1", "2020-11-20T09:30:00Z"),
        ]);
        state.tabs.push(tab);

        let area = Rect::new(0, 0, 40, 10);
        let mut buf = Buffer::empty(area);
        draw_transaction_list(&mut buf, area, &mut state);
        let row = |y| -> String {
            let row: String = (3..area.width).map(|x| buf[(x, y)].symbol()).collect();
            row.trim_end().to_string()
        };
        // The time is right-aligned, under the amount.
        assert_eq!(
            row(3),
            format!("{CATEGORY_SYMBOL} Groceries{}18:00", " ".repeat(20))
        );
        assert_eq!(buf[(3, 3)].fg, state.category_color(Some("home")));
        // Without a category, there's only the time.
        assert_eq!(row(5), format!("{}09:30", " ".repeat(31)));
    }

    #[test]
    fn a_filtered_list_shows_only_the_category_with_a_bar_above() {
        let mut state = AppState::new(None);
        state.tz = jiff::tz::TimeZone::UTC;
        state.categories = Categories::new([
            named("home", "Home", None),
            named("groceries", "Groceries", Some("home")),
        ]);
        let now = jiff::Timestamp::now();
        let at = |hours: i64| (now - jiff::SignedDuration::from_hours(hours)).to_string();
        let groceries = |id, hours, cents| Transaction {
            category: Some("groceries".to_string()),
            parent_category: Some("home".to_string()),
            amount: money(cents),
            ..txn(id, &at(hours))
        };
        let mut tab = TabState::new(account("a", 0));
        tab.set_transactions(vec![
            groceries("g1", 1, -1250),
            txn("other", &at(2)),
            groceries("g2", 3, -2000),
            // A refund isn't spending.
            groceries("refund", 4, 500),
        ]);
        state.tabs.push(tab);
        state.filter = Some(CategoryFilter::new(Some("groceries".to_string())));
        // This month's name changes; the transactions are within 30 days.
        state.period = Period::Last30Days;

        let area = Rect::new(0, 0, 80, 12);
        let mut buf = Buffer::empty(area);
        let [bar, list] =
            Layout::vertical([Constraint::Length(1), Constraint::Fill(1)]).areas(area);
        draw_filter_bar(&mut buf, bar, &state);
        draw_transaction_list(&mut buf, list, &mut state);
        let rows: Vec<String> = (0..area.height)
            .map(|y| (0..area.width).map(|x| buf[(x, y)].symbol()).collect())
            .collect();
        assert_eq!(
            rows[0].trim_end(),
            " Spending on Home › Groceries in the last 30 days        2 transactions, $32.50"
        );
        let shown = rows.join("\n");
        assert!(shown.contains("Transaction g1") && shown.contains("Transaction g2"));
        assert!(!shown.contains("Transaction other") && !shown.contains("refund"));
    }

    #[test]
    fn the_title_is_only_the_loading_status() {
        let mut state = AppState::new(None);
        let mut tab = TabState::new(account("a", 0));
        tab.set_transactions(Vec::new());
        state.tabs.push(tab);
        let area = Rect::new(0, 0, 40, 6);
        let render = |state: &mut AppState| {
            let mut buf = Buffer::empty(area);
            draw_transaction_list(&mut buf, area, state);
            (0..area.height)
                .map(|y| {
                    let row: String = (0..area.width).map(|x| buf[(x, y)].symbol()).collect();
                    row.trim_end().to_string()
                })
                .collect::<Vec<_>>()
        };

        // Loaded and empty: no title, and a message in the list.
        let rows = render(&mut state);
        assert_eq!(rows[0], "");
        assert_eq!(rows[1], " No transactions.");

        // Refreshing.
        state.tabs[0]
            .store
            .refresh("2026-11-01T00:00:00Z".parse().unwrap());
        assert_eq!(render(&mut state)[0], " Refreshing…");
    }
}
