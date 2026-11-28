use crossterm::event::{Event, EventStream, KeyEvent, KeyEventKind};
use futures::StreamExt;
use tokio::sync::mpsc;

use rootcause::Result;

use crate::app::store::FetchId;
use crate::client::models::{Account, Category, PaginatedResponse, Transaction};

pub enum AppEvent {
    Key(KeyEvent),
    /// The terminal was resized; the main loop redraws on any event.
    Resize,
    /// Reading terminal input failed (or the input stream ended). Fatal.
    InputError(std::io::Error),
    AccountsLoaded {
        /// Matches `AppState::accounts_generation` unless superseded by a later fetch.
        generation: u64,
        result: Result<Vec<Account>>,
    },
    TransactionsLoaded {
        /// Identifies the tab by account, since tab indices can shift when accounts reload.
        account_id: String,
        /// The `Fetch::id` of the fetch; the store drops results it no longer wants.
        fetch: FetchId,
        result: Result<PaginatedResponse<Transaction>>,
    },
    CategoriesLoaded(Result<Vec<Category>>),
    /// The display time for the header bar message with this id has run out.
    MessageExpired(u64),
}

pub fn spawn_event_reader(tx: mpsc::Sender<AppEvent>) {
    tokio::task::spawn(async move {
        let mut events = EventStream::new();
        loop {
            let app_event = match events.next().await {
                // Ignore key releases, which some platforms (e.g. Windows) report in
                // addition to presses; otherwise every key would be handled twice.
                Some(Ok(Event::Key(key))) if key.kind != KeyEventKind::Release => {
                    AppEvent::Key(key)
                }
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
