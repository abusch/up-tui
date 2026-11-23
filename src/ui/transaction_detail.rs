use jiff::{Timestamp, civil::Date, tz::TimeZone};
use opaline::names::tokens;
use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Padding, Paragraph},
};
use unicode_width::UnicodeWidthStr;

use crate::app::state::AppState;
use crate::client::models::{MoneyObject, Transaction, TransactionStatus};

/// Width of the label column in sections: the longest labels ("Round up", "Cashback")
/// plus two columns of space before the value.
const LABEL_WIDTH: usize = 10;

/// Draw the selected transaction's details in `area`, which is `beside_list` if the
/// transaction list is drawn to its left, or else on its own in place of the list.
pub fn draw_detail_pane(buf: &mut Buffer, area: Rect, state: &AppState, beside_list: bool) {
    let fg = state.theme.color(tokens::TEXT_PRIMARY).into();
    let bg = state.theme.color(tokens::BG_PANEL).into();

    // The left border separates the pane from the list: in some themes `BG_PANEL` is
    // the same colour as the list's `BG_BASE`. Without the list, it isn't needed (and
    // would double the sidebar's border).
    let borders = if beside_list {
        Borders::LEFT
    } else {
        Borders::NONE
    };
    let block = Block::default()
        .borders(borders)
        .border_style(state.theme.color(tokens::BORDER_UNFOCUSED))
        .style(Style::default().fg(fg).bg(bg))
        .padding(Padding::uniform(1));

    let Some(txn) = state.selected_transaction() else {
        // No transaction selected (nothing loaded yet, or an empty list).
        block.render(area, buf);
        return;
    };

    let width = usize::from(block.inner(area).width);
    // The lines are already wrapped to the pane's width.
    Paragraph::new(detail_lines(txn, state, width, local_today(state)))
        .block(block)
        .render(area, buf);
}

/// The current local date.
fn local_today(state: &AppState) -> Date {
    Timestamp::now().to_zoned_datetime(state.tz.clone()).date()
}

/// The detail pane's content for `txn`, wrapped to `width` columns. `today` is the
/// local date, used to show recent dates as "Today" and "Yesterday".
fn detail_lines(
    txn: &Transaction,
    state: &AppState,
    width: usize,
    today: Date,
) -> Vec<Line<'static>> {
    let theme = &state.theme;
    let fg = |token| Style::default().fg(theme.color(token).into());
    let mut out = DetailLines {
        lines: Vec::new(),
        width,
        heading_style: fg(tokens::TEXT_DIM).add_modifier(Modifier::BOLD),
        label_style: fg(tokens::TEXT_SECONDARY),
        value_style: fg(tokens::TEXT_PRIMARY),
    };

    // Header: what it was, how much, and how it's categorised.
    out.wrapped(&txn.description, fg(tokens::TEXT_PRIMARY).bold());

    let amount_style = if txn.amount.is_negative() {
        fg(tokens::TEXT_PRIMARY)
    } else {
        fg(tokens::SUCCESS)
    };
    let mut amount = vec![Span::styled(
        txn.amount.format_display(),
        amount_style.bold(),
    )];
    if let Some(foreign) = &txn.foreign_amount {
        amount.push(Span::styled(
            format!(" ({})", foreign.format_display_with_code()),
            fg(tokens::TEXT_MUTED),
        ));
    }
    let status = match txn.status {
        TransactionStatus::Held => Span::styled("● Pending", fg(tokens::WARNING)),
        TransactionStatus::Settled => Span::styled("● Settled", fg(tokens::SUCCESS)),
    };
    out.right_aligned(amount, status);

    let category = match (&txn.parent_category, &txn.category) {
        (Some(parent), Some(cat)) => Some(format!(
            "{} › {}",
            state.category_name(parent),
            state.category_name(cat)
        )),
        (Some(id), None) | (None, Some(id)) => Some(state.category_name(id).to_string()),
        (None, None) => None,
    };
    if let Some(category) = category {
        let color =
            state.category_color(txn.parent_category.as_deref().or(txn.category.as_deref()));
        out.wrapped(&category, Style::default().fg(color));
    }
    out.tags(
        &txn.tags,
        fg(tokens::TEXT_SECONDARY).bg(theme.color(tokens::BG_HIGHLIGHT).into()),
    );

    let mut when = vec![(
        "Created",
        format_datetime(&txn.created_at, &state.tz, today),
    )];
    if let Some(settled) = &txn.settled_at {
        when.push(("Settled", format_datetime(settled, &state.tz, today)));
    }
    out.section("When", &when);

    let mut details = Vec::new();
    if let Some(txn_type) = &txn.transaction_type {
        details.push(("Type", txn_type.clone()));
    }
    if let Some(account) = &txn.transfer_account {
        let label = if txn.amount.is_negative() {
            "To"
        } else {
            "From"
        };
        details.push((label, state.account_name(account).to_string()));
    }
    if let Some(card) = &txn.card_purchase_method {
        let method = card_method_name(&card.method);
        let value = match &card.card_number_suffix {
            Some(suffix) => format!("{method} ••{suffix}"),
            None => method.to_string(),
        };
        details.push(("Card", value));
    }
    if let Some(customer) = &txn.performing_customer {
        details.push(("By", customer.display_name.clone()));
    }
    // The hold amount is usually the same as the final amount; only show it when the
    // amount changed on settlement (e.g. a tip, or a different exchange rate).
    if let Some(hold) = &txn.hold_info
        && (hold.amount != txn.amount || hold.foreign_amount != txn.foreign_amount)
    {
        details.push((
            "Hold",
            with_foreign(&hold.amount, hold.foreign_amount.as_ref()),
        ));
    }
    if let Some(round_up) = &txn.round_up {
        let value = match &round_up.boost_portion {
            Some(boost) => format!(
                "{} (incl. {} boost)",
                round_up.amount.format_display(),
                boost.format_display()
            ),
            None => round_up.amount.format_display(),
        };
        details.push(("Round up", value));
    }
    if let Some(cashback) = &txn.cashback {
        details.push((
            "Cashback",
            format!(
                "{} ({})",
                cashback.amount.format_display(),
                cashback.description
            ),
        ));
    }
    out.section("Details", &details);

    if let Some(note) = &txn.note {
        out.text_section("Note", &note.text, fg(tokens::TEXT_PRIMARY));
    }
    if let Some(message) = &txn.message {
        out.text_section("Message", message, fg(tokens::TEXT_PRIMARY));
    }
    // The raw text is what appears on a statement; it's often just the description.
    if let Some(raw) = &txn.raw_text
        && !raw.trim().eq_ignore_ascii_case(txn.description.trim())
    {
        out.text_section("Raw text", raw, fg(tokens::TEXT_MUTED));
    }

    out.lines
}

/// Builds the detail pane's lines, wrapping text to the pane's width.
struct DetailLines {
    lines: Vec<Line<'static>>,
    width: usize,
    heading_style: Style,
    label_style: Style,
    value_style: Style,
}

impl DetailLines {
    /// Text wrapped to the full width.
    fn wrapped(&mut self, text: &str, style: Style) {
        for part in wrap(text, self.width) {
            self.lines.push(Line::styled(part, style));
        }
    }

    /// `left` with `right` right-aligned on the same line, or on the next line if
    /// they don't both fit.
    fn right_aligned(&mut self, left: Vec<Span<'static>>, right: Span<'static>) {
        self.lines
            .extend(super::right_aligned(left, vec![right], self.width));
    }

    /// Tags as padded "pills", wrapping between tags.
    fn tags(&mut self, tags: &[String], style: Style) {
        let mut line: Vec<Span<'static>> = Vec::new();
        let mut used = 0;
        for tag in tags {
            let pill = format!(" {tag} ");
            let pill_width = pill.width();
            if !line.is_empty() && used + 1 + pill_width > self.width {
                self.lines.push(Line::from(std::mem::take(&mut line)));
                used = 0;
            }
            if !line.is_empty() {
                line.push(Span::raw(" "));
                used += 1;
            }
            line.push(Span::styled(pill, style));
            used += pill_width;
        }
        if !line.is_empty() {
            self.lines.push(Line::from(line));
        }
    }

    /// A blank line, then a section heading filled to the full width, like the day
    /// separators in the transaction list.
    fn heading(&mut self, title: &str) {
        self.lines.push(Line::default());
        let title = format!("{} ", title.to_uppercase());
        self.lines.push(Line::styled(
            format!("{title:┄<width$}", width = self.width),
            self.heading_style,
        ));
    }

    /// A section of label/value fields; nothing if there are no fields. Long values
    /// wrap, indented to the value column.
    fn section(&mut self, title: &str, fields: &[(&str, String)]) {
        if fields.is_empty() {
            return;
        }
        self.heading(title);
        let value_width = self.width.saturating_sub(LABEL_WIDTH);
        for (label, value) in fields {
            for (i, part) in wrap(value, value_width).into_iter().enumerate() {
                let label = if i == 0 { label } else { "" };
                self.lines.push(Line::from(vec![
                    Span::styled(format!("{label:<LABEL_WIDTH$}"), self.label_style),
                    Span::styled(part, self.value_style),
                ]));
            }
        }
    }

    /// A section containing free text.
    fn text_section(&mut self, title: &str, text: &str, style: Style) {
        self.heading(title);
        self.wrapped(text, style);
    }
}

/// Word-wrap `text` to `width` columns (at least 1), breaking words that don't fit.
fn wrap(text: &str, width: usize) -> Vec<String> {
    // The default algorithm depends on textwrap's features, which other dependencies
    // can enable; first-fit fills each line as much as possible.
    let options =
        textwrap::Options::new(width.max(1)).wrap_algorithm(textwrap::WrapAlgorithm::FirstFit);
    textwrap::wrap(text, options)
        .into_iter()
        .map(|part| part.into_owned())
        .collect()
}

/// A local date and time, e.g. "Today, 14:32" or "Mon 18 Nov, 14:32"; the year is
/// shown only if it isn't the current year.
fn format_datetime(ts: &Timestamp, tz: &TimeZone, today: Date) -> String {
    let zdt = ts.to_zoned_datetime(tz.clone());
    let date = zdt.date();
    let day = if date == today {
        "Today".to_string()
    } else if today.yesterday().is_ok_and(|yesterday| yesterday == date) {
        "Yesterday".to_string()
    } else if date.year() == today.year() {
        zdt.strftime("%a %-d %b").to_string()
    } else {
        zdt.strftime("%a %-d %b %Y").to_string()
    };
    format!("{day}, {}", zdt.strftime("%H:%M"))
}

/// An amount followed by its foreign currency amount, if any.
fn with_foreign(amount: &MoneyObject, foreign: Option<&MoneyObject>) -> String {
    match foreign {
        Some(foreign) => format!(
            "{} ({})",
            amount.format_display(),
            foreign.format_display_with_code()
        ),
        None => amount.format_display(),
    }
}

/// A readable name for the API's card purchase method.
fn card_method_name(method: &str) -> &str {
    match method {
        "BAR_CODE" => "Barcode",
        "OCR" => "OCR",
        "CARD_PIN" => "Card + PIN",
        "CARD_DETAILS" => "Card details",
        "CARD_ON_FILE" => "Card on file",
        "ECOMMERCE" => "Online",
        "MAGNETIC_STRIPE" => "Magnetic stripe",
        "CONTACTLESS" => "Contactless",
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::categories::{Categories, tests::named};
    use crate::app::state::TabState;
    use crate::app::state::tests::{account, money, txn};
    use crate::client::models::{CardPurchaseMethod, HoldInfo, Note, RoundUp};

    fn text(lines: &[Line]) -> Vec<String> {
        lines
            .iter()
            .map(|line| line.spans.iter().map(|s| s.content.as_ref()).collect())
            .collect()
    }

    fn state() -> AppState {
        let mut state = AppState::new(None);
        state.tz = TimeZone::UTC;
        state
    }

    fn date(s: &str) -> Date {
        s.parse().unwrap()
    }

    #[test]
    fn full_transaction_layout() {
        let mut state = state();
        state.categories = Categories::new([
            named("good-life", "Good Life", None),
            named("restaurants", "Restaurants", Some("good-life")),
        ]);

        let mut t = txn("t", "2026-11-20T12:30:00Z");
        t.description = "Cafe Sydney".to_string();
        t.raw_text = Some("CAFE SYDNEY PTY LTD".to_string());
        t.amount = money(-5420);
        t.settled_at = Some("2026-11-21T03:10:00Z".parse().unwrap());
        t.transaction_type = Some("Purchase".to_string());
        t.card_purchase_method = Some(CardPurchaseMethod {
            method: "CONTACTLESS".to_string(),
            card_number_suffix: Some("1234".to_string()),
        });
        t.round_up = Some(RoundUp {
            amount: money(-80),
            boost_portion: None,
        });
        t.hold_info = Some(HoldInfo {
            amount: money(-5420),
            foreign_amount: None,
        });
        t.note = Some(Note {
            text: "Dinner with friends after the movie".to_string(),
        });
        t.parent_category = Some("good-life".to_string());
        t.category = Some("restaurants".to_string());
        t.tags = vec!["date night".to_string(), "food".to_string()];

        let lines = detail_lines(&t, &state, 30, date("2026-11-21"));
        assert_eq!(
            text(&lines),
            [
                "Cafe Sydney",
                "$54.20               ● Settled",
                "Good Life › Restaurants",
                " date night   food ",
                "",
                "WHEN ┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄",
                "Created   Yesterday, 12:30",
                "Settled   Today, 03:10",
                "",
                "DETAILS ┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄",
                "Type      Purchase",
                "Card      Contactless ••1234",
                "Round up  $0.80",
                "",
                "NOTE ┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄",
                "Dinner with friends after the",
                "movie",
                "",
                "RAW TEXT ┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄",
                "CAFE SYDNEY PTY LTD",
            ]
        );
        assert_eq!(
            lines[2].style.fg,
            Some(state.theme.color(tokens::WARNING).into())
        );
    }

    #[test]
    fn minimal_transaction_has_no_empty_sections() {
        let mut t = txn("t", "2026-11-20T12:30:00Z");
        t.raw_text = Some("transaction T".to_string());
        t.status = TransactionStatus::Held;
        let lines = detail_lines(&t, &state(), 30, date("2026-11-20"));
        assert_eq!(
            text(&lines),
            [
                "Transaction t",
                "$10.00               ● Pending",
                "",
                "WHEN ┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄",
                "Created   Today, 12:30",
            ]
        );
    }

    #[test]
    fn hold_is_shown_when_it_differs_and_long_values_wrap_under_the_value_column() {
        let mut t = txn("t", "2026-11-20T12:30:00Z");
        t.hold_info = Some(HoldInfo {
            amount: money(-900),
            foreign_amount: None,
        });
        t.transaction_type = Some("A very long transaction type".to_string());
        let lines = detail_lines(&t, &state(), 24, date("2026-11-20"));
        let text = text(&lines);
        assert_eq!(
            &text[text.len() - 4..],
            [
                "Type      A very long",
                "          transaction",
                "          type",
                "Hold      $9.00"
            ]
        );
    }

    #[test]
    fn status_moves_to_its_own_line_when_narrow() {
        let mut t = txn("t", "2026-11-20T12:30:00Z");
        t.foreign_amount = Some(MoneyObject {
            currency_code: "USD".to_string(),
            value: "-6.50".to_string(),
            value_in_base_units: -650,
        });
        let lines = detail_lines(&t, &state(), 20, date("2026-11-20"));
        assert_eq!(&text(&lines)[1..3], ["$10.00 (6.50 USD)", "● Settled"]);
    }

    #[test]
    fn transfers_name_the_other_account() {
        let mut state = state();
        state.tabs.push(TabState::new(account("saver", 0)));
        let mut t = txn("t", "2026-11-20T12:30:00Z");
        t.transfer_account = Some("saver".to_string());
        let lines = detail_lines(&t, &state, 30, date("2026-11-20"));
        assert!(text(&lines).contains(&"To        Account saver".to_string()));
    }

    #[test]
    fn dates_are_relative_when_recent() {
        let tz = TimeZone::UTC;
        let ts: Timestamp = "2026-11-20T14:32:00Z".parse().unwrap();
        assert_eq!(
            format_datetime(&ts, &tz, date("2026-11-20")),
            "Today, 14:32"
        );
        assert_eq!(
            format_datetime(&ts, &tz, date("2026-11-21")),
            "Yesterday, 14:32"
        );
        assert_eq!(
            format_datetime(&ts, &tz, date("2026-11-23")),
            "Fri 20 Nov, 14:32"
        );
        assert_eq!(
            format_datetime(&ts, &tz, date("2027-01-05")),
            "Fri 20 Nov 2026, 14:32"
        );
    }
}
