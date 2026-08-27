use std::error::Error;
use std::sync::LazyLock;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use chrono::{DateTime, Local};
use gpui::SharedString;
use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::ics::Import;
use crate::task::{Priority, Task, TaskKind};

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubscriptionId(u64);

static NEXT_SUBSCRIPTION_ID: AtomicU64 = AtomicU64::new(0);

static CALENDAR_CLIENT: LazyLock<Client> = LazyLock::new(|| {
    Client::builder()
        .connect_timeout(Subscription::CONNECT_TIMEOUT)
        .timeout(Subscription::TIMEOUT)
        .user_agent(concat!(
            env!("CARGO_PKG_NAME"),
            "/",
            env!("CARGO_PKG_VERSION")
        ))
        .build()
        .expect("failed to build the calendar client")
});

impl SubscriptionId {
    pub fn reserve(self) {
        NEXT_SUBSCRIPTION_ID.fetch_max(self.0 + 1, Ordering::Relaxed);
    }

    fn next() -> Self {
        Self(NEXT_SUBSCRIPTION_ID.fetch_add(1, Ordering::Relaxed))
    }
}

#[derive(Clone, Copy, Serialize, Deserialize)]
pub enum Transitions {
    Fixed {
        prep: i32,
        cleanup: i32,
    },
    Scaled {
        start_percent: i32,
        end_percent: i32,
        shortest: i32,
        longest: i32,
    },
}

impl Transitions {
    pub fn minutes(self, duration: i32) -> (i32, i32) {
        match self {
            Self::Fixed { prep, cleanup } => (prep, cleanup),
            Self::Scaled {
                start_percent,
                end_percent,
                shortest,
                longest,
            } => (
                Self::scale(start_percent, duration, shortest, longest),
                Self::scale(end_percent, duration, shortest, longest),
            ),
        }
    }

    fn scale(percent: i32, duration: i32, shortest: i32, longest: i32) -> i32 {
        (duration * percent / 100).clamp(shortest, longest.max(shortest))
    }
}

#[derive(Clone, Copy, Serialize, Deserialize)]
pub struct CalendarDefaults {
    pub priority: Priority,
    pub overrun_percent: i32,
    pub transitions: Transitions,
}

impl Default for CalendarDefaults {
    fn default() -> Self {
        Self {
            priority: Priority::Normal,
            overrun_percent: 0,
            transitions: Transitions::Fixed {
                prep: 0,
                cleanup: 0,
            },
        }
    }
}

impl CalendarDefaults {
    pub fn govern(&self, task: &mut Task) {
        let TaskKind::Fixed { duration, .. } = task.kind else {
            return;
        };

        task.priority = self.priority;
        (task.prep, task.cleanup) = self.transitions.minutes(duration);

        if let TaskKind::Fixed {
            overrun_percent, ..
        } = &mut task.kind
        {
            *overrun_percent = self.overrun_percent;
        }
    }
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubscribedEvent {
    pub subscription: SubscriptionId,
    pub uid: SharedString,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Subscription {
    pub id: SubscriptionId,
    pub url: SharedString,
    pub name: SharedString,
    pub synced: Option<DateTime<Local>>,
    pub dropped: usize,
    pub failure: Option<SharedString>,
    #[serde(default)]
    pub defaults: CalendarDefaults,
    #[serde(skip)]
    pub syncing: bool,
}

impl Subscription {
    const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
    const TIMEOUT: Duration = Duration::from_secs(30);
    const LARGEST: u64 = 512 * 1024 * 1024;

    pub fn new(url: &str) -> Self {
        let url = Self::normalize(url);

        Self {
            id: SubscriptionId::next(),
            name: Self::host(&url),
            url,
            synced: None,
            dropped: 0,
            failure: None,
            defaults: CalendarDefaults::default(),
            syncing: false,
        }
    }

    pub async fn fetch(url: SharedString) -> anyhow::Result<String> {
        let mut response = CALENDAR_CLIENT
            .get(url.as_ref())
            .send()
            .await?
            .error_for_status()?;
        let mut body = Vec::new();

        Self::admits(response.content_length())?;

        while let Some(chunk) = response.chunk().await? {
            Self::admits(Some((body.len() + chunk.len()) as u64))?;
            body.extend_from_slice(&chunk);
        }

        Ok(String::from_utf8(body)?)
    }

    fn admits(bytes: Option<u64>) -> anyhow::Result<()> {
        anyhow::ensure!(
            bytes.is_none_or(|bytes| bytes <= Self::LARGEST),
            "calendar is larger than {} MB",
            Self::LARGEST / (1024 * 1024)
        );

        Ok(())
    }

    pub fn imported(&mut self, import: Import, now: DateTime<Local>) -> Vec<Task> {
        self.syncing = false;
        self.synced = Some(now);
        self.dropped = import.dropped;
        self.failure = None;

        if let Some(name) = import.name {
            self.name = name.into();
        }

        import.tasks
    }

    pub fn failed(&mut self, failure: anyhow::Error, now: DateTime<Local>) {
        self.syncing = false;
        self.synced = Some(now);
        self.failure = Some(Self::reason(&*failure).into());
    }

    pub fn short_address(&self) -> SharedString {
        let address = Self::address(&self.url);
        let address = address.split(['?', '#']).next().unwrap_or_default();

        let Some((host, path)) = address.split_once('/') else {
            return address.to_string().into();
        };
        let path = path.trim_end_matches('/');

        match path.rsplit_once('/') {
            Some((_, tail)) => format!("{host}/…/{tail}").into(),
            None if path.is_empty() => host.to_string().into(),
            None => format!("{host}/{path}").into(),
        }
    }

    fn reason(failure: &dyn Error) -> String {
        let mut deepest = failure;

        while let Some(source) = deepest.source() {
            deepest = source;
        }

        deepest.to_string()
    }

    fn normalize(url: &str) -> SharedString {
        let url = url.trim();

        match url.split_once("://") {
            Some(("webcal", address)) => format!("https://{address}").into(),
            Some(_) => url.to_string().into(),
            None => format!("https://{url}").into(),
        }
    }

    fn address(url: &str) -> &str {
        url.split_once("://").map_or(url, |(_, address)| address)
    }

    fn host(url: &str) -> SharedString {
        Self::address(url)
            .split('/')
            .next()
            .unwrap_or_default()
            .to_string()
            .into()
    }
}
