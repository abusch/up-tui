use anyhow::Result;
use crossterm::event::{self, Event, KeyEvent};
use tokio::sync::mpsc;

use crate::api::models::{Account, Transaction};

pub enum AppEvent {
    Key(KeyEvent),
    AccountsLoaded(Result<Vec<Account>>),
    TransactionsLoaded {
        tab_index: usize,
        result: Result<Vec<Transaction>>,
    },
    CategoriesLoaded(Result<Vec<(String, String)>>),
}

pub fn spawn_event_reader(tx: mpsc::UnboundedSender<AppEvent>) {
    tokio::task::spawn_blocking(move || {
        loop {
            if let Ok(Event::Key(key)) = event::read()
                && tx.send(AppEvent::Key(key)).is_err()
            {
                break;
            }
        }
    });
}
