use chrono::Timelike;
use gpui::prelude::*;
use gpui::{App, Bounds, Corners, Pixels, Window, div, point, px, size};

use crate::block::BlockView;
use crate::clock::Clock;
use crate::grid::Grid;
use crate::schedule::Schedule;

pub struct DayColumns {
    days: usize,
    day_height: Pixels,
    corners: Corners<Pixels>,
    schedule: Schedule,
}

impl DayColumns {
    pub fn new(days: usize, day_height: Pixels) -> Self {
        Self {
            days,
            day_height,
            corners: Corners::default(),
            schedule: Schedule::sample(days as i32),
        }
    }

    pub fn set_day_height(&mut self, day_height: Pixels) {
        self.day_height = day_height;
    }

    pub fn set_corners(&mut self, corners: Corners<Pixels>) {
        self.corners = corners;
    }

    fn blocks(&self, cx: &App) -> Vec<BlockView> {
        let now = cx.global::<Clock>().now().num_seconds_from_midnight() as i32 / 60;

        (0..self.days)
            .flat_map(|day| {
                let area = Bounds {
                    origin: point(Grid::COLUMN_WIDTH * day, px(0.0)),
                    size: size(Grid::COLUMN_WIDTH - Grid::GUIDE_WIDTH, self.day_height),
                };

                self.schedule.day(day as i32, area, now)
            })
            .collect()
    }
}

impl Render for DayColumns {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .relative()
            .size_full()
            .child(Grid::new(self.days, self.day_height, self.corners))
            .children(self.blocks(cx))
    }
}
