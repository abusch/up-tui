# up-tui

A terminal UI for [Up Banking](https://up.com.au/) that lets you browse your accounts and transactions from the command line.

![Rust](https://img.shields.io/badge/language-Rust-orange)

## Features

- View all your Up accounts in a sidebar, grouped by type, with their balances and the total
- Account header with the selected account's balance, type, and money in and out this month (or over the last 30 days), with a bar showing the spending by category
- Insights view: the balance and spending by category this month (or over the last 30 days), with a breakdown of each category by subcategory
- Browse transactions grouped by day, with description, amount, and time
- Detail pane showing full transaction information, including status, category hierarchy, tags, foreign amounts, round-ups, cashback, and card details
- Vim-style keyboard navigation
- Transactions loaded lazily per account, with older pages fetched automatically as you scroll
- Refresh accounts and transactions on demand
- Dozens of built-in color themes, with a picker to switch at runtime

## Installation

From [crates.io](https://crates.io/crates/up-tui) (installs a binary named `up`):

```sh
cargo install up-tui
```

Prebuilt binaries for Linux (x86_64) and macOS (Apple Silicon) are attached to each [GitHub release](https://github.com/abusch/up-tui/releases).

Or build from source with `cargo run`.

## Setup

1. Get a personal access token from [Up API](https://api.up.com.au/)

2. Create the config file at `~/.config/up-tui/config.toml` (or `$XDG_CONFIG_HOME/up-tui/config.toml` if you set `XDG_CONFIG_HOME`). This location is used on macOS too:

   ```sh
   mkdir -p ~/.config/up-tui
   echo 'api_token = "up:yeah:your-token-here"' > ~/.config/up-tui/config.toml
   ```

3. Optionally set a theme:

   ```toml
   theme = "dracula"
   ```

   Themes come from [opaline](https://crates.io/crates/opaline) and include `tokyo-night`, `dracula`, `nord`, `catppuccin-mocha`, `gruvbox-dark`, `solarized-light`, `rose-pine`, `one-dark`, and many more. If the name isn't recognised, up-tui prints the full list of available themes on startup. Without a `theme` setting, opaline's default theme (`silkcircuit-neon`) is used.

4. Run `up` (or `cargo run` from a checkout).

## Demo mode

To try up-tui without an Up account, run `up --demo` (or `cargo run -- --demo`). It shows made-up accounts and a few months of transactions instead of yours, and needs no API token or config file (it uses your configured theme if there is one). The data is generated relative to today, so it's the same on every run on a given day. It's also handy for screenshots that don't show your own finances.

## Key Bindings

The status bar shows the most useful keys for what's on screen; press `?` to see them all (`j` / `k` scroll the list if it doesn't fit, and `?`, `Esc` or `q` close it).

| Key                    | Action                                   |
|------------------------|------------------------------------------|
| `Tab` / `l`            | Next account                             |
| `Shift+Tab` / `h`      | Previous account                         |
| `j` / `Down`           | Next transaction                         |
| `k` / `Up`             | Previous transaction                     |
| `Ctrl+D` / `Ctrl+U`    | Page down / up                           |
| `g` / `G`              | Jump to top / bottom                     |
| `[` / `]`              | Jump to the day above / below            |
| `Enter`                | Toggle the transaction detail pane       |
| `o`                    | Show a QR code to open it in Up's app    |
| `a`                    | Toggle the account sidebar               |
| `i`                    | Switch between transactions and Insights |
| `p`                    | Switch between this month and 30 days    |
| `r`                    | Refresh accounts and transactions        |
| `t`                    | Open the theme picker                    |
| `?`                    | Show or hide the list of key bindings    |
| `q` / `Ctrl+C`         | Quit                                     |

In the Insights view, `j` / `k` select a category, `Enter` shows its breakdown by subcategory (where `j` / `k` select a subcategory), and `Esc` goes back to the list of categories. `Enter` on a subcategory shows its transactions over the period in the transaction list, with a bar above it saying what it's filtered by; `Esc` goes back to the breakdown.

`o` shows a QR code of the selected transaction's link into Up's app: scan it with your phone's camera to open the transaction there (to add a note or a receipt, say). `Esc` closes it.

If the window is too narrow for the transaction list and the detail pane side by side, the pane is shown in place of the list (the navigation keys still move through the transactions).

In the theme picker, use `Up` / `Down` (or `Ctrl+P` / `Ctrl+N`) to move (the whole UI previews the highlighted theme), type to filter, `Enter` to apply the theme, and `Esc` to close without changing it. The chosen theme isn't saved; set `theme` in the config file to make it permanent.
