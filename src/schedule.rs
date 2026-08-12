use std::ops::Range;

use chrono::{DateTime, Datelike, Local, Timelike};
use gpui::{Bounds, Pixels, point, px, size};

use crate::block::{Block, BlockView};
use crate::colorer::Colorer;
use crate::planner::Planner;
use crate::task::Task;

pub struct Schedule {
    blocks: Vec<Block>,
}

impl Schedule {
    const LANE_PADDING: Pixels = px(3.0);
    const STACK_GAP: Pixels = px(2.0);
    const MIN_HEIGHT: Pixels = px(11.0);

    pub fn plan(tasks: &mut [Task], horizon: i32, now: DateTime<Local>) -> Self {
        let mut blocks = Planner::plan(
            tasks,
            horizon,
            now.num_seconds_from_midnight() as i32 / 60,
            now.weekday(),
        );

        Colorer::color(tasks, &mut blocks);

        Self { blocks }
    }

    pub fn day(&self, day: i32, area: Bounds<Pixels>, now: i32) -> Vec<BlockView> {
        let blocks: Vec<_> = self
            .blocks
            .iter()
            .enumerate()
            .filter(|(_, block)| block.day() == day)
            .collect();

        let mut bounds: Vec<_> = Self::columns(&blocks)
            .iter()
            .zip(&blocks)
            .map(|(column, (_, block))| Self::bounds(block, *column, area))
            .collect();

        Self::separate(&mut bounds);

        blocks
            .iter()
            .zip(bounds)
            .filter_map(|((index, block), bounds)| BlockView::new(*index, block, bounds, now))
            .collect()
    }

    fn scale(minutes: i32, area: Bounds<Pixels>) -> Pixels {
        area.size.height * (minutes as f32 / Block::MINUTES_PER_DAY as f32)
    }

    fn clusters(blocks: &[(usize, &Block)]) -> Vec<Range<usize>> {
        let mut clusters: Vec<Range<usize>> = Vec::new();
        let mut end = i32::MIN;

        for (position, (_, block)) in blocks.iter().enumerate() {
            match clusters.last_mut() {
                Some(cluster) if block.start < end => cluster.end = position + 1,
                _ => clusters.push(position..position + 1),
            }

            end = end.max(block.end());
        }

        clusters
    }

    fn columns(blocks: &[(usize, &Block)]) -> Vec<(usize, usize)> {
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
        (lane, lanes): (usize, usize),
        area: Bounds<Pixels>,
    ) -> Bounds<Pixels> {
        let width = area.size.width / lanes as f32;

        Bounds {
            origin: point(
                area.origin.x + width * lane as f32 + Self::LANE_PADDING,
                area.origin.y + Self::scale(block.minute_of_day(), area),
            ),
            size: size(
                width - Self::LANE_PADDING * 2.0,
                Self::scale(block.span(), area).max(Self::MIN_HEIGHT),
            ),
        }
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

    fn lanes(blocks: &[(usize, &Block)]) -> Vec<usize> {
        let mut ends: Vec<i32> = Vec::new();

        blocks
            .iter()
            .map(|(_, block)| {
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
