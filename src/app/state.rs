use std::collections::HashMap;

use opaline::ThemeSelectorState;
use ratatui::widgets::ListState;

use crate::client::models::{Account, Transaction};

use crate::config::Config;

pub enum AppMode {
    Normal,
    Theme(ThemeSelectorState),
}

pub struct TabState {
    pub transactions: Option<Vec<Transaction>>,
    pub selected: usize,
    pub loading: bool,
    pub list_state: ListState,
}

impl TabState {
    pub fn new() -> Self {
        TabState {
            transactions: None,
            selected: 0,
            loading: false,
            list_state: ListState::default(),
        }
    }

    /// Replace the transaction list, keeping the same transaction selected if it is
    /// still present, and otherwise clamping the selection to the new list's bounds.
    /// This maintains the invariant `selected < transactions.len()` (or 0 if empty)
    /// that the navigation handlers rely on.
    pub fn set_transactions(&mut self, transactions: Vec<Transaction>) {
        let selected_id = self
            .transactions
            .as_ref()
            .and_then(|txns| txns.get(self.selected))
            .map(|t| t.id.as_str());
        self.selected = selected_id
            .and_then(|id| transactions.iter().position(|t| t.id == id))
            .unwrap_or_else(|| self.selected.min(transactions.len().saturating_sub(1)));
        self.transactions = Some(transactions);
    }
}

pub struct AppState {
    pub accounts: Vec<Account>,
    pub tabs: Vec<TabState>,
    pub active_tab: usize,
    #[allow(dead_code)]
    pub mode: AppMode,
    pub should_quit: bool,
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

    pub fn set_accounts(&mut self, accounts: Vec<Account>) {
        self.tabs = accounts.iter().map(|_| TabState::new()).collect();
        self.accounts = accounts;
        self.active_tab = 0;
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
