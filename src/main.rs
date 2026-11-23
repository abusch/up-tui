mod app;
mod backend;
mod client;
mod config;
mod ui;

use ratatui::restore;
use rootcause::{Result, prelude::*};

use crate::app::App;
use crate::client::{Client, UpClient, demo::DemoBank};

const USAGE: &str = "\
Usage: up [--demo]

Options:
  --demo      Show generated demo accounts and transactions instead of yours
              (no API token needed)
  -h, --help  Show this help";

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let mut demo = false;
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "--demo" => demo = true,
            "-h" | "--help" => {
                println!("{USAGE}");
                return Ok(());
            }
            other => {
                eprintln!("Unknown argument: {other}\n\n{USAGE}");
                std::process::exit(2);
            }
        }
    }

    // Load config before entering TUI mode
    let app = if demo {
        // The demo needs no config, but uses the configured theme if there is one.
        let theme = config::load_theme().context("Failed to load configuration")?;
        let bank = DemoBank::new(&jiff::ZonedDateTime::now());
        App::new(Client::Demo(bank), theme.as_deref())
    } else {
        let cfg = config::load_config().context("Failed to load configuration")?;
        let client = UpClient::new(&cfg.api_token)?;
        App::new(Client::Up(client), cfg.theme.as_deref())
    };

    let terminal = backend::init().context("Failed to initialize the terminal")?;
    let result = app.run(terminal).await;
    restore();

    result
}
