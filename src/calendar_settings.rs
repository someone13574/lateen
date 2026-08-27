use std::collections::HashSet;

use gpui::prelude::*;
use gpui::{App, Div, Entity, FocusHandle, FontWeight, Role, Text, Window, div, px};

use crate::agenda::Agenda;
use crate::button::Button;
use crate::calendar_list::CalendarList;
use crate::input::{Entry, Input, InputEvent, InputState};
use crate::select::{Select, SelectState};
use crate::subscription::{CalendarDefaults, SubscriptionId, Transitions};
use crate::task::Priority;
use crate::theme::ActiveTheme;

pub struct CalendarSettings {
    agenda: Entity<Agenda>,
    calendars: Entity<CalendarList>,
    subscription: SubscriptionId,
    overrun: Entity<InputState>,
    prep: Entity<InputState>,
    cleanup: Entity<InputState>,
    start_percent: Entity<InputState>,
    end_percent: Entity<InputState>,
    shortest: Entity<InputState>,
    longest: Entity<InputState>,
    priority: Entity<SelectState>,
    modes: [FocusHandle; 2],
    sync: FocusHandle,
    remove: FocusHandle,
    reset: FocusHandle,
}

impl CalendarSettings {
    pub fn new(
        agenda: Entity<Agenda>,
        calendars: Entity<CalendarList>,
        subscription: SubscriptionId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let settings = Self {
            agenda,
            calendars,
            subscription,
            overrun: Self::field(Entry::Number, Self::set_overrun, window, cx),
            prep: Self::field(Entry::Duration, Self::set_prep, window, cx),
            cleanup: Self::field(Entry::Duration, Self::set_cleanup, window, cx),
            start_percent: Self::field(Entry::Number, Self::set_start_percent, window, cx),
            end_percent: Self::field(Entry::Number, Self::set_end_percent, window, cx),
            shortest: Self::field(Entry::Duration, Self::set_shortest, window, cx),
            longest: Self::field(Entry::Duration, Self::set_longest, window, cx),
            priority: cx.new(|cx| SelectState::new(window, cx)),
            modes: std::array::from_fn(|_| cx.focus_handle().tab_stop(true)),
            sync: cx.focus_handle().tab_stop(true),
            remove: cx.focus_handle().tab_stop(true),
            reset: cx.focus_handle().tab_stop(true),
        };

        settings.reseed(cx);

        settings
    }

    pub fn subscription(&self) -> SubscriptionId {
        self.subscription
    }

    fn rescale(&mut self, scaled: bool, cx: &mut Context<Self>) {
        let subscription = self.subscription;

        self.agenda.update(cx, |agenda, cx| {
            agenda.edit_defaults(
                subscription,
                |defaults| Self::rescaled(defaults, scaled),
                cx,
            )
        });
        self.reseed(cx);
    }

    fn rescaled(defaults: &mut CalendarDefaults, scaled: bool) {
        defaults.transitions = match (defaults.transitions, scaled) {
            (Transitions::Fixed { .. }, true) => Transitions::Scaled {
                start_percent: 10,
                end_percent: 10,
                shortest: 5,
                longest: 30,
            },
            (Transitions::Scaled { .. }, false) => Transitions::Fixed {
                prep: 0,
                cleanup: 0,
            },
            (transitions, _) => transitions,
        };
    }

    fn set_overrun(defaults: &mut CalendarDefaults, percent: i32) {
        defaults.overrun_percent = percent;
    }

    fn set_prep(defaults: &mut CalendarDefaults, minutes: i32) {
        if let Transitions::Fixed { prep, .. } = &mut defaults.transitions {
            *prep = minutes;
        }
    }

    fn set_cleanup(defaults: &mut CalendarDefaults, minutes: i32) {
        if let Transitions::Fixed { cleanup, .. } = &mut defaults.transitions {
            *cleanup = minutes;
        }
    }

    fn set_start_percent(defaults: &mut CalendarDefaults, percent: i32) {
        if let Transitions::Scaled { start_percent, .. } = &mut defaults.transitions {
            *start_percent = percent;
        }
    }

    fn set_end_percent(defaults: &mut CalendarDefaults, percent: i32) {
        if let Transitions::Scaled { end_percent, .. } = &mut defaults.transitions {
            *end_percent = percent;
        }
    }

    fn set_shortest(defaults: &mut CalendarDefaults, minutes: i32) {
        if let Transitions::Scaled { shortest, .. } = &mut defaults.transitions {
            *shortest = minutes;
        }
    }

    fn set_longest(defaults: &mut CalendarDefaults, minutes: i32) {
        if let Transitions::Scaled { longest, .. } = &mut defaults.transitions {
            *longest = minutes;
        }
    }

    fn defaults(&self, cx: &App) -> Option<CalendarDefaults> {
        self.agenda
            .read(cx)
            .subscriptions()
            .iter()
            .find(|subscription| subscription.id == self.subscription)
            .map(|subscription| subscription.defaults)
    }

    fn field(
        entry: Entry,
        apply: fn(&mut CalendarDefaults, i32),
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<InputState> {
        let state = cx.new(|cx| InputState::new(entry, window, cx));

        cx.subscribe(&state, move |settings, state, event: &InputEvent, cx| {
            let value = entry
                .parse(state.read(cx).content())
                .filter(|value| *value >= 0);
            let subscription = settings.subscription;

            if let Some(value) = value {
                settings.agenda.update(cx, |agenda, cx| {
                    agenda.edit_defaults(subscription, |defaults| apply(defaults, value), cx)
                });
            }

            state.update(cx, |state, cx| state.set_invalid(value.is_none(), cx));

            if matches!(event, InputEvent::Committed) && value.is_some() {
                settings.reseed(cx);
            }
        })
        .detach();

        state
    }

    fn back(&self, cx: &App) -> Div {
        let calendars = self.calendars.clone();

        div().flex().child(
            div()
                .id("back-to-calendars")
                .role(Role::Button)
                .aria_label("Calendars")
                .flex_none()
                .pt(px(2.0))
                .pb(px(8.0))
                .text_size(px(11.5))
                .text_color(cx.theme().muted_fg)
                .cursor_pointer()
                .hover(|style| style.text_color(cx.theme().link_fg))
                .on_click(move |_event, _window, cx| {
                    calendars.update(cx, |calendars, cx| calendars.hide_settings(cx));
                })
                .child(Text::new_inaccessible("\u{2039} Calendars".into())),
        )
    }

    fn labeled(label: &'static str, control: impl IntoElement, cx: &App) -> Div {
        div()
            .flex_1()
            .mt(px(10.0))
            .text_size(px(11.0))
            .text_color(cx.theme().muted_fg)
            .child(Text::new(label.into(), label.into()))
            .child(div().mt(px(3.0)).child(control))
    }

    fn priorities(&self, priority: Priority) -> Select {
        let agenda = self.agenda.clone();
        let subscription = self.subscription;
        let options = Priority::ALL
            .map(|priority| priority.name().into())
            .to_vec();

        Select::new(
            "default-priority",
            "Default priority",
            self.priority.clone(),
            options,
        )
        .selected(priority as usize)
        .on_select(move |index, _window, cx| {
            agenda.update(cx, |agenda, cx| {
                agenda.edit_defaults(
                    subscription,
                    |defaults| defaults.priority = Priority::ALL[index],
                    cx,
                )
            });
        })
    }

    fn transitions(&self, transitions: Transitions, settings: &Entity<Self>, cx: &App) -> Div {
        div()
            .child(Self::labeled(
                "Transition time",
                self.modes(matches!(transitions, Transitions::Scaled { .. }), settings),
                cx,
            ))
            .child(match transitions {
                Transitions::Fixed { .. } => self.fixed_fields(cx),
                Transitions::Scaled { .. } => self.scaled_fields(cx),
            })
    }

    fn fixed_fields(&self, cx: &App) -> Div {
        div()
            .flex()
            .gap(px(8.0))
            .child(Self::labeled(
                "Start (min)",
                Input::new(self.prep.clone()),
                cx,
            ))
            .child(Self::labeled(
                "End (min)",
                Input::new(self.cleanup.clone()),
                cx,
            ))
    }

    fn reset(&self, cx: &App) -> Option<Div> {
        let agenda = self.agenda.clone();
        let subscription = self.subscription;
        let edited = self.edited(cx);

        (edited > 0).then(|| {
            div()
                .mt(px(14.0))
                .pt(px(13.0))
                .border_t(px(1.0))
                .border_color(cx.theme().rule)
                .child(
                    div()
                        .mb(px(8.0))
                        .text_size(px(11.0))
                        .text_color(cx.theme().dim_fg)
                        .child(Text::new(
                            "reset-exceptions".into(),
                            format!("{edited} not using defaults").into(),
                        )),
                )
                .child(
                    Button::new("reset-defaults", "Reset events to defaults")
                        .stretch()
                        .focus(&self.reset)
                        .on_click(Box::new(move |_window, cx| {
                            agenda.update(cx, |agenda, cx| agenda.apply_defaults(subscription, cx));
                        })),
                )
        })
    }

    fn scaled_fields(&self, cx: &App) -> Div {
        div()
            .child(
                div()
                    .flex()
                    .gap(px(8.0))
                    .child(Self::labeled(
                        "Start (% of duration)",
                        Input::new(self.start_percent.clone()),
                        cx,
                    ))
                    .child(Self::labeled(
                        "End (% of duration)",
                        Input::new(self.end_percent.clone()),
                        cx,
                    )),
            )
            .child(
                div()
                    .flex()
                    .gap(px(8.0))
                    .child(Self::labeled(
                        "Min (min)",
                        Input::new(self.shortest.clone()),
                        cx,
                    ))
                    .child(Self::labeled(
                        "Max (min)",
                        Input::new(self.longest.clone()),
                        cx,
                    )),
            )
    }

    fn modes(&self, scaled: bool, settings: &Entity<Self>) -> Div {
        div()
            .flex()
            .gap(px(4.0))
            .mt(px(3.0))
            .child(self.mode("transition-fixed", "Fixed", false, scaled, settings))
            .child(self.mode("transition-scaled", "Relative", true, scaled, settings))
    }

    fn mode(
        &self,
        id: &'static str,
        label: &'static str,
        selects_scaled: bool,
        scaled: bool,
        settings: &Entity<Self>,
    ) -> Button {
        let settings = settings.clone();

        Button::new(id, label)
            .stretch()
            .focus(&self.modes[selects_scaled as usize])
            .when(selects_scaled == scaled, Button::filled)
            .on_click(Box::new(move |_window, cx| {
                settings.update(cx, |settings, cx| settings.rescale(selects_scaled, cx));
            }))
    }

    fn actions(&self, cx: &App) -> Div {
        let syncing = self.agenda.clone();
        let removing = self.agenda.clone();
        let calendars = self.calendars.clone();
        let subscription = self.subscription;

        div()
            .flex()
            .gap(px(6.0))
            .mt(px(10.0))
            .child(
                Button::new("sync-now", "Sync now")
                    .stretch()
                    .focus(&self.sync)
                    .on_click(Box::new(move |_window, cx| {
                        syncing.update(cx, |agenda, cx| agenda.sync(subscription, cx));
                    })),
            )
            .child(
                Button::new("remove-calendar", "Remove")
                    .stretch()
                    .focus(&self.remove)
                    .chip(false, cx.theme().danger_fg)
                    .on_click(Box::new(move |_window, cx| {
                        calendars.update(cx, |calendars, cx| calendars.hide_settings(cx));
                        removing.update(cx, |agenda, cx| agenda.unsubscribe(subscription, cx));
                    })),
            )
    }

    fn edited(&self, cx: &App) -> usize {
        self.agenda
            .read(cx)
            .tasks()
            .iter()
            .filter(|task| task.overridden)
            .filter_map(|task| task.source.as_ref())
            .filter(|source| source.subscription == self.subscription)
            .map(|source| source.uid.clone())
            .collect::<HashSet<_>>()
            .len()
    }

    fn title(&self, cx: &App) -> Option<Div> {
        let agenda = self.agenda.read(cx);
        let subscription = agenda
            .subscriptions()
            .iter()
            .find(|other| other.id == self.subscription)?;

        Some(
            div()
                .child(
                    div()
                        .truncate()
                        .text_size(px(13.5))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(Text::new(
                            "calendar-title".into(),
                            subscription.name.clone(),
                        )),
                )
                .child(
                    div()
                        .mt(px(3.0))
                        .text_size(px(10.5))
                        .text_color(cx.theme().dim_fg)
                        .child(Text::new(
                            "calendar-summary".into(),
                            self.calendars.read(cx).status(subscription, cx),
                        )),
                ),
        )
    }

    fn reseed(&self, cx: &mut Context<Self>) {
        for (state, value) in self.values(cx) {
            state.update(cx, |state, cx| state.set_content(value, cx));
        }
    }

    fn values(&self, cx: &App) -> Vec<(Entity<InputState>, String)> {
        let Some(defaults) = self.defaults(cx) else {
            return Vec::new();
        };
        let mut values = vec![(self.overrun.clone(), defaults.overrun_percent.to_string())];

        match defaults.transitions {
            Transitions::Fixed { prep, cleanup } => values.extend([
                (self.prep.clone(), prep.to_string()),
                (self.cleanup.clone(), cleanup.to_string()),
            ]),
            Transitions::Scaled {
                start_percent,
                end_percent,
                shortest,
                longest,
            } => values.extend([
                (self.start_percent.clone(), start_percent.to_string()),
                (self.end_percent.clone(), end_percent.to_string()),
                (self.shortest.clone(), shortest.to_string()),
                (self.longest.clone(), longest.to_string()),
            ]),
        }

        values
    }
}

impl Render for CalendarSettings {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let page = div()
            .id("calendar-settings")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .pt(px(10.0))
            .px(px(12.0))
            .pb(px(18.0))
            .child(self.back(cx));
        let Some(defaults) = self.defaults(cx) else {
            return page;
        };

        page.children(self.title(cx))
            .child(self.actions(cx))
            .child(Self::labeled(
                "Default priority",
                self.priorities(defaults.priority),
                cx,
            ))
            .child(Self::labeled(
                "Overrun allowance (%)",
                Input::new(self.overrun.clone()),
                cx,
            ))
            .child(self.transitions(defaults.transitions, &cx.entity(), cx))
            .children(self.reset(cx))
    }
}
