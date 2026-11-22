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

pub struct TabState {
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
    pub fn new() -> Self {
        TabState {
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
    pub accounts: Vec<Account>,
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
            accounts: Vec::new(),
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

    /// Replace the account list. Tabs for accounts that still exist keep their loaded
    /// transactions and selection, and the active tab stays on the same account.
    pub fn set_accounts(&mut self, accounts: Vec<Account>) {
        let active_id = self.accounts.get(self.active_tab).map(|a| a.id.clone());
        let mut old_tabs: HashMap<String, TabState> = self
            .accounts
            .drain(..)
            .map(|a| a.id)
            .zip(self.tabs.drain(..))
            .collect();
        self.tabs = accounts
            .iter()
            .map(|a| old_tabs.remove(&a.id).unwrap_or_else(TabState::new))
            .collect();
        self.active_tab = active_id
            .and_then(|id| accounts.iter().position(|a| a.id == id))
            .unwrap_or(0);
        self.accounts = accounts;
    }

    /// Index of the tab showing the given account.
    pub fn tab_index(&self, account_id: &str) -> Option<usize> {
        self.accounts.iter().position(|a| a.id == account_id)
    }

    pub fn current_tab(&self) -> Option<&TabState> {
        self.tabs.get(self.active_tab)
    }

    pub fn current_tab_mut(&mut self) -> Option<&mut TabState> {
        self.tabs.get_mut(self.active_tab)
    }

    pub fn next_tab(&mut self) {
        if !self.accounts.is_empty() {
            self.active_tab = (self.active_tab + 1) % self.accounts.len();
        }
    }

    pub fn prev_tab(&mut self) {
        if !self.accounts.is_empty() {
            self.active_tab = if self.active_tab == 0 {
                self.accounts.len() - 1
            } else {
                self.active_tab - 1
            };
        }
    }

    pub fn category_name<'a>(&'a self, id: &'a str) -> &'a str {
        self.categories.get(id).map(|s| s.as_str()).unwrap_or(id)
    }
}
