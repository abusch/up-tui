# up-tui

A terminal UI for viewing Up Banking accounts and transactions, built with ratatui, opaline (theming), and tokio + reqwest. The binary is `up`; it takes only `--demo` and `-h`/`--help`, parsed by hand in `main()`.

This file holds rules and context that the code doesn't make obvious. Don't use it to describe what the code already shows (struct fields, layout details, every feature): read the code for that.

## Project Structure

```
src/
  main.rs              Entry point: parse args, load config, init terminal, run app, restore
  backend.rs           Terminal setup, and a backend wrapper working around a ratatui emoji bug
  config.rs            Locate, load and validate config.toml

  client/
    mod.rs             UpClient (HTTP, cache) and Client: the app's data source, the API or the demo data
    demo.rs            DemoBank: generated accounts and transactions for demo mode
    models.rs          JSON:API types, domain structs, money formatting

  app/
    mod.rs             App: main loop, event and key handling, fetch spawning, status messages
    state.rs           AppState, AppMode, View, TabState, InsightsState, CategoryFilter
    list.rs            ListSelection: the selection in a list of transactions, and navigation
    store.rs           TransactionStore: an account's transactions, paging, refresh
    categories.rs      Categories: names, parents, and the root categories' colour tokens
    insights.rs        Statistics for the Insights view (pure, unit-tested)
    event.rs           AppEvent enum, crossterm event reader task

  ui/
    mod.rs             Top-level layout (Widget impl for App), theme picker overlay
    contrast.rs        readable(): dim text that stays readable on its background
    sidebar.rs, header_bar.rs, account_header.rs, insights.rs,
    transaction_list.rs, transaction_detail.rs, status_bar.rs, help.rs
```

## Configuration

Read from `$XDG_CONFIG_HOME/up-tui/config.toml` (if set and absolute), else `~/.config/up-tui/config.toml`, on every platform. `dirs::config_dir()` is deliberately not used, because on macOS it is `~/Library/Application Support`; a file there is still read as a legacy fallback. The config (`api_token`, and an optional opaline `theme` id) is validated before the TUI starts, and errors are returned from `main()`.

## Architecture

- **Terminal backend**: `backend::init()` returns an `AppTerminal` whose crossterm backend is wrapped in `WideCharBackend`, working around ratatui issue #2651 (an emoji with U+FE0F drawn over non-blank cells shifts the rest of the line one column right). Keep using it rather than `DefaultTerminal` until a ratatui-crossterm release (after 0.1.2) includes PR #2721; then upgrade, delete `src/backend.rs`, and go back to `ratatui::init()`.
- **Text width**: measure whole graphemes (as `truncate_to_width()` does), not single `char`s: an emoji with U+FE0F is 2 columns wide though its characters' widths add up to 1.
- **Event loop**: all input, API results and message timers flow through one bounded `mpsc::channel::<AppEvent>(100)`; the loop is draw → recv → handle. Event handling is synchronous. **The main loop is the only consumer of the channel, so it must never `send(..).await` on it** (a full channel would deadlock). Only spawned tasks send.
- **Fetching**: `TransactionStore` is plain state that decides what to fetch next (`next_fetch()`); `App::load_tab()` makes the requests. Results carry the account id and a generation, so superseded results are dropped (accounts fetches have a generation too). A refresh keeps the old transactions until the selected period (`state.period`, a `Period`: this month or the last 30 days, switched with `p`) is covered, then replaces them all at once, so the list, totals and Insights always agree. There is one period for the header's totals, Insights and the filtered list; `TransactionStore::recent()` follows the period passed to its last `next_fetch()`/`refresh()`, so call `App::load_tab()` after changing it. A failed fetch isn't retried straight away.
- **The list shown**: the transaction list may be filtered by category (`state.filter`, with its own `ListSelection`). Code that acts on "the list" must go through `AppState::list_mut()` and `AppState::selected_transaction()`, not the active tab's list directly.
- **Time zone**: use `state.tz`, looked up once at startup; don't call `TimeZone::system()` elsewhere.
- **Status messages**: `App::info()`/`success()`/`error()`. One at a time; each has an id so only the latest message's timer clears it.

## Money

Format with the `MoneyObject` methods: `format_display()` for transaction amounts (credits get a `+`; debits are distinguished by colour), `format_signed()` for balances, `format_unsigned()` for totals known to be money out, `format_display_with_code()` for foreign currencies. They take the sign from `value_in_base_units` and the digits from the API's `value` string: never parse `value` as a float or assume two decimal places. Add amounts with `MoneyObject::sum()` (exact; `None` for mixed currencies), starting from `zero_like()` when there may be nothing to add.

## Errors

All errors are `rootcause` reports (`rootcause::Result`); there are no custom error types. Status messages show only `report.format_current_context()`; the full multi-line report is printed only for errors that end the program. API errors leave the app usable with stale data (`r` retries).

## Theming

- `state.theme` must always be opaline's global theme: the `ThemeSelector` picker draws with, and previews by changing, the global theme. Never set `state.theme` without also calling `opaline::set_theme`.
- No hardcoded colours, and nothing drawn by a third-party widget with its own colours: use semantic tokens (`opaline::names::tokens`) via `theme.color(token)`. `theme.line()`/`span()`/`style()` take *style* names (`opaline::names::styles`), not tokens, and silently return an unstyled result for an unknown name.
- Dim and muted text (`TEXT_DIM`, `TEXT_MUTED`) goes through `contrast::readable(theme, token, bg)`, with the background it's drawn on, not `theme.color()`. Themes copy editor palettes whose dim colours are barely visible even on the base background, and all but disappear on our lighter ones (Nord's `TEXT_DIM` on `BG_SELECTION`: 1.2:1). For text on a selected row, pass `BG_SELECTION`. The alternative crates (tui-nuances, ratatui-themes) have the same problem.
- Look at how existing UI uses tokens before picking one for something new (e.g. `BG_PANEL` for side panes, `BG_SELECTION` for selections, `TEXT_DIM` for secondary text, `TEXT_PRIMARY`/`SUCCESS` for debits/credits).
- Root categories get the status tokens (`ROOT_COLORS` in `src/app/categories.rs`) because those are clearly different hues in every builtin theme (a test checks this); accent tokens repeat in some themes.

## Demo Mode

`up --demo` runs on data generated by `DemoBank` (`src/client/demo.rs`) instead of the API, from a fixed seed relative to today's date, with simulated latency. **When adding something the UI shows, add it to the demo data too** (`Generator::day()` for regular transactions, `Generator::one_offs()` for a single example). `client::demo::tests` checks the data and renders the whole app in every view; set `UP_DEMO_SCREENS=<file>` when running it to write the rendered screens to a file.

## Key Bindings

Bindings are listed in the README. Key handling is in `App::handle_key`, dispatching by mode; transaction list navigation is in `App::handle_navigation_key`, built on `ListSelection` methods. When changing bindings, keep the README, the help screen (`SECTIONS` in `src/ui/help.rs`, which lists them all) and the status bar hints in `src/ui/status_bar.rs` in sync. The status bar has room for only the most useful keys in each view, ending with `?` for the help screen.

## Dependencies

When adding or updating dependencies in `Cargo.toml`, always specify the full version including the patch number (e.g. `"1.0.102"`, not `"1"` or `"1.0"`).

## Code Quality

Before considering a change complete:

- `cargo clippy --workspace` with zero warnings (CI uses `-D warnings`)
- `cargo fmt --all`
- `cargo test` (unit tests live in `#[cfg(test)]` modules alongside the code; `app::state::tests` has fixture helpers)
