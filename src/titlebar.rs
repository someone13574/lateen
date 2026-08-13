use std::mem;

use gpui::prelude::*;
use gpui::{
    App, Decorations, Div, FontWeight, MouseButton, Role, SharedString, Tiling, Window, div, px,
    size, text,
};

use crate::button::{Button, ClickHandler};
use crate::theme::ActiveTheme;
use crate::window_control::WindowControl;

#[derive(IntoElement)]
pub struct Titlebar {
    title: SharedString,
    on_new: Box<ClickHandler>,
}

impl Titlebar {
    pub fn new(title: impl Into<SharedString>, on_new: Box<ClickHandler>) -> Self {
        Self {
            title: title.into(),
            on_new,
        }
    }

    fn actions(on_new: Box<ClickHandler>) -> Div {
        div()
            .flex()
            .flex_none()
            .items_center()
            .gap(px(4.0))
            .mr(px(6.0))
            .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                cx.stop_propagation()
            })
            .on_mouse_move(|_event, _window, cx| cx.stop_propagation())
            .child(Button::new("new", "New").on_click(on_new))
            .child(Button::new("import-calendar", "Import calendar"))
    }

    fn controls(window: &Window) -> Div {
        let controls = window.window_controls();

        div()
            .flex()
            .flex_none()
            .gap(px(2.0))
            .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                cx.stop_propagation()
            })
            .on_mouse_move(|_event, _window, cx| cx.stop_propagation())
            .when(controls.minimize, |controls| {
                controls.child(WindowControl::new(
                    "minimize",
                    "icons/minimize.svg",
                    size(px(6.5), px(1.05)),
                    "Minimize",
                    Box::new(|window, _cx| window.minimize_window()),
                ))
            })
            .when(controls.maximize, |controls| {
                controls.child(WindowControl::new(
                    "maximize",
                    "icons/maximize.svg",
                    size(px(8.0), px(8.0)),
                    "Maximize",
                    Box::new(|window, _cx| window.zoom_window()),
                ))
            })
            .child(
                WindowControl::new(
                    "close",
                    "icons/close.svg",
                    size(px(7.0), px(7.0)),
                    "Close",
                    Box::new(|window, _cx| window.remove_window()),
                )
                .danger(),
            )
    }
}

impl RenderOnce for Titlebar {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let Self { title, on_new } = self;
        let decorations = window.window_decorations();
        let client = matches!(decorations, Decorations::Client { .. });
        let tiling = match decorations {
            Decorations::Client { tiling } => tiling,
            Decorations::Server => Tiling::tiled(),
        };
        let should_move = window.use_state(cx, |_window, _cx| false);

        div()
            .id("titlebar")
            .role(Role::TitleBar)
            .flex()
            .flex_none()
            .items_center()
            .h(px(40.0))
            .gap(px(12.0))
            .pl(px(14.0))
            .pr(px(8.0))
            .bg(cx.theme().titlebar_bg)
            .border_b(px(1.0))
            .border_color(cx.theme().titlebar_border)
            .when(!tiling.top && !tiling.left, |titlebar| {
                titlebar.rounded_tl(px(8.0))
            })
            .when(!tiling.top && !tiling.right, |titlebar| {
                titlebar.rounded_tr(px(8.0))
            })
            .when(client, |titlebar| {
                titlebar
                    .on_mouse_down(MouseButton::Left, {
                        let should_move = should_move.clone();
                        move |_event, _window, cx| {
                            should_move.update(cx, |should_move, _cx| *should_move = true);
                        }
                    })
                    .on_mouse_up(MouseButton::Left, {
                        let should_move = should_move.clone();
                        move |_event, _window, cx| {
                            should_move.update(cx, |should_move, _cx| *should_move = false);
                        }
                    })
                    .on_mouse_move(move |_event, window, cx| {
                        if should_move.update(cx, |should_move, _cx| mem::take(should_move)) {
                            window.start_window_move();
                        }
                    })
                    .when(window.window_controls().window_menu, |titlebar| {
                        titlebar.on_mouse_down(MouseButton::Right, |event, window, _cx| {
                            window.show_window_menu(event.position);
                        })
                    })
                    .when(window.window_controls().maximize, |titlebar| {
                        titlebar.on_click(|event, window, _cx| {
                            if event.click_count() == 2 {
                                window.zoom_window();
                            }
                        })
                    })
            })
            .when(client, |titlebar| {
                titlebar.child(
                    div()
                        .flex_none()
                        .text_size(px(13.0))
                        .font_weight(FontWeight::SEMIBOLD)
                        .letter_spacing(px(0.13))
                        .child(text!(title)),
                )
            })
            .child(div().flex_1())
            .child(Self::actions(on_new))
            .when(client, |titlebar| titlebar.child(Self::controls(window)))
    }
}
