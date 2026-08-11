use gpui::prelude::*;
use gpui::{
    Entity, TitlebarOptions, Window, WindowDecorations, WindowOptions, div, px, relative, size,
};

use crate::assets::Assets;
use crate::calendar::Calendar;
use crate::clock::ClockFormat;
use crate::theme::{ActiveTheme, Theme};
use crate::titlebar::Titlebar;
use crate::window::WindowFrame;

mod assets;
mod block;
mod button;
mod calendar;
mod clock;
mod grid;
mod schedule;
mod scrollbar;
mod theme;
mod titlebar;
mod window;
mod window_control;

const APP_NAME: &str = "Lateen";

struct RootView {
    calendar: Entity<Calendar>,
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
                .child(self.calendar.clone()),
        )
    }
}

fn main() {
    gpui_platform::application().with_assets(Assets).run(|cx| {
        Assets::load_fonts(cx).expect("failed to load embedded fonts");
        ClockFormat::init(cx);

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

            cx.new(|cx| RootView {
                calendar: cx.new(|_cx| Calendar::new()),
            })
        })
        .unwrap();
    });
}
