//! The selection in a list of transactions: an account's transactions, or those that
//! a filter matches.

use jiff::{civil::Date, tz::TimeZone};
use ratatui::widgets::ListState;

use crate::client::models::Transaction;

/// The selected transaction in a list, and the list's scroll position. The navigation
/// methods take the transactions the list shows, newest first, and keep `selected` in
/// bounds: `selected < len` (or 0 if the list is empty).
#[derive(Debug, Default)]
pub struct ListSelection {
    /// Index of the selected transaction.
    pub selected: usize,
    /// The ratatui list state, which scrolls the list to the selection.
    pub list_state: ListState,
}

impl ListSelection {
    /// The selected transaction, if there's one.
    pub fn transaction<'a>(&self, txns: &[&'a Transaction]) -> Option<&'a Transaction> {
        txns.get(self.selected).copied()
    }

    /// Bring the selection back within a list of `len` transactions, for a list that
    /// has changed (e.g. a filter's, after a refresh).
    pub fn clamp(&mut self, len: usize) {
        self.selected = self.selected.min(len.saturating_sub(1));
    }

    pub fn select_next(&mut self, txns: &[&Transaction]) {
        self.selected = (self.selected + 1).min(last_index(txns));
    }

    pub fn select_prev(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }

    pub fn page_down(&mut self, txns: &[&Transaction], page_size: usize) {
        self.selected = (self.selected + page_size).min(last_index(txns));
    }

    pub fn page_up(&mut self, page_size: usize) {
        self.selected = self.selected.saturating_sub(page_size);
    }

    pub fn select_first(&mut self) {
        self.selected = 0;
    }

    pub fn select_last(&mut self, txns: &[&Transaction]) {
        self.selected = last_index(txns);
    }

    /// Select the first (topmost) transaction of the day group above the current one.
    /// Transactions are ordered newest first, so this is a more recent day.
    pub fn select_prev_day(&mut self, txns: &[&Transaction], tz: &TimeZone) {
        let Some(current) = txns.get(self.selected) else {
            return;
        };
        let current_date = local_date(current, tz);
        let Some(prev) = txns[..self.selected]
            .iter()
            .rposition(|t| local_date(t, tz) != current_date)
        else {
            return; // already in the first day group
        };
        let prev_date = local_date(txns[prev], tz);
        self.selected = txns[..prev]
            .iter()
            .rposition(|t| local_date(t, tz) != prev_date)
            .map_or(0, |i| i + 1);
    }

    /// Select the first (topmost) transaction of the day group below the current one.
    /// Transactions are ordered newest first, so this is an older day.
    pub fn select_next_day(&mut self, txns: &[&Transaction], tz: &TimeZone) {
        let Some(current) = txns.get(self.selected) else {
            return;
        };
        let current_date = local_date(current, tz);
        let next = txns[self.selected + 1..]
            .iter()
            .position(|t| local_date(t, tz) != current_date);
        if let Some(offset) = next {
            self.selected += 1 + offset;
        }
    }
}

/// Index of the last transaction (0 if there are none).
fn last_index(txns: &[&Transaction]) -> usize {
    txns.len().saturating_sub(1)
}

/// The calendar date of a transaction in the given time zone.
fn local_date(txn: &Transaction, tz: &TimeZone) -> Date {
    txn.created_at.to_zoned_datetime(tz.clone()).date()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::state::tests::txn;

    /// Transactions on three days (newest first), all in UTC:
    /// indices 0-1 on Nov 20, 2-4 on Nov 19, 5 on Nov 17.
    fn transactions() -> Vec<Transaction> {
        vec![
            txn("t0", "2026-11-20T18:00:00Z"),
            txn("t1", "2026-11-20T09:00:00Z"),
            txn("t2", "2026-11-19T22:00:00Z"),
            txn("t3", "2026-11-19T12:00:00Z"),
            txn("t4", "2026-11-19T01:00:00Z"),
            txn("t5", "2026-11-17T15:00:00Z"),
        ]
    }

    fn selected_after(
        start: usize,
        action: impl FnOnce(&mut ListSelection, &[&Transaction]),
    ) -> usize {
        let transactions = transactions();
        let txns: Vec<_> = transactions.iter().collect();
        let mut list = ListSelection {
            selected: start,
            ..ListSelection::default()
        };
        action(&mut list, &txns);
        list.selected
    }

    #[test]
    fn next_and_prev_stop_at_the_ends() {
        assert_eq!(selected_after(0, |l, t| l.select_next(t)), 1);
        assert_eq!(selected_after(5, |l, t| l.select_next(t)), 5);
        assert_eq!(selected_after(1, |l, _| l.select_prev()), 0);
        assert_eq!(selected_after(0, |l, _| l.select_prev()), 0);
    }

    #[test]
    fn paging_clamps_to_the_list() {
        assert_eq!(selected_after(1, |l, t| l.page_down(t, 3)), 4);
        assert_eq!(selected_after(4, |l, t| l.page_down(t, 3)), 5);
        assert_eq!(selected_after(4, |l, _| l.page_up(3)), 1);
        assert_eq!(selected_after(1, |l, _| l.page_up(3)), 0);
    }

    #[test]
    fn first_and_last() {
        assert_eq!(selected_after(3, |l, _| l.select_first()), 0);
        assert_eq!(selected_after(3, |l, t| l.select_last(t)), 5);
    }

    #[test]
    fn navigation_on_an_empty_list_stays_at_zero() {
        let mut list = ListSelection::default();
        list.select_next(&[]);
        list.select_last(&[]);
        list.page_down(&[], 10);
        list.select_next_day(&[], &TimeZone::UTC);
        list.select_prev_day(&[], &TimeZone::UTC);
        assert_eq!(list.selected, 0);
    }

    #[test]
    fn clamping_keeps_the_selection_in_the_list() {
        assert_eq!(selected_after(5, |l, _| l.clamp(3)), 2);
        assert_eq!(selected_after(1, |l, _| l.clamp(3)), 1);
        assert_eq!(selected_after(1, |l, _| l.clamp(0)), 0);
    }

    #[test]
    fn next_day_jumps_to_the_top_of_the_day_below() {
        let utc = TimeZone::UTC;
        assert_eq!(selected_after(0, |l, t| l.select_next_day(t, &utc)), 2);
        assert_eq!(selected_after(3, |l, t| l.select_next_day(t, &utc)), 5);
        assert_eq!(selected_after(5, |l, t| l.select_next_day(t, &utc)), 5);
    }

    #[test]
    fn prev_day_jumps_to_the_top_of_the_day_above() {
        let utc = TimeZone::UTC;
        assert_eq!(selected_after(5, |l, t| l.select_prev_day(t, &utc)), 2);
        assert_eq!(selected_after(3, |l, t| l.select_prev_day(t, &utc)), 0);
        assert_eq!(selected_after(1, |l, t| l.select_prev_day(t, &utc)), 1);
    }

    #[test]
    fn day_boundaries_use_the_given_time_zone() {
        // In UTC+10, t1 (Nov 20 09:00Z) and t2 (Nov 19 22:00Z) are both on Nov 20
        // local time, so the next day starts at t3 rather than t2.
        let tz = TimeZone::fixed(jiff::tz::offset(10));
        let utc = TimeZone::UTC;
        assert_eq!(selected_after(1, |l, t| l.select_next_day(t, &utc)), 2);
        assert_eq!(selected_after(1, |l, t| l.select_next_day(t, &tz)), 3);
    }
}
