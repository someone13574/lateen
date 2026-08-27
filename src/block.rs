use std::f32::consts::SQRT_2;
use std::ops::Range;

use gpui::prelude::*;
use gpui::{
    App, Bounds, BoxShadow, Div, ElementId, FontWeight, Pixels, Rgba, Role, SharedString, Stateful,
    Text, Window, div, pattern_slash, px, rgb,
};

use serde::{Deserialize, Serialize};

use crate::button::ClickHandler;
use crate::clock::ClockFormat;
use crate::session::{Outcome, Session};
use crate::task::{Task, TaskId};
use crate::theme::{ActiveTheme, BlockColor, BlockColors};
use crate::tooltip::{TooltipBuilder, Tooltipped};

#[derive(Clone, Serialize, Deserialize)]
pub struct Block {
    pub task: TaskId,
    pub title: SharedString,
    pub place: Option<SharedString>,
    pub color: Option<BlockColor>,
    pub start: i32,
    pub segments: Vec<Segment>,
    pub outcome: Option<Outcome>,
    pub conflict: bool,
    #[serde(default)]
    pub all_day: bool,
}

impl Block {
    pub const MINUTES_PER_DAY: i32 = 24 * 60;

    pub fn new(
        task: TaskId,
        start: i32,
        title: impl Into<SharedString>,
        segments: Vec<Segment>,
    ) -> Self {
        Self {
            task,
            title: title.into(),
            place: None,
            color: None,
            start,
            segments,
            outcome: None,
            conflict: false,
            all_day: false,
        }
    }

    pub fn conflicting(mut self, conflict: bool) -> Self {
        self.conflict = conflict;
        self
    }

    pub fn all_day(mut self, all_day: bool) -> Self {
        self.all_day = all_day;
        self
    }

    pub fn logged(task: &Task, session: &Session, segments: Vec<Segment>) -> Self {
        Self {
            task: task.id,
            title: task.title.clone(),
            place: task.place.clone(),
            color: task.color,
            start: session.start,
            segments,
            outcome: Some(session.outcome),
            conflict: false,
            all_day: false,
        }
    }

    pub fn at(mut self, place: impl Into<SharedString>) -> Self {
        self.place = Some(place.into());
        self
    }

    pub fn span(&self) -> i32 {
        self.segments.iter().map(|segment| segment.minutes).sum()
    }

    pub fn end(&self) -> i32 {
        self.start + self.span()
    }

    pub fn day(&self) -> i32 {
        self.start.div_euclid(Self::MINUTES_PER_DAY)
    }

    pub fn covers(&self, day: i32) -> bool {
        let midnight = day * Self::MINUTES_PER_DAY;

        self.start < midnight + Self::MINUTES_PER_DAY && self.end() > midnight
    }

    pub fn slice(&self, day: i32) -> Range<i32> {
        let midnight = day * Self::MINUTES_PER_DAY;

        (self.start - midnight).max(0)..(self.end() - midnight).min(Self::MINUTES_PER_DAY)
    }

    pub fn elapsed_work(&self, now: i32) -> i32 {
        let mut start = self.start;
        let mut worked = 0;

        for segment in &self.segments {
            if segment.kind == SegmentKind::Work {
                worked += (now - start).clamp(0, segment.minutes);
            }

            start += segment.minutes;
        }

        worked
    }

    pub fn phase(&self, now: f32) -> Option<(SegmentKind, Range<i32>)> {
        let mut start = self.start;

        for segment in &self.segments {
            let end = start + segment.minutes;

            if now < end as f32 {
                return Some((segment.kind, start..end));
            }

            start = end;
        }

        None
    }

    pub fn work(&self) -> i32 {
        self.segments
            .iter()
            .filter(|segment| segment.kind == SegmentKind::Work)
            .map(|segment| segment.minutes)
            .sum()
    }

    pub fn work_range(&self) -> Range<i32> {
        let transition = |segment: &&Segment| segment.kind != SegmentKind::Work;
        let before: i32 = self
            .segments
            .iter()
            .take_while(transition)
            .map(|segment| segment.minutes)
            .sum();
        let after: i32 = self
            .segments
            .iter()
            .rev()
            .take_while(transition)
            .map(|segment| segment.minutes)
            .sum();
        let start = self.start + before;

        start..(self.end() - after).max(start)
    }
}

#[derive(Clone, Copy, Serialize, Deserialize)]
pub struct Segment {
    pub kind: SegmentKind,
    pub minutes: i32,
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SegmentKind {
    Prep,
    Work,
    Pause,
    Cleanup,
}

impl SegmentKind {
    fn hatched(self) -> bool {
        matches!(self, Self::Prep | Self::Cleanup)
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum BlockState {
    Upcoming,
    Current,
    Past,
    Happened,
    Skipped,
}

impl BlockState {
    const PAST: f32 = 0.4;
    const HAPPENED: f32 = 0.45;
    const SKIPPED: f32 = 0.34;

    fn new(block: &Block, now: f32) -> Self {
        match block.outcome {
            Some(Outcome::Skipped) => Self::Skipped,
            Some(Outcome::Assumed | Outcome::Done) => Self::Happened,
            None if block.end() as f32 <= now => Self::Past,
            None if block.start as f32 <= now => Self::Current,
            None => Self::Upcoming,
        }
    }

    fn colors(self, color: BlockColor, cx: &App) -> BlockColors {
        let theme = cx.theme();
        let faded = |strength| theme.block(color).faded(theme.grid_bg, strength);

        match self {
            Self::Upcoming => theme.block(color),
            Self::Current => theme.current_block(color),
            Self::Past => faded(Self::PAST),
            Self::Happened => faded(Self::HAPPENED),
            Self::Skipped => BlockColors {
                border: theme
                    .grid_bg
                    .blend(theme.skipped_border.alpha(Self::SKIPPED)),
                ..faded(Self::SKIPPED)
            },
        }
    }
}

#[derive(IntoElement)]
pub struct BlockView {
    index: usize,
    task: TaskId,
    title: SharedString,
    place: Option<SharedString>,
    color: BlockColor,
    state: BlockState,
    skipped: bool,
    unconfirmed: bool,
    conflict: bool,
    start: i32,
    work_range: Range<i32>,
    work: i32,
    visible: Range<i32>,
    opens: bool,
    closes: bool,
    segments: Vec<Segment>,
    bounds: Bounds<Pixels>,
    on_click: Option<Box<ClickHandler>>,
    on_done: Option<Box<ClickHandler>>,
    on_skip: Option<Box<ClickHandler>>,
    details: Option<Box<TooltipBuilder>>,
}

impl BlockView {
    const CORNER_RADIUS: Pixels = px(4.0);
    const BORDER_WIDTH: Pixels = px(1.0);
    const OUTLINE_WIDTH: Pixels = px(2.0);
    const HATCH_PITCH: f32 = 4.0 * SQRT_2;
    const COMPACT_HEIGHT: Pixels = px(21.0);
    const VERDICT_SIZE: Pixels = px(16.0);
    const VERDICT_GAP: Pixels = px(3.0);
    const CONFLICT_DIAMETER: Pixels = px(7.0);
    const META_HEIGHT: Pixels = px(30.0);
    const PLACE_HEIGHT: Pixels = px(44.0);

    pub fn new(block: &Block, day: i32, bounds: Bounds<Pixels>, now: f32) -> Option<Self> {
        let midnight = day * Block::MINUTES_PER_DAY;
        let slice = block.slice(day);

        Some(Self {
            index: 0,
            task: block.task,
            title: block.title.clone(),
            place: block.place.clone(),
            color: block.color?,
            state: BlockState::new(block, now),
            skipped: block.outcome == Some(Outcome::Skipped),
            unconfirmed: block.outcome == Some(Outcome::Assumed),
            conflict: block.conflict,
            start: block.start,
            work_range: block.work_range(),
            work: block.work(),
            visible: midnight + slice.start - block.start..midnight + slice.end - block.start,
            opens: block.start >= midnight,
            closes: block.end() <= midnight + Block::MINUTES_PER_DAY,
            segments: block.segments.clone(),
            bounds,
            on_click: None,
            on_done: None,
            on_skip: None,
            details: None,
        })
    }

    pub fn task(&self) -> TaskId {
        self.task
    }

    pub fn start(&self) -> i32 {
        self.start
    }

    pub fn occurrence(&self) -> Range<i32> {
        self.work_range.clone()
    }

    pub fn details(mut self, details: Box<TooltipBuilder>) -> Self {
        self.details = Some(details);
        self
    }

    pub fn index(mut self, index: usize) -> Self {
        self.index = index;
        self
    }

    pub fn on_click(mut self, on_click: Box<ClickHandler>) -> Self {
        self.on_click = Some(on_click);
        self
    }

    pub fn on_done(mut self, on_done: Box<ClickHandler>) -> Self {
        self.on_done = Some(on_done);
        self
    }

    pub fn on_skip(mut self, on_skip: Box<ClickHandler>) -> Self {
        self.on_skip = Some(on_skip);
        self
    }

    fn segments(&self, colors: &BlockColors) -> Vec<Div> {
        let edges = u8::from(self.opens) + u8::from(self.closes);
        let content = f32::from(self.bounds.size.height - Self::BORDER_WIDTH * edges as f32);
        let visible = (self.visible.end - self.visible.start).max(1) as f32;
        let mut drawn = Vec::new();
        let mut elapsed = 0;
        let mut top = 0.0;

        for segment in &self.segments {
            let natural = elapsed;
            let start = natural.max(self.visible.start);
            elapsed += segment.minutes;

            let end = elapsed.min(self.visible.end);
            if end <= start {
                continue;
            }

            let bottom = (content * ((end - self.visible.start) as f32 / visible)).round();
            drawn.push((
                segment.kind,
                px(bottom - top),
                natural >= self.visible.start,
            ));
            top = bottom;
        }

        let last = drawn.len().saturating_sub(1);

        drawn
            .into_iter()
            .enumerate()
            .map(|(index, (kind, height, line))| {
                Self::segment(
                    kind,
                    height,
                    index == 0 && self.opens,
                    index == last && self.closes,
                    line,
                    colors,
                )
            })
            .collect()
    }

    fn segment(
        kind: SegmentKind,
        height: Pixels,
        top: bool,
        bottom: bool,
        line: bool,
        colors: &BlockColors,
    ) -> Div {
        let work = kind == SegmentKind::Work;

        Self::fill(top, bottom)
            .flex_none()
            .h(height)
            .bg(if work { colors.work } else { colors.transition })
            .when(line && !top, |segment| {
                segment
                    .border_t(Self::BORDER_WIDTH)
                    .border_color(colors.segment_line)
            })
            .when(kind.hatched(), |segment| {
                segment.child(Self::hatch(top, bottom, colors))
            })
    }

    fn hatch(top: bool, bottom: bool, colors: &BlockColors) -> Div {
        Self::fill(top, bottom).size_full().bg(pattern_slash(
            colors.work,
            Self::HATCH_PITCH,
            Self::HATCH_PITCH,
        ))
    }

    fn note(&self, id: impl Into<ElementId>, text: SharedString, color: Rgba) -> Div {
        div()
            .flex_none()
            .mt(px(1.0))
            .truncate()
            .text_size(px(10.0))
            .text_color(color)
            .child(Text::new(id.into(), text))
    }

    fn meta(&self, cx: &App) -> String {
        let clock = cx.global::<ClockFormat>();

        format!(
            "{} - {} ({})",
            clock.time_label(self.work_range.start),
            clock.time_label(self.work_range.end),
            Self::duration_label(self.work)
        )
    }

    fn duration_label(minutes: i32) -> String {
        match (minutes / 60, minutes % 60) {
            (0, minutes) => format!("{minutes}m"),
            (hours, 0) => format!("{hours}h"),
            (hours, minutes) => format!("{hours}h {minutes}m"),
        }
    }

    fn title(&self, colors: &BlockColors) -> Div {
        let height = self.bounds.size.height;
        let text_size = if height < px(13.0) {
            px(9.5)
        } else if height < px(16.0) {
            px(10.5)
        } else {
            px(11.5)
        };

        div()
            .flex_none()
            .truncate()
            .text_size(text_size)
            .line_height((height - px(2.0)).clamp(px(9.0), px(14.4)))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(colors.fg)
            .child(Text::new(
                ("block-title", self.index).into(),
                self.title.clone(),
            ))
    }

    fn label(&self, colors: &BlockColors, cx: &App) -> Div {
        let height = self.bounds.size.height;
        let compact = height < Self::COMPACT_HEIGHT;
        let place = self.place.clone().filter(|_| height > Self::PLACE_HEIGHT);

        div()
            .absolute()
            .inset_0()
            .flex()
            .flex_col()
            .when(compact, |label| label.justify_center())
            .pl(px(6.0))
            .pr(px(5.0))
            .when(!compact, |label| label.py(px(2.0)))
            .overflow_hidden()
            .child(self.title(colors))
            .when(height > Self::META_HEIGHT, |label| {
                let meta = self.meta(cx).into();
                label.child(self.note(("block-meta", self.index), meta, colors.meta_fg))
            })
            .when_some(place, |label, place| {
                label.child(self.note(("block-place", self.index), place, colors.meta_fg))
            })
    }

    fn conflict(&self) -> Option<Div> {
        self.conflict.then(|| {
            div()
                .absolute()
                .top(px(3.0))
                .right(px(3.0))
                .size(Self::CONFLICT_DIAMETER)
                .rounded_full()
                .bg(rgb(0xb0813c))
        })
    }

    fn verdict(&mut self, cx: &App) -> Option<Div> {
        let (done, skip) = (self.on_done.take()?, self.on_skip.take()?);

        if !self.unconfirmed {
            return None;
        }

        let button = (self.bounds.size.height * 0.8).min(Self::VERDICT_SIZE);
        let inset = ((self.bounds.size.height - button) / 2.0).min(px(2.0));

        Some(
            div()
                .absolute()
                .top(inset)
                .right(px(3.0))
                .flex()
                .items_start()
                .gap(Self::VERDICT_GAP)
                .child(
                    self.verdict_button(
                        ("block-done", self.index),
                        "It happened",
                        button,
                        done,
                        cx,
                    )
                    .hover(|style| style.text_color(cx.theme().link_fg))
                    .child(Text::new_inaccessible("✓".into())),
                )
                .child(
                    self.verdict_button(
                        ("block-skip", self.index),
                        "It did not happen",
                        button,
                        skip,
                        cx,
                    )
                    .hover(|style| style.text_color(cx.theme().danger_fg))
                    .child(Text::new_inaccessible("✗".into())),
                ),
        )
    }

    fn verdict_button(
        &self,
        id: impl Into<ElementId>,
        label: &'static str,
        size: Pixels,
        on_click: Box<ClickHandler>,
        cx: &App,
    ) -> Stateful<Div> {
        div()
            .id(id.into())
            .role(Role::Button)
            .aria_label(label)
            .flex()
            .flex_none()
            .w(Self::VERDICT_SIZE)
            .h(size)
            .text_size(size * 0.625)
            .items_center()
            .justify_center()
            .text_color(cx.theme().dim_fg)
            .cursor_pointer()
            .on_click(move |_event, window, cx| {
                cx.stop_propagation();
                on_click(window, cx);
            })
    }

    fn fill(top: bool, bottom: bool) -> Div {
        let radius = Self::CORNER_RADIUS - Self::BORDER_WIDTH;

        div()
            .when(top, |fill| fill.rounded_t(radius))
            .when(bottom, |fill| fill.rounded_b(radius))
    }
}

impl RenderOnce for BlockView {
    fn render(mut self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let colors = self.state.colors(self.color, cx);
        let verdict = self.verdict(cx);
        let details = self.details.take();
        let index = self.index;

        let block = div()
            .id(("block", self.index))
            .role(Role::Button)
            .aria_label(self.title.clone())
            .absolute()
            .top(self.bounds.origin.y)
            .left(self.bounds.origin.x)
            .w(self.bounds.size.width)
            .h(self.bounds.size.height)
            .flex()
            .flex_col()
            .overflow_hidden()
            .when(self.opens, |block| {
                block
                    .rounded_t(Self::CORNER_RADIUS)
                    .border_t(Self::BORDER_WIDTH)
            })
            .when(self.closes, |block| {
                block
                    .rounded_b(Self::CORNER_RADIUS)
                    .border_b(Self::BORDER_WIDTH)
            })
            .border_l(Self::BORDER_WIDTH)
            .border_r(Self::BORDER_WIDTH)
            .border_color(colors.border)
            .when(self.skipped, |block| block.border_dashed())
            .when(self.state == BlockState::Current, |block| {
                block.shadow(vec![
                    BoxShadow::new(px(0.0), px(0.0), colors.outline.into())
                        .spread_radius(Self::OUTLINE_WIDTH),
                ])
            })
            .children(self.segments(&colors))
            .child(self.label(&colors, cx))
            .children(self.conflict())
            .children(verdict)
            .when_some(self.on_click, |block, on_click| {
                block.cursor_pointer().on_click(move |_event, window, cx| {
                    cx.stop_propagation();
                    on_click(window, cx);
                })
            });

        match details {
            Some(details) => {
                Tooltipped::new(("block-details", index), block, details).into_any_element()
            }
            None => block.into_any_element(),
        }
    }
}
