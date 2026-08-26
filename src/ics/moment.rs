use chrono::{DateTime, FixedOffset, Local, NaiveDate, NaiveDateTime, NaiveTime, TimeZone, Utc};
use chrono_tz::Tz;

use crate::ics::line::Line;
use crate::ics::zone::Zone;

pub struct Moment {
    pub local: DateTime<Local>,
    pub all_day: bool,
}

impl Moment {
    pub fn parse(line: &Line, zones: &[Zone]) -> Option<Self> {
        Self::read(&line.value, line.parameter_value("TZID"), zones)
    }

    pub fn each(line: &Line, zones: &[Zone]) -> Vec<Self> {
        line.value
            .split(',')
            .filter_map(|value| Self::read(value, line.parameter_value("TZID"), zones))
            .collect()
    }

    pub fn read(value: &str, name: Option<&str>, zones: &[Zone]) -> Option<Self> {
        let date = NaiveDate::parse_from_str(value.get(..8)?, "%Y%m%d").ok()?;
        let Some(clock) = value.get(9..15) else {
            return Some(Self {
                local: Self::resolve(date, NaiveTime::MIN, None, zones)?,
                all_day: true,
            });
        };
        let time = NaiveTime::parse_from_str(clock, "%H%M%S").ok()?;
        let local = match value.ends_with('Z') {
            true => Utc.from_utc_datetime(&date.and_time(time)).into(),
            false => Self::resolve(date, time, name, zones)?,
        };

        Some(Self {
            local,
            all_day: false,
        })
    }

    fn resolve(
        date: NaiveDate,
        time: NaiveTime,
        name: Option<&str>,
        zones: &[Zone],
    ) -> Option<DateTime<Local>> {
        let naive = date.and_time(time);
        let Some(name) = name else {
            return Local.from_local_datetime(&naive).earliest();
        };

        if let Ok(zone) = name.parse::<Tz>() {
            return Some(
                zone.from_local_datetime(&naive)
                    .earliest()?
                    .with_timezone(&Local),
            );
        }

        match Self::declared(name, naive, zones) {
            Some(offset) => Some(offset.from_local_datetime(&naive).earliest()?.into()),
            None => Local.from_local_datetime(&naive).earliest(),
        }
    }

    fn declared(name: &str, naive: NaiveDateTime, zones: &[Zone]) -> Option<FixedOffset> {
        zones
            .iter()
            .find(|zone| zone.named(name))
            .and_then(|zone| zone.offset(naive))
    }
}
