use std::f32::consts::SQRT_2;

use gpui::prelude::*;
use gpui::{
    App, Bounds, BoxShadow, Div, ElementId, FontWeight, Pixels, Rgba, SharedString, Text, Window,
    div, pattern_slash, px,
};

use crate::clock::ClockFormat;
use crate::theme::{ActiveTheme, BlockColor, BlockColors};

pub struct Block {
    pub task: usize,
    pub title: SharedString,
    pub place: Option<SharedString>,
    pub color: Option<BlockColor>,
    pub start: i32,
    pub segments: Vec<Segment>,
}

impl Block {
    pub const MINUTES_PER_DAY: i32 = 24 * 60;

    pub fn new(
        task: usize,
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

    pub fn minute_of_day(&self) -> i32 {
        self.start.rem_euclid(Self::MINUTES_PER_DAY)
    }

    fn work_start(&self) -> i32 {
        self.start
            + self
                .segments
                .iter()
                .take_while(|segment| segment.kind == SegmentKind::Prep)
                .map(|segment| segment.minutes)
                .sum::<i32>()
    }

    fn work(&self) -> i32 {
        self.segments
            .iter()
            .filter(|segment| segment.kind == SegmentKind::Work)
            .map(|segment| segment.minutes)
            .sum()
    }
}

#[derive(Clone, Copy)]
pub struct Segment {
    pub kind: SegmentKind,
    pub minutes: i32,
}

#[derive(Clone, Copy, PartialEq, Eq)]
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
}

impl BlockState {
    fn new(block: &Block, now: i32) -> Self {
        if block.end() <= now {
            Self::Past
        } else if block.start <= now {
            Self::Current
        } else {
            Self::Upcoming
        }
    }

    fn colors(self, color: BlockColor, cx: &App) -> BlockColors {
        let theme = cx.theme();

        match self {
            Self::Upcoming => theme.block(color),
            Self::Current => theme.current_block(color),
            Self::Past => theme.block(color).faded(theme.grid_bg),
        }
    }
}

#[derive(IntoElement)]
pub struct BlockView {
    index: usize,
    title: SharedString,
    place: Option<SharedString>,
    color: BlockColor,
    state: BlockState,
    work_start: i32,
    work: i32,
    span: i32,
    segments: Vec<Segment>,
    bounds: Bounds<Pixels>,
}

impl BlockView {
    const CORNER_RADIUS: Pixels = px(4.0);
    const BORDER_WIDTH: Pixels = px(1.0);
    const RING_WIDTH: Pixels = px(2.0);
    const HATCH_PITCH: f32 = 4.0 * SQRT_2;
    const COMPACT_HEIGHT: Pixels = px(21.0);
    const META_HEIGHT: Pixels = px(30.0);
    const PLACE_HEIGHT: Pixels = px(44.0);

    pub fn new(index: usize, block: &Block, bounds: Bounds<Pixels>, now: i32) -> Option<Self> {
        Some(Self {
            index,
            title: block.title.clone(),
            place: block.place.clone(),
            color: block.color?,
            state: BlockState::new(block, now),
            work_start: block.work_start(),
            work: block.work(),
            span: block.span(),
            segments: block.segments.clone(),
            bounds,
        })
    }

    fn segments(&self, colors: &BlockColors) -> Vec<Div> {
        let content = self.bounds.size.height - Self::BORDER_WIDTH * 2.0;
        let last = self.segments.len() - 1;
        let mut top = px(0.0);
        let mut elapsed = 0;

        self.segments
            .iter()
            .enumerate()
            .map(|(index, segment)| {
                elapsed += segment.minutes;
                let bottom = content * (elapsed as f32 / self.span as f32);
                let height = bottom - top;
                top = bottom;

                Self::segment(segment, height, index == 0, index == last, colors)
            })
            .collect()
    }

    fn segment(
        segment: &Segment,
        height: Pixels,
        top: bool,
        bottom: bool,
        colors: &BlockColors,
    ) -> Div {
        let work = segment.kind == SegmentKind::Work;

        Self::fill(top, bottom)
            .flex_none()
            .h(height)
            .bg(if work { colors.work } else { colors.transition })
            .when(!work, |segment| {
                segment
                    .border_t(Self::BORDER_WIDTH)
                    .border_color(colors.segment_line)
            })
            .when(segment.kind.hatched(), |segment| {
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
            clock.time_label(self.work_start),
            clock.time_label(self.work_start + self.work),
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

    fn fill(top: bool, bottom: bool) -> Div {
        let radius = Self::CORNER_RADIUS - Self::BORDER_WIDTH;

        div()
            .when(top, |fill| fill.rounded_t(radius))
            .when(bottom, |fill| fill.rounded_b(radius))
    }
}

impl RenderOnce for BlockView {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let colors = self.state.colors(self.color, cx);

        div()
            .absolute()
            .top(self.bounds.origin.y)
            .left(self.bounds.origin.x)
            .w(self.bounds.size.width)
            .h(self.bounds.size.height)
            .flex()
            .flex_col()
            .overflow_hidden()
            .rounded(Self::CORNER_RADIUS)
            .border(Self::BORDER_WIDTH)
            .border_color(colors.border)
            .when(self.state == BlockState::Current, |block| {
                block.shadow(vec![
                    BoxShadow::new(px(0.0), px(0.0), colors.ring.into())
                        .spread_radius(Self::RING_WIDTH),
                ])
            })
            .children(self.segments(&colors))
            .child(self.label(&colors, cx))
    }
}
