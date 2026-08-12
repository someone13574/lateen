use gpui::prelude::*;
use gpui::{
    App, ElementId, FontWeight, Pixels, Point, Role, SharedString, Size, Text, Window, div, point,
    px, size,
};

use crate::theme::ActiveTheme;

pub type ClickHandler = dyn Fn(&mut Window, &mut App) + 'static;

#[derive(IntoElement)]
pub struct Button {
    id: ElementId,
    label: SharedString,
    small: bool,
    filled: bool,
    bare: bool,
    stretch: bool,
    size: Option<Size<Pixels>>,
    padding: Option<Point<Pixels>>,
    on_click: Option<Box<ClickHandler>>,
}

impl Button {
    pub fn new(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            small: false,
            filled: false,
            bare: false,
            stretch: false,
            size: None,
            padding: None,
            on_click: None,
        }
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

    pub fn padding(mut self, horizontal: Pixels, vertical: Pixels) -> Self {
        self.padding = Some(point(horizontal, vertical));
        self
    }

    pub fn on_click(mut self, on_click: Box<ClickHandler>) -> Self {
        self.on_click = Some(on_click);
        self
    }
}

impl RenderOnce for Button {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
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
            .when(self.stretch, |button| button.flex_1().justify_center())
            .when(!self.stretch, |button| button.flex_none())
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
            .when(!self.filled && !self.bare, |button| {
                button
                    .border_color(cx.theme().button_border)
                    .bg(cx.theme().button_bg)
                    .hover(|style| style.bg(cx.theme().button_hover_bg))
            })
            .on_click(move |_event, window, cx| {
                cx.stop_propagation();

                if let Some(on_click) = &self.on_click {
                    on_click(window, cx);
                }
            })
            .child(Text::new_inaccessible(self.label))
    }
}
