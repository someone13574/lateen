use gpui::{
    App, ElementId, InteractiveElement, IntoElement, ParentElement, RenderOnce, Role, SharedString,
    StatefulInteractiveElement, Styled, Text, Window, div, px,
};

use crate::theme::ActiveTheme;

#[derive(IntoElement)]
pub struct Button {
    id: ElementId,
    label: SharedString,
}

impl Button {
    pub fn new(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
        }
    }
}

impl RenderOnce for Button {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        div()
            .id(self.id)
            .role(Role::Button)
            .aria_label(self.label.clone())
            .flex()
            .flex_none()
            .items_center()
            .px(px(10.0))
            .py(px(4.0))
            .rounded(px(5.0))
            .border(px(1.0))
            .border_color(cx.theme().button_border)
            .bg(cx.theme().button_bg)
            .text_size(px(11.5))
            .cursor_pointer()
            .hover(|style| style.bg(cx.theme().button_hover_bg))
            .on_click(|_event, _window, cx| cx.stop_propagation())
            .child(Text::new_inaccessible(self.label))
    }
}
