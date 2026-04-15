use jiff::tz::TimeZone;
use opaline::names::tokens;
use ratatui::prelude::*;
use ratatui::widgets::{Block, BorderType, Borders, HighlightSpacing, List, ListItem};

use crate::app::state::AppState;
use crate::client::models::Transaction;

pub fn draw_transaction_list(buf: &mut Buffer, area: Rect, state: &mut AppState) {
    // Record visible page size for Ctrl+D/Ctrl+U scrolling in the key handler.
    // Subtract 2 for borders, divide by 2 because each transaction item is 2 lines tall.
    state.list_height = area.height.saturating_sub(2) / 2;

    let fg = state.theme.color(tokens::TEXT_PRIMARY).into();
    let bg = state.theme.color(tokens::BG_BASE).into();
    let muted = state.theme.color(tokens::TEXT_MUTED).into();
    let accent = state.theme.color(tokens::ACCENT_PRIMARY).into();
    let selection = state.theme.color(tokens::BG_SELECTION).into();
    let border = state.theme.color(tokens::BORDER_FOCUSED);

    let base_style = Style::default().fg(fg).bg(bg);

    let title;
    let transactions = match state.current_tab() {
        Some(tab) if tab.loading && tab.transactions.is_none() => {
            title = " Transactions - Loading... ";
            None
        }
        Some(tab) => match &tab.transactions {
            Some(txns) if !txns.is_empty() => {
                title = if tab.loading {
                    " Transactions - Refreshing... "
                } else {
                    " Transactions "
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
            let block = Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(border)
                .title(title)
                .title_style(accent)
                .style(base_style);
            block.render(area, buf);
            return;
        }
    };

    // 2 for borders + 2 for highlight symbol ("▎ ")
    let inner_width = area.width.saturating_sub(4) as usize;
    let tz = TimeZone::system();

    // Build list items with day separators. Track which list index corresponds
    // to each transaction so we can map `tab.selected` to the right row.
    let mut items: Vec<ListItem> = Vec::new();
    let mut txn_to_list_index: Vec<usize> = Vec::new();
    let mut last_date: Option<jiff::civil::Date> = None;

    for txn in transactions {
        let zdt = txn.created_at.to_zoned(tz.clone());
        let date = zdt.date();

        if last_date != Some(date) {
            last_date = Some(date);
            let label = zdt.strftime("%a %-d %b").to_string().to_uppercase();
            let padded = format!("{:<width$}", label, width = inner_width);
            items.push(ListItem::new(Line::from(Span::styled(
                padded,
                Style::default()
                    .fg(bg)
                    .bg(muted)
                    .add_modifier(Modifier::BOLD),
            ))));
        }

        txn_to_list_index.push(items.len());
        items.push(build_transaction_item(txn, &zdt, inner_width, &state.theme));
    }

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(border)
                .title(title)
                .title_style(accent)
                .style(base_style),
        )
        .highlight_style(Style::default().bg(selection).add_modifier(Modifier::BOLD))
        .highlight_symbol(Line::from(Span::styled("▎ ", Style::default().fg(accent))))
        .highlight_spacing(HighlightSpacing::Always)
        .repeat_highlight_symbol(true);

    let tab = state.current_tab_mut().unwrap();
    let list_index = txn_to_list_index.get(tab.selected).copied().unwrap_or(0);
    tab.list_state.select(Some(list_index));
    StatefulWidget::render(list, area, buf, &mut tab.list_state);
}

fn build_transaction_item<'a>(
    txn: &Transaction,
    zdt: &jiff::Zoned,
    inner_width: usize,
    palette: &opaline::Theme,
) -> ListItem<'a> {
    let time = zdt.strftime("%H:%M").to_string();
    let amount = txn.amount.format_display();
    let amount_len = amount.len();
    let max_desc_len = inner_width.saturating_sub(amount_len + 1);

    let desc = &txn.description;
    let truncated_desc: String = if desc.chars().count() > max_desc_len {
        let mut s: String = desc.chars().take(max_desc_len.saturating_sub(1)).collect();
        s.push('…');
        s
    } else {
        desc.clone()
    };

    let padding = inner_width.saturating_sub(truncated_desc.len() + amount_len);

    let amount_style = if txn.amount.value_in_base_units >= 0 {
        palette.color(tokens::SUCCESS)
    } else {
        palette.color(tokens::CODE_NUMBER)
    };

    let line1 = Line::from(vec![
        Span::raw(truncated_desc),
        Span::raw(" ".repeat(padding)),
        Span::styled(amount, amount_style),
    ]);
    let line2 = Line::from(Span::styled(time, palette.color(tokens::TEXT_DIM)));

    ListItem::new(Text::from(vec![line1, line2]))
}
