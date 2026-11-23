use etcetera::{BaseStrategy, base_strategy::Xdg};
use rootcause::{Result, bail, prelude::*};
use serde::Deserialize;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

#[derive(Deserialize)]
pub struct Config {
    /// Required, except in demo mode; empty if it isn't set.
    #[serde(default)]
    pub api_token: String,
    pub theme: Option<String>,
}

/// The config file location: `$XDG_CONFIG_HOME/up-tui/config.toml` if set (and absolute),
/// otherwise `~/.config/up-tui/config.toml`, on every platform. The XDG strategy is used
/// directly rather than `choose_base_strategy()`, which would pick `AppData` on Windows.
pub fn config_path() -> Result<PathBuf> {
    let xdg = Xdg::new().context("Could not determine the config directory")?;
    Ok(xdg.config_dir().join("up-tui").join("config.toml"))
}

/// Load the config file, which must exist and set an API token.
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
    let config = parse(&contents, &path)?;
    if config.api_token.is_empty() {
        bail!("api_token is missing or empty in {}", path.display());
    }
    Ok(config)
}

/// The theme set in the config file, for demo mode, which needs no API token: `None`
/// if there is no config file, or it doesn't set one. A config file that can't be read
/// or is invalid is still an error, rather than its theme being quietly ignored.
pub fn load_theme() -> Result<Option<String>> {
    let path = config_path()?;
    let contents = match std::fs::read_to_string(&path) {
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(None),
        result => {
            result.context_with(|| format!("Failed to read config file: {}", path.display()))?
        }
    };
    Ok(parse(&contents, &path)?.theme)
}

/// Parse the config file's `contents`, read from `path`, and check its theme.
fn parse(contents: &str, path: &Path) -> Result<Config> {
    let config: Config = toml::from_str(contents).context_with(|| {
        format!(
            "Invalid config file: {}\n\nExpected format:\n  api_token = \"up:yeah:xxxxxxxx\"",
            path.display()
        )
    })?;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parsing_checks_the_theme_but_not_the_api_token() {
        let path = Path::new("config.toml");
        // Demo mode needs only a theme.
        let config = parse("theme = \"nord\"", path).unwrap();
        assert_eq!(config.api_token, "");
        assert_eq!(config.theme.as_deref(), Some("nord"));
        assert!(parse("theme = \"no-such-theme\"", path).is_err());
        assert!(parse("api_token = 1", path).is_err());
    }
}
