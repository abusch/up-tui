use std::sync::Arc;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use tokio::sync::mpsc;

use crate::api::client::UpClient;
use crate::app::event::AppEvent;
use crate::app::state::{AppMode, AppState};

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
        AppEvent::TransactionsLoaded { tab_index, result } => {
            if let Some(tab) = state.tabs.get_mut(tab_index) {
                tab.loading = false;
                match result {
                    Ok(transactions) => {
                        tab.transactions = Some(transactions);
                    }
                    Err(e) => {
                        state.set_status(
                            format!("Failed to load transactions: {}", e),
                            true,
                        );
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
    match state.mode {
        AppMode::Normal => handle_normal_key(state, key, client, tx),
        AppMode::Detail => handle_detail_key(state, key),
    }
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
            if let Some(tab) = state.current_tab_mut() {
                if let Some(ref txns) = tab.transactions {
                    if !txns.is_empty() && tab.selected < txns.len() - 1 {
                        tab.selected += 1;
                    }
                }
            }
        }
        KeyCode::Char('k') | KeyCode::Up => {
            if let Some(tab) = state.current_tab_mut() {
                if tab.selected > 0 {
                    tab.selected -= 1;
                }
            }
        }
        KeyCode::Char('g') => {
            if let Some(tab) = state.current_tab_mut() {
                tab.selected = 0;
            }
        }
        KeyCode::Char('G') => {
            if let Some(tab) = state.current_tab_mut() {
                if let Some(ref txns) = tab.transactions {
                    if !txns.is_empty() {
                        tab.selected = txns.len() - 1;
                    }
                }
            }
        }
        KeyCode::Enter => {
            if let Some(tab) = state.current_tab() {
                if let Some(ref txns) = tab.transactions {
                    if !txns.is_empty() {
                        state.mode = AppMode::Detail;
                    }
                }
            }
        }
        KeyCode::Char('r') => {
            let idx = state.active_tab;
            fetch_transactions(state, idx, client, tx);
        }
        _ => {}
    }
}

fn handle_detail_key(state: &mut AppState, key: KeyEvent) {
    match key.code {
        KeyCode::Esc | KeyCode::Char('q') => {
            state.mode = AppMode::Normal;
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
    if let Some(tab) = state.tabs.get(idx) {
        if tab.transactions.is_none() && !tab.loading {
            fetch_transactions(state, idx, client, tx);
        }
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
            let result = client.get_transactions(&account_id).await;
            let _ = tx.send(AppEvent::TransactionsLoaded {
                tab_index,
                result,
            });
        });
    }
}

pub fn fetch_accounts(client: &Arc<UpClient>, tx: &mpsc::UnboundedSender<AppEvent>) {
    let client = Arc::clone(client);
    let tx = tx.clone();
    tokio::spawn(async move {
        let result = client.get_accounts().await;
        let _ = tx.send(AppEvent::AccountsLoaded(result));
    });
}
