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
        page: TransactionPage,
        page_size: u32,
        reload: bool,
    ) -> Result<PaginatedResponse<Transaction>> {
        let url = transactions_url(account_id, page, page_size)?;
        let resp: JsonApiResponse<Vec<Resource<TransactionAttributes, TransactionRelationships>>> =
            self.get_json(&url, reload).await?;
        Ok(PaginatedResponse {
            data: resp.data.into_iter().map(Transaction::from).collect(),
            next_page_url: resp.links.and_then(|l| l.next),
        })
    }
}

/// The URL of a page of an account's transactions.
fn transactions_url(account_id: &str, page: TransactionPage, page_size: u32) -> Result<String> {
    let (since, until) = match page {
        TransactionPage::Next(url) => return Ok(url),
        TransactionPage::First { since, until } => (since, until),
    };
    let mut url = reqwest::Url::parse(&format!("{BASE_URL}/accounts/{account_id}/transactions"))
        .context("Invalid account id")?;
    {
        let mut query = url.query_pairs_mut();
        query.append_pair("page[size]", &page_size.to_string());
        if let Some(since) = since {
            query.append_pair("filter[since]", &since.to_string());
        }
        if let Some(until) = until {
            query.append_pair("filter[until]", &until.to_string());
        }
    }
    Ok(url.into())
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
        page: TransactionPage,
        page_size: u32,
        reload: bool,
    ) -> Result<PaginatedResponse<Transaction>> {
        match self {
            Client::Up(client) => {
                client
                    .get_transactions(account_id, page, page_size, reload)
                    .await
            }
            Client::Demo(bank) => bank.get_transactions(account_id, page, page_size).await,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transaction_urls_have_the_range_as_rfc_3339_filters() {
        let url = |page| transactions_url("abc", page, 100).unwrap();
        assert_eq!(
            url(TransactionPage::NEWEST),
            "https://api.up.com.au/api/v1/accounts/abc/transactions?page%5Bsize%5D=100"
        );
        let range = TransactionPage::First {
            since: Some("2025-12-31T12:59:59Z".parse().unwrap()),
            until: Some("2026-02-01T00:00:01Z".parse().unwrap()),
        };
        assert_eq!(
            url(range),
            "https://api.up.com.au/api/v1/accounts/abc/transactions?page%5Bsize%5D=100\
             &filter%5Bsince%5D=2025-12-31T12%3A59%3A59Z&filter%5Buntil%5D=2026-02-01T00%3A00%3A01Z"
        );
        let next = "https://api.up.com.au/api/v1/accounts/abc/transactions?page%5Bafter%5D=x";
        assert_eq!(url(TransactionPage::Next(next.to_string())), next);
    }
}
