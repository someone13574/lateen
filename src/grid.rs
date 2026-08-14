use std::panic::Location;

use gpui::{
    App, BorderStyle, Bounds, Corners, Edges, Element, ElementId, GlobalElementId, Hsla,
    InspectorElementId, IntoElement, LayoutId, Pixels, Style, Window, fill, point, px, quad, size,
};

use crate::theme::ActiveTheme;

pub struct Grid {
    days: usize,
    day_height: Pixels,
    corners: Corners<Pixels>,
}

impl Grid {
    pub const COLUMN_WIDTH: Pixels = px(218.0);
    pub const GUIDE_WIDTH: Pixels = px(1.0);

    pub fn new(days: usize, day_height: Pixels, corners: Corners<Pixels>) -> Self {
        Self {
            days,
            day_height,
            corners,
        }
    }

    fn paint_hour_line(
        &self,
        bounds: Bounds<Pixels>,
        visible: Bounds<Pixels>,
        hour: usize,
        window: &mut Window,
        cx: &App,
    ) {
        let top = bounds.origin.y + self.day_height * (hour as f32 / 24.0);
        if top + Self::GUIDE_WIDTH <= visible.top() || top >= visible.bottom() {
            return;
        }

        let above_bottom = visible.bottom() - top - Self::GUIDE_WIDTH;
        let left = visible.left() + Self::corner_inset(above_bottom, self.corners.bottom_left);
        let right = visible.right() - Self::corner_inset(above_bottom, self.corners.bottom_right);

        window.paint_quad(fill(
            Bounds::from_corners(point(left, top), point(right, top + Self::GUIDE_WIDTH)),
            cx.theme().grid_hour_line,
        ));
    }

    fn paint_day_separator(
        &self,
        bounds: Bounds<Pixels>,
        visible: Bounds<Pixels>,
        day: usize,
        window: &mut Window,
        cx: &App,
    ) {
        let right = bounds.origin.x + Self::COLUMN_WIDTH * (day + 1);
        let left = right - Self::GUIDE_WIDTH;
        if right <= visible.left() || left >= visible.right() {
            return;
        }

        let bottom = visible.bottom()
            - Self::corner_inset(visible.right() - right, self.corners.bottom_right).max(
                Self::corner_inset(left - visible.left(), self.corners.bottom_left),
            );

        window.paint_quad(fill(
            Bounds::from_corners(point(left, visible.top()), point(right, bottom)),
            cx.theme().grid_day_border,
        ));
    }

    fn corner_inset(distance_from_corner: Pixels, radius: Pixels) -> Pixels {
        let distance_from_corner = distance_from_corner.max(px(0.0));
        if distance_from_corner >= radius {
            return px(0.0);
        }

        let straight = f32::from(radius) - f32::from(distance_from_corner);
        radius - px((f32::from(radius).powi(2) - straight.powi(2)).sqrt())
    }
}

impl IntoElement for Grid {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for Grid {
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
            flex_shrink: 0.0,
            size: size(
                (Self::COLUMN_WIDTH * self.days).into(),
                self.day_height.into(),
            ),
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
        let visible = window.content_mask().bounds;

        window.paint_quad(quad(
            visible,
            self.corners,
            cx.theme().grid_bg,
            Edges::default(),
            Hsla::transparent_black(),
            BorderStyle::default(),
        ));

        for hour in 0..24 {
            self.paint_hour_line(bounds, visible, hour, window, cx);
        }

        for day in 0..self.days {
            self.paint_day_separator(bounds, visible, day, window, cx);
        }
    }
}
