use std::cmp::Reverse;
use std::ops::Range;

use chrono::Weekday;

use crate::block::{Block, Segment, SegmentKind};
use crate::session::Session;
use crate::task::{Breaks, Flexible, Priority, Recurrence, Repeat, Task, TaskKind};

pub struct Planner<'a> {
    tasks: &'a [Task],
    log: &'a [Session],
    pin: Option<&'a Block>,
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

    pub fn plan(
        tasks: &'a [Task],
        log: &'a [Session],
        pin: Option<&'a Block>,
        horizon: i32,
        now: i32,
        today: Weekday,
    ) -> Vec<Block> {
        let mut planner = Self {
            tasks,
            log,
            pin,
            today,
            now,
            horizon,
            reservations: Vec::new(),
        };

        let mut blocks = planner.fixed();
        blocks.extend(planner.hold());
        let fixed = planner.reservations.len();
        let instances = planner.instances();
        let mut order: Vec<_> = (0..instances.len()).collect();
        let (mut placements, squeezed) = planner.place_all(&instances, &order, fixed);

        if squeezed.contains(&true) {
            order.sort_by_key(|instance| !squeezed[*instance]);
            placements = planner.place_all(&instances, &order, fixed).0;
        }

        blocks.extend(
            Self::merge(&instances, placements)
                .into_iter()
                .map(|session| session.block(&instances)),
        );
        blocks.sort_by_key(|block| block.start);

        blocks
    }

    fn fixed(&mut self) -> Vec<Block> {
        let tasks = self.tasks;
        let mut blocks = Vec::new();

        for task in tasks {
            let TaskKind::Fixed {
                start,
                duration,
                recurrence,
                overrun_percent,
            } = task.kind
            else {
                continue;
            };
            let span = Self::allowance(duration, overrun_percent);

            for day in 0..self.horizon {
                if !self.places_on(task, recurrence, day) {
                    continue;
                }

                let block_start = day * Block::MINUTES_PER_DAY + start - task.prep;
                let segments = self.cut(task, block_start, Self::segments(task, span, None));
                self.reserve(block_start, Self::span(&segments), task.priority);
                blocks.push(Self::block(task, block_start, segments));

                if matches!(recurrence, Recurrence::Once { .. }) {
                    break;
                }
            }
        }

        blocks
    }

    fn hold(&mut self) -> Option<Block> {
        let pin = self.pin?;
        let priority = self.tasks.iter().find(|task| task.id == pin.task)?.priority;

        self.reserve(pin.start, pin.span(), priority);

        Some(pin.clone())
    }

    fn held(&self, instance: &Instance) -> Option<&Block> {
        self.pin.filter(|pin| {
            pin.task == instance.task.id && pin.start >= instance.start && pin.start < instance.end
        })
    }

    fn cut(&self, task: &Task, start: i32, segments: Vec<Segment>) -> Vec<Segment> {
        let span = Self::span(&segments);
        let ended = self
            .log
            .iter()
            .find(|session| session.task == task.id && session.start == start)
            .map(|session| session.end)
            .filter(|ended| *ended > start && *ended < start + span);

        match ended {
            Some(ended) => vec![Self::segment(SegmentKind::Work, ended - start)],
            None => segments,
        }
    }

    fn instances(&self) -> Vec<Instance<'a>> {
        let mut instances: Vec<_> = self
            .tasks
            .iter()
            .flat_map(|task| self.task_instances(task))
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

    fn task_instances(&self, task: &'a Task) -> Vec<Instance<'a>> {
        let TaskKind::Flexible(flexible) = &task.kind else {
            return Vec::new();
        };

        match flexible.repeat {
            Repeat::Once {
                earliest_day,
                deadline_day,
            } => vec![self.instance(task, flexible, earliest_day..deadline_day + 1)],
            Repeat::Daily => (0..self.horizon)
                .filter(|day| self.occurs_on(task, *day))
                .map(|day| self.instance(task, flexible, day..day + 1))
                .collect(),
            repeat => {
                let cycle = repeat.cycle();

                (0..self.horizon)
                    .step_by(cycle as usize)
                    .map(|day| self.instance(task, flexible, day..day + cycle))
                    .collect()
            }
        }
    }

    fn instance(&self, task: &'a Task, flexible: &'a Flexible, days: Range<i32>) -> Instance<'a> {
        Instance {
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
        let sessions = Self::chunk(flexible, flexible.total).len() as i32;

        open_days * window - flexible.total.max(1) - sessions * (task.prep + task.cleanup)
    }

    fn place_all(
        &mut self,
        instances: &[Instance],
        order: &[usize],
        fixed: usize,
    ) -> (Vec<Placement>, Vec<bool>) {
        self.reservations.truncate(fixed);

        let mut placements = Vec::new();
        let mut squeezed = vec![false; instances.len()];

        for index in order {
            let instance = &instances[*index];

            if instance.end > self.now {
                squeezed[*index] = self.place(instance, *index, &mut placements);
            }
        }

        (placements, squeezed)
    }

    fn place(
        &mut self,
        instance: &Instance,
        index: usize,
        placements: &mut Vec<Placement>,
    ) -> bool {
        let task = instance.task;
        let held = self.held(instance).map_or(0, Block::work);
        let need = instance.flexible.total - self.logged(instance) - held;
        if need <= 0 {
            return false;
        }

        let pieces = Self::chunk(instance.flexible, need);
        let slots = Self::chunk(instance.flexible, instance.flexible.total).len();
        let spacing = Self::MIN_SESSION_GAP.max(task.prep + task.cleanup);
        let mut from = self.resume(instance, spacing);
        let mut squeezed = false;

        for (position, work) in pieces.iter().enumerate() {
            let segments = Self::segments(task, *work, instance.flexible.breaks);
            let span = Self::span(&segments);
            let prefer = Self::prefer(instance, slots, slots - pieces.len() + position);

            if let Some((start, relax)) = self.find(instance, span, from, prefer) {
                let conflict = self
                    .collision(start..start + span, task.priority, false)
                    .is_some();

                self.reserve(start, span, task.priority);
                placements.push(Placement {
                    instance: index,
                    start,
                    work: *work,
                    segments,
                    conflict,
                });
                squeezed |= relax > 0;
                from = start + span + spacing;
            }
        }

        squeezed
    }

    fn prefer(instance: &Instance, slots: usize, position: usize) -> Option<i32> {
        if slots < 2 {
            return None;
        }

        let anchor = instance.start + instance.flexible.window.start;
        let anchor_end = (anchor + 60).max(instance.end);
        let spread = (anchor_end - anchor) as f32 * position as f32 / slots as f32;

        Some(Self::snap_nearest(anchor as f32 + spread))
    }

    fn logged(&self, instance: &Instance) -> i32 {
        self.log
            .iter()
            .filter(|session| session.within(instance.task.id, &(instance.start..instance.end)))
            .map(Session::credited)
            .sum()
    }

    fn resume(&self, instance: &Instance, spacing: i32) -> i32 {
        let settled = self
            .log
            .iter()
            .filter(|session| session.within(instance.task.id, &(instance.start..instance.end)))
            .filter(|session| session.happened())
            .map(|session| session.end + spacing)
            .chain(self.held(instance).map(|pin| pin.end() + spacing))
            .max();

        settled
            .into_iter()
            .chain([self.now, instance.start])
            .max()
            .unwrap_or(self.now)
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

    fn chunk(flexible: &Flexible, need: i32) -> Vec<i32> {
        let need = need.max(1);
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

    fn merge(instances: &[Instance], placements: Vec<Placement>) -> Vec<Placement> {
        let mut placements = placements;
        placements.sort_by_key(|session| session.start);

        let mut merged: Vec<Placement> = Vec::new();

        for session in placements {
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

    fn block(task: &Task, start: i32, segments: Vec<Segment>) -> Block {
        let block = Block::new(task.id, start, task.title.clone(), segments);

        match &task.place {
            Some(place) => block.at(place.clone()),
            None => block,
        }
    }

    fn occurs_on(&self, task: &Task, day: i32) -> bool {
        task.days.contains(&self.weekday(day))
    }

    fn allowance(duration: i32, percent: i32) -> i32 {
        let span = duration as f32 * (1.0 + percent.max(0) as f32 / 100.0);

        (span.round() as i32).max(1)
    }

    fn places_on(&self, task: &Task, recurrence: Recurrence, day: i32) -> bool {
        match recurrence {
            Recurrence::Once {
                earliest_day,
                deadline_day,
            } => (earliest_day..=deadline_day).contains(&day),
            Recurrence::Weekly => self.occurs_on(task, day),
            Recurrence::Biweekly => self.occurs_on(task, day) && self.week(day) % 2 == 0,
        }
    }

    fn week(&self, day: i32) -> i32 {
        (day + self.today.num_days_from_sunday() as i32).div_euclid(7)
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
    task: &'a Task,
    flexible: &'a Flexible,
    start: i32,
    end: i32,
    slack: i32,
}

struct Placement {
    instance: usize,
    start: i32,
    work: i32,
    segments: Vec<Segment>,
    conflict: bool,
}

impl Placement {
    fn block(self, instances: &[Instance]) -> Block {
        let instance = &instances[self.instance];

        Planner::block(instance.task, self.start, self.segments).conflicting(self.conflict)
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
