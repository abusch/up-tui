use std::sync::Arc;
use std::time::Duration;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use opaline::{ThemeSelectorAction, ThemeSelectorState};
use ratatui::prelude::*;
use ratatui::{DefaultTerminal, layout::Rect};
use ratatui_toaster::{
    ToastBuilder, ToastConstraint, ToastEngine, ToastEngineBuilder, ToastPosition, ToastType,
};
use rootcause::prelude::*;
use tokio::sync::mpsc;

use crate::app::state::{AppMode, LoadKind, TabState};
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
    pub fn new(cfg: Config) -> rootcause::Result<Self> {
        let (tx, rx) = mpsc::channel::<AppEvent>(100);
        let toast_engine = ToastEngineBuilder::new(Rect::default()).build();
        Ok(Self {
            client: Arc::new(UpClient::new(&cfg.api_token)?),
            state: AppState::new(cfg.theme.as_deref()),
            tx,
            rx,
            toaster: toast_engine,
            toast_id: 0,
            last_toast: None,
            screen_area: Rect::default(),
        })
    }

    pub async fn run(mut self, mut terminal: DefaultTerminal) -> rootcause::Result<()> {
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
                        self.ensure_transactions_loaded();
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
                    self.state.categories = categories.into_iter().collect();
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
                let Some(tab) = self
                    .state
                    .tab_index(&account_id)
                    .and_then(|idx| self.state.tabs.get_mut(idx))
                else {
                    return; // account no longer exists
                };
                if tab.generation != generation {
                    return; // superseded by a later fetch
                }
                let kind = tab.loading.take();
                match result {
                    Ok(page) if kind == Some(LoadKind::Append) => {
                        tab.append_transactions(page.data, page.next_page_url);
                    }
                    Ok(page) => {
                        tab.set_transactions(page.data, page.next_page_url);
                        self.success("Transactions loaded");
                    }
                    Err(e) => {
                        self.error(format!(
                            "Failed to load transactions: {}",
                            e.format_current_context()
                        ));
                    }
                }
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
                if self.ensure_transactions_loaded() {
                    self.info("Loading transactions...");
                }
            }
            KeyCode::BackTab | KeyCode::Char('h') => {
                self.state.prev_tab();
                if self.ensure_transactions_loaded() {
                    self.info("Loading transactions...");
                }
            }
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
            KeyCode::Char('o') => {
                self.state.show_detail = !self.state.show_detail;
            }
            KeyCode::Char('r') => {
                // Re-fetch accounts too: this refreshes balances, and recovers if the
                // initial accounts load failed (in which case there are no tabs yet).
                if self.state.tabs.is_empty() {
                    self.info("Loading accounts...");
                }
                self.fetch_accounts(true);
                let idx = self.state.active_tab;
                self.fetch_transactions(idx, LoadKind::Replace, None, true);
            }
            KeyCode::Char('t') => {
                self.state.mode = AppMode::Theme(ThemeSelectorState::with_current_selected());
            }
            _ => {}
        }

        // If navigation brought the selection near the end of what's loaded, fetch more.
        self.maybe_fetch_next_page();
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

    /// Start loading the active tab's transactions if they haven't been loaded and
    /// nothing is in flight. Returns whether a fetch was started.
    fn ensure_transactions_loaded(&mut self) -> bool {
        let idx = self.state.active_tab;
        if let Some(tab) = self.state.tabs.get(idx)
            && tab.transactions.is_none()
            && tab.loading.is_none()
        {
            self.fetch_transactions(idx, LoadKind::Replace, None, false);
            true
        } else {
            false
        }
    }

    fn maybe_fetch_next_page(&mut self) {
        let idx = self.state.active_tab;
        let lookahead = self.state.page_size;
        if let Some(tab) = self.state.tabs.get(idx)
            && tab.wants_next_page(lookahead)
        {
            let next_url = tab.next_page.clone();
            self.fetch_transactions(idx, LoadKind::Append, next_url, false);
        }
    }

    /// Fetch transactions for a tab: the first page (`next_url == None`) or the page at
    /// `next_url`. Any fetch already in flight for the tab is superseded.
    fn fetch_transactions(
        &mut self,
        tab_index: usize,
        kind: LoadKind,
        next_url: Option<String>,
        reload: bool,
    ) {
        let Some(tab) = self.state.tabs.get_mut(tab_index) else {
            return;
        };
        let account_id = tab.account.id.clone();
        let generation = tab.start_load(kind);
        let opts = PaginationOptions {
            next_url,
            ..Default::default()
        };
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
