use etcetera::{BaseStrategy, base_strategy::Xdg};
use rootcause::{Result, bail, prelude::*};
use serde::Deserialize;
use std::path::PathBuf;

#[derive(Deserialize)]
pub struct Config {
    pub api_token: String,
    pub theme: Option<String>,
}

/// The config file location: `$XDG_CONFIG_HOME/up-tui/config.toml` if set (and absolute),
/// otherwise `~/.config/up-tui/config.toml`, on every platform. The XDG strategy is used
/// directly rather than `choose_base_strategy()`, which would pick `AppData` on Windows.
pub fn config_path() -> Result<PathBuf> {
    let Ok(xdg) = Xdg::new() else {
        bail!("Could not determine the config directory: the home directory is not set");
    };
    Ok(xdg.config_dir().join("up-tui").join("config.toml"))
}

pub fn load_config() -> Result<Config> {
    let path = config_path()?;
    let contents = std::fs::read_to_string(&path).context_with(|| {
        let dir = path.parent().unwrap_or(&path).display();
        format!(
            "Failed to read config file: {}\n\nCreate it with:\n  mkdir -p '{dir}'\n  echo 'api_token = \"up:yeah:xxxxxxxx\"' > '{}'",
            path.display(),
            path.display()
        )
    })?;
    let config: Config = toml::from_str(&contents).context_with(|| {
        format!(
            "Invalid config file: {}\n\nExpected format:\n  api_token = \"up:yeah:xxxxxxxx\"",
            path.display()
        )
    })?;
    if config.api_token.is_empty() {
        bail!("api_token is empty in {}", path.display());
    }
    if let Some(theme) = &config.theme
        && opaline::load_by_name(theme).is_none()
    {
        let available: Vec<String> = opaline::list_available_themes()
            .into_iter()
            .map(|t| t.name)
            .collect();
        bail!(
            "Unknown theme \"{theme}\" in {}\n\nAvailable themes: {}",
            path.display(),
            available.join(", ")
        );
    }
    Ok(config)
}
