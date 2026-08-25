use std::ops::Range;

use gpui::prelude::*;
use gpui::{App, Bounds, Corners, Entity, Pixels, Window, div, point, px, size};

use crate::agenda::Agenda;
use crate::block::BlockView;
use crate::button::ClickHandler;
use crate::clock::Clock;
use crate::grid::Grid;
use crate::session::Outcome;
use crate::task::TaskId;
use crate::task_details::TaskDetails;
use crate::tooltip::TooltipBuilder;

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
        let now = cx.global::<Clock>().minute_of_day();
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
                let occurrence = block.occurrence();

                block
                    .index(index)
                    .details(self.details(task, occurrence))
                    .on_click(self.open(task))
                    .on_done(self.settle(task, start, Outcome::Done))
                    .on_skip(self.settle(task, start, Outcome::Skipped))
            })
            .collect()
    }

    fn details(&self, task: TaskId, occurrence: Range<i32>) -> Box<TooltipBuilder> {
        TaskDetails::new(self.agenda.clone(), task).occurrence(occurrence)
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
            .id("day-columns")
            .relative()
            .size_full()
            .on_click(cx.listener(|columns, _event, _window, cx| {
                columns.agenda.update(cx, Agenda::deselect);
            }))
            .child(Grid::new(self.days, self.day_height, self.corners))
            .children(self.blocks(cx))
    }
}
