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

    // 2 for borders + 1 for highlight symbol ("▎")
    let inner_width = area.width.saturating_sub(4) as usize;
    let tz = TimeZone::system();

    // Build list items with day separators. Track which list index corresponds
    // to each transaction so we can map `tab.selected` to the right row.
    let mut items: Vec<ListItem> = Vec::new();
    let mut txn_to_list_index: Vec<usize> = Vec::new();
    let mut last_date: Option<jiff::civil::Date> = None;

    for (i, txn) in transactions.iter().enumerate() {
        let zdt = txn.created_at.to_zoned(tz.clone());
        let date = zdt.date();

        if last_date != Some(date) {
            last_date = Some(date);
            let label = zdt.strftime("%a %-d %b").to_string().to_uppercase();
            let padded = format!("{:<width$}", label, width = inner_width);
            items.push(ListItem::new(Line::from(Span::styled(
                padded,
                Style::default()
                    .fg(palette.bg)
                    .bg(palette.muted)
                    .add_modifier(Modifier::BOLD),
            ))));
        }

        txn_to_list_index.push(items.len());
        items.push(build_transaction_item(txn, &zdt, inner_width, palette));
        let _ = i;
    }

    let loading = state.current_tab().unwrap().loading;
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
        )
        .highlight_symbol(Line::from(Span::styled(
            "▎ ",
            Style::default().fg(palette.accent),
        )))
        .highlight_spacing(ratatui::widgets::HighlightSpacing::Always)
        .repeat_highlight_symbol(true);

    let tab = state.current_tab_mut().unwrap();
    let list_index = txn_to_list_index.get(tab.selected).copied().unwrap_or(0);
    tab.list_state.select(Some(list_index));
    f.render_stateful_widget(list, area, &mut tab.list_state);
}

fn build_transaction_item<'a>(
    txn: &Transaction,
    zdt: &jiff::Zoned,
    inner_width: usize,
    palette: ratatui_themes::ThemePalette,
) -> ListItem<'a> {
    let time = zdt.strftime("%H:%M").to_string();
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
    let line2 = Line::from(Span::styled(time, Style::default().fg(palette.muted)));

    ListItem::new(Text::from(vec![line1, line2]))
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
