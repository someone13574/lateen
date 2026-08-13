use gpui::prelude::*;
use gpui::{
    Entity, TitlebarOptions, Window, WindowDecorations, WindowOptions, div, px, relative, size,
};

use crate::agenda::Agenda;
use crate::assets::Assets;
use crate::bottom_bar::BottomBar;
use crate::calendar::Calendar;
use crate::clock::{Clock, ClockFormat};
use crate::input::InputState;
use crate::panel::Panel;
use crate::select::SelectState;
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
mod input;
mod now_card;
mod panel;
mod planner;
mod schedule;
mod scrollbar;
mod select;
mod session;
mod task;
mod theme;
mod titlebar;
mod window;
mod window_control;

const APP_NAME: &str = "Lateen";

struct RootView {
    calendar: Entity<Calendar>,
    panel: Entity<Panel>,
    bottom_bar: Entity<BottomBar>,
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
                .child(Titlebar::new(APP_NAME))
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

            cx.new(|cx| RootView {
                calendar: cx.new(|cx| Calendar::new(agenda.clone(), cx)),
                panel: cx.new(|cx| Panel::new(agenda, window, cx)),
                bottom_bar: cx.new(BottomBar::new),
            })
        })
        .unwrap();
    });
}
