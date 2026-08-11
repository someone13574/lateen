use std::time::Duration;

use chrono::{Local, Timelike};
use gpui::prelude::*;
use gpui::{App, Decorations, Div, Pixels, Text, Tiling, Window, div, px};

use crate::button::Button;
use crate::clock::{Clock, ClockFormat};
use crate::theme::ActiveTheme;

pub struct BottomBar;

impl BottomBar {
    const HEIGHT: Pixels = px(30.0);
    const CORNER_RADIUS: Pixels = px(8.0);

    pub fn new(cx: &mut Context<Self>) -> Self {
        Self::follow_seconds(cx);

        Self
    }

    fn follow_seconds(cx: &mut Context<Self>) {
        cx.spawn(async move |bar, cx| {
            loop {
                let nanoseconds = Local::now().nanosecond() % 1_000_000_000;
                let until_next_second = Duration::from_nanos((1_000_000_000 - nanoseconds).into());
                cx.background_executor().timer(until_next_second).await;

                if bar.update(cx, |_bar, cx| cx.notify()).is_err() {
                    break;
                }
            }
        })
        .detach();
    }

    fn time(cx: &App) -> Div {
        let time = cx.global::<Clock>().now().time();

        div()
            .flex_none()
            .text_color(cx.theme().bottom_bar_time_fg)
            .child(Text::new(
                "clock".into(),
                cx.global::<ClockFormat>().second_label(time).into(),
            ))
    }

    fn controls() -> Div {
        div()
            .flex()
            .flex_none()
            .items_center()
            .gap(px(5.0))
            .child(
                Button::new("rewind", "−15m")
                    .small()
                    .on_click(Box::new(|_window, cx| Clock::shift(-15, cx))),
            )
            .child(
                Button::new("advance", "+15m")
                    .small()
                    .on_click(Box::new(|_window, cx| Clock::shift(15, cx))),
            )
            .child(
                Button::new("live", "Live")
                    .small()
                    .on_click(Box::new(|_window, cx| Clock::reset(cx))),
            )
    }
}

impl Render for BottomBar {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tiling = match window.window_decorations() {
            Decorations::Client { tiling } => tiling,
            Decorations::Server => Tiling::tiled(),
        };

        div()
            .flex()
            .flex_none()
            .items_center()
            .h(Self::HEIGHT)
            .gap(px(12.0))
            .px(px(12.0))
            .overflow_hidden()
            .bg(cx.theme().bottom_bar_bg)
            .border_t(px(1.0))
            .border_color(cx.theme().bottom_bar_border)
            .text_size(px(11.0))
            .text_color(cx.theme().bottom_bar_fg)
            .when(!tiling.bottom && !tiling.left, |bar| {
                bar.rounded_bl(Self::CORNER_RADIUS)
            })
            .when(!tiling.bottom && !tiling.right, |bar| {
                bar.rounded_br(Self::CORNER_RADIUS)
            })
            .child(Self::time(cx))
            .child(div().flex_1())
            .child(Self::controls())
    }
}
