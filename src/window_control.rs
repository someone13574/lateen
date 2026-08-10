use gpui::prelude::*;
use gpui::{App, ElementId, Pixels, Role, SharedString, Size, Window, div, px, svg};

use crate::theme::ActiveTheme;

type ClickHandler = dyn Fn(&mut Window, &mut App) + 'static;

#[derive(IntoElement)]
pub struct WindowControl {
    id: ElementId,
    icon: SharedString,
    icon_size: Size<Pixels>,
    label: SharedString,
    danger: bool,
    on_click: Box<ClickHandler>,
}

impl WindowControl {
    pub fn new(
        id: impl Into<ElementId>,
        icon: impl Into<SharedString>,
        icon_size: Size<Pixels>,
        label: impl Into<SharedString>,
        on_click: Box<ClickHandler>,
    ) -> Self {
        Self {
            id: id.into(),
            icon: icon.into(),
            icon_size,
            label: label.into(),
            danger: false,
            on_click,
        }
    }

    pub fn danger(mut self) -> Self {
        self.danger = true;
        self
    }
}

impl RenderOnce for WindowControl {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = *cx.theme();
        let (hover_bg, hover_fg) = if self.danger {
            (theme.titlebar_close_hover_bg, theme.titlebar_close_hover_fg)
        } else {
            (theme.titlebar_control_hover_bg, theme.titlebar_control_fg)
        };

        let group = self.label.clone();

        div()
            .id(self.id)
            .role(Role::Button)
            .aria_label(self.label)
            .group(group.clone())
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .w(px(28.0))
            .h(px(24.0))
            .rounded(px(4.0))
            .text_color(theme.titlebar_control_fg)
            .cursor_pointer()
            .hover(|style| style.bg(hover_bg).text_color(hover_fg))
            .child(
                svg()
                    .path(self.icon)
                    .w(self.icon_size.width)
                    .h(self.icon_size.height)
                    .flex_none()
                    .text_color(theme.titlebar_control_fg)
                    .group_hover(group, |style| style.text_color(hover_fg)),
            )
            .on_click(move |_event, window, cx| {
                cx.stop_propagation();
                (self.on_click)(window, cx)
            })
    }
}
