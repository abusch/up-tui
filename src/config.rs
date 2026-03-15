use anyhow::{Context, Result};
use serde::Deserialize;
use std::path::PathBuf;

#[derive(Deserialize)]
pub struct Config {
    pub api_token: String,
}

pub fn config_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("~/.config"))
        .join("up-tui")
        .join("config.toml")
}

pub fn load_config() -> Result<Config> {
    let path = config_path();
    let contents = std::fs::read_to_string(&path)
        .with_context(|| format!("Failed to read config file: {}\n\nCreate it with:\n  mkdir -p ~/.config/up-tui\n  echo 'api_token = \"up:yeah:xxxxxxxx\"' > ~/.config/up-tui/config.toml", path.display()))?;
    let config: Config = toml::from_str(&contents)
        .with_context(|| format!("Invalid config file: {}\n\nExpected format:\n  api_token = \"up:yeah:xxxxxxxx\"", path.display()))?;
    if config.api_token.is_empty() {
        anyhow::bail!("api_token is empty in {}", path.display());
    }
    Ok(config)
}
