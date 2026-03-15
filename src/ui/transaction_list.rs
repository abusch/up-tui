use chrono::Local;
use ratatui::layout::{Constraint, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, Borders, Cell, Row, Table, TableState};
use ratatui::Frame;

use crate::api::models::Transaction;
use crate::app::state::AppState;

pub fn draw_transaction_list(f: &mut Frame, area: Rect, state: &AppState) {
    let tab = match state.current_tab() {
        Some(t) => t,
        None => {
            let block = Block::default()
                .borders(Borders::ALL)
                .title(" Transactions ");
            f.render_widget(block, area);
            return;
        }
    };

    if tab.loading && tab.transactions.is_none() {
        let block = Block::default()
            .borders(Borders::ALL)
            .title(" Transactions - Loading... ");
        f.render_widget(block, area);
        return;
    }

    let transactions = match &tab.transactions {
        Some(t) => t,
        None => {
            let block = Block::default()
                .borders(Borders::ALL)
                .title(" Transactions ");
            f.render_widget(block, area);
            return;
        }
    };

    if transactions.is_empty() {
        let block = Block::default()
            .borders(Borders::ALL)
            .title(" Transactions - No transactions ");
        f.render_widget(block, area);
        return;
    }

    let header = Row::new(vec![
        Cell::from("Date"),
        Cell::from("Description"),
        Cell::from("Amount"),
        Cell::from("Status"),
    ])
    .style(
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
    )
    .height(1);

    let rows: Vec<Row> = transactions
        .iter()
        .enumerate()
        .map(|(i, txn)| {
            let date = format_date(txn);
            let amount = format_amount(txn);
            let amount_color = if txn.amount.value_in_base_units >= 0 {
                Color::Green
            } else {
                Color::Red
            };

            let style = if i == tab.selected {
                Style::default()
                    .bg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };

            Row::new(vec![
                Cell::from(date),
                Cell::from(txn.description.clone()),
                Cell::from(amount).style(Style::default().fg(amount_color)),
                Cell::from(txn.status.to_string()),
            ])
            .style(style)
        })
        .collect();

    let title = if tab.loading {
        " Transactions - Refreshing... "
    } else {
        " Transactions "
    };

    let widths = [
        Constraint::Length(14),
        Constraint::Fill(1),
        Constraint::Length(12),
        Constraint::Length(6),
    ];

    let table = Table::new(rows, widths)
        .header(header)
        .block(Block::default().borders(Borders::ALL).title(title))
        .row_highlight_style(
            Style::default()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        );

    let mut table_state = TableState::default();
    table_state.select(Some(tab.selected));
    f.render_stateful_widget(table, area, &mut table_state);
}

fn format_date(txn: &Transaction) -> String {
    let dt = txn.created_at.with_timezone(&Local);
    dt.format("%d %b %H:%M").to_string()
}

fn format_amount(txn: &Transaction) -> String {
    let cents = txn.amount.value_in_base_units;
    let abs = (cents.unsigned_abs() as f64) / 100.0;
    if cents >= 0 {
        format!("+${:.2}", abs)
    } else {
        format!("-${:.2}", abs)
    }
}
