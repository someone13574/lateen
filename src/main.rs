use gpui::prelude::*;
use gpui::{
    Entity, MouseButton, TitlebarOptions, Window, WindowDecorations, WindowOptions, div, px,
    relative, size,
};

use crate::agenda::Agenda;
use crate::assets::Assets;
use crate::bottom_bar::BottomBar;
use crate::button::ClickHandler;
use crate::calendar::Calendar;
use crate::clock::{Clock, ClockFormat};
use crate::import::Import;
use crate::input::InputState;
use crate::notifier::Notifier;
use crate::panel::Panel;
use crate::select::SelectState;
use crate::selectable_text::TextSelection;
use crate::theme::{ActiveTheme, Theme};
use crate::titlebar::Titlebar;
use crate::window::WindowFrame;

mod agenda;
mod assets;
mod block;
mod bottom_bar;
mod button;
mod calendar;
mod clock;
mod colorer;
mod commitment_list;
mod cursor;
mod day_columns;
mod editor;
mod grid;
mod import;
mod input;
mod notifier;
mod now_card;
mod panel;
mod planner;
mod schedule;
mod scrollbar;
mod select;
mod selectable_text;
mod session;
mod store;
mod task;
mod theme;
mod titlebar;
mod window;
mod window_control;

const APP_NAME: &str = "Lateen";

struct RootView {
    agenda: Entity<Agenda>,
    import: Entity<Import>,
    calendar: Entity<Calendar>,
    panel: Entity<Panel>,
    bottom_bar: Entity<BottomBar>,
}

impl RootView {
    fn add_commitment(&self) -> Box<ClickHandler> {
        let agenda = self.agenda.clone();
        let import = self.import.clone();

        Box::new(move |_window, cx| {
            agenda.update(cx, |agenda, cx| agenda.add(cx));
            import.update(cx, |import, cx| import.close(cx));
        })
    }

    fn import_calendar(&self) -> Box<ClickHandler> {
        let agenda = self.agenda.clone();
        let import = self.import.clone();

        Box::new(move |_window, cx| {
            agenda.update(cx, Agenda::deselect);
            import.update(cx, |import, cx| import.toggle(cx));
        })
    }
}

impl Render for RootView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        WindowFrame::new(
            div()
                .flex()
                .flex_col()
                .size_full()
                .font_family("Inter")
                .line_height(relative(1.21))
                .text_color(cx.theme().fg)
                .on_mouse_down(MouseButton::Left, |_event, window, _cx| {
                    if !window.default_prevented() {
                        window.blur();
                    }
                })
                .child(Titlebar::new(
                    APP_NAME,
                    self.add_commitment(),
                    self.import_calendar(),
                ))
                .child(
                    div()
                        .flex()
                        .flex_1()
                        .min_h_0()
                        .child(self.calendar.clone())
                        .child(self.panel.clone()),
                )
                .child(self.bottom_bar.clone()),
        )
    }
}

fn main() {
    gpui_platform::application().with_assets(Assets).run(|cx| {
        Assets::load_fonts(cx).expect("failed to load embedded fonts");
        Clock::init(cx);
        ClockFormat::init(cx);
        InputState::init(cx);
        SelectState::init(cx);
        TextSelection::init(cx);

        let decorations = match gpui::guess_compositor() {
            "X11" => WindowDecorations::Server,
            _ => WindowDecorations::Client,
        };

        let options = WindowOptions {
            titlebar: Some(TitlebarOptions {
                title: Some(APP_NAME.into()),
                ..Default::default()
            }),
            window_decorations: Some(decorations),
            window_min_size: Some(size(px(480.0), px(360.0))),
            app_id: Some("com.github.someone13574.lateen".to_string()),
            ..Default::default()
        };

        cx.open_window(options, |window, cx| {
            Theme::init(window, cx);

            let agenda = cx.new(Agenda::new);
            let import = cx.new(|cx| Import::new(window, cx));

            Notifier::init(agenda.clone(), cx);

            cx.new(|cx| RootView {
                calendar: cx.new(|cx| Calendar::new(agenda.clone(), cx)),
                panel: cx.new(|cx| Panel::new(agenda.clone(), import.clone(), window, cx)),
                bottom_bar: cx.new(BottomBar::new),
                agenda,
                import,
            })
        })
        .unwrap();
    });
}
