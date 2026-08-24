use std::ops::Range;

use chrono::{DateTime, Local, Timelike};
use gpui::{Bounds, Pixels, point, px, size};

use crate::block::{Block, BlockView};
use crate::colorer::Colorer;
use crate::planner::Planner;
use crate::session::Session;
use crate::task::{Task, TaskKind};

pub struct Schedule {
    blocks: Vec<Block>,
    past: Vec<Block>,
}

impl Schedule {
    const LANE_PADDING: Pixels = px(3.0);
    const STACK_GAP: Pixels = px(2.0);
    const MIN_HEIGHT: Pixels = px(11.0);

    pub fn plan(
        tasks: &mut [Task],
        log: &[Session],
        pin: Option<&Block>,
        horizon: i32,
        now: DateTime<Local>,
    ) -> Self {
        let minute = now.num_seconds_from_midnight() as i32 / 60;
        let mut blocks = Planner::plan(tasks, log, pin, horizon, minute, now.date_naive());
        let mut past = Self::past(tasks, log);

        Colorer::color(tasks, &mut blocks, &mut past);

        Self { past, blocks }
    }

    pub fn blocks(&self) -> impl Iterator<Item = &Block> {
        self.blocks.iter()
    }

    pub fn day(&self, day: i32, area: Bounds<Pixels>, now: f32) -> Vec<BlockView> {
        let mut blocks: Vec<_> = self
            .blocks
            .iter()
            .chain(&self.past)
            .filter(|block| block.covers(day))
            .collect();

        blocks.sort_by_key(|block| block.start);

        let mut bounds: Vec<_> = Self::columns(&blocks)
            .iter()
            .zip(&blocks)
            .map(|(column, block)| Self::bounds(block, day, *column, area))
            .collect();

        Self::separate(&mut bounds);

        blocks
            .iter()
            .zip(bounds)
            .filter_map(|(block, bounds)| BlockView::new(block, day, bounds, now))
            .collect()
    }

    fn past(tasks: &[Task], log: &[Session]) -> Vec<Block> {
        log.iter()
            .filter_map(|session| {
                let task = tasks
                    .iter()
                    .find(|task| task.id == session.task)
                    .filter(|task| matches!(task.kind, TaskKind::Flexible(_)))?;
                let span = (session.end - session.start).max(1);

                Some(Block::logged(
                    task,
                    session,
                    Planner::shape(task, session.work, span),
                ))
            })
            .collect()
    }

    fn scale(minutes: i32, area: Bounds<Pixels>) -> Pixels {
        area.size.height * (minutes as f32 / Block::MINUTES_PER_DAY as f32)
    }

    fn clusters(blocks: &[&Block]) -> Vec<Range<usize>> {
        let mut clusters: Vec<Range<usize>> = Vec::new();
        let mut end = i32::MIN;

        for (position, block) in blocks.iter().enumerate() {
            match clusters.last_mut() {
                Some(cluster) if block.start < end => cluster.end = position + 1,
                _ => clusters.push(position..position + 1),
            }

            end = end.max(block.end());
        }

        clusters
    }

    fn columns(blocks: &[&Block]) -> Vec<(usize, usize)> {
        let mut columns = Vec::with_capacity(blocks.len());

        for cluster in Self::clusters(blocks) {
            let lanes = Self::lanes(&blocks[cluster]);
            let count = lanes.iter().max().map_or(1, |lane| lane + 1);

            columns.extend(lanes.iter().map(|lane| (*lane, count)));
        }

        columns
    }

    fn bounds(
        block: &Block,
        day: i32,
        (lane, lanes): (usize, usize),
        area: Bounds<Pixels>,
    ) -> Bounds<Pixels> {
        let width = area.size.width / lanes as f32;
        let slice = block.slice(day);
        let top = Self::edge(slice.start, area);
        let bottom = Self::edge(slice.end, area);

        Bounds {
            origin: point(
                area.origin.x + width * lane as f32 + Self::LANE_PADDING,
                top,
            ),
            size: size(
                width - Self::LANE_PADDING * 2.0,
                (bottom - top).max(Self::MIN_HEIGHT),
            ),
        }
    }

    fn edge(minute: i32, area: Bounds<Pixels>) -> Pixels {
        px(f32::from(area.origin.y + Self::scale(minute, area)).round())
    }

    fn separate(bounds: &mut [Bounds<Pixels>]) {
        for position in 0..bounds.len() {
            let block = bounds[position];
            let below = bounds[position + 1..]
                .iter()
                .find(|other| other.left() < block.right() && block.left() < other.right());

            if let Some(below) = below {
                let room = below.top() - block.top() - Self::STACK_GAP;
                bounds[position].size.height = block.size.height.min(room.max(px(0.0)));
            }
        }
    }

    fn lanes(blocks: &[&Block]) -> Vec<usize> {
        let mut ends: Vec<i32> = Vec::new();

        blocks
            .iter()
            .map(|block| {
                let lane = ends
                    .iter()
                    .position(|end| *end <= block.start)
                    .unwrap_or(ends.len());

                match ends.get_mut(lane) {
                    Some(end) => *end = block.end(),
                    None => ends.push(block.end()),
                }

                lane
            })
            .collect()
    }
}
