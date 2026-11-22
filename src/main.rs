mod app;
mod backend;
mod client;
mod config;
mod ui;

use ratatui::restore;
use rootcause::{Result, prelude::*};

use crate::app::App;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    // Load config before entering TUI mode
    let cfg = config::load_config().context("Failed to load configuration")?;

    let app = App::new(cfg)?;

    let terminal = backend::init().context("Failed to initialize the terminal")?;
    let result = app.run(terminal).await;
    restore();

    result
}
