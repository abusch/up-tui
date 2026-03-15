use std::collections::HashMap;

use ratatui::widgets::ListState;
use ratatui_themes::{Theme, ThemePalette};

use up_api::models::{Account, Transaction};

use crate::config::Config;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AppMode {
    Normal,
    Detail,
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
}

pub struct AppState {
    pub accounts: Vec<Account>,
    pub tabs: Vec<TabState>,
    pub active_tab: usize,
    pub mode: AppMode,
    pub status_message: Option<String>,
    pub status_is_error: bool,
    pub should_quit: bool,
    pub categories: HashMap<String, String>,
    pub theme: Theme,
    #[allow(dead_code)]
    pub config: Config,
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
            status_message: None,
            status_is_error: false,
            should_quit: false,
            categories: HashMap::new(),
            theme: Theme::new(config.theme),
            config,
            list_height: 0,
        }
    }

    pub fn palette(&self) -> ThemePalette {
        self.theme.palette()
    }

    pub fn next_theme(&mut self) {
        self.theme = Theme::new(self.theme.name.next());
    }

    pub fn prev_theme(&mut self) {
        self.theme = Theme::new(self.theme.name.prev());
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

    pub fn set_status(&mut self, msg: String, is_error: bool) {
        self.status_message = Some(msg);
        self.status_is_error = is_error;
    }

    pub fn clear_status(&mut self) {
        self.status_message = None;
        self.status_is_error = false;
    }

    pub fn category_name<'a>(&'a self, id: &'a str) -> &'a str {
        self.categories.get(id).map(|s| s.as_str()).unwrap_or(id)
    }
}
