use gpui::prelude::*;
use gpui::{App, Div, Entity, FontWeight, Rgba, SharedString, Text, Window, div, px, relative};

use crate::agenda::Agenda;
use crate::block::{Block, SegmentKind};
use crate::button::{Button, ClickHandler};
use crate::clock::{Clock, ClockFormat};
use crate::selectable_text::SelectableText;
use crate::task::{Task, TaskId, TaskKind};
use crate::task_details::TaskDetails;
use crate::theme::{ActiveTheme, BlockColor};
use crate::tooltip::{TooltipBuilder, Tooltipped};

pub struct Running {
    agenda: Entity<Agenda>,
    task: TaskId,
    start: i32,
    title: SharedString,
    range: String,
    countdown: String,
    phase: &'static str,
    elapsed: f32,
    color: Option<BlockColor>,
    working: bool,
    splittable: bool,
    flexible: bool,
}

pub struct Upcoming {
    title: SharedString,
    when: String,
    countdown: String,
}

#[derive(IntoElement)]
pub struct NowCard {
    state: NowState,
    details: Option<Box<TooltipBuilder>>,
}

enum NowState {
    Running(Running),
    Idle(Option<Upcoming>),
}

impl Running {
    fn new(block: &Block, task: Option<&Task>, agenda: Entity<Agenda>, now: f32, cx: &App) -> Self {
        let clock = *cx.global::<ClockFormat>();
        let (kind, phase) = block
            .phase(now)
            .unwrap_or((SegmentKind::Work, block.start..block.end()));
        let length = (phase.end - phase.start).max(1) as f32;
        let next = block.phase(phase.end as f32).map(|(kind, _)| kind);

        Self {
            agenda,
            task: block.task,
            start: block.start,
            title: block.title.clone(),
            range: Self::range(block, clock),
            countdown: Self::countdown(phase.end as f32 - now),
            phase: Self::phase_label(kind, next),
            elapsed: ((now - phase.start as f32) / length).clamp(0.0, 1.0),
            color: block.color,
            working: kind == SegmentKind::Work,
            splittable: task.is_some_and(|task| task.splittable()),
            flexible: task.is_some_and(|task| matches!(task.kind, TaskKind::Flexible(_))),
        }
    }

    fn phase_label(kind: SegmentKind, next: Option<SegmentKind>) -> &'static str {
        match kind {
            SegmentKind::Prep => "until it starts",
            SegmentKind::Work if next == Some(SegmentKind::Pause) => "until break",
            SegmentKind::Work => "left",
            SegmentKind::Pause => "of break",
            SegmentKind::Cleanup => "wrapping up",
        }
    }

    fn range(block: &Block, clock: ClockFormat) -> String {
        let work = block.work_range();
        let range = format!(
            "{} to {}",
            clock.time_label(work.start),
            clock.time_label(work.end)
        );

        match &block.place {
            Some(place) => format!("{range}, {place}"),
            None => range,
        }
    }

    fn countdown(minutes: f32) -> String {
        let seconds = (minutes.max(0.0) * 60.0).round() as i32;
        let (hours, minutes, seconds) = (seconds / 3600, seconds / 60 % 60, seconds % 60);

        match hours {
            0 => format!("{minutes:02}:{seconds:02}"),
            _ => format!("{hours}:{minutes:02}:{seconds:02}"),
        }
    }

    fn render(&self, window: &mut Window, cx: &App) -> Div {
        let theme = *cx.theme();
        let tint = self.color.map(|color| theme.now_card(color));
        let phase_fg = match (self.working, self.color) {
            (true, Some(color)) => theme.swatch(color),
            _ => theme.muted_fg,
        };

        if !cx.reduce_motion() {
            window.request_animation_frame();
        }

        div()
            .rounded(px(8.0))
            .border(px(1.0))
            .border_color(tint.map_or(theme.panel_divider, |tint| tint.border))
            .bg(tint.map_or(theme.card_bg, |tint| tint.bg))
            .pt(px(11.0))
            .px(px(12.0))
            .pb(px(12.0))
            .child(self.heading(phase_fg, cx))
            .child(self.progress(phase_fg, cx))
            .child(self.actions())
    }

    fn actions(&self) -> Div {
        let end_label = match self.splittable {
            true => "End session here",
            false => "Done",
        };

        div()
            .flex()
            .mt(px(11.0))
            .gap(px(5.0))
            .child(
                Button::new("end-early", end_label)
                    .filled()
                    .stretch()
                    .padding(px(5.0), px(5.0))
                    .on_click(self.dispatch(Agenda::end_early)),
            )
            .when(self.splittable, |actions| {
                actions.child(
                    Button::new("finish", "End full task")
                        .stretch()
                        .padding(px(5.0), px(5.0))
                        .on_click(self.dispatch(Agenda::finish)),
                )
            })
            .when(self.flexible, |actions| {
                actions.child(
                    Button::new("skip", "Skip")
                        .padding(px(9.0), px(5.0))
                        .on_click(self.dispatch(Agenda::skip)),
                )
            })
    }

    fn dispatch(
        &self,
        action: fn(&mut Agenda, TaskId, i32, &mut Context<Agenda>),
    ) -> Box<ClickHandler> {
        let agenda = self.agenda.clone();
        let (task, start) = (self.task, self.start);

        Box::new(move |_window, cx| {
            agenda.update(cx, |agenda, cx| action(agenda, task, start, cx));
        })
    }

    fn heading(&self, phase_fg: Rgba, cx: &App) -> Div {
        let theme = *cx.theme();

        div()
            .flex()
            .items_start()
            .gap(px(10.0))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .child(
                        div()
                            .truncate()
                            .text_size(px(15.0))
                            .line_height(px(18.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(SelectableText::new("now-title", self.title.clone())),
                    )
                    .child(
                        div()
                            .mt(px(3.0))
                            .truncate()
                            .text_size(px(11.5))
                            .text_color(theme.muted_fg)
                            .child(SelectableText::new("now-range", self.range.clone())),
                    ),
            )
            .child(self.timer(phase_fg, cx))
    }

    fn timer(&self, phase_fg: Rgba, cx: &App) -> Div {
        div()
            .flex()
            .flex_col()
            .flex_none()
            .items_end()
            .child(
                div()
                    .text_size(px(21.0))
                    .line_height(px(21.0))
                    .letter_spacing(px(-0.42))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(phase_fg)
                    .child(SelectableText::new("now-countdown", self.countdown.clone())),
            )
            .child(
                div()
                    .mt(px(4.0))
                    .text_size(px(10.0))
                    .text_color(cx.theme().faint_fg)
                    .child(Text::new("now-phase".into(), self.phase.into())),
            )
    }

    fn progress(&self, phase_fg: Rgba, cx: &App) -> Div {
        div()
            .mt(px(11.0))
            .h(px(3.0))
            .rounded(px(2.0))
            .overflow_hidden()
            .bg(cx.theme().track_bg)
            .child(div().h_full().w(relative(self.elapsed)).bg(phase_fg))
    }
}

impl Upcoming {
    fn new(block: &Block, now: f32, cx: &App) -> Self {
        let start = block.work_range().start;
        let when = cx.global::<ClockFormat>().time_label(start);

        Self {
            title: block.title.clone(),
            when: match &block.place {
                Some(place) => format!("{when}, {place}"),
                None => when,
            },
            countdown: Self::duration_label((start as f32 - now).round() as i32),
        }
    }

    fn render(&self, cx: &App) -> Div {
        let theme = *cx.theme();

        div()
            .flex()
            .items_center()
            .gap(px(10.0))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .child(
                        div()
                            .text_size(px(11.0))
                            .text_color(theme.faint_fg)
                            .child(Text::new("next-lead".into(), "Nothing until".into())),
                    )
                    .child(
                        div()
                            .mt(px(3.0))
                            .truncate()
                            .text_size(px(15.0))
                            .line_height(px(18.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(SelectableText::new("next-title", self.title.clone())),
                    )
                    .child(
                        div()
                            .mt(px(3.0))
                            .truncate()
                            .text_size(px(11.5))
                            .text_color(theme.muted_fg)
                            .child(SelectableText::new("next-when", self.when.clone())),
                    ),
            )
            .child(self.timer(cx))
    }

    fn timer(&self, cx: &App) -> Div {
        let theme = *cx.theme();

        div()
            .flex()
            .flex_col()
            .flex_none()
            .items_end()
            .child(
                div()
                    .text_size(px(21.0))
                    .line_height(px(21.0))
                    .letter_spacing(px(-0.42))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme.body_fg)
                    .child(SelectableText::new(
                        "next-countdown",
                        self.countdown.clone(),
                    )),
            )
            .child(
                div()
                    .mt(px(4.0))
                    .text_size(px(10.0))
                    .text_color(theme.faint_fg)
                    .child(Text::new("next-lag".into(), "from now".into())),
            )
    }

    fn duration_label(minutes: i32) -> String {
        let (hours, minutes) = (minutes / 60, minutes % 60);
        let (days, hours) = (hours / 24, hours % 24);

        match (days, hours, minutes) {
            (0, 0, minutes) => format!("{minutes}m"),
            (0, hours, 0) => format!("{hours}h"),
            (0, hours, minutes) => format!("{hours}h {minutes}m"),
            (days, 0, _) => format!("{days}d"),
            (days, hours, _) => format!("{days}d {hours}h"),
        }
    }
}

impl NowCard {
    pub fn new(agenda: &Entity<Agenda>, cx: &App) -> Self {
        let now = cx.global::<Clock>().minute_of_day();
        let state = agenda.read(cx);

        match state.running(now) {
            Some(block) => Self {
                details: Some(Self::details(agenda, block)),
                state: NowState::Running(Running::new(
                    block,
                    state.task(block.task),
                    agenda.clone(),
                    now,
                    cx,
                )),
            },
            None => {
                let next = state.upcoming(now);

                Self {
                    details: next.map(|block| Self::details(agenda, block)),
                    state: NowState::Idle(next.map(|block| Upcoming::new(block, now, cx))),
                }
            }
        }
    }

    fn details(agenda: &Entity<Agenda>, block: &Block) -> Box<TooltipBuilder> {
        TaskDetails::new(agenda.clone(), block.task).occurrence(block.work_range())
    }

    fn idle(next: Option<&Upcoming>, cx: &App) -> Div {
        let theme = *cx.theme();

        div()
            .rounded(px(8.0))
            .border(px(1.0))
            .border_color(theme.panel_divider)
            .bg(theme.card_bg)
            .py(px(12.0))
            .px(px(13.0))
            .child(match next {
                Some(next) => next.render(cx),
                None => div()
                    .text_size(px(14.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme.muted_fg)
                    .child(Text::new(
                        "next-none".into(),
                        "Nothing else scheduled".into(),
                    )),
            })
    }
}

impl RenderOnce for NowCard {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let card = div()
            .flex_none()
            .p(px(12.0))
            .border_b(px(1.0))
            .border_color(cx.theme().panel_divider)
            .child(match &self.state {
                NowState::Running(running) => running.render(window, cx),
                NowState::Idle(next) => Self::idle(next.as_ref(), cx),
            });

        match self.details {
            Some(details) => Tooltipped::new("now-details", card, details).into_any_element(),
            None => card.into_any_element(),
        }
    }
}
