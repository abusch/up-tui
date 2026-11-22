use opaline::names::tokens;
use ratatui::{
    prelude::*,
    widgets::{Block, Padding, Paragraph, Wrap},
};

use crate::app::state::AppState;
use crate::client::models::TransactionStatus;

pub fn draw_detail_pane(buf: &mut Buffer, area: Rect, state: &AppState) {
    let fg = state.theme.color(tokens::TEXT_PRIMARY).into();
    let bg = state.theme.color(tokens::BG_CODE).into();
    let secondary = state.theme.color(tokens::TEXT_SECONDARY).into();
    let warning = state.theme.color(tokens::WARNING).into();
    let success = state.theme.color(tokens::SUCCESS).into();

    let block = Block::default()
        .title(" Detail ")
        .style(Style::default().fg(fg).bg(bg))
        .padding(Padding::uniform(1));

    let Some(txn) = state
        .current_tab()
        .and_then(|tab| tab.transactions.as_ref()?.get(tab.selected))
    else {
        // No transaction selected (nothing loaded yet, or an empty list).
        block.render(area, buf);
        return;
    };

    let label_style = Style::default().fg(secondary).add_modifier(Modifier::BOLD);
    let value_style = Style::default().fg(fg);

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

    let amount_color = if txn.amount.is_negative() {
        fg
    } else {
        success
    };
    add_field(
        &mut lines,
        "Amount",
        &txn.amount.format_display(),
        label_style,
        Style::default().fg(amount_color),
    );

    let (text, style) = match txn.status {
        TransactionStatus::Held => ("Pending", warning),
        TransactionStatus::Settled => ("Settled", success),
    };
    add_field(&mut lines, "Status", text, label_style, style.into());

    if let Some(ref txn_type) = txn.transaction_type {
        add_field(&mut lines, "Type", txn_type, label_style, value_style);
    }

    let format_time = |ts: &jiff::Timestamp| {
        ts.to_zoned_datetime(state.tz.clone())
            .strftime("%Y-%m-%d %H:%M:%S")
            .to_string()
    };

    add_field(
        &mut lines,
        "Created",
        &format_time(&txn.created_at),
        label_style,
        value_style,
    );

    if let Some(ref settled) = txn.settled_at {
        add_field(
            &mut lines,
            "Settled",
            &format_time(settled),
            label_style,
            value_style,
        );
    }

    if let Some(ref msg) = txn.message {
        add_field(&mut lines, "Message", msg, label_style, value_style);
    }

    if let Some(ref note) = txn.note {
        add_field(&mut lines, "Note", &note.text, label_style, value_style);
    }

    if let Some(ref foreign) = txn.foreign_amount {
        add_field(
            &mut lines,
            "Foreign Amount",
            &foreign.format_with_code(),
            label_style,
            value_style,
        );
    }

    if let Some(ref hold) = txn.hold_info {
        add_field(
            &mut lines,
            "Hold Amount",
            &hold.amount.format_display(),
            label_style,
            value_style,
        );
        if let Some(ref foreign) = hold.foreign_amount {
            add_field(
                &mut lines,
                "Hold Foreign",
                &foreign.format_with_code(),
                label_style,
                value_style,
            );
        }
    }

    if let Some(ref round_up) = txn.round_up {
        add_field(
            &mut lines,
            "Round Up",
            &round_up.amount.format_display(),
            label_style,
            value_style,
        );
        if let Some(ref boost) = round_up.boost_portion {
            add_field(
                &mut lines,
                "Boost Portion",
                &boost.format_display(),
                label_style,
                value_style,
            );
        }
    }

    if let Some(ref cashback) = txn.cashback {
        add_field(
            &mut lines,
            "Cashback",
            &format!(
                "{} ({})",
                cashback.amount.format_display(),
                cashback.description
            ),
            label_style,
            value_style,
        );
    }

    if let Some(ref card) = txn.card_purchase_method {
        let display = match &card.card_number_suffix {
            Some(suffix) => format!("{} (****{suffix})", card.method),
            None => card.method.clone(),
        };
        add_field(
            &mut lines,
            "Card Method",
            &display,
            label_style,
            value_style,
        );
    }

    if let Some(ref customer) = txn.performing_customer {
        add_field(
            &mut lines,
            "Customer",
            &customer.display_name,
            label_style,
            value_style,
        );
    }

    let category = match (&txn.parent_category, &txn.category) {
        (Some(parent), Some(cat)) => Some(format!(
            "{} / {}",
            state.category_name(parent),
            state.category_name(cat)
        )),
        (Some(id), None) | (None, Some(id)) => Some(state.category_name(id).to_string()),
        (None, None) => None,
    };
    if let Some(category) = category {
        add_field(&mut lines, "Category", &category, label_style, value_style);
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
        .block(block)
        .wrap(Wrap { trim: false });

    paragraph.render(area, buf);
}

/// Width of the label column: the longest label ("Foreign Amount"), its colon, and
/// a space before the value.
const LABEL_WIDTH: usize = 16;

fn add_field(
    lines: &mut Vec<Line<'_>>,
    label: &str,
    value: &str,
    label_style: Style,
    value_style: Style,
) {
    let padded_label = format!("{:<LABEL_WIDTH$}", format!("{label}:"));
    lines.push(Line::from(vec![
        Span::styled(padded_label, label_style),
        Span::styled(value.to_string(), value_style),
    ]));
}
