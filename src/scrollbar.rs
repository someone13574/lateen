use std::panic::Location;

use gpui::prelude::*;
use gpui::{
    Along, App, Axis, BorderStyle, Bounds, Corners, DispatchPhase, Edges, Element, ElementId,
    Entity, GlobalElementId, Hitbox, HitboxBehavior, Hsla, InspectorElementId, LayoutId,
    MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels, Point, ScrollHandle, Style,
    Window, px, quad, relative, size,
};

use crate::theme::ActiveTheme;

#[derive(IntoElement)]
pub struct Scrollbar {
    id: ElementId,
    axis: Axis,
    scroll: ScrollHandle,
    yield_corner: bool,
}

impl Scrollbar {
    pub const THICKNESS: Pixels = px(10.0);
    const INSET: Pixels = px(2.5);
    const MIN_LENGTH: Pixels = px(24.0);
    const START_MARGIN: Pixels = px(4.0);
    const CORNER_MARGIN: Pixels = px(10.75);

    pub fn new(id: impl Into<ElementId>, axis: Axis, scroll: &ScrollHandle) -> Self {
        Self {
            id: id.into(),
            axis,
            scroll: scroll.clone(),
            yield_corner: false,
        }
    }

    pub fn yield_corner(mut self) -> Self {
        self.yield_corner = true;
        self
    }
}

impl RenderOnce for Scrollbar {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let state = window.use_keyed_state(self.id, cx, |_window, _cx| ScrollbarState::default());

        ScrollbarElement {
            axis: self.axis,
            scroll: self.scroll,
            yield_corner: self.yield_corner,
            state,
        }
    }
}

#[derive(Default)]
struct ScrollbarState {
    grab: Option<Pixels>,
    hovered: bool,
}

#[derive(Clone)]
struct ScrollbarElement {
    axis: Axis,
    scroll: ScrollHandle,
    yield_corner: bool,
    state: Entity<ScrollbarState>,
}

impl ScrollbarElement {
    fn track(&self, bounds: Bounds<Pixels>) -> Bounds<Pixels> {
        let end_margin = if self.yield_corner {
            Scrollbar::CORNER_MARGIN
        } else {
            Scrollbar::START_MARGIN
        };

        Bounds {
            origin: bounds
                .origin
                .apply_along(self.axis, |edge| edge + Scrollbar::START_MARGIN),
            size: bounds.size.apply_along(self.axis, |length| {
                length - Scrollbar::START_MARGIN - end_margin
            }),
        }
    }

    fn thumb(&self, track: Bounds<Pixels>) -> Option<Bounds<Pixels>> {
        let travel = self.scroll.max_offset().along(self.axis);
        let track_length = track.size.along(self.axis);
        if travel <= px(0.0) || track_length <= px(0.0) {
            return None;
        }

        let length =
            (track_length * (track_length / (track_length + travel))).max(Scrollbar::MIN_LENGTH);
        if length >= track_length {
            return None;
        }

        let scrolled = (-self.scroll.offset().along(self.axis)).clamp(px(0.0), travel);
        let start = (track_length - length) * (scrolled / travel);
        let across = self.axis.invert();

        Some(Bounds {
            origin: track
                .origin
                .apply_along(self.axis, |edge| edge + start)
                .apply_along(across, |edge| edge + Scrollbar::INSET),
            size: track
                .size
                .apply_along(self.axis, |_| length)
                .apply_along(across, |thickness| thickness - Scrollbar::INSET * 2.0),
        })
    }

    fn dragged_offset(
        &self,
        track: Bounds<Pixels>,
        thumb: Bounds<Pixels>,
        position: Point<Pixels>,
        grab: Pixels,
    ) -> Point<Pixels> {
        let room = track.size.along(self.axis) - thumb.size.along(self.axis);
        let start =
            (position.along(self.axis) - track.origin.along(self.axis) - grab).clamp(px(0.0), room);
        let scrolled = if room > px(0.0) {
            self.scroll.max_offset().along(self.axis) * (start / room)
        } else {
            px(0.0)
        };

        self.scroll.offset().apply_along(self.axis, |_| -scrolled)
    }

    fn paint_drag(
        &self,
        track: Bounds<Pixels>,
        thumb: Bounds<Pixels>,
        hitbox: Hitbox,
        window: &mut Window,
    ) {
        let view = window.current_view();

        window.on_mouse_event({
            let element = self.clone();
            let hitbox = hitbox.clone();

            move |event: &MouseDownEvent, phase, window, cx| {
                if phase != DispatchPhase::Bubble
                    || event.button != MouseButton::Left
                    || !hitbox.is_hovered(window)
                {
                    return;
                }

                let pressed = event.position.along(element.axis);
                let start = thumb.origin.along(element.axis);
                let grab = if (start..start + thumb.size.along(element.axis)).contains(&pressed) {
                    pressed - start
                } else {
                    let grab = thumb.size.along(element.axis) * 0.5;
                    element.scroll.set_offset(element.dragged_offset(
                        track,
                        thumb,
                        event.position,
                        grab,
                    ));
                    cx.notify(view);
                    grab
                };

                window.capture_pointer(hitbox.id);
                element.state.update(cx, |state, cx| {
                    state.grab = Some(grab);
                    cx.notify();
                });
                cx.stop_propagation();
            }
        });

        window.on_mouse_event({
            let element = self.clone();
            let hitbox = hitbox.clone();

            move |event: &MouseMoveEvent, phase, window, cx| {
                if phase != DispatchPhase::Bubble {
                    return;
                }

                match element.state.read(cx).grab {
                    Some(grab) if event.dragging() => {
                        element.scroll.set_offset(element.dragged_offset(
                            track,
                            thumb,
                            event.position,
                            grab,
                        ));
                        cx.notify(view);
                        cx.stop_propagation();
                    }
                    Some(_) => {}
                    None => {
                        let hovered = hitbox.is_hovered(window);
                        if hovered != element.state.read(cx).hovered {
                            element.state.update(cx, |state, cx| {
                                state.hovered = hovered;
                                cx.notify();
                            });
                        }
                    }
                }
            }
        });

        window.on_mouse_event({
            let state = self.state.clone();

            move |event: &MouseUpEvent, phase, _window, cx| {
                if phase != DispatchPhase::Bubble || state.read(cx).grab.is_none() {
                    return;
                }

                let hovered = hitbox.bounds.contains(&event.position);
                state.update(cx, |state, cx| {
                    state.grab = None;
                    state.hovered = hovered;
                    cx.notify();
                });
            }
        });
    }
}

impl IntoElement for ScrollbarElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for ScrollbarElement {
    type RequestLayoutState = ();
    type PrepaintState = Option<(Hitbox, Bounds<Pixels>, Bounds<Pixels>)>;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let style = Style {
            size: size(relative(1.0).into(), relative(1.0).into()),
            ..Default::default()
        };

        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        _cx: &mut App,
    ) -> Self::PrepaintState {
        let track = self.track(bounds);
        let thumb = self.thumb(track)?;

        Some((
            window.insert_hitbox(track, HitboxBehavior::Normal),
            track,
            thumb,
        ))
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let Some((hitbox, track, thumb)) = prepaint.clone() else {
            if self.state.read(cx).grab.is_some() || self.state.read(cx).hovered {
                self.state.update(cx, |state, cx| {
                    *state = ScrollbarState::default();
                    cx.notify();
                });
            }
            return;
        };

        let state = self.state.read(cx);
        let color = if state.grab.is_some() || state.hovered {
            cx.theme().scrollbar_thumb_hover
        } else {
            cx.theme().scrollbar_thumb
        };

        window.paint_quad(quad(
            thumb,
            Corners::all(thumb.size.along(self.axis.invert()) * 0.5),
            color,
            Edges::default(),
            Hsla::transparent_black(),
            BorderStyle::default(),
        ));

        self.paint_drag(track, thumb, hitbox, window);
    }
}
