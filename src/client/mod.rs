pub mod demo;
pub mod models;

use http_cache_reqwest::{Cache, CacheMode, HttpCache, HttpCacheOptions, MokaManager};

use reqwest::header::{AUTHORIZATION, HeaderMap, HeaderValue};
use reqwest_middleware::{ClientBuilder, ClientWithMiddleware};
use serde::de::DeserializeOwned;

use models::*;
use rootcause::{Result, prelude::*};

const BASE_URL: &str = "https://api.up.com.au/api/v1";

pub struct UpClient {
    client: ClientWithMiddleware,
}

impl UpClient {
    pub fn new(token: &str) -> Result<Self> {
        let mut headers = HeaderMap::new();
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {}", token)).context("Invalid API token")?,
        );
        let client = reqwest::Client::builder()
            .default_headers(headers)
            .build()
            .context("Failed to create the HTTP client")?;
        let client = ClientBuilder::new(client)
            .with(Cache(HttpCache {
                mode: CacheMode::Default,
                manager: MokaManager::default(),
                options: HttpCacheOptions::default(),
            }))
            .build();
        Ok(UpClient { client })
    }

    /// GET `url` and decode the JSON:API response. With `reload`, the HTTP cache is
    /// bypassed (the response still updates it), for user-requested refreshes.
    async fn get_json<T: DeserializeOwned>(
        &self,
        url: &str,
        reload: bool,
    ) -> Result<JsonApiResponse<T>> {
        let mut req = self.client.get(url);
        if reload {
            req = req.with_extension(CacheMode::Reload);
        }
        Ok(req.send().await?.error_for_status()?.json().await?)
    }

    pub async fn get_categories(&self) -> Result<Vec<Category>> {
        let url = format!("{BASE_URL}/categories");
        let resp: JsonApiResponse<Vec<Resource<CategoryAttributes, CategoryRelationships>>> =
            self.get_json(&url, false).await?;
        Ok(resp.data.into_iter().map(Category::from).collect())
    }

    pub async fn get_accounts(
        &self,
        opts: PaginationOptions,
        reload: bool,
    ) -> Result<PaginatedResponse<Account>> {
        let url = opts
            .next_url
            .unwrap_or_else(|| format!("{BASE_URL}/accounts?page[size]={}", opts.page_size));
        let resp: JsonApiResponse<Vec<Resource<AccountAttributes>>> =
            self.get_json(&url, reload).await?;
        Ok(PaginatedResponse {
            data: resp.data.into_iter().map(Account::from).collect(),
            next_page_url: resp.links.and_then(|l| l.next),
        })
    }

    pub async fn get_transactions(
        &self,
        account_id: &str,
        opts: PaginationOptions,
        reload: bool,
    ) -> Result<PaginatedResponse<Transaction>> {
        let url = opts.next_url.unwrap_or_else(|| {
            format!(
                "{BASE_URL}/accounts/{account_id}/transactions?page[size]={}",
                opts.page_size
            )
        });
        let resp: JsonApiResponse<Vec<Resource<TransactionAttributes, TransactionRelationships>>> =
            self.get_json(&url, reload).await?;
        Ok(PaginatedResponse {
            data: resp.data.into_iter().map(Transaction::from).collect(),
            next_page_url: resp.links.and_then(|l| l.next),
        })
    }
}

/// Where the app's data comes from.
pub enum Client {
    /// The Up API.
    Up(UpClient),
    /// Generated data, for demo mode (`up --demo`).
    Demo(demo::DemoBank),
}

impl Client {
    pub async fn get_categories(&self) -> Result<Vec<Category>> {
        match self {
            Client::Up(client) => client.get_categories().await,
            Client::Demo(bank) => bank.get_categories().await,
        }
    }

    pub async fn get_accounts(
        &self,
        opts: PaginationOptions,
        reload: bool,
    ) -> Result<PaginatedResponse<Account>> {
        match self {
            Client::Up(client) => client.get_accounts(opts, reload).await,
            Client::Demo(bank) => bank.get_accounts().await,
        }
    }

    pub async fn get_transactions(
        &self,
        account_id: &str,
        opts: PaginationOptions,
        reload: bool,
    ) -> Result<PaginatedResponse<Transaction>> {
        match self {
            Client::Up(client) => client.get_transactions(account_id, opts, reload).await,
            Client::Demo(bank) => bank.get_transactions(account_id, opts).await,
        }
    }
}
