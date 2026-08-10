use gpui::{
    App, AppContext, Render, Styled, TitlebarOptions, WindowDecorations, WindowOptions, div, px,
    size,
};

use crate::theme::Theme;
use crate::window::WindowFrame;

mod theme;
mod window;

struct RootView {}

impl Render for RootView {
    fn render(
        &mut self,
        _window: &mut gpui::Window,
        _cx: &mut gpui::prelude::Context<Self>,
    ) -> impl gpui::prelude::IntoElement {
        WindowFrame::new(div().size_full())
    }
}

fn main() {
    gpui_platform::application().run(|cx: &mut App| {
        let decorations = match gpui::guess_compositor() {
            "X11" => WindowDecorations::Server,
            _ => WindowDecorations::Client,
        };

        let options = WindowOptions {
            titlebar: Some(TitlebarOptions {
                title: Some("Lateen".into()),
                ..Default::default()
            }),
            window_decorations: Some(decorations),
            window_min_size: Some(size(px(480.0), px(360.0))),
            ..Default::default()
        };

        cx.open_window(options, |window, cx| {
            Theme::init(window, cx);

            cx.new(|_cx| RootView {})
        })
        .unwrap();
    });
}
