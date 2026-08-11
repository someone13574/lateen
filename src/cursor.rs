use std::panic::Location;

use gpui::{
    App, BorderStyle, Bounds, Corners, Edges, Element, ElementId, GlobalElementId, Hsla,
    InspectorElementId, IntoElement, LayoutId, Pixels, ScrollHandle, Style, Window, fill, point,
    px, quad, relative, size,
};

use crate::block::Block;
use crate::theme::ActiveTheme;

pub struct Cursor {
    column: Bounds<Pixels>,
    minute_of_day: f32,
    rail: Pixels,
    border: Pixels,
    horizontal: ScrollHandle,
    vertical: ScrollHandle,
}

impl Cursor {
    const THICKNESS: Pixels = px(2.0);
    const DOT_DIAMETER: Pixels = px(7.0);

    pub fn new(
        column: Bounds<Pixels>,
        minute_of_day: f32,
        rail: Pixels,
        border: Pixels,
        horizontal: &ScrollHandle,
        vertical: &ScrollHandle,
    ) -> Self {
        Self {
            column,
            minute_of_day,
            rail,
            border,
            horizontal: horizontal.clone(),
            vertical: vertical.clone(),
        }
    }

    fn snap(length: Pixels, scale: f32) -> Pixels {
        let device = f32::from(length) * scale;

        px((device.abs() - 0.5).ceil().copysign(device) / scale)
    }

    fn snap_thickness(length: Pixels, rail: Pixels, scale: f32) -> Pixels {
        let device = (f32::from(length) * scale).ceil().max(1.0);
        let rail = f32::from(rail) * scale;

        px((device + (device - rail).rem_euclid(2.0)) / scale)
    }

    fn top(&self, bounds: Bounds<Pixels>) -> Pixels {
        bounds.top()
            + self.column.top()
            + self.vertical.offset().y
            + self.column.size.height * (self.minute_of_day / Block::MINUTES_PER_DAY as f32)
    }
}

impl IntoElement for Cursor {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for Cursor {
    type RequestLayoutState = ();
    type PrepaintState = ();

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
        _bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        _window: &mut Window,
        _cx: &mut App,
    ) -> Self::PrepaintState {
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        _prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let scale = window.scale_factor();
        let snap = |length: Pixels| Self::snap(length, scale);

        let rail = snap(bounds.left() + self.rail);
        let right = bounds.left() + self.column.right() + self.horizontal.offset().x;
        if right <= rail {
            return;
        }

        let border = snap(self.border).max(px(1.0) / scale);
        let thickness = Self::snap_thickness(Self::THICKNESS, border, scale);
        let diameter = Self::snap_thickness(Self::DOT_DIAMETER, border, scale);

        let top = snap(self.top(bounds));
        let line = Bounds::from_corners(point(rail, top), point(snap(right), top + thickness));

        window.paint_quad(fill(line, cx.theme().cursor_fg));

        window.paint_quad(quad(
            Bounds::new(
                point(
                    rail - (border + diameter) / 2.0,
                    line.top() - (diameter - thickness) / 2.0,
                ),
                size(diameter, diameter),
            ),
            Corners::all(diameter / 2.0),
            cx.theme().cursor_fg,
            Edges::default(),
            Hsla::transparent_black(),
            BorderStyle::default(),
        ));
    }
}
