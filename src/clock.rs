use ashpd::desktop::settings::Settings;
use chrono::{DateTime, Duration, Local, NaiveTime, Timelike};
use futures_lite::StreamExt;
use gpui::prelude::*;
use gpui::{App, Global, TaskExt};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ClockFormat {
    TwelveHour,
    TwentyFourHour,
}

impl ClockFormat {
    const NAMESPACE: &str = "org.gnome.desktop.interface";
    const KEY: &str = "clock-format";

    pub fn init(cx: &mut App) {
        cx.set_global(Self::TwelveHour);

        cx.spawn(async move |cx| {
            let settings = Settings::new().await?;

            if let Ok(format) = settings.read::<String>(Self::NAMESPACE, Self::KEY).await {
                cx.update(|cx| Self::apply(&format, cx));
            }

            let mut changes = settings
                .receive_setting_changed_with_args::<String>(Self::NAMESPACE, Self::KEY)
                .await?;
            while let Some(change) = changes.next().await {
                if let Ok(format) = change {
                    cx.update(|cx| Self::apply(&format, cx));
                }
            }

            anyhow::Ok(())
        })
        .detach_and_log_err(cx);
    }

    pub fn time_label(self, minutes: i32) -> String {
        let minutes = minutes.rem_euclid(24 * 60);
        let (hour, minute) = (minutes / 60, minutes % 60);

        match self {
            Self::TwelveHour => {
                let clock = if hour % 12 == 0 { 12 } else { hour % 12 };
                let period = if hour < 12 { "am" } else { "pm" };

                format!("{clock}:{minute:02} {period}")
            }
            Self::TwentyFourHour => format!("{hour:02}:{minute:02}"),
        }
    }

    pub fn parse(text: &str) -> Option<i32> {
        let text = text.trim().to_lowercase();
        let (clock, midday) = match text.strip_suffix("am") {
            Some(clock) => (clock, Some(0)),
            None => match text.strip_suffix("pm") {
                Some(clock) => (clock, Some(12)),
                None => (text.as_str(), None),
            },
        };
        let (hour, minute) = match clock.trim().split_once(':') {
            Some((hour, minute)) => (
                hour.trim().parse::<i32>().ok()?,
                minute.trim().parse::<i32>().ok()?,
            ),
            None => (clock.trim().parse::<i32>().ok()?, 0),
        };
        let hour = match midday {
            Some(offset) => hour % 12 + offset,
            None => hour,
        };

        ((0..24).contains(&hour) && (0..60).contains(&minute)).then_some(hour * 60 + minute)
    }

    pub fn second_label(self, time: NaiveTime) -> String {
        let (hour, minute, second) = (time.hour(), time.minute(), time.second());

        match self {
            Self::TwelveHour => {
                let clock = if hour % 12 == 0 { 12 } else { hour % 12 };
                let period = if hour < 12 { "am" } else { "pm" };

                format!("{clock}:{minute:02}:{second:02} {period}")
            }
            Self::TwentyFourHour => format!("{hour:02}:{minute:02}:{second:02}"),
        }
    }

    pub fn hour_label(self, hour: usize) -> String {
        match self {
            Self::TwelveHour => {
                let clock = if hour.is_multiple_of(12) {
                    12
                } else {
                    hour % 12
                };
                let period = if hour < 12 { "am" } else { "pm" };

                format!("{clock}{period}")
            }
            Self::TwentyFourHour => format!("{hour:02}:00"),
        }
    }

    fn apply(format: &str, cx: &mut App) {
        cx.set_global(if format == "24h" {
            Self::TwentyFourHour
        } else {
            Self::TwelveHour
        });
        cx.refresh_windows();
    }
}

impl Global for ClockFormat {}

#[derive(Clone, Copy)]
pub struct Clock {
    offset: Duration,
}

impl Clock {
    pub fn init(cx: &mut App) {
        cx.set_global(Self {
            offset: Duration::zero(),
        });
    }

    pub fn now(&self) -> DateTime<Local> {
        Local::now() + self.offset
    }

    pub fn minute_of_day(&self) -> f32 {
        let now = self.now();
        let fraction = (now.nanosecond() % 1_000_000_000) as f32 / 1e9;

        (now.num_seconds_from_midnight() as f32 + fraction) / 60.0
    }

    pub fn travel(minutes: f32, cx: &mut App) {
        let step = Duration::milliseconds((minutes * 60_000.0) as i64);

        cx.update_global::<Self, _>(|clock, _cx| clock.offset += step);
        cx.refresh_windows();
    }

    pub fn reset(cx: &mut App) {
        cx.update_global::<Self, _>(|clock, _cx| clock.offset = Duration::zero());
        cx.refresh_windows();
    }
}

impl Global for Clock {}
