use http_cache_reqwest::{Cache, CacheMode, HttpCache, HttpCacheOptions, MokaManager};
use reqwest::Client;
use reqwest::header::{AUTHORIZATION, HeaderMap, HeaderValue};
use reqwest_middleware::{ClientBuilder, ClientWithMiddleware};

use super::error::Result;
use super::models::*;

const BASE_URL: &str = "https://api.up.com.au/api/v1";

pub struct UpClient {
    client: ClientWithMiddleware,
}

impl UpClient {
    pub fn new(token: &str) -> Result<Self> {
        let mut headers = HeaderMap::new();
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {}", token))?,
        );
        let client = Client::builder().default_headers(headers).build()?;
        let client = ClientBuilder::new(client)
            .with(Cache(HttpCache {
                mode: CacheMode::Default,
                manager: MokaManager::default(),
                options: HttpCacheOptions::default(),
            }))
            .build();
        Ok(UpClient { client })
    }

    pub async fn get_categories(&self) -> Result<Vec<(String, String)>> {
        let url = format!("{}/categories", BASE_URL);
        let resp: JsonApiResponse<Vec<Resource<CategoryAttributes>>> = self
            .client
            .get(&url)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        Ok(resp
            .data
            .into_iter()
            .map(|r| (r.id, r.attributes.name))
            .collect())
    }

    pub async fn get_accounts(&self) -> Result<Vec<Account>> {
        let url = format!("{}/accounts?page[size]=100", BASE_URL);
        let resp: JsonApiResponse<Vec<Resource<AccountAttributes>>> = self
            .client
            .get(&url)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        Ok(resp.data.into_iter().map(Account::from).collect())
    }

    pub async fn get_transactions(&self, account_id: &str) -> Result<Vec<Transaction>> {
        let url = format!(
            "{}/accounts/{}/transactions?page[size]=50",
            BASE_URL, account_id
        );
        let resp: JsonApiResponse<Vec<Resource<TransactionAttributes, TransactionRelationships>>> =
            self.client
                .get(&url)
                .send()
                .await?
                .error_for_status()?
                .json()
                .await?;
        Ok(resp.data.into_iter().map(Transaction::from).collect())
    }
}
