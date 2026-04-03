use jiff::tz::TimeZone;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, List, ListItem};

use crate::app::state::AppState;
use up_api::models::Transaction;

pub fn draw_transaction_list(f: &mut Frame, area: Rect, state: &mut AppState) {
    let palette = state.palette();
    let base_style = Style::default().fg(palette.fg).bg(palette.bg);

    let tab = match state.current_tab() {
        Some(t) => t,
        None => {
            let block = Block::default()
                .borders(Borders::ALL)
                .title(" Transactions ")
                .style(base_style);
            f.render_widget(block, area);
            return;
        }
    };

    if tab.loading && tab.transactions.is_none() {
        let block = Block::default()
            .borders(Borders::ALL)
            .title(" Transactions - Loading... ")
            .style(base_style);
        f.render_widget(block, area);
        return;
    }

    let transactions = match &tab.transactions {
        Some(t) => t,
        None => {
            let block = Block::default()
                .borders(Borders::ALL)
                .title(" Transactions ")
                .style(base_style);
            f.render_widget(block, area);
            return;
        }
    };

    if transactions.is_empty() {
        let block = Block::default()
            .borders(Borders::ALL)
            .title(" Transactions - No transactions ")
            .style(base_style);
        f.render_widget(block, area);
        return;
    }

    let inner_width = area.width.saturating_sub(2) as usize;

    let items: Vec<ListItem> = transactions
        .iter()
        .map(|txn| {
            let date = format_date(txn);
            let amount = format_amount(txn);
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
                Style::default().fg(palette.success)
            } else {
                Style::default().fg(palette.fg)
            };

            let line1 = Line::from(vec![
                Span::raw(truncated_desc),
                Span::raw(" ".repeat(padding)),
                Span::styled(amount, amount_style),
            ]);
            let line2 = Line::from(Span::styled(date, Style::default().fg(palette.muted)));

            ListItem::new(Text::from(vec![line1, line2]))
        })
        .collect();

    let loading = tab.loading;
    let title = if loading {
        " Transactions - Refreshing... "
    } else {
        " Transactions "
    };

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(title)
                .style(base_style),
        )
        .highlight_style(
            Style::default()
                .bg(palette.selection)
                .fg(palette.fg)
                .add_modifier(Modifier::BOLD),
        );

    // Use the persisted ListState from TabState so that the scroll offset
    // is preserved across renders, avoiding jumpy scrolling.
    let tab = state.current_tab_mut().unwrap();
    tab.list_state.select(Some(tab.selected));
    f.render_stateful_widget(list, area, &mut tab.list_state);
}

fn format_date(txn: &Transaction) -> String {
    let zdt = txn.created_at.to_zoned(TimeZone::system());
    zdt.strftime("%d %b %H:%M").to_string()
}

fn format_amount(txn: &Transaction) -> String {
    let cents = txn.amount.value_in_base_units;
    let abs = (cents.unsigned_abs() as f64) / 100.0;
    if cents >= 0 {
        format!("+${:.2}", abs)
    } else {
        format!("${:.2}", abs)
    }
}
