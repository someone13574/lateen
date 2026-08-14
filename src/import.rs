use gpui::prelude::*;
use gpui::{App, Div, Entity, FontWeight, Text, Window, div, px};

use crate::button::{Button, ClickHandler};
use crate::theme::ActiveTheme;

struct Source {
    name: &'static str,
    detail: &'static str,
}

#[derive(Default)]
pub struct Import {
    open: bool,
    connected: [bool; 3],
}

impl Import {
    const SOURCES: [Source; 3] = [
        Source {
            name: "Google Calendar",
            detail: "timetable@university.edu",
        },
        Source {
            name: "CalDAV server",
            detail: "caldav.university.edu",
        },
        Source {
            name: "ICS file",
            detail: "Read a .ics from disk",
        },
    ];

    pub fn toggle(&mut self, cx: &mut Context<Self>) {
        self.open = !self.open;
        cx.notify();
    }

    pub fn close(&mut self, cx: &mut Context<Self>) {
        self.open = false;
        cx.notify();
    }

    fn row(&self, index: usize, source: &Source, import: &Entity<Self>, cx: &App) -> Div {
        let connected = self.connected[index];
        let label = if connected { "Added" } else { "Add" };

        div()
            .flex()
            .items_center()
            .gap(px(8.0))
            .py(px(6.0))
            .border_t(px(1.0))
            .border_color(cx.theme().rule)
            .child(Self::describe(index, source, cx))
            .child(
                Button::new(("source-action", index), label)
                    .small()
                    .padding(px(9.0), px(3.0))
                    .chip(connected, cx.theme().import_action_fg)
                    .on_click(Self::connect(import, index)),
            )
    }

    fn card(&self, import: &Entity<Self>, cx: &App) -> Div {
        div()
            .mb(px(12.0))
            .p(px(11.0))
            .rounded(px(7.0))
            .border(px(1.0))
            .border_color(cx.theme().button_border)
            .bg(cx.theme().card_bg)
            .child(
                div()
                    .mb(px(8.0))
                    .text_size(px(11.5))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(Text::new("import-heading".into(), "Import calendar".into())),
            )
            .children(
                Self::SOURCES
                    .iter()
                    .enumerate()
                    .map(|(index, source)| self.row(index, source, import, cx)),
            )
    }

    fn describe(index: usize, source: &Source, cx: &App) -> Div {
        div()
            .flex_1()
            .min_w_0()
            .child(
                div()
                    .text_size(px(11.5))
                    .child(Text::new(("source-name", index).into(), source.name.into())),
            )
            .child(
                div()
                    .text_size(px(10.5))
                    .text_color(cx.theme().faint_fg)
                    .child(Text::new(
                        ("source-detail", index).into(),
                        source.detail.into(),
                    )),
            )
    }

    fn connect(import: &Entity<Self>, index: usize) -> Box<ClickHandler> {
        let import = import.clone();

        Box::new(move |_window, cx| {
            import.update(cx, |import, cx| {
                import.connected[index] = !import.connected[index];
                cx.notify();
            });
        })
    }
}

impl Render for Import {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let import = cx.entity();

        div().when(self.open, |panel| panel.child(self.card(&import, cx)))
    }
}
