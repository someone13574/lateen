use chrono::{Days, Weekday};
use gpui::prelude::*;
use gpui::{
    App, Div, ElementId, Entity, FontWeight, Rgba, Role, SharedString, Stateful, Text, Window, div,
    px,
};

use crate::agenda::Agenda;
use crate::block::Block;
use crate::button::{Button, ClickHandler};
use crate::clock::{Clock, ClockFormat};
use crate::input::{Input, InputEvent, InputState};
use crate::select::{Select, SelectState};
use crate::session::{Outcome, Session};
use crate::task::{
    Breaks, Flexible, Priority, Recurrence, Repeat, Sessions, Task, TaskId, TaskKind,
};
use crate::theme::ActiveTheme;

pub struct Editor {
    agenda: Entity<Agenda>,
    task: TaskId,
    title: Entity<InputState>,
    start: Entity<InputState>,
    duration: Entity<InputState>,
    place: Entity<InputState>,
    overrun: Entity<InputState>,
    total: Entity<InputState>,
    opens: Entity<InputState>,
    closes: Entity<InputState>,
    preferred: Entity<InputState>,
    shortest: Entity<InputState>,
    longest: Entity<InputState>,
    break_every: Entity<InputState>,
    break_minutes: Entity<InputState>,
    prep: Entity<InputState>,
    cleanup: Entity<InputState>,
    repeat: Entity<SelectState>,
    earliest: Entity<SelectState>,
    deadline: Entity<SelectState>,
}

impl Editor {
    pub fn new(
        agenda: Entity<Agenda>,
        task: TaskId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let editor = Self {
            agenda,
            task,
            title: Self::field(Self::set_title, window, cx),
            start: Self::field(Self::set_start, window, cx),
            duration: Self::field(Self::set_duration, window, cx),
            place: Self::field(Self::set_place, window, cx),
            overrun: Self::field(Self::set_overrun, window, cx),
            total: Self::field(Self::set_total, window, cx),
            opens: Self::field(Self::set_opens, window, cx),
            closes: Self::field(Self::set_closes, window, cx),
            preferred: Self::field(Self::set_preferred, window, cx),
            shortest: Self::field(Self::set_shortest, window, cx),
            longest: Self::field(Self::set_longest, window, cx),
            break_every: Self::field(Self::set_break_every, window, cx),
            break_minutes: Self::field(Self::set_break_minutes, window, cx),
            prep: Self::field(Self::set_prep, window, cx),
            cleanup: Self::field(Self::set_cleanup, window, cx),
            repeat: cx.new(|cx| SelectState::new(window, cx)),
            earliest: cx.new(|cx| SelectState::new(window, cx)),
            deadline: cx.new(|cx| SelectState::new(window, cx)),
        };

        editor.reseed(cx);

        editor
    }

    pub fn task(&self) -> TaskId {
        self.task
    }

    fn field(
        apply: fn(&mut Task, &str),
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<InputState> {
        let state = cx.new(|cx| InputState::new(window, cx));

        cx.subscribe(&state, move |editor, state, _event: &InputEvent, cx| {
            let text = state.read(cx).content().clone();
            let task = editor.task;

            editor.agenda.update(cx, |agenda, cx| {
                agenda.edit(task, |task| apply(task, &text), cx)
            });
        })
        .detach();

        state
    }

    fn reseed(&self, cx: &mut Context<Self>) {
        for (state, value) in self.values(cx) {
            state.update(cx, |state, cx| state.set_content(value, cx));
        }
    }

    fn values(&self, cx: &App) -> Vec<(Entity<InputState>, String)> {
        let Some(task) = self.agenda.read(cx).task(self.task) else {
            return Vec::new();
        };
        let clock = *cx.global::<ClockFormat>();
        let mut values = vec![
            (self.title.clone(), task.title.to_string()),
            (self.prep.clone(), task.prep.to_string()),
            (self.cleanup.clone(), task.cleanup.to_string()),
        ];

        match &task.kind {
            TaskKind::Fixed {
                start,
                duration,
                overrun_percent,
                ..
            } => values.extend([
                (self.start.clone(), clock.time_label(*start)),
                (self.duration.clone(), duration.to_string()),
                (
                    self.place.clone(),
                    task.place.clone().unwrap_or_default().to_string(),
                ),
                (self.overrun.clone(), overrun_percent.to_string()),
            ]),
            TaskKind::Flexible(flexible) => values.extend(self.flexible_values(flexible, clock)),
        }

        values
    }

    fn flexible_values(
        &self,
        flexible: &Flexible,
        clock: ClockFormat,
    ) -> Vec<(Entity<InputState>, String)> {
        let mut values = vec![
            (self.total.clone(), flexible.total.to_string()),
            (self.opens.clone(), clock.time_label(flexible.window.start)),
            (self.closes.clone(), clock.time_label(flexible.window.end)),
            (
                self.break_every.clone(),
                flexible.breaks.map_or(0, |breaks| breaks.every).to_string(),
            ),
            (
                self.break_minutes.clone(),
                flexible
                    .breaks
                    .map_or(0, |breaks| breaks.minutes)
                    .to_string(),
            ),
        ];

        if let Some(sessions) = flexible.sessions {
            values.extend([
                (self.preferred.clone(), sessions.preferred.to_string()),
                (self.shortest.clone(), sessions.shortest.to_string()),
                (self.longest.clone(), sessions.longest.to_string()),
            ]);
        }

        values
    }

    fn set_kind(&mut self, fixed: bool, cx: &mut Context<Self>) {
        let task = self.task;

        self.agenda.update(cx, |agenda, cx| {
            agenda.edit(task, |task| Self::convert(task, fixed), cx)
        });
        self.reseed(cx);
    }

    fn convert(task: &mut Task, fixed: bool) {
        let kind = match (&task.kind, fixed) {
            (TaskKind::Flexible(flexible), true) => TaskKind::Fixed {
                start: flexible.window.start,
                duration: flexible.total,
                recurrence: Recurrence::Weekly,
                overrun_percent: 0,
            },
            (TaskKind::Fixed { duration, .. }, false) => {
                TaskKind::Flexible(Flexible::new(*duration, 9 * 60, 22 * 60))
            }
            _ => return,
        };

        task.kind = kind;
    }

    fn toggle_day(task: &mut Task, day: Weekday) {
        match task.days.iter().position(|other| *other == day) {
            Some(index) => {
                task.days.remove(index);
            }
            None => {
                task.days.push(day);
                task.days.sort_by_key(Weekday::num_days_from_sunday);
            }
        }
    }

    fn set_splittable(&mut self, splittable: bool, cx: &mut Context<Self>) {
        let task = self.task;

        self.agenda.update(cx, |agenda, cx| {
            agenda.edit(task, |task| Self::split(task, splittable), cx)
        });
        self.reseed(cx);
    }

    fn split(task: &mut Task, splittable: bool) {
        if let Some(flexible) = Self::flexible(task) {
            flexible.sessions = splittable.then_some(Sessions {
                shortest: 15,
                preferred: 30,
                longest: 90,
            });
        }
    }

    fn repeat_index(repeat: Repeat) -> usize {
        match repeat {
            Repeat::Daily => 0,
            Repeat::Weekly => 1,
            Repeat::Biweekly => 2,
            Repeat::Monthly => 3,
            Repeat::Yearly => 4,
            Repeat::Once { .. } => 5,
        }
    }

    fn set_repeat(task: &mut Task, index: usize) {
        let Some(flexible) = Self::flexible(task) else {
            return;
        };
        let (earliest_day, deadline_day) = match flexible.repeat {
            Repeat::Once {
                earliest_day,
                deadline_day,
            } => (earliest_day, deadline_day),
            _ => (0, 6),
        };

        flexible.repeat = match index {
            1 => Repeat::Weekly,
            2 => Repeat::Biweekly,
            3 => Repeat::Monthly,
            4 => Repeat::Yearly,
            5 => Repeat::Once {
                earliest_day,
                deadline_day,
            },
            _ => Repeat::Daily,
        };
    }

    fn recurrence_index(recurrence: Recurrence) -> usize {
        match recurrence {
            Recurrence::Once => 0,
            Recurrence::Weekly => 1,
            Recurrence::Biweekly => 2,
        }
    }

    fn set_recurrence(task: &mut Task, index: usize) {
        if let TaskKind::Fixed { recurrence, .. } = &mut task.kind {
            *recurrence = match index {
                1 => Recurrence::Weekly,
                2 => Recurrence::Biweekly,
                _ => Recurrence::Once,
            };
        }
    }

    fn set_earliest(task: &mut Task, day: i32) {
        if let Some(Repeat::Once {
            earliest_day,
            deadline_day,
        }) = Self::flexible(task).map(|flexible| &mut flexible.repeat)
        {
            *earliest_day = day;
            *deadline_day = (*deadline_day).max(day);
        }
    }

    fn set_deadline(task: &mut Task, day: i32) {
        if let Some(Repeat::Once {
            earliest_day,
            deadline_day,
        }) = Self::flexible(task).map(|flexible| &mut flexible.repeat)
        {
            *deadline_day = day;
            *earliest_day = (*earliest_day).min(day);
        }
    }

    fn set_title(task: &mut Task, text: &str) {
        task.title = text.to_string().into();
    }

    fn set_place(task: &mut Task, text: &str) {
        task.place = (!text.trim().is_empty()).then(|| text.to_string().into());
    }

    fn set_prep(task: &mut Task, text: &str) {
        task.prep = Self::amount(text).unwrap_or(task.prep);
    }

    fn set_cleanup(task: &mut Task, text: &str) {
        task.cleanup = Self::amount(text).unwrap_or(task.cleanup);
    }

    fn set_start(task: &mut Task, text: &str) {
        if let (TaskKind::Fixed { start, .. }, Some(minutes)) =
            (&mut task.kind, ClockFormat::parse(text))
        {
            *start = minutes;
        }
    }

    fn set_duration(task: &mut Task, text: &str) {
        if let (TaskKind::Fixed { duration, .. }, Some(minutes)) =
            (&mut task.kind, Self::amount(text))
        {
            *duration = minutes.max(1);
        }
    }

    fn set_overrun(task: &mut Task, text: &str) {
        if let (
            TaskKind::Fixed {
                overrun_percent, ..
            },
            Some(percent),
        ) = (&mut task.kind, Self::amount(text))
        {
            *overrun_percent = percent;
        }
    }

    fn set_total(task: &mut Task, text: &str) {
        if let (Some(flexible), Some(minutes)) = (Self::flexible(task), Self::amount(text)) {
            flexible.total = minutes.max(1);
        }
    }

    fn set_opens(task: &mut Task, text: &str) {
        if let (Some(flexible), Some(minutes)) = (Self::flexible(task), ClockFormat::parse(text)) {
            flexible.window.start = minutes;
        }
    }

    fn set_closes(task: &mut Task, text: &str) {
        if let (Some(flexible), Some(minutes)) = (Self::flexible(task), ClockFormat::parse(text)) {
            flexible.window.end = minutes;
        }
    }

    fn set_preferred(task: &mut Task, text: &str) {
        if let (Some(sessions), Some(minutes)) = (Self::sessions(task), Self::amount(text)) {
            sessions.preferred = minutes.max(1);
        }
    }

    fn set_shortest(task: &mut Task, text: &str) {
        if let (Some(sessions), Some(minutes)) = (Self::sessions(task), Self::amount(text)) {
            sessions.shortest = minutes.max(1);
        }
    }

    fn set_longest(task: &mut Task, text: &str) {
        if let (Some(sessions), Some(minutes)) = (Self::sessions(task), Self::amount(text)) {
            sessions.longest = minutes.max(1);
        }
    }

    fn set_break_every(task: &mut Task, text: &str) {
        if let (Some(flexible), Some(every)) = (Self::flexible(task), Self::amount(text)) {
            let minutes = flexible.breaks.map_or(0, |breaks| breaks.minutes);

            flexible.breaks = (every > 0).then_some(Breaks { every, minutes });
        }
    }

    fn set_break_minutes(task: &mut Task, text: &str) {
        if let (Some(flexible), Some(minutes)) = (Self::flexible(task), Self::amount(text)) {
            let every = flexible.breaks.map_or(0, |breaks| breaks.every);

            flexible.breaks = (every > 0).then_some(Breaks { every, minutes });
        }
    }

    fn amount(text: &str) -> Option<i32> {
        text.trim().parse().ok().filter(|minutes| *minutes >= 0)
    }

    fn flexible(task: &mut Task) -> Option<&mut Flexible> {
        match &mut task.kind {
            TaskKind::Flexible(flexible) => Some(flexible),
            TaskKind::Fixed { .. } => None,
        }
    }

    fn sessions(task: &mut Task) -> Option<&mut Sessions> {
        Self::flexible(task)?.sessions.as_mut()
    }

    fn back(&self, cx: &App) -> Div {
        let agenda = self.agenda.clone();

        div().flex().child(
            div()
                .id("back-to-list")
                .role(Role::Button)
                .aria_label("All commitments")
                .flex_none()
                .pt(px(2.0))
                .pb(px(8.0))
                .text_size(px(11.5))
                .text_color(cx.theme().muted_fg)
                .cursor_pointer()
                .hover(|style| style.text_color(cx.theme().link_fg))
                .on_click(move |_event, _window, cx| {
                    agenda.update(cx, |agenda, cx| agenda.deselect(cx));
                })
                .child(Text::new_inaccessible("‹ All commitments".into())),
        )
    }

    fn heading(label: &'static str, cx: &App) -> Div {
        div()
            .mt(px(15.0))
            .mb(px(6.0))
            .text_size(px(11.0))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(cx.theme().dim_fg)
            .child(Text::new(label.into(), label.into()))
    }

    fn labeled(label: &'static str, control: impl IntoElement, cx: &App) -> Div {
        div()
            .flex_1()
            .text_size(px(11.0))
            .text_color(cx.theme().muted_fg)
            .child(Text::new(label.into(), label.into()))
            .child(div().mt(px(3.0)).child(control))
    }

    fn toggle(
        id: impl Into<ElementId>,
        label: SharedString,
        selected: bool,
        unselected_fg: Rgba,
        cx: &App,
    ) -> Stateful<Div> {
        let theme = *cx.theme();

        div()
            .id(id)
            .role(Role::Button)
            .aria_label(label.clone())
            .aria_selected(selected)
            .flex()
            .flex_1()
            .justify_center()
            .cursor_pointer()
            .border(px(1.0))
            .border_color(if selected {
                theme.chip_border
            } else {
                theme.button_border
            })
            .bg(if selected {
                theme.chip_bg
            } else {
                theme.card_bg
            })
            .text_color(if selected {
                theme.link_fg
            } else {
                unselected_fg
            })
            .child(Text::new_inaccessible(label))
    }

    fn kinds(fixed: bool, editor: &Entity<Self>, cx: &App) -> Div {
        div()
            .flex()
            .gap(px(4.0))
            .mt(px(9.0))
            .child(Self::kind_button(
                "kind-fixed",
                "Fixed time",
                true,
                fixed,
                editor,
                cx,
            ))
            .child(Self::kind_button(
                "kind-flexible",
                "Flexible",
                false,
                fixed,
                editor,
                cx,
            ))
    }

    fn sessions_list(&self, splittable: bool, cx: &App) -> Option<Div> {
        let now = cx.global::<Clock>().minute_of_day() as i32;
        let agenda = self.agenda.read(cx);
        let task = agenda.task(self.task)?;
        let run = task.run(now.div_euclid(Block::MINUTES_PER_DAY));
        let past = agenda.logged(self.task, &run);
        let mut future: Vec<_> = agenda
            .schedule()
            .blocks()
            .filter(|block| block.task == self.task && block.end() > now)
            .cloned()
            .collect();

        future.sort_by_key(|block| block.start);
        if !splittable {
            future.truncate(past.is_empty() as usize);
        }

        Some(
            div()
                .child(Self::sessions_heading(
                    if splittable {
                        "Sessions"
                    } else {
                        "When it lands"
                    },
                    agenda.progress(task, now).filter(|_| splittable),
                    cx,
                ))
                .children(past.iter().map(|session| self.past_row(session, cx)))
                .children(future.iter().map(|block| self.future_row(block, cx))),
        )
    }

    fn length(id: usize, minutes: i32, cx: &App) -> Div {
        div()
            .flex_none()
            .text_size(px(10.5))
            .text_color(cx.theme().dim_fg)
            .child(Text::new(
                ("session-length", id).into(),
                Self::duration_label(minutes).into(),
            ))
    }

    fn range_label(start: i32, end: i32, cx: &App) -> String {
        let clock = *cx.global::<ClockFormat>();

        format!(
            "{}, {} to {}",
            Self::day_tag(start.div_euclid(Block::MINUTES_PER_DAY), cx),
            clock.time_label(start),
            clock.time_label(end)
        )
    }

    fn day_tag(day: i32, cx: &App) -> String {
        match day {
            0 => "Today".to_string(),
            day => (cx.global::<Clock>().now().date_naive() + Days::new(day.max(0) as u64))
                .format("%a")
                .to_string(),
        }
    }

    fn outcome_label(outcome: Outcome) -> &'static str {
        match outcome {
            Outcome::Assumed => "assumed done",
            Outcome::Done => "confirmed",
            Outcome::Skipped => "not done, put back in the queue",
        }
    }

    fn duration_label(minutes: i32) -> String {
        match (minutes / 60, minutes % 60) {
            (0, minutes) => format!("{minutes}m"),
            (hours, 0) => format!("{hours}h"),
            (hours, minutes) => format!("{hours}h {minutes}m"),
        }
    }

    fn verdict(&self, start: i32) -> Div {
        div()
            .flex()
            .flex_none()
            .gap(px(3.0))
            .child(
                Button::new(("session-done", start as usize), "✓")
                    .fixed(px(22.0), px(20.0))
                    .on_click(self.settle(start, Outcome::Done)),
            )
            .child(
                Button::new(("session-skip", start as usize), "✕")
                    .fixed(px(22.0), px(20.0))
                    .on_click(self.settle(start, Outcome::Skipped)),
            )
    }

    fn settle(&self, start: i32, outcome: Outcome) -> Box<ClickHandler> {
        let agenda = self.agenda.clone();
        let task = self.task;

        Box::new(move |_window, cx| {
            agenda.update(cx, |agenda, cx| agenda.confirm(task, start, outcome, cx));
        })
    }

    fn finish(&self, start: i32) -> Box<ClickHandler> {
        let agenda = self.agenda.clone();
        let task = self.task;

        Box::new(move |_window, cx| {
            agenda.update(cx, |agenda, cx| agenda.finish(task, start, cx));
        })
    }

    fn future_row(&self, block: &Block, cx: &App) -> Div {
        let theme = *cx.theme();
        let start = block.work_start();

        Self::session_row(theme.card_bg, theme.rule)
            .child(Self::session_text(
                start as usize,
                Self::range_label(start, start + block.work(), cx),
                "planned",
                theme.bottom_bar_time_fg,
                cx,
            ))
            .child(Self::length(start as usize, block.work(), cx))
            .child(
                Button::new(("session-finish", block.start as usize), "Done")
                    .small()
                    .padding(px(8.0), px(3.0))
                    .on_click(self.finish(block.start)),
            )
    }

    fn past_row(&self, session: &Session, cx: &App) -> Div {
        let theme = *cx.theme();
        let assumed = session.outcome == Outcome::Assumed;
        let id = session.start as usize;

        Self::session_row(
            if assumed {
                theme.pending_bg
            } else {
                theme.card_bg
            },
            if assumed {
                theme.pending_border
            } else {
                theme.rule
            },
        )
        .child(Self::session_text(
            id,
            Self::range_label(session.start, session.end, cx),
            Self::outcome_label(session.outcome),
            if session.outcome == Outcome::Skipped {
                theme.faint_fg
            } else {
                theme.bottom_bar_time_fg
            },
            cx,
        ))
        .child(Self::length(id, session.work, cx))
        .child(self.verdict(session.start))
    }

    fn session_row(bg: Rgba, border: Rgba) -> Div {
        div()
            .flex()
            .items_center()
            .gap(px(9.0))
            .mb(px(4.0))
            .px(px(9.0))
            .py(px(7.0))
            .rounded(px(6.0))
            .border(px(1.0))
            .border_color(border)
            .bg(bg)
    }

    fn session_text(id: usize, range: String, note: &'static str, fg: Rgba, cx: &App) -> Div {
        div()
            .flex_1()
            .min_w_0()
            .child(
                div()
                    .text_size(px(11.5))
                    .text_color(fg)
                    .child(Text::new(("session-range", id).into(), range.into())),
            )
            .child(
                div()
                    .text_size(px(10.0))
                    .text_color(cx.theme().faint_fg)
                    .child(Text::new(("session-note", id).into(), note.into())),
            )
    }

    fn sessions_heading(label: &'static str, progress: Option<(i32, i32)>, cx: &App) -> Div {
        let theme = *cx.theme();
        let progress = progress.map(|(done, total)| {
            format!(
                "{} of {}",
                Self::duration_label(done),
                Self::duration_label(total)
            )
        });

        div()
            .flex()
            .items_center()
            .gap(px(8.0))
            .mt(px(16.0))
            .mb(px(6.0))
            .child(
                div()
                    .flex_none()
                    .text_size(px(11.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme.dim_fg)
                    .child(Text::new(label.into(), label.into())),
            )
            .child(div().flex_1().h(px(1.0)).bg(theme.rule))
            .children(progress.map(|progress| {
                div()
                    .flex_none()
                    .text_size(px(10.5))
                    .text_color(theme.dim_fg)
                    .child(Text::new("sessions-progress".into(), progress.into()))
            }))
    }

    fn delete(&self, cx: &App) -> Stateful<Div> {
        let agenda = self.agenda.clone();
        let task = self.task;

        div()
            .id("delete")
            .role(Role::Button)
            .aria_label("Delete")
            .flex()
            .justify_center()
            .mt(px(12.0))
            .p(px(6.0))
            .rounded(px(5.0))
            .border(px(1.0))
            .border_color(cx.theme().danger_border)
            .bg(cx.theme().card_bg)
            .text_size(px(11.5))
            .text_color(cx.theme().danger_fg)
            .cursor_pointer()
            .hover(|style| style.bg(cx.theme().danger_bg))
            .on_click(move |_event, _window, cx| {
                agenda.update(cx, |agenda, cx| agenda.remove(task, cx));
            })
            .child(Text::new_inaccessible("Delete".into()))
    }

    fn splitting(&self, splittable: bool, editor: &Entity<Self>, cx: &App) -> Div {
        div()
            .child(Self::heading("Splitting", cx))
            .child(Self::splittable(splittable, editor, cx))
            .children(splittable.then(|| self.sessions_row(cx)))
    }

    fn splittable(checked: bool, editor: &Entity<Self>, cx: &App) -> Stateful<Div> {
        let editor = editor.clone();

        div()
            .id("splittable")
            .role(Role::CheckBox)
            .aria_label("Can be broken up")
            .flex()
            .items_center()
            .gap(px(7.0))
            .cursor_pointer()
            .text_size(px(11.5))
            .text_color(cx.theme().chip_fg)
            .on_click(move |_event, _window, cx| {
                editor.update(cx, |editor, cx| editor.set_splittable(!checked, cx));
            })
            .child(Self::checkbox(checked, cx))
            .child(Text::new_inaccessible("Can be broken up".into()))
    }

    fn checkbox(checked: bool, cx: &App) -> Div {
        let theme = *cx.theme();

        div()
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .size(px(13.0))
            .rounded(px(3.0))
            .border(px(1.0))
            .border_color(if checked {
                theme.accent_bg
            } else {
                theme.button_border
            })
            .bg(if checked {
                theme.accent_bg
            } else {
                theme.card_bg
            })
            .text_size(px(9.0))
            .text_color(theme.accent_fg)
            .child(Text::new_inaccessible(
                if checked { "✓" } else { "" }.into(),
            ))
    }

    fn sessions_row(&self, cx: &App) -> Div {
        div()
            .flex()
            .gap(px(6.0))
            .mt(px(8.0))
            .child(Self::labeled(
                "Target",
                Input::new(self.preferred.clone()),
                cx,
            ))
            .child(Self::labeled("Min", Input::new(self.shortest.clone()), cx))
            .child(Self::labeled("Max", Input::new(self.longest.clone()), cx))
    }

    fn breaks(&self, cx: &App) -> Div {
        div()
            .child(Self::heading("Breaks", cx))
            .child(
                div()
                    .flex()
                    .gap(px(8.0))
                    .child(Self::labeled(
                        "Work for (min)",
                        Input::new(self.break_every.clone()),
                        cx,
                    ))
                    .child(Self::labeled(
                        "Then rest (min)",
                        Input::new(self.break_minutes.clone()),
                        cx,
                    )),
            )
            .child(
                div()
                    .mt(px(4.0))
                    .text_size(px(10.5))
                    .text_color(cx.theme().faint_fg)
                    .child(Text::new(
                        "breaks-note".into(),
                        "Set work to 0 for no breaks.".into(),
                    )),
            )
    }

    fn either_side(&self, cx: &App) -> Div {
        div().child(Self::heading("Time either side", cx)).child(
            div()
                .flex()
                .gap(px(8.0))
                .child(Self::labeled(
                    "Prep (min)",
                    Input::new(self.prep.clone()),
                    cx,
                ))
                .child(Self::labeled(
                    "Wrap up (min)",
                    Input::new(self.cleanup.clone()),
                    cx,
                )),
        )
    }

    fn how_much(&self, repeat: Repeat, cx: &App) -> Div {
        div().child(Self::heading("How much", cx)).child(
            div()
                .flex()
                .gap(px(8.0))
                .child(Self::labeled("Minutes", Input::new(self.total.clone()), cx))
                .child(Self::labeled("Repeats", self.repeats(repeat), cx)),
        )
    }

    fn one_off(&self, earliest_day: i32, deadline_day: i32, cx: &App) -> Stateful<Div> {
        let options = Self::day_options(cx);

        div()
            .id("one-off")
            .flex()
            .gap(px(8.0))
            .mt(px(8.0))
            .child(Self::labeled(
                "Not before",
                self.day_select(
                    "earliest",
                    "Not before",
                    &self.earliest,
                    options.clone(),
                    earliest_day,
                    Self::set_earliest,
                ),
                cx,
            ))
            .child(Self::labeled(
                "Due",
                self.day_select(
                    "deadline",
                    "Due",
                    &self.deadline,
                    options,
                    deadline_day,
                    Self::set_deadline,
                ),
                cx,
            ))
    }

    fn day_select(
        &self,
        id: &'static str,
        label: &'static str,
        state: &Entity<SelectState>,
        options: Vec<SharedString>,
        day: i32,
        set: fn(&mut Task, i32),
    ) -> Select {
        let agenda = self.agenda.clone();
        let task = self.task;

        Select::new(id, label, state.clone(), options)
            .selected(day.max(0) as usize)
            .on_select(move |index, _window, cx| {
                agenda.update(cx, |agenda, cx| {
                    agenda.edit(task, |task| set(task, index as i32), cx)
                });
            })
    }

    fn day_options(cx: &App) -> Vec<SharedString> {
        let today = cx.global::<Clock>().now().date_naive();

        (0..10)
            .map(|day| {
                let date = (today + Days::new(day)).format("%a %-d");

                match day {
                    0 => format!("Today ({date})").into(),
                    1 => format!("Tomorrow ({date})").into(),
                    _ => date.to_string().into(),
                }
            })
            .collect()
    }

    fn repeats(&self, repeat: Repeat) -> Select {
        let agenda = self.agenda.clone();
        let task = self.task;
        let options = ["Daily", "Weekly", "Biweekly", "Monthly", "Yearly", "Once"]
            .map(SharedString::from)
            .to_vec();

        Select::new("repeats", "Repeats", self.repeat.clone(), options)
            .selected(Self::repeat_index(repeat))
            .on_select(move |index, _window, cx| {
                agenda.update(cx, |agenda, cx| {
                    agenda.edit(task, |task| Self::set_repeat(task, index), cx)
                });
            })
    }

    fn recurrences(&self, recurrence: Recurrence) -> Select {
        let agenda = self.agenda.clone();
        let task = self.task;
        let options = ["Not repeating", "Each week", "Every other week"]
            .map(SharedString::from)
            .to_vec();

        Select::new("recurrence", "Repeats", self.repeat.clone(), options)
            .selected(Self::recurrence_index(recurrence))
            .on_select(move |index, _window, cx| {
                agenda.update(cx, |agenda, cx| {
                    agenda.edit(task, |task| Self::set_recurrence(task, index), cx)
                });
            })
    }

    fn allowed_hours(&self, cx: &App) -> Div {
        div().child(Self::heading("Allowed hours", cx)).child(
            div()
                .flex()
                .gap(px(8.0))
                .child(Self::labeled(
                    "Not before",
                    Input::new(self.opens.clone()),
                    cx,
                ))
                .child(Self::labeled(
                    "Finished by",
                    Input::new(self.closes.clone()),
                    cx,
                )),
        )
    }

    fn when(&self, recurrence: Recurrence, cx: &App) -> Div {
        div()
            .child(Self::heading("When", cx))
            .child(
                div()
                    .flex()
                    .gap(px(8.0))
                    .child(Self::labeled("Starts", Input::new(self.start.clone()), cx))
                    .child(Self::labeled(
                        "Runs for (min)",
                        Input::new(self.duration.clone()),
                        cx,
                    ))
                    .child(Self::labeled("Repeats", self.recurrences(recurrence), cx)),
            )
            .child(div().mt(px(8.0)).child(Self::labeled(
                "Location",
                Input::new(self.place.clone()),
                cx,
            )))
            .child(div().mt(px(8.0)).child(Self::labeled(
                "Overrun allowance (%)",
                Input::new(self.overrun.clone()),
                cx,
            )))
    }

    fn days(&self, days: &[Weekday], cx: &App) -> Div {
        let week = [
            Weekday::Sun,
            Weekday::Mon,
            Weekday::Tue,
            Weekday::Wed,
            Weekday::Thu,
            Weekday::Fri,
            Weekday::Sat,
        ];

        div().child(Self::heading("Days", cx)).child(
            div()
                .flex()
                .gap(px(3.0))
                .children(week.map(|day| self.day_chip(day, days.contains(&day), cx))),
        )
    }

    fn priorities(&self, priority: Priority, cx: &App) -> Div {
        let choices = [
            (Priority::Lowest, "Lowest"),
            (Priority::Low, "Low"),
            (Priority::Normal, "Normal"),
            (Priority::High, "High"),
            (Priority::Highest, "Highest"),
        ];

        div().flex().gap(px(3.0)).children(
            choices
                .map(|(choice, label)| self.priority_chip(choice, label, choice == priority, cx)),
        )
    }

    fn day_chip(&self, day: Weekday, selected: bool, cx: &App) -> Stateful<Div> {
        let agenda = self.agenda.clone();
        let task = self.task;
        let index = day.num_days_from_sunday() as usize;

        Self::toggle(
            ("day", index),
            ["S", "M", "T", "W", "T", "F", "S"][index].into(),
            selected,
            cx.theme().dim_fg,
            cx,
        )
        .rounded(px(4.0))
        .py(px(4.0))
        .text_size(px(10.5))
        .on_click(move |_event, _window, cx| {
            agenda.update(cx, |agenda, cx| {
                agenda.edit(task, |task| Self::toggle_day(task, day), cx)
            });
        })
    }

    fn priority_chip(
        &self,
        priority: Priority,
        label: &'static str,
        selected: bool,
        cx: &App,
    ) -> Stateful<Div> {
        let agenda = self.agenda.clone();
        let task = self.task;

        Self::toggle(
            ("priority", priority as usize),
            label.into(),
            selected,
            cx.theme().dim_fg,
            cx,
        )
        .rounded(px(4.0))
        .py(px(4.0))
        .text_size(px(10.5))
        .on_click(move |_event, _window, cx| {
            agenda.update(cx, |agenda, cx| {
                agenda.edit(task, |task| task.priority = priority, cx)
            });
        })
    }

    fn kind_button(
        id: &'static str,
        label: &'static str,
        selects_fixed: bool,
        fixed: bool,
        editor: &Entity<Self>,
        cx: &App,
    ) -> Stateful<Div> {
        let editor = editor.clone();

        Self::toggle(
            id,
            label.into(),
            selects_fixed == fixed,
            cx.theme().muted_fg,
            cx,
        )
        .rounded(px(5.0))
        .p(px(5.0))
        .text_size(px(11.5))
        .font_weight(FontWeight::MEDIUM)
        .on_click(move |_event, _window, cx| {
            editor.update(cx, |editor, cx| editor.set_kind(selects_fixed, cx));
        })
    }
}

impl Render for Editor {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let page = div()
            .id("editor")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .pt(px(10.0))
            .px(px(12.0))
            .pb(px(18.0))
            .child(self.back(cx));
        let Some(task) = self.agenda.read(cx).task(self.task) else {
            return page;
        };
        let fixed = matches!(task.kind, TaskKind::Fixed { .. });
        let splittable = task.splittable();
        let priority = task.priority;
        let days = task.days.clone();
        let repeat = match &task.kind {
            TaskKind::Flexible(flexible) => flexible.repeat,
            TaskKind::Fixed { .. } => Repeat::Daily,
        };
        let recurrence = match task.kind {
            TaskKind::Fixed { recurrence, .. } => recurrence,
            TaskKind::Flexible(_) => Recurrence::Weekly,
        };
        let editor = cx.entity();

        page.child(
            div().font_weight(FontWeight::SEMIBOLD).child(
                Input::new(self.title.clone())
                    .text_size(px(14.0))
                    .padding(px(8.0), px(6.0)),
            ),
        )
        .child(Self::kinds(fixed, &editor, cx))
        .child(Self::heading("Priority", cx))
        .child(self.priorities(priority, cx))
        .children(fixed.then(|| self.when(recurrence, cx)))
        .children((!fixed).then(|| self.how_much(repeat, cx)))
        .children(match repeat {
            Repeat::Once {
                earliest_day,
                deadline_day,
            } => Some(self.one_off(earliest_day, deadline_day, cx)),
            _ => None,
        })
        .children((!fixed).then(|| self.splitting(splittable, &editor, cx)))
        .children((!fixed).then(|| self.allowed_hours(cx)))
        .children((!matches!(repeat, Repeat::Once { .. })).then(|| self.days(&days, cx)))
        .child(self.either_side(cx))
        .children((!fixed).then(|| self.breaks(cx)))
        .children(
            (!fixed)
                .then(|| self.sessions_list(splittable, cx))
                .flatten(),
        )
        .child(self.delete(cx))
    }
}
