use opaline::names::tokens;
use ratatui::{
    prelude::*,
    widgets::{Block, HighlightSpacing, List, ListItem, Padding, Paragraph},
};

use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use crate::app::state::{AppState, RECENT_DAYS};
use crate::app::store::LoadKind;
use crate::client::models::{MoneyObject, Transaction};

/// The list's block, without its style.
fn list_block() -> Block<'static> {
    Block::default().padding(Padding::uniform(1))
}

/// How many transactions fit in a list drawn in `area`, for Ctrl+D/Ctrl+U paging and
/// for deciding when to load the next page. Each transaction item is 2 rows tall; day
/// separator rows are ignored, so this is an upper bound.
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
                let dim = Style::default().fg(state.theme.color(tokens::TEXT_DIM).into());
                Paragraph::new(Line::styled("No transactions.", dim)).render(inner, buf);
            }
            return;
        }
    };

    // 2 for horizontal padding + 2 for highlight symbol ("▎ ")
    let inner_width = area.width.saturating_sub(4) as usize;
    let tz = state.tz.clone();

    // Build list items with day separators. Track which list index corresponds
    // to each transaction so we can map the selection to the right row.
    let mut items: Vec<ListItem> = Vec::new();
    let mut txn_to_list_index: Vec<usize> = Vec::new();
    let mut last_date: Option<jiff::civil::Date> = None;

    for txn in transactions {
        let zdt = txn.created_at.to_zoned_datetime(tz.clone());
        let date = zdt.date();

        if last_date != Some(date) {
            last_date = Some(date);
            let label = zdt.strftime("%a %-d %b ").to_string().to_uppercase();
            let padded = format!("{:┄<width$}", label, width = inner_width);
            items.push(ListItem::new(Line::from(Span::styled(
                padded,
                Style::default()
                    .fg(state.theme.color(tokens::TEXT_DIM).into())
                    .bg(bg)
                    .add_modifier(Modifier::BOLD),
            ))));
        }

        txn_to_list_index.push(items.len());
        items.push(build_transaction_item(txn, &zdt, inner_width, &state.theme));
    }

    let list = List::new(items)
        .block(block)
        .highlight_style(Style::default().bg(selection).add_modifier(Modifier::BOLD))
        .highlight_symbol(Line::styled("▎ ", accent))
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
    // A filter shows the recent transactions, which are only there once loaded.
    let transactions = match &state.filter {
        Some(filter) => tab.store.recent().map(|_| filter.transactions(tab)),
        None => tab
            .store
            .transactions()
            .map(|txns| txns.iter().collect::<Vec<_>>()),
    };
    let status = match (tab.store.loading(), &transactions) {
        (None, _) => None,
        (Some(_), None) => Some("Loading..."),
        (Some(LoadKind::Replace), Some(_)) => Some("Refreshing..."),
        (Some(LoadKind::Append), Some(_)) => Some("Loading more..."),
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
            category_path(category, state),
            Style::default()
                .fg(state.category_color(category))
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(" in the last {RECENT_DAYS} days"),
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

    // The right part is left out if it doesn't fit.
    let left_width: usize = left.iter().map(|s| s.content.width()).sum();
    let width = usize::from(area.width);
    let mut spans = left;
    if left_width + 1 + right.width() <= width {
        spans.push(Span::raw(" ".repeat(width - left_width - right.width())));
        spans.push(Span::styled(right, fg(tokens::TEXT_DIM)));
    }
    Paragraph::new(Line::from(spans))
        .style(Style::default().bg(theme.color(tokens::BG_HIGHLIGHT).into()))
        .render(area, buf);
}

/// A category's name with its parent's, like the detail pane's
/// ("Good Life › Restaurants & Cafes"), or "Uncategorised".
fn category_path(category: Option<&str>, state: &AppState) -> String {
    let Some(id) = category else {
        return "Uncategorised".to_string();
    };
    let root = state.categories.root(id);
    if root == id {
        state.category_name(id).to_string()
    } else {
        format!(
            "{} › {}",
            state.category_name(root),
            state.category_name(id)
        )
    }
}

fn build_transaction_item<'a>(
    txn: &Transaction,
    zdt: &jiff::ZonedDateTime,
    inner_width: usize,
    palette: &opaline::Theme,
) -> ListItem<'a> {
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
    let line2 = Line::from(Span::styled(time, palette.color(tokens::TEXT_DIM)));

    ListItem::new(Text::from(vec![line1, line2]))
}

/// Truncate `s` to at most `max_width` terminal columns, appending `…` if anything was cut.
pub(super) fn truncate_to_width(s: &str, max_width: usize) -> String {
    if s.width() <= max_width {
        return s.to_string();
    }
    if max_width == 0 {
        return String::new();
    }
    let budget = max_width - 1; // reserve one column for the ellipsis
    let mut out = String::new();
    let mut width = 0;
    // Measure whole graphemes, as ratatui does: an emoji followed by a variation
    // selector is wider than its characters' widths added up, and mustn't be split.
    for grapheme in s.graphemes(true) {
        let w = grapheme.width();
        if width + w > budget {
            break;
        }
        out.push_str(grapheme);
        width += w;
    }
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::categories::{Categories, tests::named};
    use crate::app::state::{
        CategoryFilter, TabState,
        tests::{account, money, txn},
    };

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

        // Row 1 is the top padding, row 2 the day separator, row 3 the transaction.
        let marker = &buf[(1, 3)];
        assert_eq!(marker.symbol(), "▎");
        assert_eq!(
            marker.fg,
            Color::from(state.theme.color(tokens::ACCENT_PRIMARY))
        );
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
        assert_eq!(render(&mut state)[0], " Refreshing...");
    }

    #[test]
    fn short_strings_are_unchanged() {
        assert_eq!(truncate_to_width("Café", 10), "Café");
        assert_eq!(truncate_to_width("Café", 4), "Café");
    }

    #[test]
    fn truncation_fits_width_exactly() {
        let s = truncate_to_width("Woolworths Metro", 8);
        assert_eq!(s, "Woolwor…");
        assert_eq!(s.width(), 8);
    }

    #[test]
    fn multibyte_and_wide_characters_are_measured_in_columns() {
        assert_eq!(truncate_to_width("Café Allongé", 6), "Café …");
        // Each CJK character is 2 columns wide; a split one is dropped, not cut in half.
        let s = truncate_to_width("寿司寿司", 6);
        assert_eq!(s, "寿司…");
        assert!(s.width() <= 6);
    }

    #[test]
    fn emoji_with_a_selector_are_measured_whole() {
        // "🏖\u{FE0F}" is 2 columns, though its characters are 1 and 0 columns wide.
        let s = truncate_to_width("🏖\u{FE0F}🏖\u{FE0F} Beach", 4);
        assert_eq!(s, "🏖\u{FE0F}…");
        assert_eq!(s.width(), 3);
        assert_eq!(truncate_to_width("a🏖\u{FE0F}bc", 3), "a…");
    }

    #[test]
    fn zero_width_yields_empty() {
        assert_eq!(truncate_to_width("abc", 0), "");
    }
}
