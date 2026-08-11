use std::ops::Range;

use gpui::{Bounds, Pixels, point, px, size};

use crate::block::{Block, BlockView, Segment, SegmentKind};
use crate::theme::BlockColor;

pub struct Schedule {
    blocks: Vec<Block>,
}

impl Schedule {
    const LANE_PADDING: Pixels = px(3.0);
    const STACK_GAP: Pixels = px(2.0);
    const MIN_HEIGHT: Pixels = px(11.0);

    pub fn sample(days: i32) -> Self {
        let block = |day, hour, minute, color, title: &'static str, segments| {
            Block::new(Self::time(day, hour, minute), color, title, segments)
        };
        let prep = |minutes| Segment {
            kind: SegmentKind::Prep,
            minutes,
        };
        let work = |minutes| Segment {
            kind: SegmentKind::Work,
            minutes,
        };
        let pause = |minutes| Segment {
            kind: SegmentKind::Pause,
            minutes,
        };
        let cleanup = |minutes| Segment {
            kind: SegmentKind::Cleanup,
            minutes,
        };

        let mut blocks = vec![
            block(0, 6, 0, BlockColor::Slate, "Vitamins", vec![work(5)]),
            block(
                0,
                7,
                15,
                BlockColor::Green,
                "Wake-up run",
                vec![prep(5), work(10), cleanup(5)],
            ),
            block(
                0,
                9,
                15,
                BlockColor::Slate,
                "Research standup",
                vec![work(15)],
            ),
            block(
                0,
                9,
                40,
                BlockColor::Blue,
                "Calculus III lecture",
                vec![prep(20), work(90), cleanup(5)],
            )
            .at("Maths 210"),
            block(
                0,
                11,
                35,
                BlockColor::Amber,
                "Lunch",
                vec![prep(2), work(15), cleanup(3)],
            ),
            block(0, 11, 55, BlockColor::Green, "Walk back", vec![work(15)]),
            block(
                0,
                12,
                10,
                BlockColor::Violet,
                "Study, Calculus III",
                vec![prep(5), work(25), pause(5), work(20), cleanup(5)],
            ),
            block(
                0,
                14,
                0,
                BlockColor::Red,
                "Thesis draft, chapter 2",
                vec![prep(5), work(50), pause(10), work(40), cleanup(5)],
            )
            .at("Engineering Sciences Building, room 3.14"),
            block(
                0,
                17,
                0,
                BlockColor::Green,
                "Strength session",
                vec![prep(20), work(60), cleanup(20)],
            ),
            block(0, 20, 0, BlockColor::Amber, "Reading", vec![work(45)]),
            block(0, 20, 45, BlockColor::Slate, "Wind down", vec![work(10)]),
            block(0, 20, 55, BlockColor::Violet, "Lights out", vec![work(5)]),
        ];

        blocks.extend([
            block(
                1,
                9,
                0,
                BlockColor::Blue,
                "Physics lab",
                vec![prep(20), work(180), cleanup(10)],
            )
            .at("Science block B"),
            block(
                1,
                10,
                30,
                BlockColor::Red,
                "Supervisor meeting",
                vec![work(45)],
            )
            .at("Ada 214"),
            block(
                1,
                10,
                45,
                BlockColor::Amber,
                "Grant deadline call",
                vec![work(20)],
            ),
            block(
                1,
                15,
                0,
                BlockColor::Violet,
                "Study, Calculus III",
                vec![prep(5), work(45), cleanup(5)],
            ),
        ]);

        blocks.extend([
            block(
                2,
                8,
                0,
                BlockColor::Blue,
                "Comparative analysis of variational inference methods",
                vec![prep(10), work(120), cleanup(10)],
            )
            .at("Institute for Advanced Computational Sciences, seminar room 4"),
            block(
                2,
                11,
                0,
                BlockColor::Green,
                "Standing desk stretch",
                vec![work(4)],
            ),
            block(
                2,
                11,
                10,
                BlockColor::Amber,
                "Coffee with the visiting lecturer",
                vec![work(25)],
            )
            .at("Union cafe"),
            block(
                2,
                12,
                0,
                BlockColor::Red,
                "Marking, second-year problem sheets",
                vec![prep(5), work(90), pause(10), work(90), cleanup(5)],
            ),
        ]);

        blocks.extend((3..days).flat_map(|day| {
            [
                block(
                    day,
                    9,
                    15,
                    BlockColor::Slate,
                    "Research standup",
                    vec![work(15)],
                ),
                block(
                    day,
                    11,
                    40,
                    BlockColor::Amber,
                    "Lunch",
                    vec![prep(2), work(15), cleanup(3)],
                ),
                block(
                    day,
                    14,
                    0,
                    BlockColor::Violet,
                    "Study, Calculus III",
                    vec![prep(5), work(25), pause(5), work(20), cleanup(5)],
                ),
                block(day, 20, 0, BlockColor::Amber, "Reading", vec![work(45)]),
            ]
        }));

        blocks.sort_by_key(|block| block.start);
        Self { blocks }
    }

    fn time(day: i32, hour: i32, minute: i32) -> i32 {
        day * Block::MINUTES_PER_DAY + hour * 60 + minute
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
            .map(|((index, block), bounds)| BlockView::new(*index, block, bounds, now))
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
