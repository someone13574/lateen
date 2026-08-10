use gpui::prelude::*;
use gpui::{App, Global, Rgba, Window, WindowAppearance, rgb, rgba};

#[derive(Clone, Copy)]
pub struct Theme {
    pub fg: Rgba,
    pub titlebar_bg: Rgba,
    pub titlebar_border: Rgba,
    pub titlebar_control_fg: Rgba,
    pub titlebar_control_hover_bg: Rgba,
    pub titlebar_close_hover_bg: Rgba,
    pub titlebar_close_hover_fg: Rgba,
    pub button_bg: Rgba,
    pub button_border: Rgba,
    pub button_hover_bg: Rgba,
    pub window_bg: Rgba,
    pub window_border: Rgba,
    pub window_shadow: Rgba,
    pub window_contact_shadow: Rgba,
}

impl Theme {
    pub fn light() -> Self {
        Self {
            fg: rgb(0x1c1c1a),
            titlebar_bg: rgb(0xfbfbfa),
            titlebar_border: rgb(0xd5d5d0),
            titlebar_control_fg: rgb(0x54544e),
            titlebar_control_hover_bg: rgb(0xdededa),
            titlebar_close_hover_bg: rgb(0xc8402f),
            titlebar_close_hover_fg: rgb(0xffffff),
            button_bg: rgb(0xffffff),
            button_border: rgb(0xcfcfca),
            button_hover_bg: rgb(0xf0f0ed),
            window_bg: rgb(0xf4f4f2),
            window_border: rgb(0xb4b4ae),
            window_shadow: rgba(0x00000038),
            window_contact_shadow: rgba(0x0000001f),
        }
    }

    pub fn dark() -> Self {
        Self {
            fg: rgb(0xe7e8ea),
            titlebar_bg: rgb(0x232629),
            titlebar_border: rgb(0x33373b),
            titlebar_control_fg: rgb(0xb4b8bd),
            titlebar_control_hover_bg: rgb(0x31353a),
            titlebar_close_hover_bg: rgb(0xc8402f),
            titlebar_close_hover_fg: rgb(0x10131a),
            button_bg: rgb(0x26292d),
            button_border: rgb(0x3a3f45),
            button_hover_bg: rgb(0x2d3136),
            window_bg: rgb(0x1a1c1f),
            window_border: rgb(0x000000),
            window_shadow: rgba(0x00000038),
            window_contact_shadow: rgba(0x0000001f),
        }
    }

    pub fn init(window: &mut Window, cx: &mut App) {
        cx.set_global(Self::for_appearance(window.appearance()));

        window
            .observe_window_appearance(|window, cx| {
                cx.set_global(Self::for_appearance(window.appearance()));
                window.refresh();
            })
            .detach();
    }

    fn for_appearance(appearance: WindowAppearance) -> Self {
        match appearance {
            WindowAppearance::Light | WindowAppearance::VibrantLight => Self::light(),
            WindowAppearance::Dark | WindowAppearance::VibrantDark => Self::dark(),
        }
    }
}

impl Global for Theme {}

pub trait ActiveTheme {
    fn theme(&self) -> &Theme;
}

impl ActiveTheme for App {
    fn theme(&self) -> &Theme {
        self.global()
    }
}

impl<T> ActiveTheme for Context<'_, T> {
    fn theme(&self) -> &Theme {
        self.global()
    }
}
