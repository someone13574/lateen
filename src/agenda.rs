use std::cmp::Reverse;
use std::ops::Range;
use std::time::Duration;

use chrono::{DateTime, Local, Timelike};
use gpui::App;
use gpui::prelude::*;

use crate::block::Block;
use crate::clock::Clock;
use crate::schedule::Schedule;
use crate::session::{Outcome, Session};
use crate::task::{Repeat, Task, TaskId, TaskKind};

pub struct Agenda {
    tasks: Vec<Task>,
    log: Vec<Session>,
    pin: Option<Block>,
    schedule: Schedule,
    planned_at: i64,
    selected: Option<TaskId>,
}

impl Agenda {
    pub const HORIZON: i32 = 14;

    pub fn new(cx: &mut Context<Self>) -> Self {
        let mut tasks = Task::seed();
        let log = Vec::new();
        let now = cx.global::<Clock>().now();
        Self::follow_minutes(cx);

        Self {
            schedule: Schedule::plan(&mut tasks, &log, None, Self::HORIZON, now),
            planned_at: Self::minute(now),
            pin: None,
            tasks,
            log,
            selected: None,
        }
    }

    pub fn selected(&self) -> Option<TaskId> {
        self.selected
    }

    pub fn select(&mut self, task: TaskId, cx: &mut Context<Self>) {
        self.selected = Some(task);
        cx.notify();
    }

    pub fn deselect(&mut self, cx: &mut Context<Self>) {
        self.selected = None;
        cx.notify();
    }

    pub fn schedule(&self) -> &Schedule {
        &self.schedule
    }

    pub fn tasks(&self) -> &[Task] {
        &self.tasks
    }

    pub fn unconfirmed(&self) -> Vec<&Session> {
        let mut pending: Vec<_> = self
            .log
            .iter()
            .filter(|session| session.outcome == Outcome::Assumed)
            .collect();

        pending.sort_by_key(|session| Reverse(session.end));

        pending
    }

    pub fn logged(&self, task: TaskId, run: &Range<i32>) -> Vec<Session> {
        let mut sessions: Vec<_> = self
            .log
            .iter()
            .filter(|session| session.within(task, run))
            .copied()
            .collect();

        sessions.sort_by_key(|session| session.start);

        sessions
    }

    pub fn progress(&self, task: &Task, now: i32) -> Option<(i32, i32)> {
        let TaskKind::Flexible(flexible) = &task.kind else {
            return None;
        };
        let repeating = matches!(flexible.repeat, Repeat::Once { .. });
        if !task.splittable() && !repeating {
            return None;
        }

        let run = task.run(now.div_euclid(Block::MINUTES_PER_DAY));
        let done = self
            .log
            .iter()
            .filter(|session| session.within(task.id, &run))
            .map(Session::credited)
            .sum();

        Some((done, flexible.total))
    }

    pub fn task(&self, id: TaskId) -> Option<&Task> {
        self.tasks.iter().find(|task| task.id == id)
    }

    pub fn running(&self, now: i32) -> Option<&Block> {
        self.schedule
            .blocks()
            .filter(|block| block.start <= now && block.end() > now)
            .max_by_key(|block| self.task(block.task).map(|task| task.priority))
    }

    pub fn upcoming(&self, now: i32) -> Option<&Block> {
        self.schedule
            .blocks()
            .filter(|block| block.start > now)
            .min_by_key(|block| block.start)
    }

    pub fn replan(&mut self, cx: &App) -> bool {
        let now = cx.global::<Clock>().now();
        if Self::minute(now) == self.planned_at {
            return false;
        }

        let minute = Self::minute_of_day(now);
        self.sweep(minute);
        self.hold(minute);
        self.plan(now);
        self.hold(minute);

        true
    }

    pub fn confirm(&mut self, task: TaskId, start: i32, outcome: Outcome, cx: &mut Context<Self>) {
        let session = self
            .log
            .iter_mut()
            .find(|session| session.task == task && session.start == start);

        if let Some(session) = session {
            session.outcome = outcome;
            self.plan(cx.global::<Clock>().now());
            cx.notify();
        }
    }

    pub fn confirm_all(&mut self, cx: &mut Context<Self>) {
        for session in &mut self.log {
            if session.outcome == Outcome::Assumed {
                session.outcome = Outcome::Done;
            }
        }

        self.plan(cx.global::<Clock>().now());
        cx.notify();
    }

    pub fn end_early(&mut self, task: TaskId, start: i32, cx: &mut Context<Self>) {
        let now = Self::minute_of_day(cx.global::<Clock>().now());
        let Some(credits_elapsed) = self
            .task(task)
            .map(|task| matches!(task.kind, TaskKind::Fixed { .. }) || task.splittable())
        else {
            return;
        };

        let work = match credits_elapsed {
            true => self
                .block(task, start)
                .map_or(0, |block| block.elapsed_work(now)),
            false => self.remaining(task, start),
        };

        self.record(task, start, now, work, Outcome::Done, cx);
    }

    pub fn finish(&mut self, task: TaskId, start: i32, cx: &mut Context<Self>) {
        let now = Self::minute_of_day(cx.global::<Clock>().now());
        let work = self.remaining(task, start);

        self.record(task, start, now, work, Outcome::Done, cx);
    }

    pub fn skip(&mut self, task: TaskId, start: i32, cx: &mut Context<Self>) {
        let Some(end) = self.block(task, start).map(Block::end) else {
            return;
        };

        self.record(task, start, end, 0, Outcome::Skipped, cx);
    }

    pub fn edit(&mut self, task: TaskId, edit: impl FnOnce(&mut Task), cx: &mut Context<Self>) {
        let Some(target) = self.tasks.iter_mut().find(|other| other.id == task) else {
            return;
        };

        edit(target);
        self.plan(cx.global::<Clock>().now());
        cx.notify();
    }

    pub fn remove(&mut self, task: TaskId, cx: &mut Context<Self>) {
        if self.pin.as_ref().is_some_and(|pin| pin.task == task) {
            self.pin = None;
        }

        self.tasks.retain(|other| other.id != task);
        self.log.retain(|session| session.task != task);
        self.selected = None;
        self.plan(cx.global::<Clock>().now());
        cx.notify();
    }

    fn hold(&mut self, now: i32) {
        if self.pin.as_ref().is_some_and(|pin| pin.end() <= now) {
            self.pin = None;
        }

        if self.pin.is_some() {
            return;
        }

        self.pin = self
            .schedule
            .blocks()
            .find(|block| {
                block.start <= now
                    && block.end() > now
                    && self.is_flexible(block.task)
                    && !self.recorded(block)
            })
            .cloned();
    }

    fn pinned(&self, task: TaskId, start: i32) -> bool {
        self.pin
            .as_ref()
            .is_some_and(|pin| pin.task == task && pin.start == start)
    }

    fn block(&self, task: TaskId, start: i32) -> Option<&Block> {
        self.schedule
            .blocks()
            .find(|block| block.task == task && block.start == start)
    }

    fn remaining(&self, task: TaskId, start: i32) -> i32 {
        let Some(TaskKind::Flexible(flexible)) = self.task(task).map(|task| &task.kind) else {
            return 0;
        };
        let run = self
            .task(task)
            .unwrap()
            .run(start.div_euclid(Block::MINUTES_PER_DAY));
        let done: i32 = self
            .log
            .iter()
            .filter(|session| session.within(task, &run))
            .map(Session::credited)
            .sum();

        (flexible.total - done).max(0)
    }

    fn record(
        &mut self,
        task: TaskId,
        start: i32,
        end: i32,
        work: i32,
        outcome: Outcome,
        cx: &mut Context<Self>,
    ) {
        let session = Session {
            task,
            start,
            end,
            work,
            outcome,
        };
        let existing = self
            .log
            .iter_mut()
            .find(|other| other.task == task && other.start == start);

        match existing {
            Some(existing) => *existing = session,
            None => self.log.push(session),
        }

        if self.pinned(task, start) {
            self.pin = None;
        }

        self.plan(cx.global::<Clock>().now());
        cx.notify();
    }

    fn plan(&mut self, now: DateTime<Local>) {
        self.schedule = Schedule::plan(
            &mut self.tasks,
            &self.log,
            self.pin.as_ref(),
            Self::HORIZON,
            now,
        );
        self.planned_at = Self::minute(now);
    }

    fn sweep(&mut self, now: i32) {
        let elapsed: Vec<_> = self
            .schedule
            .blocks()
            .filter(|block| block.end() <= now && self.is_flexible(block.task))
            .filter(|block| !self.recorded(block))
            .map(Session::assumed)
            .collect();

        self.log.extend(elapsed);
    }

    fn recorded(&self, block: &Block) -> bool {
        self.log
            .iter()
            .any(|session| session.task == block.task && session.start == block.start)
    }

    fn is_flexible(&self, task: TaskId) -> bool {
        self.tasks
            .iter()
            .any(|other| other.id == task && matches!(other.kind, TaskKind::Flexible(_)))
    }

    fn minute_of_day(now: DateTime<Local>) -> i32 {
        now.num_seconds_from_midnight() as i32 / 60
    }

    fn follow_minutes(cx: &mut Context<Self>) {
        cx.spawn(async move |agenda, cx| {
            loop {
                cx.background_executor()
                    .timer(Self::until_next_minute())
                    .await;

                let replanned =
                    agenda.update(cx, |agenda, cx| agenda.replan(cx).then(|| cx.notify()));
                if replanned.is_err() {
                    break;
                }
            }
        })
        .detach();
    }

    fn until_next_minute() -> Duration {
        let now = Local::now();
        let elapsed = now.second() as u64 * 1_000_000_000 + now.nanosecond() as u64 % 1_000_000_000;

        Duration::from_nanos(60_000_000_000_u64.saturating_sub(elapsed).max(1))
    }

    fn minute(now: DateTime<Local>) -> i64 {
        now.timestamp().div_euclid(60)
    }
}
