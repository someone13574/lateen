use std::ops::Range;

use crate::block::Block;
use crate::task::TaskId;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Assumed,
    Done,
    Skipped,
}

#[derive(Clone, Copy)]
pub struct Session {
    pub task: TaskId,
    pub start: i32,
    pub end: i32,
    pub work: i32,
    pub outcome: Outcome,
}

impl Session {
    pub fn assumed(block: &Block) -> Self {
        Self {
            task: block.task,
            start: block.start,
            end: block.end(),
            work: block.work(),
            outcome: Outcome::Assumed,
        }
    }

    pub fn day(&self) -> i32 {
        self.start.div_euclid(Block::MINUTES_PER_DAY)
    }

    pub fn credited(&self) -> i32 {
        match self.outcome {
            Outcome::Skipped => 0,
            Outcome::Assumed | Outcome::Done => self.work,
        }
    }

    pub fn happened(&self) -> bool {
        self.outcome != Outcome::Skipped
    }

    pub fn within(&self, task: TaskId, run: &Range<i32>) -> bool {
        self.task == task && self.start >= run.start && self.start < run.end
    }
}
