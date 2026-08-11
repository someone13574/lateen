use gpui::prelude::*;
use gpui::{
    App, Bounds, BoxShadow, CursorStyle, Decorations, Div, MouseButton, Pixels, ResizeEdge, Tiling,
    Window, div, point, px, size,
};

use crate::theme::ActiveTheme;

#[derive(IntoElement)]
pub struct WindowFrame<C: IntoElement + 'static> {
    content: C,
}

impl<C: IntoElement + 'static> WindowFrame<C> {
    const RESIZE_HANDLE_SIZE: Pixels = px(9.0);
    const RESIZE_CORNER_REACH: Pixels = px(9.0);
    const SHADOW_OFFSET_Y: Pixels = px(12.0);
    const SHADOW_BLUR: Pixels = px(17.0);
    const CONTACT_SHADOW_OFFSET_Y: Pixels = px(2.0);
    const CONTACT_SHADOW_BLUR: Pixels = px(3.0);

    pub fn new(content: C) -> Self {
        Self { content }
    }

    fn client_inset() -> Pixels {
        let reach = |offset_y: Pixels, blur: Pixels| offset_y + blur * 3.0;

        reach(Self::SHADOW_OFFSET_Y, Self::SHADOW_BLUR)
            .max(reach(
                Self::CONTACT_SHADOW_OFFSET_Y,
                Self::CONTACT_SHADOW_BLUR,
            ))
            .max(Self::RESIZE_HANDLE_SIZE)
    }

    fn resize_cursor(edge: ResizeEdge) -> CursorStyle {
        match edge {
            ResizeEdge::Top | ResizeEdge::Bottom => CursorStyle::ResizeUpDown,
            ResizeEdge::Left | ResizeEdge::Right => CursorStyle::ResizeLeftRight,
            ResizeEdge::TopLeft | ResizeEdge::BottomRight => CursorStyle::ResizeUpLeftDownRight,
            ResizeEdge::TopRight | ResizeEdge::BottomLeft => CursorStyle::ResizeUpRightDownLeft,
        }
    }

    fn resize_handle(edge: ResizeEdge) -> Div {
        div()
            .absolute()
            .cursor(Self::resize_cursor(edge))
            .on_mouse_down(MouseButton::Left, move |_event, window, _cx| {
                window.start_window_resize(edge);
            })
    }

    fn corner_handles(edge: ResizeEdge, place: impl Fn(Div, Pixels) -> Div) -> [Div; 2] {
        [
            place(Self::resize_handle(edge), px(0.0))
                .w(Self::RESIZE_HANDLE_SIZE + Self::RESIZE_CORNER_REACH)
                .h(Self::RESIZE_HANDLE_SIZE),
            place(Self::resize_handle(edge), Self::RESIZE_HANDLE_SIZE)
                .w(Self::RESIZE_HANDLE_SIZE)
                .h(Self::RESIZE_CORNER_REACH),
        ]
    }
}

impl<C: IntoElement + 'static> RenderOnce for WindowFrame<C> {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let client_inset = Self::client_inset();

        let tiling = match window.window_decorations() {
            Decorations::Client { tiling } => {
                window.set_client_inset(client_inset);
                tiling
            }
            Decorations::Server => Tiling::tiled(),
        };

        let inset = |tiled| if tiled { px(0.0) } else { client_inset };
        let (top_inset, bottom_inset) = (inset(tiling.top), inset(tiling.bottom));
        let (left_inset, right_inset) = (inset(tiling.left), inset(tiling.right));

        let handle_offset = |tiled| (inset(tiled) - Self::RESIZE_HANDLE_SIZE).max(px(0.0));
        let (top_handle, bottom_handle) = (handle_offset(tiling.top), handle_offset(tiling.bottom));
        let (left_handle, right_handle) = (handle_offset(tiling.left), handle_offset(tiling.right));

        let corner_clearance = |tiled| {
            if tiled {
                px(0.0)
            } else {
                client_inset + Self::RESIZE_CORNER_REACH
            }
        };
        let (top_clearance, bottom_clearance) = (
            corner_clearance(tiling.top),
            corner_clearance(tiling.bottom),
        );
        let (left_clearance, right_clearance) = (
            corner_clearance(tiling.left),
            corner_clearance(tiling.right),
        );

        let viewport = window.viewport_size();
        window.set_input_region(Some(&[Bounds {
            origin: point(left_handle, top_handle),
            size: size(
                viewport.width - left_handle - right_handle,
                viewport.height - top_handle - bottom_handle,
            ),
        }]));

        div()
            .size_full()
            .pt(top_inset)
            .pb(bottom_inset)
            .pl(left_inset)
            .pr(right_inset)
            .child(
                div()
                    .size_full()
                    .overflow_hidden()
                    .bg(cx.theme().window_bg)
                    .border_color(cx.theme().window_border)
                    .when(!tiling.top, |body| body.border_t(px(1.0)))
                    .when(!tiling.bottom, |body| body.border_b(px(1.0)))
                    .when(!tiling.left, |body| body.border_l(px(1.0)))
                    .when(!tiling.right, |body| body.border_r(px(1.0)))
                    .when(!tiling.top && !tiling.left, |body| body.rounded_tl(px(9.0)))
                    .when(!tiling.top && !tiling.right, |body| {
                        body.rounded_tr(px(9.0))
                    })
                    .when(!tiling.bottom && !tiling.left, |body| {
                        body.rounded_bl(px(9.0))
                    })
                    .when(!tiling.bottom && !tiling.right, |body| {
                        body.rounded_br(px(9.0))
                    })
                    .when(!tiling.is_tiled(), |body| {
                        body.shadow(vec![
                            BoxShadow::new(
                                px(0.0),
                                Self::SHADOW_OFFSET_Y,
                                cx.theme().window_shadow.into(),
                            )
                            .blur_radius(Self::SHADOW_BLUR),
                            BoxShadow::new(
                                px(0.0),
                                Self::CONTACT_SHADOW_OFFSET_Y,
                                cx.theme().window_contact_shadow.into(),
                            )
                            .blur_radius(Self::CONTACT_SHADOW_BLUR),
                        ])
                    })
                    .child(self.content),
            )
            .when(!tiling.top, |backdrop| {
                backdrop.child(
                    Self::resize_handle(ResizeEdge::Top)
                        .top(top_handle)
                        .left(left_clearance)
                        .right(right_clearance)
                        .h(Self::RESIZE_HANDLE_SIZE),
                )
            })
            .when(!tiling.bottom, |backdrop| {
                backdrop.child(
                    Self::resize_handle(ResizeEdge::Bottom)
                        .bottom(bottom_handle)
                        .left(left_clearance)
                        .right(right_clearance)
                        .h(Self::RESIZE_HANDLE_SIZE),
                )
            })
            .when(!tiling.left, |backdrop| {
                backdrop.child(
                    Self::resize_handle(ResizeEdge::Left)
                        .left(left_handle)
                        .top(top_clearance)
                        .bottom(bottom_clearance)
                        .w(Self::RESIZE_HANDLE_SIZE),
                )
            })
            .when(!tiling.right, |backdrop| {
                backdrop.child(
                    Self::resize_handle(ResizeEdge::Right)
                        .right(right_handle)
                        .top(top_clearance)
                        .bottom(bottom_clearance)
                        .w(Self::RESIZE_HANDLE_SIZE),
                )
            })
            .when(!tiling.top && !tiling.left, |backdrop| {
                backdrop.children(Self::corner_handles(
                    ResizeEdge::TopLeft,
                    |handle, along_edge| handle.top(top_handle + along_edge).left(left_handle),
                ))
            })
            .when(!tiling.top && !tiling.right, |backdrop| {
                backdrop.children(Self::corner_handles(
                    ResizeEdge::TopRight,
                    |handle, along_edge| handle.top(top_handle + along_edge).right(right_handle),
                ))
            })
            .when(!tiling.bottom && !tiling.left, |backdrop| {
                backdrop.children(Self::corner_handles(
                    ResizeEdge::BottomLeft,
                    |handle, along_edge| {
                        handle.bottom(bottom_handle + along_edge).left(left_handle)
                    },
                ))
            })
            .when(!tiling.bottom && !tiling.right, |backdrop| {
                backdrop.children(Self::corner_handles(
                    ResizeEdge::BottomRight,
                    |handle, along_edge| {
                        handle
                            .bottom(bottom_handle + along_edge)
                            .right(right_handle)
                    },
                ))
            })
    }
}
