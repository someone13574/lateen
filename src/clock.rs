use ashpd::desktop::settings::Settings;
use futures_lite::StreamExt;
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
