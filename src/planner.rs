use std::cmp::Reverse;
use std::ops::Range;

use chrono::Weekday;

use crate::block::{Block, Segment, SegmentKind};
use crate::task::{Breaks, Flexible, Priority, Repeat, Task, TaskKind};

pub struct Planner<'a> {
    tasks: &'a [Task],
    today: Weekday,
    now: i32,
    horizon: i32,
    reservations: Vec<Reservation>,
}

impl<'a> Planner<'a> {
    const RELAXATIONS: [Relaxation; 6] = [
        Relaxation::new(true, true, true, false),
        Relaxation::new(false, true, true, false),
        Relaxation::new(false, false, true, false),
        Relaxation::new(true, true, true, true),
        Relaxation::new(false, false, true, true),
        Relaxation::new(false, false, false, true),
    ];
    const START_GRID: i32 = 5;
    const MIN_SESSION_GAP: i32 = 20;

    pub fn plan(tasks: &'a [Task], horizon: i32, now: i32, today: Weekday) -> Vec<Block> {
        let mut planner = Self {
            tasks,
            today,
            now,
            horizon,
            reservations: Vec::new(),
        };

        let mut blocks = planner.fixed();
        let fixed = planner.reservations.len();
        let instances = planner.instances();
        let mut order: Vec<_> = (0..instances.len()).collect();
        let (mut sessions, squeezed) = planner.place_all(&instances, &order, fixed);

        if squeezed.contains(&true) {
            order.sort_by_key(|instance| !squeezed[*instance]);
            sessions = planner.place_all(&instances, &order, fixed).0;
        }

        blocks.extend(
            Self::merge(&instances, sessions)
                .into_iter()
                .map(|session| session.block(&instances)),
        );
        blocks.sort_by_key(|block| block.start);

        blocks
    }

    fn fixed(&mut self) -> Vec<Block> {
        let tasks = self.tasks;
        let mut blocks = Vec::new();

        for (index, task) in tasks.iter().enumerate() {
            let TaskKind::Fixed { start, duration } = task.kind else {
                continue;
            };

            for day in 0..self.horizon {
                if !self.occurs_on(task, day) {
                    continue;
                }

                let segments = Self::segments(task, duration, None);
                let block_start = day * Block::MINUTES_PER_DAY + start - task.prep;
                self.reserve(block_start, Self::span(&segments), task.priority);
                blocks.push(Self::block(index, task, block_start, segments));
            }
        }

        blocks
    }

    fn instances(&self) -> Vec<Instance<'a>> {
        let mut instances: Vec<_> = self
            .tasks
            .iter()
            .enumerate()
            .flat_map(|(index, task)| self.task_instances(index, task))
            .collect();

        instances.sort_by_key(|instance| {
            (
                Reverse(instance.task.priority),
                instance.slack,
                instance.end,
                instance.start,
            )
        });

        instances
    }

    fn task_instances(&self, index: usize, task: &'a Task) -> Vec<Instance<'a>> {
        let TaskKind::Flexible(flexible) = &task.kind else {
            return Vec::new();
        };

        match flexible.repeat {
            Repeat::Once {
                earliest_day,
                deadline_day,
            } => vec![self.instance(index, task, flexible, earliest_day..deadline_day + 1)],
            Repeat::Daily => (0..self.horizon)
                .filter(|day| self.occurs_on(task, *day))
                .map(|day| self.instance(index, task, flexible, day..day + 1))
                .collect(),
        }
    }

    fn instance(
        &self,
        index: usize,
        task: &'a Task,
        flexible: &'a Flexible,
        days: Range<i32>,
    ) -> Instance<'a> {
        Instance {
            index,
            task,
            flexible,
            start: days.start * Block::MINUTES_PER_DAY,
            end: days.end * Block::MINUTES_PER_DAY,
            slack: self.slack(task, flexible, days),
        }
    }

    fn slack(&self, task: &Task, flexible: &Flexible, days: Range<i32>) -> i32 {
        let window = (flexible.window.end - flexible.window.start).max(0);
        let open_days = days.filter(|day| self.occurs_on(task, *day)).count() as i32;
        let sessions = Self::chunk(flexible).len() as i32;

        open_days * window - flexible.total.max(1) - sessions * (task.prep + task.cleanup)
    }

    fn place_all(
        &mut self,
        instances: &[Instance],
        order: &[usize],
        fixed: usize,
    ) -> (Vec<Session>, Vec<bool>) {
        self.reservations.truncate(fixed);

        let mut sessions = Vec::new();
        let mut squeezed = vec![false; instances.len()];

        for index in order {
            let instance = &instances[*index];

            if instance.end > self.now {
                squeezed[*index] = self.place(instance, *index, &mut sessions);
            }
        }

        (sessions, squeezed)
    }

    fn place(&mut self, instance: &Instance, index: usize, sessions: &mut Vec<Session>) -> bool {
        let task = instance.task;
        let pieces = Self::chunk(instance.flexible);
        let spacing = Self::MIN_SESSION_GAP.max(task.prep + task.cleanup);
        let mut from = self.now.max(instance.start);
        let mut squeezed = false;

        for (position, work) in pieces.iter().enumerate() {
            let segments = Self::segments(task, *work, instance.flexible.breaks);
            let span = Self::span(&segments);
            let prefer = Self::prefer(instance, pieces.len(), position);

            if let Some((start, relax)) = self.find(instance, span, from, prefer) {
                self.reserve(start, span, task.priority);
                sessions.push(Session {
                    instance: index,
                    start,
                    work: *work,
                    segments,
                });
                squeezed |= relax > 0;
                from = start + span + spacing;
            }
        }

        squeezed
    }

    fn prefer(instance: &Instance, pieces: usize, position: usize) -> Option<i32> {
        if pieces < 2 {
            return None;
        }

        let anchor = instance.start + instance.flexible.window.start;
        let anchor_end = (anchor + 60).max(instance.end);
        let spread = (anchor_end - anchor) as f32 * position as f32 / pieces as f32;

        Some(Self::snap_nearest(anchor as f32 + spread))
    }

    fn find(
        &self,
        instance: &Instance,
        span: i32,
        from: i32,
        prefer: Option<i32>,
    ) -> Option<(i32, usize)> {
        for relax in 0..Self::RELAXATIONS.len() {
            let target = prefer
                .filter(|prefer| *prefer > from)
                .and_then(|prefer| self.scan(instance, span, prefer, relax));

            let start = target
                .or_else(|| self.nearest(instance, span, from, relax))
                .or_else(|| self.scan(instance, span, from, relax));

            if let Some(start) = start {
                return Some((start, relax));
            }
        }

        None
    }

    fn nearest(&self, instance: &Instance, span: i32, from: i32, relax: usize) -> Option<i32> {
        if relax != 1 && relax != 2 {
            return None;
        }

        let window = &instance.flexible.window;
        let anchor = self.now.max(from).max(instance.start);
        let midnight = Self::day_start(Self::day(anchor));
        let opens = midnight + window.start;
        let closes = midnight + window.end;
        let after = self.scan(instance, span, from.max(closes), relax);
        let before = (opens > anchor)
            .then(|| self.scan_back(instance, span, opens, relax))
            .flatten();

        match (before, after) {
            (Some(before), Some(after)) if opens - before - span <= after - closes => Some(before),
            (Some(before), None) => Some(before),
            (_, after) => after,
        }
    }

    fn scan(&self, instance: &Instance, span: i32, at: i32, relax: usize) -> Option<i32> {
        let relaxation = Self::RELAXATIONS[relax];
        let window = &instance.flexible.window;
        let end = self.limit(instance, relaxation);
        let mut cursor = Self::snap_up(at.max(instance.start).max(self.now));

        while cursor + span <= end {
            let day = Self::day(cursor);
            let minute = cursor - Self::day_start(day);

            if relaxation.days && !self.occurs_on(instance.task, day) {
                cursor = Self::day_start(day + 1);
                continue;
            }
            if relaxation.hours && minute < window.start {
                cursor = Self::day_start(day) + window.start;
                continue;
            }
            if relaxation.hours && minute + span > window.end {
                cursor = Self::day_start(day + 1) + window.start;
                continue;
            }

            match self.collision(
                cursor..cursor + span,
                instance.task.priority,
                relaxation.evict,
            ) {
                Some(hit) => cursor = Self::snap_up(hit.end),
                None => return Some(cursor),
            }
        }

        None
    }

    fn scan_back(&self, instance: &Instance, span: i32, by: i32, relax: usize) -> Option<i32> {
        let relaxation = Self::RELAXATIONS[relax];
        let window = &instance.flexible.window;
        let start = self.now.max(instance.start);
        let mut cursor = Self::snap_down(by.min(self.limit(instance, relaxation)) - span);

        while cursor >= start {
            let day = Self::day(cursor);
            let minute = cursor - Self::day_start(day);

            if relaxation.days && !self.occurs_on(instance.task, day) {
                cursor = Self::day_start(day) - span;
                continue;
            }
            if relaxation.hours && minute + span > window.end {
                cursor = Self::day_start(day) + window.end - span;
                continue;
            }
            if relaxation.hours && minute < window.start {
                cursor = Self::day_start(day - 1) + window.end - span;
                continue;
            }

            match self.collision(
                cursor..cursor + span,
                instance.task.priority,
                relaxation.evict,
            ) {
                Some(hit) => cursor = hit.start - span,
                None => return Some(cursor),
            }
        }

        None
    }

    fn collision(&self, at: Range<i32>, priority: Priority, evict: bool) -> Option<Reservation> {
        self.reservations.iter().copied().find(|reservation| {
            let overlaps = reservation.start < at.end && reservation.end > at.start;
            let evictable = evict && reservation.priority < priority && !reservation.started;

            overlaps && !evictable
        })
    }

    fn reserve(&mut self, start: i32, span: i32, priority: Priority) {
        self.reservations.push(Reservation {
            start,
            end: start + span,
            priority,
            started: start <= self.now,
        });
    }

    fn limit(&self, instance: &Instance, relaxation: Relaxation) -> i32 {
        if relaxation.deadline {
            instance.end
        } else {
            Self::day_start(self.horizon)
        }
    }

    fn chunk(flexible: &Flexible) -> Vec<i32> {
        let need = flexible.total.max(1);
        let Some(sessions) = flexible.sessions else {
            return vec![need];
        };

        let shortest = sessions.shortest.max(1);
        let preferred = sessions.preferred.max(5);
        let longest = sessions.longest.max(shortest);

        if need <= preferred.max(shortest) {
            return vec![need];
        }

        let mut count = (need as f32 / preferred as f32).round().max(1.0) as i32;
        while need as f32 / count as f32 > longest as f32 && count < 40 {
            count += 1;
        }
        while count > 1 && (need as f32 / count as f32) < shortest as f32 {
            count -= 1;
        }

        Self::split(need, count)
    }

    fn split(need: i32, count: i32) -> Vec<i32> {
        let mut left = need;

        (0..count)
            .map(|index| {
                let piece = if index == count - 1 {
                    left
                } else {
                    (need as f32 / count as f32).round() as i32
                };

                left -= piece;
                piece
            })
            .filter(|piece| *piece > 0)
            .collect()
    }

    fn segments(task: &Task, work: i32, breaks: Option<Breaks>) -> Vec<Segment> {
        let mut segments = Vec::new();

        if task.prep > 0 {
            segments.push(Self::segment(SegmentKind::Prep, task.prep));
        }

        segments.extend(Self::work_segments(work, breaks));

        if task.cleanup > 0 {
            segments.push(Self::segment(SegmentKind::Cleanup, task.cleanup));
        }

        segments
    }

    fn work_segments(work: i32, breaks: Option<Breaks>) -> Vec<Segment> {
        let Some(breaks) = breaks.filter(|breaks| breaks.every > 0 && work > breaks.every) else {
            return vec![Self::segment(SegmentKind::Work, work)];
        };

        let mut segments = Vec::new();
        let mut left = work;

        while left > 0 {
            let piece = breaks.every.min(left);
            segments.push(Self::segment(SegmentKind::Work, piece));
            left -= piece;

            if left > 0 {
                segments.push(Self::segment(SegmentKind::Pause, breaks.minutes));
            }
        }

        segments
    }

    fn segment(kind: SegmentKind, minutes: i32) -> Segment {
        Segment { kind, minutes }
    }

    fn span(segments: &[Segment]) -> i32 {
        segments.iter().map(|segment| segment.minutes).sum()
    }

    fn merge(instances: &[Instance], sessions: Vec<Session>) -> Vec<Session> {
        let mut sessions = sessions;
        sessions.sort_by_key(|session| session.start);

        let mut merged: Vec<Session> = Vec::new();

        for session in sessions {
            let previous = merged
                .iter_mut()
                .rev()
                .find(|other| other.instance == session.instance)
                .filter(|other| (other.end() - session.start).abs() < 2);

            match previous {
                Some(previous) => {
                    let instance = &instances[session.instance];
                    previous.work += session.work;
                    previous.segments =
                        Self::segments(instance.task, previous.work, instance.flexible.breaks);
                }
                None => merged.push(session),
            }
        }

        merged
    }

    fn block(index: usize, task: &Task, start: i32, segments: Vec<Segment>) -> Block {
        let block = Block::new(index, start, task.title.clone(), segments);

        match &task.place {
            Some(place) => block.at(place.clone()),
            None => block,
        }
    }

    fn occurs_on(&self, task: &Task, day: i32) -> bool {
        task.days.contains(&self.weekday(day))
    }

    fn weekday(&self, day: i32) -> Weekday {
        (0..day.rem_euclid(7)).fold(self.today, |weekday, _| weekday.succ())
    }

    fn day(minutes: i32) -> i32 {
        minutes.div_euclid(Block::MINUTES_PER_DAY)
    }

    fn day_start(day: i32) -> i32 {
        day * Block::MINUTES_PER_DAY
    }

    fn snap_nearest(minutes: f32) -> i32 {
        (minutes / Self::START_GRID as f32).round() as i32 * Self::START_GRID
    }

    fn snap_up(minutes: i32) -> i32 {
        (minutes + Self::START_GRID - 1).div_euclid(Self::START_GRID) * Self::START_GRID
    }

    fn snap_down(minutes: i32) -> i32 {
        minutes.div_euclid(Self::START_GRID) * Self::START_GRID
    }
}

#[derive(Clone, Copy)]
struct Reservation {
    start: i32,
    end: i32,
    priority: Priority,
    started: bool,
}

struct Instance<'a> {
    index: usize,
    task: &'a Task,
    flexible: &'a Flexible,
    start: i32,
    end: i32,
    slack: i32,
}

struct Session {
    instance: usize,
    start: i32,
    work: i32,
    segments: Vec<Segment>,
}

impl Session {
    fn block(self, instances: &[Instance]) -> Block {
        let instance = &instances[self.instance];

        Planner::block(instance.index, instance.task, self.start, self.segments)
    }

    fn end(&self) -> i32 {
        self.start + Planner::span(&self.segments)
    }
}

#[derive(Clone, Copy)]
struct Relaxation {
    hours: bool,
    days: bool,
    deadline: bool,
    evict: bool,
}

impl Relaxation {
    const fn new(hours: bool, days: bool, deadline: bool, evict: bool) -> Self {
        Self {
            hours,
            days,
            deadline,
            evict,
        }
    }
}
