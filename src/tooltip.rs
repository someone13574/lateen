use std::cell::RefCell;
use std::panic::Location;
use std::rc::Rc;
use std::time::{Duration, Instant};

use gpui::prelude::*;
use gpui::{
    AnyElement, AnyTooltip, AnyView, App, AvailableSpace, Bounds, BoxShadow, Element, ElementId,
    GlobalElementId, Hitbox, HitboxBehavior, InspectorElementId, IntoElement, LayoutId,
    MouseMoveEvent, Pixels, Point, TooltipId, Window, div, point, px, relative, size,
};

use crate::theme::ActiveTheme;

type TooltipContent = dyn Fn(&mut Window, &mut App) -> AnyElement;

pub type TooltipBuilder = dyn Fn(&mut Window, &mut App) -> AnyView;

pub struct Tooltip {
    content: Rc<TooltipContent>,
}

struct PlacedTooltip {
    view: AnyView,
    right_padding: Pixels,
    bottom_padding: Pixels,
}

impl Tooltip {
    const MAX_WIDTH: Pixels = px(288.0);

    pub fn element(
        content: impl Fn(&mut Window, &mut App) -> AnyElement + 'static,
    ) -> Box<TooltipBuilder> {
        let content = Rc::new(content);

        Box::new(move |_window, cx| {
            let content = content.clone();

            cx.new(|_cx| Self { content }).into()
        })
    }
}

impl Render for PlacedTooltip {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .pr(self.right_padding)
            .pb(self.bottom_padding)
            .child(self.view.clone())
    }
}

impl Render for Tooltip {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *cx.theme();
        let content = self.content.clone();

        div().pl(px(8.0)).pt(px(10.0)).child(
            div()
                .occlude()
                .max_w(Self::MAX_WIDTH)
                .px(px(8.0))
                .py(px(5.0))
                .rounded(px(4.0))
                .border(px(1.0))
                .border_color(theme.button_border)
                .bg(theme.card_bg)
                .shadow(vec![
                    BoxShadow::new(px(0.0), px(4.0), theme.window_shadow.into())
                        .blur_radius(px(10.0)),
                ])
                .font_family("Inter")
                .line_height(relative(1.21))
                .text_size(px(11.5))
                .text_color(theme.fg)
                .child(content(window, cx)),
        )
    }
}

pub struct Tooltipped {
    id: ElementId,
    child: AnyElement,
    build: Rc<TooltipBuilder>,
}

#[derive(Default)]
pub struct HoverTracking {
    inside: bool,
    spent: bool,
    settled: Option<(Instant, Point<Pixels>)>,
    anchor: Option<Point<Pixels>>,
    view: Option<AnyView>,
    strayed: Option<Instant>,
    id: Option<TooltipId>,
    watching: bool,
    bounds: Option<Bounds<Pixels>>,
}

impl HoverTracking {
    const SHOW_DELAY: Duration = Duration::from_millis(500);
    const HIDE_DELAY: Duration = Duration::from_millis(150);
    const TICK: Duration = Duration::from_millis(40);
    const JITTER: Pixels = px(3.0);

    fn strayed_from(anchor: Point<Pixels>, position: Point<Pixels>) -> bool {
        (position.x - anchor.x).abs() > Self::JITTER || (position.y - anchor.y).abs() > Self::JITTER
    }

    fn moved(&mut self, inside: bool, position: Point<Pixels>) {
        if self.inside != inside {
            self.inside = inside;

            if !inside {
                self.settled = None;
                if self.anchor.is_none() {
                    self.spent = false;
                }
            }
        }

        if inside && self.anchor.is_none() && !self.spent {
            let restart = match self.settled {
                Some((_, at)) => Self::strayed_from(at, position),
                None => true,
            };

            if restart {
                self.settled = Some((Instant::now(), position));
            }
        }

        if let Some(anchor) = self.anchor {
            match Self::strayed_from(anchor, position) {
                true => drop(self.strayed.get_or_insert_with(Instant::now)),
                false => self.strayed = None,
            }
        }
    }

    fn ready(&self) -> bool {
        self.inside
            && self.anchor.is_none()
            && self
                .settled
                .is_some_and(|(since, _)| since.elapsed() >= Self::SHOW_DELAY)
    }

    fn stale(&self) -> bool {
        self.anchor.is_some()
            && self
                .strayed
                .is_some_and(|since| since.elapsed() >= Self::HIDE_DELAY)
    }

    fn resting(&self) -> bool {
        self.anchor.is_none() && self.settled.is_none()
    }

    fn show(&mut self, view: AnyView, position: Point<Pixels>) {
        self.anchor = Some(position);
        self.view = Some(view);
        self.settled = None;
        self.strayed = None;
        self.spent = true;
    }

    fn hide(&mut self) {
        self.anchor = None;
        self.view = None;
        self.strayed = None;
    }

    fn hide_after_stray(&mut self) {
        self.hide();
        if !self.inside {
            self.spent = false;
        }
    }

    fn clear(&mut self) -> bool {
        let active = self.anchor.is_some()
            || self.view.is_some()
            || self.settled.is_some()
            || self.strayed.is_some();

        self.inside = false;
        self.spent = false;
        self.settled = None;
        self.anchor = None;
        self.view = None;
        self.strayed = None;

        active
    }

    fn update_bounds(&mut self, bounds: Bounds<Pixels>) -> bool {
        let moved = self.bounds.is_some_and(|previous| previous != bounds);
        self.bounds = Some(bounds);

        moved && self.clear()
    }
}

impl Tooltipped {
    pub fn new(
        id: impl Into<ElementId>,
        child: impl IntoElement,
        build: Box<TooltipBuilder>,
    ) -> Self {
        Self {
            id: id.into(),
            child: child.into_any_element(),
            build: build.into(),
        }
    }

    fn watch(
        state: &Rc<RefCell<HoverTracking>>,
        build: &Rc<TooltipBuilder>,
        window: &mut Window,
        cx: &mut App,
    ) {
        if state.borrow().watching {
            return;
        }

        state.borrow_mut().watching = true;

        let weak = Rc::downgrade(state);
        let build = build.clone();

        window
            .spawn(cx, async move |cx| {
                loop {
                    cx.background_executor().timer(HoverTracking::TICK).await;

                    let Some(state) = weak.upgrade() else {
                        return;
                    };

                    let resting = cx
                        .update(|window, cx| Self::settle(&state, &build, window, cx))
                        .unwrap_or(true);

                    if resting {
                        state.borrow_mut().watching = false;
                        return;
                    }
                }
            })
            .detach();
    }

    fn settle(
        state: &Rc<RefCell<HoverTracking>>,
        build: &Rc<TooltipBuilder>,
        window: &mut Window,
        cx: &mut App,
    ) -> bool {
        let tracking = state.borrow();

        if tracking.ready() {
            let position = tracking.settled.map(|(_, at)| at).unwrap_or_default();

            drop(tracking);

            let view = build(window, cx);

            state.borrow_mut().show(view, position);
            window.refresh();

            return false;
        }

        if tracking.stale() && !tracking.id.is_some_and(|id| id.is_hovered(window)) {
            drop(tracking);

            state.borrow_mut().hide_after_stray();
            window.refresh();

            return false;
        }

        tracking.resting()
    }

    fn request(state: &Rc<RefCell<HoverTracking>>, window: &mut Window, cx: &mut App) {
        let Some(view) = state.borrow().view.clone() else {
            return;
        };
        let anchor = state.borrow().anchor.unwrap_or_default();
        let tooltip_size =
            view.clone()
                .into_any_element()
                .layout_as_root(AvailableSpace::min_size(), window, cx);

        let inset = window.client_inset().unwrap_or(Pixels::ZERO);
        let visual_bounds = Bounds {
            origin: point(inset, inset),
            size: window.viewport_size() - size(inset * 2.0, inset * 2.0),
        };
        let force_left = anchor.x + px(1.0) + tooltip_size.width > visual_bounds.right()
            && anchor.x - tooltip_size.width - px(1.0) >= visual_bounds.left();
        let force_up = anchor.y + px(1.0) + tooltip_size.height > visual_bounds.bottom()
            && anchor.y - tooltip_size.height - px(1.0) >= visual_bounds.top();
        let compensated_anchor = anchor
            + point(
                if force_left { inset } else { px(0.0) },
                if force_up { inset } else { px(0.0) },
            );
        let view = if force_left || force_up {
            cx.new(|_cx| PlacedTooltip {
                view,
                right_padding: if force_left { inset } else { px(0.0) },
                bottom_padding: if force_up { inset } else { px(0.0) },
            })
            .into()
        } else {
            view
        };

        let id = window.set_tooltip(AnyTooltip {
            view,
            mouse_position: compensated_anchor,
            check_visible_and_update: Rc::new(|_bounds, _window, _cx| true),
        });

        state.borrow_mut().id = Some(id);
    }
}

impl IntoElement for Tooltipped {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for Tooltipped {
    type RequestLayoutState = ();
    type PrepaintState = (Hitbox, Rc<RefCell<HoverTracking>>);

    fn id(&self) -> Option<ElementId> {
        Some(self.id.clone())
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
    ) -> (LayoutId, ()) {
        (self.child.request_layout(window, cx), ())
    }

    fn prepaint(
        &mut self,
        id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        self.child.prepaint(window, cx);

        let hitbox = window.insert_hitbox(bounds, HitboxBehavior::Normal);
        let Some(id) = id else {
            return (hitbox, Rc::default());
        };

        let state =
            window.with_element_state(id, |state: Option<Rc<RefCell<HoverTracking>>>, _| {
                let state = state.unwrap_or_default();

                (state.clone(), state)
            });

        state.borrow_mut().update_bounds(bounds);
        Self::request(&state, window, cx);

        (hitbox, state)
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _request: &mut (),
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        self.child.paint(window, cx);

        let (hitbox, state) = prepaint.clone();
        let build = self.build.clone();

        let move_state = state.clone();
        window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
            if !phase.bubble() {
                return;
            }

            let inside = hitbox.is_hovered(window);

            move_state.borrow_mut().moved(inside, event.position);

            let resting = move_state.borrow().resting();

            if !resting {
                Self::watch(&move_state, &build, window, cx);
            }
        });
    }
}
