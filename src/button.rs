use gpui::{
    App, ElementId, InteractiveElement, IntoElement, ParentElement, RenderOnce, Role, SharedString,
    StatefulInteractiveElement, Styled, Text, Window, div, px,
};

use crate::theme::ActiveTheme;

type ClickHandler = dyn Fn(&mut Window, &mut App) + 'static;

#[derive(IntoElement)]
pub struct Button {
    id: ElementId,
    label: SharedString,
    small: bool,
    on_click: Option<Box<ClickHandler>>,
}

impl Button {
    pub fn new(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            small: false,
            on_click: None,
        }
    }

    pub fn small(mut self) -> Self {
        self.small = true;
        self
    }

    pub fn on_click(mut self, on_click: Box<ClickHandler>) -> Self {
        self.on_click = Some(on_click);
        self
    }
}

impl RenderOnce for Button {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let (padding_x, padding_y, radius, text_size) = if self.small {
            (px(7.0), px(2.0), px(4.0), px(11.0))
        } else {
            (px(10.0), px(4.0), px(5.0), px(11.5))
        };

        div()
            .id(self.id)
            .role(Role::Button)
            .aria_label(self.label.clone())
            .flex()
            .flex_none()
            .items_center()
            .px(padding_x)
            .py(padding_y)
            .rounded(radius)
            .border(px(1.0))
            .border_color(cx.theme().button_border)
            .bg(cx.theme().button_bg)
            .text_size(text_size)
            .cursor_pointer()
            .hover(|style| style.bg(cx.theme().button_hover_bg))
            .on_click(move |_event, window, cx| {
                cx.stop_propagation();

                if let Some(on_click) = &self.on_click {
                    on_click(window, cx);
                }
            })
            .child(Text::new_inaccessible(self.label))
    }
}
