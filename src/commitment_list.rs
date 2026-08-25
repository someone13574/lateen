use std::ops::Range;

use chrono::Days;
use gpui::prelude::*;
use gpui::{
    App, ClickEvent, Div, ElementId, Entity, FontWeight, Pixels, Rgba, SharedString, Text, Window,
    div, px, relative,
};

use crate::agenda::Agenda;
use crate::button::{Button, ClickHandler, Verdict};
use crate::calendar_list::CalendarList;
use crate::clock::{Clock, ClockFormat};
use crate::session::{Outcome, Session};
use crate::task::{Priority, Repeat, Task, TaskId, TaskKind};
use crate::task_details::TaskDetails;
use crate::theme::ActiveTheme;
use crate::tooltip::{TooltipBuilder, Tooltipped};

#[derive(IntoElement)]
pub struct CommitmentList {
    agenda: Entity<Agenda>,
    calendars: Entity<CalendarList>,
}

impl CommitmentList {
    const EMPTY_BODY: &str = "Add a fixed event, or a flexible one to be scheduled around it.";

    pub fn new(agenda: Entity<Agenda>, calendars: Entity<CalendarList>) -> Self {
        Self { agenda, calendars }
    }

    fn row(&self, index: usize, task: &Task, now: i32, cx: &App) -> Tooltipped {
        let theme = *cx.theme();
        let swatch = task.color.map_or(theme.rule, |color| theme.swatch(color));
        let progress = self.agenda.read(cx).progress(task, now);

        let row = div()
            .id(("commitment", index))
            .flex()
            .gap(px(9.0))
            .mb(px(3.0))
            .px(px(9.0))
            .py(px(8.0))
            .rounded(px(6.0))
            .border(px(1.0))
            .border_color(theme.rule)
            .bg(theme.card_bg)
            .cursor_pointer()
            .hover(|style| {
                style
                    .border_color(cx.theme().chip_border)
                    .bg(cx.theme().row_hover_bg)
            })
            .on_click(self.open(task.id))
            .child(div().flex_none().w(px(4.0)).rounded(px(2.0)).bg(swatch))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .child(Self::row_title(index, task, cx))
                    .child(Self::row_line(
                        ("commitment-meta", index),
                        Self::meta(task, cx),
                        px(11.0),
                        theme.body_fg,
                        px(3.0),
                    ))
                    .child(Self::row_line(
                        ("commitment-sub", index),
                        Self::sub(task),
                        px(10.5),
                        theme.dim_fg,
                        px(1.0),
                    ))
                    .children(progress.map(|progress| Self::progress(index, swatch, progress, cx))),
            );

        Tooltipped::new(
            ("commitment-details", index),
            row,
            self.details(task.id, None),
        )
    }

    fn details(&self, task: TaskId, occurrence: Option<Range<i32>>) -> Box<TooltipBuilder> {
        let details = TaskDetails::new(self.agenda.clone(), task);

        match occurrence {
            Some(occurrence) => details.occurrence(occurrence),
            None => details.commitment(),
        }
    }

    fn open(&self, task: TaskId) -> impl Fn(&ClickEvent, &mut Window, &mut App) + 'static {
        let agenda = self.agenda.clone();

        move |_event, _window, cx| {
            agenda.update(cx, |agenda, cx| agenda.select(task, cx));
        }
    }

    fn row_title(index: usize, task: &Task, cx: &App) -> Div {
        div()
            .flex()
            .items_baseline()
            .gap(px(7.0))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_size(px(12.5))
                    .font_weight(FontWeight::MEDIUM)
                    .child(Text::new(
                        ("commitment-title", index).into(),
                        task.title.clone(),
                    )),
            )
            .child(
                div()
                    .flex_none()
                    .text_size(px(10.5))
                    .text_color(cx.theme().faint_fg)
                    .child(Text::new(
                        ("commitment-priority", index).into(),
                        Priority::label(task.priority).into(),
                    )),
            )
    }

    fn row_line(
        id: impl Into<ElementId>,
        text: String,
        size: Pixels,
        color: Rgba,
        top: Pixels,
    ) -> Div {
        div()
            .mt(top)
            .truncate()
            .text_size(size)
            .text_color(color)
            .child(Text::new(id.into(), text.into()))
    }

    fn progress(index: usize, swatch: Rgba, (done, total): (i32, i32), cx: &App) -> Div {
        let theme = *cx.theme();
        let fraction = (done as f32 / total.max(1) as f32).clamp(0.0, 1.0);
        let label = format!(
            "{} of {}",
            Self::duration_label(done),
            Self::duration_label(total)
        );

        div()
            .flex()
            .items_center()
            .gap(px(7.0))
            .mt(px(6.0))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .h(px(4.0))
                    .rounded(px(2.0))
                    .overflow_hidden()
                    .bg(theme.rule)
                    .child(div().h_full().w(relative(fraction)).bg(swatch)),
            )
            .child(
                div()
                    .flex_none()
                    .text_size(px(10.0))
                    .text_color(theme.dim_fg)
                    .child(Text::new(
                        ("commitment-progress", index).into(),
                        label.into(),
                    )),
            )
    }

    fn meta(task: &Task, cx: &App) -> String {
        let clock = cx.global::<ClockFormat>();

        if let Some(days) = task.all_day_days() {
            return Self::all_day_label(days);
        }

        match &task.kind {
            TaskKind::Fixed {
                start, duration, ..
            } => format!(
                "{} - {} ({})",
                clock.time_label(*start),
                clock.time_label(start + duration),
                Self::duration_label(*duration)
            ),
            TaskKind::Flexible(flexible) => match (flexible.repeat, task.dates.until) {
                (Repeat::Never, Some(until)) => format!(
                    "{} by {}",
                    Self::duration_label(flexible.total),
                    Self::day_label(until, cx)
                ),
                (repeat, _) => format!(
                    "{} {}",
                    Self::duration_label(flexible.total),
                    repeat.cadence()
                ),
            },
        }
    }

    fn sub(task: &Task) -> String {
        let days = task.days_label();

        match &task.kind {
            TaskKind::Fixed { recurrence, .. } => match &task.place {
                Some(place) => place.to_string(),
                None => recurrence.label().map_or(days, str::to_string),
            },
            TaskKind::Flexible(flexible) => match flexible.sessions {
                Some(sessions) => format!("{days}, {}m sessions", sessions.preferred),
                None => format!("{days}, in one sitting"),
            },
        }
    }

    fn day_label(day: i32, cx: &App) -> String {
        let date = cx.global::<Clock>().now().date_naive() + Days::new(day.max(0) as u64);

        date.format("%a %-d").to_string()
    }

    fn all_day_label(days: i32) -> String {
        match days {
            1 => "All day".to_string(),
            days => format!("All day, {days} days"),
        }
    }

    fn duration_label(minutes: i32) -> String {
        match (minutes / 60, minutes % 60) {
            (0, minutes) => format!("{minutes}m"),
            (hours, 0) => format!("{hours}h"),
            (hours, minutes) => format!("{hours}h {minutes}m"),
        }
    }

    fn heading(label: &'static str, trailing: Option<Div>, cx: &App) -> Div {
        let theme = *cx.theme();

        div()
            .flex()
            .items_center()
            .gap(px(8.0))
            .px(px(2.0))
            .pt(px(2.0))
            .pb(px(6.0))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_size(px(11.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme.heading_fg)
                    .child(Text::new(label.into(), label.into())),
            )
            .children(trailing)
    }

    fn pending_row(&self, index: usize, session: &Session, cx: &App) -> Tooltipped {
        let theme = *cx.theme();
        let state = self.agenda.read(cx);
        let task = state.task(session.task);
        let swatch = task
            .and_then(|task| task.color)
            .map_or(theme.rule, |color| theme.swatch(color));
        let title = task.map_or_else(SharedString::default, |task| task.title.clone());

        let row = div()
            .flex()
            .items_center()
            .gap(px(9.0))
            .mb(px(3.0))
            .px(px(9.0))
            .py(px(8.0))
            .rounded(px(6.0))
            .border(px(1.0))
            .border_color(theme.pending_border)
            .bg(theme.pending_bg)
            .child(
                div()
                    .flex_none()
                    .w(px(4.0))
                    .self_stretch()
                    .rounded(px(2.0))
                    .bg(swatch),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .child(
                        div()
                            .truncate()
                            .text_size(px(12.0))
                            .font_weight(FontWeight::MEDIUM)
                            .child(Text::new(("pending-title", index).into(), title)),
                    )
                    .child(Self::row_line(
                        ("pending-meta", index),
                        Self::session_label(session, cx),
                        px(10.5),
                        theme.pending_meta_fg,
                        px(2.0),
                    )),
            )
            .child(self.verdict(index, session));

        Tooltipped::new(
            ("pending-details", index),
            row,
            self.details(session.task, Some(session.start..session.end)),
        )
    }

    fn verdict(&self, index: usize, session: &Session) -> Div {
        let (task, start) = (session.task, session.start);

        div()
            .flex()
            .flex_none()
            .gap(px(3.0))
            .child(
                Button::new(("pending-done", index), "✓")
                    .fixed(px(24.0), px(22.0))
                    .verdict(Verdict::Affirm)
                    .on_click(self.settle(task, start, Outcome::Done)),
            )
            .child(
                Button::new(("pending-skip", index), "✗")
                    .fixed(px(24.0), px(22.0))
                    .verdict(Verdict::Deny)
                    .on_click(self.settle(task, start, Outcome::Skipped)),
            )
    }

    fn settle(&self, task: TaskId, start: i32, outcome: Outcome) -> Box<ClickHandler> {
        let agenda = self.agenda.clone();

        Box::new(move |_window, cx| {
            agenda.update(cx, |agenda, cx| agenda.confirm(task, start, outcome, cx));
        })
    }

    fn session_label(session: &Session, cx: &App) -> String {
        let clock = cx.global::<ClockFormat>();

        let times = format!(
            "{} to {}, {}",
            clock.time_label(session.start),
            clock.time_label(session.end),
            Self::duration_label(session.work)
        );

        match session.day() {
            0 => times,
            day => format!("{}, {times}", Self::past_day_label(day, cx)),
        }
    }

    fn past_day_label(day: i32, cx: &App) -> String {
        if day == -1 {
            return "Yesterday".to_string();
        }

        let date = cx.global::<Clock>().now().date_naive() - Days::new(day.unsigned_abs() as u64);

        date.format("%a %-d").to_string()
    }

    fn pending(&self, cx: &App) -> Vec<Div> {
        let state = self.agenda.read(cx);
        let (old, today): (Vec<_>, Vec<_>) = state
            .unconfirmed()
            .into_iter()
            .enumerate()
            .partition(|(_, session)| session.day() < 0);

        [("Today", today, 0..i32::MAX), ("Old", old, i32::MIN..0)]
            .into_iter()
            .enumerate()
            .filter(|(_, (_, rows, _))| !rows.is_empty())
            .map(|(index, (label, rows, days))| self.pending_group(index, label, &rows, days, cx))
            .collect()
    }

    fn pending_group(
        &self,
        index: usize,
        label: &'static str,
        rows: &[(usize, &Session)],
        days: Range<i32>,
        cx: &App,
    ) -> Div {
        let agenda = self.agenda.clone();
        let confirm_all = Button::text(("confirm-all", index), "Confirm all").on_click(Box::new(
            move |_window, cx| {
                agenda.update(cx, |agenda, cx| agenda.confirm_all(days.clone(), cx));
            },
        ));

        div()
            .mb(px(14.0))
            .child(Self::heading(
                label,
                Some(div().flex_none().child(confirm_all)),
                cx,
            ))
            .children(
                rows.iter()
                    .map(|(index, session)| self.pending_row(*index, session, cx)),
            )
    }

    fn group(&self, label: &'static str, fixed: bool, now: i32, cx: &App) -> Option<Div> {
        let state = self.agenda.read(cx);
        let members: Vec<_> = state
            .tasks()
            .iter()
            .enumerate()
            .filter(|(_, task)| matches!(task.kind, TaskKind::Fixed { .. }) == fixed)
            .collect();
        if members.is_empty() {
            return None;
        }

        let count = div()
            .flex_none()
            .text_size(px(10.5))
            .text_color(cx.theme().faint_fg)
            .child(Text::new(
                ("group-count", fixed as usize).into(),
                members.len().to_string().into(),
            ));

        Some(
            div()
                .mb(px(14.0))
                .child(Self::heading(label, Some(count), cx))
                .children(
                    members
                        .iter()
                        .map(|(index, task)| self.row(*index, task, now, cx)),
                ),
        )
    }

    fn body(&self, now: i32, cx: &App) -> Vec<Div> {
        if self.agenda.read(cx).tasks().is_empty() {
            return vec![self.empty(cx)];
        }

        [
            self.group("Fixed", true, now, cx),
            self.group("Flexible", false, now, cx),
        ]
        .into_iter()
        .flatten()
        .collect()
    }

    fn empty(&self, cx: &App) -> Div {
        div()
            .flex()
            .flex_col()
            .items_center()
            .gap(px(13.0))
            .pt(px(44.0))
            .px(px(10.0))
            .text_center()
            .child(Self::empty_copy(cx))
            .child(self.empty_actions())
    }

    fn empty_copy(cx: &App) -> Div {
        let theme = *cx.theme();

        div()
            .flex()
            .flex_col()
            .items_center()
            .gap(px(5.0))
            .child(
                div()
                    .text_size(px(13.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme.heading_fg)
                    .child(Text::new("empty-title".into(), "No commitments yet".into())),
            )
            .child(
                div()
                    .max_w(px(232.0))
                    .text_size(px(11.5))
                    .text_color(theme.dim_fg)
                    .child(Text::new("empty-body".into(), Self::EMPTY_BODY.into())),
            )
    }

    fn empty_actions(&self) -> Div {
        div()
            .flex()
            .gap(px(7.0))
            .child(
                Button::new("empty-new", "New commitment")
                    .filled()
                    .padding(px(12.0), px(5.0))
                    .on_click(self.add()),
            )
            .child(
                Button::new("empty-import", "Import calendar")
                    .padding(px(12.0), px(5.0))
                    .on_click(self.open_calendars()),
            )
    }

    fn add(&self) -> Box<ClickHandler> {
        let agenda = self.agenda.clone();
        let calendars = self.calendars.clone();

        Box::new(move |_window, cx| {
            agenda.update(cx, |agenda, cx| agenda.add(cx));
            calendars.update(cx, |calendars, cx| calendars.close(cx));
        })
    }

    fn open_calendars(&self) -> Box<ClickHandler> {
        let agenda = self.agenda.clone();
        let calendars = self.calendars.clone();

        Box::new(move |_window, cx| {
            agenda.update(cx, Agenda::deselect);
            calendars.update(cx, |calendars, cx| calendars.toggle(cx));
        })
    }
}

impl RenderOnce for CommitmentList {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let now = cx.global::<Clock>().minute_of_day() as i32;

        div()
            .id("commitment-list")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .pt(px(10.0))
            .px(px(12.0))
            .pb(px(16.0))
            .child(self.calendars.clone())
            .children(self.pending(cx))
            .children(self.body(now, cx))
    }
}
