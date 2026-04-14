use anyhow::Result;
use crossterm::event::{Event, EventStream, KeyEvent};
use futures::StreamExt;
use ratatui_toaster::{ToastBuilder, ToastMessage};
use tokio::sync::mpsc;

use crate::client::models::{Account, Transaction};

pub enum AppEvent {
    Key(KeyEvent),
    AccountsLoaded(Result<Vec<Account>>),
    TransactionsLoaded {
        tab_index: usize,
        result: Result<Vec<Transaction>>,
    },
    CategoriesLoaded(Result<Vec<(String, String)>>),
    ToastShow(ToastBuilder),
    ToastHide,
}

impl From<ToastMessage> for AppEvent {
    fn from(value: ToastMessage) -> Self {
        match value {
            ToastMessage::Show {
                message,
                toast_type,
                position,
            } => Self::ToastShow(
                ToastBuilder::new(message.into())
                    .toast_type(toast_type)
                    .position(position)
                    .constraint(ratatui_toaster::ToastConstraint::Auto),
            ),
            ToastMessage::Hide => Self::ToastHide,
        }
    }
}

pub fn spawn_event_reader(tx: mpsc::Sender<AppEvent>) {
    tokio::task::spawn(async move {
        let mut events = EventStream::new();
        while let Some(Ok(event)) = events.next().await {
            if let Event::Key(key) = event {
                let _ = tx.send(AppEvent::Key(key)).await;
            }
        }
    });
}
