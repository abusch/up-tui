mod api;
mod app;
mod config;
mod ui;

use std::io;
use std::panic;
use std::sync::Arc;

use anyhow::Result;
use crossterm::event::{DisableMouseCapture, EnableMouseCapture};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use tokio::sync::mpsc;

use api::client::UpClient;
use app::event::{spawn_event_reader, AppEvent};
use app::handler::{fetch_accounts, handle_event};
use app::state::AppState;

#[tokio::main]
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

    // Set up panic hook to restore terminal
    let default_hook = panic::take_hook();
    panic::set_hook(Box::new(move |info| {
        let _ = restore_terminal();
        default_hook(info);
    }));

    // Set up terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = run_app(&mut terminal, client).await;

    // Restore terminal
    restore_terminal()?;

    if let Err(e) = result {
        eprintln!("Error: {:#}", e);
        std::process::exit(1);
    }

    Ok(())
}

async fn run_app(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    client: Arc<UpClient>,
) -> Result<()> {
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

fn restore_terminal() -> Result<()> {
    disable_raw_mode()?;
    execute!(
        io::stdout(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    Ok(())
}
