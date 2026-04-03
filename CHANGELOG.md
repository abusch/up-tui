# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
