use crossterm::event::{Event, EventStream, KeyEvent};
use futures::StreamExt;
use ratatui_toaster::{ToastBuilder, ToastMessage};
use rootcause::Result;
use tokio::sync::mpsc;

use crate::client::models::{Account, Transaction};

pub enum AppEvent {
    Key(KeyEvent),
    /// The terminal was resized; the main loop redraws on any event.
    Resize,
    /// Reading terminal input failed (or the input stream ended). Fatal.
    InputError(std::io::Error),
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
        loop {
            let app_event = match events.next().await {
                Some(Ok(Event::Key(key))) => AppEvent::Key(key),
                Some(Ok(Event::Resize(..))) => AppEvent::Resize,
                Some(Ok(_)) => continue,
                Some(Err(e)) => AppEvent::InputError(e),
                None => AppEvent::InputError(std::io::Error::other("terminal input stream closed")),
            };
            let fatal = matches!(app_event, AppEvent::InputError(_));
            if tx.send(app_event).await.is_err() || fatal {
                break;
            }
        }
    });
}
