//! Typed client for the Airthings Consumer API.
//! Spec: https://consumer-api.airthings.com/openapi.yaml

use reqwest::{header::HeaderMap, Response, StatusCode};
use serde::{Deserialize, Deserializer, Serialize};

use crate::error::{Error, Result};

pub const DEFAULT_BASE_URL: &str = "https://consumer-api.airthings.com";

/// Upper bound on sensor pages fetched per poll, as a guard against a server that never stops paging.
const MAX_PAGES: u32 = 50;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Unit {
    #[default]
    Metric,
    Imperial,
}

impl Unit {
    fn as_str(self) -> &'static str {
        match self {
            Unit::Metric => "metric",
            Unit::Imperial => "imperial",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Account {
    pub id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Device {
    pub serial_number: String,
    #[serde(default)]
    pub home: Option<String>,
    #[serde(default)]
    pub name: String,
    #[serde(rename = "type", default)]
    pub device_type: String,
    #[serde(default)]
    pub sensors: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SensorValue {
    pub sensor_type: String,
    pub value: f64,
    #[serde(default)]
    pub unit: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceSensors {
    pub serial_number: String,
    #[serde(default, deserialize_with = "skip_nulls")]
    pub sensors: Vec<SensorValue>,
    #[serde(default)]
    pub recorded: Option<String>,
    #[serde(default)]
    pub battery_percentage: Option<i32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteControlState {
    pub mode: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fan_speed: Option<u8>,
}

/// Rate-limit headers reported by the sensors endpoint.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RateLimit {
    pub limit: Option<u32>,
    pub remaining: Option<u32>,
    pub reset: Option<String>,
}

impl RateLimit {
    fn from_headers(headers: &HeaderMap) -> Self {
        let text = |name: &str| headers.get(name).and_then(|v| v.to_str().ok()).map(str::to_owned);
        RateLimit {
            limit: text("x-ratelimit-limit").and_then(|v| v.parse().ok()),
            remaining: text("x-ratelimit-remaining").and_then(|v| v.parse().ok()),
            reset: text("x-ratelimit-reset"),
        }
    }
}

#[derive(Deserialize)]
struct AccountsResponse {
    #[serde(default)]
    accounts: Vec<Account>,
}

#[derive(Deserialize)]
struct DevicesResponse {
    #[serde(default)]
    devices: Vec<Device>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SensorsPage {
    #[serde(default)]
    results: Vec<DeviceSensors>,
    #[serde(default)]
    has_next: bool,
}

#[derive(Deserialize)]
struct ErrorBody {
    message: Option<String>,
}

/// The API marks individual sensor entries as nullable.
fn skip_nulls<'de, D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Vec<SensorValue>, D::Error> {
    let values: Option<Vec<Option<SensorValue>>> = Option::deserialize(deserializer)?;
    Ok(values.unwrap_or_default().into_iter().flatten().collect())
}

#[derive(Clone)]
pub struct ApiClient {
    http: reqwest::Client,
    base_url: String,
}

impl ApiClient {
    pub fn new(http: reqwest::Client, base_url: impl Into<String>) -> Self {
        ApiClient { http, base_url: base_url.into().trim_end_matches('/').to_owned() }
    }

    pub async fn accounts(&self, token: &str) -> Result<Vec<Account>> {
        let resp = self.send(self.http.get(self.url("/v1/accounts")), token).await?;
        Ok(resp.json::<AccountsResponse>().await?.accounts)
    }

    pub async fn devices(&self, token: &str, account_id: &str) -> Result<Vec<Device>> {
        let url = self.url(&format!("/v1/accounts/{account_id}/devices"));
        let resp = self.send(self.http.get(url), token).await?;
        Ok(resp.json::<DevicesResponse>().await?.devices)
    }

    /// Latest values for every device on the account, following pagination.
    pub async fn sensors(&self, token: &str, account_id: &str, unit: Unit) -> Result<(Vec<DeviceSensors>, RateLimit)> {
        let url = self.url(&format!("/v1/accounts/{account_id}/sensors"));
        let mut results = Vec::new();
        let mut rate_limit = RateLimit::default();
        for page in 1..=MAX_PAGES {
            let request = self.http.get(&url).query(&[("pageNumber", page.to_string()), ("unit", unit.as_str().to_owned())]);
            let resp = self.send(request, token).await?;
            rate_limit = RateLimit::from_headers(resp.headers());
            let body: SensorsPage = resp.json().await?;
            results.extend(body.results);
            if !body.has_next {
                break;
            }
        }
        Ok((results, rate_limit))
    }

    /// Last mode reported by a Renew air purifier, or `None` if it has never synced one.
    pub async fn remote_control(&self, token: &str, account_id: &str, serial: &str) -> Result<Option<RemoteControlState>> {
        let url = self.url(&format!("/v1/accounts/{account_id}/devices/{serial}/remote-control"));
        match self.send(self.http.get(url), token).await {
            Ok(resp) => Ok(Some(resp.json().await?)),
            Err(Error::Api { status: 404, .. }) => Ok(None),
            Err(e) => Err(e),
        }
    }

    pub async fn set_remote_control(&self, token: &str, account_id: &str, serial: &str, state: &RemoteControlState) -> Result<()> {
        let url = self.url(&format!("/v1/accounts/{account_id}/devices/{serial}/remote-control"));
        self.send(self.http.put(url).json(state), token).await?;
        Ok(())
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }

    async fn send(&self, request: reqwest::RequestBuilder, token: &str) -> Result<Response> {
        let resp = request.bearer_auth(token).send().await?;
        check_status(resp).await
    }
}

async fn check_status(resp: Response) -> Result<Response> {
    match resp.status() {
        s if s.is_success() => Ok(resp),
        StatusCode::UNAUTHORIZED => Err(Error::Unauthorized),
        StatusCode::TOO_MANY_REQUESTS => Err(Error::RateLimited(retry_after_seconds(resp.headers()))),
        status => {
            let text = resp.text().await.unwrap_or_default();
            let message = serde_json::from_str::<ErrorBody>(&text)
                .ok()
                .and_then(|b| b.message)
                .unwrap_or(text);
            Err(Error::Api { status: status.as_u16(), message })
        }
    }
}

/// The spec documents `X-RateLimit-Retry-After` as a number of seconds; fall back to a minute.
fn retry_after_seconds(headers: &HeaderMap) -> u64 {
    ["x-ratelimit-retry-after", "retry-after"]
        .iter()
        .find_map(|name| headers.get(*name)?.to_str().ok()?.trim().parse::<u64>().ok())
        .unwrap_or(60)
}
