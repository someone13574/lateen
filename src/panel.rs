use std::time::Duration;

use chrono::{Local, Timelike};
use gpui::prelude::*;
use gpui::{Decorations, Entity, Pixels, Tiling, Window, div, px};

use crate::agenda::Agenda;
use crate::bottom_bar::BottomBar;
use crate::calendar_list::CalendarList;
use crate::commitment_list::CommitmentList;
use crate::editor::Editor;
use crate::now_card::NowCard;
use crate::theme::ActiveTheme;

pub struct Panel {
    agenda: Entity<Agenda>,
    calendars: Entity<CalendarList>,
    editor: Option<Entity<Editor>>,
}

impl Panel {
    const WIDTH: Pixels = px(340.0);
    const MIN_WIDTH: Pixels = px(264.0);
    const CORNER_RADIUS: Pixels = px(8.0);

    pub fn new(
        agenda: Entity<Agenda>,
        calendars: Entity<CalendarList>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        cx.observe_in(&agenda, window, |panel, agenda, window, cx| {
            panel.sync(&agenda, window, cx);
            cx.notify();
        })
        .detach();
        Self::follow_seconds(cx);

        Self {
            agenda,
            calendars,
            editor: None,
        }
    }

    fn sync(&mut self, agenda: &Entity<Agenda>, window: &mut Window, cx: &mut Context<Self>) {
        let selected = agenda.read(cx).selected();

        if selected != self.editor.as_ref().map(|editor| editor.read(cx).task()) {
            self.editor = selected.map(|task| {
                cx.new(|cx| Editor::new(agenda.clone(), self.calendars.clone(), task, window, cx))
            });
        }
    }

    fn follow_seconds(cx: &mut Context<Self>) {
        cx.spawn(async move |panel, cx| {
            loop {
                let nanoseconds = Local::now().nanosecond() % 1_000_000_000;
                let until_next_second = Duration::from_nanos((1_000_000_000 - nanoseconds).into());
                cx.background_executor().timer(until_next_second).await;

                if panel.update(cx, |_panel, cx| cx.notify()).is_err() {
                    break;
                }
            }
        })
        .detach();
    }
}

impl Render for Panel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tiling = match window.window_decorations() {
            Decorations::Client { tiling } => tiling,
            Decorations::Server => Tiling::tiled(),
        };

        div()
            .flex()
            .flex_col()
            .flex_shrink(1.0)
            .w(Self::WIDTH)
            .min_w(Self::MIN_WIDTH)
            .bg(cx.theme().panel_bg)
            .border_l(px(1.0))
            .border_color(cx.theme().panel_border)
            .when(
                !BottomBar::enabled() && !tiling.bottom && !tiling.right,
                |panel| panel.rounded_br(Self::CORNER_RADIUS),
            )
            .child(NowCard::new(&self.agenda, cx))
            .child(match &self.editor {
                Some(editor) => editor.clone().into_any_element(),
                None => CommitmentList::new(self.agenda.clone(), self.calendars.clone())
                    .into_any_element(),
            })
    }
}
