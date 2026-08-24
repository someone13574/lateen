use std::ops::Range;
use std::sync::atomic::{AtomicU64, Ordering};

use chrono::{Datelike, NaiveDate, Weekday};
use gpui::SharedString;
use serde::{Deserialize, Serialize};

use crate::block::Block;
use crate::subscription::SubscribedEvent;
use crate::theme::BlockColor;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TaskId(u64);

static NEXT_TASK_ID: AtomicU64 = AtomicU64::new(0);

impl TaskId {
    pub fn reserve(self) {
        NEXT_TASK_ID.fetch_max(self.0 + 1, Ordering::Relaxed);
    }

    fn next() -> Self {
        Self(NEXT_TASK_ID.fetch_add(1, Ordering::Relaxed))
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: TaskId,
    pub title: SharedString,
    pub place: Option<SharedString>,
    pub color: Option<BlockColor>,
    pub days: Vec<Weekday>,
    pub priority: Priority,
    pub prep: i32,
    pub cleanup: i32,
    pub dates: Dates,
    pub kind: TaskKind,
    #[serde(default)]
    pub cancelled: Vec<i32>,
    #[serde(default)]
    pub source: Option<SubscribedEvent>,
}

#[derive(Clone, Copy, Default, Serialize, Deserialize)]
pub struct Dates {
    pub from: Option<i32>,
    pub until: Option<i32>,
}

impl Dates {
    const FURTHEST: i32 = i32::MAX / Block::MINUTES_PER_DAY - 1;

    pub fn covers(self, day: i32) -> bool {
        self.from.is_none_or(|from| day >= from) && self.until.is_none_or(|until| day <= until)
    }

    pub fn first(self) -> i32 {
        self.from.unwrap_or(0)
    }

    pub fn span(self) -> Range<i32> {
        let first = self.first().clamp(-Self::FURTHEST, Self::FURTHEST);
        let last = self.until.unwrap_or(first).clamp(first, Self::FURTHEST);

        first..last + 1
    }

    pub fn shift(&mut self, days: i32) {
        self.from = self.from.map(|from| from - days);
        self.until = self.until.map(|until| until - days);
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub enum TaskKind {
    Fixed {
        start: i32,
        duration: i32,
        recurrence: Recurrence,
        overrun_percent: i32,
    },
    Flexible(Flexible),
}

impl Task {
    pub fn seed() -> Vec<Self> {
        let weekdays = || {
            vec![
                Weekday::Mon,
                Weekday::Tue,
                Weekday::Wed,
                Weekday::Thu,
                Weekday::Fri,
            ]
        };
        let every_day = || {
            let mut days = weekdays();
            days.extend([Weekday::Sat, Weekday::Sun]);
            days
        };

        vec![
            Self::fixed(
                "Calculus III lecture",
                vec![Weekday::Mon, Weekday::Wed, Weekday::Fri],
                10 * 60,
                90,
            )
            .at("Maths 210")
            .priority(Priority::High)
            .transitions(20, 5),
            Self::fixed("Physics lab", vec![Weekday::Tue], 14 * 60, 180)
                .at("Science block B")
                .priority(Priority::High)
                .transitions(20, 10),
            Self::fixed("Research standup", weekdays(), 9 * 60 + 15, 15),
            Self::flexible(
                "Study, Calculus III",
                weekdays(),
                Flexible::new(90, 11 * 60 + 40, 22 * 60).sessions(20, 30, 45),
            )
            .priority(Priority::Low)
            .transitions(5, 5),
            Self::flexible(
                "Lunch",
                every_day(),
                Flexible::new(15, 11 * 60 + 30, 13 * 60 + 30),
            )
            .priority(Priority::Lowest)
            .transitions(2, 3),
            Self::flexible(
                "Thesis draft, chapter 2",
                weekdays(),
                Flexible::new(300, 9 * 60, 18 * 60).sessions(45, 90, 120),
            )
            .due(0, 4)
            .priority(Priority::Highest)
            .transitions(5, 5),
            Self::flexible(
                "Strength session",
                vec![Weekday::Mon, Weekday::Wed, Weekday::Fri],
                Flexible::new(60, 17 * 60, 21 * 60),
            )
            .transitions(20, 20),
            Self::flexible(
                "Reading",
                every_day(),
                Flexible::new(45, 20 * 60, 23 * 60 + 20).sessions(15, 25, 45),
            )
            .priority(Priority::Lowest),
        ]
    }

    pub fn draft() -> Self {
        Self::flexible(
            "New commitment",
            vec![
                Weekday::Mon,
                Weekday::Tue,
                Weekday::Wed,
                Weekday::Thu,
                Weekday::Fri,
                Weekday::Sat,
                Weekday::Sun,
            ],
            Flexible::new(30, 9 * 60, 22 * 60),
        )
    }

    pub fn fixed(
        title: impl Into<SharedString>,
        days: Vec<Weekday>,
        start: i32,
        duration: i32,
    ) -> Self {
        Self::new(
            title,
            days,
            TaskKind::Fixed {
                start,
                duration,
                recurrence: Recurrence::Weekly,
                overrun_percent: 0,
            },
        )
    }

    pub fn flexible(
        title: impl Into<SharedString>,
        days: Vec<Weekday>,
        flexible: Flexible,
    ) -> Self {
        Self::new(title, days, TaskKind::Flexible(flexible))
    }

    pub fn at(mut self, place: impl Into<SharedString>) -> Self {
        self.place = Some(place.into());
        self
    }

    pub fn priority(mut self, priority: Priority) -> Self {
        self.priority = priority;
        self
    }

    pub fn transitions(mut self, prep: i32, cleanup: i32) -> Self {
        self.prep = prep;
        self.cleanup = cleanup;
        self
    }

    pub fn spanning(mut self, dates: Dates) -> Self {
        self.dates = dates;
        self
    }

    pub fn without(mut self, cancelled: Vec<i32>) -> Self {
        self.cancelled = cancelled;
        self
    }

    pub fn shift(&mut self, days: i32) {
        self.dates.shift(days);

        for day in &mut self.cancelled {
            *day -= days;
        }
    }

    pub fn covers(&self, day: i32) -> bool {
        self.dates.covers(day) && !self.cancelled.contains(&day)
    }

    pub fn recurring(mut self, recurrence: Recurrence) -> Self {
        if let TaskKind::Fixed {
            recurrence: current,
            ..
        } = &mut self.kind
        {
            *current = recurrence;
        }

        self
    }

    pub fn managed(mut self, source: SubscribedEvent) -> Self {
        self.source = Some(source);
        self
    }

    pub fn keep_unmanaged(&mut self, previous: &Self) {
        self.color = previous.color;
        self.priority = previous.priority;
        self.prep = previous.prep;
        self.cleanup = previous.cleanup;

        if let (
            TaskKind::Fixed {
                overrun_percent, ..
            },
            TaskKind::Fixed {
                overrun_percent: kept,
                ..
            },
        ) = (&mut self.kind, &previous.kind)
        {
            *overrun_percent = *kept;
        }
    }

    pub fn run(&self, day: i32, today: NaiveDate) -> Range<i32> {
        let TaskKind::Flexible(flexible) = &self.kind else {
            return day * Block::MINUTES_PER_DAY..(day + 1) * Block::MINUTES_PER_DAY;
        };

        match flexible.repeat {
            Repeat::Never => {
                let span = self.dates.span();

                span.start * Block::MINUTES_PER_DAY..span.end * Block::MINUTES_PER_DAY
            }
            repeat => {
                let cycle = repeat.cycle();
                let opens = day - (today.num_days_from_ce() + day).rem_euclid(cycle);

                opens * Block::MINUTES_PER_DAY..(opens + cycle) * Block::MINUTES_PER_DAY
            }
        }
    }

    pub fn splittable(&self) -> bool {
        matches!(&self.kind, TaskKind::Flexible(flexible) if flexible.sessions.is_some())
    }

    pub fn window(&self) -> Range<i32> {
        match &self.kind {
            TaskKind::Fixed {
                start, duration, ..
            } => *start..*start + *duration,
            TaskKind::Flexible(flexible) => flexible.window.clone(),
        }
    }

    pub fn repeats(&self) -> bool {
        match &self.kind {
            TaskKind::Fixed { recurrence, .. } => !matches!(recurrence, Recurrence::Never),
            TaskKind::Flexible(flexible) => !matches!(flexible.repeat, Repeat::Never),
        }
    }

    pub fn repeats_by_weekday(&self) -> bool {
        match &self.kind {
            TaskKind::Fixed { recurrence, .. } => {
                matches!(recurrence, Recurrence::Weekly | Recurrence::Biweekly)
            }
            TaskKind::Flexible(flexible) => !matches!(flexible.repeat, Repeat::Never),
        }
    }

    pub fn due(mut self, from: i32, until: i32) -> Self {
        self.dates = Dates {
            from: Some(from),
            until: Some(until),
        };

        if let TaskKind::Flexible(flexible) = &mut self.kind {
            flexible.repeat = Repeat::Never;
        }

        self
    }

    fn new(title: impl Into<SharedString>, days: Vec<Weekday>, kind: TaskKind) -> Self {
        Self {
            id: TaskId::next(),
            title: title.into(),
            place: None,
            color: None,
            days,
            priority: Priority::Normal,
            prep: 0,
            cleanup: 0,
            dates: Dates::default(),
            kind,
            cancelled: Vec::new(),
            source: None,
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Flexible {
    pub total: i32,
    pub repeat: Repeat,
    pub window: Range<i32>,
    pub sessions: Option<Sessions>,
}

impl Flexible {
    pub fn new(total: i32, opens: i32, closes: i32) -> Self {
        Self {
            total,
            repeat: Repeat::Daily,
            window: opens..closes,
            sessions: None,
        }
    }

    pub fn sessions(mut self, shortest: i32, preferred: i32, longest: i32) -> Self {
        self.sessions = Some(Sessions {
            shortest,
            preferred,
            longest,
        });
        self
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Repeat {
    Never,
    Daily,
    Weekly,
    Biweekly,
    Monthly,
    Yearly,
}

impl Repeat {
    pub fn cycle(self) -> i32 {
        match self {
            Self::Never | Self::Daily => 1,
            Self::Weekly => 7,
            Self::Biweekly => 14,
            Self::Monthly => 30,
            Self::Yearly => 365,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Recurrence {
    Never,
    Weekly,
    Biweekly,
    Monthly,
    Yearly,
}

#[derive(Clone, Copy, Serialize, Deserialize)]
pub struct Sessions {
    pub shortest: i32,
    pub preferred: i32,
    pub longest: i32,
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Priority {
    Lowest,
    Low,
    Normal,
    High,
    Highest,
}
