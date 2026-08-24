use chrono::{DateTime, Local, Timelike};
use gpui::prelude::*;
use gpui::{App, Div, Entity, FontWeight, Role, SharedString, Text, Window, div, px};

use crate::button::{Button, ClickHandler};
use crate::clock::{Clock, ClockFormat};
use crate::select::{Select, SelectState};
use crate::theme::{ActiveTheme, BlockColor};

struct RemoteCalendar {
    name: &'static str,
    detail: &'static str,
    color: BlockColor,
    events: i32,
}

struct Subscription {
    calendar: usize,
    synced: DateTime<Local>,
}

pub struct Import {
    open: bool,
    subscriptions: Vec<Subscription>,
    choice: Entity<SelectState>,
    chosen: usize,
}

impl Import {
    const CALENDARS: [RemoteCalendar; 3] = [
        RemoteCalendar {
            name: "University calendar",
            detail: "timetable.ics, subscribed",
            color: BlockColor::Blue,
            events: 2,
        },
        RemoteCalendar {
            name: "Work calendar",
            detail: "caldav.example.org",
            color: BlockColor::Green,
            events: 1,
        },
        RemoteCalendar {
            name: "Google Calendar",
            detail: "calendar.google.com",
            color: BlockColor::Amber,
            events: 3,
        },
    ];

    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            open: false,
            subscriptions: Vec::new(),
            choice: cx.new(|cx| SelectState::new(window, cx)),
            chosen: 0,
        }
    }

    pub fn toggle(&mut self, cx: &mut Context<Self>) {
        self.open = !self.open;
        cx.notify();
    }

    pub fn open(&mut self, cx: &mut Context<Self>) {
        self.open = true;
        cx.notify();
    }

    pub fn close(&mut self, cx: &mut Context<Self>) {
        self.open = false;
        cx.notify();
    }

    fn available(&self) -> Vec<usize> {
        (0..Self::CALENDARS.len())
            .filter(|calendar| {
                !self
                    .subscriptions
                    .iter()
                    .any(|subscription| subscription.calendar == *calendar)
            })
            .collect()
    }

    fn row(
        &self,
        index: usize,
        subscription: &Subscription,
        import: &Entity<Self>,
        cx: &App,
    ) -> Div {
        let calendar = &Self::CALENDARS[subscription.calendar];

        div()
            .flex()
            .items_center()
            .gap(px(9.0))
            .px(px(9.0))
            .py(px(8.0))
            .rounded(px(6.0))
            .border(px(1.0))
            .border_color(cx.theme().rule)
            .bg(cx.theme().card_bg)
            .child(
                div()
                    .flex_none()
                    .self_start()
                    .mt(px(3.0))
                    .size(px(8.0))
                    .rounded(px(2.0))
                    .bg(cx.theme().swatch(calendar.color)),
            )
            .child(Self::describe(index, subscription, calendar, cx))
            .child(Self::actions(index, import, cx))
    }

    fn sync(import: &Entity<Self>, index: usize) -> Box<ClickHandler> {
        let import = import.clone();

        Box::new(move |_window, cx| {
            let now = cx.global::<Clock>().now();

            import.update(cx, |import, cx| {
                if let Some(subscription) = import.subscriptions.get_mut(index) {
                    subscription.synced = now;
                }

                cx.notify();
            });
        })
    }

    fn remove(import: &Entity<Self>, index: usize) -> Box<ClickHandler> {
        let import = import.clone();

        Box::new(move |_window, cx| {
            import.update(cx, |import, cx| {
                import.subscriptions.remove(index);
                import.chosen = 0;
                cx.notify();
            });
        })
    }

    fn add(import: &Entity<Self>) -> Box<ClickHandler> {
        let import = import.clone();

        Box::new(move |_window, cx| {
            let now = cx.global::<Clock>().now();

            import.update(cx, |import, cx| {
                let Some(calendar) = import.available().get(import.chosen).copied() else {
                    return;
                };

                import.subscriptions.push(Subscription {
                    calendar,
                    synced: now,
                });
                import.chosen = 0;
                cx.notify();
            });
        })
    }

    fn synced_label(subscription: &Subscription, calendar: &RemoteCalendar, cx: &App) -> String {
        let plural = if calendar.events == 1 { "" } else { "s" };
        let elapsed = cx.global::<Clock>().now() - subscription.synced;
        let when = match elapsed.num_seconds() < 60 {
            true => "just now".to_string(),
            false => cx
                .global::<ClockFormat>()
                .time_label(subscription.synced.num_seconds_from_midnight() as i32 / 60),
        };

        format!("{} event{plural}, synced {when}", calendar.events)
    }

    fn add_row(&self, available: &[usize], import: &Entity<Self>) -> Div {
        let options: Vec<SharedString> = available
            .iter()
            .map(|calendar| Self::CALENDARS[*calendar].name.into())
            .collect();
        let chosen = self.chosen.min(options.len().saturating_sub(1));
        let choosing = import.clone();

        div()
            .flex()
            .gap(px(6.0))
            .mt(px(11.0))
            .child(
                div().flex_1().min_w_0().child(
                    Select::new("calendar", "Calendar", self.choice.clone(), options)
                        .selected(chosen)
                        .on_select(move |index, _window, cx| {
                            choosing.update(cx, |import, cx| {
                                import.chosen = index;
                                cx.notify();
                            });
                        }),
                ),
            )
            .child(Button::new("add-calendar", "Add").on_click(Self::add(import)))
    }

    fn header(import: &Entity<Self>, cx: &App) -> Div {
        let import = import.clone();

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
                    .child(Text::new("import-heading".into(), "Calendars".into())),
            )
            .child(
                div()
                    .id("close-import")
                    .role(Role::Button)
                    .aria_label("Close")
                    .flex_none()
                    .text_size(px(11.0))
                    .text_color(cx.theme().dim_fg)
                    .cursor_pointer()
                    .hover(|style| style.text_color(cx.theme().link_fg))
                    .on_click(move |_event, _window, cx| {
                        import.update(cx, |import, cx| import.close(cx));
                    })
                    .child(Text::new_inaccessible("Close".into())),
            )
    }

    fn empty(cx: &App) -> Div {
        div()
            .mb(px(11.0))
            .text_size(px(11.0))
            .text_color(cx.theme().dim_fg)
            .child(Text::new(
                "import-empty".into(),
                "Nothing connected yet.".into(),
            ))
    }

    fn card(&self, import: &Entity<Self>, cx: &App) -> Div {
        let available = self.available();

        div()
            .mb(px(12.0))
            .p(px(11.0))
            .rounded(px(7.0))
            .border(px(1.0))
            .border_color(cx.theme().button_border)
            .bg(cx.theme().card_bg)
            .child(Self::header(import, cx))
            .children(self.subscriptions.is_empty().then(|| Self::empty(cx)))
            .child(
                div().flex().flex_col().gap(px(4.0)).children(
                    self.subscriptions
                        .iter()
                        .enumerate()
                        .map(|(index, subscription)| self.row(index, subscription, import, cx)),
                ),
            )
            .children((!available.is_empty()).then(|| self.add_row(&available, import)))
    }

    fn describe(
        index: usize,
        subscription: &Subscription,
        calendar: &RemoteCalendar,
        cx: &App,
    ) -> Div {
        div()
            .flex_1()
            .min_w_0()
            .child(
                div()
                    .text_size(px(11.5))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(Text::new(
                        ("calendar-name", index).into(),
                        calendar.name.into(),
                    )),
            )
            .child(
                div()
                    .mt(px(2.0))
                    .text_size(px(10.5))
                    .text_color(cx.theme().dim_fg)
                    .child(Text::new(
                        ("calendar-detail", index).into(),
                        calendar.detail.into(),
                    )),
            )
            .child(
                div()
                    .mt(px(3.0))
                    .text_size(px(10.5))
                    .text_color(cx.theme().faint_fg)
                    .child(Text::new(
                        ("calendar-synced", index).into(),
                        Self::synced_label(subscription, calendar, cx).into(),
                    )),
            )
    }

    fn actions(index: usize, import: &Entity<Self>, cx: &App) -> Div {
        div()
            .flex()
            .flex_col()
            .flex_none()
            .gap(px(3.0))
            .child(
                Button::new(("sync-calendar", index), "Sync")
                    .small()
                    .centered()
                    .padding(px(8.0), px(3.0))
                    .chip(false, cx.theme().muted_fg)
                    .on_click(Self::sync(import, index)),
            )
            .child(
                Button::new(("remove-calendar", index), "Remove")
                    .small()
                    .centered()
                    .padding(px(8.0), px(3.0))
                    .chip(false, cx.theme().danger_fg)
                    .on_click(Self::remove(import, index)),
            )
    }
}

impl Render for Import {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let import = cx.entity();

        div().when(self.open, |panel| panel.child(self.card(&import, cx)))
    }
}
