use chrono::{DateTime, Local, NaiveDate};

use crate::ics::line::Line;
use crate::ics::moment::Moment;
use crate::ics::rule::Rule;
use crate::ics::zone::Zone;

pub enum Repetition {
    Once,
    Every(Rule),
    Unsupported,
}

pub struct Event {
    pub uid: String,
    pub summary: String,
    pub place: Option<String>,
    pub start: DateTime<Local>,
    pub all_day: bool,
    pub minutes: i32,
    pub repeats: Repetition,
    pub excluded: Vec<NaiveDate>,
    pub instance: Option<NaiveDate>,
}

impl Event {
    pub fn read(lines: &[Line], zones: &[Zone]) -> Option<Self> {
        if Self::value(lines, "STATUS").as_deref() == Some("CANCELLED") {
            return None;
        }

        let start = Self::find(lines, "DTSTART").and_then(|line| Moment::parse(line, zones))?;
        let summary = Self::value(lines, "SUMMARY").unwrap_or_default();

        Some(Self {
            uid: Self::identity(lines, &summary, &start),
            summary,
            place: Self::value(lines, "LOCATION").filter(|place| !place.is_empty()),
            minutes: Self::length(lines, &start, zones),
            all_day: start.all_day,
            start: start.local,
            repeats: Self::repetition(lines),
            excluded: Self::excluded(lines, zones),
            instance: Self::find(lines, "RECURRENCE-ID")
                .and_then(|line| Moment::parse(line, zones))
                .map(|moment| moment.local.date_naive()),
        })
    }

    fn identity(lines: &[Line], summary: &str, start: &Moment) -> String {
        Self::value(lines, "UID")
            .filter(|uid| !uid.is_empty())
            .unwrap_or_else(|| format!("{summary}@{}", start.local))
    }

    fn find<'a>(lines: &'a [Line], name: &str) -> Option<&'a Line> {
        lines.iter().find(|line| line.name == name)
    }

    fn value(lines: &[Line], name: &str) -> Option<String> {
        Self::find(lines, name).map(Line::text)
    }

    fn repetition(lines: &[Line]) -> Repetition {
        let Some(line) = Self::find(lines, "RRULE") else {
            return Repetition::Once;
        };

        match Rule::parse(line) {
            Some(rule) => Repetition::Every(rule),
            None => Repetition::Unsupported,
        }
    }

    fn length(lines: &[Line], start: &Moment, zones: &[Zone]) -> i32 {
        if let Some(end) = Self::find(lines, "DTEND").and_then(|line| Moment::parse(line, zones)) {
            return (end.local - start.local).num_minutes() as i32;
        }

        Self::value(lines, "DURATION")
            .and_then(|value| Self::duration(&value))
            .unwrap_or_default()
    }

    fn excluded(lines: &[Line], zones: &[Zone]) -> Vec<NaiveDate> {
        lines
            .iter()
            .filter(|line| line.name == "EXDATE")
            .flat_map(|line| Moment::each(line, zones))
            .map(|moment| moment.local.date_naive())
            .collect()
    }

    fn duration(value: &str) -> Option<i32> {
        let mut minutes = 0;
        let mut number = 0;

        for character in value.chars() {
            let unit = match character {
                '0'..='9' => {
                    number = number * 10 + character.to_digit(10)? as i32;
                    continue;
                }
                'W' => 7 * 24 * 60,
                'D' => 24 * 60,
                'H' => 60,
                'M' => 1,
                'S' => 0,
                'P' | 'T' => continue,
                _ => return None,
            };

            minutes += number * unit;
            number = 0;
        }

        Some(minutes)
    }
}
