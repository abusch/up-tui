use jiff::tz::TimeZone;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph, Wrap};

use crate::app::state::AppState;

pub fn draw_detail_pane(f: &mut Frame, area: Rect, state: &AppState) {
    let palette = state.palette();
    let base_style = Style::default().fg(palette.fg).bg(palette.bg);

    let tab = match state.current_tab() {
        Some(t) => t,
        None => {
            let block = Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .title(" Detail ")
                .style(base_style);
            f.render_widget(block, area);
            return;
        }
    };

    let txn = match &tab.transactions {
        Some(txns) => match txns.get(tab.selected) {
            Some(t) => t,
            None => {
                let block = Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .title(" Detail ")
                    .style(base_style);
                f.render_widget(block, area);
                return;
            }
        },
        None => {
            let block = Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .title(" Detail ")
                .style(base_style);
            f.render_widget(block, area);
            return;
        }
    };

    let label_style = Style::default()
        .fg(palette.secondary)
        .add_modifier(Modifier::BOLD);
    let value_style = Style::default().fg(palette.fg);

    let mut lines = Vec::new();

    add_field(
        &mut lines,
        "Description",
        &txn.description,
        label_style,
        value_style,
    );

    if let Some(ref raw) = txn.raw_text {
        add_field(&mut lines, "Raw Text", raw, label_style, value_style);
    }

    let amount_color = if txn.amount.value_in_base_units >= 0 {
        palette.success
    } else {
        palette.error
    };
    lines.push(Line::from(vec![
        Span::styled("Amount:       ", label_style),
        Span::styled(
            txn.amount.format_display(true),
            Style::default().fg(amount_color),
        ),
    ]));

    add_field(
        &mut lines,
        "Status",
        &txn.status.to_string(),
        label_style,
        value_style,
    );

    add_field(
        &mut lines,
        "Created",
        &txn.created_at
            .to_zoned(TimeZone::system())
            .strftime("%Y-%m-%d %H:%M:%S")
            .to_string(),
        label_style,
        value_style,
    );

    if let Some(ref settled) = txn.settled_at {
        add_field(
            &mut lines,
            "Settled",
            &settled
                .to_zoned(TimeZone::system())
                .strftime("%Y-%m-%d %H:%M:%S")
                .to_string(),
            label_style,
            value_style,
        );
    }

    if let Some(ref msg) = txn.message {
        add_field(&mut lines, "Message", msg, label_style, value_style);
    }

    if let Some(ref foreign) = txn.foreign_amount {
        add_field(
            &mut lines,
            "Foreign Amount",
            &format!("{} {}", foreign.value, foreign.currency_code),
            label_style,
            value_style,
        );
    }

    if let Some(ref round_up) = txn.round_up {
        add_field(
            &mut lines,
            "Round Up",
            &round_up.amount.value,
            label_style,
            value_style,
        );
        if let Some(ref boost) = round_up.boost_portion {
            add_field(
                &mut lines,
                "Boost Portion",
                &boost.value,
                label_style,
                value_style,
            );
        }
    }

    if let Some(ref cashback) = txn.cashback {
        add_field(
            &mut lines,
            "Cashback",
            &format!("{} ({})", cashback.amount.value, cashback.description),
            label_style,
            value_style,
        );
    }

    if let Some(ref card) = txn.card_purchase_method {
        let suffix = card.card_number_suffix.as_deref().unwrap_or("****");
        add_field(
            &mut lines,
            "Card Method",
            &format!("{} (****{})", card.method, suffix),
            label_style,
            value_style,
        );
    }

    if txn.category.is_some() || txn.parent_category.is_some() {
        let display = match (&txn.parent_category, &txn.category) {
            (Some(parent), Some(cat)) => {
                format!(
                    "{} / {}",
                    state.category_name(parent),
                    state.category_name(cat)
                )
            }
            (None, Some(cat)) => state.category_name(cat).to_string(),
            (Some(parent), None) => state.category_name(parent).to_string(),
            (None, None) => unreachable!(),
        };
        add_field(&mut lines, "Category", &display, label_style, value_style);
    }

    if !txn.tags.is_empty() {
        add_field(
            &mut lines,
            "Tags",
            &txn.tags.join(", "),
            label_style,
            value_style,
        );
    }

    let paragraph = Paragraph::new(lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .title(" Detail ")
                .style(base_style),
        )
        .wrap(Wrap { trim: false });

    f.render_widget(paragraph, area);
}

fn add_field(
    lines: &mut Vec<Line<'_>>,
    label: &str,
    value: &str,
    label_style: Style,
    value_style: Style,
) {
    let padded_label = format!(
        "{label}:{:>pad$}",
        "",
        pad = 13usize.saturating_sub(label.len())
    );
    lines.push(Line::from(vec![
        Span::styled(padded_label, label_style),
        Span::styled(value.to_string(), value_style),
    ]));
}
