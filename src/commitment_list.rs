use chrono::{Days, Weekday};
use gpui::prelude::*;
use gpui::{
    App, ClickEvent, Div, ElementId, Entity, FontWeight, Pixels, Rgba, SharedString, Stateful,
    Text, Window, div, px, relative,
};

use crate::agenda::Agenda;
use crate::button::{Button, ClickHandler};
use crate::clock::{Clock, ClockFormat};
use crate::session::{Outcome, Session};
use crate::task::{Priority, Repeat, Task, TaskId, TaskKind};
use crate::theme::ActiveTheme;

#[derive(IntoElement)]
pub struct CommitmentList {
    agenda: Entity<Agenda>,
}

impl CommitmentList {
    pub fn new(agenda: Entity<Agenda>) -> Self {
        Self { agenda }
    }

    fn row(&self, index: usize, task: &Task, now: i32, cx: &App) -> Stateful<Div> {
        let theme = *cx.theme();
        let swatch = task.color.map_or(theme.rule, |color| theme.swatch(color));
        let progress = self.agenda.read(cx).progress(task, now);

        div()
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
            )
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
                        Self::priority_label(task.priority).into(),
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

        match &task.kind {
            TaskKind::Fixed {
                start, duration, ..
            } => format!(
                "{} - {} ({})",
                clock.time_label(*start),
                clock.time_label(start + duration),
                Self::duration_label(*duration)
            ),
            TaskKind::Flexible(flexible) => match flexible.repeat {
                Repeat::Once { deadline_day, .. } => format!(
                    "{} by {}",
                    Self::duration_label(flexible.total),
                    Self::day_label(deadline_day, cx)
                ),
                repeat => format!(
                    "{} {}",
                    Self::duration_label(flexible.total),
                    Self::cadence_label(repeat)
                ),
            },
        }
    }

    fn cadence_label(repeat: Repeat) -> &'static str {
        match repeat {
            Repeat::Daily => "each day",
            Repeat::Weekly => "each week",
            Repeat::Biweekly => "every other week",
            Repeat::Monthly => "each month",
            Repeat::Yearly => "each year",
            Repeat::Once { .. } => "once",
        }
    }

    fn sub(task: &Task) -> String {
        let days = Self::days_label(&task.days);

        match &task.kind {
            TaskKind::Fixed { .. } => task.place.clone().map_or(days, |place| place.to_string()),
            TaskKind::Flexible(flexible) => match flexible.sessions {
                Some(sessions) => format!("{days}, {}m sessions", sessions.preferred),
                None => format!("{days}, in one sitting"),
            },
        }
    }

    fn days_label(days: &[Weekday]) -> String {
        let weekend = [Weekday::Sat, Weekday::Sun];
        let weekdays = [
            Weekday::Mon,
            Weekday::Tue,
            Weekday::Wed,
            Weekday::Thu,
            Weekday::Fri,
        ];

        match days.len() {
            0 => "no days".to_string(),
            7 => "every day".to_string(),
            5 if weekdays.iter().all(|day| days.contains(day)) => "weekdays".to_string(),
            2 if weekend.iter().all(|day| days.contains(day)) => "weekends".to_string(),
            _ => days
                .iter()
                .map(|day| day.to_string())
                .collect::<Vec<_>>()
                .join(", "),
        }
    }

    fn day_label(day: i32, cx: &App) -> String {
        let date = cx.global::<Clock>().now().date_naive() + Days::new(day.max(0) as u64);

        date.format("%a %-d").to_string()
    }

    fn duration_label(minutes: i32) -> String {
        match (minutes / 60, minutes % 60) {
            (0, minutes) => format!("{minutes}m"),
            (hours, 0) => format!("{hours}h"),
            (hours, minutes) => format!("{hours}h {minutes}m"),
        }
    }

    fn priority_label(priority: Priority) -> &'static str {
        match priority {
            Priority::Lowest => "lowest",
            Priority::Low => "low",
            Priority::Normal => "normal",
            Priority::High => "high",
            Priority::Highest => "highest",
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
                    .flex_none()
                    .text_size(px(11.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme.dim_fg)
                    .child(Text::new(label.into(), label.into())),
            )
            .child(div().flex_1().h(px(1.0)).bg(theme.rule))
            .children(trailing)
    }

    fn pending_row(&self, index: usize, session: &Session, cx: &App) -> Div {
        let theme = *cx.theme();
        let state = self.agenda.read(cx);
        let task = state.task(session.task);
        let swatch = task
            .and_then(|task| task.color)
            .map_or(theme.rule, |color| theme.swatch(color));
        let title = task.map_or_else(SharedString::default, |task| task.title.clone());

        div()
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
            .child(self.verdict(index, session))
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
                    .on_click(self.settle(task, start, Outcome::Done)),
            )
            .child(
                Button::new(("pending-skip", index), "✕")
                    .fixed(px(24.0), px(22.0))
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

        format!(
            "{} to {}, {}",
            clock.time_label(session.start),
            clock.time_label(session.end),
            Self::duration_label(session.work)
        )
    }

    fn pending(&self, cx: &App) -> Option<Div> {
        let state = self.agenda.read(cx);
        let sessions = state.unconfirmed();
        if sessions.is_empty() {
            return None;
        }

        let agenda = self.agenda.clone();
        let confirm_all =
            Button::text("confirm-all", "Confirm all").on_click(Box::new(move |_window, cx| {
                agenda.update(cx, Agenda::confirm_all)
            }));

        Some(
            div()
                .mb(px(14.0))
                .child(Self::heading(
                    "Did these happen?",
                    Some(div().flex_none().child(confirm_all)),
                    cx,
                ))
                .children(
                    sessions
                        .iter()
                        .enumerate()
                        .map(|(index, session)| self.pending_row(index, session, cx)),
                ),
        )
    }

    fn group(&self, label: &'static str, fixed: bool, now: i32, cx: &App) -> Div {
        let state = self.agenda.read(cx);
        let members: Vec<_> = state
            .tasks()
            .iter()
            .enumerate()
            .filter(|(_, task)| matches!(task.kind, TaskKind::Fixed { .. }) == fixed)
            .collect();
        let count = div()
            .flex_none()
            .text_size(px(10.5))
            .text_color(cx.theme().faint_fg)
            .child(Text::new(
                ("group-count", fixed as usize).into(),
                members.len().to_string().into(),
            ));

        div()
            .mb(px(14.0))
            .child(Self::heading(label, Some(count), cx))
            .children(
                members
                    .iter()
                    .map(|(index, task)| self.row(*index, task, now, cx)),
            )
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
            .children(self.pending(cx))
            .child(self.group("Fixed", true, now, cx))
            .child(self.group("Flexible", false, now, cx))
    }
}
