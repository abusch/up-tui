# up-tui

A terminal UI application for viewing Up Banking accounts and transactions.

## Overview

up-tui connects to the [Up Banking API](https://developer.up.com.au/) and presents accounts as tabs with their transactions in a navigable list, plus an optional detail pane. Built with ratatui for rendering, opaline for theming, and tokio + reqwest for async HTTP. The binary is named `up`.

## Configuration

The app reads its config from `$XDG_CONFIG_HOME/up-tui/config.toml` if `XDG_CONFIG_HOME` is set (and absolute), otherwise `~/.config/up-tui/config.toml` — on every platform, including macOS:

```toml
api_token = "up:yeah:xxxxxxxx"
theme = "tokyo-night"  # optional
```

- `dirs::config_dir()` is deliberately *not* used for the main path, because on macOS it is `~/Library/Application Support`. For backwards compatibility, if the file doesn't exist at the path above but does exist under `dirs::config_dir()/up-tui/config.toml`, that legacy file is used.
- `theme` is an opaline builtin theme id (kebab-case, e.g. `"dracula"`, `"catppuccin-mocha"`, `"nord"`). If omitted, opaline's default theme (`silkcircuit-neon`) is used.
- Config is validated before the TUI starts: the file must parse, `api_token` must be non-empty, and `theme` (if set) must load via `opaline::load_by_name`; otherwise the error lists the available themes. The token itself is not checked against the API until the first request.
- Missing or invalid config is returned as an error from `main()`, which prints it to stderr and exits before entering TUI mode.

## Architecture

### Runtime Setup

`main()` is `#[tokio::main(flavor = "current_thread")]`. It loads the config, builds the `App` (including the HTTP client), then calls `ratatui::init()`, runs `App::run()`, and calls `ratatui::restore()`. `ratatui::init()` also installs a panic hook that restores the terminal.

### Event Loop

All input and API results flow through a single bounded `tokio::sync::mpsc::channel::<AppEvent>(100)`:

- **Terminal events**: `spawn_event_reader` runs a crossterm `EventStream` on a tokio task. It forwards key presses (key *releases* are ignored, so keys aren't handled twice on platforms that report them) as `AppEvent::Key`, and terminal resizes as `AppEvent::Resize` (handled as a no-op; receiving any event triggers a redraw). If the input stream errors or ends, it sends `AppEvent::InputError`, and `run()` returns that error, so the app exits instead of hanging with no keyboard.
- **API results**: spawned tasks send `AppEvent::AccountsLoaded`, `AppEvent::TransactionsLoaded`, or `AppEvent::CategoriesLoaded` when HTTP requests complete.
- **Toast timers**: `AppEvent::ToastExpired(id)` (see Toasts below).

The main loop is: draw → recv event → handle event → repeat, until `state.should_quit`.

Event handling (`handle_event`, `handle_key`, …) is synchronous. **The main loop is the only consumer of the channel, so it must never `send(..).await` on it** — if the channel is full that send can never complete and the app deadlocks. Only spawned tasks send into the channel.

### State

`App` (`src/app/mod.rs`) owns the API client, the channel, the toast engine, and `AppState`.

`AppState` (`src/app/state.rs`) holds:

- `tabs` — one `TabState` per account, in API order
- `active_tab` — currently selected tab index
- `mode` — `AppMode::Normal`, or `AppMode::Theme(ThemeSelectorState)` while the theme picker is open
- `accounts_generation` — incremented for every accounts fetch; stale `AccountsLoaded` results are dropped
- `categories` — cached `HashMap<String, String>` mapping category IDs to display names, fetched once on startup via `GET /categories`
- `theme` — the current theme, an `Arc<opaline::Theme>` that is always opaline's global theme (`opaline::current()`); see Theming
- `tz` — the local `jiff` time zone, looked up once at startup; used for displaying times and grouping transactions by day (don't call `TimeZone::system()` elsewhere)
- `show_detail` — whether the detail pane is visible
- `page_size` — how many transactions fit in the list view (computed from the list block's inner area, 2 rows per item); written during rendering and used for `Ctrl+D`/`Ctrl+U` and for deciding when to fetch the next page

`TabState` holds:

- `account` — the `Account` this tab shows (name, balance)
- `transactions` — `None` until the first successful load
- `selected` — index into `transactions`. Invariant: `selected < transactions.len()` (or 0 when empty). Always replace the list via `set_transactions()`, which keeps the same transaction selected (by `id`) when it is still present and clamps otherwise.
- `loading` — `Option<LoadKind>`: `Replace` (initial load/refresh) or `Append` (next page)
- `generation` — incremented by `start_load()` for every fetch; results carrying an older generation were superseded and are dropped
- `next_page` — URL of the next page of transactions, if any
- `list_state` — ratatui `ListState`, used for scrolling

`set_accounts()` keeps the `TabState` of accounts that still exist (matched by account id, with the account data such as the balance updated) and keeps the active tab on the same account.

### Loading, Refresh and Pagination

- On startup, accounts and categories are fetched. When accounts arrive, the active tab's transactions are fetched.
- Transactions are fetched per tab on first view: switching to a tab with no loaded transactions (and nothing in flight) triggers a fetch. This also retries a tab whose previous load failed.
- `r` re-fetches the account list (refreshing balances, and recovering if the initial accounts load failed) and the current tab's first page of transactions. Refreshes bypass the HTTP cache. Refreshing replaces the list with the first page only.
- Transactions are fetched 50 per page. When navigation brings the selection within `page_size` items of the end of the loaded list and there is a `next_page`, the next page is fetched and appended.
- `TransactionsLoaded` identifies its tab by `account_id` (not tab index) plus `generation`.

### Toasts

Status messages (loading, success, errors) are shown as toasts in the top-right corner via `ratatui-toaster`. The toaster's built-in auto-hide is not used (no `action_tx`; the engine is `ToastEngine<()>`), because each of its timers hides whichever toast is current. Instead `App::toast()` shows the toast directly, assigns it an id, and spawns a 3-second timer that sends `ToastExpired(id)`; only the most recent toast's timer hides it. Toast positions are computed when a toast is shown, so `App::render_toasts()` (called on every render) re-shows the visible toast when the screen size changes. The toast engine is only used to lay toasts out; `App::render_toasts()` draws them with `ui::toast::draw_toast()`, because the toaster's own rendering has hardcoded colours. The toast engine is private to `App`; use `info()`/`success()`/`error()` to show toasts.

### Rendering

`impl Widget for &mut App` in `src/ui/mod.rs` draws the background, the tab bar, the transaction list (and the detail pane on the right, taking one third of the width, when `show_detail` is set), the status bar, toasts, and the theme picker overlay when in `AppMode::Theme`. Rendering takes `&mut` because it records `page_size`, updates each tab's `list_state`, and updates the toast area.

## Project Structure

```
src/
  main.rs              Entry point: load config, init terminal, run app, restore
  config.rs            Locate, load and validate config.toml

  client/
    mod.rs             UpClient: Bearer auth, HTTP cache middleware, account/transaction/category endpoints
    error.rs           Client Error enum (thiserror) and Result alias
    models.rs          JSON:API envelope types, relationship types, domain structs, money formatting

  app/
    mod.rs             App: main loop, event and key handling, fetch spawning, toasts
    state.rs           AppState, AppMode, TabState, LoadKind
    event.rs           AppEvent enum, crossterm event reader task

  ui/
    mod.rs             Top-level layout (Widget impl for App), theme picker overlay
    tabs.rs            Tab bar showing account names and balances
    transaction_list.rs  Transaction list grouped by day, description truncation
    transaction_detail.rs  Detail pane with full transaction info
    status_bar.rs      Keybinding hints
    toast.rs           Toast drawing in theme colours
```

## API Layer

- Base URL: `https://api.up.com.au/api/v1`
- Auth: `Authorization: Bearer {token}` set as a default header on the `reqwest::Client`
- The client is wrapped with `reqwest-middleware` and `http-cache-reqwest` (in-memory Moka cache, `CacheMode::Default`). Endpoint methods take a `reload` flag; when set, the request uses `CacheMode::Reload` (bypass the cache, update it with the response). It is used for user-initiated refreshes.
- `get_accounts` and `get_transactions` take `PaginationOptions` (page size 50, or a `next_url` from a previous response) and return `PaginatedResponse { data, next_page_url }`
- `Resource<A, R>` is generic over attributes (`A`) and relationships (`R`, defaults to `serde_json::Value`)
- Format money with the `MoneyObject` methods: `format_display()` for transaction amounts (`+$10.56` for credits, `$10.56` for debits, which are distinguished by colour), `format_signed()` for balances (`-$10.56`), and `format_with_code()` for foreign currencies (`-10.56 USD`). They take the sign from `value_in_base_units` and the digits from the API's `value` string; don't parse `value` as a float or assume two decimal places for foreign currencies
- Transaction relationships (category, parentCategory, tags) are deserialized into typed `TransactionRelationships` and flattened into the `Transaction` domain struct as IDs
- `GET /categories` is called once on startup to populate the category ID → display name cache in `AppState`. The detail pane resolves IDs to names via `state.category_name()`, falling back to the raw ID if the cache hasn't loaded yet
- Categories are hierarchical. The detail pane displays them on a single line as "Parent / Category" (e.g. "Good Life / Restaurants & Cafes")
- Client errors are `client::error::Error` (thiserror), carried as-is in `AppEvent` results and shown in toasts via their `Display`. `rootcause` is only used for top-level errors that end the program (`main`, `config`, `App::new`/`App::run`)

## Key Bindings

| Key                 | Mode         | Action                                      |
|---------------------|--------------|---------------------------------------------|
| `q` / `Q` / `Ctrl+C`| Normal       | Quit                                        |
| `Tab` / `l`         | Normal       | Next tab                                    |
| `Shift+Tab` / `h`   | Normal       | Previous tab                                |
| `j` / `↓`           | Normal       | Next transaction                            |
| `k` / `↑`           | Normal       | Previous transaction                        |
| `Ctrl+D` / `Ctrl+U` | Normal       | Page down / up                              |
| `g` / `G`           | Normal       | Jump to top / bottom of the loaded list     |
| `[` / `]`           | Normal       | Jump to the start of the day group above / below (the list is newest first) |
| `o`                 | Normal       | Toggle the detail pane                      |
| `r`                 | Normal       | Refresh accounts and the current tab        |
| `t`                 | Normal       | Open the theme picker                       |
| `↑` / `↓`           | Theme picker | Move selection                              |
| typing / `Backspace`| Theme picker | Filter themes                               |
| `Enter`             | Theme picker | Apply the selected theme                    |
| `Esc`               | Theme picker | Close without changing the theme            |

Key handling lives in `App::handle_normal_key` and `App::handle_key` (which delegates to `ThemeSelectorState::handle_key` in theme mode). Selection movement is implemented as `TabState` methods (`select_next`, `page_down`, `select_next_day`, …), which keep `selected` in bounds and are unit-tested; the key handler applies them to the active tab via `App::navigate`. After every Normal-mode key, `maybe_fetch_next_page()` checks whether to load more transactions. The status bar hints in `src/ui/status_bar.rs` should be kept in sync with the bindings.

## Theming

Powered by the `opaline` crate (with its `widgets` feature, for the `ThemeSelector` picker). The initial theme comes from the config `theme` field (an opaline builtin id), defaulting to opaline's default theme. `t` opens the picker to choose any builtin theme at runtime; the choice is not saved to the config file.

The picker (`ThemeSelector`) draws itself with opaline's *global* theme, and previews the highlighted theme by changing it (restoring the original on `Esc`). So `AppState::new` makes the configured theme the global one, and after every key in theme mode `state.theme` is set back to `opaline::current()`: the picker and the rest of the UI always use the same theme, and the whole UI previews the highlighted theme. Don't set `state.theme` without also calling `opaline::set_theme`.

All UI modules read colors from `state.theme` via semantic tokens (`opaline::names::tokens`) — no hardcoded colors, and nothing drawn by a third-party widget with its own colours. Use `theme.color(token)`; `theme.line()`/`span()`/`style()` take opaline *style* names (`opaline::names::styles`), not tokens, and silently return an unstyled result for an unknown name. Tokens in use:

- `BG_BASE` / `TEXT_PRIMARY` — base background and foreground
- `BG_PANEL` — transaction list background; `BG_CODE` — detail pane background
- `BORDER_UNFOCUSED` — the detail pane's left border, which separates it from the list (many themes use the same colour for `BG_PANEL` and `BG_CODE`)
- `BG_ELEVATED` — toast and theme picker backgrounds
- `BG_SELECTION` — selected row background
- `ACCENT_PRIMARY` — active tab, list and detail pane titles, selection marker
- `TEXT_MUTED` — inactive tabs; `TEXT_DIM` — day separators, times, status bar text
- `TEXT_SECONDARY` — field labels in the detail pane
- Amounts: `TEXT_PRIMARY` for debits, `SUCCESS` for credits
- `SUCCESS` / `WARNING` — settled / pending (held) status
- `INFO` — keybinding hints in the status bar
- `INFO` / `SUCCESS` / `WARNING` / `ERROR` — toast borders, by toast type

## Dependencies

When adding or updating dependencies in `Cargo.toml`, always specify the full version including the patch number (e.g. `"1.0.102"` not `"1"` or `"1.0"`). This applies to both workspace and package-level dependencies.

## Code Quality

All code must pass `cargo clippy --workspace` with zero warnings before being considered complete. Always run clippy after making changes and fix any warnings it reports.

Run `cargo fmt --all` after making changes to ensure consistent formatting across the workspace.

Run `cargo test` after making changes. Unit tests live alongside the code in `#[cfg(test)]` modules (e.g. `src/app/state.rs`, `src/ui/transaction_list.rs`). `app::state::tests` has helpers for building test fixtures. CI runs `cargo test --workspace` and `cargo clippy --workspace -- -D warnings`.

## Error Handling

- **Config errors**: Returned from `main()` and printed to stderr before entering TUI mode.
- **API/network errors**: Shown as error toasts. The app remains usable with stale data; `r` retries.
- **Terminal input errors**: `run()` returns the error, the terminal is restored, and the app exits.
- **Panics**: The panic hook installed by `ratatui::init()` restores the terminal.
