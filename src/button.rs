use std::rc::Rc;

use gpui::prelude::*;
use gpui::{
    App, Div, ElementId, FocusHandle, FontWeight, KeyBinding, MouseButton, Pixels, Point, Rgba,
    Role, SharedString, Size, Text, Window, actions, div, point, px, size,
};

use crate::theme::ActiveTheme;

actions!(button, [FocusNext, FocusPrevious, Press]);

pub type ClickHandler = dyn Fn(&mut Window, &mut App) + 'static;

#[derive(Clone, Copy)]
pub enum Verdict {
    Affirm,
    Deny,
}

impl Verdict {
    fn hover(self, cx: &App) -> (Rgba, Rgba, Rgba) {
        let theme = cx.theme();

        match self {
            Self::Affirm => (theme.chip_bg, theme.chip_border, theme.link_fg),
            Self::Deny => (theme.danger_bg, theme.danger_border, theme.danger_fg),
        }
    }
}

#[derive(IntoElement)]
pub struct Button {
    id: ElementId,
    label: SharedString,
    small: bool,
    filled: bool,
    bare: bool,
    stretch: bool,
    chip: Option<(bool, Rgba)>,
    size: Option<Size<Pixels>>,
    padding: Option<Point<Pixels>>,
    verdict: Option<Verdict>,
    active: Option<(Rgba, Rgba)>,
    focus_handle: Option<FocusHandle>,
    on_click: Option<Rc<ClickHandler>>,
    on_press: Option<Box<ClickHandler>>,
    on_release: Option<Rc<ClickHandler>>,
}

impl Button {
    pub const KEY_CONTEXT: &'static str = "Button";

    pub fn init(cx: &mut App) {
        cx.bind_keys([
            KeyBinding::new("enter", Press, Some(Self::KEY_CONTEXT)),
            KeyBinding::new("space", Press, Some(Self::KEY_CONTEXT)),
            KeyBinding::new("tab", FocusNext, Some(Self::KEY_CONTEXT)),
            KeyBinding::new("shift-tab", FocusPrevious, Some(Self::KEY_CONTEXT)),
        ]);
    }

    pub fn new(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            small: false,
            filled: false,
            bare: false,
            stretch: false,
            chip: None,
            size: None,
            padding: None,
            verdict: None,
            active: None,
            focus_handle: None,
            on_click: None,
            on_press: None,
            on_release: None,
        }
    }

    pub fn verdict(mut self, verdict: Verdict) -> Self {
        self.verdict = Some(verdict);
        self
    }

    pub fn active(mut self, fill: Rgba, fg: Rgba) -> Self {
        self.active = Some((fill, fg));
        self
    }

    pub fn small(mut self) -> Self {
        self.small = true;
        self
    }

    pub fn text(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Self {
        let mut button = Self::new(id, label);
        button.bare = true;
        button
    }

    pub fn fixed(mut self, width: Pixels, height: Pixels) -> Self {
        self.size = Some(size(width, height));
        self
    }

    pub fn filled(mut self) -> Self {
        self.filled = true;
        self
    }

    pub fn stretch(mut self) -> Self {
        self.stretch = true;
        self
    }

    pub fn chip(mut self, selected: bool, unselected_fg: Rgba) -> Self {
        self.chip = Some((selected, unselected_fg));
        self
    }

    pub fn padding(mut self, horizontal: Pixels, vertical: Pixels) -> Self {
        self.padding = Some(point(horizontal, vertical));
        self
    }

    pub fn focus(mut self, focus_handle: &FocusHandle) -> Self {
        self.focus_handle = Some(focus_handle.clone());
        self
    }

    pub fn on_click(mut self, on_click: Box<ClickHandler>) -> Self {
        self.on_click = Some(Rc::from(on_click));
        self
    }

    pub fn on_press(mut self, on_press: Box<ClickHandler>) -> Self {
        self.on_press = Some(on_press);
        self
    }

    pub fn on_release(mut self, on_release: Box<ClickHandler>) -> Self {
        self.on_release = Some(Rc::from(on_release));
        self
    }

    fn ring(radius: Pixels, color: Rgba) -> Div {
        div()
            .absolute()
            .inset(px(-1.0))
            .border(px(2.0))
            .rounded(radius + px(1.0))
            .border_color(color)
    }
}

impl RenderOnce for Button {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let on_click = self.on_click.clone();
        let (radius, text_size) = if self.small {
            (px(4.0), px(11.0))
        } else {
            (px(5.0), px(11.5))
        };
        let bare = self.bare || self.size.is_some();
        let padding = self.padding.unwrap_or(match (bare, self.small) {
            (true, _) => point(px(0.0), px(0.0)),
            (false, true) => point(px(7.0), px(2.0)),
            (false, false) => point(px(10.0), px(4.0)),
        });

        div()
            .id(self.id)
            .role(Role::Button)
            .aria_label(self.label.clone())
            .flex()
            .items_center()
            .when(self.stretch, |button| button.flex_1())
            .when(!self.stretch, |button| button.flex_none())
            .when(self.stretch, |button| button.justify_center())
            .px(padding.x)
            .py(padding.y)
            .text_size(text_size)
            .cursor_pointer()
            .when_some(self.size, |button, size| {
                button
                    .w(size.width)
                    .h(size.height)
                    .justify_center()
                    .rounded(px(4.0))
                    .text_size(px(11.0))
                    .text_color(cx.theme().chip_fg)
            })
            .when(self.bare, |button| {
                button
                    .text_size(px(10.5))
                    .text_color(cx.theme().link_fg)
                    .hover(|style| style.underline())
            })
            .when(!self.bare, |button| button.border(px(1.0)))
            .when(self.size.is_none() && !self.bare, |button| {
                button.rounded(radius)
            })
            .when(self.filled, |button| {
                button
                    .border_color(cx.theme().accent_bg)
                    .bg(cx.theme().accent_bg)
                    .text_color(cx.theme().accent_fg)
                    .font_weight(FontWeight::SEMIBOLD)
                    .hover(|style| style.bg(cx.theme().accent_hover_bg))
            })
            .when(
                !self.filled && !self.bare && self.chip.is_none(),
                |button| {
                    button
                        .border_color(cx.theme().button_border)
                        .bg(cx.theme().button_bg)
                        .when(self.verdict.is_none(), |button| {
                            button.hover(|style| style.bg(cx.theme().button_hover_bg))
                        })
                },
            )
            .when_some(self.chip, |button, (selected, unselected_fg)| {
                let theme = *cx.theme();

                button
                    .border_color(if selected {
                        theme.chip_border
                    } else {
                        theme.button_border
                    })
                    .bg(if selected {
                        theme.chip_bg
                    } else {
                        theme.button_bg
                    })
                    .text_color(if selected {
                        theme.link_fg
                    } else {
                        unselected_fg
                    })
                    .when(!selected, |button| {
                        button.hover(|style| style.bg(theme.button_hover_bg))
                    })
            })
            .when_some(self.verdict, |button, verdict| {
                let (bg, border, fg) = verdict.hover(cx);

                button.hover(move |style| style.bg(bg).border_color(border).text_color(fg))
            })
            .when_some(self.active, |button, (fill, fg)| {
                button.border_color(fill).bg(fill).text_color(fg)
            })
            .when_some(self.on_press, |button, on_press| {
                button.on_mouse_down(MouseButton::Left, move |_event, window, cx| {
                    on_press(window, cx);
                })
            })
            .when_some(self.on_release, |button, on_release| {
                let outside = on_release.clone();

                button
                    .on_mouse_up(MouseButton::Left, move |_event, window, cx| {
                        on_release(window, cx);
                    })
                    .on_mouse_up_out(MouseButton::Left, move |_event, window, cx| {
                        outside(window, cx);
                    })
            })
            .when_some(self.focus_handle, |button, focus_handle| {
                let press = on_click.clone();

                button
                    .relative()
                    .key_context(Self::KEY_CONTEXT)
                    .track_focus(&focus_handle)
                    .on_action(move |_: &Press, window, cx| {
                        if let Some(press) = &press {
                            press(window, cx);
                        }
                    })
                    .on_action(|_: &FocusNext, window: &mut Window, cx: &mut App| {
                        window.focus_next(cx)
                    })
                    .on_action(|_: &FocusPrevious, window: &mut Window, cx: &mut App| {
                        window.focus_prev(cx)
                    })
                    .when(focus_handle.is_focused(window), |button| {
                        button
                            .border_color(cx.theme().input_ring)
                            .child(Self::ring(radius, cx.theme().input_ring))
                    })
            })
            .on_click(move |_event, window, cx| {
                cx.stop_propagation();

                if let Some(on_click) = &on_click {
                    on_click(window, cx);
                }
            })
            .child(Text::new_inaccessible(self.label))
    }
}
