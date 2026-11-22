use rootcause::{Result, bail, prelude::*};
use serde::Deserialize;
use std::path::PathBuf;

#[derive(Deserialize)]
pub struct Config {
    pub api_token: String,
    pub theme: Option<String>,
}

/// The config file location: `$XDG_CONFIG_HOME/up-tui/config.toml` if set, otherwise
/// `~/.config/up-tui/config.toml`, on every platform. (`dirs::config_dir()` is not used
/// because it returns `~/Library/Application Support` on macOS, contradicting the docs.)
pub fn config_path() -> Result<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| dirs::home_dir().map(|home| home.join(".config")));
    let Some(base) = base else {
        bail!(
            "Could not determine the config directory: neither $XDG_CONFIG_HOME nor the home directory is set"
        );
    };
    let path = base.join("up-tui").join("config.toml");

    // Backwards compatibility: earlier versions read from the platform config dir
    // (`~/Library/Application Support` on macOS). Keep using it if that's where the
    // file is and the new location doesn't exist.
    if !path.exists()
        && let Some(legacy) = dirs::config_dir().map(|d| d.join("up-tui").join("config.toml"))
        && legacy.exists()
    {
        return Ok(legacy);
    }

    Ok(path)
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
