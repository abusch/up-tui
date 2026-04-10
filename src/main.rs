mod app;
mod client;
mod config;
mod ui;

use anyhow::Result;
use ratatui::{init, restore};

use crate::app::App;

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

    let app = App::new(cfg)?;

    let terminal = init();
    let result = app.run(terminal).await;
    restore();

    result
}
