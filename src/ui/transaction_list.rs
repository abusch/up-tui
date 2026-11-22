use opaline::names::tokens;
use ratatui::{
    prelude::*,
    widgets::{Block, HighlightSpacing, List, ListItem, Padding},
};

use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::app::state::{AppState, LoadKind};
use crate::client::models::Transaction;

pub fn draw_transaction_list(buf: &mut Buffer, area: Rect, state: &mut AppState) {
    let fg = state.theme.color(tokens::TEXT_PRIMARY).into();
    let bg = state.theme.color(tokens::BG_BASE).into();
    let accent = state.theme.color(tokens::ACCENT_PRIMARY);
    let selection = state.theme.color(tokens::BG_SELECTION).into();

    let base_style = Style::default().fg(fg).bg(bg);
    let block = |title: &'static str| {
        Block::default()
            .title(title)
            .title_style(accent)
            .padding(Padding::uniform(1))
            .style(base_style)
    };

    // Record how many transactions fit in the list, for Ctrl+D/Ctrl+U paging and for
    // deciding when to load the next page. Each transaction item is 2 rows tall; day
    // separator rows are ignored, so this is an upper bound.
    state.page_size = usize::from(block(" Transactions ").inner(area).height / 2);

    let title;
    let transactions = match state.current_tab() {
        Some(tab) if tab.loading.is_some() && tab.transactions.is_none() => {
            title = " Transactions - Loading... ";
            None
        }
        Some(tab) => match &tab.transactions {
            Some(txns) if !txns.is_empty() => {
                title = match tab.loading {
                    Some(LoadKind::Replace) => " Transactions - Refreshing... ",
                    Some(LoadKind::Append) => " Transactions - Loading more... ",
                    None => " Transactions ",
                };
                Some(txns)
            }
            Some(_) => {
                title = " Transactions - No transactions ";
                None
            }
            None => {
                title = " Transactions ";
                None
            }
        },
        None => {
            title = " Transactions ";
            None
        }
    };

    let transactions = match transactions {
        Some(txns) => txns,
        None => {
            block(title).render(area, buf);
            return;
        }
    };

    // 2 for horizontal padding + 2 for highlight symbol ("▎ ")
    let inner_width = area.width.saturating_sub(4) as usize;
    let tz = state.tz.clone();

    // Build list items with day separators. Track which list index corresponds
    // to each transaction so we can map `tab.selected` to the right row.
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
        .block(block(title))
        .highlight_style(Style::default().bg(selection).add_modifier(Modifier::BOLD))
        .highlight_symbol(Line::styled("▎ ", accent))
        .highlight_spacing(HighlightSpacing::Always)
        .repeat_highlight_symbol(true);

    let tab = state.current_tab_mut().unwrap();
    let list_index = txn_to_list_index.get(tab.selected).copied().unwrap_or(0);
    tab.list_state.select(Some(list_index));
    StatefulWidget::render(list, area, buf, &mut tab.list_state);
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
fn truncate_to_width(s: &str, max_width: usize) -> String {
    if s.width() <= max_width {
        return s.to_string();
    }
    if max_width == 0 {
        return String::new();
    }
    let budget = max_width - 1; // reserve one column for the ellipsis
    let mut out = String::new();
    let mut width = 0;
    for c in s.chars() {
        let w = c.width().unwrap_or(0);
        if width + w > budget {
            break;
        }
        out.push(c);
        width += w;
    }
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::state::{
        TabState,
        tests::{account, txn},
    };

    #[test]
    fn selected_row_is_marked_in_the_accent_colour() {
        let mut state = AppState::new(None);
        state.tz = jiff::tz::TimeZone::UTC;
        let mut tab = TabState::new(account("a", 0));
        tab.set_transactions(vec![txn("t0", "2026-11-20T18:00:00Z")], None);
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
    fn zero_width_yields_empty() {
        assert_eq!(truncate_to_width("abc", 0), "");
    }
}
