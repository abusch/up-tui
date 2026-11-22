use std::collections::HashMap;

use opaline::ThemeSelectorState;
use ratatui::widgets::ListState;

use crate::client::models::{Account, Transaction};

use crate::config::Config;

pub enum AppMode {
    Normal,
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
        }
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

    /// Whether the next page should be fetched: there is one, nothing is loading, and
    /// the selection is within `lookahead` rows of the end of the loaded list.
    pub fn wants_next_page(&self, lookahead: usize) -> bool {
        match (&self.transactions, &self.next_page, self.loading) {
            (Some(txns), Some(_), None) => self.selected + lookahead.max(1) >= txns.len(),
            _ => false,
        }
    }
}

pub struct AppState {
    /// One tab per account, in the order the API returns them.
    pub tabs: Vec<TabState>,
    pub active_tab: usize,
    #[allow(dead_code)]
    pub mode: AppMode,
    pub should_quit: bool,
    /// Incremented for every accounts fetch; stale results are dropped.
    pub accounts_generation: u64,
    pub categories: HashMap<String, String>,
    pub theme: opaline::Theme,
    pub config: Config,
    /// Whether the detail pane is visible.
    pub show_detail: bool,
    /// Height of the transaction list area (in rows), updated during rendering.
    pub list_height: u16,
}

impl AppState {
    pub fn new(config: Config) -> Self {
        AppState {
            tabs: Vec::new(),
            active_tab: 0,
            mode: AppMode::Normal,
            should_quit: false,
            accounts_generation: 0,
            categories: HashMap::new(),
            theme: config
                .theme
                .as_deref()
                .and_then(opaline::load_by_name)
                .unwrap_or_default(),
            config,
            show_detail: false,
            list_height: 0,
        }
    }

    /// Replace the account list. Tabs for accounts that still exist are updated with
    /// the new account data (e.g. balance) but keep their loaded transactions and
    /// selection, and the active tab stays on the same account.
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
        self.active_tab = active_id.and_then(|id| self.tab_index(&id)).unwrap_or(0);
    }

    /// Index of the tab showing the given account.
    pub fn tab_index(&self, account_id: &str) -> Option<usize> {
        self.tabs.iter().position(|t| t.account.id == account_id)
    }

    pub fn current_tab(&self) -> Option<&TabState> {
        self.tabs.get(self.active_tab)
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
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::client::models::MoneyObject;

    pub fn account(id: &str, cents: i64) -> Account {
        Account {
            id: id.to_string(),
            display_name: format!("Account {id}"),
            account_type: "TRANSACTIONAL".to_string(),
            balance: MoneyObject {
                currency_code: "AUD".to_string(),
                value: format!("{}.{:02}", cents / 100, cents % 100),
                value_in_base_units: cents,
            },
        }
    }

    pub fn state() -> AppState {
        AppState::new(Config {
            api_token: "up:yeah:test".to_string(),
            theme: None,
        })
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
