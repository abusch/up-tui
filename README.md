# up-tui

A terminal UI for [Up Banking](https://up.com.au/) that lets you browse your accounts and transactions from the command line.

![Rust](https://img.shields.io/badge/language-Rust-orange)

## Features

- View all your Up accounts as tabs with balances
- Browse transactions with date, description, amount, and status
- Transaction detail overlay showing full information including category hierarchy, tags, foreign amounts, round-ups, cashback, and card details
- Vim-style keyboard navigation
- Lazy-loaded transactions per account
- Refresh data on demand
- 15 built-in color themes, switchable at runtime

## Setup

1. Get a personal access token from [Up API](https://api.up.com.au/)

2. Create the config file:

   ```sh
   mkdir -p ~/.config/up-tui
   echo 'api_token = "up:yeah:your-token-here"' > ~/.config/up-tui/config.toml
   ```

3. Optionally set a theme (defaults to Tokyo Night):

   ```toml
   theme = "dracula"
   ```

   Available themes: `tokyo-night`, `dracula`, `nord`, `catppuccin-mocha`, `catppuccin-latte`, `gruvbox-dark`, `gruvbox-light`, `solarized-dark`, `solarized-light`, `monokai-pro`, `rose-pine`, `kanagawa`, `everforest`, `cyberpunk`, `one-dark-pro`

4. Build and run:

   ```sh
   cargo run
   ```

## Key Bindings

| Key                | Action               |
|--------------------|----------------------|
| `Tab` / `l`        | Next account         |
| `Shift+Tab` / `h`  | Previous account     |
| `j` / `Down`       | Next transaction     |
| `k` / `Up`         | Previous transaction |
| `g` / `G`          | Jump to top / bottom |
| `Enter`            | View transaction detail |
| `Esc` / `q`        | Close detail / quit  |
| `r`                | Refresh transactions |
| `t` / `T`          | Next / previous theme |
| `Ctrl+C`           | Quit                 |
