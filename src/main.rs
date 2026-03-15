mod api;
mod app;
mod config;
mod ui;

use std::sync::Arc;

use anyhow::Result;
use ratatui::DefaultTerminal;
use tokio::sync::mpsc;

use api::client::UpClient;
use app::event::{spawn_event_reader, AppEvent};
use app::handler::{fetch_accounts, handle_event};
use app::state::AppState;

fn main() -> Result<()> {
    // Load config before entering TUI mode
    let cfg = match config::load_config() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error: {:#}", e);
            std::process::exit(1);
        }
    };

    let client = Arc::new(UpClient::new(&cfg.api_token)?);

    let rt = tokio::runtime::Runtime::new()?;
    ratatui::run(|terminal| rt.block_on(run_app(terminal, client)))?;

    Ok(())
}

async fn run_app(terminal: &mut DefaultTerminal, client: Arc<UpClient>) -> Result<()> {
    let mut state = AppState::new();
    state.set_status("Loading accounts...".into(), false);

    let (tx, mut rx) = mpsc::unbounded_channel::<AppEvent>();

    // Spawn crossterm event reader
    spawn_event_reader(tx.clone());

    // Fetch accounts on startup
    fetch_accounts(&client, &tx);

    loop {
        terminal.draw(|f| ui::draw(f, &state))?;

        if let Some(event) = rx.recv().await {
            handle_event(&mut state, event, &client, &tx);
        }

        if state.should_quit {
            break;
        }
    }

    Ok(())
}
