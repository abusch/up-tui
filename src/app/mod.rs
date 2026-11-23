use std::sync::Arc;
use std::time::Duration;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use opaline::{ThemeSelectorAction, ThemeSelectorState};
use ratatui::widgets::Widget;
use rootcause::prelude::*;
use tokio::sync::mpsc;

use crate::app::categories::Categories;
use crate::app::insights::{spending_by_root, spending_in_root};
use crate::app::state::{AppMode, CategoryFilter, Message, MessageKind, View};
use crate::app::store::{Fetch, LoadKind, PageOutcome};
use crate::backend::AppTerminal;
use crate::{
    app::{
        event::{AppEvent, spawn_event_reader},
        state::AppState,
    },
    client::{Client, models::PaginationOptions},
};

pub mod categories;
pub mod event;
pub mod insights;
pub mod list;
pub mod state;
pub mod store;

pub struct App {
    client: Arc<Client>,
    pub state: AppState,
    tx: mpsc::Sender<AppEvent>,
    rx: mpsc::Receiver<AppEvent>,
    /// Id of the most recently shown message; only its timer may clear it.
    message_id: u64,
}

/// How long a message stays in the header bar.
const MESSAGE_DURATION: Duration = Duration::from_secs(3);
/// How long an error stays in the header bar: longer, so it can be read.
const ERROR_DURATION: Duration = Duration::from_secs(8);

impl App {
    /// Create the app, getting its data from `client`. `theme` is an opaline theme id.
    pub fn new(client: Client, theme: Option<&str>) -> Self {
        let (tx, rx) = mpsc::channel::<AppEvent>(100);
        Self {
            client: Arc::new(client),
            state: AppState::new(theme),
            tx,
            rx,
            message_id: 0,
        }
    }

    pub async fn run(mut self, mut terminal: AppTerminal) -> rootcause::Result<()> {
        // Spawn crossterm event reader
        spawn_event_reader(self.tx.clone());

        // Fetch accounts and categories on startup
        self.fetch_accounts(false);
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
            AppEvent::AccountsLoaded { generation, result } => {
                if generation != self.state.accounts_generation {
                    return; // superseded by a later fetch
                }
                match result {
                    Ok(accounts) => {
                        self.state.set_accounts(accounts);
                        self.success("Accounts loaded");
                        self.load_tab(self.state.active_tab);
                    }
                    Err(e) => {
                        self.error(format!(
                            "Failed to load accounts: {}",
                            e.format_current_context()
                        ));
                    }
                }
            }
            AppEvent::CategoriesLoaded(result) => match result {
                Ok(categories) => {
                    self.success("Categories loaded");
                    self.state.categories = Categories::new(categories);
                }
                Err(e) => {
                    self.error(format!(
                        "Failed to load categories: {}",
                        e.format_current_context()
                    ));
                }
            },
            AppEvent::TransactionsLoaded {
                account_id,
                generation,
                result,
            } => {
                let Some(idx) = self.state.tab_index(&account_id) else {
                    return; // account no longer exists
                };
                let tab = &mut self.state.tabs[idx];
                match result {
                    Ok(page) => match tab.page_loaded(generation, page) {
                        None => return, // superseded by a later fetch
                        Some(PageOutcome::Replaced) => self.success("Transactions loaded"),
                        Some(PageOutcome::Appended | PageOutcome::Staged) => {}
                    },
                    Err(e) => {
                        if !tab.store.load_failed(generation) {
                            return; // superseded by a later fetch
                        }
                        self.error(format!(
                            "Failed to load transactions: {}",
                            e.format_current_context()
                        ));
                        // Not retried until the user does something.
                        return;
                    }
                }
                // Carry on until the period is loaded (or a refresh is complete).
                self.load_tab(idx);
            }
            AppEvent::MessageExpired(id) => {
                if id == self.message_id {
                    self.state.message = None;
                }
            }
        }
    }

    fn handle_key(&mut self, key: KeyEvent) {
        match self.state.mode {
            AppMode::Normal => self.handle_normal_key(key),
            AppMode::Theme(ref mut selector) => {
                let action = selector.handle_key(key);
                // The picker previews the highlighted theme (and applies the chosen one,
                // or restores the original on Esc) by setting opaline's global theme.
                // Follow it, so the whole UI shows the preview.
                self.state.theme = opaline::current();
                if matches!(
                    action,
                    ThemeSelectorAction::Select(_) | ThemeSelectorAction::Cancel
                ) {
                    self.state.mode = AppMode::Normal;
                }
            }
            AppMode::Help { ref mut scroll } => match key.code {
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    self.state.should_quit = true;
                }
                KeyCode::Char('?') | KeyCode::Char('q') | KeyCode::Esc => {
                    self.state.mode = AppMode::Normal;
                }
                KeyCode::Char('j') | KeyCode::Down => *scroll = scroll.saturating_add(1),
                KeyCode::Char('k') | KeyCode::Up => *scroll = scroll.saturating_sub(1),
                _ => {}
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
                self.ensure_tab_loaded();
            }
            KeyCode::BackTab | KeyCode::Char('h') => {
                self.state.prev_tab();
                self.ensure_tab_loaded();
            }
            KeyCode::Char('i') => {
                self.state.view = match self.state.view {
                    View::Transactions => View::Insights,
                    View::Insights => View::Transactions,
                };
                // The filter belongs to the transaction list it was opened in.
                self.state.filter = None;
            }
            // Back from the filtered list to the Insights breakdown it was opened from.
            KeyCode::Esc | KeyCode::Backspace if self.state.filter.is_some() => {
                self.state.filter = None;
                self.state.view = View::Insights;
            }
            // The fetch it needs (if it starts earlier) is started below.
            KeyCode::Char('p') => {
                self.state.period = self.state.period.next();
            }
            KeyCode::Char('a') => {
                self.state.show_sidebar = !self.state.show_sidebar;
            }
            KeyCode::Char('r') => {
                // Re-fetch accounts too: this refreshes balances, and recovers if the
                // initial accounts load failed (in which case there are no tabs yet).
                if self.state.tabs.is_empty() {
                    self.info("Loading accounts...");
                }
                self.fetch_accounts(true);
                self.refresh_tab(self.state.active_tab);
            }
            KeyCode::Char('t') => {
                self.state.mode = AppMode::Theme(ThemeSelectorState::with_current_selected());
            }
            KeyCode::Char('?') => {
                self.state.mode = AppMode::Help { scroll: 0 };
            }
            _ if self.state.view == View::Insights => self.handle_insights_key(key),
            KeyCode::Enter => {
                self.state.show_detail = !self.state.show_detail;
            }
            _ => self.handle_navigation_key(key),
        }

        // If navigation brought the selection near the end of what's loaded, fetch more
        // (or retry a failed fetch).
        self.load_tab(self.state.active_tab);
    }

    /// Normal-mode keys in the Insights view, other than those common to both views:
    /// they select a root category in the spending list, open or close its breakdown
    /// by subcategory, and select a subcategory in the breakdown.
    fn handle_insights_key(&mut self, key: KeyEvent) {
        let Some(recent) = self.state.current_tab().and_then(|t| t.store.recent()) else {
            return;
        };
        let roots = spending_by_root(recent);
        let insights = &self.state.insights;
        let breakdown = match insights.root.row(&roots) {
            Some(root) if insights.breakdown => {
                Some(spending_in_root(recent, root.category.as_deref()))
            }
            _ => None,
        };
        // The list that j/k move in: the breakdown's subcategories, or the roots.
        let insights = &mut self.state.insights;
        let (selection, rows) = match breakdown {
            Some(subcategories) => (&mut insights.subcategory, subcategories),
            None => (&mut insights.root, roots),
        };
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => selection.select_next(&rows),
            KeyCode::Char('k') | KeyCode::Up => selection.select_prev(&rows),
            // In the breakdown: show the selected subcategory's transactions.
            KeyCode::Enter if insights.breakdown => {
                if let Some(row) = selection.row(&rows) {
                    self.state.filter = Some(CategoryFilter::new(row.category.clone()));
                    self.state.view = View::Transactions;
                }
            }
            KeyCode::Enter if !rows.is_empty() => insights.breakdown = true,
            KeyCode::Esc | KeyCode::Backspace => insights.breakdown = false,
            _ => {}
        }
    }

    /// Keys that move the selection in the transaction list (the active tab's, or the
    /// filtered one).
    fn handle_navigation_key(&mut self, key: KeyEvent) {
        let page_size = self.state.page_size;
        let tz = self.state.tz.clone();
        let Some((txns, list)) = self.state.list_mut() else {
            return;
        };
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => list.select_next(&txns),
            KeyCode::Char('k') | KeyCode::Up => list.select_prev(),
            KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                list.page_down(&txns, page_size);
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                list.page_up(page_size);
            }
            KeyCode::Char('g') => list.select_first(),
            KeyCode::Char('G') => list.select_last(&txns),
            KeyCode::Char('[') => list.select_prev_day(&txns, &tz),
            KeyCode::Char(']') => list.select_next_day(&txns, &tz),
            _ => {}
        }
    }

    pub fn fetch_categories(&self) {
        let client = Arc::clone(&self.client);
        let tx = self.tx.clone();
        tokio::spawn(async move {
            let result = client.get_categories().await;
            let _ = tx.send(AppEvent::CategoriesLoaded(result)).await;
        });
    }

    pub fn fetch_accounts(&mut self, reload: bool) {
        self.state.accounts_generation += 1;
        let generation = self.state.accounts_generation;
        let client = Arc::clone(&self.client);
        let tx = self.tx.clone();
        tokio::spawn(async move {
            let result = client
                .get_accounts(PaginationOptions::default(), reload)
                .await
                .map(|page| page.data);
            let _ = tx
                .send(AppEvent::AccountsLoaded { generation, result })
                .await;
        });
    }

    /// Start loading the active tab's transactions if they haven't loaded (or a
    /// previous load failed), and nothing is in flight.
    fn ensure_tab_loaded(&mut self) {
        let idx = self.state.active_tab;
        if self.load_tab(idx) && self.state.tabs[idx].store.transactions().is_none() {
            self.info("Loading transactions...");
        }
    }

    /// Start the fetch the tab needs next, if any (see `TransactionStore::next_fetch`).
    /// Returns whether one was started.
    fn load_tab(&mut self, tab_index: usize) -> bool {
        let Some(since) = self.period_start() else {
            return false;
        };
        let lookahead = self.state.page_size;
        let Some(tab) = self.state.tabs.get_mut(tab_index) else {
            return false;
        };
        let wants_more = tab.wants_more(lookahead);
        let Some(fetch) = tab.store.next_fetch(since, wants_more) else {
            return false;
        };
        let account_id = tab.account.id.clone();
        self.spawn_fetch(account_id, fetch);
        true
    }

    /// Reload the tab's transactions from the newest page, superseding any fetch in
    /// flight.
    fn refresh_tab(&mut self, tab_index: usize) {
        let Some(since) = self.period_start() else {
            return;
        };
        let Some(tab) = self.state.tabs.get_mut(tab_index) else {
            return;
        };
        let fetch = tab.store.refresh(since);
        let account_id = tab.account.id.clone();
        self.spawn_fetch(account_id, fetch);
    }

    /// The start of the selected period, as of now.
    fn period_start(&self) -> Option<jiff::Timestamp> {
        self.state.period.start(self.state.today(), &self.state.tz)
    }

    /// Fetch a page of an account's transactions, sending the result as
    /// `TransactionsLoaded`.
    fn spawn_fetch(&self, account_id: String, fetch: Fetch) {
        let opts = PaginationOptions {
            page_size: store::PAGE_SIZE,
            next_url: fetch.url,
        };
        // Pages fetched from the newest one (initial loads and refreshes) bypass the
        // HTTP cache, so a refresh gets up-to-date transactions.
        let reload = fetch.kind == LoadKind::Replace;
        let generation = fetch.generation;
        let client = Arc::clone(&self.client);
        let tx = self.tx.clone();
        tokio::spawn(async move {
            let result = client.get_transactions(&account_id, opts, reload).await;
            let _ = tx
                .send(AppEvent::TransactionsLoaded {
                    account_id,
                    generation,
                    result,
                })
                .await;
        });
    }

    /// Show a message in the header bar, replacing any other, and clear it after a
    /// while. This must not go through `self.tx`: the main loop is the only consumer of
    /// that channel, so awaiting a send on it here can deadlock when the channel is full.
    fn show_message(&mut self, text: impl Into<String>, kind: MessageKind) {
        self.state.message = Some(Message {
            text: text.into(),
            kind,
        });

        // Clear it after a while, unless another message has replaced it by then.
        self.message_id += 1;
        let id = self.message_id;
        let duration = match kind {
            MessageKind::Error => ERROR_DURATION,
            MessageKind::Info | MessageKind::Success => MESSAGE_DURATION,
        };
        let tx = self.tx.clone();
        tokio::spawn(async move {
            tokio::time::sleep(duration).await;
            let _ = tx.send(AppEvent::MessageExpired(id)).await;
        });
    }

    fn success(&mut self, msg: impl Into<String>) {
        self.show_message(msg, MessageKind::Success)
    }

    fn error(&mut self, msg: impl Into<String>) {
        self.show_message(msg, MessageKind::Error)
    }

    fn info(&mut self, msg: impl Into<String>) {
        self.show_message(msg, MessageKind::Info)
    }
}
