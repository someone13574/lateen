use std::cmp::Reverse;
use std::collections::HashSet;
use std::ops::Range;
use std::time::Duration;

use chrono::{DateTime, Local, NaiveDate, Timelike};
use gpui::App;
use gpui::prelude::*;
use gpui_tokio::Tokio;

use crate::block::Block;
use crate::clock::Clock;
use crate::ics::{Ics, Import};
use crate::notifier::Notifier;
use crate::schedule::Schedule;
use crate::session::{Outcome, Session};
use crate::store::StoredAgenda;
use crate::subscription::{Subscription, SubscriptionId};
use crate::task::{Repeat, Task, TaskId, TaskKind};

pub struct Agenda {
    tasks: Vec<Task>,
    subscriptions: Vec<Subscription>,
    log: Vec<Session>,
    pin: Option<Block>,
    schedule: Schedule,
    planned_at: i64,
    planned_on: NaiveDate,
    resumed_on: Option<NaiveDate>,
    selected: Option<TaskId>,
}

impl Agenda {
    pub const HORIZON: i32 = 48;

    const SAMPLE: Duration = Duration::from_secs(1);
    const RESYNC_MINUTES: i64 = 30;

    pub fn new(cx: &mut Context<Self>) -> Self {
        Self::follow_clock(cx);

        let mut agenda = match StoredAgenda::load() {
            Some(stored) => Self::restored(stored, cx),
            None => Self::seeded(cx),
        };

        agenda.resync(cx);

        agenda
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

    pub fn subscriptions(&self) -> &[Subscription] {
        &self.subscriptions
    }

    pub fn subscribe(&mut self, url: &str, cx: &mut Context<Self>) {
        let subscription = Subscription::new(url);
        let known = self
            .subscriptions
            .iter()
            .find(|other| other.url == subscription.url)
            .map(|other| other.id);
        let id = known.unwrap_or(subscription.id);

        if known.is_none() {
            self.subscriptions.push(subscription);
        }

        self.sync(id, cx);
    }

    pub fn unsubscribe(&mut self, id: SubscriptionId, cx: &mut Context<Self>) {
        self.subscriptions
            .retain(|subscription| subscription.id != id);
        self.drop_imported(id);
        self.drop_dangling();
        self.plan(cx.global::<Clock>().now(), cx);
        cx.notify();
    }

    pub fn sync(&mut self, id: SubscriptionId, cx: &mut Context<Self>) {
        let today = self.planned_on;
        let Some(subscription) = self.subscriptions.iter_mut().find(|other| other.id == id) else {
            return;
        };

        if subscription.syncing {
            return;
        }

        subscription.syncing = true;

        let fetch = Tokio::spawn_result(cx, Subscription::fetch(subscription.url.clone()));

        cx.spawn(async move |agenda, cx| {
            let import = cx
                .background_spawn(async move {
                    anyhow::Ok(Ics::parse(&fetch.await?).import(id, today, Agenda::HORIZON))
                })
                .await;

            agenda
                .update(cx, |agenda, cx| agenda.adopt(id, import, today, cx))
                .ok();
        })
        .detach();

        cx.notify();
    }

    pub fn today(&self) -> NaiveDate {
        self.planned_on
    }

    pub fn resumed_on(&self) -> Option<NaiveDate> {
        self.resumed_on
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
        let one_off = matches!(flexible.repeat, Repeat::Never);
        if !task.splittable() && !one_off {
            return None;
        }

        let run = task.run(now.div_euclid(Block::MINUTES_PER_DAY), self.planned_on);
        let done: i32 = self
            .log
            .iter()
            .filter(|session| session.within(task.id, &run))
            .map(Session::credited)
            .sum();
        let running = self
            .pin
            .as_ref()
            .filter(|pin| pin.task == task.id && run.contains(&pin.start))
            .map_or(0, |pin| pin.elapsed_work(now));

        Some((done + running, flexible.total))
    }

    pub fn task(&self, id: TaskId) -> Option<&Task> {
        self.tasks.iter().find(|task| task.id == id)
    }

    pub fn running(&self, now: f32) -> Option<&Block> {
        self.schedule
            .blocks()
            .filter(|block| block.start as f32 <= now && block.end() as f32 > now)
            .min_by_key(|block| Reverse(self.task(block.task).map(|task| task.priority)))
    }

    pub fn upcoming(&self, now: f32) -> Option<&Block> {
        self.schedule
            .blocks()
            .filter(|block| block.start as f32 > now)
            .min_by_key(|block| block.start)
    }

    pub fn replan(&mut self, cx: &App) -> bool {
        let now = cx.global::<Clock>().now();
        if Self::minute(now) == self.planned_at {
            return false;
        }

        self.advance(now, cx);

        true
    }

    pub fn confirm(&mut self, task: TaskId, start: i32, outcome: Outcome, cx: &mut Context<Self>) {
        let session = self
            .log
            .iter_mut()
            .find(|session| session.task == task && session.start == start);

        if let Some(session) = session {
            session.outcome = outcome;
            self.plan(cx.global::<Clock>().now(), cx);
            cx.notify();
        }
    }

    pub fn toggle(&mut self, task: TaskId, start: i32, outcome: Outcome, cx: &mut Context<Self>) {
        let settled = self
            .log
            .iter()
            .find(|session| session.task == task && session.start == start)
            .is_some_and(|session| session.outcome == outcome);

        match settled {
            true => self.confirm(task, start, Outcome::Assumed, cx),
            false => self.confirm(task, start, outcome, cx),
        }
    }

    pub fn confirm_all(&mut self, days: Range<i32>, cx: &mut Context<Self>) {
        for session in &mut self.log {
            if session.outcome == Outcome::Assumed && days.contains(&session.day()) {
                session.outcome = Outcome::Done;
            }
        }

        self.plan(cx.global::<Clock>().now(), cx);
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
        self.share_unmanaged(task);

        if self.pin.as_ref().is_some_and(|pin| pin.task == task) {
            self.pin = None;
        }

        self.plan(cx.global::<Clock>().now(), cx);
        cx.notify();
    }

    pub fn add(&mut self, cx: &mut Context<Self>) {
        let task = Task::draft();

        self.selected = Some(task.id);
        self.tasks.push(task);
        self.plan(cx.global::<Clock>().now(), cx);
        cx.notify();
    }

    pub fn remove(&mut self, task: TaskId, cx: &mut Context<Self>) {
        if self.pin.as_ref().is_some_and(|pin| pin.task == task) {
            self.pin = None;
        }

        self.tasks.retain(|other| other.id != task);
        self.log.retain(|session| session.task != task);
        self.selected = None;
        self.plan(cx.global::<Clock>().now(), cx);
        cx.notify();
    }

    fn adopt(
        &mut self,
        id: SubscriptionId,
        import: anyhow::Result<Import>,
        today: NaiveDate,
        cx: &mut Context<Self>,
    ) {
        let now = cx.global::<Clock>().now();
        let Some(subscription) = self.subscriptions.iter_mut().find(|other| other.id == id) else {
            return;
        };

        match import {
            Ok(import) => {
                let tasks = subscription.imported(import, now);

                self.replace(id, tasks, today);
            }
            Err(failure) => subscription.failed(failure, now),
        }

        self.plan(now, cx);
        cx.notify();
    }

    fn replace(&mut self, id: SubscriptionId, mut tasks: Vec<Task>, today: NaiveDate) {
        let rolled = (self.planned_on - today).num_days() as i32;
        let mut claimed = HashSet::new();

        for task in &mut tasks {
            task.shift(rolled);

            if let Some(previous) = self
                .tasks
                .iter()
                .find(|other| Self::same_event(other, task))
            {
                task.keep_unmanaged(previous);
            }

            let previous = self
                .tasks
                .iter()
                .find(|other| Self::same_occurrence(other, task) && !claimed.contains(&other.id));

            if let Some(previous) = previous {
                claimed.insert(previous.id);
                task.id = previous.id;
            }
        }

        self.drop_imported(id);
        self.tasks.extend(tasks);
        self.drop_dangling();
    }

    fn share_unmanaged(&mut self, task: TaskId) {
        let Some(edited) = self.task(task).cloned() else {
            return;
        };
        let Some(source) = &edited.source else {
            return;
        };

        for other in &mut self.tasks {
            if other.id != task && other.source.as_ref() == Some(source) {
                other.keep_unmanaged(&edited);
            }
        }
    }

    fn drop_imported(&mut self, id: SubscriptionId) {
        self.tasks.retain(|task| {
            task.source
                .as_ref()
                .is_none_or(|source| source.subscription != id)
        });
    }

    fn drop_dangling(&mut self) {
        let remaining: Vec<_> = self.tasks.iter().map(|task| task.id).collect();

        self.log.retain(|session| remaining.contains(&session.task));
        self.pin = self.pin.take().filter(|pin| remaining.contains(&pin.task));
        self.selected = self.selected.filter(|task| remaining.contains(task));
    }

    fn same_occurrence(task: &Task, other: &Task) -> bool {
        Self::same_event(task, other) && task.dates.from == other.dates.from
    }

    fn same_event(task: &Task, other: &Task) -> bool {
        task.source.is_some() && task.source == other.source
    }

    fn resync(&mut self, cx: &mut Context<Self>) {
        let subscribed: Vec<_> = self
            .subscriptions
            .iter()
            .map(|subscription| subscription.id)
            .collect();

        for id in subscribed {
            self.sync(id, cx);
        }
    }

    fn refresh(&mut self, cx: &mut Context<Self>) {
        let now = cx.global::<Clock>().now();
        let stale: Vec<_> = self
            .subscriptions
            .iter()
            .filter(|subscription| !subscription.syncing)
            .filter(|subscription| {
                subscription.synced.is_none_or(|synced| {
                    (now - synced).num_minutes() >= Self::RESYNC_MINUTES
                        || synced.date_naive() != now.date_naive()
                })
            })
            .map(|subscription| subscription.id)
            .collect();

        for id in stale {
            self.sync(id, cx);
        }
    }

    fn seeded(cx: &App) -> Self {
        let now = cx.global::<Clock>().now();
        let mut tasks = if StoredAgenda::starts_empty() {
            Vec::new()
        } else {
            Task::seed()
        };
        let log = Vec::new();
        let agenda = Self {
            schedule: Schedule::plan(&mut tasks, &log, None, Self::HORIZON, now),
            planned_at: Self::minute(now),
            planned_on: now.date_naive(),
            resumed_on: None,
            pin: None,
            subscriptions: Vec::new(),
            tasks,
            log,
            selected: None,
        };

        agenda.store(now, cx);

        agenda
    }

    fn restored(stored: StoredAgenda, cx: &mut App) -> Self {
        let StoredAgenda {
            mut tasks,
            subscriptions,
            log,
            pin,
            planned_at,
            clock_offset,
        } = stored;

        Clock::resume(clock_offset, cx);

        let now = cx.global::<Clock>().now();

        for task in &tasks {
            task.id.reserve();
        }

        for subscription in &subscriptions {
            subscription.id.reserve();
        }

        let mut agenda = Self {
            schedule: Schedule::plan(&mut tasks, &log, pin.as_ref(), Self::HORIZON, planned_at),
            planned_on: planned_at.date_naive(),
            resumed_on: Some(planned_at.date_naive()),
            planned_at: Self::minute(planned_at),
            pin,
            subscriptions,
            tasks,
            log,
            selected: None,
        };

        agenda.catch_up(now, cx);

        agenda
    }

    fn catch_up(&mut self, now: DateTime<Local>, cx: &App) {
        while self.planned_on < now.date_naive() {
            let Some(midnight) = Self::midnight_after(self.planned_on) else {
                break;
            };

            self.plan(midnight, cx);
        }

        self.advance(now, cx);
    }

    fn midnight_after(day: NaiveDate) -> Option<DateTime<Local>> {
        day.succ_opt()?
            .and_hms_opt(0, 0, 0)?
            .and_local_timezone(Local)
            .earliest()
    }

    fn advance(&mut self, now: DateTime<Local>, cx: &App) {
        self.roll(now.date_naive());

        let minute = Self::minute_of_day(now);
        self.sweep(minute);
        self.settle(minute);
        self.hold(minute);
        self.plan(now, cx);
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
            .run(start.div_euclid(Block::MINUTES_PER_DAY), self.planned_on);
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

        self.plan(cx.global::<Clock>().now(), cx);
        cx.notify();
    }

    fn plan(&mut self, now: DateTime<Local>, cx: &App) {
        self.roll(now.date_naive());
        self.schedule = Schedule::plan(
            &mut self.tasks,
            &self.log,
            self.pin.as_ref(),
            Self::HORIZON,
            now,
        );
        self.planned_at = Self::minute(now);
        self.hold(Self::minute_of_day(now));
        self.store(now, cx);
    }

    fn store(&self, now: DateTime<Local>, cx: &App) {
        let stored = StoredAgenda {
            tasks: self.tasks.clone(),
            subscriptions: self.subscriptions.clone(),
            log: self.log.clone(),
            pin: self.pin.clone(),
            planned_at: now,
            clock_offset: cx.global::<Clock>().offset(),
        };

        cx.background_spawn(async move { stored.save() })
            .detach_and_log_err(cx);
    }

    fn roll(&mut self, today: NaiveDate) {
        let days = (today - self.planned_on).num_days() as i32;
        if days == 0 {
            return;
        }

        self.planned_on = today;

        if days > 0 {
            self.sweep(days * Block::MINUTES_PER_DAY);
        }

        self.rebase(days);
    }

    fn rebase(&mut self, days: i32) {
        let shift = days * Block::MINUTES_PER_DAY;

        for session in &mut self.log {
            session.start -= shift;
            session.end -= shift;
        }

        if let Some(pin) = &mut self.pin {
            pin.start -= shift;
        }

        for task in &mut self.tasks {
            task.shift(days);
        }
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

    fn settle(&mut self, now: i32) {
        let Self { tasks, log, .. } = self;

        for session in log.iter_mut().filter(|s| s.outcome == Outcome::Assumed) {
            let splittable = tasks
                .iter()
                .any(|task| task.id == session.task && task.splittable());
            let midnight =
                (session.start.div_euclid(Block::MINUTES_PER_DAY) + 1) * Block::MINUTES_PER_DAY;

            if !splittable && now >= midnight {
                session.outcome = Outcome::Done;
            }
        }
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

    fn follow_clock(cx: &mut Context<Self>) {
        cx.spawn(async move |agenda, cx| {
            loop {
                cx.background_executor().timer(Self::SAMPLE).await;

                let ticked = agenda.update(cx, |agenda, cx| {
                    if agenda.replan(cx) {
                        cx.notify();
                    }

                    agenda.refresh(cx);
                    Notifier::announce(agenda, cx);
                });

                if ticked.is_err() {
                    break;
                }
            }
        })
        .detach();
    }

    fn minute(now: DateTime<Local>) -> i64 {
        now.timestamp().div_euclid(60)
    }
}
