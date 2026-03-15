use anyhow::Result;
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION};

use super::models::*;

const BASE_URL: &str = "https://api.up.com.au/api/v1";

pub struct UpClient {
    client: reqwest::Client,
}

impl UpClient {
    pub fn new(token: &str) -> Result<Self> {
        let mut headers = HeaderMap::new();
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {}", token))?,
        );
        let client = reqwest::Client::builder()
            .default_headers(headers)
            .build()?;
        Ok(UpClient { client })
    }

    pub async fn get_accounts(&self) -> Result<Vec<Account>> {
        let url = format!("{}/accounts?page[size]=100", BASE_URL);
        let resp: JsonApiResponse<Vec<Resource<AccountAttributes>>> =
            self.client.get(&url).send().await?.error_for_status()?.json().await?;
        Ok(resp.data.into_iter().map(Account::from).collect())
    }

    pub async fn get_transactions(&self, account_id: &str) -> Result<Vec<Transaction>> {
        let url = format!(
            "{}/accounts/{}/transactions?page[size]=50",
            BASE_URL, account_id
        );
        let resp: JsonApiResponse<Vec<Resource<TransactionAttributes>>> =
            self.client.get(&url).send().await?.error_for_status()?.json().await?;
        Ok(resp.data.into_iter().map(Transaction::from).collect())
    }
}
