use std::collections::HashMap;
use std::mem;
use std::ops::Range;

use futures_lite::StreamExt;
use gpui::prelude::*;
use gpui::{App, Entity, Global, TaskExt};
use zbus::zvariant::Value;
use zbus::{Connection, Proxy};

use crate::APP_NAME;
use crate::agenda::Agenda;
use crate::block::{Block, SegmentKind};
use crate::clock::{Clock, ClockFormat};
use crate::task::{TaskId, TaskKind};

pub struct Notifier {
    server: Option<Proxy<'static>>,
    announced: i32,
    serial: u64,
    skips: Vec<Skip>,
}

struct Skip {
    action: String,
    task: TaskId,
    start: i32,
    end: i32,
}

impl Notifier {
    const SKIP: &str = "skip";
    const CATCH_UP: i32 = 2;
    const SERVICE: &str = "org.freedesktop.Notifications";
    const OBJECT: &str = "/org/freedesktop/Notifications";
    const EXPIRY: i32 = -1;

    pub fn init(agenda: Entity<Agenda>, cx: &mut App) {
        let announced = Self::minute(cx);

        cx.set_global(Self {
            server: None,
            announced,
            serial: 0,
            skips: Vec::new(),
        });

        cx.spawn(async move |cx| {
            let session = Connection::session().await?;
            let server = Proxy::new(&session, Self::SERVICE, Self::OBJECT, Self::SERVICE).await?;
            let mut invoked = server.receive_signal("ActionInvoked").await?;

            cx.update(|cx| {
                cx.update_global::<Self, _>(|notifier, _cx| notifier.server = Some(server.clone()))
            });

            while let Some(signal) = invoked.next().await {
                if let Ok((_sent, action)) = signal.body().deserialize::<(u32, String)>() {
                    cx.update(|cx| Self::invoke(&action, &agenda, cx));
                }
            }

            anyhow::Ok(())
        })
        .detach_and_log_err(cx);
    }

    pub fn announce(agenda: &Agenda, cx: &mut App) {
        let now = Self::minute(cx);
        let focused = cx.active_window().is_some();

        cx.update_global::<Self, _>(|notifier, cx| {
            let due = notifier.due(now);
            notifier.skips.retain(|target| target.end > now);

            if focused {
                return;
            }

            for block in agenda.schedule().blocks() {
                let cues = Self::cues(block).into_iter();

                for cue in cues.filter(|cue| due.contains(&cue.at)) {
                    notifier.send(block, cue, Self::flexible(block, agenda), cx);
                }
            }
        });
    }

    fn due(&mut self, now: i32) -> Range<i32> {
        let announced = mem::replace(&mut self.announced, now);

        if now <= announced {
            return now..now;
        }

        (announced + 1).max(now - Self::CATCH_UP)..now + 1
    }

    fn flexible(block: &Block, agenda: &Agenda) -> bool {
        agenda
            .task(block.task)
            .is_some_and(|task| matches!(task.kind, TaskKind::Flexible(_)))
    }

    fn invoke(action: &str, agenda: &Entity<Agenda>, cx: &mut App) {
        let target = cx
            .global::<Self>()
            .skips
            .iter()
            .find(|target| target.action == action)
            .map(|target| (target.task, target.start));

        if let Some((task, start)) = target {
            agenda.update(cx, |agenda, cx| agenda.skip(task, start, cx));
        }
    }

    fn send(&mut self, block: &Block, cue: Cue, flexible: bool, cx: &mut App) {
        let Some(server) = self.server.clone() else {
            return;
        };

        let title = block.title.to_string();
        let body = cue.body(block, *cx.global::<ClockFormat>());
        let actions = match flexible && cue.kind.opens() {
            true => vec![self.skip(block), "Skip".to_owned()],
            false => Vec::new(),
        };

        cx.spawn(async move |_cx| {
            Self::notify(&server, title, body, actions).await?;

            anyhow::Ok(())
        })
        .detach_and_log_err(cx);
    }

    async fn notify(
        server: &Proxy<'static>,
        title: String,
        body: String,
        actions: Vec<String>,
    ) -> zbus::Result<u32> {
        let hints = HashMap::<&str, Value>::new();
        let call = (
            APP_NAME,
            0u32,
            "",
            title.as_str(),
            body.as_str(),
            actions,
            hints,
            Self::EXPIRY,
        );

        server.call("Notify", &call).await
    }

    fn skip(&mut self, block: &Block) -> String {
        self.serial += 1;

        let action = format!("{}-{}", Self::SKIP, self.serial);

        self.skips.push(Skip {
            action: action.clone(),
            task: block.task,
            start: block.start,
            end: block.end(),
        });

        action
    }

    fn cues(block: &Block) -> Vec<Cue> {
        let mut at = block.start;
        let mut previous = None;
        let mut cues = Vec::new();

        for segment in &block.segments {
            let until = at + segment.minutes;

            if let Some(kind) = CueKind::new(segment.kind, previous) {
                cues.push(Cue { at, until, kind });
            }

            at = until;
            previous = Some(segment.kind);
        }

        if previous.is_some_and(|kind| kind != SegmentKind::Cleanup) {
            cues.push(Cue {
                at,
                until: at,
                kind: CueKind::End,
            });
        }

        cues
    }

    fn minute(cx: &App) -> i32 {
        cx.global::<Clock>().minute_of_day() as i32
    }
}

impl Global for Notifier {}

struct Cue {
    at: i32,
    until: i32,
    kind: CueKind,
}

impl Cue {
    fn body(&self, block: &Block, clock: ClockFormat) -> String {
        let body = match self.kind {
            CueKind::Prep => format!("Starts at {}", clock.time_label(self.until)),
            CueKind::Start => format!("Until {}", clock.time_label(block.end())),
            CueKind::BreakStart => format!("Back at {}", clock.time_label(self.until)),
            CueKind::BreakEnd => format!("Until {}", clock.time_label(self.until)),
            CueKind::Cleanup => "Cleanup".to_owned(),
            CueKind::End => "Ended".to_owned(),
        };

        match block.place.as_ref().filter(|_| self.kind.opens()) {
            Some(place) => format!("{body}, {place}"),
            None => body,
        }
    }
}

#[derive(Clone, Copy)]
enum CueKind {
    Prep,
    Start,
    BreakStart,
    BreakEnd,
    Cleanup,
    End,
}

impl CueKind {
    fn new(kind: SegmentKind, previous: Option<SegmentKind>) -> Option<Self> {
        match (kind, previous) {
            (SegmentKind::Prep, _) => Some(Self::Prep),
            (SegmentKind::Pause, _) => Some(Self::BreakStart),
            (SegmentKind::Cleanup, _) => Some(Self::Cleanup),
            (SegmentKind::Work, None) => Some(Self::Start),
            (SegmentKind::Work, Some(SegmentKind::Pause)) => Some(Self::BreakEnd),
            (SegmentKind::Work, Some(_)) => None,
        }
    }

    fn opens(self) -> bool {
        matches!(self, Self::Prep | Self::Start)
    }
}
