//! Fetch every device and its latest sensor values in one go, when the app asks for it.

use serde::{Deserialize, Serialize};

use crate::api::{Device, DeviceSensors, RateLimit, Unit};
use crate::error::Result;
use crate::session::Session;

/// What the account looked like at one refresh.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Snapshot {
    pub devices: Vec<Device>,
    pub readings: Vec<DeviceSensors>,
    pub rate_limit: Option<RateLimit>,
}

/// Fetch the device list and the latest readings. Readings are always fetched in metric.
pub async fn fetch(session: &Session) -> Result<Snapshot> {
    let account = session.account_id().await?;
    let account = account.as_str();
    let devices = session.call(|t| async move { session.api().devices(&t, account).await }).await?;
    let (readings, rate_limit) =
        session.call(|t| async move { session.api().sensors(&t, account, Unit::Metric).await }).await?;
    Ok(Snapshot { devices, readings, rate_limit: Some(rate_limit) })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::ApiClient;
    use crate::auth::{AuthClient, Credentials};
    use crate::store::{MemoryStore, SecretStore};
    use serde_json::json;
    use std::sync::Arc;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn fetches_devices_and_readings() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"access_token": "a", "expires_in": 3600})))
            .mount(&server)
            .await;
        Mock::given(path("/v1/accounts"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"accounts": [{"id": "acc"}]})))
            .mount(&server)
            .await;
        Mock::given(path("/v1/accounts/acc/devices"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"devices": [
                {"serialNumber": "A", "name": "Office", "type": "VIEW_PLUS", "sensors": ["co2"]}
            ]})))
            .mount(&server)
            .await;
        Mock::given(path("/v1/accounts/acc/sensors"))
            .respond_with(ResponseTemplate::new(200).insert_header("X-RateLimit-Remaining", "100").set_body_json(json!({
                "results": [{"serialNumber": "A", "recorded": "2026-10-14T10:00:00Z",
                             "sensors": [{"sensorType": "co2", "value": 700, "unit": "ppm"}]}],
                "hasNext": false
            })))
            .mount(&server)
            .await;

        let secrets = Arc::new(MemoryStore::default());
        secrets
            .save(&Credentials::ClientCredentials { client_id: "id".into(), client_secret: "s".into(), scope: "read".into() })
            .unwrap();
        let http = reqwest::Client::new();
        let session = Session::new(
            AuthClient::new(http.clone(), format!("{}/v1/token", server.uri())),
            ApiClient::new(http, server.uri()),
            secrets,
        );

        let snapshot = fetch(&session).await.unwrap();

        assert_eq!(snapshot.devices[0].name, "Office");
        assert_eq!(snapshot.readings[0].sensors[0].value, 700.0);
        assert_eq!(snapshot.rate_limit.unwrap().remaining, Some(100));
    }
}
