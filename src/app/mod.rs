use std::sync::Arc;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use jiff::tz::TimeZone;
use opaline::{ThemeSelectorAction, ThemeSelectorState, load_by_name};
use ratatui::prelude::*;
use ratatui::{DefaultTerminal, layout::Rect};
use ratatui_toaster::{
    ToastBuilder, ToastConstraint, ToastEngine, ToastEngineBuilder, ToastPosition, ToastType,
};
use rootcause::prelude::*;
use tokio::sync::mpsc;

use crate::app::state::AppMode;
use crate::{
    app::{
        event::{AppEvent, spawn_event_reader},
        state::AppState,
    },
    client::{UpClient, models::PaginationOptions},
    config::Config,
};

pub mod event;
pub mod state;

pub struct App {
    client: Arc<UpClient>,
    pub state: AppState,
    tx: mpsc::Sender<AppEvent>,
    rx: mpsc::Receiver<AppEvent>,
    pub toaster: ToastEngine<AppEvent>,
}

impl App {
    pub fn new(cfg: Config) -> rootcause::Result<Self> {
        let (tx, rx) = mpsc::channel::<AppEvent>(100);
        let toast_engine = ToastEngineBuilder::new(Rect::default())
            .action_tx(tx.clone())
            .build();
        Ok(Self {
            client: Arc::new(UpClient::new(&cfg.api_token)?),
            state: AppState::new(cfg),
            tx,
            rx,
            toaster: toast_engine,
        })
    }

    pub async fn run(mut self, mut terminal: DefaultTerminal) -> rootcause::Result<()> {
        // Spawn crossterm event reader
        spawn_event_reader(self.tx.clone());

        // Fetch accounts and categories on startup
        self.fetch_accounts();
        self.fetch_categories();

        self.info("Loading accounts...");
        while !self.state.should_quit {
            terminal.draw(|f| self.render(f.area(), f.buffer_mut()))?;

            match self.rx.recv().await {
                // Without keyboard input the app can't be used or even quit, so bail out.
                Some(AppEvent::InputError(e)) => {
                    return Err(e)
                        .context("Failed to read terminal input")
                        .map_err(Into::into);
                }
                Some(event) => self.handle_event(event),
                None => break,
            }
        }

        Ok(())
    }

    pub fn handle_event(&mut self, event: AppEvent) {
        match event {
            AppEvent::Key(key) => self.handle_key(key),
            // Nothing to do: receiving the event triggers a redraw at the new size.
            AppEvent::Resize => {}
            // Handled in `run`, which exits the loop.
            AppEvent::InputError(_) => {}
            AppEvent::AccountsLoaded(result) => match result {
                Ok(accounts) => {
                    self.state.set_accounts(accounts);
                    self.success("Accounts loaded");
                    // Auto-fetch transactions for the first tab
                    if !self.state.accounts.is_empty() {
                        self.fetch_transactions(0);
                    }
                }
                Err(e) => {
                    self.error(format!("Failed to load accounts: {}", e));
                }
            },
            AppEvent::CategoriesLoaded(result) => match result {
                Ok(categories) => {
                    self.success("Categories loaded");
                    self.state.categories = categories.into_iter().collect();
                }
                Err(e) => {
                    self.error(format!("Failed to load categories: {}", e));
                }
            },
            AppEvent::TransactionsLoaded { tab_index, result } => {
                if let Some(tab) = self.state.tabs.get_mut(tab_index) {
                    tab.loading = false;
                    match result {
                        Ok(transactions) => {
                            tab.set_transactions(transactions);
                            self.success("Transactions loaded");
                        }
                        Err(e) => {
                            self.error(format!("Failed to load transactions: {}", e));
                        }
                    }
                }
            }
            AppEvent::ToastShow(msg) => self.toaster.show_toast(msg),
            AppEvent::ToastHide => self.toaster.hide_toast(),
        }
    }

    fn handle_key(&mut self, key: KeyEvent) {
        match self.state.mode {
            AppMode::Normal => self.handle_normal_key(key),
            AppMode::Theme(ref mut state) => match state.handle_key(key) {
                ThemeSelectorAction::Cancel => self.state.mode = AppMode::Normal,
                ThemeSelectorAction::Select(new_theme) => {
                    self.state.theme = load_by_name(&new_theme).unwrap();
                    self.state.mode = AppMode::Normal;
                }
                ThemeSelectorAction::Navigate
                | ThemeSelectorAction::FilterChanged
                | ThemeSelectorAction::Noop => {}
            },
        }
    }

    fn handle_normal_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('q') | KeyCode::Char('Q') => {
                self.state.should_quit = true;
            }
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.state.should_quit = true;
            }
            KeyCode::Tab | KeyCode::Char('l') => {
                self.state.next_tab();
                self.maybe_fetch_transactions();
            }
            KeyCode::BackTab | KeyCode::Char('h') => {
                self.state.prev_tab();
                self.maybe_fetch_transactions();
            }
            KeyCode::Char('j') | KeyCode::Down => {
                if let Some(tab) = self.state.current_tab_mut()
                    && let Some(ref txns) = tab.transactions
                    && !txns.is_empty()
                    && tab.selected < txns.len() - 1
                {
                    tab.selected += 1;
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if let Some(tab) = self.state.current_tab_mut()
                    && tab.selected > 0
                {
                    tab.selected -= 1;
                }
            }
            KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                let page = self.state.list_height as usize;
                if let Some(tab) = self.state.current_tab_mut()
                    && let Some(ref txns) = tab.transactions
                    && !txns.is_empty()
                {
                    tab.selected = (tab.selected + page).min(txns.len() - 1);
                }
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                let page = self.state.list_height as usize;
                if let Some(tab) = self.state.current_tab_mut() {
                    tab.selected = tab.selected.saturating_sub(page);
                }
            }
            KeyCode::Char('g') => {
                if let Some(tab) = self.state.current_tab_mut() {
                    tab.selected = 0;
                }
            }
            KeyCode::Char('G') => {
                if let Some(tab) = self.state.current_tab_mut()
                    && let Some(ref txns) = tab.transactions
                    && !txns.is_empty()
                {
                    tab.selected = txns.len() - 1;
                }
            }
            KeyCode::Char('[') => {
                let tz = TimeZone::system();
                if let Some(tab) = self.state.current_tab_mut()
                    && let Some(ref txns) = tab.transactions
                    && !txns.is_empty()
                {
                    let current_date = txns[tab.selected]
                        .created_at
                        .to_zoned_datetime(tz.clone())
                        .date();
                    // Find the first transaction of the previous day
                    if let Some(prev) = txns[..tab.selected].iter().rposition(|t| {
                        t.created_at.to_zoned_datetime(tz.clone()).date() != current_date
                    }) {
                        // prev is in the previous day; find the first transaction of that day
                        let prev_date = txns[prev].created_at.to_zoned_datetime(tz.clone()).date();
                        let first = txns[..=prev]
                            .iter()
                            .position(|t| {
                                t.created_at.to_zoned_datetime(tz.clone()).date() == prev_date
                            })
                            .unwrap_or(prev);
                        tab.selected = first;
                    }
                }
            }
            KeyCode::Char(']') => {
                let tz = TimeZone::system();
                if let Some(tab) = self.state.current_tab_mut()
                    && let Some(ref txns) = tab.transactions
                    && !txns.is_empty()
                {
                    let current_date = txns[tab.selected]
                        .created_at
                        .to_zoned_datetime(tz.clone())
                        .date();
                    // Find the first transaction of the next day
                    if let Some(pos) = txns[tab.selected + 1..].iter().position(|t| {
                        t.created_at.to_zoned_datetime(tz.clone()).date() != current_date
                    }) {
                        tab.selected = tab.selected + 1 + pos;
                    }
                }
            }
            KeyCode::Enter => {}
            KeyCode::Char('o') => {
                self.state.show_detail = !self.state.show_detail;
            }
            KeyCode::Char('r') => {
                let idx = self.state.active_tab;
                self.fetch_transactions(idx);
            }
            KeyCode::Char('t') => {
                self.state.mode = AppMode::Theme(ThemeSelectorState::default());
            }
            _ => {}
        }
    }

    pub fn fetch_categories(&self) {
        let client = Arc::clone(&self.client);
        let tx = self.tx.clone();
        tokio::spawn(async move {
            let result = client.get_categories().await.map_err(Into::into);
            let _ = tx.send(AppEvent::CategoriesLoaded(result)).await;
        });
    }

    pub fn fetch_accounts(&self) {
        let client = Arc::clone(&self.client);
        let tx = self.tx.clone();
        tokio::spawn(async move {
            let result = client
                .get_accounts(PaginationOptions::default())
                .await
                .map(|page| page.data)
                .map_err(Into::into);
            let _ = tx.send(AppEvent::AccountsLoaded(result)).await;
        });
    }

    fn maybe_fetch_transactions(&mut self) {
        let idx = self.state.active_tab;
        if let Some(tab) = self.state.tabs.get(idx)
            && tab.transactions.is_none()
            && !tab.loading
        {
            self.info("Loading transactions...");
            self.fetch_transactions(idx);
        }
    }

    fn fetch_transactions(&mut self, tab_index: usize) {
        if let Some(account) = self.state.accounts.get(tab_index) {
            if let Some(tab) = self.state.tabs.get_mut(tab_index) {
                tab.loading = true;
            }
            let account_id = account.id.clone();
            let client = Arc::clone(&self.client);
            let tx = self.tx.clone();
            tokio::spawn(async move {
                let result = client
                    .get_transactions(&account_id, PaginationOptions::default())
                    .await
                    .map(|page| page.data)
                    .map_err(Into::into);
                let _ = tx
                    .send(AppEvent::TransactionsLoaded { tab_index, result })
                    .await;
            });
        }
    }

    /// Show a toast directly. This must not go through `self.tx`: the main loop is the
    /// only consumer of that channel, so awaiting a send on it here can deadlock when
    /// the channel is full.
    fn toast(&mut self, msg: impl Into<String>, typ: ToastType) {
        let msg: String = msg.into();
        self.toaster.show_toast(
            ToastBuilder::new(msg.into())
                .toast_type(typ)
                .position(ToastPosition::TopRight)
                .constraint(ToastConstraint::Auto),
        );
    }

    fn success(&mut self, msg: impl Into<String>) {
        self.toast(msg, ToastType::Success)
    }

    fn error(&mut self, msg: impl Into<String>) {
        self.toast(msg, ToastType::Error)
    }

    fn info(&mut self, msg: impl Into<String>) {
        self.toast(msg, ToastType::Info)
    }
}
