use chrono::{Duration, Local, NaiveDate};
use gpui::prelude::*;
use gpui::{
    App, Axis, Corners, Decorations, Div, FontWeight, PinchEvent, Pixels, ScrollHandle,
    ScrollWheelEvent, Stateful, Text, Tiling, Window, div, point, px,
};

use crate::clock::ClockFormat;
use crate::grid::Grid;
use crate::scrollbar::Scrollbar;
use crate::theme::ActiveTheme;

pub struct Calendar {
    horizontal: ScrollHandle,
    vertical: ScrollHandle,
    day_height: Pixels,
}

impl Calendar {
    const GUTTER_WIDTH: Pixels = px(53.0);
    const HEADER_HEIGHT: Pixels = px(47.0);
    const MIN_LABEL_SPACING: Pixels = px(66.0);
    const CORNER_RADIUS: Pixels = px(8.0);
    const MIN_DAY_HEIGHT: Pixels = px(72.0);
    const MAX_DAY_HEIGHT: Pixels = px(5760.0);
    const DAYS: usize = 14;
    const ZOOM_RATE: f32 = 0.002;

    pub fn new() -> Self {
        Self {
            horizontal: ScrollHandle::new(),
            vertical: ScrollHandle::new(),
            day_height: px(1440.0),
        }
    }

    fn day_height(&self) -> Pixels {
        self.day_height
            .clamp(Self::MIN_DAY_HEIGHT, Self::MAX_DAY_HEIGHT)
            .max(self.vertical.bounds().size.height)
    }

    fn zoom(&mut self, event: &ScrollWheelEvent, window: &mut Window, cx: &mut Context<Self>) {
        if !event.modifiers.control {
            return;
        }
        cx.stop_propagation();

        let delta = f32::from(event.delta.pixel_delta(window.line_height()).y);
        if delta != 0.0 {
            self.scale_day((delta * Self::ZOOM_RATE).exp(), event.position.y, cx);
        }
    }

    fn pinch(&mut self, event: &PinchEvent, _window: &mut Window, cx: &mut Context<Self>) {
        if event.delta != 0.0 {
            self.scale_day(1.0 + event.delta, event.position.y, cx);
        }
    }

    fn scale_day(&mut self, factor: f32, anchor: Pixels, cx: &mut Context<Self>) {
        let viewport = self.vertical.bounds();
        let anchor = (anchor - viewport.origin.y).clamp(px(0.0), viewport.size.height);
        let before = self.day_height();
        self.day_height = before * factor;

        let after = self.day_height();
        let scrolled = (anchor - self.vertical.offset().y) * (after / before) - anchor;
        self.vertical
            .set_offset(point(self.vertical.offset().x, -scrolled.max(px(0.0))));
        cx.notify();
    }

    fn header(&self, cx: &mut Context<Self>) -> Div {
        let today = Local::now().date_naive();

        div()
            .flex()
            .flex_none()
            .h(Self::HEADER_HEIGHT)
            .child(
                div()
                    .flex_none()
                    .w(Self::GUTTER_WIDTH)
                    .border_r(px(1.0))
                    .border_color(cx.theme().gutter_border)
                    .child(
                        div()
                            .size_full()
                            .bg(cx.theme().gutter_bg)
                            .border_b(px(1.0))
                            .border_color(cx.theme().column_header_border),
                    ),
            )
            .child(
                div()
                    .id("calendar-headers")
                    .flex_1()
                    .min_w_0()
                    .overflow_x_scroll()
                    .restrict_scroll_to_axis()
                    .track_scroll(&self.horizontal)
                    .child(
                        div()
                            .flex()
                            .flex_none()
                            .w(Grid::COLUMN_WIDTH * Self::DAYS)
                            .children(
                                (0..Self::DAYS).map(|day| Self::column_header(day, today, cx)),
                            ),
                    ),
            )
    }

    fn column_header(day: usize, today: NaiveDate, cx: &mut Context<Self>) -> Div {
        let theme = *cx.theme();
        let date = today + Duration::days(day as i64);
        let (bg, fg, sub_fg) = if day == 0 {
            (
                theme.today_header_bg,
                theme.today_header_fg,
                theme.today_header_sub_fg,
            )
        } else {
            (
                theme.column_header_bg,
                theme.column_header_fg,
                theme.column_header_sub_fg,
            )
        };

        div()
            .flex_none()
            .w(Grid::COLUMN_WIDTH)
            .h(Self::HEADER_HEIGHT)
            .border_r(px(1.0))
            .border_color(theme.grid_day_border)
            .child(
                div()
                    .size_full()
                    .flex()
                    .flex_col()
                    .justify_center()
                    .px(px(11.0))
                    .bg(bg)
                    .border_b(px(1.0))
                    .border_color(theme.column_header_border)
                    .child(
                        div()
                            .text_size(px(10.5))
                            .text_color(sub_fg)
                            .child(Text::new(
                                ("weekday", day).into(),
                                date.format("%a").to_string().into(),
                            )),
                    )
                    .child(
                        div()
                            .mt(px(1.0))
                            .text_size(px(14.0))
                            .font_weight(if day == 0 {
                                FontWeight::BOLD
                            } else {
                                FontWeight::SEMIBOLD
                            })
                            .text_color(fg)
                            .child(Text::new(
                                ("date", day).into(),
                                Self::date_label(day, date).into(),
                            )),
                    ),
            )
    }

    fn date_label(day: usize, date: NaiveDate) -> String {
        match day {
            0 => "Today".to_string(),
            1 => "Tomorrow".to_string(),
            _ => date.format("%-d %b").to_string(),
        }
    }

    fn hour_label(hour: usize, day_height: Pixels, cx: &App) -> Div {
        div()
            .absolute()
            .top(day_height * (hour as f32 / 24.0) - px(6.0))
            .right(px(7.0))
            .h(px(12.0))
            .child(Text::new(
                ("hour", hour).into(),
                cx.global::<ClockFormat>().hour_label(hour).into(),
            ))
    }

    fn gutter(&self, tiling: Tiling, cx: &mut Context<Self>) -> Stateful<Div> {
        let day_height = self.day_height();
        let hours_per_label = (1..=3)
            .find(|step| day_height * (*step as f32 / 24.0) >= Self::MIN_LABEL_SPACING)
            .unwrap_or(3);

        div()
            .id("calendar-times")
            .flex()
            .flex_col()
            .flex_none()
            .w(Self::GUTTER_WIDTH)
            .overflow_y_scroll()
            .restrict_scroll_to_axis()
            .track_scroll(&self.vertical)
            .bg(cx.theme().gutter_bg)
            .border_r(px(1.0))
            .border_color(cx.theme().gutter_border)
            .when(!tiling.bottom && !tiling.left, |gutter| {
                gutter.rounded_bl(Self::CORNER_RADIUS)
            })
            .child(
                div()
                    .relative()
                    .flex_none()
                    .h(day_height)
                    .text_size(px(10.0))
                    .line_height(px(12.0))
                    .text_color(cx.theme().gutter_fg)
                    .children(
                        (hours_per_label..24)
                            .step_by(hours_per_label)
                            .map(|hour| Self::hour_label(hour, day_height, cx)),
                    ),
            )
    }

    fn grid(&self, tiling: Tiling) -> Grid {
        let corners = Corners {
            bottom_right: if tiling.bottom || tiling.right {
                px(0.0)
            } else {
                Self::CORNER_RADIUS
            },
            ..Default::default()
        };

        Grid::new(Self::DAYS, self.day_height(), corners)
    }
}

impl Render for Calendar {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tiling = match window.window_decorations() {
            Decorations::Client { tiling } => tiling,
            Decorations::Server => Tiling::tiled(),
        };

        div()
            .relative()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .bg(cx.theme().calendar_bg)
            .when(!tiling.bottom && !tiling.left, |calendar| {
                calendar.rounded_bl(Self::CORNER_RADIUS)
            })
            .when(!tiling.bottom && !tiling.right, |calendar| {
                calendar.rounded_br(Self::CORNER_RADIUS)
            })
            .child(self.header(cx))
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .child(self.gutter(tiling, cx))
                    .child(
                        div()
                            .id("calendar-days")
                            .flex_1()
                            .min_w_0()
                            .overflow_x_scroll()
                            .restrict_scroll_to_axis()
                            .track_scroll(&self.horizontal)
                            .child(
                                div()
                                    .id("calendar-hours")
                                    .flex()
                                    .flex_col()
                                    .flex_none()
                                    .w(Grid::COLUMN_WIDTH * Self::DAYS)
                                    .h_full()
                                    .overflow_y_scroll()
                                    .restrict_scroll_to_axis()
                                    .track_scroll(&self.vertical)
                                    .child(self.grid(tiling)),
                            ),
                    ),
            )
            .child(
                div()
                    .absolute()
                    .inset_0()
                    .on_scroll_wheel(cx.listener(Self::zoom))
                    .on_pinch(cx.listener(Self::pinch)),
            )
            .child(
                div()
                    .absolute()
                    .top(Self::HEADER_HEIGHT)
                    .right_0()
                    .bottom_0()
                    .w(Scrollbar::THICKNESS)
                    .child(Scrollbar::new(
                        "calendar-vertical-scrollbar",
                        Axis::Vertical,
                        &self.vertical,
                    )),
            )
            .child(
                div()
                    .absolute()
                    .left(Self::GUTTER_WIDTH)
                    .right_0()
                    .bottom_0()
                    .h(Scrollbar::THICKNESS)
                    .child(
                        Scrollbar::new(
                            "calendar-horizontal-scrollbar",
                            Axis::Horizontal,
                            &self.horizontal,
                        )
                        .yield_corner(),
                    ),
            )
    }
}
