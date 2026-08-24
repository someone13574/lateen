use std::time::Duration;

use chrono::{Local, Timelike};
use gpui::prelude::*;
use gpui::{Entity, Pixels, Window, div, px};

use crate::agenda::Agenda;
use crate::commitment_list::CommitmentList;
use crate::editor::Editor;
use crate::import::Import;
use crate::now_card::NowCard;
use crate::theme::ActiveTheme;

pub struct Panel {
    agenda: Entity<Agenda>,
    import: Entity<Import>,
    editor: Option<Entity<Editor>>,
}

impl Panel {
    const WIDTH: Pixels = px(340.0);
    const MIN_WIDTH: Pixels = px(264.0);

    pub fn new(
        agenda: Entity<Agenda>,
        import: Entity<Import>,
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
            import,
            editor: None,
        }
    }

    fn sync(&mut self, agenda: &Entity<Agenda>, window: &mut Window, cx: &mut Context<Self>) {
        let selected = agenda.read(cx).selected();

        if selected != self.editor.as_ref().map(|editor| editor.read(cx).task()) {
            self.editor = selected.map(|task| {
                cx.new(|cx| Editor::new(agenda.clone(), self.import.clone(), task, window, cx))
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
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .flex_shrink(1.0)
            .w(Self::WIDTH)
            .min_w(Self::MIN_WIDTH)
            .bg(cx.theme().panel_bg)
            .border_l(px(1.0))
            .border_color(cx.theme().panel_border)
            .child(NowCard::new(&self.agenda, cx))
            .child(match &self.editor {
                Some(editor) => editor.clone().into_any_element(),
                None => {
                    CommitmentList::new(self.agenda.clone(), self.import.clone()).into_any_element()
                }
            })
    }
}
