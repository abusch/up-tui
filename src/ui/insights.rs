use jiff::ToSpan;
use opaline::names::tokens;
use ratatui::{
    prelude::*,
    symbols::Marker,
    widgets::{Axis, Block, Chart, Dataset, GraphType, Padding, Paragraph},
};
use unicode_width::UnicodeWidthStr;

use super::transaction_list::truncate_to_width;
use crate::app::insights::{CategorySpending, daily_balances, spending_by_root, spending_in_root};
use crate::app::state::{AppState, TabState};
use crate::client::models::MoneyObject;

/// Partial blocks for the fractional end of a bar, in eighths (index 1 is ⅛).
const PARTIAL_BLOCKS: [&str; 8] = ["", "▏", "▎", "▍", "▌", "▋", "▊", "▉"];
/// Drawn before the selected category, like the transaction list's selection
/// marker. Other rows are indented by the same width.
const MARKER: &str = "▎ ";
const MARKER_WIDTH: usize = 2;
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
/// the balance over the period at the top, then spending by root category, or
/// the selected root category's breakdown by subcategory.
pub fn draw_insights(buf: &mut Buffer, area: Rect, state: &AppState) {
    let theme = &state.theme;
    let fg = |token| Style::default().fg(theme.color(token).into());
    let block = Block::default()
        .title(format!(" Insights - {} ", state.period.name(state.today())))
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
            "Couldn't load this period's transactions. Press r to retry.".to_string()
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

    let roots = spending_by_root(recent);
    let insights = &state.insights;
    let section = match insights.root.row(&roots) {
        Some(root) if insights.breakdown => {
            let spending = spending_in_root(recent, root.category.as_deref());
            let name = category_name(root.category.as_deref(), state);
            CategorySection {
                heading: format!("Spending on {name}"),
                selected: insights.subcategory.index(&spending),
                footer: format!(
                    "{} spent on {name}, {} of all spending",
                    money(tab, root.total),
                    percent(root.total, total_of(&roots))
                ),
                spending,
            }
        }
        _ => CategorySection {
            heading: "Spending by category".to_string(),
            selected: insights.root.index(&roots),
            footer: format!(
                "{} spent, not counting transfers between your accounts",
                money(tab, total_of(&roots))
            ),
            spending: roots,
        },
    };
    let lines = section.lines(
        tab,
        state,
        usize::from(inner.width),
        usize::from(category_area.height),
    );
    Paragraph::new(lines).render(category_area, buf);
}

/// A section heading filled to `width`, like the detail pane's.
fn heading(title: &str, width: usize, state: &AppState) -> Line<'static> {
    let style = Style::default()
        .fg(state.theme.color(tokens::TEXT_DIM).into())
        .add_modifier(Modifier::BOLD);
    let title = format!("{} ", title.to_uppercase());
    Line::styled(format!("{title:┄<width$}"), style)
}

/// A category's display name, or "Uncategorised" for `None`.
fn category_name(category: Option<&str>, state: &AppState) -> String {
    match category {
        Some(id) => state.category_name(id).to_string(),
        None => "Uncategorised".to_string(),
    }
}

/// A row of the category list: the name, the bar's colour (its root category's) and
/// the amount, in base units.
struct Row {
    name: String,
    color: Color,
    amount: i64,
}

impl Row {
    fn new(spending: &CategorySpending, state: &AppState) -> Self {
        let category = spending.category.as_deref();
        Row {
            name: category_name(category, state),
            color: state.category_color(category),
            amount: spending.total,
        }
    }
}

/// A list of categories' spending: the spending by root category, or a root
/// category's breakdown by subcategory.
struct CategorySection {
    heading: String,
    /// The categories, largest first.
    spending: Vec<CategorySpending>,
    /// The index of the selected category in `spending`.
    selected: Option<usize>,
    /// The line below the list, with its total.
    footer: String,
}

impl CategorySection {
    /// The section's lines: the heading, a bar per category with the selected one
    /// highlighted, and the footer, in at most `max_lines` lines. If the categories
    /// don't all fit, the list scrolls to show the selected one.
    fn lines(
        &self,
        tab: &TabState,
        state: &AppState,
        width: usize,
        max_lines: usize,
    ) -> Vec<Line<'static>> {
        let mut lines = vec![heading(&self.heading, width, state)];
        let total = total_of(&self.spending);
        if total == 0 {
            lines.push(Line::styled("No spending in this period.", dim(state)));
            return lines;
        }

        let rows: Vec<Row> = self.spending.iter().map(|c| Row::new(c, state)).collect();
        let mut bars = bar_lines(&rows, total, self.selected, tab, state, width);
        // Leave room for the heading and the footer, and keep the selection in view
        // (at the bottom when scrolled).
        let max_rows = max_lines.saturating_sub(2).max(1);
        let first = self.selected.unwrap_or(0).saturating_sub(max_rows - 1);
        bars.truncate(first + max_rows);
        lines.extend(bars.drain(first..));
        lines.push(Line::styled(self.footer.clone(), dim(state)));
        lines
    }
}

/// The total of `spending`, in base units.
fn total_of(spending: &[CategorySpending]) -> i64 {
    spending
        .iter()
        .fold(0, |sum, c| sum.saturating_add(c.total))
}

/// `base_units` in the account's currency, e.g. "$12.50".
fn money(tab: &TabState, base_units: i64) -> String {
    tab.account
        .balance
        .with_base_units(base_units)
        .map(|m| m.format_unsigned())
        .unwrap_or_default()
}

fn dim(state: &AppState) -> Style {
    Style::default().fg(state.theme.color(tokens::TEXT_DIM).into())
}

/// A line per row: a marker column (with the marker on the `selected` row, which is
/// highlighted), the name, a bar relative to the largest amount, the amount, and its
/// share of `total`. The lines fill `width`, so the highlight covers the whole row.
fn bar_lines(
    rows: &[Row],
    total: i64,
    selected: Option<usize>,
    tab: &TabState,
    state: &AppState,
    width: usize,
) -> Vec<Line<'static>> {
    let theme = &state.theme;
    let fg = |token| Style::default().fg(theme.color(token).into());
    let selected_style = Style::default()
        .bg(theme.color(tokens::BG_SELECTION).into())
        .add_modifier(Modifier::BOLD);

    let amounts: Vec<String> = rows.iter().map(|row| money(tab, row.amount)).collect();
    let amount_width = amounts.iter().map(|a| a.width()).max().unwrap_or(0);
    let name_width = rows
        .iter()
        .map(|row| row.name.width())
        .max()
        .unwrap_or(0)
        .min(MAX_NAME_WIDTH)
        .min(width / 3);
    // Marker, name, gap, bar, gap, amount, gap, percentage.
    let bar_width =
        width.saturating_sub(MARKER_WIDTH + name_width + 2 + 1 + amount_width + 2 + PERCENT_WIDTH);
    let largest = rows.iter().map(|row| row.amount).max().unwrap_or(1).max(1);

    rows.iter()
        .zip(&amounts)
        .enumerate()
        .map(|(i, (row, formatted))| {
            let name = truncate_to_width(&row.name, name_width);
            let name_padding = name_width.saturating_sub(name.width());
            let bar = bar(row.amount, largest, bar_width);
            let bar_padding = bar_width.saturating_sub(bar.width());
            let is_selected = selected == Some(i);
            let marker = if is_selected {
                Span::styled(MARKER, fg(tokens::ACCENT_PRIMARY))
            } else {
                Span::raw(" ".repeat(MARKER_WIDTH))
            };
            let line = Line::from(vec![
                marker,
                Span::styled(name, fg(tokens::TEXT_PRIMARY)),
                Span::raw(" ".repeat(name_padding + 2)),
                Span::styled(bar, Style::default().fg(row.color)),
                Span::raw(" ".repeat(bar_padding + 1)),
                Span::styled(
                    format!("{formatted:>amount_width$}"),
                    fg(tokens::TEXT_PRIMARY),
                ),
                Span::styled(
                    format!("  {:>PERCENT_WIDTH$}", percent(row.amount, total)),
                    fg(tokens::TEXT_DIM),
                ),
            ]);
            if is_selected {
                line.style(selected_style)
            } else {
                line
            }
        })
        .collect()
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

/// The balance at the end of each day of the period so far, as a line chart. Its axis
/// spans the whole period (`Period::last_day`), so a month's line ends at today.
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

    let today = state.today();
    let (first_day, last_day) = (state.period.first_day(today), state.period.last_day(today));
    let balance = &tab.account.balance;
    let balances = daily_balances(
        balance.value_in_base_units,
        recent,
        &state.tz,
        first_day,
        today,
    );
    if balances.is_empty() {
        return;
    }
    // The index of the axis's last day: the days are numbered from `first_day`.
    let last_index = first_day
        .until(last_day)
        .map_or(0, |span| span.get_days())
        .max(i32::try_from(balances.len()).unwrap_or(i32::MAX) - 1);
    let middle_day = first_day.saturating_add((last_index / 2).days());
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
                .bounds([0.0, f64::from(last_index)])
                .labels([day(first_day), day(middle_day), day(last_day)]),
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
    use crate::app::categories::{Categories, tests::named};
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

    fn categories_state() -> AppState {
        let mut state = state();
        state.categories = Categories::new([
            named("good-life", "Good Life", None),
            named("home", "Home", None),
            named("groceries", "Groceries", Some("home")),
            named("rent", "Rent & Mortgage", Some("home")),
            named("utilities", "Utilities", Some("home")),
            named("internet", "Internet", Some("home")),
        ]);
        state
    }

    fn section(spending: Vec<CategorySpending>, selected: Option<usize>) -> CategorySection {
        CategorySection {
            heading: "Spending by category".to_string(),
            spending,
            selected,
            footer: "Footer".to_string(),
        }
    }

    #[test]
    fn categories_are_listed_with_bars_amounts_and_shares() {
        let state = categories_state();
        let tab = TabState::new(account("a", 0));
        let spending = vec![
            spent(Some("home"), 41230),
            spent(Some("good-life"), 20615),
            spent(None, 8000),
        ];
        let lines = section(spending, Some(1)).lines(&tab, &state, 60, 10);
        assert_eq!(
            text(&lines),
            [
                "SPENDING BY CATEGORY ┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄",
                "  Home           █████████████████████████████ $412.30   59%",
                "▎ Good Life      ██████████████▌               $206.15   30%",
                "  Uncategorised  █████▋                         $80.00   11%",
                "Footer",
            ]
        );

        // The bars are in their root category's colour, and muted without a category.
        let color = |token| Some(state.theme.color(token).into());
        let bar_colors: Vec<_> = lines[1..4].iter().map(|l| l.spans[3].style.fg).collect();
        assert_eq!(
            bar_colors,
            [
                color(tokens::INFO),
                color(tokens::WARNING),
                color(tokens::TEXT_MUTED),
            ]
        );
        // Only the selected row is highlighted.
        let selection = Some(state.theme.color(tokens::BG_SELECTION).into());
        let backgrounds: Vec<_> = lines[1..4].iter().map(|l| l.style.bg).collect();
        assert_eq!(backgrounds, [None, selection, None]);
    }

    #[test]
    fn the_list_scrolls_to_the_selection_when_there_is_no_room() {
        let state = categories_state();
        let tab = TabState::new(account("a", 0));
        let spending = vec![
            spent(Some("rent"), 5000),
            spent(Some("groceries"), 3000),
            spent(Some("utilities"), 1500),
            spent(Some("internet"), 500),
        ];
        // Heading, 2 rows, footer.
        let names = |selected| {
            let lines = text(&section(spending.clone(), selected).lines(&tab, &state, 50, 4));
            assert_eq!(lines.len(), 4);
            assert_eq!(lines[3], "Footer");
            [lines[1].clone(), lines[2].clone()].map(|l| {
                l.trim_start_matches(['▎', ' '])
                    .split("  ")
                    .next()
                    .unwrap()
                    .to_string()
            })
        };
        assert_eq!(names(Some(0)), ["Rent & Mortgage", "Groceries"]);
        assert_eq!(names(Some(1)), ["Rent & Mortgage", "Groceries"]);
        assert_eq!(names(Some(2)), ["Groceries", "Utilities"]);
        assert_eq!(names(Some(3)), ["Utilities", "Internet"]);
    }

    #[test]
    fn no_spending() {
        let lines = section(vec![], None).lines(&TabState::new(account("a", 0)), &state(), 40, 10);
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
