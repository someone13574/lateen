use chrono::{DateTime, Local, NaiveDate, NaiveTime, TimeZone, Utc};
use chrono_tz::Tz;

use crate::ics::line::Line;

pub struct Moment {
    pub local: DateTime<Local>,
    pub all_day: bool,
}

impl Moment {
    pub fn parse(line: &Line) -> Option<Self> {
        Self::read(&line.value, line.parameter_value("TZID"))
    }

    pub fn each(line: &Line) -> Vec<Self> {
        line.value
            .split(',')
            .filter_map(|value| Self::read(value, line.parameter_value("TZID")))
            .collect()
    }

    pub fn read(value: &str, zone: Option<&str>) -> Option<Self> {
        let date = NaiveDate::parse_from_str(value.get(..8)?, "%Y%m%d").ok()?;
        let Some(clock) = value.get(9..15) else {
            return Some(Self {
                local: Self::at(date, NaiveTime::MIN, None)?,
                all_day: true,
            });
        };
        let time = NaiveTime::parse_from_str(clock, "%H%M%S").ok()?;
        let local = match value.ends_with('Z') {
            true => Utc.from_utc_datetime(&date.and_time(time)).into(),
            false => Self::at(date, time, zone)?,
        };

        Some(Self {
            local,
            all_day: false,
        })
    }

    fn at(date: NaiveDate, time: NaiveTime, zone: Option<&str>) -> Option<DateTime<Local>> {
        let naive = date.and_time(time);

        match zone.and_then(|name| name.parse::<Tz>().ok()) {
            Some(zone) => Some(
                zone.from_local_datetime(&naive)
                    .earliest()?
                    .with_timezone(&Local),
            ),
            None => Local.from_local_datetime(&naive).earliest(),
        }
    }
}
