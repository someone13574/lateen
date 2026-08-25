use std::ops::Range;

use chrono::{Datelike, Duration};
use gpui::prelude::*;
use gpui::{
    AnyElement, App, Div, Entity, FontWeight, HighlightStyle, Pixels, SharedString, div, px,
};

use crate::agenda::Agenda;
use crate::block::Block;
use crate::clock::{Clock, ClockFormat};
use crate::selectable_text::SelectableText;
use crate::task::{Priority, Recurrence, Repeat, Task, TaskId, TaskKind};
use crate::theme::ActiveTheme;
use crate::tooltip::{Tooltip, TooltipBuilder};

type DetailLines = Vec<(String, HighlightStyle)>;

pub struct TaskDetails {
    agenda: Entity<Agenda>,
    task: TaskId,
}

impl TaskDetails {
    const MAX_HEIGHT: Pixels = px(300.0);

    pub fn new(agenda: Entity<Agenda>, task: TaskId) -> Self {
        Self { agenda, task }
    }

    pub fn occurrence(self, occurrence: Range<i32>) -> Box<TooltipBuilder> {
        self.build(move |task, cx| Self::occurrence_lines(task, occurrence.clone(), cx))
    }

    pub fn commitment(self) -> Box<TooltipBuilder> {
        self.build(Self::commitment_lines)
    }

    fn build(
        self,
        lines: impl Fn(&Task, &App) -> (DetailLines, Vec<String>) + 'static,
    ) -> Box<TooltipBuilder> {
        Tooltip::element(move |_window, cx| self.content(&lines, cx))
    }

    fn content(
        &self,
        lines: &impl Fn(&Task, &App) -> (DetailLines, Vec<String>),
        cx: &mut App,
    ) -> AnyElement {
        let Some(task) = self.agenda.read(cx).task(self.task).cloned() else {
            return div().into_any_element();
        };

        let (headline, facts) = lines(&task, cx);
        let mut detail = Vec::new();

        detail.extend(
            facts
                .chunks(2)
                .map(|pair| (pair.join(" · "), Self::muted(cx))),
        );

        detail.extend(
            self.calendar_name(&task, cx)
                .map(|calendar| (format!("From {calendar}"), Self::muted(cx))),
        );

        div()
            .id("task-details")
            .flex()
            .flex_col()
            .max_h(Self::MAX_HEIGHT)
            .overflow_y_scroll()
            .child(Self::block("task-details-headline", headline))
            .children(Self::detail_block(detail, cx))
            .into_any_element()
    }

    fn block(id: &'static str, lines: DetailLines) -> SelectableText {
        let (text, runs) = Self::compose(lines);

        SelectableText::new(id, text).runs(runs)
    }

    fn detail_block(lines: DetailLines, cx: &App) -> Option<Div> {
        if lines.is_empty() {
            return None;
        }

        Some(
            div()
                .child(div().my(px(7.0)).h(px(1.0)).bg(cx.theme().rule))
                .child(Self::block("task-details-detail", lines)),
        )
    }

    fn compose(lines: DetailLines) -> (SharedString, Vec<(Range<usize>, HighlightStyle)>) {
        let mut text = String::new();
        let mut runs = Vec::new();

        for (line, style) in lines {
            if !text.is_empty() {
                text.push('\n');
            }

            let start = text.len();
            text.push_str(&line);
            runs.push((start..text.len(), style));
        }

        (text.into(), runs)
    }

    fn strong(cx: &App) -> HighlightStyle {
        HighlightStyle {
            color: Some(cx.theme().heading_fg.into()),
            font_weight: Some(FontWeight::MEDIUM),
            ..Default::default()
        }
    }

    fn body(cx: &App) -> HighlightStyle {
        HighlightStyle {
            color: Some(cx.theme().body_fg.into()),
            ..Default::default()
        }
    }

    fn muted(cx: &App) -> HighlightStyle {
        HighlightStyle {
            color: Some(cx.theme().dim_fg.into()),
            ..Default::default()
        }
    }

    fn calendar_name(&self, task: &Task, cx: &App) -> Option<SharedString> {
        let source = task.source.as_ref()?;

        self.agenda
            .read(cx)
            .subscriptions()
            .iter()
            .find(|subscription| subscription.id == source.subscription)
            .map(|subscription| subscription.name.clone())
    }

    fn occurrence_lines(
        task: &Task,
        occurrence: Range<i32>,
        cx: &App,
    ) -> (DetailLines, Vec<String>) {
        let day = occurrence.start.div_euclid(Block::MINUTES_PER_DAY);
        let mut headline = vec![
            (task.title.to_string(), Self::strong(cx)),
            (Self::when_label(day, &occurrence, cx), Self::body(cx)),
        ];

        headline.extend(Self::place_line(task, cx));

        let mut facts = vec![Self::kind_and_days(task)];

        facts.extend(Self::transition_fact(task));

        (headline, facts)
    }

    fn commitment_lines(task: &Task, cx: &App) -> (DetailLines, Vec<String>) {
        let mut headline = vec![
            (task.title.to_string(), Self::strong(cx)),
            (Self::schedule_label(task, cx), Self::body(cx)),
        ];

        headline.extend(Self::place_line(task, cx));

        let mut facts = vec![Self::kind_and_days(task)];

        facts.extend(Self::sessions_fact(task));
        facts.extend(Self::dates_fact(task, cx));
        facts.extend(Self::priority_fact(task));
        facts.extend(Self::transition_fact(task));

        (headline, facts)
    }

    fn kind_and_days(task: &Task) -> String {
        let mut label = Self::kind_label(task);

        if let Some(cadence) = Self::cadence(task) {
            let joint = match task.kind {
                TaskKind::Fixed { .. } => ", ",
                TaskKind::Flexible(_) => " ",
            };

            label = format!("{label}{joint}{cadence}");
        }

        match task.days.len() {
            7 => label,
            _ => format!("{label} on {}", task.days_label()),
        }
    }

    fn cadence(task: &Task) -> Option<&'static str> {
        let every_day = task.days.len() == 7;

        match &task.kind {
            TaskKind::Fixed { recurrence, .. } => match recurrence {
                Recurrence::Never => None,
                Recurrence::Weekly if !every_day => None,
                recurrence => recurrence.label(),
            },
            TaskKind::Flexible(flexible) => match flexible.repeat {
                Repeat::Never => None,
                Repeat::Daily if !every_day => None,
                repeat => Some(repeat.cadence()),
            },
        }
    }

    fn when_label(day: i32, occurrence: &Range<i32>, cx: &App) -> String {
        let clock = *cx.global::<ClockFormat>();
        let times = format!(
            "{} - {} ({})",
            clock.time_label(occurrence.start),
            clock.time_label(occurrence.end),
            Self::duration_label(occurrence.end - occurrence.start)
        );

        Self::sentence(&format!("{}, {times}", Self::day_label(day, cx)))
    }

    fn kind_label(task: &Task) -> String {
        match &task.kind {
            TaskKind::Fixed { .. } => "Fixed time".to_string(),
            TaskKind::Flexible(flexible) => {
                format!("Flexible time, {}", Self::duration_label(flexible.total))
            }
        }
    }

    fn place_line(task: &Task, cx: &App) -> Option<(String, HighlightStyle)> {
        task.place
            .clone()
            .map(|place| (place.to_string(), Self::muted(cx)))
    }

    fn schedule_label(task: &Task, cx: &App) -> String {
        let clock = *cx.global::<ClockFormat>();

        match &task.kind {
            TaskKind::Fixed {
                start, duration, ..
            } => format!(
                "{} - {} ({})",
                clock.time_label(*start),
                clock.time_label(start + duration),
                Self::duration_label(*duration)
            ),
            TaskKind::Flexible(flexible) => format!(
                "{} - {} ({})",
                clock.time_label(flexible.window.start),
                clock.time_label(flexible.window.end),
                Self::duration_label(flexible.total)
            ),
        }
    }

    fn sessions_fact(task: &Task) -> Option<String> {
        let TaskKind::Flexible(flexible) = &task.kind else {
            return None;
        };

        Some(match flexible.sessions {
            Some(sessions) => format!("{} at a time", Self::duration_label(sessions.preferred)),
            None => "In one sitting".to_string(),
        })
    }

    fn dates_fact(task: &Task, cx: &App) -> Option<String> {
        match (task.dates.from, task.dates.until) {
            (None, None) => None,
            (Some(from), Some(until)) => Some(format!(
                "{} until {}",
                Self::sentence(&Self::day_label(from, cx)),
                Self::day_label(until, cx)
            )),
            (Some(from), None) => Some(format!("From {}", Self::day_label(from, cx))),
            (None, Some(until)) => Some(format!("Until {}", Self::day_label(until, cx))),
        }
    }

    fn priority_fact(task: &Task) -> Option<String> {
        match task.priority {
            Priority::Normal => None,
            priority => Some(Self::sentence(&format!("{} priority", priority.label()))),
        }
    }

    fn transition_fact(task: &Task) -> Option<String> {
        match (task.prep, task.cleanup) {
            (0, 0) => None,
            (prep, 0) => Some(format!("{prep}m before")),
            (0, cleanup) => Some(format!("{cleanup}m after")),
            (prep, cleanup) => Some(format!("{prep}m before, {cleanup}m after")),
        }
    }

    fn day_label(day: i32, cx: &App) -> String {
        let today = cx.global::<Clock>().now().date_naive();
        let date = today + Duration::days(day.into());
        let near = day < 14 && date.month() == today.month();

        match day {
            -1 => "yesterday".to_string(),
            0 => "today".to_string(),
            1 => "tomorrow".to_string(),
            _ if date < today || date.year() != today.year() => {
                date.format("%b %-d %Y").to_string()
            }
            _ if near => date.format("%a %-d").to_string(),
            _ => date.format("%b %-d").to_string(),
        }
    }

    fn duration_label(minutes: i32) -> String {
        match (minutes / 60, minutes % 60) {
            (0, minutes) => format!("{minutes}m"),
            (hours, 0) => format!("{hours}h"),
            (hours, minutes) => format!("{hours}h {minutes}m"),
        }
    }

    fn sentence(text: &str) -> String {
        let mut characters = text.chars();

        match characters.next() {
            Some(first) => first.to_uppercase().chain(characters).collect(),
            None => String::new(),
        }
    }
}
