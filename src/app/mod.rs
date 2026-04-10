use std::sync::Arc;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use jiff::tz::TimeZone;
use ratatui::DefaultTerminal;
use tokio::sync::mpsc;

use crate::{
    app::{
        event::{AppEvent, spawn_event_reader},
        state::AppState,
    },
    client::{UpClient, models::PaginationOptions},
    config::Config,
    ui,
};

pub mod event;
pub mod state;

pub struct App {
    client: Arc<UpClient>,
    state: AppState,
    tx: mpsc::UnboundedSender<AppEvent>,
    rx: mpsc::UnboundedReceiver<AppEvent>,
}

impl App {
    pub fn new(cfg: Config) -> anyhow::Result<Self> {
        let (tx, rx) = mpsc::unbounded_channel::<AppEvent>();
        Ok(Self {
            client: Arc::new(UpClient::new(&cfg.api_token)?),
            state: AppState::new(cfg),
            tx,
            rx,
        })
    }

    pub async fn run(mut self, mut terminal: DefaultTerminal) -> anyhow::Result<()> {
        self.state.set_status("Loading accounts...".into(), false);

        // Spawn crossterm event reader
        spawn_event_reader(self.tx.clone());

        // Fetch accounts and categories on startup
        self.fetch_accounts();
        self.fetch_categories();

        while !self.state.should_quit {
            terminal.draw(|f| ui::draw(f, &mut self.state))?;

            if let Some(event) = self.rx.recv().await {
                self.handle_event(event).await;
            }
        }

        Ok(())
    }

    pub async fn handle_event(&mut self, event: AppEvent) {
        match event {
            AppEvent::Key(key) => self.handle_key(key),
            AppEvent::AccountsLoaded(result) => match result {
                Ok(accounts) => {
                    self.state.set_accounts(accounts);
                    self.state.set_status("Accounts loaded".into(), false);
                    // Auto-fetch transactions for the first tab
                    if !self.state.accounts.is_empty() {
                        self.fetch_transactions(0);
                    }
                }
                Err(e) => {
                    self.state
                        .set_status(format!("Failed to load accounts: {}", e), true);
                }
            },
            AppEvent::CategoriesLoaded(result) => match result {
                Ok(categories) => {
                    self.state.categories = categories.into_iter().collect();
                }
                Err(e) => {
                    self.state
                        .set_status(format!("Failed to load categories: {}", e), true);
                }
            },
            AppEvent::TransactionsLoaded { tab_index, result } => {
                if let Some(tab) = self.state.tabs.get_mut(tab_index) {
                    tab.loading = false;
                    match result {
                        Ok(transactions) => {
                            tab.transactions = Some(transactions);
                        }
                        Err(e) => {
                            self.state
                                .set_status(format!("Failed to load transactions: {}", e), true);
                        }
                    }
                }
            }
        }
    }

    fn handle_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('q') | KeyCode::Char('Q') => {
                self.state.should_quit = true;
            }
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.state.should_quit = true;
            }
            KeyCode::Tab | KeyCode::Char('l') => {
                self.state.next_tab();
                self.state.clear_status();
                self.maybe_fetch_transactions();
            }
            KeyCode::BackTab | KeyCode::Char('h') => {
                self.state.prev_tab();
                self.state.clear_status();
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
                    let current_date = txns[tab.selected].created_at.to_zoned(tz.clone()).date();
                    // Find the first transaction of the previous day
                    if let Some(prev) = txns[..tab.selected]
                        .iter()
                        .rposition(|t| t.created_at.to_zoned(tz.clone()).date() != current_date)
                    {
                        // prev is in the previous day; find the first transaction of that day
                        let prev_date = txns[prev].created_at.to_zoned(tz.clone()).date();
                        let first = txns[..=prev]
                            .iter()
                            .position(|t| t.created_at.to_zoned(tz.clone()).date() == prev_date)
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
                    let current_date = txns[tab.selected].created_at.to_zoned(tz.clone()).date();
                    // Find the first transaction of the next day
                    if let Some(pos) = txns[tab.selected + 1..]
                        .iter()
                        .position(|t| t.created_at.to_zoned(tz.clone()).date() != current_date)
                    {
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
                self.state.next_theme();
                self.state.set_status(
                    format!("Theme: {}", self.state.theme.name.display_name()),
                    false,
                );
            }
            KeyCode::Char('T') => {
                self.state.prev_theme();
                self.state.set_status(
                    format!("Theme: {}", self.state.theme.name.display_name()),
                    false,
                );
            }
            _ => {}
        }
    }

    pub fn fetch_categories(&self) {
        let client = Arc::clone(&self.client);
        let tx = self.tx.clone();
        tokio::spawn(async move {
            let result = client.get_categories().await.map_err(Into::into);
            let _ = tx.send(AppEvent::CategoriesLoaded(result));
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
            let _ = tx.send(AppEvent::AccountsLoaded(result));
        });
    }

    fn maybe_fetch_transactions(&mut self) {
        let idx = self.state.active_tab;
        if let Some(tab) = self.state.tabs.get(idx)
            && tab.transactions.is_none()
            && !tab.loading
        {
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
                let _ = tx.send(AppEvent::TransactionsLoaded { tab_index, result });
            });
        }
    }
}
