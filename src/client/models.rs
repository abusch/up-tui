use jiff::Timestamp;
use serde::Deserialize;

// --- JSON:API envelope types ---

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JsonApiResponse<T> {
    pub data: T,
    pub links: Option<PaginationLinks>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PaginationLinks {
    pub next: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Resource<A, R = serde_json::Value> {
    pub id: String,
    #[serde(rename = "type")]
    pub resource_type: String,
    pub attributes: A,
    #[serde(default)]
    pub relationships: R,
}

// --- JSON:API relationship types ---

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RelationshipData {
    pub id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToOneRelationship {
    pub data: Option<RelationshipData>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToManyRelationship {
    pub data: Vec<RelationshipData>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransactionRelationships {
    #[serde(default)]
    pub category: Option<ToOneRelationship>,
    pub parent_category: Option<ToOneRelationship>,
    #[serde(default)]
    pub tags: Option<ToManyRelationship>,
    /// The other account of a transfer between the user's own accounts.
    #[serde(default)]
    pub transfer_account: Option<ToOneRelationship>,
}

// --- Category types ---

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CategoryAttributes {
    pub name: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CategoryRelationships {
    #[serde(default)]
    pub parent: Option<ToOneRelationship>,
}

/// A transaction category. Categories form a hierarchy: a root category (e.g. "Good
/// Life") has no parent, and its children (e.g. "Restaurants & Cafes") have it as their
/// parent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Category {
    pub id: String,
    pub name: String,
    pub parent: Option<String>,
}

impl From<Resource<CategoryAttributes, CategoryRelationships>> for Category {
    fn from(r: Resource<CategoryAttributes, CategoryRelationships>) -> Self {
        Category {
            id: r.id,
            name: r.attributes.name,
            parent: r.relationships.parent.and_then(|p| p.data).map(|d| d.id),
        }
    }
}

// --- Account types ---

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountAttributes {
    pub display_name: String,
    pub account_type: AccountType,
    pub ownership_type: OwnershipType,
    pub balance: MoneyObject,
    pub created_at: Timestamp,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MoneyObject {
    pub currency_code: String,
    pub value: String,
    pub value_in_base_units: i64,
}

// The formatting methods take the sign from `value_in_base_units` and the digits from
// the API's `value` string (e.g. "10.56"), which is already formatted with the right
// number of decimal places for the currency. No floating point is involved.
impl MoneyObject {
    pub fn is_negative(&self) -> bool {
        self.value_in_base_units < 0
    }

    /// The amount without its sign, e.g. `"10.56"`.
    fn magnitude(&self) -> &str {
        self.value.strip_prefix('-').unwrap_or(&self.value)
    }

    /// A transaction amount in dollars: credits get a `+` prefix and debits have no
    /// sign, since the UI distinguishes them by colour (`+$10.56`, `$10.56`).
    pub fn format_display(&self) -> String {
        let sign = if self.is_negative() { "" } else { "+" };
        format!("{sign}${}", self.magnitude())
    }

    /// An amount in dollars without its sign, e.g. for a total that's known to be
    /// money out (`$10.56`).
    pub fn format_unsigned(&self) -> String {
        format!("${}", self.magnitude())
    }

    /// A signed amount in dollars, e.g. for balances (`$10.56`, `-$10.56`).
    pub fn format_signed(&self) -> String {
        let sign = if self.is_negative() { "-" } else { "" };
        format!("{sign}${}", self.magnitude())
    }

    /// The sum of `amounts`, or `None` if there are none, they aren't all in the same
    /// currency with the same number of decimal places, or the sum overflows.
    pub fn sum<'a>(amounts: impl IntoIterator<Item = &'a MoneyObject>) -> Option<MoneyObject> {
        let mut amounts = amounts.into_iter();
        let first = amounts.next()?;
        let decimals = first.decimals();
        let mut total = first.value_in_base_units;
        for amount in amounts {
            if amount.currency_code != first.currency_code || amount.decimals() != decimals {
                return None;
            }
            total = total.checked_add(amount.value_in_base_units)?;
        }
        first.with_base_units(total)
    }

    /// An amount of `base_units` (e.g. cents) in the same currency, and with the same
    /// number of decimal places. `None` if the number of decimal places is absurd.
    pub fn with_base_units(&self, base_units: i64) -> Option<MoneyObject> {
        let sign = if base_units < 0 { "-" } else { "" };
        let magnitude = base_units.unsigned_abs();
        let value = match self.decimals() {
            0 => format!("{sign}{magnitude}"),
            decimals => {
                let unit = 10u64.checked_pow(u32::try_from(decimals).ok()?)?;
                format!("{sign}{}.{:0decimals$}", magnitude / unit, magnitude % unit)
            }
        };
        Some(MoneyObject {
            currency_code: self.currency_code.clone(),
            value,
            value_in_base_units: base_units,
        })
    }

    /// Zero, in the same currency and with the same number of decimal places.
    pub fn zero_like(&self) -> MoneyObject {
        let value = match self.decimals() {
            0 => "0".to_string(),
            decimals => format!("0.{}", "0".repeat(decimals)),
        };
        MoneyObject {
            currency_code: self.currency_code.clone(),
            value,
            value_in_base_units: 0,
        }
    }

    /// The number of base units in one whole unit of the currency (100 for dollars,
    /// 1 for yen), from the number of decimal places in `value`.
    pub fn base_units_per_unit(&self) -> i64 {
        u32::try_from(self.decimals())
            .ok()
            .and_then(|decimals| 10i64.checked_pow(decimals))
            .unwrap_or(1)
    }

    /// Number of decimal places in the API's `value` string.
    fn decimals(&self) -> usize {
        self.value
            .split_once('.')
            .map_or(0, |(_, fraction)| fraction.len())
    }

    /// A foreign currency amount with its currency code, signed like
    /// `format_display` (`+10.56 USD` for credits, `10.56 USD` for debits).
    pub fn format_display_with_code(&self) -> String {
        let sign = if self.is_negative() { "" } else { "+" };
        format!("{sign}{} {}", self.magnitude(), self.currency_code)
    }
}

// --- Transaction types ---

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransactionAttributes {
    pub status: TransactionStatus,
    pub raw_text: Option<String>,
    pub description: String,
    pub message: Option<String>,
    pub amount: MoneyObject,
    pub foreign_amount: Option<MoneyObject>,
    pub card_purchase_method: Option<CardPurchaseMethod>,
    pub settled_at: Option<Timestamp>,
    pub created_at: Timestamp,
    pub round_up: Option<RoundUp>,
    pub cashback: Option<Cashback>,
    pub hold_info: Option<HoldInfo>,
    pub transaction_type: Option<String>,
    pub note: Option<Note>,
    pub performing_customer: Option<Customer>,
    /// A link that opens the transaction in Up's app. It's documented at
    /// <https://developer.up.com.au/#transactions> but missing from Up's OpenAPI spec,
    /// so it's optional, to be safe.
    #[serde(rename = "deepLinkURL", default)]
    pub deep_link_url: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoundUp {
    pub amount: MoneyObject,
    pub boost_portion: Option<MoneyObject>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Cashback {
    pub description: String,
    pub amount: MoneyObject,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CardPurchaseMethod {
    pub method: String,
    pub card_number_suffix: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HoldInfo {
    pub amount: MoneyObject,
    pub foreign_amount: Option<MoneyObject>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Note {
    pub text: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Customer {
    pub display_name: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TransactionStatus {
    Held,
    Settled,
}

impl std::fmt::Display for TransactionStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TransactionStatus::Held => write!(f, "Held"),
            TransactionStatus::Settled => write!(f, "Settled"),
        }
    }
}

// --- Pagination types ---

#[derive(Debug)]
pub struct PaginationOptions {
    pub page_size: u32,
    pub next_url: Option<String>,
}

impl Default for PaginationOptions {
    fn default() -> Self {
        Self {
            page_size: 50,
            next_url: None,
        }
    }
}

/// Which page of an account's transactions to get. Pages are newest first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransactionPage {
    /// The first page of the transactions created between `since` and `until`
    /// (`None` leaves that end open). Up's API doesn't say whether the ends are
    /// included, so callers overlap ranges and drop the duplicates.
    First {
        since: Option<Timestamp>,
        until: Option<Timestamp>,
    },
    /// The page at a URL returned with the one before it
    /// (`PaginatedResponse::next_page_url`), in the same range.
    Next(String),
}

impl TransactionPage {
    /// The first page of all the transactions: the newest ones.
    pub const NEWEST: TransactionPage = TransactionPage::First {
        since: None,
        until: None,
    };
}

#[derive(Debug)]
pub struct PaginatedResponse<T> {
    pub data: Vec<T>,
    pub next_page_url: Option<String>,
}

// --- Domain structs (flattened from Resource) ---

#[derive(Debug, Clone)]
pub struct Account {
    pub id: String,
    pub display_name: String,
    pub account_type: AccountType,
    pub ownership_type: OwnershipType,
    pub balance: MoneyObject,
}

/// The kind of account. Types the app doesn't know (yet) are kept as the API names
/// them, and shown that way.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AccountType {
    Transactional,
    Saver,
    HomeLoan,
    #[serde(untagged)]
    Other(String),
}

impl AccountType {
    /// Sort key for showing accounts grouped by type: spending accounts, then savers,
    /// then home loans, then anything else.
    pub fn order(&self) -> u8 {
        match self {
            AccountType::Transactional => 0,
            AccountType::Saver => 1,
            AccountType::HomeLoan => 2,
            AccountType::Other(_) => 3,
        }
    }

    /// The name of an account of this type, e.g. "Saver".
    pub fn name(&self) -> &str {
        match self {
            AccountType::Transactional => "Spending account",
            AccountType::Saver => "Saver",
            AccountType::HomeLoan => "Home loan",
            AccountType::Other(other) => other,
        }
    }

    /// The heading of a group of accounts of this type, e.g. "Savers".
    pub fn group_name(&self) -> &str {
        match self {
            AccountType::Transactional => "Spending",
            AccountType::Saver => "Savers",
            AccountType::HomeLoan => "Home loans",
            AccountType::Other(other) => other,
        }
    }
}

/// Who owns an account. Types the app doesn't know (yet) are kept as the API names
/// them, and shown that way.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum OwnershipType {
    Individual,
    /// A 2Up account, shared with another person.
    Joint,
    #[serde(untagged)]
    Other(String),
}

impl OwnershipType {
    /// A readable name, e.g. "Joint (2Up)".
    pub fn name(&self) -> &str {
        match self {
            OwnershipType::Individual => "Individual",
            OwnershipType::Joint => "Joint (2Up)",
            OwnershipType::Other(other) => other,
        }
    }
}

impl From<Resource<AccountAttributes>> for Account {
    fn from(r: Resource<AccountAttributes>) -> Self {
        Account {
            id: r.id,
            display_name: r.attributes.display_name,
            account_type: r.attributes.account_type,
            ownership_type: r.attributes.ownership_type,
            balance: r.attributes.balance,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Transaction {
    pub id: String,
    pub status: TransactionStatus,
    pub description: String,
    pub raw_text: Option<String>,
    pub message: Option<String>,
    pub amount: MoneyObject,
    pub foreign_amount: Option<MoneyObject>,
    pub hold_info: Option<HoldInfo>,
    pub card_purchase_method: Option<CardPurchaseMethod>,
    pub settled_at: Option<Timestamp>,
    pub created_at: Timestamp,
    pub round_up: Option<RoundUp>,
    pub cashback: Option<Cashback>,
    pub transaction_type: Option<String>,
    pub note: Option<Note>,
    pub performing_customer: Option<Customer>,
    /// A link that opens the transaction in Up's app, if the API gives one.
    pub deep_link_url: Option<String>,
    pub category: Option<String>,
    pub parent_category: Option<String>,
    pub tags: Vec<String>,
    /// Account id of the other side of a transfer between the user's own accounts.
    pub transfer_account: Option<String>,
}

impl Transaction {
    /// The root category the transaction is in: its category's parent, or its category
    /// if that has no parent (Up's categories are two levels deep). This comes with the
    /// transaction, so it doesn't depend on the categories having loaded.
    pub fn root_category(&self) -> Option<&str> {
        self.parent_category.as_deref().or(self.category.as_deref())
    }
}

impl From<Resource<TransactionAttributes, TransactionRelationships>> for Transaction {
    fn from(r: Resource<TransactionAttributes, TransactionRelationships>) -> Self {
        let category = r
            .relationships
            .category
            .and_then(|rel| rel.data)
            .map(|d| d.id);
        let parent_category = r
            .relationships
            .parent_category
            .and_then(|rel| rel.data)
            .map(|d| d.id);
        let tags = r
            .relationships
            .tags
            .map(|rel| rel.data.into_iter().map(|d| d.id).collect())
            .unwrap_or_default();
        let transfer_account = r
            .relationships
            .transfer_account
            .and_then(|rel| rel.data)
            .map(|d| d.id);

        Transaction {
            id: r.id,
            status: r.attributes.status,
            description: r.attributes.description,
            raw_text: r.attributes.raw_text,
            message: r.attributes.message,
            amount: r.attributes.amount,
            foreign_amount: r.attributes.foreign_amount,
            hold_info: r.attributes.hold_info,
            card_purchase_method: r.attributes.card_purchase_method,
            settled_at: r.attributes.settled_at,
            created_at: r.attributes.created_at,
            round_up: r.attributes.round_up,
            cashback: r.attributes.cashback,
            transaction_type: r.attributes.transaction_type,
            note: r.attributes.note,
            performing_customer: r.attributes.performing_customer,
            deep_link_url: r.attributes.deep_link_url,
            category,
            parent_category,
            tags,
            transfer_account,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::MoneyObject;

    fn money(value: &str, base_units: i64, currency: &str) -> MoneyObject {
        MoneyObject {
            currency_code: currency.to_string(),
            value: value.to_string(),
            value_in_base_units: base_units,
        }
    }

    #[test]
    fn format_display_marks_credits_only() {
        assert_eq!(money("10.56", 1056, "AUD").format_display(), "+$10.56");
        assert_eq!(money("-10.56", -1056, "AUD").format_display(), "$10.56");
        assert_eq!(money("0.00", 0, "AUD").format_display(), "+$0.00");
    }

    #[test]
    fn format_unsigned_drops_the_sign() {
        assert_eq!(money("-10.56", -1056, "AUD").format_unsigned(), "$10.56");
        assert_eq!(money("0.00", 0, "AUD").format_unsigned(), "$0.00");
    }

    #[test]
    fn format_signed_puts_the_sign_before_the_dollar() {
        assert_eq!(money("1234.50", 123450, "AUD").format_signed(), "$1234.50");
        assert_eq!(money("-12.30", -1230, "AUD").format_signed(), "-$12.30");
    }

    #[test]
    fn format_display_with_code_keeps_the_currency_precision() {
        assert_eq!(
            money("-12.50", -1250, "USD").format_display_with_code(),
            "12.50 USD"
        );
        assert_eq!(
            money("12.50", 1250, "USD").format_display_with_code(),
            "+12.50 USD"
        );
        // Yen have no minor unit: base units are whole yen.
        assert_eq!(
            money("-1500", -1500, "JPY").format_display_with_code(),
            "1500 JPY"
        );
    }

    #[test]
    fn sum_adds_base_units_and_keeps_the_precision() {
        let total = MoneyObject::sum(&[
            money("1234.56", 123456, "AUD"),
            money("-1300.00", -130000, "AUD"),
            money("0.05", 5, "AUD"),
        ])
        .unwrap();
        assert_eq!(total.value, "-65.39");
        assert_eq!(total.value_in_base_units, -6539);
        assert_eq!(total.format_signed(), "-$65.39");

        let yen = MoneyObject::sum(&[money("1500", 1500, "JPY"), money("20", 20, "JPY")]);
        assert_eq!(yen.unwrap().value, "1520");
    }

    #[test]
    fn zero_like_keeps_the_currency_and_precision() {
        let zero = money("-12.50", -1250, "AUD").zero_like();
        assert_eq!((zero.value.as_str(), zero.value_in_base_units), ("0.00", 0));
        assert_eq!(zero.currency_code, "AUD");
        assert_eq!(money("1500", 1500, "JPY").zero_like().value, "0");
    }

    #[test]
    fn with_base_units_keeps_the_currency_and_precision() {
        let aud = money("1.00", 100, "AUD");
        assert_eq!(
            aud.with_base_units(-123456).unwrap().format_signed(),
            "-$1234.56"
        );
        assert_eq!(aud.with_base_units(5).unwrap().value, "0.05");
        assert_eq!(
            money("1", 1, "JPY").with_base_units(1500).unwrap().value,
            "1500"
        );
    }

    #[test]
    fn base_units_per_unit_follows_the_precision() {
        assert_eq!(money("1.00", 100, "AUD").base_units_per_unit(), 100);
        assert_eq!(money("1500", 1500, "JPY").base_units_per_unit(), 1);
    }

    #[test]
    fn sum_requires_a_single_currency() {
        assert!(MoneyObject::sum(&[]).is_none());
        assert!(
            MoneyObject::sum(&[money("1.00", 100, "AUD"), money("1.00", 100, "USD")]).is_none()
        );
    }

    #[test]
    fn large_amounts_are_exact() {
        let m = money("90071992547409.93", 9007199254740993, "AUD");
        assert_eq!(m.format_signed(), "$90071992547409.93");
    }

    #[test]
    fn unknown_account_types_are_kept_as_named() {
        use super::{AccountType, OwnershipType};
        let account_type = |json| serde_json::from_str::<AccountType>(json).unwrap();
        assert_eq!(account_type(r#""SAVER""#), AccountType::Saver);
        assert_eq!(account_type(r#""HOME_LOAN""#), AccountType::HomeLoan);
        let other = account_type(r#""CREDIT_CARD""#);
        assert_eq!(other, AccountType::Other("CREDIT_CARD".to_string()));
        assert_eq!(other.name(), "CREDIT_CARD");
        assert_eq!(
            serde_json::from_str::<OwnershipType>(r#""JOINT""#).unwrap(),
            OwnershipType::Joint
        );
    }
}
