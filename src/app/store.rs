//! An account's transactions, loaded from the API page by page.
//!
//! One store per account holds a single copy of its transactions, newest first, as the
//! API returns them. Everything that shows transactions reads from it: the transaction
//! list, and (through `recent()`) the account header's totals and the Insights view.
//!
//! The store only keeps state and decides what to fetch next (`next_fetch()`,
//! `refresh()`); `App` makes the requests and passes the results back
//! (`page_loaded()`, `load_failed()`).

use jiff::Timestamp;

use crate::client::models::{PaginatedResponse, Transaction};

/// Transactions fetched per page: the most the API allows, so that a month of
/// transactions usually takes a single request.
pub const PAGE_SIZE: u32 = 100;

/// What kind of fetch is in flight.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoadKind {
    /// A fetch starting from the newest page: the initial load, or a refresh. A
    /// refresh's pages replace the list once they cover the recent period, so the
    /// list, totals and Insights all change at once.
    Replace,
    /// The page after the loaded ones, appended to the list.
    Append,
}

/// A fetch for `App` to start.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fetch {
    pub kind: LoadKind,
    /// The page's URL, or `None` for the newest page.
    pub url: Option<String>,
    /// Pass back to `page_loaded()` or `load_failed()` with the result.
    pub generation: u64,
}

/// What a loaded page did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageOutcome {
    /// The list was replaced (the initial load, or a refresh that's complete).
    Replaced,
    /// The page was added to the end of the list.
    Appended,
    /// The page belongs to a refresh that needs more pages before it replaces the list.
    Staged,
}

/// The pages of a refresh so far.
#[derive(Debug, Default)]
struct Staged {
    transactions: Vec<Transaction>,
    next_page: Option<String>,
}

#[derive(Debug, Default)]
pub struct TransactionStore {
    /// Newest first. `None` until the first page has loaded.
    transactions: Option<Vec<Transaction>>,
    /// URL of the page after `transactions`, if there are more.
    next_page: Option<String>,
    /// The start of the recent period that `recent()` covers, as of the last
    /// `next_fetch()` or `refresh()`.
    period_start: Option<Timestamp>,
    /// The fetch in flight, if any. There is at most one, so a page that arrives is
    /// never older than the transactions already loaded.
    loading: Option<LoadKind>,
    /// Incremented for every fetch started. Results carrying an older value were
    /// superseded and are dropped.
    generation: u64,
    /// The pages of a refresh that doesn't cover the recent period yet.
    staged: Option<Staged>,
}

impl TransactionStore {
    /// The loaded transactions, newest first, or `None` if none have loaded yet.
    pub fn transactions(&self) -> Option<&[Transaction]> {
        self.transactions.as_deref()
    }

    /// The fetch in flight, if any.
    pub fn loading(&self) -> Option<LoadKind> {
        self.loading
    }

    /// All the transactions since the start of the recent period, or `None` if they
    /// aren't all loaded (yet, or because loading failed).
    pub fn recent(&self) -> Option<&[Transaction]> {
        let since = self.period_start?;
        let transactions = self.transactions.as_deref()?;
        if !covers(transactions, self.next_page.is_some(), since) {
            return None;
        }
        // Newest first, so the ones since `since` come first.
        let end = transactions.partition_point(|t| t.created_at >= since);
        Some(&transactions[..end])
    }

    /// The fetch to start next, if any: the first page if none has loaded, the rest of
    /// a refresh, or the next page if the recent period since `period_start` isn't all
    /// loaded or the list `wants_more`. Nothing while a fetch is in flight.
    ///
    /// It's called after every page loads, so that loading continues until nothing more
    /// is needed, and when the user does something that might need more. It isn't called
    /// after a failure, so a failed fetch is retried then rather than straight away.
    pub fn next_fetch(&mut self, period_start: Timestamp, wants_more: bool) -> Option<Fetch> {
        if self.loading.is_some() {
            return None;
        }
        self.period_start = Some(period_start);
        if let Some(staged) = &self.staged {
            let url = staged.next_page.clone();
            return Some(self.start(LoadKind::Replace, url));
        }
        let Some(transactions) = &self.transactions else {
            return Some(self.start(LoadKind::Replace, None));
        };
        let url = self.next_page.clone()?;
        let needed = wants_more || !covers(transactions, true, period_start);
        needed.then(|| self.start(LoadKind::Append, Some(url)))
    }

    /// Reload from the newest page, superseding any fetch in flight. The loaded
    /// transactions stay until the new pages cover the recent period, then are
    /// replaced (and any older pages dropped).
    pub fn refresh(&mut self, period_start: Timestamp) -> Fetch {
        self.period_start = Some(period_start);
        self.staged = None;
        self.start(LoadKind::Replace, None)
    }

    fn start(&mut self, kind: LoadKind, url: Option<String>) -> Fetch {
        self.generation += 1;
        self.loading = Some(kind);
        Fetch {
            kind,
            url,
            generation: self.generation,
        }
    }

    /// Add a page fetched by the fetch with this generation. Returns `None` (and changes
    /// nothing) if that fetch was superseded.
    pub fn page_loaded(
        &mut self,
        generation: u64,
        page: PaginatedResponse<Transaction>,
    ) -> Option<PageOutcome> {
        if generation != self.generation {
            return None;
        }
        match self.loading.take()? {
            LoadKind::Append => {
                self.transactions.get_or_insert_default().extend(page.data);
                self.next_page = page.next_page_url;
                Some(PageOutcome::Appended)
            }
            LoadKind::Replace => {
                let mut staged = self.staged.take().unwrap_or_default();
                staged.transactions.extend(page.data);
                staged.next_page = page.next_page_url;
                // With nothing to show yet, show the first page straight away; the rest
                // of the period is then appended.
                let complete = self.transactions.is_none()
                    || self.period_start.is_none_or(|since| {
                        covers(&staged.transactions, staged.next_page.is_some(), since)
                    });
                if complete {
                    self.transactions = Some(staged.transactions);
                    self.next_page = staged.next_page;
                    Some(PageOutcome::Replaced)
                } else {
                    self.staged = Some(staged);
                    Some(PageOutcome::Staged)
                }
            }
        }
    }

    /// Record that the fetch with this generation failed. The loaded transactions stay,
    /// and a refresh in progress is abandoned. Returns `false` (and changes nothing) if
    /// that fetch was superseded.
    pub fn load_failed(&mut self, generation: u64) -> bool {
        if generation != self.generation {
            return false;
        }
        self.loading = None;
        self.staged = None;
        true
    }

    /// A store with these transactions loaded, and the recent period covering all of
    /// them.
    #[cfg(test)]
    pub fn loaded(transactions: Vec<Transaction>, next_page: Option<String>) -> Self {
        TransactionStore {
            transactions: Some(transactions),
            next_page,
            period_start: Some(Timestamp::MIN),
            ..Default::default()
        }
    }
}

/// Whether `transactions` (newest first) include all of those since `since`: there are
/// no more pages, or the oldest one is from before `since`.
fn covers(transactions: &[Transaction], has_more: bool, since: Timestamp) -> bool {
    !has_more || transactions.last().is_some_and(|t| t.created_at < since)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::state::tests::txn;

    fn since() -> Timestamp {
        "2026-11-05T00:00:00Z".parse().unwrap()
    }

    fn page(transactions: &[(&str, &str)], next: Option<&str>) -> PaginatedResponse<Transaction> {
        PaginatedResponse {
            data: transactions
                .iter()
                .map(|&(id, created_at)| txn(id, created_at))
                .collect(),
            next_page_url: next.map(str::to_owned),
        }
    }

    fn ids(transactions: Option<&[Transaction]>) -> Vec<&str> {
        transactions
            .unwrap_or_default()
            .iter()
            .map(|t| t.id.as_str())
            .collect()
    }

    fn fetch(kind: LoadKind, url: Option<&str>, generation: u64) -> Option<Fetch> {
        Some(Fetch {
            kind,
            url: url.map(str::to_owned),
            generation,
        })
    }

    #[test]
    fn first_page_shows_at_once_then_loading_continues_until_the_period_is_covered() {
        let mut store = TransactionStore::default();
        let first = store.next_fetch(since(), false);
        assert_eq!(first, fetch(LoadKind::Replace, None, 1));
        assert_eq!(store.next_fetch(since(), true), None, "one fetch at a time");

        let outcome = store.page_loaded(
            1,
            page(
                &[("a", "2026-11-20T10:00:00Z"), ("b", "2026-11-10T10:00:00Z")],
                Some("p2"),
            ),
        );
        assert_eq!(outcome, Some(PageOutcome::Replaced));
        assert_eq!(ids(store.transactions()), ["a", "b"]);
        assert!(store.recent().is_none(), "the period isn't all loaded yet");

        assert_eq!(
            store.next_fetch(since(), false),
            fetch(LoadKind::Append, Some("p2"), 2)
        );
        let outcome = store.page_loaded(
            2,
            page(
                &[("c", "2026-11-06T10:00:00Z"), ("d", "2026-11-01T10:00:00Z")],
                Some("p3"),
            ),
        );
        assert_eq!(outcome, Some(PageOutcome::Appended));
        assert_eq!(ids(store.recent()), ["a", "b", "c"]);

        // The period is covered: more is only fetched when the list wants it.
        assert_eq!(store.next_fetch(since(), false), None);
        assert_eq!(
            store.next_fetch(since(), true),
            fetch(LoadKind::Append, Some("p3"), 3)
        );
    }

    #[test]
    fn everything_is_loaded_when_there_are_no_more_pages() {
        let mut store = TransactionStore::default();
        let first = store.next_fetch(since(), true).unwrap();
        store.page_loaded(
            first.generation,
            page(&[("a", "2026-11-20T10:00:00Z")], None),
        );
        assert_eq!(ids(store.recent()), ["a"]);
        assert_eq!(store.next_fetch(since(), true), None);
    }

    #[test]
    fn a_refresh_replaces_the_list_once_it_covers_the_period() {
        let mut store = TransactionStore::default();
        let first = store.next_fetch(since(), false).unwrap();
        store.page_loaded(
            first.generation,
            page(
                &[
                    ("old", "2026-11-20T10:00:00Z"),
                    ("older", "2026-10-01T10:00:00Z"),
                ],
                Some("old-p2"),
            ),
        );

        let refresh = store.refresh(since());
        assert_eq!(refresh.kind, LoadKind::Replace);
        assert_eq!(refresh.url, None);
        let outcome = store.page_loaded(
            refresh.generation,
            page(&[("new", "2026-11-21T10:00:00Z")], Some("p2")),
        );
        assert_eq!(outcome, Some(PageOutcome::Staged));
        // Until the refresh is complete, the old transactions stay.
        assert_eq!(ids(store.transactions()), ["old", "older"]);
        assert_eq!(ids(store.recent()), ["old"]);

        let rest = store.next_fetch(since(), false);
        assert_eq!(
            rest,
            fetch(LoadKind::Replace, Some("p2"), refresh.generation + 1)
        );
        let outcome = store.page_loaded(
            refresh.generation + 1,
            page(
                &[
                    ("old", "2026-11-20T10:00:00Z"),
                    ("older", "2026-10-01T10:00:00Z"),
                ],
                Some("p3"),
            ),
        );
        assert_eq!(outcome, Some(PageOutcome::Replaced));
        assert_eq!(ids(store.transactions()), ["new", "old", "older"]);
        assert_eq!(ids(store.recent()), ["new", "old"]);
        assert_eq!(
            store.next_fetch(since(), true),
            fetch(LoadKind::Append, Some("p3"), 4)
        );
    }

    #[test]
    fn superseded_results_are_ignored() {
        let mut store = TransactionStore::loaded(
            vec![txn("a", "2026-11-20T10:00:00Z")],
            Some("p2".to_string()),
        );
        let append = store.next_fetch(since(), true).unwrap();
        let refresh = store.refresh(since());
        let late = page(&[("b", "2026-11-19T10:00:00Z")], None);
        assert_eq!(store.page_loaded(append.generation, late), None);
        assert!(!store.load_failed(append.generation));
        assert_eq!(store.loading(), Some(LoadKind::Replace));
        assert_eq!(ids(store.transactions()), ["a"]);
        assert!(store.load_failed(refresh.generation));
        assert_eq!(store.loading(), None);
    }

    #[test]
    fn a_failed_refresh_keeps_the_loaded_transactions() {
        let mut store = TransactionStore::loaded(vec![txn("a", "2026-11-20T10:00:00Z")], None);
        let refresh = store.refresh(since());
        store.page_loaded(
            refresh.generation,
            page(&[("b", "2026-11-21T10:00:00Z")], Some("p2")),
        );
        let rest = store.next_fetch(since(), false).unwrap();
        assert!(store.load_failed(rest.generation));
        assert_eq!(ids(store.transactions()), ["a"]);
        assert_eq!(ids(store.recent()), ["a"]);
        // The abandoned refresh isn't continued.
        assert_eq!(store.next_fetch(since(), false), None);
    }

    #[test]
    fn a_failed_first_load_is_retried_from_the_start() {
        let mut store = TransactionStore::default();
        let first = store.next_fetch(since(), false).unwrap();
        assert!(store.load_failed(first.generation));
        assert!(store.transactions().is_none());
        assert!(store.recent().is_none());
        assert_eq!(
            store.next_fetch(since(), false),
            fetch(LoadKind::Replace, None, 2)
        );
    }
}
