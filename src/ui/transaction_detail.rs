use chrono::Local;
use ratatui::layout::{Constraint, Flex, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};
use ratatui::Frame;

use crate::api::models::Transaction;
use crate::app::state::AppState;

pub fn draw_detail_overlay(f: &mut Frame, state: &AppState) {
    let tab = match state.current_tab() {
        Some(t) => t,
        None => return,
    };

    let txn = match &tab.transactions {
        Some(txns) => match txns.get(tab.selected) {
            Some(t) => t,
            None => return,
        },
        None => return,
    };

    let area = centered_rect(60, 70, f.area());

    f.render_widget(Clear, area);

    let mut lines = Vec::new();

    add_field(&mut lines, "Description", &txn.description);

    if let Some(ref raw) = txn.raw_text {
        add_field(&mut lines, "Raw Text", raw);
    }

    let amount_color = if txn.amount.value_in_base_units >= 0 {
        Color::Green
    } else {
        Color::Red
    };
    lines.push(Line::from(vec![
        Span::styled(
            "Amount:       ",
            Style::default().add_modifier(Modifier::BOLD),
        ),
        Span::styled(format_amount(txn), Style::default().fg(amount_color)),
    ]));

    add_field(&mut lines, "Status", &txn.status.to_string());

    add_field(
        &mut lines,
        "Created",
        &txn.created_at
            .with_timezone(&Local)
            .format("%Y-%m-%d %H:%M:%S")
            .to_string(),
    );

    if let Some(ref settled) = txn.settled_at {
        add_field(
            &mut lines,
            "Settled",
            &settled
                .with_timezone(&Local)
                .format("%Y-%m-%d %H:%M:%S")
                .to_string(),
        );
    }

    if let Some(ref msg) = txn.message {
        add_field(&mut lines, "Message", msg);
    }

    if let Some(ref foreign) = txn.foreign_amount {
        add_field(
            &mut lines,
            "Foreign Amount",
            &format!("{} {}", foreign.value, foreign.currency_code),
        );
    }

    if let Some(ref round_up) = txn.round_up {
        add_field(&mut lines, "Round Up", &round_up.amount.value);
        if let Some(ref boost) = round_up.boost_portion {
            add_field(&mut lines, "Boost Portion", &boost.value);
        }
    }

    if let Some(ref cashback) = txn.cashback {
        add_field(
            &mut lines,
            "Cashback",
            &format!("{} ({})", cashback.amount.value, cashback.description),
        );
    }

    if let Some(ref card) = txn.card_purchase_method {
        let suffix = card
            .card_number_suffix
            .as_deref()
            .unwrap_or("****");
        add_field(
            &mut lines,
            "Card Method",
            &format!("{} (****{})", card.method, suffix),
        );
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "Press Esc or q to close",
        Style::default().fg(Color::DarkGray),
    )));

    let paragraph = Paragraph::new(lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Transaction Detail ")
                .style(Style::default().fg(Color::White)),
        )
        .wrap(Wrap { trim: false });

    f.render_widget(paragraph, area);
}

fn add_field(lines: &mut Vec<Line<'_>>, label: &str, value: &str) {
    let padded_label = format!("{:14}", format!("{}:", label));
    lines.push(Line::from(vec![
        Span::styled(padded_label, Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(value.to_string()),
    ]));
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

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let [area] = Layout::vertical([Constraint::Percentage(percent_y)])
        .flex(Flex::Center)
        .areas(r);
    let [area] = Layout::horizontal([Constraint::Percentage(percent_x)])
        .flex(Flex::Center)
        .areas(area);
    area
}
