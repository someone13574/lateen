use chrono::Timelike;
use gpui::prelude::*;
use gpui::{App, Bounds, Corners, Entity, Pixels, Window, div, point, px, size};

use crate::agenda::Agenda;
use crate::block::BlockView;
use crate::button::ClickHandler;
use crate::clock::Clock;
use crate::grid::Grid;
use crate::session::Outcome;
use crate::task::TaskId;

pub struct DayColumns {
    days: usize,
    day_height: Pixels,
    corners: Corners<Pixels>,
    agenda: Entity<Agenda>,
}

impl DayColumns {
    pub fn new(
        days: usize,
        day_height: Pixels,
        agenda: Entity<Agenda>,
        cx: &mut Context<Self>,
    ) -> Self {
        cx.observe(&agenda, |_columns, _agenda, cx| cx.notify())
            .detach();

        Self {
            days,
            day_height,
            corners: Corners::default(),
            agenda,
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
        let schedule = self.agenda.read(cx).schedule();

        (0..self.days)
            .flat_map(|day| {
                let area = Bounds {
                    origin: point(Grid::COLUMN_WIDTH * day, px(0.0)),
                    size: size(Grid::COLUMN_WIDTH - Grid::GUIDE_WIDTH, self.day_height),
                };

                schedule.day(day as i32, area, now)
            })
            .enumerate()
            .map(|(index, block)| {
                let (task, start) = (block.task(), block.start());

                block
                    .index(index)
                    .on_click(self.open(task))
                    .on_done(self.settle(task, start, Outcome::Done))
                    .on_skip(self.settle(task, start, Outcome::Skipped))
            })
            .collect()
    }

    fn settle(&self, task: TaskId, start: i32, outcome: Outcome) -> Box<ClickHandler> {
        let agenda = self.agenda.clone();

        Box::new(move |_window, cx| {
            agenda.update(cx, |agenda, cx| agenda.confirm(task, start, outcome, cx));
        })
    }

    fn open(&self, task: TaskId) -> Box<ClickHandler> {
        let agenda = self.agenda.clone();

        Box::new(move |_window, cx| {
            agenda.update(cx, |agenda, cx| agenda.select(task, cx));
        })
    }
}

impl Render for DayColumns {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.agenda
            .clone()
            .update(cx, |agenda, cx| agenda.replan(cx));

        div()
            .relative()
            .size_full()
            .child(Grid::new(self.days, self.day_height, self.corners))
            .children(self.blocks(cx))
    }
}
