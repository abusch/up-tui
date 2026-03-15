use anyhow::Result;
use crossterm::event::{Event, EventStream, KeyEvent};
use futures::StreamExt;
use tokio::sync::mpsc;

use up_api::models::{Account, Transaction};

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
    tokio::task::spawn(async move {
        let mut events = EventStream::new();
        while let Some(Ok(event)) = events.next().await {
            if let Event::Key(key) = event {
                let _ = tx.send(AppEvent::Key(key));
            }
        }
    });
}
