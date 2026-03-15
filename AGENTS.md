# up-tui

A terminal UI application for viewing Up Banking accounts and transactions.

## Overview

up-tui connects to the [Up Banking API](https://developer.up.com.au/) and presents accounts as tabs with their transactions in a navigable list. Built with ratatui for rendering and tokio + reqwest for async HTTP.

## Configuration

The app reads its API token from `~/.config/up-tui/config.toml`:

```toml
api_token = "up:yeah:xxxxxxxx"
```

Config is validated before the TUI starts. Missing or invalid config prints an error to stderr and exits.

## Architecture

### Runtime Setup

`main()` is synchronous. It creates a tokio `Runtime` manually, then calls `ratatui::run()` which handles terminal setup, restore, and panic recovery. The async app loop runs inside `ratatui::run()` via `rt.block_on()`. This avoids the "runtime within runtime" problem that occurs when combining `#[tokio::main]` with `ratatui::run()`.

### Event Loop

All input and API results flow through a single `tokio::sync::mpsc::unbounded_channel` of `AppEvent`:

- **Key events**: A blocking crossterm reader runs on `tokio::task::spawn_blocking` and sends `AppEvent::Key` into the channel.
- **API results**: Async tasks send `AppEvent::AccountsLoaded` or `AppEvent::TransactionsLoaded` when HTTP requests complete.

The main loop is: draw → recv event → handle event → repeat.

### State

`AppState` holds all application state:

- `accounts` — list of accounts from the API
- `tabs` — one `TabState` per account, each tracking its transaction list, selected index, and loading flag
- `active_tab` — currently selected tab index
- `mode` — `Normal` (browsing) or `Detail` (viewing transaction overlay)
- `status_message` / `status_is_error` — status bar content

### Lazy Loading

Transactions are fetched per-tab on first view. Switching to a tab that has no loaded transactions triggers an async fetch. The `r` key forces a re-fetch of the current tab.

## Project Structure

```
src/
  main.rs              Entry point, runtime setup, main loop
  config.rs            Load and validate ~/.config/up-tui/config.toml

  api/
    mod.rs             Re-exports
    client.rs          UpClient with Bearer auth, account/transaction endpoints
    models.rs          JSON:API envelope types, relationship types, domain structs

  app/
    mod.rs             Re-exports
    state.rs           AppState, AppMode, TabState
    event.rs           AppEvent enum, crossterm key reader
    handler.rs         Key dispatch, state mutations, async API call spawning

  ui/
    mod.rs             Top-level draw() with layout splits
    tabs.rs            Tab bar showing account names and balances
    transaction_list.rs  Table widget with date, description, amount, status
    transaction_detail.rs  Centered overlay with full transaction info
    status_bar.rs      Keybinding hints and error/loading messages
```

## API Layer

- Base URL: `https://api.up.com.au/api/v1`
- Auth: `Authorization: Bearer {token}` set as a default header on reqwest::Client
- `Resource<A, R>` is generic over attributes (`A`) and relationships (`R`, defaults to `serde_json::Value`)
- Transaction relationships (category, parentCategory, tags) are deserialized into typed `TransactionRelationships` and flattened into the `Transaction` domain struct

## Key Bindings

| Key              | Mode   | Action                    |
|------------------|--------|---------------------------|
| `q` / `Ctrl+C`  | Normal | Quit                      |
| `Tab` / `l`     | Normal | Next tab                  |
| `Shift+Tab` / `h` | Normal | Previous tab            |
| `j` / `↓`       | Normal | Next transaction          |
| `k` / `↑`       | Normal | Previous transaction      |
| `g` / `G`       | Normal | Jump to top / bottom      |
| `Enter`          | Normal | Open detail overlay       |
| `r`              | Normal | Refresh current tab       |
| `Esc` / `q`     | Detail | Close overlay             |

## Error Handling

- **Config errors**: Print to stderr and exit before entering TUI mode.
- **API/network errors**: Display in the status bar (red). The app remains usable with stale data.
- **Terminal/panic errors**: Handled by `ratatui::run()` which restores the terminal automatically.
