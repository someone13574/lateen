use std::collections::HashSet;

use chrono::{DateTime, Local, Timelike};
use gpui::prelude::*;
use gpui::{
    App, ClickEvent, Div, Entity, Focusable, FontWeight, Role, SharedString, Stateful, Text,
    Window, div, px,
};

use crate::agenda::Agenda;
use crate::button::Button;
use crate::clock::{Clock, ClockFormat};
use crate::input::{Entry, Input, InputEvent, InputState};
use crate::selectable_text::SelectableText;
use crate::subscription::{Subscription, SubscriptionId};
use crate::theme::{ActiveTheme, Theme};
use crate::tooltip::{Tooltip, TooltipBuilder, Tooltipped};

pub struct CalendarList {
    open: bool,
    settings: Option<SubscriptionId>,
    agenda: Entity<Agenda>,
    url: Entity<InputState>,
}

impl CalendarList {
    pub fn new(agenda: Entity<Agenda>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let url = cx.new(|cx| InputState::new(Entry::Text, window, cx));

        cx.subscribe_in(&url, window, |list, url, event, window, cx| {
            if matches!(event, InputEvent::Committed) && url.focus_handle(cx).is_focused(window) {
                list.add(cx);
            }
        })
        .detach();

        Self {
            open: false,
            settings: None,
            agenda,
            url,
        }
    }

    pub fn settings(&self) -> Option<SubscriptionId> {
        self.settings
    }

    pub fn show_settings(&mut self, id: SubscriptionId, cx: &mut Context<Self>) {
        self.open = true;
        self.settings = Some(id);
        cx.notify();
    }

    pub fn hide_settings(&mut self, cx: &mut Context<Self>) {
        self.settings = None;
        cx.notify();
    }

    pub fn toggle(&mut self, cx: &mut Context<Self>) {
        self.open = self.settings.take().is_some() || !self.open;
        cx.notify();
    }

    pub fn close(&mut self, cx: &mut Context<Self>) {
        self.open = false;
        self.settings = None;
        cx.notify();
    }

    const ROW_GROUP: &'static str = "calendar-row";

    fn card(&self, list: &Entity<Self>, cx: &App) -> Div {
        let subscriptions = self.agenda.read(cx).subscriptions();

        div()
            .mb(px(12.0))
            .p(px(11.0))
            .rounded(px(7.0))
            .border(px(1.0))
            .border_color(cx.theme().button_border)
            .bg(cx.theme().card_bg)
            .child(Self::header(list, cx))
            .child(
                div().flex().flex_col().gap(px(4.0)).children(
                    subscriptions
                        .iter()
                        .enumerate()
                        .map(|(index, subscription)| self.row(index, subscription, list, cx)),
                ),
            )
            .child(self.add_row(list))
    }

    fn row(
        &self,
        index: usize,
        subscription: &Subscription,
        list: &Entity<Self>,
        cx: &App,
    ) -> Stateful<Div> {
        let theme = *cx.theme();

        div()
            .id(("calendar", index))
            .role(Role::Button)
            .aria_label(subscription.name.clone())
            .group(Self::ROW_GROUP)
            .flex()
            .items_center()
            .gap(px(9.0))
            .px(px(9.0))
            .py(px(8.0))
            .rounded(px(6.0))
            .border(px(1.0))
            .border_color(theme.rule)
            .bg(theme.card_bg)
            .cursor_pointer()
            .hover(|style| style.border_color(theme.chip_border).bg(theme.row_hover_bg))
            .on_click(Self::opener(list, subscription.id))
            .child(self.describe(index, subscription, cx))
            .child(Self::chevron(theme))
    }

    fn chevron(theme: Theme) -> Div {
        div()
            .flex_none()
            .text_size(px(13.0))
            .text_color(theme.faint_fg)
            .group_hover(Self::ROW_GROUP, |style| style.text_color(theme.link_fg))
            .child(Text::new_inaccessible("\u{203a}".into()))
    }

    fn opener(
        list: &Entity<Self>,
        id: SubscriptionId,
    ) -> impl Fn(&ClickEvent, &mut Window, &mut App) + 'static {
        let list = list.clone();

        move |_event, _window, cx| {
            list.update(cx, |list, cx| list.show_settings(id, cx));
        }
    }

    fn describe(&self, index: usize, subscription: &Subscription, cx: &App) -> Div {
        let failed = subscription.failure.is_some();

        div()
            .flex_1()
            .min_w_0()
            .child(
                div()
                    .text_size(px(11.5))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(Text::new(
                        ("calendar-name", index).into(),
                        subscription.name.clone(),
                    )),
            )
            .child(Tooltipped::new(
                ("calendar-address", index),
                div()
                    .mt(px(2.0))
                    .truncate()
                    .text_size(px(10.5))
                    .text_color(cx.theme().dim_fg)
                    .child(Text::new(
                        ("calendar-url", index).into(),
                        subscription.short_address(),
                    )),
                Self::full_address(subscription.url.clone()),
            ))
            .child(
                div()
                    .mt(px(3.0))
                    .text_size(px(10.5))
                    .text_color(match failed {
                        true => cx.theme().danger_fg,
                        false => cx.theme().faint_fg,
                    })
                    .child(Text::new(
                        ("calendar-status", index).into(),
                        self.status(subscription, cx),
                    )),
            )
    }

    fn full_address(url: SharedString) -> Box<TooltipBuilder> {
        Tooltip::element(move |_window, _cx| {
            SelectableText::new("calendar-address-tooltip", url.clone()).into_any_element()
        })
    }

    pub fn status(&self, subscription: &Subscription, cx: &App) -> SharedString {
        let Some(synced) = subscription.synced.filter(|_| !subscription.syncing) else {
            return "Syncing".into();
        };

        if let Some(failure) = &subscription.failure {
            return failure.clone();
        }

        let events = self.events(subscription.id, cx);
        let plural = if events == 1 { "" } else { "s" };
        let unsupported = match subscription.dropped {
            0 => String::new(),
            dropped => format!(", {dropped} unsupported"),
        };

        format!(
            "{events} event{plural}{unsupported}, synced {}",
            Self::when(synced, cx)
        )
        .into()
    }

    fn events(&self, id: SubscriptionId, cx: &App) -> usize {
        self.agenda
            .read(cx)
            .tasks()
            .iter()
            .filter_map(|task| task.source.as_ref())
            .filter(|source| source.subscription == id)
            .map(|source| source.uid.clone())
            .collect::<HashSet<_>>()
            .len()
    }

    fn when(synced: DateTime<Local>, cx: &App) -> String {
        let now = cx.global::<Clock>().now();

        if (now - synced).num_seconds() < 60 {
            return "just now".to_string();
        }

        let time = cx
            .global::<ClockFormat>()
            .time_label(synced.num_seconds_from_midnight() as i32 / 60);

        match synced.date_naive() == now.date_naive() {
            true => time,
            false => format!("{} {time}", synced.format("%-d %b")),
        }
    }

    fn header(list: &Entity<Self>, cx: &App) -> Div {
        let list = list.clone();

        div()
            .flex()
            .items_center()
            .mb(px(9.0))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_size(px(11.5))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(Text::new("calendars-heading".into(), "Calendars".into())),
            )
            .child(
                div()
                    .id("close-calendars")
                    .role(Role::Button)
                    .aria_label("Close")
                    .flex_none()
                    .text_size(px(11.0))
                    .text_color(cx.theme().dim_fg)
                    .cursor_pointer()
                    .hover(|style| style.text_color(cx.theme().link_fg))
                    .on_click(move |_event, _window, cx| {
                        list.update(cx, |list, cx| list.close(cx));
                    })
                    .child(Text::new_inaccessible("Close".into())),
            )
    }

    fn add(&mut self, cx: &mut Context<Self>) {
        let url = self.url.read(cx).content().clone();

        if url.trim().is_empty() {
            return;
        }

        self.agenda
            .update(cx, |agenda, cx| agenda.subscribe(&url, cx));
        self.url.update(cx, |url, cx| url.set_content("", cx));
        cx.notify();
    }

    fn add_row(&self, list: &Entity<Self>) -> Div {
        let list = list.clone();

        div()
            .flex()
            .gap(px(6.0))
            .mt(px(11.0))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .child(Input::new(self.url.clone()).placeholder("Calendar address")),
            )
            .child(
                Button::new("add-calendar", "Add").on_click(Box::new(move |_window, cx| {
                    list.update(cx, |list, cx| list.add(cx));
                })),
            )
    }
}

impl Render for CalendarList {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let list = cx.entity();

        div().when(self.open, |panel| panel.child(self.card(&list, cx)))
    }
}
