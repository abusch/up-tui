mod app;
mod client;
mod config;
mod ui;

use anyhow::{Context, Result};
use ratatui::{init, restore};

use crate::app::App;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    // Load config before entering TUI mode
    let cfg = config::load_config().context("Failed to load configuration")?;

    let app = App::new(cfg)?;

    let terminal = init();
    let result = app.run(terminal).await;
    restore();

    result
}
