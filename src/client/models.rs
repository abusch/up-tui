#![allow(dead_code)]

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
    pub prev: Option<String>,
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
}

// --- Category types ---

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CategoryAttributes {
    pub name: String,
}

// --- Account types ---

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountAttributes {
    pub display_name: String,
    pub account_type: String,
    pub balance: MoneyObject,
    pub created_at: Timestamp,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MoneyObject {
    pub currency_code: String,
    pub value: String,
    pub value_in_base_units: i64,
}

impl MoneyObject {
    /// Format as a dollar amount. When `signed` is true, both positive and
    /// negative values get an explicit sign (`+$1.23` / `-$1.23`). When false,
    /// only positive values get a `+` prefix and negatives show as `$1.23`.
    pub fn format_display(&self, signed: bool) -> String {
        let cents = self.value_in_base_units;
        let abs = (cents.unsigned_abs() as f64) / 100.0;
        if cents >= 0 {
            format!("+${abs:.2}")
        } else if signed {
            format!("-${abs:.2}")
        } else {
            format!("${abs:.2}")
        }
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

#[derive(Debug, Clone, Deserialize)]
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

#[derive(Debug)]
pub struct PaginatedResponse<T> {
    pub data: Vec<T>,
    pub next_page_url: Option<String>,
}

impl<T> PaginatedResponse<T> {
    /// Returns `PaginationOptions` to fetch the next page, or `None` if there is no next page.
    pub fn next_page(&self) -> Option<PaginationOptions> {
        self.next_page_url.as_ref().map(|url| PaginationOptions {
            next_url: Some(url.clone()),
            ..Default::default()
        })
    }
}

// --- Domain structs (flattened from Resource) ---

#[derive(Debug)]
pub struct Account {
    pub id: String,
    pub display_name: String,
    pub account_type: String,
    pub balance: MoneyObject,
}

impl From<Resource<AccountAttributes>> for Account {
    fn from(r: Resource<AccountAttributes>) -> Self {
        Account {
            id: r.id,
            display_name: r.attributes.display_name,
            account_type: r.attributes.account_type,
            balance: r.attributes.balance,
        }
    }
}

#[derive(Debug)]
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
    pub category: Option<String>,
    pub parent_category: Option<String>,
    pub tags: Vec<String>,
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
            category,
            parent_category,
            tags,
        }
    }
}
