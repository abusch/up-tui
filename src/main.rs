mod app;
mod client;
mod config;
mod ui;

use std::sync::Arc;

use anyhow::Result;
use ratatui::{DefaultTerminal, init, restore};
use tokio::sync::mpsc;

use client::UpClient;

use app::event::{AppEvent, spawn_event_reader};
use app::handler::{fetch_accounts, fetch_categories, handle_event};
use app::state::AppState;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    // Load config before entering TUI mode
    let cfg = match config::load_config() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error: {:#}", e);
            std::process::exit(1);
        }
    };

    let client = Arc::new(UpClient::new(&cfg.api_token)?);
    let state = AppState::new(cfg);

    let mut terminal = init();
    let result = run_app(&mut terminal, client, state).await;
    restore();

    result
}

async fn run_app(
    terminal: &mut DefaultTerminal,
    client: Arc<UpClient>,
    mut state: AppState,
) -> Result<()> {
    state.set_status("Loading accounts...".into(), false);

    let (tx, mut rx) = mpsc::unbounded_channel::<AppEvent>();

    // Spawn crossterm event reader
    spawn_event_reader(tx.clone());

    // Fetch accounts and categories on startup
    fetch_accounts(&client, &tx);
    fetch_categories(&client, &tx);

    loop {
        terminal.draw(|f| ui::draw(f, &mut state))?;

        if let Some(event) = rx.recv().await {
            handle_event(&mut state, event, &client, &tx);
        }

        if state.should_quit {
            break;
        }
    }

    Ok(())
}
