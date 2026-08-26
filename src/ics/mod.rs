use std::ops::Range;

use chrono::{Datelike, Days, NaiveDate, TimeDelta, Timelike, Weekday};

use crate::block::Block;
use crate::ics::event::{Event, Repetition};
use crate::ics::line::Line;
use crate::ics::rule::{Rule, Weekly};
use crate::ics::zone::Zone;
use crate::subscription::{SubscribedEvent, SubscriptionId};
use crate::task::{Dates, Recurrence, Task};

mod event;
mod line;
mod moment;
mod rule;
mod zone;

pub struct Ics {
    name: Option<String>,
    events: Vec<Event>,
}

pub struct Import {
    pub name: Option<String>,
    pub tasks: Vec<Task>,
    pub dropped: usize,
}

impl Ics {
    const MARKER: i32 = 15;
    const WEEK: TimeDelta = TimeDelta::days(7);
    const UNTITLED: &'static str = "Untitled";

    pub fn parse(text: &str) -> Self {
        let mut ics = Self {
            name: None,
            events: Vec::new(),
        };
        let lines: Vec<_> = Line::unfold(text)
            .iter()
            .filter_map(|line| Line::parse(line))
            .collect();
        let zones = Zone::all(&lines);
        let mut current: Option<Vec<Line>> = None;
        let mut nested = 0;

        for line in lines {
            ics.read(line, &zones, &mut current, &mut nested);
        }

        ics
    }

    pub fn import(&self, subscription: SubscriptionId, today: NaiveDate, horizon: i32) -> Import {
        let mut import = Import {
            name: self.name.clone(),
            tasks: Vec::new(),
            dropped: 0,
        };

        for (position, event) in self.events.iter().enumerate() {
            if self.superseded(event) || self.duplicate(event, position) {
                continue;
            }

            let source = SubscribedEvent {
                subscription,
                uid: event.uid.clone().into(),
            };

            match &event.repeats {
                Repetition::Unsupported => import.dropped += 1,
                Repetition::Once => {
                    let window = Self::window(event, today, horizon);

                    import
                        .tasks
                        .extend(Self::once(event, source, today, &window));
                }
                Repetition::Every(rule) => match self.repeating(event, rule, source, today) {
                    Some(task) if Self::ended(&task) => continue,
                    Some(task) => import.tasks.push(task),
                    None => import.dropped += 1,
                },
            }
        }

        import
    }

    fn ended(task: &Task) -> bool {
        task.dates.until.is_some_and(|until| until < 0)
    }

    fn repeating(
        &self,
        event: &Event,
        rule: &Rule,
        source: SubscribedEvent,
        today: NaiveDate,
    ) -> Option<Task> {
        let date = event.start.date_naive();
        let (recurrence, opening, days) = match rule.weekly(date) {
            Some(weekly) => {
                let recurrence = match weekly.interval {
                    1 => Recurrence::Weekly,
                    2 => Recurrence::Biweekly,
                    _ => return None,
                };

                (recurrence, Self::opening(&weekly, date)?, weekly.days)
            }
            None => (rule.cyclic()?, date, vec![date.weekday()]),
        };

        Some(
            Self::commitment(event, source, days)
                .recurring(recurrence)
                .spanning(Self::between(opening, rule.last(date), today))
                .without(self.skipped(event, today)),
        )
    }

    fn skipped(&self, event: &Event, today: NaiveDate) -> Vec<i32> {
        event
            .excluded
            .iter()
            .copied()
            .chain(self.moved(event))
            .map(|date| Self::offset(date, today))
            .collect()
    }

    fn moved(&self, event: &Event) -> Vec<NaiveDate> {
        self.instances(&event.uid)
            .filter(|instance| !Self::matches(event, instance))
            .filter_map(|instance| instance.instance)
            .collect()
    }

    fn superseded(&self, event: &Event) -> bool {
        event.instance.is_some()
            && self
                .series(&event.uid)
                .is_some_and(|series| Self::matches(series, event))
    }

    fn duplicate(&self, event: &Event, position: usize) -> bool {
        self.events[..position]
            .iter()
            .any(|other| other.uid == event.uid && other.instance == event.instance)
    }

    fn series(&self, uid: &str) -> Option<&Event> {
        self.events
            .iter()
            .find(|event| event.uid == uid && event.instance.is_none())
    }

    fn instances(&self, uid: &str) -> impl Iterator<Item = &Event> {
        self.events
            .iter()
            .filter(move |event| event.uid == uid && event.instance.is_some())
    }

    fn matches(series: &Event, instance: &Event) -> bool {
        let Some(date) = instance.instance else {
            return false;
        };

        instance.start.date_naive() == date
            && instance.start.time() == series.start.time()
            && instance.minutes == series.minutes
            && instance.place == series.place
            && instance.summary == series.summary
    }

    fn once(
        event: &Event,
        source: SubscribedEvent,
        today: NaiveDate,
        window: &Range<NaiveDate>,
    ) -> Option<Task> {
        let date = event.start.date_naive();

        window
            .contains(&date)
            .then(|| Self::occurrence(event, source, date, today))
    }

    fn occurrence(
        event: &Event,
        source: SubscribedEvent,
        date: NaiveDate,
        today: NaiveDate,
    ) -> Task {
        let day = Self::offset(date, today);
        let dates = Dates {
            from: Some(day),
            until: Some(day),
        };

        Self::commitment(event, source, vec![date.weekday()])
            .spanning(dates)
            .recurring(Recurrence::Never)
    }

    fn commitment(event: &Event, source: SubscribedEvent, days: Vec<Weekday>) -> Task {
        let start = match event.all_day {
            true => 0,
            false => event.start.num_seconds_from_midnight() as i32 / 60,
        };
        let title = match event.summary.is_empty() {
            true => Self::UNTITLED,
            false => &event.summary,
        };
        let duration = match event.all_day {
            true => event.minutes.max(Block::MINUTES_PER_DAY),
            false if event.minutes <= 0 => Self::MARKER,
            false => event.minutes,
        };
        let task = Task::fixed(title, days, start, duration)
            .managed(source)
            .all_day(event.all_day);

        match &event.place {
            Some(place) => task.at(place.clone()),
            None => task,
        }
    }

    fn window(event: &Event, today: NaiveDate, horizon: i32) -> Range<NaiveDate> {
        let spans = (event.minutes / Block::MINUTES_PER_DAY).max(0) as u64;
        let first = today.checked_sub_days(Days::new(spans)).unwrap_or(today);

        first..today + Days::new(horizon as u64)
    }

    fn between(opening: NaiveDate, until: Option<NaiveDate>, today: NaiveDate) -> Dates {
        Dates {
            from: Some(Self::offset(opening, today)),
            until: until.map(|last| Self::offset(last, today)),
        }
    }

    fn offset(date: NaiveDate, today: NaiveDate) -> i32 {
        (date - today).num_days() as i32
    }

    fn opening(weekly: &Weekly, start: NaiveDate) -> Option<NaiveDate> {
        let openings: Vec<_> = weekly
            .days
            .iter()
            .filter_map(|weekday| weekly.opening(*weekday, start))
            .collect();
        let opening = openings.iter().copied().min()?;
        let spread = openings.iter().copied().max()? - opening;

        (spread < Self::WEEK).then_some(opening)
    }

    fn read(
        &mut self,
        line: Line,
        zones: &[Zone],
        current: &mut Option<Vec<Line>>,
        nested: &mut i32,
    ) {
        match (line.name.as_str(), line.value.as_str()) {
            ("BEGIN", "VEVENT") => {
                *current = Some(Vec::new());
                *nested = 0;
            }
            ("END", "VEVENT") => {
                if let Some(lines) = current.take() {
                    self.events.extend(Event::read(&lines, zones));
                }
            }
            ("BEGIN", _) => *nested += 1,
            ("END", _) => *nested -= 1,
            ("X-WR-CALNAME", _) => self.name = Some(line.text()),
            _ => {
                if let (0, Some(lines)) = (*nested, current) {
                    lines.push(line);
                }
            }
        }
    }
}
