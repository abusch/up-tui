//! Statistics for the Insights view, computed from an account's recent transactions
//! (`TabState::recent`).

use std::collections::HashMap;

use jiff::{civil::Date, tz::TimeZone};

use crate::app::state::RECENT_DAYS;
use crate::client::models::Transaction;

/// Money spent in one category over the recent period.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CategorySpending {
    /// The category id, or `None` for transactions without a category.
    pub category: Option<String>,
    /// The amount spent, in base units (e.g. cents); positive.
    pub total: i64,
}

/// Money spent by category: the debits, excluding transfers between the user's own
/// accounts (including round-ups to savers), largest first. Refunds aren't subtracted.
pub fn spending_by_category(transactions: &[Transaction]) -> Vec<CategorySpending> {
    let mut totals: HashMap<Option<&str>, i64> = HashMap::new();
    for txn in transactions
        .iter()
        .filter(|t| t.amount.is_negative() && t.transfer_account.is_none())
    {
        let total = totals.entry(txn.category.as_deref()).or_default();
        *total = total.saturating_sub(txn.amount.value_in_base_units);
    }
    let mut spending: Vec<_> = totals
        .into_iter()
        .map(|(category, total)| CategorySpending {
            category: category.map(str::to_owned),
            total,
        })
        .collect();
    // Ties are ordered by category id, so the order doesn't change between renders.
    spending.sort_by(|a, b| {
        b.total
            .cmp(&a.total)
            .then_with(|| a.category.cmp(&b.category))
    });
    spending
}

/// The balance at the end of each of the last `RECENT_DAYS` days (including today),
/// oldest first, in base units. It's worked out backwards from `balance`, the current
/// balance, through `transactions`, which must be all the transactions since the start
/// of the first day (as `TabState::recent` is). Days are local dates in `tz`.
pub fn daily_balances(
    balance: i64,
    transactions: &[Transaction],
    tz: &TimeZone,
    today: Date,
) -> Vec<(Date, i64)> {
    let mut net_by_day: HashMap<Date, i64> = HashMap::new();
    for txn in transactions {
        let day = txn.created_at.to_zoned_datetime(tz.clone()).date();
        let net = net_by_day.entry(day).or_default();
        *net = net.saturating_add(txn.amount.value_in_base_units);
    }

    let mut balances = Vec::new();
    let mut day = today;
    let mut end_of_day = balance;
    for _ in 0..RECENT_DAYS {
        balances.push((day, end_of_day));
        // The balance at the end of the previous day is before this day's transactions.
        end_of_day = end_of_day.saturating_sub(net_by_day.get(&day).copied().unwrap_or(0));
        let Ok(previous) = day.yesterday() else {
            break;
        };
        day = previous;
    }
    balances.reverse();
    balances
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::state::tests::{money, txn};

    fn spent(id: &str, created_at: &str, cents: i64, category: Option<&str>) -> Transaction {
        Transaction {
            amount: money(cents),
            category: category.map(str::to_owned),
            ..txn(id, created_at)
        }
    }

    #[test]
    fn spending_counts_debits_by_category_without_transfers() {
        let mut transfer = spent("t", "2026-11-20T10:00:00Z", -50000, None);
        transfer.transfer_account = Some("saver".to_string());
        let transactions = [
            spent("a", "2026-11-20T10:00:00Z", -1250, Some("groceries")),
            spent("b", "2026-11-19T10:00:00Z", -3000, Some("restaurants")),
            spent("c", "2026-11-18T10:00:00Z", -2000, Some("groceries")),
            spent("d", "2026-11-18T10:00:00Z", -500, None),
            // A refund and a salary: not spending.
            spent("e", "2026-11-17T10:00:00Z", 1000, Some("groceries")),
            spent("f", "2026-11-17T10:00:00Z", 400000, None),
            transfer,
        ];
        let category = |c: Option<&str>, total| CategorySpending {
            category: c.map(str::to_owned),
            total,
        };
        assert_eq!(
            spending_by_category(&transactions),
            [
                category(Some("groceries"), 3250),
                category(Some("restaurants"), 3000),
                category(None, 500),
            ]
        );
    }

    #[test]
    fn balances_are_worked_back_from_the_current_balance() {
        let tz = TimeZone::UTC;
        let today: Date = "2026-11-23".parse().unwrap();
        let transactions = [
            spent("a", "2026-11-23T09:00:00Z", -1000, None),
            spent("b", "2026-11-22T09:00:00Z", 5000, None),
            spent("c", "2026-11-22T18:00:00Z", -2000, None),
            spent("d", "2026-11-20T09:00:00Z", -500, None),
        ];
        let balances = daily_balances(10000, &transactions, &tz, today);
        assert_eq!(balances.len(), RECENT_DAYS as usize);
        assert_eq!(balances[0].0, "2026-10-25".parse::<Date>().unwrap());
        let last: Vec<_> = balances[balances.len() - 5..]
            .iter()
            .map(|&(day, balance)| (day.to_string(), balance))
            .collect();
        assert_eq!(
            last,
            [
                ("2026-11-19".to_string(), 8500),
                ("2026-11-20".to_string(), 8000),
                ("2026-11-21".to_string(), 8000),
                ("2026-11-22".to_string(), 11000),
                ("2026-11-23".to_string(), 10000),
            ]
        );
        // Before the earliest transaction, the balance doesn't change.
        assert!(
            balances[..balances.len() - 5]
                .iter()
                .all(|&(_, b)| b == 8500)
        );
    }
}
