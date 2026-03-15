use chrono::{DateTime, Utc};
use serde::Deserialize;

// --- JSON:API envelope types ---

#[derive(Debug, Deserialize)]
pub struct JsonApiResponse<T> {
    pub data: T,
    pub links: Option<PaginationLinks>,
}

#[derive(Debug, Deserialize)]
pub struct PaginationLinks {
    pub prev: Option<String>,
    pub next: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct Resource<A> {
    pub id: String,
    #[serde(rename = "type")]
    pub resource_type: String,
    pub attributes: A,
    pub relationships: Option<serde_json::Value>,
}

// --- Account types ---

#[derive(Debug, Deserialize)]
pub struct AccountAttributes {
    #[serde(rename = "displayName")]
    pub display_name: String,
    #[serde(rename = "accountType")]
    pub account_type: String,
    pub balance: MoneyObject,
    #[serde(rename = "createdAt")]
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MoneyObject {
    #[serde(rename = "currencyCode")]
    pub currency_code: String,
    pub value: String,
    #[serde(rename = "valueInBaseUnits")]
    pub value_in_base_units: i64,
}

// --- Transaction types ---

#[derive(Debug, Deserialize)]
pub struct TransactionAttributes {
    pub status: TransactionStatus,
    #[serde(rename = "rawText")]
    pub raw_text: Option<String>,
    pub description: String,
    pub message: Option<String>,
    pub amount: MoneyObject,
    #[serde(rename = "foreignAmount")]
    pub foreign_amount: Option<MoneyObject>,
    #[serde(rename = "cardPurchaseMethod")]
    pub card_purchase_method: Option<CardPurchaseMethod>,
    #[serde(rename = "settledAt")]
    pub settled_at: Option<DateTime<Utc>>,
    #[serde(rename = "createdAt")]
    pub created_at: DateTime<Utc>,
    #[serde(rename = "roundUp")]
    pub round_up: Option<RoundUp>,
    pub cashback: Option<Cashback>,
    #[serde(rename = "holdInfo")]
    pub hold_info: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RoundUp {
    pub amount: MoneyObject,
    #[serde(rename = "boostPortion")]
    pub boost_portion: Option<MoneyObject>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Cashback {
    pub description: String,
    pub amount: MoneyObject,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CardPurchaseMethod {
    pub method: String,
    #[serde(rename = "cardNumberSuffix")]
    pub card_number_suffix: Option<String>,
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
            TransactionStatus::Held => write!(f, "HELD"),
            TransactionStatus::Settled => write!(f, "STTL"),
        }
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
    pub card_purchase_method: Option<CardPurchaseMethod>,
    pub settled_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub round_up: Option<RoundUp>,
    pub cashback: Option<Cashback>,
}

impl From<Resource<TransactionAttributes>> for Transaction {
    fn from(r: Resource<TransactionAttributes>) -> Self {
        Transaction {
            id: r.id,
            status: r.attributes.status,
            description: r.attributes.description,
            raw_text: r.attributes.raw_text,
            message: r.attributes.message,
            amount: r.attributes.amount,
            foreign_amount: r.attributes.foreign_amount,
            card_purchase_method: r.attributes.card_purchase_method,
            settled_at: r.attributes.settled_at,
            created_at: r.attributes.created_at,
            round_up: r.attributes.round_up,
            cashback: r.attributes.cashback,
        }
    }
}
