use crate::block::Block;
use crate::task::{Task, TaskId};
use crate::theme::BlockColor;

pub struct Colorer<'a> {
    tasks: &'a mut [Task],
    neighbours: Vec<Neighbour>,
    widest: f32,
}

impl<'a> Colorer<'a> {
    const CLOSE: i32 = 240;
    const NEAR_ENOUGH: f32 = 0.5;
    const BAND: f32 = 0.15;
    const LIKENESS: f32 = 0.5;
    const TODAY_STARTS: i32 = 0;

    pub fn color(tasks: &'a mut [Task], blocks: &mut [Block], past: &mut [Block]) {
        for index in 0..tasks.len() {
            if tasks[index].color.is_some() {
                continue;
            }

            let color = Self::pick(&tasks[index], &tasks[..index]);
            tasks[index].color = Some(color);
        }

        let neighbours = {
            let mut ordered: Vec<&Block> = blocks
                .iter()
                .chain(past.iter())
                .filter(|block| block.end() > Self::TODAY_STARTS)
                .collect();

            ordered.sort_by_key(|block| block.start);
            Self::neighbours(tasks, &ordered)
        };

        let mut colorer = Self {
            neighbours,
            widest: Self::widest(),
            tasks,
        };

        colorer.repaint();

        for block in blocks.iter_mut().chain(past.iter_mut()) {
            block.color = Self::position(colorer.tasks, block.task)
                .and_then(|task| colorer.tasks[task].color);
        }
    }

    fn position(tasks: &[Task], id: TaskId) -> Option<usize> {
        tasks.iter().position(|task| task.id == id)
    }

    fn pick(task: &Task, against: &[Task]) -> BlockColor {
        let cost = Self::cost(task, against);
        let mut cheapest = BlockColor::ALL[0];

        for color in BlockColor::ALL {
            if cost[color as usize] < cost[cheapest as usize] {
                cheapest = color;
            }
        }

        cheapest
    }

    fn cost(task: &Task, against: &[Task]) -> [i32; BlockColor::ALL.len()] {
        let mut cost = [0; BlockColor::ALL.len()];
        let window = task.window();

        for other in against {
            let Some(color) = other.color else {
                continue;
            };
            let other_window = other.window();
            let shares_day = other.days.iter().any(|day| task.days.contains(day));
            let shares_hours = other_window.start < window.end && other_window.end > window.start;

            cost[color as usize] += match (shares_day, shares_hours) {
                (true, true) => 10,
                (true, false) => 4,
                _ => 1,
            };
        }

        cost
    }

    fn neighbours(tasks: &[Task], blocks: &[&Block]) -> Vec<Neighbour> {
        let mut neighbours = Vec::new();

        for (position, block) in blocks.iter().enumerate() {
            for (between, other) in blocks[position + 1..].iter().enumerate() {
                let gap = (other.start - block.end()).max(0);

                if other.day() != block.day() || gap > Self::CLOSE {
                    break;
                }

                if other.task == block.task {
                    continue;
                }

                if let Some((earlier, later)) =
                    Self::position(tasks, block.task).zip(Self::position(tasks, other.task))
                {
                    neighbours.push(Neighbour {
                        earlier,
                        later,
                        gap,
                        between,
                    });
                }
            }
        }

        neighbours
    }

    fn widest() -> f32 {
        BlockColor::ALL
            .into_iter()
            .flat_map(|color| BlockColor::ALL.map(|other| color.gap(other)))
            .fold(0.0, f32::max)
    }

    fn repaint(&mut self) {
        let near_enough = self.widest * Self::NEAR_ENOUGH;
        let mut moved = vec![false; self.tasks.len()];

        for index in self.movers(near_enough) {
            if moved[index] {
                continue;
            }

            if let Some(color) = self.choose(index, near_enough) {
                self.tasks[index].color = Some(color);
                moved[index] = true;
            }
        }
    }

    fn movers(&self, near_enough: f32) -> Vec<usize> {
        self.neighbours
            .iter()
            .filter(|neighbour| {
                self.tasks[neighbour.earlier]
                    .color
                    .is_some_and(|color| self.distance(color, neighbour.later) < near_enough)
            })
            .map(|neighbour| neighbour.earlier.max(neighbour.later))
            .collect()
    }

    fn choose(&self, index: usize, near_enough: f32) -> Option<BlockColor> {
        let current = self.tasks[index].color?;
        let held = self.min_gap(index, current);
        let used = self.used(index);
        let mut candidates = self.survivors(index, near_enough);
        let relaxed = candidates.is_empty();

        if relaxed {
            candidates = self.roomiest(index);
        }

        let pick = self.cheapest(index, &candidates, &used)?;

        if pick == current {
            return None;
        }

        let better = if relaxed {
            self.min_gap(index, pick) > held
        } else {
            held < near_enough
                || self.score(index, pick, &used) < self.score(index, current, &used) - 0.01
        };

        better.then_some(pick)
    }

    fn survivors(&self, index: usize, near_enough: f32) -> Vec<BlockColor> {
        BlockColor::ALL
            .into_iter()
            .filter(|color| self.min_gap(index, *color) >= near_enough)
            .collect()
    }

    fn roomiest(&self, index: usize) -> Vec<BlockColor> {
        let top = BlockColor::ALL
            .into_iter()
            .map(|color| self.min_gap(index, color))
            .fold(0.0, f32::max);

        BlockColor::ALL
            .into_iter()
            .filter(|color| self.min_gap(index, *color) >= top - self.widest * Self::BAND)
            .collect()
    }

    fn cheapest(
        &self,
        index: usize,
        candidates: &[BlockColor],
        used: &[i32; BlockColor::ALL.len()],
    ) -> Option<BlockColor> {
        candidates.iter().copied().min_by(|left, right| {
            self.score(index, *left, used)
                .total_cmp(&self.score(index, *right, used))
        })
    }

    fn score(&self, index: usize, color: BlockColor, used: &[i32; BlockColor::ALL.len()]) -> f32 {
        let mut alike = 0.0;
        let mut total = 0.0;

        for neighbour in &self.neighbours {
            let Some(other) = neighbour.other(index) else {
                continue;
            };
            let weight = neighbour.weight();

            alike += (1.0 - (self.distance(color, other) / self.widest).min(1.0)) * weight;
            total += weight;
        }

        let reuse = used[color as usize] as f32;

        if total > 0.0 {
            reuse + (alike / total) * Self::LIKENESS
        } else {
            reuse
        }
    }

    fn min_gap(&self, index: usize, color: BlockColor) -> f32 {
        self.abutting(index)
            .map(|other| self.distance(color, other))
            .fold(f32::INFINITY, f32::min)
    }

    fn abutting(&self, index: usize) -> impl Iterator<Item = usize> {
        self.neighbours
            .iter()
            .filter_map(move |neighbour| neighbour.other(index))
    }

    fn used(&self, index: usize) -> [i32; BlockColor::ALL.len()] {
        let mut used = [0; BlockColor::ALL.len()];

        for (other, task) in self.tasks.iter().enumerate() {
            if other == index {
                continue;
            }

            if let Some(color) = task.color {
                used[color as usize] += 1;
            }
        }

        used
    }

    fn distance(&self, color: BlockColor, task: usize) -> f32 {
        self.tasks[task]
            .color
            .map_or(f32::INFINITY, |other| color.gap(other))
    }
}

struct Neighbour {
    earlier: usize,
    later: usize,
    gap: i32,
    between: usize,
}

impl Neighbour {
    fn other(&self, task: usize) -> Option<usize> {
        if self.earlier == task {
            Some(self.later)
        } else if self.later == task {
            Some(self.earlier)
        } else {
            None
        }
    }

    fn weight(&self) -> f32 {
        (Colorer::CLOSE as f32 / (30.0 + self.gap as f32)) / (1.0 + 3.0 * self.between as f32)
    }
}
