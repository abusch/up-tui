pub mod error;
pub mod models;

use http_cache_reqwest::{Cache, CacheMode, HttpCache, HttpCacheOptions, MokaManager};
use reqwest::Client;
use reqwest::header::{AUTHORIZATION, HeaderMap, HeaderValue};
use reqwest_middleware::{ClientBuilder, ClientWithMiddleware};
use serde::de::DeserializeOwned;

use error::Result;
use models::*;

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

    async fn get_json<T: DeserializeOwned>(&self, url: &str) -> Result<JsonApiResponse<T>> {
        Ok(self
            .client
            .get(url)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?)
    }

    pub async fn get_categories(&self) -> Result<Vec<(String, String)>> {
        let url = format!("{BASE_URL}/categories");
        let resp: JsonApiResponse<Vec<Resource<CategoryAttributes>>> = self.get_json(&url).await?;
        Ok(resp
            .data
            .into_iter()
            .map(|r| (r.id, r.attributes.name))
            .collect())
    }

    pub async fn get_accounts(
        &self,
        opts: PaginationOptions,
    ) -> Result<PaginatedResponse<Account>> {
        let url = opts
            .next_url
            .unwrap_or_else(|| format!("{BASE_URL}/accounts?page[size]={}", opts.page_size));
        let resp: JsonApiResponse<Vec<Resource<AccountAttributes>>> = self.get_json(&url).await?;
        Ok(PaginatedResponse {
            data: resp.data.into_iter().map(Account::from).collect(),
            next_page_url: resp.links.and_then(|l| l.next),
        })
    }

    pub async fn get_transactions(
        &self,
        account_id: &str,
        opts: PaginationOptions,
    ) -> Result<PaginatedResponse<Transaction>> {
        let url = opts.next_url.unwrap_or_else(|| {
            format!(
                "{BASE_URL}/accounts/{account_id}/transactions?page[size]={}",
                opts.page_size
            )
        });
        let resp: JsonApiResponse<Vec<Resource<TransactionAttributes, TransactionRelationships>>> =
            self.get_json(&url).await?;
        Ok(PaginatedResponse {
            data: resp.data.into_iter().map(Transaction::from).collect(),
            next_page_url: resp.links.and_then(|l| l.next),
        })
    }
}
