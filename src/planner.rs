use std::cmp::Reverse;
use std::ops::Range;

use chrono::{Datelike, NaiveDate, TimeDelta, Weekday};

use crate::block::{Block, Segment, SegmentKind};
use crate::session::Session;
use crate::task::{Flexible, Priority, Recurrence, Repeat, Task, TaskId, TaskKind};

pub struct Planner<'a> {
    tasks: &'a [Task],
    log: &'a [Session],
    pin: Option<&'a Block>,
    today: NaiveDate,
    now: i32,
    horizon: i32,
    commitments: Vec<Commitment>,
}

impl<'a> Planner<'a> {
    const REACHES: [Reach; 3] = [
        Reach::new(false, false),
        Reach::new(true, false),
        Reach::new(true, true),
    ];
    const START_GRID: i32 = 5;
    const MOST_SESSIONS: i32 = 40;
    const MIN_SESSION_GAP: i32 = 20;
    const IDEAL_PAUSE: i32 = 5;
    const WORK_PER_BREAK: i32 = 5;
    const LONG_PAUSE: i32 = 10;
    const ELBOW_ROOM: i32 = 30;
    const UNBROKEN: i32 = 180;
    const JOIN: i32 = 15;
    const WAKING: i32 = 16 * 60;
    const COMFORTABLE_PERCENT: i32 = 55;
    const STRAIN: f32 = 4.0;
    const LOOSE: f32 = 0.55;
    const OVERDUE: f32 = 400.0;
    const OVERDUE_HOUR: f32 = 90.0;
    const FRAGMENT: f32 = 2.5;
    const FATIGUE: f32 = 25.0;
    const CROWD: f32 = 60.0;
    const HASTE: f32 = 12.0;
    const DELAY: f32 = 8.0;
    const SPREAD: f32 = 45.0;
    const CRAMPED: f32 = 4.0;

    pub fn plan(
        tasks: &'a [Task],
        log: &'a [Session],
        pin: Option<&'a Block>,
        horizon: i32,
        now: i32,
        today: NaiveDate,
    ) -> Vec<Block> {
        let mut planner = Self {
            tasks,
            log,
            pin,
            today,
            now,
            horizon: horizon + 1,
            commitments: Vec::new(),
        };

        let mut blocks = planner.fixed();
        blocks.extend(planner.hold());
        planner.settled();

        let demands = planner.demands();

        for index in 0..demands.len() {
            blocks.extend(planner.serve(&demands[index], &demands[index + 1..]));
        }

        blocks.retain(|block| block.start < Self::day_start(horizon));
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
            let reach =
                Self::span(&Self::segments(task, span)).div_euclid(Block::MINUTES_PER_DAY) + 1;

            for day in -reach..self.horizon {
                if !self.places_on(task, recurrence, day) {
                    continue;
                }

                let start = day * Block::MINUTES_PER_DAY + start - task.prep;
                let segments = self.cut(task, start, Self::segments(task, span));

                self.commit(task.id, start, Self::span(&segments), task.priority);
                blocks.push(Self::block(task, start, segments));

                if matches!(recurrence, Recurrence::Never) {
                    break;
                }
            }
        }

        blocks
    }

    fn hold(&mut self) -> Option<Block> {
        let pin = self.pin?;
        let task = self.tasks.iter().find(|task| task.id == pin.task)?;

        if !matches!(task.kind, TaskKind::Flexible(_)) {
            return None;
        }

        self.commit(task.id, pin.start, pin.span(), task.priority);

        Some(pin.clone())
    }

    fn settled(&mut self) {
        let spent: Vec<_> = self
            .log
            .iter()
            .filter(|session| session.happened() && session.end > session.start)
            .filter_map(|session| {
                let task = self.tasks.iter().find(|task| task.id == session.task)?;

                Some((
                    task.id,
                    session.start,
                    session.end - session.start,
                    task.priority,
                ))
            })
            .collect();

        for (task, start, span, priority) in spent {
            self.commit(task, start, span, priority);
        }
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

    fn demands(&self) -> Vec<Demand<'a>> {
        let mut demands: Vec<_> = self
            .tasks
            .iter()
            .flat_map(|task| self.task_demands(task))
            .filter(|demand| demand.run.end > self.now)
            .collect();

        demands.sort_by_key(|demand| {
            (
                Reverse(demand.task.priority),
                demand.slack,
                demand.run.end,
                demand.run.start,
            )
        });

        demands
    }

    fn task_demands(&self, task: &'a Task) -> Vec<Demand<'a>> {
        let TaskKind::Flexible(flexible) = &task.kind else {
            return Vec::new();
        };

        match flexible.repeat {
            Repeat::Never => vec![self.demand(task, flexible, task.dates.span())],
            Repeat::Daily => (0..self.horizon)
                .filter(|day| self.occurs_on(task, *day) && task.dates.covers(*day))
                .map(|day| self.demand(task, flexible, day..day + 1))
                .collect(),
            repeat => {
                let cycle = repeat.cycle();

                (self.opens(cycle)..self.horizon)
                    .step_by(cycle as usize)
                    .filter(|day| (*day..day + cycle).any(|each| task.dates.covers(each)))
                    .map(|day| self.demand(task, flexible, day..day + cycle))
                    .collect()
            }
        }
    }

    fn demand(&self, task: &'a Task, flexible: &'a Flexible, days: Range<i32>) -> Demand<'a> {
        let run = days.start * Block::MINUTES_PER_DAY..days.end * Block::MINUTES_PER_DAY;
        let open = days.filter(|day| self.occurs_on(task, *day)).count().max(1) as i32;
        let held = self.held(&run, task).map_or(0, Block::work);
        let need = flexible.total - self.logged(&run, task) - held;
        let window = (flexible.window.end - flexible.window.start).max(0);
        let spans: Vec<i32> = Self::chunk(flexible, flexible.total)
            .iter()
            .map(|work| Self::span(&Self::segments(task, *work)))
            .collect();

        Demand {
            slack: open * window - spans.iter().sum::<i32>(),
            earliest: self.earliest(&run, task),
            want: (spans.iter().sum::<i32>() + open - 1) / open,
            piece: spans.iter().copied().max().unwrap_or(0),
            need,
            open,
            task,
            flexible,
            run,
        }
    }

    fn held(&self, run: &Range<i32>, task: &Task) -> Option<&'a Block> {
        self.pin
            .filter(|pin| pin.task == task.id && run.contains(&pin.start))
    }

    fn logged(&self, run: &Range<i32>, task: &Task) -> i32 {
        self.log
            .iter()
            .filter(|session| session.within(task.id, run))
            .map(Session::credited)
            .sum()
    }

    fn earliest(&self, run: &Range<i32>, task: &Task) -> i32 {
        self.log
            .iter()
            .filter(|session| session.within(task.id, run) && session.happened())
            .map(|session| session.end)
            .chain(self.held(run, task).map(Block::end))
            .map(|end| end + Self::MIN_SESSION_GAP)
            .chain([self.now, run.start])
            .max()
            .unwrap_or(self.now)
    }

    fn serve(&mut self, demand: &Demand, pending: &[Demand]) -> Vec<Block> {
        if demand.need <= 0 {
            return Vec::new();
        }

        let mut earliest = demand.earliest;
        let mut used = self.busy_days(demand);
        let mut blocks = Vec::new();

        for work in Self::chunk(demand.flexible, demand.need) {
            let segments = Self::segments(demand.task, work);
            let span = Self::span(&segments);
            let Some(at) = self.choose(demand, pending, span, work, earliest, &used) else {
                continue;
            };
            let conflict = self.overlapped(&at);

            self.commit(demand.task.id, at.start, span, demand.task.priority);
            used.push(Self::day(at.start));
            earliest = at.end + Self::MIN_SESSION_GAP;
            blocks.push(Self::block(demand.task, at.start, segments).conflicting(conflict));
        }

        blocks
    }

    fn busy_days(&self, demand: &Demand) -> Vec<i32> {
        self.commitments
            .iter()
            .filter(|taken| taken.task == demand.task.id && demand.run.contains(&taken.start))
            .map(|taken| Self::day(taken.start))
            .collect()
    }

    fn choose(
        &self,
        demand: &Demand,
        pending: &[Demand],
        span: i32,
        work: i32,
        earliest: i32,
        used: &[i32],
    ) -> Option<Range<i32>> {
        for reach in Self::REACHES {
            let mut best: Option<(f32, Range<i32>)> = None;

            for day in Self::day(earliest)..self.limit(demand, reach) {
                let view = self.view(demand, pending, day, span, earliest, reach);

                for start in &view.starts {
                    let at = *start..*start + span;
                    let cost = self.cost(demand, &view, &at, work, used);

                    if best.as_ref().is_none_or(|(lowest, _)| cost < *lowest) {
                        best = Some((cost, at));
                    }
                }
            }

            if let Some((_, at)) = best {
                return Some(at);
            }
        }

        None
    }

    fn view(
        &self,
        demand: &Demand,
        pending: &[Demand],
        day: i32,
        span: i32,
        earliest: i32,
        reach: Reach,
    ) -> DayView {
        let midnight = Self::day_start(day);
        let closes = midnight + Block::MINUTES_PER_DAY;
        let bounds = midnight.max(earliest)..(closes + span).min(Self::day_start(self.horizon));
        let open = self.free(
            bounds.clone(),
            reach.outrank.then_some(demand.task.priority),
        );
        let busy = self.runs(day, 0);
        let rivals: Vec<_> = pending
            .iter()
            .filter(|other| other.run.contains(&midnight) && self.occurs_on(other.task, day))
            .map(|other| self.rival(other, midnight))
            .collect();

        DayView {
            starts: Self::starts(&open, &bounds, span, closes),
            runs: Self::joined(&busy, Self::JOIN),
            load: self.booked(day, &rivals),
            rivals,
            busy,
            day,
        }
    }

    fn booked(&self, day: i32, rivals: &[Rival]) -> i32 {
        let midnight = Self::day_start(day);
        let closes = midnight + Block::MINUTES_PER_DAY;
        let mut spans: Vec<_> = self
            .commitments
            .iter()
            .filter(|taken| !rivals.iter().any(|rival| rival.task == taken.task))
            .map(|taken| taken.start.max(midnight)..taken.end.min(closes))
            .filter(|span| span.end > span.start)
            .collect();

        spans.sort_unstable_by_key(|span| span.start);

        Self::joined(&spans, 0)
            .iter()
            .map(|run| run.end - run.start)
            .sum::<i32>()
            + rivals.iter().map(|rival| rival.want).sum::<i32>()
    }

    fn starts(open: &[Range<i32>], bounds: &Range<i32>, span: i32, closes: i32) -> Vec<i32> {
        let mut starts = Vec::new();

        for space in open {
            let last = space.end - span;
            let mut cursor = Self::snap_up(space.start);

            while cursor <= last {
                starts.push(cursor);
                cursor += Self::START_GRID;
            }

            if last >= space.start && space.start > bounds.start {
                starts.push(space.start);
            }

            if last >= space.start && space.end < bounds.end {
                starts.push(last);
            }
        }

        starts.retain(|start| *start < closes);
        starts.sort_unstable();
        starts.dedup();

        starts
    }

    fn rival(&self, other: &Demand, midnight: i32) -> Rival {
        let window = midnight + other.flexible.window.start..midnight + other.flexible.window.end;
        let open = self.room(window.clone(), other.task.id);
        let comfort = other.piece + Self::ELBOW_ROOM;

        Rival {
            task: other.task.id,
            weight: Self::weight(other.task.priority),
            eased: Rival::pinch(other.want, comfort, &open),
            want: other.want,
            comfort,
            window,
            open,
        }
    }

    fn joined(spans: &[Range<i32>], join: i32) -> Vec<Range<i32>> {
        spans.iter().cloned().fold(Vec::new(), |mut runs, span| {
            match runs.last_mut() {
                Some(last) if span.start <= last.end + join => last.end = last.end.max(span.end),
                _ => runs.push(span),
            }

            runs
        })
    }

    fn limit(&self, demand: &Demand, reach: Reach) -> i32 {
        match reach.beyond && !demand.task.repeats() {
            true => self.horizon,
            false => (Self::day(demand.run.end - 1) + 1).min(self.horizon),
        }
    }

    fn cost(
        &self,
        demand: &Demand,
        view: &DayView,
        at: &Range<i32>,
        work: i32,
        used: &[i32],
    ) -> f32 {
        let weight = Self::weight(demand.task.priority);

        weight
            * (self.strain(demand, at)
                + Self::overdue(demand, at)
                + Self::haste(demand, at, work)
                + Self::delay(demand, at, work) * (weight - 1.0).max(0.0)
                + Self::spread(demand, at, used))
            + Self::fragment(view, at)
            + Self::fatigue(view, at)
            + Self::crowd(view, at)
            + view
                .rivals
                .iter()
                .map(|rival| rival.squeeze(at))
                .sum::<f32>()
    }

    fn strain(&self, demand: &Demand, at: &Range<i32>) -> f32 {
        let day = Self::day(at.start);
        let midnight = Self::day_start(day);
        let window = &demand.flexible.window;
        let early = (midnight + window.start - at.start).max(0);
        let late = (at.end - midnight - window.end).max(0);
        let off = !self.occurs_on(demand.task, day);
        let strayed = (early + late) as f32 + f32::from(off) * (at.end - at.start) as f32;

        Self::STRAIN * Self::tolerance(demand.task) * strayed
    }

    fn overdue(demand: &Demand, at: &Range<i32>) -> f32 {
        let late = (at.end - demand.run.end).max(0);

        match late > 0 && !demand.task.repeats() {
            true => Self::OVERDUE + Self::OVERDUE_HOUR * late as f32 / 60.0,
            false => 0.0,
        }
    }

    fn haste(demand: &Demand, at: &Range<i32>, work: i32) -> f32 {
        if demand.run.end - demand.run.start <= Block::MINUTES_PER_DAY {
            return 0.0;
        }

        let waited = (at.start - demand.run.start) as f32 / Block::MINUTES_PER_DAY as f32;

        Self::HASTE * waited * work as f32 / 60.0
    }

    fn delay(demand: &Demand, at: &Range<i32>, work: i32) -> f32 {
        let window = &demand.flexible.window;
        let opens = Self::day_start(Self::day(at.start)) + window.start;
        let room = (window.end - window.start - (at.end - at.start)).max(1);
        let reached = ((at.start - opens) as f32 / room as f32).clamp(0.0, 1.0);

        Self::DELAY * reached * work as f32 / 60.0
    }

    fn spread(demand: &Demand, at: &Range<i32>, used: &[i32]) -> f32 {
        let day = Self::day(at.start);
        let stacked = used.iter().filter(|other| **other == day).count();

        match demand.open > 1 {
            true => Self::SPREAD * stacked as f32,
            false => 0.0,
        }
    }

    fn fragment(view: &DayView, at: &Range<i32>) -> f32 {
        let before = view
            .busy
            .iter()
            .filter(|run| run.end <= at.start)
            .map(|run| run.end)
            .max();
        let after = view
            .busy
            .iter()
            .filter(|run| run.start >= at.end)
            .map(|run| run.start)
            .min();
        let split = Self::dead(before.map(|end| at.start - end))
            + Self::dead(after.map(|start| start - at.end));
        let whole = match (before, after) {
            (Some(before), Some(after)) => Self::dead(Some(after - before)),
            _ => 0.0,
        };

        split - whole
    }

    fn dead(gap: Option<i32>) -> f32 {
        match gap {
            Some(gap) if gap > 0 && gap < Self::LONG_PAUSE => Self::FRAGMENT * gap as f32,
            _ => 0.0,
        }
    }

    fn fatigue(view: &DayView, at: &Range<i32>) -> f32 {
        let midnight = Self::day_start(view.day);
        let piece = at.start..at.end.min(midnight + Block::MINUTES_PER_DAY);
        let touching: Vec<_> = view
            .runs
            .iter()
            .filter(|run| {
                run.start <= piece.end + Self::JOIN && run.end + Self::JOIN >= piece.start
            })
            .cloned()
            .collect();
        let joined = touching
            .iter()
            .map(|run| run.start)
            .chain([piece.start])
            .min()
            .unwrap_or(piece.start)
            ..touching
                .iter()
                .map(|run| run.end)
                .chain([piece.end])
                .max()
                .unwrap_or(piece.end);

        view.strained(&joined) - touching.iter().map(|run| view.strained(run)).sum::<f32>()
    }

    fn unbroken(length: i32) -> f32 {
        let over = (length - Self::UNBROKEN).max(0) as f32 / 60.0;

        Self::FATIGUE * over.powi(2)
    }

    fn crowd(view: &DayView, at: &Range<i32>) -> f32 {
        let added = at
            .end
            .min(Self::day_start(view.day) + Block::MINUTES_PER_DAY)
            - at.start;

        Self::packed(view.load + added) - Self::packed(view.load)
    }

    fn packed(load: i32) -> f32 {
        let comfortable = Self::WAKING * Self::COMFORTABLE_PERCENT / 100;
        let over = (load - comfortable).max(0) as f32 / 60.0;

        Self::CROWD * over.powi(2)
    }

    fn without(open: &[Range<i32>], at: &Range<i32>) -> Vec<Range<i32>> {
        open.iter()
            .flat_map(|span| {
                [
                    span.start..span.end.min(at.start),
                    span.start.max(at.end)..span.end,
                ]
            })
            .filter(|span| span.end > span.start)
            .collect()
    }

    fn chunk(flexible: &Flexible, need: i32) -> Vec<i32> {
        let need = need.clamp(1, Self::MOST_SESSIONS * Block::MINUTES_PER_DAY);
        let Some(sessions) = flexible.sessions else {
            return vec![need];
        };

        let shortest = sessions.shortest.clamp(1, Block::MINUTES_PER_DAY);
        let preferred = sessions
            .preferred
            .clamp(Self::START_GRID, Block::MINUTES_PER_DAY);
        let longest = sessions.longest.clamp(shortest, Block::MINUTES_PER_DAY);

        if need <= preferred.max(shortest) {
            return vec![need];
        }

        let mut count =
            ((need as f32 / preferred as f32).round() as i32).clamp(1, Self::MOST_SESSIONS);

        while need > count * longest && count < Self::MOST_SESSIONS {
            count += 1;
        }
        while count > 1 && need < count * shortest {
            count -= 1;
        }

        Self::split(need, count)
    }

    fn split(need: i32, count: i32) -> Vec<i32> {
        let even = need.div_euclid(count);
        let spare = need.rem_euclid(count);

        (0..count)
            .map(|index| even + i32::from(index < spare))
            .filter(|piece| *piece > 0)
            .collect()
    }

    pub fn shape(task: &Task, work: i32, span: i32) -> Vec<Segment> {
        let segments = Self::segments(task, work);
        let short = span - Self::span(&segments);

        match short > 0 {
            true => Self::padded(segments, short),
            false => Self::fitted(segments, span),
        }
    }

    fn padded(segments: Vec<Segment>, extra: i32) -> Vec<Segment> {
        let mut segments = segments;
        let worked = segments
            .iter()
            .rposition(|segment| segment.kind == SegmentKind::Work);

        match worked {
            Some(index) => segments[index].minutes += extra,
            None => {
                let before = segments
                    .iter()
                    .position(|segment| segment.kind == SegmentKind::Cleanup)
                    .unwrap_or(segments.len());

                segments.insert(before, Self::segment(SegmentKind::Work, extra));
            }
        }

        segments
    }

    fn segments(task: &Task, work: i32) -> Vec<Segment> {
        let mut segments = Vec::new();

        if task.prep > 0 {
            segments.push(Self::segment(SegmentKind::Prep, task.prep));
        }

        segments.extend(Self::work_segments(
            work.min(Block::MINUTES_PER_DAY),
            task.splittable(),
        ));

        if task.cleanup > 0 {
            segments.push(Self::segment(SegmentKind::Cleanup, task.cleanup));
        }

        Self::fitted(segments, Block::MINUTES_PER_DAY)
    }

    fn fitted(segments: Vec<Segment>, limit: i32) -> Vec<Segment> {
        segments
            .into_iter()
            .scan(limit, |left, segment| {
                let minutes = segment.minutes.clamp(0, *left);
                *left -= minutes;

                Some(Self::segment(segment.kind, minutes))
            })
            .filter(|segment| segment.minutes > 0)
            .collect()
    }

    fn work_segments(work: i32, splittable: bool) -> Vec<Segment> {
        let unbroken = Self::IDEAL_PAUSE * Self::WORK_PER_BREAK;

        if !splittable || work <= unbroken {
            return vec![Self::segment(SegmentKind::Work, work)];
        }

        let pieces = (work + unbroken - 1) / unbroken;
        let mut segments = Vec::new();

        for (index, piece) in Self::split(work, pieces).into_iter().enumerate() {
            if index > 0 {
                segments.push(Self::segment(SegmentKind::Pause, Self::IDEAL_PAUSE));
            }

            segments.push(Self::segment(SegmentKind::Work, piece));
        }

        segments
    }

    fn segment(kind: SegmentKind, minutes: i32) -> Segment {
        Segment { kind, minutes }
    }

    fn span(segments: &[Segment]) -> i32 {
        segments.iter().map(|segment| segment.minutes).sum()
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
        if !task.dates.covers(day) {
            return false;
        }

        match recurrence {
            Recurrence::Never => day == task.dates.first(),
            Recurrence::Weekly => self.occurs_on(task, day),
            Recurrence::Biweekly => self.occurs_on(task, day) && self.week(day) % 2 == 0,
        }
    }

    fn week(&self, day: i32) -> i32 {
        (self.today.num_days_from_ce() + day).div_euclid(7)
    }

    fn weekday(&self, day: i32) -> Weekday {
        (self.today + TimeDelta::days(day as i64)).weekday()
    }

    fn opens(&self, cycle: i32) -> i32 {
        -self.today.num_days_from_ce().rem_euclid(cycle)
    }

    fn tolerance(task: &Task) -> f32 {
        match task.repeats() {
            true => Self::LOOSE,
            false => 1.0,
        }
    }

    fn weight(priority: Priority) -> f32 {
        match priority {
            Priority::Lowest => 0.5,
            Priority::Low => 0.7,
            Priority::Normal => 1.0,
            Priority::High => 1.5,
            Priority::Highest => 2.2,
        }
    }

    fn day(minutes: i32) -> i32 {
        minutes.div_euclid(Block::MINUTES_PER_DAY)
    }

    fn day_start(day: i32) -> i32 {
        day * Block::MINUTES_PER_DAY
    }

    fn snap_up(minutes: i32) -> i32 {
        (minutes + Self::START_GRID - 1).div_euclid(Self::START_GRID) * Self::START_GRID
    }

    fn free(&self, bounds: Range<i32>, outranked: Option<Priority>) -> Vec<Range<i32>> {
        self.open(bounds, |taken| {
            outranked.is_none_or(|priority| taken.priority >= priority)
        })
    }

    fn room(&self, bounds: Range<i32>, task: TaskId) -> Vec<Range<i32>> {
        self.open(bounds, |taken| taken.task != task)
    }

    fn open(&self, bounds: Range<i32>, blocking: impl Fn(&Commitment) -> bool) -> Vec<Range<i32>> {
        let mut blocked: Vec<_> = self
            .commitments
            .iter()
            .filter(|taken| taken.end > bounds.start && taken.start < bounds.end)
            .filter(|taken| blocking(taken))
            .map(|taken| taken.start..taken.end)
            .collect();

        blocked.sort_unstable_by_key(|taken| taken.start);

        let mut open = Vec::new();
        let mut cursor = bounds.start;

        for taken in blocked {
            if taken.start > cursor {
                open.push(cursor..taken.start);
            }

            cursor = cursor.max(taken.end);
        }

        if cursor < bounds.end {
            open.push(cursor..bounds.end);
        }

        open
    }

    fn commit(&mut self, task: TaskId, start: i32, span: i32, priority: Priority) {
        self.commitments.push(Commitment {
            task,
            start,
            end: start + span,
            priority,
        });
    }

    fn overlapped(&self, at: &Range<i32>) -> bool {
        self.commitments
            .iter()
            .any(|taken| taken.start < at.end && taken.end > at.start)
    }

    fn runs(&self, day: i32, join: i32) -> Vec<Range<i32>> {
        let midnight = Self::day_start(day);
        let closes = midnight + Block::MINUTES_PER_DAY;
        let mut spans: Vec<_> = self
            .commitments
            .iter()
            .map(|taken| taken.start.max(midnight)..taken.end.min(closes))
            .filter(|span| span.end > span.start)
            .collect();

        spans.sort_unstable_by_key(|span| span.start);

        spans.into_iter().fold(Vec::new(), |mut runs, span| {
            match runs.last_mut() {
                Some(last) if span.start <= last.end + join => last.end = last.end.max(span.end),
                _ => runs.push(span),
            }

            runs
        })
    }
}

struct Commitment {
    task: TaskId,
    start: i32,
    end: i32,
    priority: Priority,
}

struct DayView {
    day: i32,
    starts: Vec<i32>,
    busy: Vec<Range<i32>>,
    runs: Vec<Range<i32>>,
    load: i32,
    rivals: Vec<Rival>,
}

impl DayView {
    fn strained(&self, run: &Range<i32>) -> f32 {
        let growth: i32 = self.rivals.iter().map(|rival| rival.likely(run)).sum();

        Planner::unbroken(run.end - run.start + growth)
    }
}

struct Rival {
    task: TaskId,
    weight: f32,
    want: i32,
    comfort: i32,
    window: Range<i32>,
    open: Vec<Range<i32>>,
    eased: f32,
}

impl Rival {
    fn squeeze(&self, at: &Range<i32>) -> f32 {
        if at.end.min(self.window.end) <= at.start.max(self.window.start) {
            return 0.0;
        }

        let left = Planner::without(&self.open, at);

        Planner::CRAMPED * self.weight * (Self::pinch(self.want, self.comfort, &left) - self.eased)
    }

    fn likely(&self, run: &Range<i32>) -> i32 {
        let near = run.start - Planner::JOIN..run.end + Planner::JOIN;
        let shared = self.window.end.min(near.end) - self.window.start.max(near.start);
        let window = (self.window.end - self.window.start).max(1);

        self.want * shared.max(0) / window
    }

    fn pinch(want: i32, comfort: i32, open: &[Range<i32>]) -> f32 {
        let (total, best) = open
            .iter()
            .map(|span| span.end - span.start)
            .fold((0, 0), |(total, best), length| {
                (total + length, best.max(length))
            });

        ((want - total).max(0) + (comfort - best).max(0)) as f32
    }
}

struct Demand<'a> {
    task: &'a Task,
    flexible: &'a Flexible,
    run: Range<i32>,
    need: i32,
    earliest: i32,
    slack: i32,
    open: i32,
    want: i32,
    piece: i32,
}

#[derive(Clone, Copy)]
struct Reach {
    beyond: bool,
    outrank: bool,
}

impl Reach {
    const fn new(beyond: bool, outrank: bool) -> Self {
        Self { beyond, outrank }
    }
}
