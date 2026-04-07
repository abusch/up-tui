use std::sync::Arc;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use jiff::tz::TimeZone;
use tokio::sync::mpsc;

use crate::app::event::AppEvent;
use crate::app::state::AppState;
use crate::client::UpClient;
use crate::client::models::PaginationOptions;

pub fn handle_event(
    state: &mut AppState,
    event: AppEvent,
    client: &Arc<UpClient>,
    tx: &mpsc::UnboundedSender<AppEvent>,
) {
    match event {
        AppEvent::Key(key) => handle_key(state, key, client, tx),
        AppEvent::AccountsLoaded(result) => match result {
            Ok(accounts) => {
                state.set_accounts(accounts);
                state.set_status("Accounts loaded".into(), false);
                // Auto-fetch transactions for the first tab
                if !state.accounts.is_empty() {
                    fetch_transactions(state, 0, client, tx);
                }
            }
            Err(e) => {
                state.set_status(format!("Failed to load accounts: {}", e), true);
            }
        },
        AppEvent::CategoriesLoaded(result) => match result {
            Ok(categories) => {
                state.categories = categories.into_iter().collect();
            }
            Err(e) => {
                state.set_status(format!("Failed to load categories: {}", e), true);
            }
        },
        AppEvent::TransactionsLoaded { tab_index, result } => {
            if let Some(tab) = state.tabs.get_mut(tab_index) {
                tab.loading = false;
                match result {
                    Ok(transactions) => {
                        tab.transactions = Some(transactions);
                    }
                    Err(e) => {
                        state.set_status(format!("Failed to load transactions: {}", e), true);
                    }
                }
            }
        }
    }
}

fn handle_key(
    state: &mut AppState,
    key: KeyEvent,
    client: &Arc<UpClient>,
    tx: &mpsc::UnboundedSender<AppEvent>,
) {
    handle_normal_key(state, key, client, tx);
}

fn handle_normal_key(
    state: &mut AppState,
    key: KeyEvent,
    client: &Arc<UpClient>,
    tx: &mpsc::UnboundedSender<AppEvent>,
) {
    match key.code {
        KeyCode::Char('q') | KeyCode::Char('Q') => {
            state.should_quit = true;
        }
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            state.should_quit = true;
        }
        KeyCode::Tab | KeyCode::Char('l') => {
            state.next_tab();
            state.clear_status();
            maybe_fetch_transactions(state, client, tx);
        }
        KeyCode::BackTab | KeyCode::Char('h') => {
            state.prev_tab();
            state.clear_status();
            maybe_fetch_transactions(state, client, tx);
        }
        KeyCode::Char('j') | KeyCode::Down => {
            if let Some(tab) = state.current_tab_mut()
                && let Some(ref txns) = tab.transactions
                && !txns.is_empty()
                && tab.selected < txns.len() - 1
            {
                tab.selected += 1;
            }
        }
        KeyCode::Char('k') | KeyCode::Up => {
            if let Some(tab) = state.current_tab_mut()
                && tab.selected > 0
            {
                tab.selected -= 1;
            }
        }
        KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            let page = state.list_height as usize;
            if let Some(tab) = state.current_tab_mut()
                && let Some(ref txns) = tab.transactions
                && !txns.is_empty()
            {
                tab.selected = (tab.selected + page).min(txns.len() - 1);
            }
        }
        KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            let page = state.list_height as usize;
            if let Some(tab) = state.current_tab_mut() {
                tab.selected = tab.selected.saturating_sub(page);
            }
        }
        KeyCode::Char('g') => {
            if let Some(tab) = state.current_tab_mut() {
                tab.selected = 0;
            }
        }
        KeyCode::Char('G') => {
            if let Some(tab) = state.current_tab_mut()
                && let Some(ref txns) = tab.transactions
                && !txns.is_empty()
            {
                tab.selected = txns.len() - 1;
            }
        }
        KeyCode::Char('[') => {
            let tz = TimeZone::system();
            if let Some(tab) = state.current_tab_mut()
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
            if let Some(tab) = state.current_tab_mut()
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
            state.show_detail = !state.show_detail;
        }
        KeyCode::Char('r') => {
            let idx = state.active_tab;
            fetch_transactions(state, idx, client, tx);
        }
        KeyCode::Char('t') => {
            state.next_theme();
            state.set_status(format!("Theme: {}", state.theme.name.display_name()), false);
        }
        KeyCode::Char('T') => {
            state.prev_theme();
            state.set_status(format!("Theme: {}", state.theme.name.display_name()), false);
        }
        _ => {}
    }
}

fn maybe_fetch_transactions(
    state: &mut AppState,
    client: &Arc<UpClient>,
    tx: &mpsc::UnboundedSender<AppEvent>,
) {
    let idx = state.active_tab;
    if let Some(tab) = state.tabs.get(idx)
        && tab.transactions.is_none()
        && !tab.loading
    {
        fetch_transactions(state, idx, client, tx);
    }
}

fn fetch_transactions(
    state: &mut AppState,
    tab_index: usize,
    client: &Arc<UpClient>,
    tx: &mpsc::UnboundedSender<AppEvent>,
) {
    if let Some(account) = state.accounts.get(tab_index) {
        if let Some(tab) = state.tabs.get_mut(tab_index) {
            tab.loading = true;
        }
        let account_id = account.id.clone();
        let client = Arc::clone(client);
        let tx = tx.clone();
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

pub fn fetch_categories(client: &Arc<UpClient>, tx: &mpsc::UnboundedSender<AppEvent>) {
    let client = Arc::clone(client);
    let tx = tx.clone();
    tokio::spawn(async move {
        let result = client.get_categories().await.map_err(Into::into);
        let _ = tx.send(AppEvent::CategoriesLoaded(result));
    });
}

pub fn fetch_accounts(client: &Arc<UpClient>, tx: &mpsc::UnboundedSender<AppEvent>) {
    let client = Arc::clone(client);
    let tx = tx.clone();
    tokio::spawn(async move {
        let result = client
            .get_accounts(PaginationOptions::default())
            .await
            .map(|page| page.data)
            .map_err(Into::into);
        let _ = tx.send(AppEvent::AccountsLoaded(result));
    });
}
