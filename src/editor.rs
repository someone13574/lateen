use std::ops::Range;

use chrono::{Days, TimeDelta, Weekday};
use gpui::prelude::*;
use gpui::{
    App, ClickEvent, Div, ElementId, Entity, FocusHandle, FontWeight, KeyBinding, Rgba, Role,
    SharedString, Stateful, Text, Window, actions, div, px,
};

use crate::agenda::Agenda;
use crate::block::Block;
use crate::button::{Button, ClickHandler};
use crate::calendar_list::CalendarList;
use crate::clock::{Clock, ClockFormat};
use crate::input::{Entry, Input, InputEvent, InputState};
use crate::select::{Select, SelectState};
use crate::selectable_text::SelectableText;
use crate::session::{Outcome, Session};
use crate::subscription::SubscriptionId;
use crate::task::{
    Dates, Flexible, Priority, Recurrence, Repeat, Sessions, Task, TaskId, TaskKind,
};
use crate::theme::{ActiveTheme, Theme};

actions!(day_chip, [FocusNext, FocusPrevious, Toggle]);

#[derive(Clone, Copy)]
struct DayField {
    id: &'static str,
    label: &'static str,
    open_ended: &'static str,
    first: i32,
}

pub struct Editor {
    agenda: Entity<Agenda>,
    calendars: Entity<CalendarList>,
    task: TaskId,
    title: Entity<InputState>,
    start: Entity<InputState>,
    duration: Entity<InputState>,
    place: Entity<InputState>,
    overrun: Entity<InputState>,
    total: Entity<InputState>,
    allowed_start: Entity<InputState>,
    allowed_end: Entity<InputState>,
    preferred: Entity<InputState>,
    shortest: Entity<InputState>,
    longest: Entity<InputState>,
    transition_start: Entity<InputState>,
    transition_end: Entity<InputState>,
    priority: Entity<SelectState>,
    repeat: Entity<SelectState>,
    earliest: Entity<SelectState>,
    deadline: Entity<SelectState>,
    days: [FocusHandle; 7],
}

impl Editor {
    const READONLY_OPACITY: f32 = 0.55;
    const BANNER_GROUP: &'static str = "calendar-banner";
    const DAY_KEY_CONTEXT: &'static str = "DayChip";

    pub fn init(cx: &mut App) {
        cx.bind_keys([
            KeyBinding::new("enter", Toggle, Some(Self::DAY_KEY_CONTEXT)),
            KeyBinding::new("space", Toggle, Some(Self::DAY_KEY_CONTEXT)),
            KeyBinding::new("tab", FocusNext, Some(Self::DAY_KEY_CONTEXT)),
            KeyBinding::new("shift-tab", FocusPrevious, Some(Self::DAY_KEY_CONTEXT)),
        ]);
    }

    pub fn new(
        agenda: Entity<Agenda>,
        calendars: Entity<CalendarList>,
        task: TaskId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let editor = Self {
            agenda,
            calendars,
            task,
            title: Self::text_field(Self::set_title, window, cx),
            start: Self::field(Entry::Time, 0, Self::set_start, window, cx),
            duration: Self::field(Entry::Duration, 1, Self::set_duration, window, cx),
            place: Self::text_field(Self::set_place, window, cx),
            overrun: Self::field(Entry::Number, 0, Self::set_overrun, window, cx),
            total: Self::field(Entry::Duration, 1, Self::set_total, window, cx),
            allowed_start: Self::field(Entry::Time, 0, Self::set_allowed_start, window, cx),
            allowed_end: Self::field(Entry::Time, 0, Self::set_allowed_end, window, cx),
            preferred: Self::field(Entry::Duration, 1, Self::set_preferred, window, cx),
            shortest: Self::field(Entry::Duration, 1, Self::set_shortest, window, cx),
            longest: Self::field(Entry::Duration, 1, Self::set_longest, window, cx),
            transition_start: Self::field(
                Entry::Duration,
                0,
                Self::set_transition_start,
                window,
                cx,
            ),
            transition_end: Self::field(Entry::Duration, 0, Self::set_transition_end, window, cx),
            priority: cx.new(|cx| SelectState::new(window, cx)),
            repeat: cx.new(|cx| SelectState::new(window, cx)),
            earliest: cx.new(|cx| SelectState::new(window, cx)),
            deadline: cx.new(|cx| SelectState::new(window, cx)),
            days: std::array::from_fn(|_| cx.focus_handle().tab_stop(true)),
        };

        editor.reseed(cx);
        editor.follow_source(cx);

        editor
    }

    pub fn task(&self) -> TaskId {
        self.task
    }

    fn field(
        entry: Entry,
        minimum: i32,
        apply: fn(&mut Task, i32),
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<InputState> {
        let state = cx.new(|cx| InputState::new(entry, window, cx));

        cx.subscribe(&state, move |editor, state, event: &InputEvent, cx| {
            let value = entry
                .parse(state.read(cx).content())
                .filter(|value| *value >= minimum);
            let task = editor.task;

            if let Some(value) = value {
                editor.agenda.update(cx, |agenda, cx| {
                    agenda.edit(task, |task| apply(task, value), cx)
                });
            }

            state.update(cx, |state, cx| state.set_invalid(value.is_none(), cx));

            if matches!(event, InputEvent::Committed) && value.is_some() {
                editor.commit(&state, cx);
            }
        })
        .detach();

        state
    }

    fn text_field(
        apply: fn(&mut Task, &str),
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<InputState> {
        let state = cx.new(|cx| InputState::new(Entry::Text, window, cx));

        cx.subscribe(&state, move |editor, state, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Committed) {
                return;
            }

            let text = state.read(cx).content().clone();
            let task = editor.task;

            editor.agenda.update(cx, |agenda, cx| {
                agenda.edit(task, |task| apply(task, &text), cx)
            });
        })
        .detach();

        state
    }

    fn commit(&self, state: &Entity<InputState>, cx: &mut Context<Self>) {
        let Some((_, value)) = self
            .values(cx)
            .into_iter()
            .find(|(field, _)| field == state)
        else {
            return;
        };

        state.update(cx, |state, cx| state.set_content(value, cx));
    }

    fn follow_source(&self, cx: &mut Context<Self>) {
        cx.observe(&self.agenda.clone(), |editor, _agenda, cx| {
            for (state, value) in editor.managed_values(cx) {
                if state.read(cx).content() != &value {
                    state.update(cx, |state, cx| state.set_content(value, cx));
                }
            }
        })
        .detach();
    }

    fn managed_values(&self, cx: &App) -> Vec<(Entity<InputState>, SharedString)> {
        let Some(task) = self.agenda.read(cx).task(self.task) else {
            return Vec::new();
        };
        let TaskKind::Fixed {
            start, duration, ..
        } = &task.kind
        else {
            return Vec::new();
        };

        if task.source.is_none() {
            return Vec::new();
        }

        vec![
            (self.title.clone(), task.title.clone()),
            (
                self.start.clone(),
                cx.global::<ClockFormat>().time_label(*start).into(),
            ),
            (self.duration.clone(), duration.to_string().into()),
            (self.place.clone(), task.place.clone().unwrap_or_default()),
        ]
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
            (self.transition_start.clone(), task.prep.to_string()),
            (self.transition_end.clone(), task.cleanup.to_string()),
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
            (
                self.allowed_start.clone(),
                clock.time_label(flexible.window.start),
            ),
            (
                self.allowed_end.clone(),
                clock.time_label(flexible.window.end),
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
            Repeat::Never => 0,
            Repeat::Daily => 1,
            Repeat::Weekly => 2,
            Repeat::Biweekly => 3,
            Repeat::Monthly => 4,
            Repeat::Yearly => 5,
        }
    }

    fn set_repeat(task: &mut Task, index: usize) {
        let Some(flexible) = Self::flexible(task) else {
            return;
        };

        flexible.repeat = match index {
            1 => Repeat::Daily,
            2 => Repeat::Weekly,
            3 => Repeat::Biweekly,
            4 => Repeat::Monthly,
            5 => Repeat::Yearly,
            _ => Repeat::Never,
        };

        Self::anchor(task, index == 0);
    }

    fn recurrence_index(recurrence: Recurrence) -> usize {
        match recurrence {
            Recurrence::Never => 0,
            Recurrence::Weekly => 1,
            Recurrence::Biweekly => 2,
            Recurrence::Monthly => 3,
            Recurrence::Yearly => 4,
        }
    }

    fn set_recurrence(task: &mut Task, index: usize) {
        let TaskKind::Fixed { recurrence, .. } = &mut task.kind else {
            return;
        };

        *recurrence = match index {
            1 => Recurrence::Weekly,
            2 => Recurrence::Biweekly,
            _ => Recurrence::Never,
        };

        Self::anchor(task, index == 0);
    }

    fn anchor(task: &mut Task, once: bool) {
        if once && task.dates.from.is_none() {
            task.dates.from = Some(0);
        }
    }

    fn set_from(task: &mut Task, day: Option<i32>) {
        task.dates.from = day;
        Self::anchor(task, !task.repeats());

        if let (Some(from), Some(until)) = (task.dates.from, task.dates.until) {
            task.dates.until = Some(until.max(from));
        }
    }

    fn set_until(task: &mut Task, day: Option<i32>) {
        task.dates.until = day;

        if let (Some(until), Some(from)) = (day, task.dates.from) {
            task.dates.from = Some(from.min(until));
        }
    }

    fn set_title(task: &mut Task, text: &str) {
        task.title = text.to_string().into();
    }

    fn set_place(task: &mut Task, text: &str) {
        task.place = (!text.trim().is_empty()).then(|| text.to_string().into());
    }

    fn set_transition_start(task: &mut Task, minutes: i32) {
        task.prep = minutes;
    }

    fn set_transition_end(task: &mut Task, minutes: i32) {
        task.cleanup = minutes;
    }

    fn set_start(task: &mut Task, minutes: i32) {
        if let TaskKind::Fixed { start, .. } = &mut task.kind {
            *start = minutes;
        }
    }

    fn set_duration(task: &mut Task, minutes: i32) {
        if let TaskKind::Fixed { duration, .. } = &mut task.kind {
            *duration = minutes;
        }
    }

    fn set_overrun(task: &mut Task, percent: i32) {
        if let TaskKind::Fixed {
            overrun_percent, ..
        } = &mut task.kind
        {
            *overrun_percent = percent;
        }
    }

    fn set_total(task: &mut Task, minutes: i32) {
        if let Some(flexible) = Self::flexible(task) {
            flexible.total = minutes;
        }
    }

    fn set_allowed_start(task: &mut Task, minutes: i32) {
        if let Some(flexible) = Self::flexible(task) {
            flexible.window.start = minutes;
        }
    }

    fn set_allowed_end(task: &mut Task, minutes: i32) {
        if let Some(flexible) = Self::flexible(task) {
            flexible.window.end = minutes;
        }
    }

    fn set_preferred(task: &mut Task, minutes: i32) {
        if let Some(sessions) = Self::sessions(task) {
            sessions.preferred = minutes;
        }
    }

    fn set_shortest(task: &mut Task, minutes: i32) {
        if let Some(sessions) = Self::sessions(task) {
            sessions.shortest = minutes;
        }
    }

    fn set_longest(task: &mut Task, minutes: i32) {
        if let Some(sessions) = Self::sessions(task) {
            sessions.longest = minutes;
        }
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

    fn provenance(&self, subscription: SubscriptionId, cx: &App) -> Div {
        let name = self
            .agenda
            .read(cx)
            .subscriptions()
            .iter()
            .find(|other| other.id == subscription)
            .map(|subscription| subscription.name.clone())
            .unwrap_or_default();

        div().flex_1().min_w_0().child(
            div()
                .truncate()
                .text_size(px(11.5))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(cx.theme().heading_fg)
                .child(Text::new(
                    "managed-by".into(),
                    format!("Managed by {name}").into(),
                )),
        )
    }

    fn open_settings(
        &self,
        subscription: SubscriptionId,
    ) -> impl Fn(&ClickEvent, &mut Window, &mut App) + 'static {
        let agenda = self.agenda.clone();
        let calendars = self.calendars.clone();

        move |_event, _window, cx| {
            agenda.update(cx, Agenda::deselect);
            calendars.update(cx, |calendars, cx| {
                calendars.show_settings(subscription, cx)
            });
        }
    }

    fn opener(theme: Theme) -> Div {
        div()
            .flex_none()
            .text_size(px(13.0))
            .text_color(theme.dim_fg)
            .group_hover(Self::BANNER_GROUP, |style| style.text_color(theme.link_fg))
            .child(Text::new_inaccessible("\u{203a}".into()))
    }

    fn banner(&self, subscription: SubscriptionId, cx: &App) -> Stateful<Div> {
        let theme = *cx.theme();

        div()
            .id("calendar-settings")
            .role(Role::Button)
            .aria_label("Calendar settings")
            .group(Self::BANNER_GROUP)
            .flex()
            .items_center()
            .gap(px(8.0))
            .mb(px(9.0))
            .px(px(10.0))
            .py(px(8.0))
            .rounded(px(5.0))
            .border(px(1.0))
            .border_color(theme.button_border)
            .bg(theme.card_bg)
            .text_color(theme.muted_fg)
            .cursor_pointer()
            .hover(|style| style.border_color(theme.chip_border).bg(theme.row_hover_bg))
            .on_click(self.open_settings(subscription))
            .child(self.provenance(subscription, cx))
            .child(Self::opener(theme))
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
        readonly: bool,
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
            .when(!readonly, |toggle| toggle.cursor_pointer())
            .when(readonly, |toggle| toggle.opacity(Self::READONLY_OPACITY))
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

    fn kinds(fixed: bool, readonly: bool, editor: &Entity<Self>, cx: &App) -> Div {
        div()
            .flex()
            .gap(px(4.0))
            .mt(px(9.0))
            .child(Self::kind_button(
                "kind-fixed",
                "Fixed time",
                true,
                fixed,
                readonly,
                editor,
                cx,
            ))
            .child(Self::kind_button(
                "kind-flexible",
                "Flexible",
                false,
                fixed,
                readonly,
                editor,
                cx,
            ))
    }

    fn sessions_list(&self, splittable: bool, cx: &App) -> Option<Div> {
        let clock = cx.global::<Clock>().minute_of_day();
        let now = clock as i32;
        let agenda = self.agenda.read(cx);
        let task = agenda.task(self.task)?;
        let mut ahead: Vec<_> = agenda
            .schedule()
            .blocks()
            .filter(|block| block.task == self.task && block.end() > now)
            .cloned()
            .collect();

        ahead.sort_by_key(|block| block.start);

        let run = Self::run(task, &ahead, agenda, now);
        let past = agenda.logged(self.task, &run);
        let mut future: Vec<_> = ahead
            .into_iter()
            .filter(|block| run.contains(&block.start))
            .collect();

        if !splittable {
            future.truncate(past.is_empty() as usize);
        }

        Some(
            div()
                .child(Self::sessions_heading(
                    agenda.progress(task, now).filter(|_| splittable),
                    cx,
                ))
                .children(past.iter().map(|session| self.past_row(session, task, cx)))
                .children(future.iter().map(|block| self.future_row(block, clock, cx))),
        )
    }

    fn run(task: &Task, ahead: &[Block], agenda: &Agenda, now: i32) -> Range<i32> {
        let current = task.run(now.div_euclid(Block::MINUTES_PER_DAY), agenda.today());
        let started = !agenda.logged(task.id, &current).is_empty()
            || ahead.iter().any(|block| current.contains(&block.start));

        match ahead.first() {
            Some(next) if !started => task.run(
                next.start.div_euclid(Block::MINUTES_PER_DAY),
                agenda.today(),
            ),
            _ => current,
        }
    }

    fn length(id: usize, minutes: i32, cx: &App) -> Div {
        div()
            .flex_none()
            .text_size(px(10.5))
            .text_color(cx.theme().dim_fg)
            .child(SelectableText::new(
                ("session-length", id),
                Self::duration_label(minutes),
            ))
    }

    fn range_label(range: Range<i32>, cx: &App) -> String {
        let clock = *cx.global::<ClockFormat>();

        format!(
            "{}, {} to {}",
            Self::day_tag(range.start.div_euclid(Block::MINUTES_PER_DAY), cx),
            clock.time_label(range.start),
            clock.time_label(range.end)
        )
    }

    fn day_tag(day: i32, cx: &App) -> String {
        match day {
            0 => "Today".to_string(),
            1 => "Tomorrow".to_string(),
            day => (cx.global::<Clock>().now().date_naive() + Days::new(day.max(0) as u64))
                .format("%a %-d")
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

    fn countdown(minutes: f32) -> String {
        let seconds = (minutes.max(0.0) * 60.0).round() as i32;

        match (seconds / 3600, seconds / 60 % 60, seconds % 60) {
            (0, minutes, seconds) => format!("{minutes:02}:{seconds:02}"),
            (hours, minutes, seconds) => format!("{hours}:{minutes:02}:{seconds:02}"),
        }
    }

    fn verdict(&self, session: &Session, cx: &App) -> Div {
        let theme = *cx.theme();
        let start = session.start;
        let swatch = self
            .agenda
            .read(cx)
            .task(self.task)
            .and_then(|task| task.color)
            .map_or(theme.accent_bg, |color| theme.swatch(color));

        div()
            .flex()
            .flex_none()
            .gap(px(3.0))
            .child(
                Button::new(("session-done", start as usize), "✓")
                    .fixed(px(22.0), px(20.0))
                    .when(session.outcome == Outcome::Done, |button| {
                        button.active(swatch, theme.accent_fg)
                    })
                    .on_click(self.settle(start, Outcome::Done)),
            )
            .child(
                Button::new(("session-skip", start as usize), "✗")
                    .fixed(px(22.0), px(20.0))
                    .when(session.outcome == Outcome::Skipped, |button| {
                        button.active(theme.danger_fg, theme.card_bg)
                    })
                    .on_click(self.settle(start, Outcome::Skipped)),
            )
    }

    fn settle(&self, start: i32, outcome: Outcome) -> Box<ClickHandler> {
        let agenda = self.agenda.clone();
        let task = self.task;

        Box::new(move |_window, cx| {
            agenda.update(cx, |agenda, cx| agenda.toggle(task, start, outcome, cx));
        })
    }

    fn finish(&self, start: i32) -> Box<ClickHandler> {
        let agenda = self.agenda.clone();
        let task = self.task;

        Box::new(move |_window, cx| {
            agenda.update(cx, |agenda, cx| agenda.finish(task, start, cx));
        })
    }

    fn future_row(&self, block: &Block, now: f32, cx: &App) -> Div {
        let theme = *cx.theme();
        let start = block.start;
        let running = block.start as f32 <= now;
        let border = match (running, block.color) {
            (true, Some(color)) => theme.now_card(color).border,
            _ => theme.rule,
        };

        Self::session_row(theme.card_bg, border)
            .child(Self::session_text(
                start as usize,
                Self::range_label(block.work_range(), cx),
                match running {
                    true => format!(
                        "running, {} left",
                        Self::countdown(block.end() as f32 - now)
                    )
                    .into(),
                    false => "planned".into(),
                },
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

    fn past_row(&self, session: &Session, task: &Task, cx: &App) -> Div {
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
            Self::range_label(session.work_range(task), cx),
            Self::outcome_label(session.outcome).into(),
            if session.outcome == Outcome::Skipped {
                theme.faint_fg
            } else {
                theme.bottom_bar_time_fg
            },
            cx,
        ))
        .child(Self::length(id, session.work, cx))
        .child(self.verdict(session, cx))
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

    fn session_text(id: usize, range: String, note: SharedString, fg: Rgba, cx: &App) -> Div {
        div()
            .flex_1()
            .min_w_0()
            .child(
                div()
                    .text_size(px(11.5))
                    .text_color(fg)
                    .child(SelectableText::new(("session-range", id), range)),
            )
            .child(
                div()
                    .text_size(px(10.0))
                    .text_color(cx.theme().faint_fg)
                    .child(Text::new(("session-note", id).into(), note)),
            )
    }

    fn sessions_heading(progress: Option<(i32, i32)>, cx: &App) -> Div {
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
                    .flex_1()
                    .min_w_0()
                    .text_size(px(11.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme.heading_fg)
                    .child(Text::new("sessions-heading".into(), "Sessions".into())),
            )
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

    fn splitting(&self, splittable: bool, readonly: bool, editor: &Entity<Self>, cx: &App) -> Div {
        div()
            .child(Self::heading("Splitting", cx))
            .child(Self::splittable(splittable, readonly, editor, cx))
            .children(splittable.then(|| self.sessions_row(readonly, cx)))
    }

    fn splittable(checked: bool, readonly: bool, editor: &Entity<Self>, cx: &App) -> Stateful<Div> {
        let editor = editor.clone();

        div()
            .id("splittable")
            .role(Role::CheckBox)
            .aria_label("Can be split")
            .flex()
            .items_center()
            .gap(px(7.0))
            .text_size(px(11.5))
            .text_color(cx.theme().chip_fg)
            .when(!readonly, |field| {
                field.cursor_pointer().on_click(move |_event, _window, cx| {
                    editor.update(cx, |editor, cx| editor.set_splittable(!checked, cx));
                })
            })
            .child(Self::checkbox(checked, cx))
            .child(Text::new_inaccessible("Can be split".into()))
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

    fn sessions_row(&self, readonly: bool, cx: &App) -> Div {
        div()
            .flex()
            .gap(px(6.0))
            .mt(px(8.0))
            .child(Self::labeled(
                "Target",
                Input::new(self.preferred.clone()).readonly(readonly),
                cx,
            ))
            .child(Self::labeled(
                "Min",
                Input::new(self.shortest.clone()).readonly(readonly),
                cx,
            ))
            .child(Self::labeled(
                "Max",
                Input::new(self.longest.clone()).readonly(readonly),
                cx,
            ))
    }

    fn transition_time(&self, cx: &App) -> Div {
        div().child(Self::heading("Transition time", cx)).child(
            div()
                .flex()
                .gap(px(8.0))
                .child(Self::labeled(
                    "Start (min)",
                    Input::new(self.transition_start.clone()),
                    cx,
                ))
                .child(Self::labeled(
                    "End (min)",
                    Input::new(self.transition_end.clone()),
                    cx,
                )),
        )
    }

    fn amount(&self, repeat: Repeat, readonly: bool, cx: &App) -> Div {
        div().child(Self::heading("Amount", cx)).child(
            div()
                .flex()
                .gap(px(8.0))
                .child(Self::labeled(
                    "Minutes",
                    Input::new(self.total.clone()).readonly(readonly),
                    cx,
                ))
                .child(Self::labeled("Repeats", self.repeats(repeat, readonly), cx)),
        )
    }

    const FROM: DayField = DayField {
        id: "from",
        label: "From",
        open_ended: "Today",
        first: 1,
    };
    const UNTIL: DayField = DayField {
        id: "until",
        label: "Until",
        open_ended: "Forever",
        first: 0,
    };
    const DAY_CHOICES: i32 = 10;

    fn dates(&self, dates: Dates, readonly: bool, cx: &App) -> Stateful<Div> {
        div()
            .id("dates")
            .flex()
            .gap(px(8.0))
            .mt(px(8.0))
            .child(Self::labeled(
                Self::FROM.label,
                self.day_select(
                    Self::FROM,
                    &self.earliest,
                    dates.from,
                    Self::set_from,
                    readonly,
                    cx,
                ),
                cx,
            ))
            .child(Self::labeled(
                Self::UNTIL.label,
                self.day_select(
                    Self::UNTIL,
                    &self.deadline,
                    dates.until,
                    Self::set_until,
                    readonly,
                    cx,
                ),
                cx,
            ))
    }

    fn day_select(
        &self,
        field: DayField,
        state: &Entity<SelectState>,
        day: Option<i32>,
        set: fn(&mut Task, Option<i32>),
        readonly: bool,
        cx: &App,
    ) -> Select {
        let agenda = self.agenda.clone();
        let task = self.task;
        let first = field.first;

        let choices = first..first + Self::DAY_CHOICES;
        let mut options = Self::day_options(field, cx);
        let listed = day.filter(|day| choices.contains(day));
        let selected = match (day, listed) {
            (None, _) => 0,
            (Some(_), Some(day)) => (day - first) as usize + 1,
            (Some(day), None) => {
                options.push(Self::day_label(day, cx));

                options.len() - 1
            }
        };

        Select::new(field.id, field.label, state.clone(), options)
            .selected(selected)
            .readonly(readonly)
            .on_select(move |index, _window, cx| {
                let day = match index {
                    0 => None,
                    index if index <= Self::DAY_CHOICES as usize => Some(index as i32 - 1 + first),
                    _ => return,
                };

                agenda.update(cx, |agenda, cx| {
                    agenda.edit(task, |task| set(task, day), cx)
                });
            })
    }

    fn day_label(day: i32, cx: &App) -> SharedString {
        let date = cx.global::<Clock>().now().date_naive() + TimeDelta::days(i64::from(day));

        date.format("%-d %b %Y").to_string().into()
    }

    fn day_options(field: DayField, cx: &App) -> Vec<SharedString> {
        let today = cx.global::<Clock>().now().date_naive();

        std::iter::once(SharedString::from(field.open_ended))
            .chain((field.first..field.first + Self::DAY_CHOICES).map(|day| {
                let date = (today + Days::new(day as u64)).format("%a %-d");

                match day {
                    0 => format!("Today ({date})").into(),
                    1 => format!("Tomorrow ({date})").into(),
                    _ => date.to_string().into(),
                }
            }))
            .collect()
    }

    fn repeats(&self, repeat: Repeat, readonly: bool) -> Select {
        let agenda = self.agenda.clone();
        let task = self.task;
        let options = [
            "Doesn't repeat",
            "Daily",
            "Weekly",
            "Biweekly",
            "Monthly",
            "Yearly",
        ]
        .map(SharedString::from)
        .to_vec();

        Select::new("repeats", "Repeats", self.repeat.clone(), options)
            .selected(Self::repeat_index(repeat))
            .readonly(readonly)
            .on_select(move |index, _window, cx| {
                agenda.update(cx, |agenda, cx| {
                    agenda.edit(task, |task| Self::set_repeat(task, index), cx)
                });
            })
    }

    fn recurrences(&self, recurrence: Recurrence, readonly: bool) -> Select {
        let agenda = self.agenda.clone();
        let task = self.task;
        let mut options = ["Doesn't repeat", "Weekly", "Biweekly"]
            .map(SharedString::from)
            .to_vec();

        if readonly {
            options.extend(["Monthly".into(), "Yearly".into()]);
        }

        Select::new("recurrence", "Repeats", self.repeat.clone(), options)
            .selected(Self::recurrence_index(recurrence))
            .readonly(readonly)
            .on_select(move |index, _window, cx| {
                agenda.update(cx, |agenda, cx| {
                    agenda.edit(task, |task| Self::set_recurrence(task, index), cx)
                });
            })
    }

    fn allowed_time_range(&self, readonly: bool, cx: &App) -> Div {
        div().child(Self::heading("Allowed time range", cx)).child(
            div()
                .flex()
                .gap(px(8.0))
                .child(Self::labeled(
                    "Start",
                    Input::new(self.allowed_start.clone()).readonly(readonly),
                    cx,
                ))
                .child(Self::labeled(
                    "End",
                    Input::new(self.allowed_end.clone()).readonly(readonly),
                    cx,
                )),
        )
    }

    fn when(&self, recurrence: Recurrence, readonly: bool, cx: &App) -> Div {
        div()
            .child(Self::heading("When", cx))
            .child(
                div()
                    .flex()
                    .gap(px(8.0))
                    .child(Self::labeled(
                        "Start",
                        Input::new(self.start.clone()).readonly(readonly),
                        cx,
                    ))
                    .child(Self::labeled(
                        "Duration (min)",
                        Input::new(self.duration.clone()).readonly(readonly),
                        cx,
                    ))
                    .child(Self::labeled(
                        "Repeats",
                        self.recurrences(recurrence, readonly),
                        cx,
                    )),
            )
            .child(div().mt(px(8.0)).child(Self::labeled(
                "Location",
                Input::new(self.place.clone()).readonly(readonly),
                cx,
            )))
            .child(div().mt(px(8.0)).child(Self::labeled(
                "Overrun allowance (%)",
                Input::new(self.overrun.clone()),
                cx,
            )))
    }

    fn days(&self, days: &[Weekday], readonly: bool, window: &Window, cx: &App) -> Div {
        let week = [
            Weekday::Sun,
            Weekday::Mon,
            Weekday::Tue,
            Weekday::Wed,
            Weekday::Thu,
            Weekday::Fri,
            Weekday::Sat,
        ];

        div()
            .child(Self::heading("Days", cx))
            .child(div().flex().gap(px(3.0)).children(
                week.map(|day| self.day_chip(day, days.contains(&day), readonly, window, cx)),
            ))
    }

    fn priorities(&self, priority: Priority) -> Select {
        let agenda = self.agenda.clone();
        let task = self.task;
        let options = ["Lowest", "Low", "Normal", "High", "Highest"]
            .map(SharedString::from)
            .to_vec();

        Select::new("priority", "Priority", self.priority.clone(), options)
            .selected(priority as usize)
            .on_select(move |index, _window, cx| {
                agenda.update(cx, |agenda, cx| {
                    agenda.edit(task, |task| task.priority = Self::ranked(index), cx)
                });
            })
    }

    fn ranked(index: usize) -> Priority {
        match index {
            0 => Priority::Lowest,
            1 => Priority::Low,
            3 => Priority::High,
            4 => Priority::Highest,
            _ => Priority::Normal,
        }
    }

    fn day_toggler(&self, day: Weekday) -> impl Fn(&mut App) + 'static {
        let agenda = self.agenda.clone();
        let task = self.task;

        move |cx| {
            agenda.update(cx, |agenda, cx| {
                agenda.edit(task, |task| Self::toggle_day(task, day), cx)
            });
        }
    }

    fn interactive_day(
        &self,
        chip: Stateful<Div>,
        day: Weekday,
        window: &Window,
        cx: &App,
    ) -> Stateful<Div> {
        let focus_handle = self.days[day.num_days_from_sunday() as usize].clone();
        let toggle = self.day_toggler(day);
        let click = self.day_toggler(day);

        chip.relative()
            .key_context(Self::DAY_KEY_CONTEXT)
            .track_focus(&focus_handle)
            .on_action(move |_: &Toggle, _window, cx| toggle(cx))
            .on_action(|_: &FocusNext, window: &mut Window, cx: &mut App| window.focus_next(cx))
            .on_action(|_: &FocusPrevious, window: &mut Window, cx: &mut App| window.focus_prev(cx))
            .on_click(move |_event, _window, cx| click(cx))
            .when(focus_handle.is_focused(window), |chip| {
                chip.border_color(cx.theme().input_ring)
                    .child(Self::ring(cx.theme().input_ring))
            })
    }

    fn ring(color: Rgba) -> Div {
        div()
            .absolute()
            .inset(px(-1.0))
            .border(px(2.0))
            .rounded(px(5.0))
            .border_color(color)
    }

    fn day_chip(
        &self,
        day: Weekday,
        selected: bool,
        readonly: bool,
        window: &Window,
        cx: &App,
    ) -> Stateful<Div> {
        let index = day.num_days_from_sunday() as usize;

        Self::toggle(
            ("day", index),
            ["S", "M", "T", "W", "T", "F", "S"][index].into(),
            selected,
            cx.theme().dim_fg,
            readonly,
            cx,
        )
        .rounded(px(4.0))
        .py(px(4.0))
        .text_size(px(10.5))
        .when(!readonly, |chip| {
            self.interactive_day(chip, day, window, cx)
        })
    }

    fn kind_button(
        id: &'static str,
        label: &'static str,
        selects_fixed: bool,
        fixed: bool,
        readonly: bool,
        editor: &Entity<Self>,
        cx: &App,
    ) -> Stateful<Div> {
        let editor = editor.clone();

        Self::toggle(
            id,
            label.into(),
            selects_fixed == fixed,
            cx.theme().muted_fg,
            readonly,
            cx,
        )
        .rounded(px(5.0))
        .p(px(5.0))
        .text_size(px(11.5))
        .font_weight(FontWeight::MEDIUM)
        .when(!readonly, |button| {
            button.on_click(move |_event, _window, cx| {
                editor.update(cx, |editor, cx| editor.set_kind(selects_fixed, cx));
            })
        })
    }
}

impl Render for Editor {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
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
        let managed = task.source.as_ref().map(|source| source.subscription);
        let readonly = managed.is_some();
        let form = div()
            .child(
                div().font_weight(FontWeight::SEMIBOLD).child(
                    Input::new(self.title.clone())
                        .text_size(px(14.0))
                        .padding(px(8.0), px(6.0))
                        .readonly(readonly),
                ),
            )
            .child(Self::kinds(fixed, readonly, &editor, cx))
            .child(Self::heading("Priority", cx))
            .child(self.priorities(priority))
            .children(fixed.then(|| self.when(recurrence, readonly, cx)))
            .children((!fixed).then(|| self.amount(repeat, readonly, cx)))
            .child(self.dates(task.dates, readonly, cx))
            .children((!fixed).then(|| self.splitting(splittable, readonly, &editor, cx)))
            .children((!fixed).then(|| self.allowed_time_range(readonly, cx)))
            .children(
                task.repeats_by_weekday()
                    .then(|| self.days(&days, readonly, window, cx)),
            )
            .child(self.transition_time(cx));

        page.children(managed.map(|subscription| self.banner(subscription, cx)))
            .child(form)
            .children(
                (!fixed)
                    .then(|| self.sessions_list(splittable, cx))
                    .flatten(),
            )
            .children((!readonly).then(|| self.delete(cx)))
    }
}
