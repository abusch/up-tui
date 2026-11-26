//! An account's transactions, loaded from the API page by page.
//!
//! One store per account holds a single copy of its transactions, newest first, as the
//! API returns them. Everything that shows transactions reads from it: the transaction
//! list, and (through `recent()`) the account header's totals and the Insights view.
//!
//! The newest page is loaded first, and shown straight away. If the recent period goes
//! back further than that, the rest of it is split into ranges of dates that are loaded
//! in parallel, each following its own pages, and added all at once when they're all
//! in. Older transactions are then loaded a page at a time, as the list wants them.
//!
//! The store only keeps state and decides what to fetch next (`next_fetches()`,
//! `refresh()`); `App` makes the requests and passes the results back
//! (`page_loaded()`, `load_failed()`).

use std::collections::HashSet;

use jiff::{SignedDuration, Timestamp};

use crate::client::models::{PaginatedResponse, Transaction, TransactionPage};

/// Transactions fetched per page: the most the API allows, so that a month of
/// transactions usually takes a single request.
pub const PAGE_SIZE: u32 = 100;

/// The length of the ranges of dates that a period is split into, to load them in
/// parallel: about a month, which usually takes a page or two. A year takes a dozen.
const RANGE: SignedDuration = SignedDuration::from_hours(30 * 24);

/// How far ranges are widened at each end. Up's API doesn't say whether a range
/// includes its ends, so ranges overlap (and the duplicates are dropped) rather than
/// risk missing the transactions at their ends. Its times are to the second.
const OVERLAP: SignedDuration = SignedDuration::from_secs(1);

/// What kind of load is in progress.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoadKind {
    /// A load starting from the newest page: the initial load, or a refresh. A
    /// refresh's pages replace the list once they cover the recent period, so the
    /// list, totals and Insights all change at once.
    Replace,
    /// Transactions older than the loaded ones, added to the end of the list.
    Append,
}

/// Identifies a fetch, for `page_loaded()` and `load_failed()`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FetchId {
    /// Incremented for every load started: results of earlier ones are dropped.
    generation: u64,
    /// Which of the load's ranges the page is in (0 if it has just one).
    range: usize,
}

/// A fetch for `App` to start.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fetch {
    pub kind: LoadKind,
    pub page: TransactionPage,
    /// Pass back to `page_loaded()` or `load_failed()` with the result.
    pub id: FetchId,
}

/// What a loaded page did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageOutcome {
    /// The list was replaced (the initial load, or a refresh that's complete).
    Replaced,
    /// Older transactions were added to the end of the list.
    Appended,
    /// The page is kept until the rest of its load is in.
    Staged,
}

/// A load in progress.
#[derive(Debug)]
struct Load {
    kind: LoadKind,
    step: Step,
}

#[derive(Debug)]
enum Step {
    /// The newest page: the initial load, or the start of a refresh.
    Newest,
    /// A page of transactions older than the loaded ones, for the list.
    Older,
    /// The rest of the recent period, in ranges loaded in parallel.
    Ranges {
        /// A refresh's newest page, which the ranges are added to, replacing the list;
        /// `None` to add them to the loaded transactions.
        base: Option<Vec<Transaction>>,
        /// Newest first.
        ranges: Vec<Range>,
        /// The oldest range's start: once they're all in, every transaction created
        /// after this is loaded.
        since: Timestamp,
    },
}

/// One of the ranges a period is loaded in, and its pages so far.
#[derive(Debug)]
struct Range {
    transactions: Vec<Transaction>,
    next: RangeState,
}

#[derive(Debug)]
enum RangeState {
    /// This page is to be fetched next.
    Pending(TransactionPage),
    /// A page is being fetched.
    InFlight,
    /// All its pages are in.
    Done,
}

#[derive(Debug, Default)]
pub struct TransactionStore {
    /// Newest first. `None` until the first page has loaded.
    transactions: Option<Vec<Transaction>>,
    /// Every transaction created after this is in `transactions`; `None` if they all
    /// are.
    complete_after: Option<Timestamp>,
    /// The next page of transactions older than the loaded ones, if there may be any.
    older: Option<TransactionPage>,
    /// The start of the recent period that `recent()` covers, as of the last
    /// `next_fetches()` or `refresh()`.
    period_start: Option<Timestamp>,
    /// The load in progress, if any. There is at most one, so its pages are never
    /// older than the transactions already loaded.
    load: Option<Load>,
    /// Incremented for every load started. Results carrying an older value were
    /// superseded and are dropped.
    generation: u64,
}

impl TransactionStore {
    /// The loaded transactions, newest first, or `None` if none have loaded yet.
    pub fn transactions(&self) -> Option<&[Transaction]> {
        self.transactions.as_deref()
    }

    /// The load in progress, if any.
    pub fn loading(&self) -> Option<LoadKind> {
        self.load.as_ref().map(|load| load.kind)
    }

    /// All the transactions since the start of the recent period, or `None` if they
    /// aren't all loaded (yet, or because loading failed).
    pub fn recent(&self) -> Option<&[Transaction]> {
        let since = self.period_start?;
        let transactions = self.transactions.as_deref()?;
        if !self.covers(since) {
            return None;
        }
        // Newest first, so the ones since `since` come first.
        let end = transactions.partition_point(|t| t.created_at >= since);
        Some(&transactions[..end])
    }

    /// Whether the loaded transactions include all of those since `since`.
    fn covers(&self, since: Timestamp) -> bool {
        self.complete_after.is_none_or(|after| after < since)
    }

    /// The fetches to start now, if any: the first page if none has loaded, the rest
    /// of the recent period since `period_start` if it isn't all loaded, or the next
    /// page if the list `wants_more`. While a load is in progress, only the next pages
    /// of its ranges, though `recent()` follows the new `period_start` straight away.
    ///
    /// It's called after every page loads, so that loading continues until nothing more
    /// is needed, and when the user does something that might need more (such as
    /// changing the period). It isn't called after a failure, so a failed fetch is
    /// retried then rather than straight away.
    pub fn next_fetches(&mut self, period_start: Timestamp, wants_more: bool) -> Vec<Fetch> {
        self.period_start = Some(period_start);
        if self.load.is_none() {
            if self.transactions.is_none() {
                return vec![self.start(LoadKind::Replace, Step::Newest, TransactionPage::NEWEST)];
            }
            if let Some(after) = self.complete_after.filter(|&after| after >= period_start) {
                self.start_ranges(LoadKind::Append, None, period_start, after);
            } else if let Some(older) = self.older.clone().filter(|_| wants_more) {
                return vec![self.start(LoadKind::Append, Step::Older, older)];
            }
        }
        self.pending_fetches()
    }

    /// Reload from the newest page, superseding any load in progress. The loaded
    /// transactions stay until the new pages cover the recent period, then are
    /// replaced (and any older pages dropped).
    pub fn refresh(&mut self, period_start: Timestamp) -> Fetch {
        self.period_start = Some(period_start);
        self.start(LoadKind::Replace, Step::Newest, TransactionPage::NEWEST)
    }

    /// Start a load of a single page: the newest one, or the one after the loaded
    /// transactions.
    fn start(&mut self, kind: LoadKind, step: Step, page: TransactionPage) -> Fetch {
        self.generation += 1;
        self.load = Some(Load { kind, step });
        Fetch {
            kind,
            page,
            id: FetchId {
                generation: self.generation,
                range: 0,
            },
        }
    }

    /// Start loading the transactions created from `from` to `to` (both included), in
    /// ranges of `RANGE`, adding them to `base` (or the loaded transactions).
    fn start_ranges(
        &mut self,
        kind: LoadKind,
        base: Option<Vec<Transaction>>,
        from: Timestamp,
        to: Timestamp,
    ) {
        let mut ranges = Vec::new();
        let mut until = to;
        loop {
            let since = shift(until, -RANGE).max(from);
            let page = TransactionPage::First {
                since: Some(shift(since, -OVERLAP)),
                until: Some(shift(until, OVERLAP)),
            };
            ranges.push(Range {
                transactions: Vec::new(),
                next: RangeState::Pending(page),
            });
            if since <= from {
                break;
            }
            until = since;
        }
        self.generation += 1;
        self.load = Some(Load {
            kind,
            step: Step::Ranges {
                base,
                ranges,
                since: shift(from, -OVERLAP),
            },
        });
    }

    /// The fetches of the load's ranges that are waiting to start, now marked as
    /// started.
    fn pending_fetches(&mut self) -> Vec<Fetch> {
        let generation = self.generation;
        let Some(Load {
            kind,
            step: Step::Ranges { ranges, .. },
        }) = &mut self.load
        else {
            return Vec::new();
        };
        let mut fetches = Vec::new();
        for (index, range) in ranges.iter_mut().enumerate() {
            if let RangeState::Pending(page) = &range.next {
                fetches.push(Fetch {
                    kind: *kind,
                    page: page.clone(),
                    id: FetchId {
                        generation,
                        range: index,
                    },
                });
                range.next = RangeState::InFlight;
            }
        }
        fetches
    }

    /// Add a page fetched by the fetch with this id. Returns `None` (and changes
    /// nothing) if that fetch was superseded.
    pub fn page_loaded(
        &mut self,
        id: FetchId,
        page: PaginatedResponse<Transaction>,
    ) -> Option<PageOutcome> {
        if id.generation != self.generation {
            return None;
        }
        let load = self.load.as_mut()?;
        let Step::Ranges { ranges, .. } = &mut load.step else {
            let load = self.load.take()?;
            return Some(match load.step {
                Step::Older => self.older_loaded(page),
                _ => self.newest_loaded(page),
            });
        };
        let range = ranges.get_mut(id.range)?;
        if !matches!(range.next, RangeState::InFlight) {
            return None;
        }
        range.transactions.extend(page.data);
        range.next = match page.next_page_url {
            Some(url) => RangeState::Pending(TransactionPage::Next(url)),
            None => RangeState::Done,
        };
        if !ranges.iter().all(|r| matches!(r.next, RangeState::Done)) {
            return Some(PageOutcome::Staged);
        }
        // All the ranges are in (this always matches: it's the load matched above).
        let Some(Load {
            step:
                Step::Ranges {
                    base,
                    ranges,
                    since,
                },
            ..
        }) = self.load.take()
        else {
            return None;
        };
        let loaded = ranges.into_iter().flat_map(|range| range.transactions);
        let (base, outcome) = match base {
            Some(base) => (base, PageOutcome::Replaced),
            None => (
                self.transactions.take().unwrap_or_default(),
                PageOutcome::Appended,
            ),
        };
        self.transactions = Some(merge(base, loaded));
        self.complete_after = Some(since);
        self.older = Some(TransactionPage::First {
            since: None,
            until: Some(shift(since, OVERLAP)),
        });
        Some(outcome)
    }

    /// Show the newest page, unless it's a refresh's and doesn't cover the recent
    /// period: then load the rest of the period, and replace the list once it's in.
    fn newest_loaded(&mut self, page: PaginatedResponse<Transaction>) -> PageOutcome {
        let complete_after = complete_after(&page);
        match (complete_after, self.period_start) {
            // A refresh, which leaves the list alone until it's complete.
            (Some(after), Some(since)) if after >= since && self.transactions.is_some() => {
                self.start_ranges(LoadKind::Replace, Some(page.data), since, after);
                PageOutcome::Staged
            }
            // With nothing to show yet, show the first page straight away; the rest
            // of the period is then loaded.
            _ => {
                self.transactions = Some(page.data);
                self.complete_after = complete_after;
                self.older = page.next_page_url.map(TransactionPage::Next);
                PageOutcome::Replaced
            }
        }
    }

    /// Add a page of older transactions to the end of the list.
    fn older_loaded(&mut self, page: PaginatedResponse<Transaction>) -> PageOutcome {
        self.complete_after = complete_after(&page);
        self.older = self
            .complete_after
            .and(page.next_page_url)
            .map(TransactionPage::Next);
        let loaded = self.transactions.take().unwrap_or_default();
        self.transactions = Some(merge(loaded, page.data));
        PageOutcome::Appended
    }

    /// Record that the fetch with this id failed. The loaded transactions stay, and
    /// the load it's part of (such as a refresh) is abandoned. Returns `false` (and
    /// changes nothing) if that load was superseded or has already failed.
    pub fn load_failed(&mut self, id: FetchId) -> bool {
        if id.generation != self.generation || self.load.is_none() {
            return false;
        }
        self.load = None;
        true
    }

    /// A store with these transactions loaded, and the recent period covering all of
    /// them.
    #[cfg(test)]
    pub fn loaded(transactions: Vec<Transaction>, next_page: Option<String>) -> Self {
        let page = PaginatedResponse {
            data: transactions,
            next_page_url: next_page,
        };
        TransactionStore {
            complete_after: complete_after(&page),
            older: page.next_page_url.map(TransactionPage::Next),
            transactions: Some(page.data),
            period_start: Some(Timestamp::MIN),
            ..Default::default()
        }
    }
}

/// After loading `page` (and the newer pages before it), every transaction created
/// after this is loaded: its oldest transaction's time, or `None` if it's the last
/// page (or an empty one).
fn complete_after(page: &PaginatedResponse<Transaction>) -> Option<Timestamp> {
    page.next_page_url.as_ref()?;
    page.data.last().map(|t| t.created_at)
}

/// `transactions` with those of `more` that aren't already there, newest first.
fn merge(
    mut transactions: Vec<Transaction>,
    more: impl IntoIterator<Item = Transaction>,
) -> Vec<Transaction> {
    let mut ids: HashSet<String> = transactions.iter().map(|t| t.id.clone()).collect();
    transactions.extend(more.into_iter().filter(|t| ids.insert(t.id.clone())));
    // Stable, so transactions from the same second stay in the API's order.
    transactions.sort_by_key(|t| std::cmp::Reverse(t.created_at));
    transactions
}

/// `t` moved by `duration`, stopping at the ends of time.
fn shift(t: Timestamp, duration: SignedDuration) -> Timestamp {
    // Only adding calendar units can fail, which a `SignedDuration` doesn't have.
    t.saturating_add(duration).unwrap_or(t)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::state::tests::txn;

    /// Serves pages of transactions, newest first, as the API might: ranges include
    /// their ends, or not.
    struct Api {
        /// Newest first.
        transactions: Vec<Transaction>,
        includes_ends: bool,
        page_size: usize,
    }

    impl Api {
        fn new(transactions: Vec<Transaction>, includes_ends: bool) -> Self {
            Api {
                transactions,
                includes_ends,
                page_size: 50,
            }
        }

        fn page(&self, page: &TransactionPage) -> PaginatedResponse<Transaction> {
            let (since, until, offset) = match page {
                TransactionPage::First { since, until } => (*since, *until, 0),
                TransactionPage::Next(url) => {
                    let parts: Vec<&str> = url.split('|').collect();
                    let bound = |s: &str| s.parse().ok();
                    (bound(parts[0]), bound(parts[1]), parts[2].parse().unwrap())
                }
            };
            let includes = |t: &Transaction| {
                let after = since.is_none_or(|since| {
                    t.created_at > since || (self.includes_ends && t.created_at == since)
                });
                let before = until.is_none_or(|until| {
                    t.created_at < until || (self.includes_ends && t.created_at == until)
                });
                after && before
            };
            let matching: Vec<_> = self.transactions.iter().filter(|t| includes(t)).collect();
            let end = (offset + self.page_size).min(matching.len());
            let bound = |t: Option<Timestamp>| t.map(|t| t.to_string()).unwrap_or_default();
            PaginatedResponse {
                data: matching[offset..end].iter().map(|&t| t.clone()).collect(),
                next_page_url: (end < matching.len())
                    .then(|| format!("{}|{}|{end}", bound(since), bound(until))),
            }
        }

        /// Run the store's fetches until it needs no more, answering the parallel ones
        /// in reverse order. Returns the outcomes, and the most fetches in progress at
        /// once.
        fn load(
            &self,
            store: &mut TransactionStore,
            since: Timestamp,
        ) -> (Vec<PageOutcome>, usize) {
            let mut outcomes = Vec::new();
            let mut in_progress = store.next_fetches(since, false);
            let mut most = in_progress.len();
            while let Some(fetch) = in_progress.pop() {
                outcomes.extend(store.page_loaded(fetch.id, self.page(&fetch.page)));
                in_progress.extend(store.next_fetches(since, false));
                most = most.max(in_progress.len());
            }
            (outcomes, most)
        }
    }

    /// A transaction every six hours, back from 2026-11-26, for `days`, with two at the
    /// same time now and then.
    fn transactions(days: i64) -> Vec<Transaction> {
        let newest: Timestamp = "2026-11-26T06:00:00Z".parse().unwrap();
        let mut transactions = Vec::new();
        for i in 0..days * 4 {
            let at = newest - SignedDuration::from_hours(6 * i);
            transactions.push(txn(&format!("t{i}"), &at.to_string()));
            if i % 7 == 0 {
                transactions.push(txn(&format!("t{i}b"), &at.to_string()));
            }
        }
        transactions
    }

    fn ids(transactions: Option<&[Transaction]>) -> Vec<&str> {
        transactions
            .unwrap_or_default()
            .iter()
            .map(|t| t.id.as_str())
            .collect()
    }

    fn timestamp(s: &str) -> Timestamp {
        s.parse().unwrap()
    }

    #[test]
    fn a_long_period_loads_in_parallel_ranges_whatever_the_api_does_with_their_ends() {
        let since = timestamp("2026-01-01T00:00:00Z");
        for includes_ends in [true, false] {
            let api = Api::new(transactions(500), includes_ends);
            let mut store = TransactionStore::default();

            // The newest page shows at once.
            let first = store.next_fetches(since, false);
            assert_eq!(first.len(), 1);
            assert_eq!(first[0].page, TransactionPage::NEWEST);
            assert_eq!(store.next_fetches(since, false), [], "one load at a time");
            let outcome = store.page_loaded(first[0].id, api.page(&first[0].page));
            assert_eq!(outcome, Some(PageOutcome::Replaced));
            assert_eq!(store.transactions().unwrap().len(), 50);
            assert!(store.recent().is_none(), "the period isn't all loaded yet");

            // The rest of the period loads in ranges of a month, all at once, and is
            // added when they're all in.
            let (outcomes, most) = api.load(&mut store, since);
            assert!(most >= 10, "only {most} fetches at once");
            assert_eq!(outcomes.last(), Some(&PageOutcome::Appended));
            assert!(
                outcomes[..outcomes.len() - 1]
                    .iter()
                    .all(|&o| o == PageOutcome::Staged)
            );
            let expected: Vec<_> = api
                .transactions
                .iter()
                .filter(|t| t.created_at >= since)
                .map(|t| t.id.as_str())
                .collect();
            assert_eq!(
                ids(store.recent()),
                expected,
                "includes ends: {includes_ends}"
            );
            let all = ids(store.transactions());
            assert_eq!(
                all.iter().collect::<HashSet<_>>().len(),
                all.len(),
                "duplicates"
            );
            assert_eq!(store.loading(), None);
        }
    }

    #[test]
    fn older_transactions_load_a_page_at_a_time_when_the_list_wants_them() {
        let since = timestamp("2026-10-01T00:00:00Z");
        let api = Api::new(transactions(150), false);
        let mut store = TransactionStore::default();
        api.load(&mut store, since);
        let loaded = store.transactions().unwrap().len();
        assert_eq!(store.next_fetches(since, false), []);

        let older = store.next_fetches(since, true);
        assert_eq!(older.len(), 1);
        assert_eq!(older[0].kind, LoadKind::Append);
        let outcome = store.page_loaded(older[0].id, api.page(&older[0].page));
        assert_eq!(outcome, Some(PageOutcome::Appended));
        // Without the ones at the end of the period again.
        let all = ids(store.transactions());
        assert_eq!(all, ids(Some(&api.transactions[..all.len()])));
        assert!(all.len() > loaded);

        // Until there are no more.
        while let [fetch] = store.next_fetches(since, true).as_slice() {
            store.page_loaded(fetch.id, api.page(&fetch.page));
        }
        assert_eq!(ids(store.transactions()), ids(Some(&api.transactions)));
    }

    #[test]
    fn a_period_within_the_newest_page_takes_one_fetch() {
        let since = timestamp("2026-11-20T00:00:00Z");
        let api = Api::new(transactions(100), false);
        let mut store = TransactionStore::default();
        let (outcomes, _) = api.load(&mut store, since);
        assert_eq!(outcomes, [PageOutcome::Replaced]);
        let expected = api.transactions.iter().filter(|t| t.created_at >= since);
        assert_eq!(store.recent().unwrap().len(), expected.count());
    }

    #[test]
    fn everything_is_loaded_when_there_are_no_more_pages() {
        let mut store = TransactionStore::default();
        let api = Api::new(vec![txn("a", "2026-11-20T10:00:00Z")], false);
        let since = timestamp("2026-01-01T00:00:00Z");
        api.load(&mut store, since);
        assert_eq!(ids(store.recent()), ["a"]);
        assert_eq!(store.next_fetches(since, true), []);
    }

    #[test]
    fn a_refresh_replaces_the_list_once_it_covers_the_period() {
        let since = timestamp("2026-09-01T00:00:00Z");
        let mut api = Api::new(transactions(150), true);
        let mut store = TransactionStore::default();
        api.load(&mut store, since);
        // The list goes back further than the period.
        let older = store.next_fetches(since, true).remove(0);
        store.page_loaded(older.id, api.page(&older.page));
        let before = ids(store.transactions()).len();

        api.transactions
            .insert(0, txn("new", "2026-11-26T07:00:00Z"));
        let refresh = store.refresh(since);
        assert_eq!(refresh.page, TransactionPage::NEWEST);
        let outcome = store.page_loaded(refresh.id, api.page(&refresh.page));
        assert_eq!(outcome, Some(PageOutcome::Staged));
        // Until the refresh is complete, the old transactions stay.
        assert_eq!(store.loading(), Some(LoadKind::Replace));
        assert_eq!(store.transactions().unwrap().len(), before);
        assert!(store.recent().is_some());

        // The rest of the period loads in parallel ranges too, then replaces the list.
        let (outcomes, most) = api.load(&mut store, since);
        assert!(most > 1);
        assert_eq!(outcomes.last(), Some(&PageOutcome::Replaced));
        assert!(
            outcomes[..outcomes.len() - 1]
                .iter()
                .all(|&o| o == PageOutcome::Staged)
        );
        // The period's transactions, with the new one; the older pages are dropped.
        let expected: Vec<_> = api
            .transactions
            .iter()
            .filter(|t| t.created_at >= since)
            .map(|t| t.id.as_str())
            .collect();
        assert_eq!(ids(store.recent()), expected);
        assert_eq!(ids(store.transactions()), expected);
    }

    #[test]
    fn superseded_results_are_ignored() {
        let mut store = TransactionStore::loaded(
            vec![txn("a", "2026-11-20T10:00:00Z")],
            Some("p2".to_string()),
        );
        let since = timestamp("2026-11-01T00:00:00Z");
        let ranges = store.next_fetches(since, true);
        let refresh = store.refresh(since);
        let late = PaginatedResponse {
            data: vec![txn("b", "2026-11-19T10:00:00Z")],
            next_page_url: None,
        };
        assert_eq!(store.page_loaded(ranges[0].id, late), None);
        assert!(!store.load_failed(ranges[0].id));
        assert_eq!(store.loading(), Some(LoadKind::Replace));
        assert_eq!(ids(store.transactions()), ["a"]);
        assert!(store.load_failed(refresh.id));
        assert_eq!(store.loading(), None);
    }

    #[test]
    fn a_failed_range_abandons_the_load_and_keeps_the_loaded_transactions() {
        let since = timestamp("2026-01-01T00:00:00Z");
        let api = Api::new(transactions(400), false);
        let mut store = TransactionStore::default();
        let first = store.next_fetches(since, false).remove(0);
        store.page_loaded(first.id, api.page(&first.page));
        let shown = ids(store.transactions()).len();

        let ranges = store.next_fetches(since, false);
        store.page_loaded(ranges[1].id, api.page(&ranges[1].page));
        assert!(store.load_failed(ranges[0].id));
        // Failing again, or loading, in the abandoned load changes nothing.
        assert!(!store.load_failed(ranges[2].id));
        assert_eq!(
            store.page_loaded(ranges[3].id, api.page(&ranges[3].page)),
            None
        );
        assert_eq!(ids(store.transactions()).len(), shown);
        assert!(store.recent().is_none());

        // Trying again starts the ranges over.
        let again = store.next_fetches(since, false);
        assert_eq!(again.len(), ranges.len());
        assert!(
            again
                .iter()
                .all(|f| f.id.generation > ranges[0].id.generation)
        );
    }

    #[test]
    fn a_failed_first_load_is_retried_from_the_start() {
        let mut store = TransactionStore::default();
        let since = timestamp("2026-11-01T00:00:00Z");
        let first = store.next_fetches(since, false).remove(0);
        assert!(store.load_failed(first.id));
        assert!(store.transactions().is_none());
        assert!(store.recent().is_none());
        let again = store.next_fetches(since, false);
        assert_eq!(again.len(), 1);
        assert_eq!(again[0].page, TransactionPage::NEWEST);
    }
}
