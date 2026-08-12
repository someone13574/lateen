use std::ops::Range;

use chrono::Weekday;
use gpui::SharedString;

use crate::theme::BlockColor;

pub struct Task {
    pub title: SharedString,
    pub place: Option<SharedString>,
    pub color: Option<BlockColor>,
    pub days: Vec<Weekday>,
    pub priority: Priority,
    pub prep: i32,
    pub cleanup: i32,
    pub kind: TaskKind,
}

pub enum TaskKind {
    Fixed { start: i32, duration: i32 },
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
                Flexible::new(90, 11 * 60 + 40, 22 * 60)
                    .sessions(20, 30, 45)
                    .breaks(25, 5),
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
                Flexible::new(300, 9 * 60, 18 * 60)
                    .due(0, 4)
                    .sessions(45, 90, 120)
                    .breaks(50, 10),
            )
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

    pub fn fixed(
        title: impl Into<SharedString>,
        days: Vec<Weekday>,
        start: i32,
        duration: i32,
    ) -> Self {
        Self::new(title, days, TaskKind::Fixed { start, duration })
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

    pub fn window(&self) -> Range<i32> {
        match &self.kind {
            TaskKind::Fixed { start, duration } => *start..*start + *duration,
            TaskKind::Flexible(flexible) => flexible.window.clone(),
        }
    }

    fn new(title: impl Into<SharedString>, days: Vec<Weekday>, kind: TaskKind) -> Self {
        Self {
            title: title.into(),
            place: None,
            color: None,
            days,
            priority: Priority::Normal,
            prep: 0,
            cleanup: 0,
            kind,
        }
    }
}

pub struct Flexible {
    pub total: i32,
    pub repeat: Repeat,
    pub window: Range<i32>,
    pub sessions: Option<Sessions>,
    pub breaks: Option<Breaks>,
}

impl Flexible {
    pub fn new(total: i32, opens: i32, closes: i32) -> Self {
        Self {
            total,
            repeat: Repeat::Daily,
            window: opens..closes,
            sessions: None,
            breaks: None,
        }
    }

    pub fn due(mut self, earliest_day: i32, deadline_day: i32) -> Self {
        self.repeat = Repeat::Once {
            earliest_day,
            deadline_day,
        };
        self
    }

    pub fn sessions(mut self, shortest: i32, preferred: i32, longest: i32) -> Self {
        self.sessions = Some(Sessions {
            shortest,
            preferred,
            longest,
        });
        self
    }

    pub fn breaks(mut self, every: i32, minutes: i32) -> Self {
        self.breaks = Some(Breaks { every, minutes });
        self
    }
}

pub enum Repeat {
    Daily,
    Once {
        earliest_day: i32,
        deadline_day: i32,
    },
}

#[derive(Clone, Copy)]
pub struct Sessions {
    pub shortest: i32,
    pub preferred: i32,
    pub longest: i32,
}

#[derive(Clone, Copy)]
pub struct Breaks {
    pub every: i32,
    pub minutes: i32,
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Priority {
    Lowest,
    Low,
    Normal,
    High,
    Highest,
}
