use std::collections::HashMap;
use std::sync::Arc;

use jiff::{Timestamp, ToSpan, civil::Date, tz::TimeZone};
use opaline::ThemeSelectorState;
use opaline::names::tokens;
use ratatui::style::Color;

use crate::app::categories::Categories;
use crate::app::insights::{CategorySpending, spending_in_category};
use crate::app::list::ListSelection;
use crate::app::store::{PageOutcome, TransactionStore};
use crate::client::models::{Account, MoneyObject, PaginatedResponse, Transaction};

/// The number of days, including today, that the account header's totals and the
/// Insights view cover.
pub const RECENT_DAYS: i32 = 30;

/// Start of the period the account header's totals cover: midnight at the start of
/// the first of the last `RECENT_DAYS` days (including today) in `tz`.
pub fn recent_since(today: Date, tz: &TimeZone) -> Option<Timestamp> {
    let first_day = today.checked_sub((RECENT_DAYS - 1).days()).ok()?;
    Some(first_day.to_zoned_datetime(tz.clone()).ok()?.timestamp())
}

/// A transient status message, shown in the header bar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    pub text: String,
    pub kind: MessageKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageKind {
    /// Something is happening, e.g. loading.
    Info,
    /// Something finished.
    Success,
    /// Something failed.
    Error,
}

/// What the main area shows for the selected account.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum View {
    /// The transaction list (and the detail pane, if shown).
    Transactions,
    /// Statistics over the recent period.
    Insights,
}

/// The Insights view's state: which root category is selected in its spending list,
/// whether that category's breakdown by subcategory is shown instead of the list, and
/// which subcategory is selected in it. It's kept when switching accounts.
#[derive(Debug, Default)]
pub struct InsightsState {
    /// The selected row of the spending by root category (`spending_by_root`).
    pub root: CategorySelection,
    /// The selected row of the breakdown (`spending_in_root`).
    pub subcategory: CategorySelection,
    /// Whether the selected root category's breakdown is shown.
    pub breakdown: bool,
}

/// The transaction list's filter, opened from the Insights view: the spending in one
/// category over the recent period (`spending_in_category`), the transactions that
/// make up that category's row. It's kept when switching accounts, showing the same
/// category's spending in the other account.
#[derive(Debug, Default)]
pub struct CategoryFilter {
    /// The category id (`None`: transactions without a category).
    pub category: Option<String>,
    /// The selection in the filtered list.
    pub list: ListSelection,
}

impl CategoryFilter {
    pub fn new(category: Option<String>) -> Self {
        CategoryFilter {
            category,
            list: ListSelection::default(),
        }
    }

    /// The transactions in `tab` that the filter shows, newest first: none until its
    /// recent transactions are loaded.
    pub fn transactions<'a>(&self, tab: &'a TabState) -> Vec<&'a Transaction> {
        let recent = tab.store.recent().unwrap_or_default();
        spending_in_category(recent, self.category.as_deref())
    }
}

/// The selected row of a list of categories' spending, by category id, so it stays on
/// the same category when the order changes (after a refresh, or in another account).
#[derive(Debug, Default)]
pub struct CategorySelection {
    /// The selected category's id (the inner `None` is uncategorised spending).
    /// `None`, or a category that isn't in the list, selects the first row.
    selected: Option<Option<String>>,
}

impl CategorySelection {
    /// The index of the selected row in `rows`, if there are any.
    pub fn index(&self, rows: &[CategorySpending]) -> Option<usize> {
        if rows.is_empty() {
            return None;
        }
        let index = self
            .selected
            .as_ref()
            .and_then(|selected| rows.iter().position(|r| &r.category == selected));
        Some(index.unwrap_or(0))
    }

    /// The selected row in `rows`, if there are any.
    pub fn row<'a>(&self, rows: &'a [CategorySpending]) -> Option<&'a CategorySpending> {
        rows.get(self.index(rows)?)
    }

    pub fn select_next(&mut self, rows: &[CategorySpending]) {
        if let Some(i) = self.index(rows) {
            self.select(rows, (i + 1).min(rows.len() - 1));
        }
    }

    pub fn select_prev(&mut self, rows: &[CategorySpending]) {
        if let Some(i) = self.index(rows) {
            self.select(rows, i.saturating_sub(1));
        }
    }

    fn select(&mut self, rows: &[CategorySpending], index: usize) {
        self.selected = Some(rows[index].category.clone());
    }
}

pub enum AppMode {
    Normal,
    /// The selected transaction's details are shown in a popup.
    Detail,
    Theme(ThemeSelectorState),
}

/// A tab: one account, its transactions, and the state of its transaction list.
pub struct TabState {
    pub account: Account,
    /// The account's transactions, and their loading.
    pub store: TransactionStore,
    /// The selection in the list of all its loaded transactions.
    pub list: ListSelection,
}

impl TabState {
    pub fn new(account: Account) -> Self {
        TabState {
            account,
            store: TransactionStore::default(),
            list: ListSelection::default(),
        }
    }

    /// Money in (credits) and money out (debits) over the recent period, if loaded.
    pub fn recent_totals(&self) -> Option<(MoneyObject, MoneyObject)> {
        let recent = self.store.recent()?;
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

    /// Pass a page loaded by the fetch with this generation to the store (see
    /// `TransactionStore::page_loaded`). When it replaces the list, the same transaction
    /// stays selected if it's still there, and otherwise the selection is clamped to the
    /// new list.
    pub fn page_loaded(
        &mut self,
        generation: u64,
        page: PaginatedResponse<Transaction>,
    ) -> Option<PageOutcome> {
        let selected_id = self.txns().get(self.list.selected).map(|t| t.id.clone());
        let outcome = self.store.page_loaded(generation, page)?;
        if outcome == PageOutcome::Replaced {
            let txns = self.txns();
            self.list.selected = selected_id
                .and_then(|id| txns.iter().position(|t| t.id == id))
                .unwrap_or_else(|| self.list.selected.min(txns.len().saturating_sub(1)));
        }
        Some(outcome)
    }

    /// Replace the loaded transactions, for tests.
    #[cfg(test)]
    pub fn set_transactions(&mut self, transactions: Vec<Transaction>) {
        self.store = TransactionStore::loaded(transactions, None);
        self.list.selected = 0;
    }

    /// The loaded transactions, or an empty slice if none are loaded yet.
    fn txns(&self) -> &[Transaction] {
        self.store.transactions().unwrap_or_default()
    }

    /// Whether the list wants more transactions: the selection is within `lookahead`
    /// rows of the end of the loaded ones. The store fetches the next page if there is
    /// one (see `TransactionStore::next_fetch`).
    pub fn wants_more(&self, lookahead: usize) -> bool {
        self.list.selected + lookahead.max(1) >= self.txns().len()
    }
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
    pub categories: Categories,
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
    /// What the main area shows. It stays the same when switching accounts.
    pub view: View,
    pub insights: InsightsState,
    /// The transaction list's filter, if it's filtered. Only set in
    /// `View::Transactions`: leaving it (to go back to Insights) clears it.
    pub filter: Option<CategoryFilter>,
    /// Number of transactions that fit in the list view; updated during rendering.
    pub page_size: usize,
    /// The message shown in the header bar, until it expires (see `App::show_message`).
    pub message: Option<Message>,
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
            categories: Categories::default(),
            theme: opaline::current(),
            tz: TimeZone::system(),
            show_detail: false,
            show_sidebar: true,
            view: View::Transactions,
            insights: InsightsState::default(),
            filter: None,
            page_size: 0,
            message: None,
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

    /// The selected transaction in the transaction list, if any.
    pub fn selected_transaction(&self) -> Option<&Transaction> {
        let tab = self.current_tab()?;
        match &self.filter {
            Some(filter) => filter.list.transaction(&filter.transactions(tab)),
            None => tab.store.transactions()?.get(tab.list.selected),
        }
    }

    /// The transactions the transaction list shows in the active tab (the filter's, or
    /// all the loaded ones), and the selection in them, kept within the list.
    pub fn list_mut(&mut self) -> Option<(Vec<&Transaction>, &mut ListSelection)> {
        let tab = self.tabs.get_mut(self.active_tab)?;
        let (txns, list) = match &mut self.filter {
            Some(filter) => (filter.transactions(tab), &mut filter.list),
            None => (
                tab.store
                    .transactions()
                    .unwrap_or_default()
                    .iter()
                    .collect(),
                &mut tab.list,
            ),
        };
        list.clamp(txns.len());
        Some((txns, list))
    }

    pub fn current_tab_mut(&mut self) -> Option<&mut TabState> {
        self.tabs.get_mut(self.active_tab)
    }

    pub fn next_tab(&mut self) {
        if !self.tabs.is_empty() {
            self.select_tab((self.active_tab + 1) % self.tabs.len());
        }
    }

    pub fn prev_tab(&mut self) {
        if !self.tabs.is_empty() {
            self.select_tab(if self.active_tab == 0 {
                self.tabs.len() - 1
            } else {
                self.active_tab - 1
            });
        }
    }

    /// Make `index` the active tab. A filtered list then shows the other account's
    /// transactions, from the top.
    fn select_tab(&mut self, index: usize) {
        self.active_tab = index;
        if let Some(filter) = &mut self.filter {
            filter.list = ListSelection::default();
        }
    }

    pub fn category_name<'a>(&'a self, id: &'a str) -> &'a str {
        self.categories.name(id)
    }

    /// The colour for a category: its root category's colour token (see
    /// `Categories::color_token`). Transactions without a category, or with one that
    /// isn't known (yet), are `TEXT_MUTED`.
    pub fn category_color(&self, id: Option<&str>) -> Color {
        let token = id
            .and_then(|id| self.categories.color_token(id))
            .unwrap_or(tokens::TEXT_MUTED);
        self.theme.color(token).into()
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
        tab.set_transactions(vec![
            txn("t0", "2026-11-20T18:00:00Z"),
            txn("t1", "2026-11-20T09:00:00Z"),
            txn("t2", "2026-11-19T22:00:00Z"),
            txn("t3", "2026-11-19T12:00:00Z"),
            txn("t4", "2026-11-19T01:00:00Z"),
            txn("t5", "2026-11-17T15:00:00Z"),
        ]);
        tab
    }

    /// Refresh the tab with a single page of these transactions.
    fn refresh_with(tab: &mut TabState, transactions: Vec<Transaction>) {
        let fetch = tab.store.refresh("2026-11-01T00:00:00Z".parse().unwrap());
        let page = PaginatedResponse {
            data: transactions,
            next_page_url: None,
        };
        assert_eq!(
            tab.page_loaded(fetch.generation, page),
            Some(PageOutcome::Replaced)
        );
    }

    #[test]
    fn a_refresh_keeps_the_selected_transaction() {
        let mut tab = tab();
        tab.list.selected = 2; // t2
        refresh_with(
            &mut tab,
            vec![
                txn("new", "2026-11-21T08:00:00Z"),
                txn("t0", "2026-11-20T18:00:00Z"),
                txn("t1", "2026-11-20T09:00:00Z"),
                txn("t2", "2026-11-19T22:00:00Z"),
            ],
        );
        assert_eq!(tab.list.selected, 3);
    }

    #[test]
    fn a_refresh_clamps_the_selection_when_it_is_gone() {
        let mut tab = tab();
        tab.list.selected = 5; // t5
        refresh_with(&mut tab, vec![txn("t0", "2026-11-20T18:00:00Z")]);
        assert_eq!(tab.list.selected, 0);
        refresh_with(&mut tab, Vec::new());
        assert_eq!(tab.list.selected, 0);
    }

    #[test]
    fn wants_more_near_the_end_only() {
        let mut tab = tab();
        tab.list.selected = 0;
        assert!(!tab.wants_more(3));
        tab.list.selected = 3;
        assert!(tab.wants_more(3));
        assert!(TabState::new(account("a", 0)).wants_more(3));
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
    fn category_selection_follows_the_category() {
        let row = |id: Option<&str>| CategorySpending {
            category: id.map(str::to_owned),
            total: 1,
        };
        let rows = [row(Some("a")), row(None), row(Some("c"))];
        let mut selection = CategorySelection::default();
        assert_eq!(selection.index(&rows), Some(0));
        selection.select_prev(&rows);
        assert_eq!(selection.index(&rows), Some(0));
        selection.select_next(&rows);
        assert_eq!(selection.row(&rows), Some(&row(None)));
        selection.select_next(&rows);
        selection.select_next(&rows);
        assert_eq!(selection.index(&rows), Some(2));

        // When the order changes (another account, or a refresh), the same category
        // stays selected, or the first row if it isn't there.
        let reordered = [row(Some("c")), row(Some("a"))];
        assert_eq!(selection.index(&reordered), Some(0));
        selection.select_next(&reordered);
        assert_eq!(selection.row(&reordered), Some(&row(Some("a"))));
        assert_eq!(selection.index(&[row(Some("x"))]), Some(0));
        assert_eq!(selection.index(&[]), None);
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
        tab.set_transactions(vec![
            with_amount("t0", -1050),
            with_amount("t1", 420000),
            with_amount("t2", -250),
        ]);
        let (money_in, money_out) = tab.recent_totals().unwrap();
        assert_eq!(money_in.format_display(), "+$4200.00");
        assert_eq!(money_out.format_display(), "$13.00");

        tab.set_transactions(Vec::new());
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
        s.tabs[1].set_transactions(vec![txn("t", "2026-11-20T18:00:00Z")]);
        s.tabs[1].list.selected = 7;
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
        assert_eq!(s.tabs[0].list.selected, 7);
        assert_eq!(s.tabs[0].txns().len(), 1);
        assert_eq!(s.tabs[0].account.balance.value_in_base_units, 999);
        assert_eq!(s.tabs[1].list.selected, 0);
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
