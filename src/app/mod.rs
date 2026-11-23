use std::sync::Arc;
use std::time::Duration;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use opaline::{ThemeSelectorAction, ThemeSelectorState};
use ratatui::layout::Rect;
use ratatui::prelude::*;
use ratatui_toaster::{
    ToastBuilder, ToastConstraint, ToastEngine, ToastEngineBuilder, ToastPosition, ToastType,
};
use rootcause::prelude::*;
use tokio::sync::mpsc;

use crate::app::categories::Categories;
use crate::app::state::{AppMode, TabState, View, recent_since};
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
pub mod state;
pub mod store;

pub struct App {
    client: Arc<Client>,
    pub state: AppState,
    tx: mpsc::Sender<AppEvent>,
    rx: mpsc::Receiver<AppEvent>,
    // The toaster's built-in auto-hide is not used (no `action_tx`): each of its timers
    // hides whichever toast is current, cutting later toasts short. See `toast()`.
    toaster: ToastEngine<()>,
    /// Id of the most recently shown toast; only its timer may hide it.
    toast_id: u64,
    /// The most recently shown toast, so it can be re-laid-out when the screen resizes.
    last_toast: Option<(String, ToastType)>,
    /// Screen area of the last render, used to position toasts.
    screen_area: Rect,
}

const TOAST_DURATION: Duration = Duration::from_secs(3);

fn toast_builder(msg: String, typ: ToastType) -> ToastBuilder {
    ToastBuilder::new(msg.into())
        .toast_type(typ)
        .position(ToastPosition::TopRight)
        .constraint(ToastConstraint::Auto)
}

impl App {
    /// Create the app, getting its data from `client`. `theme` is an opaline theme id.
    pub fn new(client: Client, theme: Option<&str>) -> Self {
        let (tx, rx) = mpsc::channel::<AppEvent>(100);
        let toast_engine = ToastEngineBuilder::new(Rect::default()).build();
        Self {
            client: Arc::new(client),
            state: AppState::new(theme),
            tx,
            rx,
            toaster: toast_engine,
            toast_id: 0,
            last_toast: None,
            screen_area: Rect::default(),
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
                // Carry on until the recent period is loaded (or a refresh is complete).
                self.load_tab(idx);
            }
            AppEvent::ToastExpired(id) => {
                if id == self.toast_id {
                    self.toaster.hide_toast();
                    self.last_toast = None;
                }
            }
        }
    }

    fn handle_key(&mut self, key: KeyEvent) {
        match self.state.mode {
            AppMode::Normal => self.handle_normal_key(key),
            AppMode::Detail => self.handle_detail_key(key),
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
            }
            KeyCode::Char('s') => {
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
            // The rest act on the transaction list, which isn't shown in Insights.
            _ if self.state.view != View::Transactions => {}
            KeyCode::Enter => {
                if self.state.selected_transaction().is_some() {
                    self.state.mode = AppMode::Detail;
                }
            }
            KeyCode::Char('o') => {
                self.state.show_detail = !self.state.show_detail;
            }
            _ => self.handle_navigation_key(key),
        }

        // If navigation brought the selection near the end of what's loaded, fetch more
        // (or retry a failed fetch).
        self.load_tab(self.state.active_tab);
    }

    /// Keys while the detail popup is open: it can be closed, and the navigation keys
    /// move through the transactions, with the popup showing the selected one.
    fn handle_detail_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc | KeyCode::Enter | KeyCode::Char('q') | KeyCode::Char('Q') => {
                self.state.mode = AppMode::Normal;
            }
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.state.should_quit = true;
            }
            _ => self.handle_navigation_key(key),
        }

        self.load_tab(self.state.active_tab);
    }

    /// Keys that move the selection in the active tab's transaction list.
    fn handle_navigation_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => self.navigate(TabState::select_next),
            KeyCode::Char('k') | KeyCode::Up => self.navigate(TabState::select_prev),
            KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                let page_size = self.state.page_size;
                self.navigate(|tab| tab.page_down(page_size));
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                let page_size = self.state.page_size;
                self.navigate(|tab| tab.page_up(page_size));
            }
            KeyCode::Char('g') => self.navigate(TabState::select_first),
            KeyCode::Char('G') => self.navigate(TabState::select_last),
            KeyCode::Char('[') => {
                let tz = self.state.tz.clone();
                self.navigate(|tab| tab.select_prev_day(&tz));
            }
            KeyCode::Char(']') => {
                let tz = self.state.tz.clone();
                self.navigate(|tab| tab.select_next_day(&tz));
            }
            _ => {}
        }
    }

    /// Apply a navigation action to the active tab, if there is one.
    fn navigate(&mut self, action: impl FnOnce(&mut TabState)) {
        if let Some(tab) = self.state.current_tab_mut() {
            action(tab);
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
        let Some(since) = self.recent_since() else {
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
        let Some(since) = self.recent_since() else {
            return;
        };
        let Some(tab) = self.state.tabs.get_mut(tab_index) else {
            return;
        };
        let fetch = tab.store.refresh(since);
        let account_id = tab.account.id.clone();
        self.spawn_fetch(account_id, fetch);
    }

    /// The start of the recent period, as of now.
    fn recent_since(&self) -> Option<jiff::Timestamp> {
        let today = jiff::Timestamp::now()
            .to_zoned_datetime(self.state.tz.clone())
            .date();
        recent_since(today, &self.state.tz)
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

    /// Show a toast directly. This must not go through `self.tx`: the main loop is the
    /// only consumer of that channel, so awaiting a send on it here can deadlock when
    /// the channel is full.
    fn toast(&mut self, msg: impl Into<String>, typ: ToastType) {
        let msg: String = msg.into();
        self.toaster.show_toast(toast_builder(msg.clone(), typ));
        self.last_toast = Some((msg, typ));

        // Hide it after a while, unless another toast has replaced it by then.
        self.toast_id += 1;
        let id = self.toast_id;
        let tx = self.tx.clone();
        tokio::spawn(async move {
            tokio::time::sleep(TOAST_DURATION).await;
            let _ = tx.send(AppEvent::ToastExpired(id)).await;
        });
    }

    /// Render the current toast, if any, over the whole screen `area`.
    ///
    /// A toast's position is computed when it's shown, so if the screen size changed
    /// since the last render the visible toast is re-shown to lay it out for the new
    /// size. This also places toasts that were shown before the first render.
    pub fn render_toasts(&mut self, area: Rect, buf: &mut Buffer) {
        if area != self.screen_area {
            self.screen_area = area;
            self.toaster.set_area(area);
            if self.toaster.has_toast()
                && let Some((msg, typ)) = &self.last_toast
            {
                self.toaster.show_toast(toast_builder(msg.clone(), *typ));
            }
        }
        // The toaster is only used for layout: its own rendering has hardcoded colours.
        if self.toaster.has_toast()
            && let Some((msg, typ)) = &self.last_toast
        {
            crate::ui::toast::draw_toast(
                buf,
                self.toaster.toast_area(),
                msg,
                *typ,
                &self.state.theme,
            );
        }
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
