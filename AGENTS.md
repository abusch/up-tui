# up-tui

A terminal UI application for viewing Up Banking accounts and transactions.

## Overview

up-tui connects to the [Up Banking API](https://developer.up.com.au/) and presents accounts as tabs with their transactions in a navigable list. Built with ratatui for rendering and tokio + reqwest for async HTTP.

## Configuration

The app reads its API token from `~/.config/up-tui/config.toml`:

```toml
api_token = "up:yeah:xxxxxxxx"
theme = "tokyo-night"  # optional, defaults to tokyo-night
```

The `theme` field accepts any slug from `ThemeName::slug()` (e.g. `"dracula"`, `"catppuccin-mocha"`, `"nord"`). Config is validated before the TUI starts. Missing or invalid config prints an error to stderr and exits.

## Architecture

### Runtime Setup

`main()` is synchronous. It creates a tokio `Runtime` manually, then calls `ratatui::run()` which handles terminal setup, restore, and panic recovery. The async app loop runs inside `ratatui::run()` via `rt.block_on()`. This avoids the "runtime within runtime" problem that occurs when combining `#[tokio::main]` with `ratatui::run()`.

### Event Loop

All input and API results flow through a single `tokio::sync::mpsc::unbounded_channel` of `AppEvent`:

- **Key events**: A blocking crossterm reader runs on `tokio::task::spawn_blocking` and sends `AppEvent::Key` into the channel.
- **API results**: Async tasks send `AppEvent::AccountsLoaded`, `AppEvent::TransactionsLoaded`, or `AppEvent::CategoriesLoaded` when HTTP requests complete.

The main loop is: draw → recv event → handle event → repeat.

### State

`AppState` holds all application state:

- `accounts` — list of accounts from the API
- `tabs` — one `TabState` per account, each tracking its transaction list, selected index, and loading flag
- `active_tab` — currently selected tab index
- `mode` — `Normal` (browsing) or `Detail` (viewing transaction overlay)
- `categories` — cached `HashMap<String, String>` mapping category IDs to display names, fetched once on startup via `GET /categories`
- `theme` — current `Theme` from ratatui-themes, provides the active `ThemePalette` via `state.palette()`
- `status_message` / `status_is_error` — status bar content

### Lazy Loading

Transactions are fetched per-tab on first view. Switching to a tab that has no loaded transactions triggers an async fetch. The `r` key forces a re-fetch of the current tab.

## Project Structure

```
src/
  main.rs              Entry point, runtime setup, main loop
  config.rs            Load and validate ~/.config/up-tui/config.toml

  client/
    mod.rs             UpClient with Bearer auth, account/transaction/category endpoints
    error.rs           Error enum and Result type alias
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
- Transaction relationships (category, parentCategory, tags) are deserialized into typed `TransactionRelationships` and flattened into the `Transaction` domain struct as IDs
- `GET /categories` is called once on startup to populate a category ID → display name cache in `AppState`. The detail view resolves IDs to names via `state.category_name()`, falling back to the raw ID if the cache hasn't loaded yet
- Categories are hierarchical. The detail view displays them on a single line as "Parent / Category" (e.g. "Good Life / Restaurants & Cafes")

## Key Bindings

| Key              | Mode   | Action                    |
|------------------|--------|---------------------------|
| `q` / `Ctrl+C`  | Normal | Quit                      |
| `Tab` / `l`     | Normal | Next tab                  |
| `Shift+Tab` / `h` | Normal | Previous tab            |
| `j` / `↓`       | Normal | Next transaction          |
| `k` / `↑`       | Normal | Previous transaction      |
| `[` / `]`       | Normal | Previous / next day       |
| `g` / `G`       | Normal | Jump to top / bottom      |
| `Enter`          | Normal | Open detail overlay       |
| `r`              | Normal | Refresh current tab       |
| `t` / `T`       | Normal | Next / previous theme     |
| `Esc` / `q`     | Detail | Close overlay             |

## Theming

Powered by the `ratatui-themes` crate. The initial theme is read from the config file (`theme` field, slug format); defaults to Tokyo Night. Users can cycle through all 15 built-in themes at runtime with `t`/`T`.

All UI modules read `state.palette()` for colors — no hardcoded colors remain. The palette colors are mapped as follows:

- `palette.success` / `palette.error` — positive/negative amounts, status bar success/error messages
- `palette.accent` — active tab highlight, table header, keybinding hints in status bar
- `palette.secondary` — field labels in the detail overlay
- `palette.muted` — inactive tabs, status column, hint text
- `palette.selection` — selected row background
- `palette.bg` / `palette.fg` — base background and foreground for all blocks and text

## Dependencies

When adding or updating dependencies in `Cargo.toml`, always specify the full version including the patch number (e.g. `"1.0.102"` not `"1"` or `"1.0"`). This applies to both workspace and package-level dependencies.

## Code Quality

All code must pass `cargo clippy --workspace` with zero warnings before being considered complete. Always run clippy after making changes and fix any warnings it reports.

Run `cargo fmt --all` after making changes to ensure consistent formatting across the workspace.

## Error Handling

- **Config errors**: Print to stderr and exit before entering TUI mode.
- **API/network errors**: Display in the status bar (using `palette.error`). The app remains usable with stale data.
- **Terminal/panic errors**: Handled by `ratatui::run()` which restores the terminal automatically.
