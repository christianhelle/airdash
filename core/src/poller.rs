//! Background loop that polls the latest sensor values and records them as history.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};

use serde::Serialize;
use tokio::sync::Notify;

use crate::api::{RateLimit, Unit};
use crate::error::{Error, Result};
use crate::session::Session;
use crate::settings::Settings;
use crate::store::Store;

/// The device list rarely changes, so it is fetched less often than sensor values.
const DEVICE_REFRESH: Duration = Duration::from_secs(3600);
const FIRST_RETRY: Duration = Duration::from_secs(30);
/// Below this many remaining requests, slow down until the rate-limit window resets.
const LOW_REMAINING: u32 = 3;

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PollerStatus {
    pub signed_in: bool,
    pub paused: bool,
    pub polling: bool,
    /// Epoch seconds.
    pub last_poll: Option<i64>,
    pub next_poll: Option<i64>,
    pub last_error: Option<String>,
    pub rate_limit: Option<RateLimit>,
}

#[derive(Debug, Clone)]
pub enum PollerEvent {
    Status(PollerStatus),
    DevicesUpdated,
    ReadingsUpdated,
}

pub type EventSink = Box<dyn Fn(PollerEvent) + Send + Sync>;

pub struct Poller {
    session: Arc<Session>,
    store: Arc<Store>,
    settings: Arc<RwLock<Settings>>,
    events: EventSink,
    status: Mutex<PollerStatus>,
    wake: Notify,
    paused: AtomicBool,
    force_devices: AtomicBool,
    devices_fetched: Mutex<Option<Instant>>,
}

impl Poller {
    pub fn new(session: Arc<Session>, store: Arc<Store>, settings: Arc<RwLock<Settings>>, events: EventSink) -> Self {
        Poller {
            session,
            store,
            settings,
            events,
            status: Mutex::new(PollerStatus::default()),
            wake: Notify::new(),
            paused: AtomicBool::new(false),
            force_devices: AtomicBool::new(true),
            devices_fetched: Mutex::new(None),
        }
    }

    pub fn status(&self) -> PollerStatus {
        let mut status = self.status.lock().unwrap().clone();
        status.signed_in = self.session.is_signed_in();
        status.paused = self.paused.load(Ordering::SeqCst);
        status
    }

    /// Poll as soon as possible, including the device list.
    pub fn refresh_now(&self) {
        self.force_devices.store(true, Ordering::SeqCst);
        self.wake.notify_one();
    }

    pub fn set_paused(&self, paused: bool) {
        self.paused.store(paused, Ordering::SeqCst);
        self.wake.notify_one();
        self.publish_status();
    }

    /// Forget the previous account's state, e.g. after signing out or in as someone else.
    pub fn reset(&self) {
        *self.status.lock().unwrap() = PollerStatus::default();
        *self.devices_fetched.lock().unwrap() = None;
        self.refresh_now();
        self.publish_status();
    }

    fn publish_status(&self) {
        (self.events)(PollerEvent::Status(self.status()));
    }

    fn update_status(&self, f: impl FnOnce(&mut PollerStatus)) {
        f(&mut self.status.lock().unwrap());
        self.publish_status();
    }

    fn devices_due(&self) -> bool {
        let forced = self.force_devices.swap(false, Ordering::SeqCst);
        let stale = self.devices_fetched.lock().unwrap().map(|t| t.elapsed() >= DEVICE_REFRESH).unwrap_or(true);
        forced || stale
    }

    /// One round trip: refresh devices if due, then fetch and record the latest readings.
    pub async fn poll_once(&self) -> Result<RateLimit> {
        let session = &self.session;
        let account = session.account_id().await?;
        let account = account.as_str();

        if self.devices_due() {
            let devices = session.call(|t| async move { session.api().devices(&t, account).await }).await?;
            self.store.replace_devices(&devices)?;
            *self.devices_fetched.lock().unwrap() = Some(Instant::now());
            (self.events)(PollerEvent::DevicesUpdated);
        }

        let (results, rate_limit) = session.call(|t| async move { session.api().sensors(&t, account, Unit::Metric).await }).await?;
        self.store.insert_readings(&results)?;
        (self.events)(PollerEvent::ReadingsUpdated);
        Ok(rate_limit)
    }

    pub async fn run(self: Arc<Self>) {
        let mut failures = 0u32;
        loop {
            if self.paused.load(Ordering::SeqCst) || !self.session.is_signed_in() {
                self.update_status(|s| s.next_poll = None);
                self.wake.notified().await;
                continue;
            }

            self.update_status(|s| s.polling = true);
            let outcome = self.poll_once().await;
            failures = if outcome.is_ok() { 0 } else { failures + 1 };
            let interval = Duration::from_secs(self.settings.read().unwrap().poll_interval_minutes as u64 * 60);
            let delay = next_delay(&outcome, failures, interval);
            let now = chrono::Utc::now().timestamp();
            self.update_status(|s| {
                s.polling = false;
                s.next_poll = Some(now + delay.as_secs() as i64);
                match outcome {
                    Ok(rate_limit) => {
                        s.last_poll = Some(now);
                        s.last_error = None;
                        s.rate_limit = Some(rate_limit);
                    }
                    Err(Error::Unauthorized) => s.last_error = None,
                    Err(e) => s.last_error = Some(e.to_string()),
                }
            });

            tokio::select! {
                _ = tokio::time::sleep(delay) => {}
                _ = self.wake.notified() => {}
            }
        }
    }
}

/// How long to wait before the next poll.
pub fn next_delay(outcome: &Result<RateLimit>, failures: u32, interval: Duration) -> Duration {
    match outcome {
        Ok(rate_limit) if rate_limit.remaining.is_some_and(|r| r < LOW_REMAINING) => interval * 2,
        Ok(_) => interval,
        Err(Error::RateLimited(seconds)) => Duration::from_secs((*seconds).max(60)),
        // Signed out: the loop parks until woken, so the delay is irrelevant.
        Err(Error::Unauthorized) => interval,
        Err(_) => {
            let backoff = FIRST_RETRY.saturating_mul(2u32.saturating_pow(failures.saturating_sub(1)));
            backoff.min(interval)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::ApiClient;
    use crate::auth::{AuthClient, Credentials};
    use crate::secrets::{MemoryStore, SecretStore};
    use serde_json::json;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    const FIVE_MINUTES: Duration = Duration::from_secs(300);

    #[test]
    fn waits_the_interval_after_success() {
        assert_eq!(next_delay(&Ok(RateLimit::default()), 0, FIVE_MINUTES), FIVE_MINUTES);
    }

    #[test]
    fn slows_down_when_few_requests_remain() {
        let low = RateLimit { remaining: Some(1), ..Default::default() };
        assert_eq!(next_delay(&Ok(low), 0, FIVE_MINUTES), FIVE_MINUTES * 2);
    }

    #[test]
    fn honors_retry_after_when_rate_limited() {
        assert_eq!(next_delay(&Err(Error::RateLimited(900)), 1, FIVE_MINUTES), Duration::from_secs(900));
    }

    #[test]
    fn backs_off_exponentially_up_to_the_interval() {
        let err = || Err(Error::Other("offline".into()));
        assert_eq!(next_delay(&err(), 1, FIVE_MINUTES), Duration::from_secs(30));
        assert_eq!(next_delay(&err(), 2, FIVE_MINUTES), Duration::from_secs(60));
        assert_eq!(next_delay(&err(), 3, FIVE_MINUTES), Duration::from_secs(120));
        assert_eq!(next_delay(&err(), 10, FIVE_MINUTES), FIVE_MINUTES);
    }

    #[tokio::test]
    async fn poll_once_records_devices_and_readings() {
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
            .expect(1)
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
        let session = Arc::new(Session::new(
            AuthClient::new(http.clone(), format!("{}/v1/token", server.uri())),
            ApiClient::new(http, server.uri()),
            secrets,
        ));
        let store = Arc::new(Store::open_in_memory().unwrap());
        let events = Arc::new(Mutex::new(Vec::new()));
        let sink = events.clone();
        let poller = Poller::new(
            session,
            store.clone(),
            Arc::new(RwLock::new(Settings::default())),
            Box::new(move |e| sink.lock().unwrap().push(format!("{e:?}"))),
        );

        let rate_limit = poller.poll_once().await.unwrap();
        poller.poll_once().await.unwrap(); // devices are not fetched again within the hour

        assert_eq!(rate_limit.remaining, Some(100));
        assert_eq!(store.devices().unwrap()[0].name, "Office");
        assert_eq!(store.latest().unwrap()[0].sensors[0].value, 700.0);
        let events = events.lock().unwrap();
        assert_eq!(events.iter().filter(|e| *e == "DevicesUpdated").count(), 1);
        assert_eq!(events.iter().filter(|e| *e == "ReadingsUpdated").count(), 2);
    }
}
