use gpui::{App, Context, Global, Pixels, Rgba, Window, WindowAppearance, px, rgb, rgba};

#[derive(Clone, Copy)]
pub struct Theme {
    pub window_background: Rgba,
    pub window_border: Rgba,
    pub window_border_width: Pixels,
    pub window_corner_radius: Pixels,
    pub window_shadow: Rgba,
    pub window_shadow_offset_y: Pixels,
    pub window_shadow_blur: Pixels,
    pub window_contact_shadow: Rgba,
    pub window_contact_shadow_offset_y: Pixels,
    pub window_contact_shadow_blur: Pixels,
}

impl Theme {
    pub fn light() -> Self {
        Self {
            window_background: rgb(0xf4f4f2),
            window_border: rgb(0xb4b4ae),
            window_border_width: px(1.0),
            window_corner_radius: px(9.0),
            window_shadow: rgba(0x00000038),
            window_shadow_offset_y: px(12.0),
            window_shadow_blur: px(17.0),
            window_contact_shadow: rgba(0x0000001f),
            window_contact_shadow_offset_y: px(2.0),
            window_contact_shadow_blur: px(3.0),
        }
    }

    pub fn dark() -> Self {
        Self {
            window_background: rgb(0x1a1c1f),
            window_border: rgb(0x000000),
            window_border_width: px(1.0),
            window_corner_radius: px(9.0),
            window_shadow: rgba(0x00000038),
            window_shadow_offset_y: px(12.0),
            window_shadow_blur: px(17.0),
            window_contact_shadow: rgba(0x0000001f),
            window_contact_shadow_offset_y: px(2.0),
            window_contact_shadow_blur: px(3.0),
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
