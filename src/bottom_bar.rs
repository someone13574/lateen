use std::env;
use std::time::{Duration, Instant};

use chrono::{Local, Timelike};
use gpui::prelude::*;
use gpui::{App, Decorations, Div, Pixels, Task, Text, Tiling, Window, div, px};

use crate::button::{Button, ClickHandler};
use crate::clock::{Clock, ClockFormat};
use crate::theme::ActiveTheme;

pub struct BottomBar {
    travel: Option<Task<()>>,
}

impl BottomBar {
    const HEIGHT: Pixels = px(30.0);
    const CORNER_RADIUS: Pixels = px(8.0);
    const TRAVEL_TICK: Duration = Duration::from_millis(16);
    const TRAVEL_RATE: f32 = 15.0;
    const TRAVEL_ACCEL: f32 = 6.0;
    const TRAVEL_LIMIT: f32 = 720.0;
    const TIME_TRAVEL: &str = "LATEEN_TIME_TRAVEL";

    pub fn enabled() -> bool {
        env::var_os(Self::TIME_TRAVEL).is_some()
    }

    pub fn new(cx: &mut Context<Self>) -> Self {
        Self::follow_seconds(cx);

        Self { travel: None }
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

    fn hold(direction: f32, cx: &Context<Self>) -> Box<ClickHandler> {
        let bar = cx.entity();

        Box::new(move |_window, cx| bar.update(cx, |bar, cx| bar.travel(direction, cx)))
    }

    fn rest(cx: &Context<Self>) -> Box<ClickHandler> {
        let bar = cx.entity();

        Box::new(move |_window, cx| bar.update(cx, |bar, _cx| bar.travel = None))
    }

    fn travel(&mut self, direction: f32, cx: &mut Context<Self>) {
        self.travel = Some(cx.spawn(async move |bar, cx| {
            let pressed = Instant::now();
            let mut previous = Duration::ZERO;

            loop {
                cx.background_executor().timer(Self::TRAVEL_TICK).await;

                let held = pressed.elapsed();
                let tick = (held - previous).as_secs_f32();
                let step = direction * Self::rate(held) * tick;
                previous = held;

                if bar.update(cx, |_bar, cx| Clock::travel(step, cx)).is_err() {
                    break;
                }
            }
        }));
    }

    fn rate(held: Duration) -> f32 {
        let accelerated = Self::TRAVEL_RATE * Self::TRAVEL_ACCEL.powf(held.as_secs_f32());

        accelerated.min(Self::TRAVEL_LIMIT)
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

    fn controls(cx: &Context<Self>) -> Div {
        div()
            .flex()
            .flex_none()
            .items_center()
            .gap(px(5.0))
            .child(
                Button::new("rewind", "Rewind")
                    .small()
                    .on_press(Self::hold(-1.0, cx))
                    .on_release(Self::rest(cx)),
            )
            .child(
                Button::new("advance", "Forward")
                    .small()
                    .on_press(Self::hold(1.0, cx))
                    .on_release(Self::rest(cx)),
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
            .child(Self::controls(cx))
    }
}
