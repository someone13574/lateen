use chrono::{FixedOffset, NaiveDateTime};

use crate::ics::line::Line;
use crate::ics::rule::Rule;

pub struct Zone {
    id: String,
    shifts: Vec<Shift>,
}

impl Zone {
    pub fn all(lines: &[Line]) -> Vec<Self> {
        let mut zones = Vec::new();
        let mut opened = None;

        for (position, line) in lines.iter().enumerate() {
            match (line.name.as_str(), line.value.as_str()) {
                ("BEGIN", "VTIMEZONE") => opened = Some(position + 1),
                ("END", "VTIMEZONE") => {
                    if let Some(start) = opened.take() {
                        zones.extend(Self::read(&lines[start..position]));
                    }
                }
                _ => {}
            }
        }

        zones
    }

    pub fn named(&self, name: &str) -> bool {
        self.id == name
    }

    pub fn offset(&self, naive: NaiveDateTime) -> Option<FixedOffset> {
        self.shifts
            .iter()
            .filter_map(|shift| Some((shift.latest(naive)?, shift.to)))
            .max_by_key(|(starts, _)| *starts)
            .map(|(_, to)| to)
    }

    fn read(lines: &[Line]) -> Option<Self> {
        let id = lines.iter().find(|line| line.name == "TZID")?.text();
        let mut shifts = Vec::new();
        let mut opened = None;

        for (position, line) in lines.iter().enumerate() {
            match (line.name.as_str(), line.value.as_str()) {
                ("BEGIN", "STANDARD" | "DAYLIGHT") => opened = Some(position + 1),
                ("END", "STANDARD" | "DAYLIGHT") => {
                    if let Some(start) = opened.take() {
                        shifts.extend(Shift::read(&lines[start..position]));
                    }
                }
                _ => {}
            }
        }

        (!shifts.is_empty()).then_some(Self { id, shifts })
    }
}

struct Shift {
    starts: NaiveDateTime,
    to: FixedOffset,
    repeats: Option<Rule>,
}

impl Shift {
    fn read(lines: &[Line]) -> Option<Self> {
        let start = &Self::find(lines, "DTSTART")?.value;

        Some(Self {
            starts: NaiveDateTime::parse_from_str(start, "%Y%m%dT%H%M%S").ok()?,
            to: Self::offset(&Self::find(lines, "TZOFFSETTO")?.value)?,
            repeats: Self::find(lines, "RRULE").and_then(Rule::parse),
        })
    }

    fn latest(&self, naive: NaiveDateTime) -> Option<NaiveDateTime> {
        let Some(rule) = &self.repeats else {
            return (self.starts <= naive).then_some(self.starts);
        };

        rule.through(self.starts.date(), naive.date())
            .iter()
            .map(|date| date.and_time(self.starts.time()))
            .rfind(|moment| *moment <= naive)
    }

    fn find<'a>(lines: &'a [Line], name: &str) -> Option<&'a Line> {
        lines.iter().find(|line| line.name == name)
    }

    fn offset(value: &str) -> Option<FixedOffset> {
        let (sign, digits) = match value.strip_prefix('-') {
            Some(digits) => (-1, digits),
            None => (1, value.strip_prefix('+').unwrap_or(value)),
        };
        let hours: i32 = digits.get(..2)?.parse().ok()?;
        let minutes: i32 = digits.get(2..4)?.parse().ok()?;
        let seconds: i32 = digits
            .get(4..6)
            .and_then(|text| text.parse().ok())
            .unwrap_or_default();

        FixedOffset::east_opt(sign * (hours * 3600 + minutes * 60 + seconds))
    }
}
