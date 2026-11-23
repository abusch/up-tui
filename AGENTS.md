# up-tui

A terminal UI application for viewing Up Banking accounts and transactions.

## Overview

up-tui connects to the [Up Banking API](https://developer.up.com.au/) and presents accounts in a sidebar, with the selected account's transactions in a navigable list, plus an optional detail pane. Built with ratatui for rendering, opaline for theming, and tokio + reqwest for async HTTP. The binary is named `up`.

## Command Line

`up` takes no arguments, except `--demo` (see Demo Mode) and `-h`/`--help`. They're parsed by hand in `main()`, without a library; an unknown argument prints the usage to stderr and exits with status 2.

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

`main()` is `#[tokio::main(flavor = "current_thread")]`. It parses the arguments, loads the config, builds the `App` with its data source (a `Client`), then calls `backend::init()`, runs `App::run()`, and calls `ratatui::restore()`. `backend::init()` sets up the terminal with `ratatui::init()` (raw mode, alternate screen, and a panic hook that restores the terminal), but returns an `AppTerminal`, whose crossterm backend is wrapped in `WideCharBackend`.

`WideCharBackend` (`src/backend.rs`) works around a ratatui bug. When an emoji containing U+FE0F (e.g. "🛋️") is drawn over non-blank cells, ratatui's diff also emits the cell under the emoji's second column, and the crossterm backend writes it right after the emoji without moving the cursor. Terminals that draw the emoji two columns wide have already moved past that column, so it and the rest of the run of cells land one column to the right. The wrapper drops updates to cells covered by the wide character just drawn, so the backend moves the cursor before the next cell. Keep using it (and `AppTerminal`) rather than `ratatui::DefaultTerminal` until it's no longer needed: this is ratatui issue #2651, fixed by ratatui PR #2721, which isn't in a release as of ratatui-crossterm 0.1.2. Once a ratatui-crossterm release includes that fix, upgrade, delete `src/backend.rs`, and go back to `ratatui::init()` and `DefaultTerminal`.

When measuring or truncating text, measure whole graphemes (as `truncate_to_width()` does), like ratatui, not single `char`s: an emoji with U+FE0F is 2 columns wide although its characters' widths add up to 1.

### Event Loop

All input and API results flow through a single bounded `tokio::sync::mpsc::channel::<AppEvent>(100)`:

- **Terminal events**: `spawn_event_reader` runs a crossterm `EventStream` on a tokio task. It forwards key presses (key *releases* are ignored, so keys aren't handled twice on platforms that report them) as `AppEvent::Key`, and terminal resizes as `AppEvent::Resize` (handled as a no-op; receiving any event triggers a redraw). If the input stream errors or ends, it sends `AppEvent::InputError`, and `run()` returns that error, so the app exits instead of hanging with no keyboard.
- **API results**: spawned tasks send `AppEvent::AccountsLoaded`, `AppEvent::TransactionsLoaded`, or `AppEvent::CategoriesLoaded` when HTTP requests complete.
- **Message timers**: `AppEvent::MessageExpired(id)` (see Status Messages below).

The main loop is: draw → recv event → handle event → repeat, until `state.should_quit`.

Event handling (`handle_event`, `handle_key`, …) is synchronous. **The main loop is the only consumer of the channel, so it must never `send(..).await` on it** — if the channel is full that send can never complete and the app deadlocks. Only spawned tasks send into the channel.

### State

`App` (`src/app/mod.rs`) owns the API client, the channel, the current message's id, and `AppState`.

`AppState` (`src/app/state.rs`) holds:

- `tabs` — one `TabState` per account, grouped by account type (spending, savers, home loans, others; see `Account::type_order`) and otherwise in API order. The sidebar shows them in this order, and next/previous account follow it. (They're called tabs for historical reasons: there's no tab bar any more.)
- `active_tab` — index of the selected account's tab
- `mode` — `AppMode::Normal`, `AppMode::Detail` while the detail popup is open, or `AppMode::Theme(ThemeSelectorState)` while the theme picker is open
- `accounts_generation` — incremented for every accounts fetch; stale `AccountsLoaded` results are dropped
- `categories` — the `Categories` (`src/app/categories.rs`), fetched once on startup via `GET /categories`: each category's name and parent
- `theme` — the current theme, an `Arc<opaline::Theme>` that is always opaline's global theme (`opaline::current()`); see Theming
- `tz` — the local `jiff` time zone, looked up once at startup; used for displaying times and grouping transactions by day (don't call `TimeZone::system()` elsewhere)
- `show_detail` — whether the detail pane is visible
- `show_sidebar` — whether the account sidebar is visible (it is by default)
- `view` — `View::Transactions` (the transaction list and detail pane) or `View::Insights`; toggled with `i`, and kept when switching accounts
- `message` — the status message shown in the header bar (`Message`: its text and `MessageKind`, `Info`, `Success` or `Error`), if any
- `page_size` — how many transactions fit in the list view (computed from the list block's inner area, 2 rows per item); written during rendering and used for `Ctrl+D`/`Ctrl+U` and for deciding when to fetch the next page

`TabState` holds:

- `account` — the `Account` this tab shows (name, type, ownership, balance)
- `store` — the account's `TransactionStore` (see below)
- `selected` — index into the store's transactions. Invariant: `selected < len` (or 0 when empty). Pages go into the store through `TabState::page_loaded()`, which, when the list is replaced, keeps the same transaction selected (by `id`) if it is still present and clamps otherwise.
- `list_state` — ratatui `ListState`, used for scrolling

`recent_totals()` adds up the recent period's transactions into money in and money out.

`TransactionStore` (`src/app/store.rs`) holds one copy of the account's transactions, newest first, as the API returns them. The transaction list, the account header's totals and the Insights view all read from it: `transactions()` is everything loaded, and `recent()` is the transactions over the last `RECENT_DAYS` (30) days including today, or `None` until they're all loaded. It also holds the page cursor (`next_page`), the fetch in flight (`loading()`: `LoadKind::Replace` for one starting from the newest page, `Append` for the next page; at most one at a time) and a generation per fetch, so superseded results are dropped. The store is plain state that decides what to fetch next; `App` makes the requests.

`set_accounts()` keeps the `TabState` of accounts that still exist (matched by account id, with the account data such as the balance updated) and keeps the active tab on the same account.

### Loading, Refresh and Pagination

- On startup, accounts and categories are fetched. When accounts arrive, the active tab's transactions are fetched.
- `App::load_tab()` asks the tab's store for the next fetch (`TransactionStore::next_fetch()`, given the start of the recent period, `recent_since()`, and whether the list wants more) and starts it. The store asks for, in order: the rest of a refresh in progress; the first page if nothing has loaded; the next page if the recent period isn't all loaded, or the selection is within `page_size` items of the end of the list. `load_tab()` is called after every page loads, so loading carries on until the recent period is covered, when accounts arrive, when switching tabs (`ensure_tab_loaded()`, which also shows a message for a first load), and after every Normal-mode or detail-popup key. It isn't called after a failure, so a failed fetch is retried by the next of those, not straight away.
- On the first load, the first page is shown as soon as it arrives, and the rest of the recent period is appended.
- `r` re-fetches the account list (refreshing balances, and recovering if the initial accounts load failed) and refreshes the current tab (`TransactionStore::refresh()`), which supersedes any fetch in flight. A refresh fetches from the newest page until the recent period is covered, keeping the old transactions until then and replacing them all at once (dropping any older pages), so the list, totals and Insights always show the same data. If it fails, the old transactions stay. Pages fetched from the newest one bypass the HTTP cache.
- There are no separate fetches for the recent period: following the page cursor until a transaction older than the period's start (or the last page) gets the same transactions as `filter[since]` would, and they stay in the list.
- Transactions are fetched 100 per page (`store::PAGE_SIZE`, the API's maximum), so a month usually takes one request.
- `TransactionsLoaded` identifies its tab by `account_id` (not tab index) plus the fetch's generation. Only pages that replace the list show a success message; failures show an error message.

### Status Messages

Status messages (loading, success, errors) are shown in the header bar, along the top of the screen, right-aligned after the app's name and version. There's one at a time: a new message replaces the current one. `App::show_message()` (through `info()`/`success()`/`error()`) sets `state.message`, assigns the message an id, and spawns a timer (`MESSAGE_DURATION`, 3 seconds, or `ERROR_DURATION`, 8 seconds, for errors) that sends `MessageExpired(id)`; only the most recent message's timer clears it.

### Rendering

`impl Widget for &mut App` in `src/ui/mod.rs` draws the background, the header bar along the top, the account sidebar on the left (when `show_sidebar` is set), the transaction list (and the detail pane on the right, taking one third of the screen's width, when `show_detail` is set; the list takes whatever is left), the status bar, the detail popup when in `AppMode::Detail`, and the theme picker overlay when in `AppMode::Theme`. Rendering takes `&mut` because it records `page_size`, updates each tab's `list_state`.

The sidebar (`src/ui/sidebar.rs`) lists the accounts under a heading per account type, each with its balance right-aligned, the selected one highlighted like the selected transaction, and the total of all balances at the bottom (when there's more than one account and `MoneyObject::sum` can add them up, i.e. they're in the same currency). Its width fits the longest row, between 20 and 40 columns and at most a third of the screen; longer names are truncated. 
The account header (`src/ui/account_header.rs`) sits above the transaction list and the detail pane. It shows the selected account's name, type and ownership on one line, then its balance and, right-aligned, its money in and out over the last 30 days (on a line of their own if they don't fit). The totals show "loading…" until they're loaded, the stale totals while they're refreshed, and nothing if loading failed. Its height comes from `header_height()` (0 when there is no account yet). Because the header names the account, the list's title is just "Transactions" plus the loading status. `ui::right_aligned()` is the shared helper for a line with a right-aligned part that wraps onto its own line when it doesn't fit (also used for the detail pane's amount and status).

The Insights view (`src/ui/insights.rs`) replaces the transaction list and detail pane below the header when `view` is `View::Insights`. It's computed from the store's `recent()` transactions (the same 30 days as the header's totals), with the statistics in `src/app/insights.rs` as pure, unit-tested functions:

- **Balance** (`daily_balances()`), at the top: the balance at the end of each day, worked backwards from the current balance through the recent transactions, drawn as a ratatui `Chart` (Braille line) with the first, middle and last dates. The vertical axis (`YAxis`) always includes zero, so the line's height is in proportion to the balance rather than exaggerating small changes, and is rounded out to multiples of a round step (1, 2 or 5 × a power of ten, at least one whole currency unit), which label it as whole amounts ("$2000"). The section has a fixed height (`BALANCE_HEIGHT`), so the chart is the same size for every account; on a short screen it shrinks to leave room for the categories, and is left out below `MIN_BALANCE_HEIGHT`.
- **Spending by category** (`spending_by_category()`), below it: the debits, excluding transfers between the user's own accounts (`transfer_account` is set), summed per category (`None` is shown as "Uncategorised"), largest first; refunds aren't subtracted. Each category is a row with a bar (in eighths of a column, relative to the largest) in its category's colour (see Theming), the amount and its share. The list gets the space left by the balance section; if the categories don't fit, the smallest are grouped as "Other".

While `recent()` isn't loaded, the view shows "Loading…", or a message to press `r` if loading failed.

The detail pane and the detail popup (both in `src/ui/transaction_detail.rs`) show the same content, built by `detail_lines()`, for `state.selected_transaction()`. The popup is centred, at most 72 columns wide, and shrinks to the height of its content; it draws nothing if no transaction is selected. The content has a header (description, amount with the status right-aligned, category in its colour, tags) followed by sections (When, Details, Note, Message, Raw text) that are left out when empty. Redundant information is hidden: the hold amount when it equals the final amount, and the raw text when it matches the description. Its lines are wrapped to the pane's width by `textwrap` (first-fit), with long field values indented to the value column, so the `Paragraph` is rendered without its own wrapping. Dates are shown as "Today, 14:32", "Yesterday, …" or "Mon 18 Nov, …" (with the year only if it isn't the current one).

## Project Structure

```
src/
  main.rs              Entry point: load config, init terminal, run app, restore
  backend.rs           Terminal setup, and a backend wrapper working around a ratatui emoji bug
  config.rs            Locate, load and validate config.toml

  client/
    mod.rs             UpClient: Bearer auth, HTTP cache middleware, account/transaction/category endpoints;
                       Client: the app's data source, the API or the demo data
    demo.rs            DemoBank: generated accounts and transactions for demo mode
    models.rs          JSON:API envelope types, relationship types, domain structs, money formatting

  app/
    mod.rs             App: main loop, event and key handling, fetch spawning, status messages
    state.rs           AppState, AppMode, View, TabState
    store.rs           TransactionStore: an account's transactions, paging, refresh
    categories.rs      Categories: names, parents, and the root categories' colour tokens
    insights.rs        Statistics for the Insights view (spending by category, daily balances)
    event.rs           AppEvent enum, crossterm event reader task

  ui/
    mod.rs             Top-level layout (Widget impl for App), theme picker overlay
    sidebar.rs         Account sidebar: accounts grouped by type, balances, total
    header_bar.rs      Header bar: app name and version, status messages
    account_header.rs  Selected account's name, type, balance and 30-day totals
    insights.rs        Insights view: spending by category bars, balance chart
    transaction_list.rs  Transaction list grouped by day, description truncation
    transaction_detail.rs  Detail pane and popup with full transaction info
    status_bar.rs      Keybinding hints, per mode
```

## Demo Mode

`up --demo` runs the app on generated data instead of the Up API, without reading the API token (the config is read only for the theme, and ignored if it's missing or invalid). `App` gets its data from a `Client` (`src/client/mod.rs`): `Client::Up(UpClient)` or `Client::Demo(DemoBank)`, which have the same methods. Everything above the client, including the `TransactionStore` paging, works the same in both.

`DemoBank` (`src/client/demo.rs`) generates the data at startup from a fixed seed (with its own small `Rng`), relative to the current local date, so it's the same on every run on a given day: five accounts (spending, 2Up, two savers with emoji names, one with U+FE0F, and a home loan) with about four months of transactions (pay, transfers between the accounts on both sides, bills, interest, and everyday purchases), and balances that add up (each account's balance is its opening balance plus its transactions). There's more than a page (`store::PAGE_SIZE`) of spending in any 30 days, so covering the recent period takes several fetches. Every request waits `LATENCY` (300 ms), so the loading states show. Pages are served from the generated list, with made-up `next_page_url`s (`demo://…?offset=N`).

When adding something the UI shows, add it to the demo data too (in `Generator::day()` for regular transactions, or `Generator::one_offs()` for a single example), so it can be tried and screenshotted without real data. `client::demo::tests` checks the data (ordering, unique ids, both sides of transfers, categories, paging, at least one example of each kind of transaction) and renders the whole app on it in every view; set `UP_DEMO_SCREENS=<file>` when running that test to write the rendered screens to a file.

## API Layer

- Base URL: `https://api.up.com.au/api/v1`
- Auth: `Authorization: Bearer {token}` set as a default header on the `reqwest::Client`
- The client is wrapped with `reqwest-middleware` and `http-cache-reqwest` (in-memory Moka cache, `CacheMode::Default`). Endpoint methods take a `reload` flag; when set, the request uses `CacheMode::Reload` (bypass the cache, update it with the response). It is used for user-initiated refreshes.
- `get_accounts` and `get_transactions` take `PaginationOptions` (a page size, 50 by default and 100 for transactions, or a `next_url` from a previous response) and return `PaginatedResponse { data, next_page_url }`
- `Resource<A, R>` is generic over attributes (`A`) and relationships (`R`, defaults to `serde_json::Value`)
- Format money with the `MoneyObject` methods: `format_display()` for transaction amounts (`+$10.56` for credits, `$10.56` for debits, which are distinguished by colour), `format_signed()` for balances (`-$10.56`), `format_unsigned()` for totals known to be money out (`$10.56`, and `$0.00` rather than `format_display()`'s `+$0.00`), and `format_display_with_code()` for foreign currencies (`+10.56 USD` / `10.56 USD`, signed like `format_display()`). They take the sign from `value_in_base_units` and the digits from the API's `value` string; don't parse `value` as a float or assume two decimal places for foreign currencies. Add amounts up with `MoneyObject::sum()` (exact, in base units; `None` for mixed currencies), starting from `zero_like()` when there may be nothing to add; `with_base_units()` makes an amount in the same currency from base units (e.g. for chart labels), and `base_units_per_unit()` gives the number of base units in a whole unit (100 for dollars)
- Transaction relationships (category, parentCategory, tags, transferAccount) are deserialized into typed `TransactionRelationships` and flattened into the `Transaction` domain struct as IDs. The detail pane resolves the transfer account's name from the loaded accounts via `state.account_name()`
- `GET /categories` is called once on startup and returns `Category` structs (id, name, and the parent's id, from the `parent` relationship), stored as `state.categories`. The UI resolves IDs to names via `state.category_name()`, falling back to the raw ID if they haven't loaded yet
- Categories are hierarchical. The detail pane displays them on a single line as "Parent › Category" (e.g. "Good Life › Restaurants & Cafes")
- All errors are `rootcause` reports (`rootcause::Result`); there are no custom error types. Client methods return the underlying `reqwest`/`reqwest-middleware` errors as reports, which `AppEvent` results carry as-is. Status messages show only the report's top-level message (`report.format_current_context()`), not the full report, whose `Display` is a multi-line tree with source locations; that full form is only printed for errors that end the program (`main`, `config`, `App::new`/`App::run`)

## Key Bindings

| Key                 | Mode         | Action                                      |
|---------------------|--------------|---------------------------------------------|
| `q` / `Q` / `Ctrl+C`| Normal       | Quit                                        |
| `Tab` / `l`         | Normal       | Next account                                |
| `Shift+Tab` / `h`   | Normal       | Previous account                            |
| `j` / `↓`           | Normal       | Next transaction                            |
| `k` / `↑`           | Normal       | Previous transaction                        |
| `Ctrl+D` / `Ctrl+U` | Normal       | Page down / up                              |
| `g` / `G`           | Normal       | Jump to top / bottom of the loaded list     |
| `[` / `]`           | Normal       | Jump to the start of the day group above / below (the list is newest first) |
| `Enter`             | Normal       | Open the detail popup (if a transaction is selected) |
| `o`                 | Normal       | Toggle the detail pane                      |
| `a`                 | Normal       | Toggle the account sidebar                  |
| `i`                 | Normal       | Switch between the transaction list and the Insights view |
| `Esc` / `Enter` / `q` | Detail popup | Close the popup                           |
| `Ctrl+C`            | Detail popup | Quit                                        |
| navigation keys     | Detail popup | As in Normal mode (`j`/`k`, `Ctrl+D`/`Ctrl+U`, `g`/`G`, `[`/`]`); the popup shows the newly selected transaction |
| `r`                 | Normal       | Refresh accounts and the current account's transactions |
| `t`                 | Normal       | Open the theme picker                       |
| `↑` / `↓`           | Theme picker | Move selection                              |
| typing / `Backspace`| Theme picker | Filter themes                               |
| `Enter`             | Theme picker | Apply the selected theme                    |
| `Esc`               | Theme picker | Close without changing the theme            |

Key handling lives in `App::handle_key`, which dispatches on the mode to `App::handle_normal_key`, `App::handle_detail_key`, or `ThemeSelectorState::handle_key`. The navigation keys, shared by Normal mode and the detail popup, are in `App::handle_navigation_key`. Selection movement is implemented as `TabState` methods (`select_next`, `page_down`, `select_next_day`, …), which keep `selected` in bounds and are unit-tested; the key handler applies them to the active tab via `App::navigate`. In the Insights view, the keys that act on the transaction list (navigation, `Enter`, `o`) are ignored. After every Normal-mode or detail-popup key, `load_tab()` checks whether to load more transactions. The status bar hints in `src/ui/status_bar.rs` (one list per mode, and for the Insights view) should be kept in sync with the bindings.

## Theming

Powered by the `opaline` crate (with its `widgets` feature, for the `ThemeSelector` picker). The initial theme comes from the config `theme` field (an opaline builtin id), defaulting to opaline's default theme. `t` opens the picker to choose any builtin theme at runtime; the choice is not saved to the config file.

The picker (`ThemeSelector`) draws itself with opaline's *global* theme, and previews the highlighted theme by changing it (restoring the original on `Esc`). So `AppState::new` makes the configured theme the global one, and after every key in theme mode `state.theme` is set back to `opaline::current()`: the picker and the rest of the UI always use the same theme, and the whole UI previews the highlighted theme. Don't set `state.theme` without also calling `opaline::set_theme`.

All UI modules read colors from `state.theme` via semantic tokens (`opaline::names::tokens`) — no hardcoded colors, and nothing drawn by a third-party widget with its own colours. Use `theme.color(token)`; `theme.line()`/`span()`/`style()` take opaline *style* names (`opaline::names::styles`), not tokens, and silently return an unstyled result for an unknown name. Tokens in use:

- `BG_BASE` / `TEXT_PRIMARY` — base background and foreground, including the account header and transaction list
- `BG_PANEL` — sidebar and detail pane background (the transaction list is on `BG_BASE`)
- `BORDER_UNFOCUSED` — the sidebar's right border, the account header's bottom border, and the detail pane's left border, which separate them from the list (some themes use the same colour for `BG_BASE` and `BG_PANEL`)
- `BG_ELEVATED` — detail popup and theme picker backgrounds
- `BORDER_FOCUSED` — the detail popup's border
- `BG_SELECTION` — selected transaction and account background
- `ACCENT_PRIMARY` — the app name in the header bar; sidebar, list, Insights, detail pane and popup titles, selection markers, the balance chart's line
- `TEXT_MUTED` — foreign amounts and raw text in the detail pane, and the colour for no category (or an unknown one); `TEXT_DIM` — the version in the header bar, day separators, sidebar, detail pane and Insights section headings, the Insights shares, chart axes and labels, the account header's type and labels, times, status bar text
- `TEXT_SECONDARY` — field labels in the detail pane; tag text; the sidebar's total; status message text (other than errors)
- Category colours, from `state.category_color()` (the category in the detail pane header and the Insights category bars): each of Up's root categories has a fixed token, and its children get the same colour (`ROOT_COLORS` in `src/app/categories.rs`): Good Life `WARNING`, Home `INFO`, Personal `SUCCESS`, Transport `ERROR`. They're the status tokens because those are clearly different hues in every builtin theme (a test checks that they're all different); other tokens, like the accents, repeat in some themes. Other categories (if Up adds a root category) are `TEXT_MUTED`.
- `BG_HIGHLIGHT` — the header bar's and status bar's background, tag backgrounds in the detail pane
- Amounts: `TEXT_PRIMARY` for debits (and money out in the account header), `SUCCESS` for credits (and money in)
- `SUCCESS` / `WARNING` — settled / pending (held) status
- `INFO` — keybinding hints in the status bar
- `INFO` / `SUCCESS` / `ERROR` — the status message's marker (`●`, `✓`, `✗`), by `MessageKind`; error messages are all `ERROR`, the others `TEXT_SECONDARY`

## Dependencies

When adding or updating dependencies in `Cargo.toml`, always specify the full version including the patch number (e.g. `"1.0.102"` not `"1"` or `"1.0"`). This applies to both workspace and package-level dependencies.

## Code Quality

All code must pass `cargo clippy --workspace` with zero warnings before being considered complete. Always run clippy after making changes and fix any warnings it reports.

Run `cargo fmt --all` after making changes to ensure consistent formatting across the workspace.

Run `cargo test` after making changes. Unit tests live alongside the code in `#[cfg(test)]` modules (e.g. `src/app/state.rs`, `src/ui/transaction_list.rs`). `app::state::tests` has helpers for building test fixtures. CI runs `cargo test --workspace` and `cargo clippy --workspace -- -D warnings`.

## Error Handling

- **Config errors**: Returned from `main()` and printed to stderr before entering TUI mode.
- **API/network errors**: Shown as error messages in the header bar. The app remains usable with stale data; `r` retries.
- **Terminal input errors**: `run()` returns the error, the terminal is restored, and the app exits.
- **Panics**: The panic hook installed by `ratatui::init()` (called by `backend::init()`) restores the terminal.
