# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.3.0](https://github.com/abusch/up-tui/compare/v0.2.3...v0.3.0) - 2026-11-23

### Added

- Show a QR code of the selected transaction's link into Up's app, with o
- Show the category in the transaction list, under the description, with the time on the right
- Show the spending by root category as a bar in the account header
- Set days apart in the transaction list with a banded header instead of a dotted rule
- Move up and down in the theme picker with Ctrl+P / Ctrl+N
- Make the account header stand out, on the panel background with the name in the accent colour
- Add a help screen listing the key bindings, opened with ?
- Default Insights to the current month, with p for the last 30 days
- Toggle the detail pane with Enter instead of a popup
- Show a subcategory's transactions from the Insights breakdown
- Group Insights spending by root category, with a breakdown of each
- Show status messages in a header bar instead of toasts
- Toggle the account sidebar with a instead of s
- Give each root category its own colour
- Add a demo mode with generated accounts and transactions
- Add an Insights view with spending by category and the balance over time
- Add an account header with balance, type and 30-day totals
- Show accounts in a sidebar instead of tabs
- Redesign the transaction detail pane and add a detail popup

### Fixed

- Report an invalid config file in demo mode
- Keep dim text readable on every background, in every theme
- Work around ratatui shifting text after emoji with a variation selector
- Use the base background for the list and panel background for the detail pane
- Take all UI colours from the current theme
- Align detail pane labels and simplify its code
- Format money amounts consistently
- Correct alignment, toasts, refresh and paging
- Resolve freeze, crash and macOS config bugs

### Other

- Share text layout helpers and remove duplication
- Find the config directory with etcetera instead of dirs, and drop the legacy macOS location
- Trim AGENTS.md to rules and non-obvious context
- Load each account's transactions into one TransactionStore
- Replace thiserror with rootcause in the API client
- Remove dead code and unneeded state
- Rename list_height to page_size and fix its value
- Pass client errors through AppEvent unwrapped
- Look up the system time zone once
- Move transaction navigation onto TabState
- Store each account in its tab state
- Update AGENTS.md and README to match code
- Upgrade deps
- Tweak UI a bit
- replace anyhow with rootcause for error handling

## [0.2.3](https://github.com/abusch/up-tui/compare/v0.2.2...v0.2.3) - 2026-04-15

### Added

- add more fields to transaction details
- Make transaction details its own panel instead of modal

### Other

- replace ratatui-themes with opaline
- more refactoring
- Introduce App struct and move some logic there
- small UI tweaks

## [0.2.2](https://github.com/abusch/up-tui/compare/v0.2.1...v0.2.2) - 2026-04-06

### Other

- *(ci)* fix release-binaries workflow

## [0.2.1](https://github.com/abusch/up-tui/compare/v0.2.0...v0.2.1) - 2026-04-06

### Added

- add [/] shortcuts to jump between days

## [0.2.0](https://github.com/abusch/up-tui/releases/tag/v0.2.0) - 2026-04-03

### Added

- Add HTTP caching support to the client
- *(ci)* Add CI/CD workflow
- Tweak display of transaction list
- Add Ctrl+D/Ctrl+U page scrolling for transaction list
- Remove status column
- Add support for themes
- Use categorie's display names instead of IDs
- Display category and tags in tx details

### Fixed

- directly deserialize ThemeName
- Persist ListState to prevent jumpy scrolling
- make event handling more async
- *(ui)* Remove borders from tabs and status line

### Other

- disable some unnecessary tokio features
- rename binary to `up`
- Update Cargo.toml
- various clean ups
- Switch from chrono to jiff
- Replace Table with List widget for transaction list
- Simplify negative amount display in transaction list
- Store config in appstate
- Move cargo deps versions at workspace level
- Run cargo fmt
- Move `api` module to new `up-api` crate
- switch to edition 2024 and bump some dependencies
- Add README.md
- Add AGENTS.md
- use ratatui::init() instead of handling terminal init ourselves
- Initial commit
