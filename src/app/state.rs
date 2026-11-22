use std::collections::HashMap;
use std::sync::Arc;

use jiff::{Timestamp, ToSpan, civil::Date, tz::TimeZone};
use opaline::ThemeSelectorState;
use ratatui::widgets::ListState;

use crate::client::models::{Account, MoneyObject, Transaction};

/// The number of days, including today, that the account header's totals cover.
pub const RECENT_DAYS: i32 = 30;

/// Start of the period the account header's totals cover: midnight at the start of
/// the first of the last `RECENT_DAYS` days (including today) in `tz`.
pub fn recent_since(today: Date, tz: &TimeZone) -> Option<Timestamp> {
    let first_day = today.checked_sub((RECENT_DAYS - 1).days()).ok()?;
    Some(first_day.to_zoned_datetime(tz.clone()).ok()?.timestamp())
}

pub enum AppMode {
    Normal,
    /// The selected transaction's details are shown in a popup.
    Detail,
    Theme(ThemeSelectorState),
}

/// What kind of transaction fetch is in flight for a tab.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoadKind {
    /// Initial load or refresh: the result replaces the list.
    Replace,
    /// Next page: the result is appended to the list.
    Append,
}

/// A tab: one account plus the state of its transaction list.
pub struct TabState {
    pub account: Account,
    pub transactions: Option<Vec<Transaction>>,
    pub selected: usize,
    /// The fetch currently in flight, if any.
    pub loading: Option<LoadKind>,
    /// Incremented for every fetch started. Results carrying an older value were
    /// superseded by a later fetch and are dropped.
    pub generation: u64,
    /// URL of the next page of transactions, if there are more to load.
    pub next_page: Option<String>,
    pub list_state: ListState,
    /// All the account's transactions since `recent_since`, for the header's totals;
    /// `None` until the first successful load. Fetched separately from the paginated
    /// `transactions`.
    pub recent: Option<Vec<Transaction>>,
    /// Whether a fetch of `recent` is in flight.
    pub recent_loading: bool,
    /// Like `generation`, for fetches of `recent`.
    pub recent_generation: u64,
}

impl TabState {
    pub fn new(account: Account) -> Self {
        TabState {
            account,
            transactions: None,
            selected: 0,
            loading: None,
            generation: 0,
            next_page: None,
            list_state: ListState::default(),
            recent: None,
            recent_loading: false,
            recent_generation: 0,
        }
    }

    /// Mark a new fetch of `recent` as started and return its generation.
    pub fn start_recent_load(&mut self) -> u64 {
        self.recent_generation += 1;
        self.recent_loading = true;
        self.recent_generation
    }

    /// Money in (credits) and money out (debits) over the recent period, if loaded.
    pub fn recent_totals(&self) -> Option<(MoneyObject, MoneyObject)> {
        let recent = self.recent.as_ref()?;
        // Start from zero, so that a period with no credits (or debits) adds up to zero.
        let zero = self.account.balance.zero_like();
        let total = |debits: bool| {
            let amounts = recent
                .iter()
                .map(|t| &t.amount)
                .filter(|amount| amount.is_negative() == debits);
            MoneyObject::sum(std::iter::once(&zero).chain(amounts))
        };
        Some((total(false)?, total(true)?))
    }

    /// Mark a new fetch as started and return its generation.
    pub fn start_load(&mut self, kind: LoadKind) -> u64 {
        self.generation += 1;
        self.loading = Some(kind);
        self.generation
    }

    /// Replace the transaction list, keeping the same transaction selected if it is
    /// still present, and otherwise clamping the selection to the new list's bounds.
    /// This maintains the invariant `selected < transactions.len()` (or 0 if empty)
    /// that the navigation handlers rely on.
    pub fn set_transactions(&mut self, transactions: Vec<Transaction>, next_page: Option<String>) {
        let selected_id = self
            .transactions
            .as_ref()
            .and_then(|txns| txns.get(self.selected))
            .map(|t| t.id.as_str());
        self.selected = selected_id
            .and_then(|id| transactions.iter().position(|t| t.id == id))
            .unwrap_or_else(|| self.selected.min(transactions.len().saturating_sub(1)));
        self.transactions = Some(transactions);
        self.next_page = next_page;
    }

    /// Append a further page of transactions. The selection is unaffected.
    pub fn append_transactions(&mut self, more: Vec<Transaction>, next_page: Option<String>) {
        self.transactions.get_or_insert_with(Vec::new).extend(more);
        self.next_page = next_page;
    }

    /// The loaded transactions, or an empty slice if none are loaded yet.
    fn txns(&self) -> &[Transaction] {
        self.transactions.as_deref().unwrap_or_default()
    }

    /// Index of the last loaded transaction (0 if there are none).
    fn last_index(&self) -> usize {
        self.txns().len().saturating_sub(1)
    }

    pub fn select_next(&mut self) {
        self.selected = (self.selected + 1).min(self.last_index());
    }

    pub fn select_prev(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }

    pub fn page_down(&mut self, page_size: usize) {
        self.selected = (self.selected + page_size).min(self.last_index());
    }

    pub fn page_up(&mut self, page_size: usize) {
        self.selected = self.selected.saturating_sub(page_size);
    }

    pub fn select_first(&mut self) {
        self.selected = 0;
    }

    pub fn select_last(&mut self) {
        self.selected = self.last_index();
    }

    /// Select the first (topmost) transaction of the day group above the current one.
    /// Transactions are ordered newest first, so this is a more recent day.
    pub fn select_prev_day(&mut self, tz: &TimeZone) {
        let txns = self.txns();
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
        let prev_date = local_date(&txns[prev], tz);
        self.selected = txns[..prev]
            .iter()
            .rposition(|t| local_date(t, tz) != prev_date)
            .map_or(0, |i| i + 1);
    }

    /// Select the first (topmost) transaction of the day group below the current one.
    /// Transactions are ordered newest first, so this is an older day.
    pub fn select_next_day(&mut self, tz: &TimeZone) {
        let txns = self.txns();
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

    /// Whether the next page should be fetched: there is one, nothing is loading, and
    /// the selection is within `lookahead` rows of the end of the loaded list.
    pub fn wants_next_page(&self, lookahead: usize) -> bool {
        match (&self.transactions, &self.next_page, self.loading) {
            (Some(txns), Some(_), None) => self.selected + lookahead.max(1) >= txns.len(),
            _ => false,
        }
    }
}

/// The calendar date of a transaction in the given time zone.
fn local_date(txn: &Transaction, tz: &TimeZone) -> Date {
    txn.created_at.to_zoned_datetime(tz.clone()).date()
}

pub struct AppState {
    /// One tab per account (shown in the sidebar), grouped by account type as
    /// `Account::type_order` sorts them, and otherwise in the order the API returns
    /// them. The sidebar and next/previous account follow this order.
    pub tabs: Vec<TabState>,
    pub active_tab: usize,
    pub mode: AppMode,
    pub should_quit: bool,
    /// Incremented for every accounts fetch; stale results are dropped.
    pub accounts_generation: u64,
    pub categories: HashMap<String, String>,
    /// The theme everything is drawn with. Always opaline's global theme
    /// (`opaline::current()`), which the theme picker draws with and changes to
    /// preview themes, so the picker and the rest of the UI always match.
    pub theme: Arc<opaline::Theme>,
    /// Local time zone used to display times and group transactions by day.
    /// Looked up once at startup.
    pub tz: TimeZone,
    /// Whether the detail pane is visible.
    pub show_detail: bool,
    /// Whether the account sidebar is visible.
    pub show_sidebar: bool,
    /// Number of transactions that fit in the list view; updated during rendering.
    pub page_size: usize,
}

impl AppState {
    /// Create the initial state. `theme_name` is an opaline theme id; if it's `None`
    /// (or fails to load) opaline's default theme is used.
    /// It also becomes opaline's global theme.
    pub fn new(theme_name: Option<&str>) -> Self {
        opaline::set_theme(
            theme_name
                .and_then(opaline::load_by_name)
                .unwrap_or_default(),
        );
        AppState {
            tabs: Vec::new(),
            active_tab: 0,
            mode: AppMode::Normal,
            should_quit: false,
            accounts_generation: 0,
            categories: HashMap::new(),
            theme: opaline::current(),
            tz: TimeZone::system(),
            show_detail: false,
            show_sidebar: true,
            page_size: 0,
        }
    }

    /// Replace the account list. Tabs for accounts that still exist are updated with
    /// the new account data (e.g. balance) but keep their loaded transactions and
    /// selection, and the active tab stays on the same account. The tabs are grouped by
    /// account type.
    pub fn set_accounts(&mut self, accounts: Vec<Account>) {
        let active_id = self.current_tab().map(|t| t.account.id.clone());
        let mut old_tabs: HashMap<String, TabState> = self
            .tabs
            .drain(..)
            .map(|t| (t.account.id.clone(), t))
            .collect();
        self.tabs = accounts
            .into_iter()
            .map(|account| match old_tabs.remove(&account.id) {
                Some(mut tab) => {
                    tab.account = account;
                    tab
                }
                None => TabState::new(account),
            })
            .collect();
        // A stable sort, so accounts of the same type keep the API's order.
        self.tabs.sort_by_key(|tab| tab.account.type_order());
        self.active_tab = active_id.and_then(|id| self.tab_index(&id)).unwrap_or(0);
    }

    /// Index of the tab showing the given account.
    pub fn tab_index(&self, account_id: &str) -> Option<usize> {
        self.tabs.iter().position(|t| t.account.id == account_id)
    }

    pub fn current_tab(&self) -> Option<&TabState> {
        self.tabs.get(self.active_tab)
    }

    /// The selected transaction in the active tab, if any.
    pub fn selected_transaction(&self) -> Option<&Transaction> {
        let tab = self.current_tab()?;
        tab.transactions.as_ref()?.get(tab.selected)
    }

    pub fn current_tab_mut(&mut self) -> Option<&mut TabState> {
        self.tabs.get_mut(self.active_tab)
    }

    pub fn next_tab(&mut self) {
        if !self.tabs.is_empty() {
            self.active_tab = (self.active_tab + 1) % self.tabs.len();
        }
    }

    pub fn prev_tab(&mut self) {
        if !self.tabs.is_empty() {
            self.active_tab = if self.active_tab == 0 {
                self.tabs.len() - 1
            } else {
                self.active_tab - 1
            };
        }
    }

    pub fn category_name<'a>(&'a self, id: &'a str) -> &'a str {
        self.categories.get(id).map(|s| s.as_str()).unwrap_or(id)
    }

    /// Display name of one of the user's accounts, falling back to the id if it isn't
    /// loaded.
    pub fn account_name<'a>(&'a self, id: &'a str) -> &'a str {
        self.tab_index(id)
            .map(|i| self.tabs[i].account.display_name.as_str())
            .unwrap_or(id)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::client::models::{MoneyObject, TransactionStatus};

    pub fn money(cents: i64) -> MoneyObject {
        MoneyObject {
            currency_code: "AUD".to_string(),
            value: format!("{}.{:02}", cents / 100, (cents % 100).abs()),
            value_in_base_units: cents,
        }
    }

    /// A transaction with the given id, created at the given RFC 3339 timestamp.
    pub fn txn(id: &str, created_at: &str) -> Transaction {
        Transaction {
            id: id.to_string(),
            status: TransactionStatus::Settled,
            description: format!("Transaction {id}"),
            raw_text: None,
            message: None,
            amount: money(-1000),
            foreign_amount: None,
            hold_info: None,
            card_purchase_method: None,
            settled_at: None,
            created_at: created_at.parse().unwrap(),
            round_up: None,
            cashback: None,
            transaction_type: None,
            note: None,
            performing_customer: None,
            category: None,
            parent_category: None,
            tags: Vec::new(),
            transfer_account: None,
        }
    }

    /// A tab with transactions on three days (newest first), all in UTC:
    /// indices 0-1 on Nov 20, 2-4 on Nov 19, 5 on Nov 17.
    fn tab() -> TabState {
        let mut tab = TabState::new(account("a", 0));
        tab.set_transactions(
            vec![
                txn("t0", "2026-11-20T18:00:00Z"),
                txn("t1", "2026-11-20T09:00:00Z"),
                txn("t2", "2026-11-19T22:00:00Z"),
                txn("t3", "2026-11-19T12:00:00Z"),
                txn("t4", "2026-11-19T01:00:00Z"),
                txn("t5", "2026-11-17T15:00:00Z"),
            ],
            None,
        );
        tab
    }

    fn selected_after(start: usize, action: impl FnOnce(&mut TabState)) -> usize {
        let mut tab = tab();
        tab.selected = start;
        action(&mut tab);
        tab.selected
    }

    #[test]
    fn next_and_prev_stop_at_the_ends() {
        assert_eq!(selected_after(0, TabState::select_next), 1);
        assert_eq!(selected_after(5, TabState::select_next), 5);
        assert_eq!(selected_after(1, TabState::select_prev), 0);
        assert_eq!(selected_after(0, TabState::select_prev), 0);
    }

    #[test]
    fn paging_clamps_to_the_list() {
        assert_eq!(selected_after(1, |t| t.page_down(3)), 4);
        assert_eq!(selected_after(4, |t| t.page_down(3)), 5);
        assert_eq!(selected_after(4, |t| t.page_up(3)), 1);
        assert_eq!(selected_after(1, |t| t.page_up(3)), 0);
    }

    #[test]
    fn first_and_last() {
        assert_eq!(selected_after(3, TabState::select_first), 0);
        assert_eq!(selected_after(3, TabState::select_last), 5);
    }

    #[test]
    fn navigation_on_an_empty_or_unloaded_tab_stays_at_zero() {
        let mut tab = TabState::new(account("a", 0));
        tab.select_next();
        tab.select_last();
        tab.page_down(10);
        tab.select_next_day(&TimeZone::UTC);
        tab.select_prev_day(&TimeZone::UTC);
        assert_eq!(tab.selected, 0);
    }

    #[test]
    fn next_day_jumps_to_the_top_of_the_day_below() {
        let utc = TimeZone::UTC;
        assert_eq!(selected_after(0, |t| t.select_next_day(&utc)), 2);
        assert_eq!(selected_after(3, |t| t.select_next_day(&utc)), 5);
        assert_eq!(selected_after(5, |t| t.select_next_day(&utc)), 5);
    }

    #[test]
    fn prev_day_jumps_to_the_top_of_the_day_above() {
        let utc = TimeZone::UTC;
        assert_eq!(selected_after(5, |t| t.select_prev_day(&utc)), 2);
        assert_eq!(selected_after(3, |t| t.select_prev_day(&utc)), 0);
        assert_eq!(selected_after(1, |t| t.select_prev_day(&utc)), 1);
    }

    #[test]
    fn day_boundaries_use_the_given_time_zone() {
        // In UTC+10, t1 (Nov 20 09:00Z) and t2 (Nov 19 22:00Z) are both on Nov 20
        // local time, so the next day starts at t3 rather than t2.
        let tz = TimeZone::fixed(jiff::tz::offset(10));
        assert_eq!(selected_after(1, |t| t.select_next_day(&TimeZone::UTC)), 2);
        assert_eq!(selected_after(1, |t| t.select_next_day(&tz)), 3);
    }

    #[test]
    fn set_transactions_keeps_the_selected_transaction() {
        let mut tab = tab();
        tab.selected = 2; // t2
        tab.set_transactions(
            vec![
                txn("new", "2026-11-21T08:00:00Z"),
                txn("t0", "2026-11-20T18:00:00Z"),
                txn("t1", "2026-11-20T09:00:00Z"),
                txn("t2", "2026-11-19T22:00:00Z"),
            ],
            None,
        );
        assert_eq!(tab.selected, 3);
    }

    #[test]
    fn set_transactions_clamps_when_the_selection_is_gone() {
        let mut tab = tab();
        tab.selected = 5; // t5
        tab.set_transactions(vec![txn("t0", "2026-11-20T18:00:00Z")], None);
        assert_eq!(tab.selected, 0);
        tab.set_transactions(Vec::new(), None);
        assert_eq!(tab.selected, 0);
    }

    #[test]
    fn wants_next_page_near_the_end_only() {
        let mut tab = tab();
        tab.next_page = Some("next".to_string());
        tab.selected = 0;
        assert!(!tab.wants_next_page(3));
        tab.selected = 3;
        assert!(tab.wants_next_page(3));
        tab.loading = Some(LoadKind::Append);
        assert!(!tab.wants_next_page(3), "not while loading");
        tab.loading = None;
        tab.next_page = None;
        assert!(!tab.wants_next_page(3), "not without a next page");
    }

    pub fn account(id: &str, cents: i64) -> Account {
        Account {
            id: id.to_string(),
            display_name: format!("Account {id}"),
            account_type: "TRANSACTIONAL".to_string(),
            ownership_type: "INDIVIDUAL".to_string(),
            balance: money(cents),
        }
    }

    pub fn state() -> AppState {
        AppState::new(None)
    }

    #[test]
    fn set_accounts_creates_one_tab_per_account() {
        let mut s = state();
        s.set_accounts(vec![account("a", 100), account("b", 200)]);
        let ids: Vec<_> = s.tabs.iter().map(|t| t.account.id.as_str()).collect();
        assert_eq!(ids, ["a", "b"]);
        assert_eq!(s.active_tab, 0);
    }

    #[test]
    fn recent_totals_split_credits_and_debits() {
        let mut tab = TabState::new(account("a", 0));
        assert!(tab.recent_totals().is_none());

        let with_amount = |id, cents| Transaction {
            amount: money(cents),
            ..txn(id, "2026-11-20T18:00:00Z")
        };
        tab.recent = Some(vec![
            with_amount("t0", -1050),
            with_amount("t1", 420000),
            with_amount("t2", -250),
        ]);
        let (money_in, money_out) = tab.recent_totals().unwrap();
        assert_eq!(money_in.format_display(), "+$4200.00");
        assert_eq!(money_out.format_display(), "$13.00");

        tab.recent = Some(Vec::new());
        let (money_in, money_out) = tab.recent_totals().unwrap();
        assert_eq!(
            (money_in.value.as_str(), money_out.value.as_str()),
            ("0.00", "0.00")
        );
    }

    #[test]
    fn recent_period_starts_at_local_midnight() {
        let tz = TimeZone::get("Australia/Sydney").unwrap();
        let since = recent_since("2026-11-23".parse().unwrap(), &tz).unwrap();
        // 30 days including today: from midnight on 25 October, in daylight saving time
        // (UTC+11).
        assert_eq!(since.to_string(), "2026-10-24T13:00:00Z");
    }

    #[test]
    fn set_accounts_groups_accounts_by_type() {
        let typed = |id, account_type: &str| Account {
            account_type: account_type.to_string(),
            ..account(id, 0)
        };
        let mut s = state();
        s.set_accounts(vec![
            typed("saver1", "SAVER"),
            typed("spending", "TRANSACTIONAL"),
            typed("loan", "HOME_LOAN"),
            typed("saver2", "SAVER"),
            typed("2up", "TRANSACTIONAL"),
        ]);
        s.active_tab = s.tab_index("saver2").unwrap();
        let ids: Vec<_> = s.tabs.iter().map(|t| t.account.id.as_str()).collect();
        assert_eq!(ids, ["spending", "2up", "saver1", "saver2", "loan"]);

        // The active account stays selected when the order changes.
        s.set_accounts(vec![
            typed("saver2", "SAVER"),
            typed("spending", "TRANSACTIONAL"),
        ]);
        assert_eq!(s.current_tab().unwrap().account.id, "saver2");
    }

    #[test]
    fn set_accounts_keeps_tab_state_and_updates_account_data() {
        let mut s = state();
        s.set_accounts(vec![account("a", 100), account("b", 200)]);
        s.tabs[1].selected = 7;
        s.tabs[1].next_page = Some("next".to_string());
        s.active_tab = 1;

        // "b" moves to the front, gets a new balance, and "c" is new.
        s.set_accounts(vec![
            account("b", 999),
            account("c", 300),
            account("a", 100),
        ]);

        let ids: Vec<_> = s.tabs.iter().map(|t| t.account.id.as_str()).collect();
        assert_eq!(ids, ["b", "c", "a"]);
        assert_eq!(s.active_tab, 0, "active tab should follow account b");
        assert_eq!(s.tabs[0].selected, 7);
        assert_eq!(s.tabs[0].next_page.as_deref(), Some("next"));
        assert_eq!(s.tabs[0].account.balance.value_in_base_units, 999);
        assert_eq!(s.tabs[1].selected, 0);
    }

    #[test]
    fn set_accounts_resets_active_tab_if_its_account_is_gone() {
        let mut s = state();
        s.set_accounts(vec![account("a", 100), account("b", 200)]);
        s.active_tab = 1;
        s.set_accounts(vec![account("a", 100)]);
        assert_eq!(s.active_tab, 0);
        assert_eq!(s.tab_index("b"), None);
    }
}
