use jiff::Timestamp;
use opaline::names::tokens;
use ratatui::{
    prelude::*,
    symbols::Marker,
    widgets::{Axis, Block, Chart, Dataset, GraphType, Padding, Paragraph},
};
use unicode_width::UnicodeWidthStr;

use super::transaction_list::truncate_to_width;
use crate::app::insights::{CategorySpending, daily_balances, spending_by_category};
use crate::app::state::{AppState, RECENT_DAYS, TabState};
use crate::client::models::MoneyObject;

/// Partial blocks for the fractional end of a bar, in eighths (index 1 is ⅛).
const PARTIAL_BLOCKS: [&str; 8] = ["", "▏", "▎", "▍", "▌", "▋", "▊", "▉"];
/// Maximum width of the category name column.
const MAX_NAME_WIDTH: usize = 24;
/// Width of the percentage column ("100%").
const PERCENT_WIDTH: usize = 4;
/// Rows for the balance section (heading and chart, including its axis and labels).
/// It's fixed so that the chart doesn't change size between accounts.
const BALANCE_HEIGHT: u16 = 14;
/// Rows kept for the category list (heading, a row and the total) on a short screen,
/// for which the balance section is shrunk.
const MIN_CATEGORY_HEIGHT: u16 = 3;
/// Below this height, the chart is left out rather than squashed.
const MIN_BALANCE_HEIGHT: u16 = 6;

/// The Insights view for the selected account, in place of the transaction list:
/// the balance over the recent period at the top, then spending by category.
pub fn draw_insights(buf: &mut Buffer, area: Rect, state: &AppState) {
    let theme = &state.theme;
    let fg = |token| Style::default().fg(theme.color(token).into());
    let block = Block::default()
        .title(format!(" Insights - last {RECENT_DAYS} days "))
        .title_style(theme.color(tokens::ACCENT_PRIMARY))
        .padding(Padding::uniform(1))
        .style(fg(tokens::TEXT_PRIMARY).bg(theme.color(tokens::BG_BASE).into()));
    let inner = block.inner(area);
    block.render(area, buf);

    let Some(tab) = state.current_tab() else {
        return;
    };
    let Some(recent) = tab.store.recent() else {
        let message = if tab.store.loading().is_some() {
            "Loading…".to_string()
        } else {
            format!("Couldn't load the last {RECENT_DAYS} days' transactions. Press r to retry.")
        };
        Paragraph::new(Line::styled(message, fg(tokens::TEXT_DIM))).render(inner, buf);
        return;
    };

    // The balance section has a fixed height (unless the screen is too short), and
    // the category list gets the rest, below a blank line.
    let mut balance_height =
        BALANCE_HEIGHT.min(inner.height.saturating_sub(MIN_CATEGORY_HEIGHT + 1));
    if balance_height < MIN_BALANCE_HEIGHT {
        balance_height = 0;
    }
    let [balance_area, category_area] =
        Layout::vertical([Constraint::Length(balance_height), Constraint::Fill(1)])
            .spacing(u16::from(balance_height > 0))
            .areas(inner);
    draw_balance_chart(buf, balance_area, tab, state);

    let spending = spending_by_category(recent);
    let categories = category_lines(
        &spending,
        tab,
        state,
        usize::from(inner.width),
        usize::from(category_area.height),
    );
    Paragraph::new(categories).render(category_area, buf);
}

/// A section heading filled to `width`, like the detail pane's.
fn heading(title: &str, width: usize, state: &AppState) -> Line<'static> {
    let style = Style::default()
        .fg(state.theme.color(tokens::TEXT_DIM).into())
        .add_modifier(Modifier::BOLD);
    let title = format!("{} ", title.to_uppercase());
    Line::styled(format!("{title:┄<width$}"), style)
}

/// The spending by category section: a heading, a bar per category (at most
/// `max_lines` lines in all, the smallest categories grouped as "Other"), and the
/// total.
fn category_lines(
    spending: &[CategorySpending],
    tab: &TabState,
    state: &AppState,
    width: usize,
    max_lines: usize,
) -> Vec<Line<'static>> {
    let theme = &state.theme;
    let fg = |token| Style::default().fg(theme.color(token).into());
    let dim = fg(tokens::TEXT_DIM);
    let mut lines = vec![heading("Spending by category", width, state)];

    let total: i64 = spending
        .iter()
        .fold(0, |sum, c| sum.saturating_add(c.total));
    if total == 0 {
        lines.push(Line::styled("No spending in this period.", dim));
        return lines;
    }

    // Heading and total lines, plus one row per category.
    let max_rows = max_lines.saturating_sub(2).max(1);
    let mut rows: Vec<(String, i64)> = spending
        .iter()
        .map(|c| {
            let name = match &c.category {
                Some(id) => state.category_name(id).to_string(),
                None => "Uncategorised".to_string(),
            };
            (name, c.total)
        })
        .collect();
    if rows.len() > max_rows {
        let other = rows
            .split_off(max_rows - 1)
            .iter()
            .fold(0i64, |sum, (_, t)| sum.saturating_add(*t));
        rows.push(("Other".to_string(), other));
    }

    let money = |base_units: i64| {
        tab.account
            .balance
            .with_base_units(base_units)
            .map(|m| m.format_unsigned())
            .unwrap_or_default()
    };
    let amounts: Vec<String> = rows.iter().map(|&(_, t)| money(t)).collect();
    let amount_width = amounts.iter().map(|a| a.width()).max().unwrap_or(0);
    let name_width = rows
        .iter()
        .map(|(name, _)| name.width())
        .max()
        .unwrap_or(0)
        .min(MAX_NAME_WIDTH)
        .min(width / 3);
    // Name, gap, bar, gap, amount, gap, percentage.
    let bar_width = width.saturating_sub(name_width + 2 + 1 + amount_width + 2 + PERCENT_WIDTH);
    let largest = rows.iter().map(|&(_, t)| t).max().unwrap_or(1).max(1);

    for ((name, amount), formatted) in rows.iter().zip(&amounts) {
        let name = truncate_to_width(name, name_width);
        let name_padding = name_width.saturating_sub(name.width());
        let bar = bar(*amount, largest, bar_width);
        let bar_padding = bar_width.saturating_sub(bar.width());
        lines.push(Line::from(vec![
            Span::styled(name, fg(tokens::TEXT_PRIMARY)),
            Span::raw(" ".repeat(name_padding + 2)),
            Span::styled(bar, fg(tokens::ACCENT_SECONDARY)),
            Span::raw(" ".repeat(bar_padding + 1)),
            Span::styled(
                format!("{formatted:>amount_width$}"),
                fg(tokens::TEXT_PRIMARY),
            ),
            Span::styled(
                format!("  {:>PERCENT_WIDTH$}", percent(*amount, total)),
                dim,
            ),
        ]));
    }

    lines.push(Line::styled(
        format!(
            "{} spent, not counting transfers between your accounts",
            money(total)
        ),
        dim,
    ));
    lines
}

/// A bar for `value` out of `largest`, at most `width` columns, in eighths of a column.
/// A non-zero value always gets at least an eighth.
fn bar(value: i64, largest: i64, width: usize) -> String {
    let eighths = i128::from(value) * (width as i128) * 8 / i128::from(largest.max(1));
    let eighths = usize::try_from(eighths)
        .unwrap_or(0)
        .max(usize::from(value > 0));
    let mut bar = "█".repeat(eighths / 8);
    bar.push_str(PARTIAL_BLOCKS[eighths % 8]);
    bar
}

/// `part` as a rounded percentage of `total`, or "<1%" if it rounds to zero.
fn percent(part: i64, total: i64) -> String {
    let percent = (i128::from(part) * 200 + i128::from(total)) / (2 * i128::from(total.max(1)));
    if percent == 0 && part > 0 {
        "<1%".to_string()
    } else {
        format!("{percent}%")
    }
}

/// The balance at the end of each day of the period, as a line chart.
fn draw_balance_chart(buf: &mut Buffer, area: Rect, tab: &TabState, state: &AppState) {
    let Some(recent) = tab.store.recent() else {
        return;
    };
    if area.height < 2 {
        return;
    }
    let theme = &state.theme;
    let fg = |token| Style::default().fg(theme.color(token).into());
    let dim = fg(tokens::TEXT_DIM);

    let [heading_area, chart_area] =
        Layout::vertical([Constraint::Length(1), Constraint::Fill(1)]).areas(area);
    heading("Balance", usize::from(area.width), state).render(heading_area, buf);

    let today = Timestamp::now().to_zoned_datetime(state.tz.clone()).date();
    let balance = &tab.account.balance;
    let balances = daily_balances(balance.value_in_base_units, recent, &state.tz, today);
    let (Some(first), Some(last)) = (balances.first(), balances.last()) else {
        return;
    };
    let points: Vec<(f64, f64)> = balances
        .iter()
        .enumerate()
        .map(|(i, &(_, b))| (i as f64, b as f64))
        .collect();

    let low = balances.iter().map(|&(_, b)| b).min().unwrap_or(0);
    let high = balances.iter().map(|&(_, b)| b).max().unwrap_or(0);
    let axis = YAxis::new(low, high, balance.base_units_per_unit());
    let label = |base_units: i64| {
        let text = balance
            .with_base_units(base_units)
            .as_ref()
            .map(MoneyObject::format_signed)
            .unwrap_or_default();
        Span::styled(without_zero_fraction(&text).to_string(), dim)
    };
    let middle = &balances[balances.len() / 2];
    let day = |date: jiff::civil::Date| Span::styled(date.strftime("%-d %b").to_string(), dim);

    let dataset = Dataset::default()
        .marker(Marker::Braille)
        .graph_type(GraphType::Line)
        .style(fg(tokens::ACCENT_PRIMARY))
        .data(&points);
    Chart::new(vec![dataset])
        .style(fg(tokens::TEXT_PRIMARY))
        .x_axis(
            Axis::default()
                .style(dim)
                .bounds([0.0, (points.len().saturating_sub(1)) as f64])
                .labels([day(first.0), day(middle.0), day(last.0)]),
        )
        .y_axis(
            Axis::default()
                .style(dim)
                .bounds([axis.low as f64, axis.high as f64])
                .labels(axis.ticks().map(label)),
        )
        .render(chart_area, buf);
}

/// The balance chart's vertical axis: from zero (or below, for negative balances) to
/// the highest balance (or zero), rounded out to a multiple of a "round" step, so the
/// line's height is in proportion to the balance and the labels are round amounts.
#[derive(Debug, PartialEq, Eq)]
struct YAxis {
    /// The bottom of the axis, in base units; zero or a negative multiple of `step`.
    low: i64,
    /// The top of the axis, in base units; zero or a positive multiple of `step`.
    high: i64,
    /// The distance between labels: 1, 2 or 5 times a power of ten, and at least one
    /// whole unit of the currency, so the labels are whole amounts.
    step: i64,
}

impl YAxis {
    /// Aim for about this many steps between the lowest and highest values.
    const STEPS: i64 = 4;

    /// An axis for values between `low` and `high` (in base units), in a currency with
    /// `unit` base units per whole unit.
    fn new(low: i64, high: i64, unit: i64) -> Self {
        let (low, high) = (low.min(0), high.max(0));
        let rough_step = high.saturating_sub(low) / Self::STEPS;
        let mut magnitude: i64 = 1;
        while magnitude <= rough_step / 10 {
            magnitude *= 10;
        }
        let step = [1, 2, 5, 10]
            .into_iter()
            .map(|m| magnitude.saturating_mul(m))
            .find(|&step| step >= rough_step)
            .unwrap_or(i64::MAX)
            .max(unit.max(1));
        // Round outwards to multiples of the step. Without any range (all zero), show
        // one step above zero.
        let low = low.div_euclid(step).saturating_mul(step);
        let high = high
            .saturating_add(step - 1)
            .div_euclid(step)
            .saturating_mul(step)
            .max(low.saturating_add(step));
        YAxis { low, high, step }
    }

    /// The labelled values, from `low` to `high` in steps of `step`. Ratatui spaces
    /// the labels evenly, so they line up with the values they show.
    fn ticks(&self) -> impl Iterator<Item = i64> + use<> {
        let (low, step) = (self.low, self.step);
        let count = (self.high - self.low) / step;
        (0..=count).map(move |i| low + i * step)
    }
}

/// `text` without a fractional part that's all zeros: "$1000" for "$1000.00".
fn without_zero_fraction(text: &str) -> &str {
    match text.split_once('.') {
        Some((whole, fraction)) if fraction.chars().all(|c| c == '0') => whole,
        _ => text,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::state::tests::{account, money, state, txn};
    use crate::client::models::Transaction;

    fn text(lines: &[Line]) -> Vec<String> {
        lines
            .iter()
            .map(|line| line.spans.iter().map(|s| s.content.as_ref()).collect())
            .collect()
    }

    fn spent(category: Option<&str>, total: i64) -> CategorySpending {
        CategorySpending {
            category: category.map(str::to_owned),
            total,
        }
    }

    fn ticks(low: i64, high: i64) -> (YAxis, Vec<i64>) {
        let axis = YAxis::new(low, high, 100);
        let ticks = axis.ticks().collect();
        (axis, ticks)
    }

    #[test]
    fn y_axis_starts_at_zero_with_round_steps() {
        // $1200 to $1830: from $0, in $500 steps.
        assert_eq!(
            ticks(120_000, 183_000).1,
            [0, 50_000, 100_000, 150_000, 200_000]
        );
        // Crossing zero.
        assert_eq!(ticks(-235_428, 182_973).1, [-400_000, -200_000, 0, 200_000]);
        // A home loan: zero at the top.
        assert_eq!(
            ticks(-50_000_000, -49_000_000).1,
            [-60_000_000, -40_000_000, -20_000_000, 0]
        );
        // Small amounts still step in whole dollars.
        assert_eq!(ticks(0, 250).1, [0, 100, 200, 300]);
        // Nothing at all: one step above zero.
        assert_eq!(ticks(0, 0).1, [0, 100]);
    }

    #[test]
    fn labels_drop_a_zero_fraction() {
        assert_eq!(without_zero_fraction("$1000.00"), "$1000");
        assert_eq!(without_zero_fraction("-$2000.00"), "-$2000");
        assert_eq!(without_zero_fraction("$12.50"), "$12.50");
        assert_eq!(without_zero_fraction("1500"), "1500");
    }

    #[test]
    fn bars_are_proportional_to_the_largest() {
        assert_eq!(bar(100, 100, 4), "████");
        assert_eq!(bar(50, 100, 4), "██");
        assert_eq!(bar(45, 100, 4), "█▊");
        // Tiny amounts are still visible.
        assert_eq!(bar(1, 100_000, 4), "▏");
        assert_eq!(bar(0, 100, 4), "");
    }

    #[test]
    fn percentages_are_rounded() {
        assert_eq!(percent(1, 3), "33%");
        assert_eq!(percent(2, 3), "67%");
        assert_eq!(percent(3, 3), "100%");
        assert_eq!(percent(1, 1000), "<1%");
    }

    #[test]
    fn categories_are_listed_with_bars_amounts_and_shares() {
        let mut state = state();
        state
            .categories
            .insert("groceries".to_string(), "Groceries".to_string());
        state
            .categories
            .insert("restaurants".to_string(), "Restaurants & Cafes".to_string());
        let tab = TabState::new(account("a", 0));
        let spending = [
            spent(Some("groceries"), 41230),
            spent(Some("restaurants"), 20615),
            spent(None, 8000),
        ];
        let lines = category_lines(&spending, &tab, &state, 60, 10);
        assert_eq!(
            text(&lines),
            [
                "SPENDING BY CATEGORY ┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄",
                "Groceries            █████████████████████████ $412.30   59%",
                "Restaurants & Cafes  ████████████▌             $206.15   30%",
                "Uncategorised        ████▊                      $80.00   11%",
                "$698.45 spent, not counting transfers between your accounts",
            ]
        );
    }

    #[test]
    fn small_categories_are_grouped_when_there_is_no_room() {
        let state = state();
        let tab = TabState::new(account("a", 0));
        let spending = [
            spent(Some("a"), 5000),
            spent(Some("b"), 3000),
            spent(Some("c"), 1500),
            spent(Some("d"), 500),
        ];
        // Heading, 2 rows, total.
        let lines = text(&category_lines(&spending, &tab, &state, 40, 4));
        assert_eq!(lines.len(), 4);
        assert!(lines[1].starts_with("a "));
        assert!(lines[2].starts_with("Other ") && lines[2].contains("$50.00"));
    }

    #[test]
    fn no_spending() {
        let lines = category_lines(&[], &TabState::new(account("a", 0)), &state(), 40, 10);
        assert_eq!(text(&lines)[1], "No spending in this period.");
    }

    #[test]
    fn renders_at_any_size_without_panicking() {
        let mut state = state();
        let mut tab = TabState::new(account("a", 123456));
        tab.set_transactions(vec![Transaction {
            amount: money(-2500),
            ..txn("t", "2026-11-20T10:00:00Z")
        }]);
        state.tabs.push(tab);
        for (width, height) in [(0, 0), (1, 1), (10, 3), (30, 12), (100, 40)] {
            let area = Rect::new(0, 0, width, height);
            let mut buf = Buffer::empty(area);
            draw_insights(&mut buf, area, &state);
        }
    }
}
