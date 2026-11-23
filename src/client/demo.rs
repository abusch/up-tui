//! Generated accounts and transactions for demo mode (`up --demo`): for trying the app,
//! testing it by hand, and taking screenshots, without an Up account.
//!
//! The data is made up but realistic: a few months of purchases, pay, bills, transfers
//! between the accounts and interest, with balances that add up. It's generated from a
//! fixed seed, relative to the current date, so it's the same on every run on a given
//! day. It covers what the UI can show: pending and settled transactions, holds that
//! differ from the final amount, foreign amounts, round-ups, cashback, notes, tags,
//! refunds, purchases on a 2Up account by either partner, and account names with emoji
//! (with and without U+FE0F).

use std::collections::HashMap;
use std::time::Duration;

use jiff::{
    Timestamp, ToSpan, ZonedDateTime,
    civil::{Date, Weekday},
    tz::TimeZone,
};
use rootcause::{Result, bail};

use super::models::{
    Account, CardPurchaseMethod, Cashback, Category, Customer, HoldInfo, MoneyObject, Note,
    PaginatedResponse, PaginationOptions, RoundUp, Transaction, TransactionStatus,
};

/// How many days back the transactions go (including today).
const DAYS: i32 = 120;
/// Simulated network latency for every request, so that loading states show.
const LATENCY: Duration = Duration::from_millis(300);
const SEED: u64 = 0x00c0_ffee;

const SPENDING: &str = "demo-spending";
const TWO_UP: &str = "demo-2up";
const HOLIDAY: &str = "demo-holiday";
const RAINY_DAY: &str = "demo-rainy-day";
const HOME_LOAN: &str = "demo-home-loan";

struct AccountSpec {
    id: &'static str,
    name: &'static str,
    account_type: &'static str,
    ownership_type: &'static str,
    /// The balance before the first generated transaction, in cents.
    opening_balance: i64,
}

const ACCOUNTS: &[AccountSpec] = &[
    AccountSpec {
        id: SPENDING,
        name: "Spending",
        account_type: "TRANSACTIONAL",
        ownership_type: "INDIVIDUAL",
        opening_balance: 185_000,
    },
    AccountSpec {
        id: TWO_UP,
        name: "2Up Spending",
        account_type: "TRANSACTIONAL",
        ownership_type: "JOINT",
        opening_balance: 64_000,
    },
    AccountSpec {
        id: HOLIDAY,
        // With U+FE0F: the emoji ratatui mishandles (see `backend.rs`).
        name: "🏖\u{FE0F} Holiday",
        account_type: "SAVER",
        ownership_type: "INDIVIDUAL",
        opening_balance: 210_000,
    },
    AccountSpec {
        id: RAINY_DAY,
        name: "🛟 Rainy day",
        account_type: "SAVER",
        ownership_type: "INDIVIDUAL",
        opening_balance: 800_000,
    },
    AccountSpec {
        id: HOME_LOAN,
        name: "Home Loan",
        account_type: "HOME_LOAN",
        ownership_type: "JOINT",
        opening_balance: -51_234_018,
    },
];

/// Up's categories that the demo uses: (id, name, parent id). Parents have no parent.
const CATEGORIES: &[(&str, &str, Option<&str>)] = &[
    ("good-life", "Good Life", None),
    ("home", "Home", None),
    ("personal", "Personal", None),
    ("transport", "Transport", None),
    (
        "restaurants-and-cafes",
        "Restaurants & Cafes",
        Some("good-life"),
    ),
    ("takeaway", "Takeaway", Some("good-life")),
    ("pubs-and-bars", "Pubs & Bars", Some("good-life")),
    ("tv-and-music", "TV, Music & Streaming", Some("good-life")),
    ("games-and-software", "Games & Software", Some("good-life")),
    (
        "holidays-and-travel",
        "Holidays & Travel",
        Some("good-life"),
    ),
    ("groceries", "Groceries", Some("home")),
    ("utilities", "Utilities", Some("home")),
    ("internet", "Internet", Some("home")),
    ("health-and-medical", "Health & Medical", Some("personal")),
    (
        "clothing-and-accessories",
        "Clothing & Accessories",
        Some("personal"),
    ),
    (
        "fitness-and-wellbeing",
        "Fitness & Wellbeing",
        Some("personal"),
    ),
    ("mobile-phone", "Mobile Phone", Some("personal")),
    ("public-transport", "Public Transport", Some("transport")),
    ("fuel", "Fuel", Some("transport")),
    (
        "taxis-and-share-cars",
        "Taxis & Share Cars",
        Some("transport"),
    ),
];

struct Merchant {
    name: &'static str,
    category: &'static str,
    /// The range of amounts spent, in cents.
    cents: (i64, i64),
    /// Bought online (otherwise in person, with a card).
    online: bool,
}

const fn shop(name: &'static str, category: &'static str, cents: (i64, i64)) -> Merchant {
    Merchant {
        name,
        category,
        cents,
        online: false,
    }
}

const fn online(name: &'static str, category: &'static str, cents: (i64, i64)) -> Merchant {
    Merchant {
        name,
        category,
        cents,
        online: true,
    }
}

const TRANSIT: Merchant = shop("Metro Transit", "public-transport", (450, 1080));
const CAFES: &[Merchant] = &[
    shop("Harbour Coffee", "restaurants-and-cafes", (450, 680)),
    shop("Little Owl Espresso", "restaurants-and-cafes", (480, 720)),
    shop("The Daily Grind", "restaurants-and-cafes", (420, 650)),
];
const LUNCH: &[Merchant] = &[
    shop("Little Bao", "takeaway", (1290, 2100)),
    shop("Sushi Train Co", "restaurants-and-cafes", (1450, 2400)),
    shop("Green Bowl", "takeaway", (1390, 1890)),
];
const DINNER: &[Merchant] = &[
    shop("Pho Real", "takeaway", (2400, 3800)),
    online("Pizza Fritta", "takeaway", (2800, 5200)),
    shop("Burger Lab", "takeaway", (2600, 4400)),
];
const BARS: &[Merchant] = &[
    shop("The Local Tap", "pubs-and-bars", (1200, 6800)),
    shop("Rooftop Social", "pubs-and-bars", (1800, 7400)),
];
const CORNER_SHOPS: &[Merchant] = &[
    shop("Metro Fresh", "groceries", (1500, 6500)),
    shop("Corner Market", "groceries", (900, 3200)),
];
const FUEL: Merchant = shop("Southern Cross Fuel", "fuel", (5200, 9400));
const RIDES: Merchant = online("Ryde Share", "taxis-and-share-cars", (1800, 4200));
const CLOTHES: &[Merchant] = &[
    shop("Threadbare", "clothing-and-accessories", (3500, 12000)),
    online("Outfield Supply", "clothing-and-accessories", (4999, 15000)),
];
const PHARMACY: Merchant = shop("Wellness Pharmacy", "health-and-medical", (900, 4500));
const BIG_SHOP: Merchant = shop("Metro Fresh", "groceries", (9000, 22000));
const RESTAURANTS: &[Merchant] = &[
    shop("Osteria Lume", "restaurants-and-cafes", (6000, 14000)),
    shop("Spice Route", "restaurants-and-cafes", (5400, 11000)),
];
const SUBURBS: &[&str] = &["SURRY HILLS", "NEWTOWN", "SYDNEY", "MARRICKVILLE"];

/// A demo bank: the generated data, served like the API serves the real data.
pub struct DemoBank {
    accounts: Vec<Account>,
    /// Each account's transactions, newest first, like the API returns them.
    transactions: HashMap<String, Vec<Transaction>>,
    categories: Vec<Category>,
}

impl DemoBank {
    /// Generate the data, with transactions up to `now` (in its time zone).
    pub fn new(now: &ZonedDateTime) -> Self {
        Generator::new(now).run()
    }

    pub async fn get_categories(&self) -> Result<Vec<Category>> {
        tokio::time::sleep(LATENCY).await;
        Ok(self.categories.clone())
    }

    pub async fn get_accounts(&self) -> Result<PaginatedResponse<Account>> {
        tokio::time::sleep(LATENCY).await;
        Ok(PaginatedResponse {
            data: self.accounts.clone(),
            next_page_url: None,
        })
    }

    pub async fn get_transactions(
        &self,
        account_id: &str,
        opts: PaginationOptions,
    ) -> Result<PaginatedResponse<Transaction>> {
        tokio::time::sleep(LATENCY).await;
        self.transactions_page(account_id, opts)
    }

    /// A page of an account's transactions: the first one, or the one at
    /// `opts.next_url`, which is a URL returned with a previous page.
    fn transactions_page(
        &self,
        account_id: &str,
        opts: PaginationOptions,
    ) -> Result<PaginatedResponse<Transaction>> {
        let Some(all) = self.transactions.get(account_id) else {
            bail!("No such account: {account_id}");
        };
        let start = match &opts.next_url {
            None => 0,
            Some(url) => match url.rsplit_once("offset=").map(|(_, n)| n.parse()) {
                Some(Ok::<usize, _>(offset)) => offset,
                _ => bail!("Invalid page URL: {url}"),
            },
        };
        let page_size = usize::try_from(opts.page_size).unwrap_or(usize::MAX).max(1);
        let end = start.saturating_add(page_size).min(all.len());
        let data = all.get(start..end).unwrap_or_default().to_vec();
        let next_page_url = (end < all.len())
            .then(|| format!("demo://accounts/{account_id}/transactions?offset={end}"));
        Ok(PaginatedResponse {
            data,
            next_page_url,
        })
    }
}

/// An amount in Australian dollars, from cents.
fn aud(cents: i64) -> MoneyObject {
    let sign = if cents < 0 { "-" } else { "" };
    let abs = cents.unsigned_abs();
    MoneyObject {
        currency_code: "AUD".to_string(),
        value: format!("{sign}{}.{:02}", abs / 100, abs % 100),
        value_in_base_units: cents,
    }
}

fn account_name(id: &str) -> &'static str {
    ACCOUNTS
        .iter()
        .find(|a| a.id == id)
        .map_or("Unknown", |a| a.name)
}

fn parent_category(id: &str) -> Option<&'static str> {
    CATEGORIES.iter().find(|c| c.0 == id).and_then(|c| c.2)
}

/// A small deterministic random number generator (SplitMix64), so the data is the same
/// on every run.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    /// A number in `low..=high`.
    fn range(&mut self, low: i64, high: i64) -> i64 {
        let span = high.abs_diff(low).saturating_add(1);
        low.saturating_add_unsigned(self.next() % span)
    }

    /// True `percent` percent of the time.
    fn chance(&mut self, percent: u64) -> bool {
        self.next() % 100 < percent
    }

    fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        let len = u64::try_from(items.len()).unwrap_or(u64::MAX);
        &items[usize::try_from(self.next() % len).unwrap_or(0)]
    }
}

struct Generator {
    rng: Rng,
    tz: TimeZone,
    now: Timestamp,
    today: Date,
    next_id: u64,
    transactions: HashMap<&'static str, Vec<Transaction>>,
}

impl Generator {
    fn new(now: &ZonedDateTime) -> Self {
        Generator {
            rng: Rng(SEED),
            tz: now.time_zone().clone(),
            now: now.timestamp(),
            today: now.date(),
            next_id: 0,
            transactions: HashMap::new(),
        }
    }

    fn run(mut self) -> DemoBank {
        for days_ago in (0..DAYS).rev() {
            let Ok(date) = self.today.checked_sub(days_ago.days()) else {
                continue;
            };
            self.day(date);
        }
        self.one_offs();

        let accounts = ACCOUNTS
            .iter()
            .map(|spec| {
                let txns = self.transactions.get(spec.id).map_or(&[][..], |t| t);
                let balance = txns.iter().fold(spec.opening_balance, |sum, t| {
                    sum + t.amount.value_in_base_units
                });
                Account {
                    id: spec.id.to_string(),
                    display_name: spec.name.to_string(),
                    account_type: spec.account_type.to_string(),
                    ownership_type: spec.ownership_type.to_string(),
                    balance: aud(balance),
                }
            })
            .collect();
        let transactions = self
            .transactions
            .into_iter()
            .map(|(id, mut txns)| {
                txns.sort_by(|a, b| b.created_at.cmp(&a.created_at).then(b.id.cmp(&a.id)));
                (id.to_string(), txns)
            })
            .collect();
        let categories = CATEGORIES
            .iter()
            .map(|&(id, name, parent)| Category {
                id: id.to_string(),
                name: name.to_string(),
                parent: parent.map(str::to_string),
            })
            .collect();
        DemoBank {
            accounts,
            transactions,
            categories,
        }
    }

    /// The regular transactions on one day.
    fn day(&mut self, date: Date) {
        let weekday = date.weekday();
        let workday = !matches!(weekday, Weekday::Saturday | Weekday::Sunday);

        // Pay every second Thursday (1970-01-01 was one), shared with the 2Up account
        // and the savers.
        let payday = date
            .since(jiff::civil::date(1970, 1, 1))
            .is_ok_and(|span| span.get_days().rem_euclid(14) == 0);
        if payday {
            if let Some(at) = self.time(date, 3, 12) {
                let mut salary = self.txn("Bluegum Analytics", 312_560, at);
                salary.raw_text = Some("BLUEGUM ANALYTI SALARY".to_string());
                salary.message = Some("Salary".to_string());
                salary.transaction_type = Some("Salary".to_string());
                self.add(SPENDING, salary);
            }
            if let Some(at) = self.time(date, 7, 0) {
                self.transfer(SPENDING, HOLIDAY, 50_000, at);
                self.transfer(SPENDING, RAINY_DAY, 25_000, at);
                self.transfer(SPENDING, TWO_UP, 120_000, at);
            }
            if let Some(at) = self.time(date, 9, 31) {
                let mut partner = self.txn("Sam Rivera", 120_000, at);
                partner.message = Some("House stuff".to_string());
                partner.transaction_type = Some("Transfer".to_string());
                self.add(TWO_UP, partner);
            }
        }

        // Monthly: interest, the home loan repayment, bills and subscriptions.
        match date.day() {
            1 => {
                if let Some(at) = self.time(date, 0, 30) {
                    let interest = -self.rng.range(252_000, 258_000);
                    self.interest(HOME_LOAN, interest, at);
                    let interest = self.rng.range(800, 1100);
                    self.interest(HOLIDAY, interest, at);
                    let interest = self.rng.range(2800, 3200);
                    self.interest(RAINY_DAY, interest, at);
                }
                if let Some(at) = self.time(date, 9, 0) {
                    self.transfer(TWO_UP, HOME_LOAN, 330_000, at);
                }
            }
            3 => self.subscription(SPENDING, "Streamly", "tv-and-music", 1799, date),
            5 => self.subscription(
                SPENDING,
                "Peak Fitness",
                "fitness-and-wellbeing",
                2495,
                date,
            ),
            8 => self.bill(TWO_UP, "Swift Fibre", "internet", 8900, date),
            12 => self.subscription(SPENDING, "Tunewave", "tv-and-music", 1299, date),
            18 => self.subscription(SPENDING, "Spark Mobile", "mobile-phone", 3500, date),
            22 => {
                let amount = self.rng.range(14_000, 21_000);
                self.bill(TWO_UP, "Gridline Energy", "utilities", amount, date);
            }
            _ => {}
        }

        // Everyday spending.
        if workday {
            if self.rng.chance(70) {
                self.buy(SPENDING, &TRANSIT, date, (7, 8));
            }
            if self.rng.chance(80) {
                let cafe = self.rng.pick(CAFES);
                self.buy(SPENDING, cafe, date, (8, 9));
            }
            if self.rng.chance(60) {
                let lunch = self.rng.pick(LUNCH);
                self.buy(SPENDING, lunch, date, (12, 13));
            }
            if self.rng.chance(70) {
                self.buy(SPENDING, &TRANSIT, date, (17, 18));
            }
            if self.rng.chance(35) {
                let shop = self.rng.pick(CORNER_SHOPS);
                self.buy(SPENDING, shop, date, (17, 19));
            }
        } else if self.rng.chance(60) {
            let cafe = self.rng.pick(CAFES);
            self.buy(SPENDING, cafe, date, (9, 11));
        }
        if matches!(weekday, Weekday::Friday | Weekday::Saturday) && self.rng.chance(50) {
            let bar = self.rng.pick(BARS);
            self.buy(SPENDING, bar, date, (19, 22));
        }
        if self.rng.chance(25) {
            let dinner = self.rng.pick(DINNER);
            self.buy(SPENDING, dinner, date, (18, 20));
        }
        if self.rng.chance(10) {
            self.buy(SPENDING, &FUEL, date, (7, 18));
        }
        if self.rng.chance(5) {
            self.buy(SPENDING, &RIDES, date, (10, 23));
        }
        if self.rng.chance(4) {
            let clothes = self.rng.pick(CLOTHES);
            self.buy(SPENDING, clothes, date, (10, 20));
        }
        if self.rng.chance(3) {
            self.buy(SPENDING, &PHARMACY, date, (9, 18));
        }

        // Shared spending on the 2Up account.
        if weekday == Weekday::Saturday || (weekday == Weekday::Wednesday && self.rng.chance(50)) {
            self.buy(TWO_UP, &BIG_SHOP, date, (9, 17));
        }
        if !workday && self.rng.chance(40) {
            let restaurant = self.rng.pick(RESTAURANTS);
            self.buy(TWO_UP, restaurant, date, (18, 21));
        }
    }

    /// Transactions that happen once, to show what the detail pane can show.
    fn one_offs(&mut self) {
        // A birthday dinner, with a note and a tag.
        if let Some(at) = self.days_ago(3, 19, 40) {
            let mut dinner = self.purchase(SPENDING, &RESTAURANTS[0], at);
            dinner.amount = aud(-18_650);
            dinner.note = Some(Note {
                text: "Birthday dinner for Sam 🎂".to_string(),
            });
            dinner.tags = vec!["Birthdays".to_string()];
            self.add(SPENDING, dinner);
        }
        // A purchase in a foreign currency.
        if let Some(at) = self.days_ago(5, 21, 15) {
            let games = online("Pixel Forge Games", "games-and-software", (3087, 3087));
            let mut purchase = self.purchase(SPENDING, &games, at);
            purchase.raw_text = Some("PIXELFORGE*GAMES SEATTLE WA".to_string());
            purchase.foreign_amount = Some(MoneyObject {
                currency_code: "USD".to_string(),
                value: "-19.99".to_string(),
                value_in_base_units: -1999,
            });
            self.add(SPENDING, purchase);
        }
        // A refund (which isn't subtracted from spending).
        if let Some(at) = self.days_ago(9, 11, 5) {
            let mut refund = self.txn("Outfield Supply", 4999, at);
            refund.transaction_type = Some("Refund".to_string());
            self.categorise(&mut refund, "clothing-and-accessories");
            self.add(SPENDING, refund);
        }
        // Cashback.
        if let Some(at) = self.days_ago(12, 8, 47) {
            let mut coffee = self.purchase(SPENDING, &CAFES[0], at);
            coffee.amount = aud(-620);
            coffee.cashback = Some(Cashback {
                description: "Coffee on us".to_string(),
                amount: aud(620),
            });
            self.add(SPENDING, coffee);
        }
        // A round-up with a boost.
        if let Some(at) = self.days_ago(18, 13, 20) {
            let mut lunch = self.purchase(SPENDING, &LUNCH[2], at);
            lunch.amount = aud(-1745);
            lunch.round_up = Some(RoundUp {
                amount: aud(-255),
                boost_portion: Some(aud(-200)),
            });
            self.add(SPENDING, lunch);
        }
        // Flights, paid for from the holiday saver.
        if let Some(at) = self.days_ago(20, 20, 10) {
            self.transfer(HOLIDAY, SPENDING, 150_000, at);
        }
        if let Some(at) = self.days_ago(20, 20, 25) {
            let airline = online("Coastal Air", "holidays-and-travel", (142_400, 142_400));
            let mut flights = self.purchase(SPENDING, &airline, at);
            flights.tags = vec!["Holiday".to_string()];
            self.add(SPENDING, flights);
        }
    }

    /// A time on `date`, or `None` if it's in the future (or doesn't exist).
    fn time(&self, date: Date, hour: i8, minute: i8) -> Option<Timestamp> {
        let at = date
            .at(hour, minute, 0, 0)
            .to_zoned_datetime(self.tz.clone())
            .ok()?
            .timestamp();
        (at <= self.now).then_some(at)
    }

    fn days_ago(&self, days: i32, hour: i8, minute: i8) -> Option<Timestamp> {
        let date = self.today.checked_sub(days.days()).ok()?;
        self.time(date, hour, minute)
    }

    /// A random time on `date` between the start of the `hours.0` hour and the end of
    /// the `hours.1` hour.
    fn random_time(&mut self, date: Date, hours: (i8, i8)) -> Option<Timestamp> {
        let hour = self.rng.range(hours.0.into(), hours.1.into());
        let minute = self.rng.range(0, 59);
        self.time(date, i8::try_from(hour).ok()?, i8::try_from(minute).ok()?)
    }

    /// A settled transaction with only the basics filled in.
    fn txn(&mut self, description: &str, cents: i64, at: Timestamp) -> Transaction {
        self.next_id += 1;
        Transaction {
            id: format!("demo-{:05}", self.next_id),
            status: TransactionStatus::Settled,
            description: description.to_string(),
            raw_text: None,
            message: None,
            amount: aud(cents),
            foreign_amount: None,
            hold_info: None,
            card_purchase_method: None,
            settled_at: Some(at),
            created_at: at,
            round_up: None,
            cashback: None,
            transaction_type: None,
            note: None,
            performing_customer: None,
            category: None,
            parent_category: None,
            tags: Vec::new(),
            transfer_account: None,
        }
    }

    fn add(&mut self, account: &'static str, txn: Transaction) {
        self.transactions.entry(account).or_default().push(txn);
    }

    fn categorise(&self, txn: &mut Transaction, category: &str) {
        txn.category = Some(category.to_string());
        txn.parent_category = parent_category(category).map(str::to_owned);
    }

    /// A card purchase from `merchant` at `at`, held until it settles a few hours to a
    /// couple of days later.
    fn purchase(&mut self, account: &str, merchant: &Merchant, at: Timestamp) -> Transaction {
        let cents = -self.rng.range(merchant.cents.0, merchant.cents.1);
        let mut txn = self.txn(merchant.name, cents, at);
        txn.transaction_type = Some("Purchase".to_string());
        self.categorise(&mut txn, merchant.category);
        let (method, suffix) = match (merchant.online, account) {
            (true, _) => ("ECOMMERCE", None),
            (false, TWO_UP) => ("CONTACTLESS", Some("7783")),
            (false, _) if self.rng.chance(15) => ("CARD_PIN", Some("4021")),
            (false, _) => ("CONTACTLESS", Some("4021")),
        };
        txn.card_purchase_method = Some(CardPurchaseMethod {
            method: method.to_string(),
            card_number_suffix: suffix.map(str::to_owned),
        });
        let place = if merchant.online {
            "ONLINE"
        } else {
            *self.rng.pick(SUBURBS)
        };
        txn.raw_text = Some(format!("{} {place}", merchant.name.to_uppercase()));
        if account == TWO_UP {
            let partner = if self.rng.chance(50) { "Alex" } else { "Sam" };
            txn.performing_customer = Some(Customer {
                display_name: partner.to_string(),
            });
        }
        if account == SPENDING && cents % 100 != 0 && self.rng.chance(30) {
            txn.round_up = Some(RoundUp {
                amount: aud(-(100 + cents % 100)),
                boost_portion: None,
            });
        }

        // Fuel is held at a fixed amount, then settles for what was actually bought.
        let hold = if merchant.category == "fuel" {
            aud(-10_000)
        } else {
            txn.amount.clone()
        };
        let settles_after = self.rng.range(2, 60).hours();
        match at.checked_add(settles_after) {
            Ok(settled) if settled <= self.now => txn.settled_at = Some(settled),
            _ => {
                txn.status = TransactionStatus::Held;
                txn.settled_at = None;
                txn.amount = hold.clone();
                txn.round_up = None;
            }
        }
        txn.hold_info = Some(HoldInfo {
            amount: hold,
            foreign_amount: None,
        });
        txn
    }

    fn buy(&mut self, account: &'static str, merchant: &Merchant, date: Date, hours: (i8, i8)) {
        if let Some(at) = self.random_time(date, hours) {
            let txn = self.purchase(account, merchant, at);
            self.add(account, txn);
        }
    }

    fn subscription(
        &mut self,
        account: &'static str,
        name: &str,
        category: &'static str,
        cents: i64,
        date: Date,
    ) {
        let merchant = Merchant {
            name: "",
            category,
            cents: (cents, cents),
            online: true,
        };
        if let Some(at) = self.random_time(date, (4, 6)) {
            let mut txn = self.purchase(account, &merchant, at);
            txn.description = name.to_string();
            txn.raw_text = Some(format!("{} SUBSCRIPTION", name.to_uppercase()));
            if let Some(method) = &mut txn.card_purchase_method {
                method.method = "CARD_ON_FILE".to_string();
            }
            self.add(account, txn);
        }
    }

    fn bill(&mut self, account: &'static str, name: &str, category: &str, cents: i64, date: Date) {
        if let Some(at) = self.random_time(date, (10, 14)) {
            let mut txn = self.txn(name, -cents, at);
            txn.transaction_type = Some("BPAY".to_string());
            self.categorise(&mut txn, category);
            self.add(account, txn);
        }
    }

    fn interest(&mut self, account: &'static str, cents: i64, at: Timestamp) {
        let mut txn = self.txn("Interest", cents, at);
        txn.transaction_type = Some("Interest".to_string());
        self.add(account, txn);
    }

    /// A transfer between two of the accounts: a transaction on each.
    fn transfer(&mut self, from: &'static str, to: &'static str, cents: i64, at: Timestamp) {
        for (account, other, amount, description) in [
            (
                from,
                to,
                -cents,
                format!("Transfer to {}", account_name(to)),
            ),
            (
                to,
                from,
                cents,
                format!("Transfer from {}", account_name(from)),
            ),
        ] {
            let mut txn = self.txn(&description, amount, at);
            txn.transaction_type = Some("Transfer".to_string());
            txn.transfer_account = Some(other.to_string());
            self.add(account, txn);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use ratatui::{buffer::Buffer, layout::Rect, widgets::Widget};

    use super::*;
    use crate::app::{
        App,
        categories::Categories,
        state::{AppMode, View, recent_since},
    };
    use crate::client::Client;

    fn now() -> ZonedDateTime {
        "2026-11-23T15:00:00+11:00[Australia/Sydney]"
            .parse()
            .unwrap()
    }

    fn all_pages(bank: &DemoBank, account_id: &str, page_size: u32) -> Vec<Vec<Transaction>> {
        let mut pages = Vec::new();
        let mut next_url = None;
        loop {
            let opts = PaginationOptions {
                page_size,
                next_url,
            };
            let page = bank.transactions_page(account_id, opts).unwrap();
            pages.push(page.data);
            next_url = page.next_page_url;
            if next_url.is_none() {
                return pages;
            }
        }
    }

    #[test]
    fn transactions_are_newest_first_unique_and_not_in_the_future() {
        let bank = DemoBank::new(&now());
        let mut ids = HashSet::new();
        for account in &bank.accounts {
            let txns = &bank.transactions[&account.id];
            assert!(!txns.is_empty(), "{} has no transactions", account.id);
            assert!(txns.is_sorted_by(|a, b| a.created_at >= b.created_at));
            for txn in txns {
                assert!(ids.insert(txn.id.clone()), "duplicate id {}", txn.id);
                assert!(txn.created_at <= now().timestamp());
                assert!(txn.settled_at.is_none_or(|s| s <= now().timestamp()));
            }
        }
    }

    #[test]
    fn transfers_have_a_matching_transaction_on_the_other_account() {
        let bank = DemoBank::new(&now());
        for (account, txns) in &bank.transactions {
            for txn in txns {
                let Some(other) = &txn.transfer_account else {
                    continue;
                };
                let matching = bank.transactions[other].iter().any(|t| {
                    t.transfer_account.as_ref() == Some(account)
                        && t.created_at == txn.created_at
                        && t.amount.value_in_base_units == -txn.amount.value_in_base_units
                });
                assert!(matching, "no other side for {}", txn.id);
            }
        }
    }

    #[test]
    fn categories_exist_with_their_parents() {
        let bank = DemoBank::new(&now());
        let names: HashMap<_, _> = bank.categories.iter().map(|c| (c.id.as_str(), c)).collect();
        for txn in bank.transactions.values().flatten() {
            if let Some(category) = &txn.category {
                assert!(
                    names.contains_key(category.as_str()),
                    "unknown category {category}"
                );
                assert_eq!(txn.parent_category.as_deref(), parent_category(category));
            }
        }
    }

    #[test]
    fn pages_follow_on_until_the_last() {
        let bank = DemoBank::new(&now());
        let pages = all_pages(&bank, SPENDING, 100);
        assert!(pages.len() > 2);
        assert!(pages[..pages.len() - 1].iter().all(|p| p.len() == 100));
        let paged: Vec<_> = pages.iter().flatten().map(|t| &t.id).collect();
        let all: Vec<_> = bank.transactions[SPENDING].iter().map(|t| &t.id).collect();
        assert_eq!(paged, all);
        assert!(
            bank.transactions_page(
                SPENDING,
                PaginationOptions {
                    page_size: 100,
                    next_url: Some("nonsense".to_string()),
                }
            )
            .is_err()
        );
    }

    #[test]
    fn the_same_day_gives_the_same_data() {
        let (a, b) = (DemoBank::new(&now()), DemoBank::new(&now()));
        let summary = |bank: &DemoBank| -> Vec<(String, i64)> {
            bank.transactions[SPENDING]
                .iter()
                .map(|t| (t.description.clone(), t.amount.value_in_base_units))
                .collect()
        };
        assert_eq!(summary(&a), summary(&b));
    }

    #[test]
    fn the_spending_account_shows_every_kind_of_transaction() {
        let bank = DemoBank::new(&now());
        let txns = &bank.transactions[SPENDING];
        let any = |f: fn(&Transaction) -> bool| txns.iter().any(f);
        assert!(any(|t| matches!(t.status, TransactionStatus::Held)));
        assert!(any(|t| t
            .hold_info
            .as_ref()
            .is_some_and(|h| h.amount != t.amount)));
        assert!(any(|t| t.foreign_amount.is_some()));
        assert!(any(|t| t.round_up.is_some()));
        assert!(any(|t| t.cashback.is_some()));
        assert!(any(|t| t.note.is_some()));
        assert!(any(|t| !t.tags.is_empty()));
        // More than a page in the last 30 days (on any day), so the store fetches
        // several pages to cover them.
        for now in [
            "2026-01-15T08:00:00+11:00[Australia/Sydney]",
            "2026-05-31T23:00:00+10:00[Australia/Sydney]",
            "2026-08-09T12:00:00+10:00[Australia/Sydney]",
            "2026-11-23T15:00:00+11:00[Australia/Sydney]",
        ] {
            let now: ZonedDateTime = now.parse().unwrap();
            let bank = DemoBank::new(&now);
            let month_ago = now.timestamp().checked_sub(720.hours()).unwrap();
            let recent = bank.transactions[SPENDING]
                .iter()
                .filter(|t| t.created_at >= month_ago)
                .count();
            assert!(recent > 100, "only {recent} in the 30 days to {now}");
        }
    }

    /// An app showing the demo data as of `now`, loaded through each account's
    /// `TransactionStore` as the app loads it.
    fn demo_app(now: &ZonedDateTime) -> App {
        let bank = DemoBank::new(now);
        let accounts = bank.accounts.clone();
        let categories = bank.categories.clone();
        let mut app = App::new(Client::Demo(DemoBank::new(now)), None);
        let state = &mut app.state;
        state.tz = now.time_zone().clone();
        state.set_accounts(accounts);
        state.categories = Categories::new(categories);
        let since = recent_since(now.date(), &state.tz).unwrap();
        for tab in &mut state.tabs {
            while let Some(fetch) = tab.store.next_fetch(since, false) {
                let opts = PaginationOptions {
                    page_size: crate::app::store::PAGE_SIZE,
                    next_url: fetch.url,
                };
                let page = bank.transactions_page(&tab.account.id, opts).unwrap();
                tab.page_loaded(fetch.generation, page).unwrap();
            }
            assert!(tab.store.recent().is_some());
        }
        app
    }

    fn render(app: &mut App, width: u16, height: u16) -> String {
        let area = Rect::new(0, 0, width, height);
        let mut buf = Buffer::empty(area);
        app.render(area, &mut buf);
        (0..height)
            .map(|y| {
                let row: String = (0..width).map(|x| buf[(x, y)].symbol()).collect();
                row.trim_end().to_string()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn the_app_shows_the_demo_data_in_every_view() {
        let mut app = demo_app(&now());
        let mut screens = Vec::new();
        for tab in 0..app.state.tabs.len() {
            app.state.active_tab = tab;
            app.state.view = View::Transactions;
            app.state.show_detail = true;
            screens.push(render(&mut app, 120, 40));
            app.state.mode = AppMode::Detail;
            screens.push(render(&mut app, 120, 40));
            app.state.mode = AppMode::Normal;
            app.state.view = View::Insights;
            screens.push(render(&mut app, 120, 40));
        }
        let all = screens.join("\n");
        for expected in [
            "Spending",
            "2Up Spending",
            "Holiday",
            "Home Loan",
            "Groceries",
        ] {
            assert!(all.contains(expected), "{expected:?} isn't shown");
        }
        if let Some(path) = std::env::var_os("UP_DEMO_SCREENS") {
            std::fs::write(path, screens.join("\n\n")).unwrap();
        }
    }
}
