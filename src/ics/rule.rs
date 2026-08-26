use std::str::FromStr;

use chrono::{Datelike, Days, Months, NaiveDate, Weekday};

use crate::ics::line::Line;
use crate::ics::moment::Moment;
use crate::task::Recurrence;

enum Frequency {
    Daily,
    Weekly,
    Monthly,
    Yearly,
}

pub struct Weekly {
    pub interval: u32,
    pub days: Vec<Weekday>,
    pub week_starts: Weekday,
}

impl Weekly {
    pub fn opening(&self, weekday: Weekday, start: NaiveDate) -> Option<NaiveDate> {
        let week = start - Days::new(u64::from(start.weekday().days_since(self.week_starts)));
        let opening =
            week.checked_add_days(Days::new(u64::from(weekday.days_since(self.week_starts))))?;

        match opening >= start {
            true => Some(opening),
            false => opening.checked_add_days(Days::new(7 * u64::from(self.interval))),
        }
    }
}

pub struct Rule {
    frequency: Frequency,
    interval: u32,
    week_starts: Weekday,
    weekdays: Vec<Weekday>,
    ordinals: Vec<(i32, Weekday)>,
    monthdays: Vec<i32>,
    months: Vec<u32>,
    count: Option<u32>,
    until: Option<NaiveDate>,
}

impl Rule {
    const PERIODS: u32 = 40_000;

    pub fn parse(line: &Line) -> Option<Self> {
        let mut rule = Self::empty();
        let mut repeats = false;

        for part in line.value.split(';') {
            let (name, value) = part.split_once('=')?;

            match name.to_ascii_uppercase().as_str() {
                "FREQ" => {
                    rule.frequency = Self::frequency(value)?;
                    repeats = true;
                }
                "INTERVAL" => rule.interval = value.parse().ok()?,
                "COUNT" => rule.count = Some(value.parse().ok()?),
                "UNTIL" => rule.until = Some(Self::until(value)?),
                "BYDAY" => rule.set_days(value)?,
                "BYMONTHDAY" => rule.monthdays = Self::numbers(value)?,
                "BYMONTH" => rule.months = Self::numbers(value)?,
                "WKST" => rule.week_starts = Self::weekday(value)?,
                _ => return None,
            }
        }

        repeats.then_some(rule)
    }

    pub fn weekly(&self, start: NaiveDate) -> Option<Weekly> {
        if !self.ordinals.is_empty() || !self.monthdays.is_empty() || !self.months.is_empty() {
            return None;
        }

        let every_weekday = match self.frequency {
            Frequency::Daily if self.interval == 1 => vec![
                Weekday::Mon,
                Weekday::Tue,
                Weekday::Wed,
                Weekday::Thu,
                Weekday::Fri,
                Weekday::Sat,
                Weekday::Sun,
            ],
            Frequency::Weekly => vec![start.weekday()],
            _ => return None,
        };

        Some(Weekly {
            interval: self.interval,
            days: match self.weekdays.is_empty() {
                true => every_weekday,
                false => self.weekdays.clone(),
            },
            week_starts: self.week_starts,
        })
    }

    pub fn cyclic(&self) -> Option<Recurrence> {
        if self.interval != 1
            || !self.ordinals.is_empty()
            || self.monthdays.iter().any(|day| *day < 0)
        {
            return None;
        }

        match self.frequency {
            Frequency::Monthly => Some(Recurrence::Monthly),
            Frequency::Yearly => Some(Recurrence::Yearly),
            Frequency::Daily | Frequency::Weekly => None,
        }
    }

    pub fn last(&self, start: NaiveDate) -> Option<NaiveDate> {
        match (self.until, self.count) {
            (Some(until), _) => Some(until),
            (None, Some(_)) => self.occurrences(start, None).last().copied(),
            (None, None) => None,
        }
    }

    pub fn through(&self, start: NaiveDate, limit: NaiveDate) -> Vec<NaiveDate> {
        self.occurrences(start, Some(limit))
    }

    fn occurrences(&self, start: NaiveDate, limit: Option<NaiveDate>) -> Vec<NaiveDate> {
        let mut dates = Vec::new();
        let mut counted = 0;

        for period in 0..Self::PERIODS {
            let Some(anchor) = self.period(start, period) else {
                break;
            };

            if limit.is_some_and(|edge| anchor > edge) {
                break;
            }

            for date in self.within(anchor, start) {
                if date < start {
                    continue;
                }

                counted += 1;

                if self.exhausted(date, counted) {
                    return dates;
                }
                if limit.is_none_or(|edge| date <= edge) {
                    dates.push(date);
                }
            }
        }

        dates
    }

    fn exhausted(&self, date: NaiveDate, counted: u32) -> bool {
        self.until.is_some_and(|until| date > until)
            || self.count.is_some_and(|count| counted > count)
    }

    fn period(&self, start: NaiveDate, period: u32) -> Option<NaiveDate> {
        let steps = self.interval.checked_mul(period)?;

        match self.frequency {
            Frequency::Daily => start.checked_add_days(Days::new(u64::from(steps))),
            Frequency::Weekly => self
                .week_start(start)
                .checked_add_days(Days::new(u64::from(steps) * 7)),
            Frequency::Monthly => start.with_day(1)?.checked_add_months(Months::new(steps)),
            Frequency::Yearly => start
                .with_day(1)?
                .with_month(1)?
                .checked_add_months(Months::new(steps.checked_mul(12)?)),
        }
    }

    fn within(&self, anchor: NaiveDate, start: NaiveDate) -> Vec<NaiveDate> {
        match self.frequency {
            Frequency::Daily => match self.admits(anchor) {
                true => vec![anchor],
                false => Vec::new(),
            },
            Frequency::Weekly => self.weekdays_of(anchor, start),
            Frequency::Monthly => self.days_of(anchor, start),
            Frequency::Yearly => self.months_of(anchor, start),
        }
    }

    fn admits(&self, date: NaiveDate) -> bool {
        (self.weekdays.is_empty() || self.weekdays.contains(&date.weekday()))
            && (self.months.is_empty() || self.months.contains(&date.month()))
    }

    fn week_start(&self, date: NaiveDate) -> NaiveDate {
        date - Days::new(u64::from(date.weekday().days_since(self.week_starts)))
    }

    fn weekdays_of(&self, week: NaiveDate, start: NaiveDate) -> Vec<NaiveDate> {
        let days = match self.weekdays.is_empty() {
            true => vec![start.weekday()],
            false => self.weekdays.clone(),
        };
        let mut dates: Vec<_> = days
            .iter()
            .filter_map(|weekday| {
                week.checked_add_days(Days::new(u64::from(weekday.days_since(self.week_starts))))
            })
            .filter(|date| self.months.is_empty() || self.months.contains(&date.month()))
            .collect();

        dates.sort();

        dates
    }

    fn days_of(&self, month: NaiveDate, start: NaiveDate) -> Vec<NaiveDate> {
        let length = Self::month_length(month);
        let mut dates: Vec<_> = match (self.ordinals.is_empty(), self.monthdays.is_empty()) {
            (false, _) => self
                .ordinals
                .iter()
                .filter_map(|(ordinal, weekday)| Self::nth(month, *ordinal, *weekday))
                .collect(),
            (true, false) => self
                .monthdays
                .iter()
                .filter_map(|day| month.with_day(Self::day_number(*day, length)?))
                .collect(),
            (true, true) => month.with_day(start.day()).into_iter().collect(),
        };

        dates.sort();
        dates.dedup();

        dates
    }

    fn months_of(&self, year: NaiveDate, start: NaiveDate) -> Vec<NaiveDate> {
        let months = match self.months.is_empty() {
            true => vec![start.month()],
            false => self.months.clone(),
        };
        let mut dates: Vec<_> = months
            .iter()
            .filter_map(|month| year.with_month(*month))
            .flat_map(|month| self.days_of(month, start))
            .collect();

        dates.sort();

        dates
    }

    fn nth(month: NaiveDate, ordinal: i32, weekday: Weekday) -> Option<NaiveDate> {
        let matching: Vec<_> = month
            .iter_days()
            .take(Self::month_length(month) as usize)
            .filter(|date| date.weekday() == weekday)
            .collect();
        let index = match ordinal > 0 {
            true => ordinal - 1,
            false => matching.len() as i32 + ordinal,
        };

        matching.get(usize::try_from(index).ok()?).copied()
    }

    fn month_length(month: NaiveDate) -> u32 {
        month
            .checked_add_months(Months::new(1))
            .map_or(31, |next| (next - month).num_days() as u32)
    }

    fn day_number(day: i32, length: u32) -> Option<u32> {
        let number = match day > 0 {
            true => day,
            false => length as i32 + 1 + day,
        };

        (number >= 1 && number as u32 <= length).then_some(number as u32)
    }

    fn empty() -> Self {
        Self {
            frequency: Frequency::Daily,
            interval: 1,
            week_starts: Weekday::Mon,
            weekdays: Vec::new(),
            ordinals: Vec::new(),
            monthdays: Vec::new(),
            months: Vec::new(),
            count: None,
            until: None,
        }
    }

    fn frequency(value: &str) -> Option<Frequency> {
        match value.to_ascii_uppercase().as_str() {
            "DAILY" => Some(Frequency::Daily),
            "WEEKLY" => Some(Frequency::Weekly),
            "MONTHLY" => Some(Frequency::Monthly),
            "YEARLY" => Some(Frequency::Yearly),
            _ => None,
        }
    }

    fn until(value: &str) -> Option<NaiveDate> {
        Some(Moment::read(value, None, &[])?.local.date_naive())
    }

    fn numbers<T: FromStr>(value: &str) -> Option<Vec<T>> {
        value.split(',').map(|number| number.parse().ok()).collect()
    }

    fn set_days(&mut self, value: &str) -> Option<()> {
        for day in value.split(',') {
            let split = day.len().checked_sub(2)?;
            let weekday = Self::weekday(day.get(split..)?)?;

            match day.get(..split)? {
                "" => self.weekdays.push(weekday),
                ordinal => self.ordinals.push((ordinal.parse().ok()?, weekday)),
            }
        }

        Some(())
    }

    fn weekday(value: &str) -> Option<Weekday> {
        match value.to_ascii_uppercase().as_str() {
            "MO" => Some(Weekday::Mon),
            "TU" => Some(Weekday::Tue),
            "WE" => Some(Weekday::Wed),
            "TH" => Some(Weekday::Thu),
            "FR" => Some(Weekday::Fri),
            "SA" => Some(Weekday::Sat),
            "SU" => Some(Weekday::Sun),
            _ => None,
        }
    }
}
