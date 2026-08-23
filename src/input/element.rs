use std::ops::Range;
use std::panic::Location;

use gpui::prelude::*;
use gpui::{
    Action, App, Bounds, ContentMask, Context, CursorStyle, DispatchPhase, Element, ElementId,
    ElementInputHandler, Entity, GlobalElementId, InspectorElementId, LayoutId, MouseButton,
    MouseMoveEvent, PaintQuad, Pixels, Point, ShapedLine, SharedString, Style, TextAlign, TextRun,
    UnderlineStyle, Window, div, fill, point, px, relative, size,
};

use crate::input::{
    Backspace, Copy, Cut, Delete, End, Enter, FocusNext, FocusPrevious, Home, InputState, Left,
    Paste, Redo, Right, SelectAll, SelectEnd, SelectHome, SelectLeft, SelectRight, Undo,
};
use crate::theme::ActiveTheme;

type ActionHandler = fn(&mut InputState, &mut Window, &mut Context<InputState>);

#[derive(IntoElement)]
pub struct Input {
    state: Entity<InputState>,
    placeholder: SharedString,
    text_size: Pixels,
    padding: Point<Pixels>,
}

impl Input {
    pub fn new(state: Entity<InputState>) -> Self {
        Self {
            state,
            placeholder: SharedString::default(),
            text_size: px(12.0),
            padding: point(px(6.0), px(4.0)),
        }
    }

    pub fn text_size(mut self, text_size: Pixels) -> Self {
        self.text_size = text_size;
        self
    }

    pub fn padding(mut self, horizontal: Pixels, vertical: Pixels) -> Self {
        self.padding = point(horizontal, vertical);
        self
    }

    fn action<A: Action>(
        &self,
        handler: ActionHandler,
    ) -> impl Fn(&A, &mut Window, &mut App) + use<A> {
        let state = self.state.clone();

        move |_action, window, cx| state.update(cx, |state, cx| handler(state, window, cx))
    }

    fn mouse<E: 'static>(
        &self,
        handler: fn(&mut InputState, &E, &mut Context<InputState>),
    ) -> impl Fn(&E, &mut Window, &mut App) + use<E> {
        let state = self.state.clone();

        move |event, _window, cx| state.update(cx, |state, cx| handler(state, event, cx))
    }
}

impl RenderOnce for Input {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let focus_handle = self.state.read(cx).focus_handle.clone();
        let focused = focus_handle.is_focused(window);
        let invalid = self.state.read(cx).invalid;

        div()
            .key_context(InputState::KEY_CONTEXT)
            .track_focus(&focus_handle)
            .relative()
            .flex()
            .w_full()
            .px(self.padding.x)
            .py(self.padding.y)
            .border(px(1.0))
            .rounded(px(4.0))
            .bg(cx.theme().input_bg)
            .border_color(match (invalid, focused) {
                (true, _) => cx.theme().danger_fg,
                (false, true) => cx.theme().input_focus_border,
                (false, false) => cx.theme().input_border,
            })
            .text_size(self.text_size)
            .text_color(cx.theme().fg)
            .cursor(CursorStyle::IBeam)
            .on_action(self.action::<Left>(InputState::left))
            .on_action(self.action::<Right>(InputState::right))
            .on_action(self.action::<SelectLeft>(InputState::select_left))
            .on_action(self.action::<SelectRight>(InputState::select_right))
            .on_action(self.action::<Home>(InputState::home))
            .on_action(self.action::<End>(InputState::end))
            .on_action(self.action::<SelectHome>(InputState::select_home))
            .on_action(self.action::<SelectEnd>(InputState::select_end))
            .on_action(self.action::<SelectAll>(InputState::select_all))
            .on_action(self.action::<Backspace>(InputState::backspace))
            .on_action(self.action::<Delete>(InputState::delete))
            .on_action(self.action::<Copy>(InputState::copy))
            .on_action(self.action::<Cut>(InputState::cut))
            .on_action(self.action::<Paste>(InputState::paste))
            .on_action(self.action::<Enter>(InputState::enter))
            .on_action(self.action::<FocusNext>(InputState::focus_next))
            .on_action(self.action::<FocusPrevious>(InputState::focus_previous))
            .on_action(self.action::<Undo>(InputState::undo))
            .on_action(self.action::<Redo>(InputState::redo))
            .on_mouse_down(MouseButton::Left, self.mouse(InputState::on_mouse_down))
            .on_mouse_up(MouseButton::Left, self.mouse(InputState::on_mouse_up))
            .on_mouse_up_out(MouseButton::Left, self.mouse(InputState::on_mouse_up))
            .on_mouse_down_out({
                let focus_handle = focus_handle.clone();

                move |_event, window, _cx| {
                    if focus_handle.is_focused(window) {
                        window.blur();
                    }
                }
            })
            .child(InputElement {
                state: self.state,
                placeholder: self.placeholder,
            })
            .when(focused || invalid, |input| {
                input.child(
                    div()
                        .absolute()
                        .inset(px(1.0))
                        .border(px(2.0))
                        .rounded(px(3.0))
                        .border_color(if invalid {
                            cx.theme().danger_border
                        } else {
                            cx.theme().input_ring
                        }),
                )
            })
    }
}

struct InputElement {
    state: Entity<InputState>,
    placeholder: SharedString,
}

struct InputElementLayout {
    line: ShapedLine,
    selection: Option<PaintQuad>,
    cursor: Option<PaintQuad>,
}

impl InputElement {
    fn shape(&self, window: &mut Window, cx: &App) -> ShapedLine {
        let state = self.state.read(cx);
        let style = window.text_style();
        let (text, color, marked_range) = if state.content.is_empty() {
            let placeholder = cx.theme().input_placeholder_fg;
            (self.placeholder.clone(), placeholder.into(), None)
        } else {
            (
                state.content.clone(),
                style.color,
                state.marked_range.as_ref(),
            )
        };

        let run = TextRun {
            len: text.len(),
            font: style.font(),
            color,
            background_color: None,
            underline: None,
            strikethrough: None,
            letter_spacing: style.letter_spacing,
        };
        let runs = Self::runs(text.len(), marked_range, run);
        let font_size = style.font_size.to_pixels(window.rem_size());

        window
            .text_system()
            .shape_line(text, font_size, &runs, None)
    }

    fn runs(len: usize, marked_range: Option<&Range<usize>>, run: TextRun) -> Vec<TextRun> {
        let Some(marked) = marked_range else {
            return vec![run];
        };

        let underline = UnderlineStyle {
            color: Some(run.color),
            thickness: px(1.0),
            wavy: false,
        };

        [
            TextRun {
                len: marked.start,
                ..run.clone()
            },
            TextRun {
                len: marked.end - marked.start,
                underline: Some(underline),
                ..run.clone()
            },
            TextRun {
                len: len - marked.end,
                ..run
            },
        ]
        .into_iter()
        .filter(|run| run.len > 0)
        .collect()
    }

    fn scroll(&self, cursor: Pixels, line_width: Pixels, width: Pixels, cx: &mut App) -> Pixels {
        self.state.update(cx, |state, _cx| {
            state.scroll_offset = state
                .scroll_offset
                .clamp(-cursor, width - cursor)
                .clamp((width - line_width).min(px(0.0)), px(0.0));

            state.scroll_offset
        })
    }

    fn paint_contents(
        &self,
        layout: &mut InputElementLayout,
        origin: Point<Pixels>,
        focused: bool,
        window: &mut Window,
        cx: &mut App,
    ) {
        if let Some(selection) = layout.selection.take() {
            window.paint_quad(selection);
        }

        let line_height = window.line_height();
        layout
            .line
            .paint(origin, line_height, TextAlign::Left, None, window, cx)
            .expect("failed to paint input line");

        if let Some(cursor) = layout.cursor.take().filter(|_| focused) {
            window.paint_quad(cursor);
        }
    }
}

impl IntoElement for InputElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for InputElement {
    type RequestLayoutState = ();
    type PrepaintState = InputElementLayout;

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
            size: size(relative(1.0).into(), window.line_height().into()),
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
        cx: &mut App,
    ) -> Self::PrepaintState {
        let line = self.shape(window, cx);
        let selected_range = self.state.read(cx).selected_range.clone();
        let cursor = line.x_for_index(self.state.read(cx).cursor_offset());
        let left = bounds.left() + self.scroll(cursor, line.width(), bounds.size.width, cx);

        InputElementLayout {
            selection: (!selected_range.is_empty()).then(|| {
                fill(
                    Bounds::from_corners(
                        point(left + line.x_for_index(selected_range.start), bounds.top()),
                        point(left + line.x_for_index(selected_range.end), bounds.bottom()),
                    ),
                    cx.theme().input_selection_bg,
                )
            }),
            cursor: selected_range.is_empty().then(|| {
                fill(
                    Bounds::new(
                        point(left + cursor, bounds.top()),
                        size(px(1.0), bounds.size.height),
                    ),
                    cx.theme().fg,
                )
            }),
            line,
        }
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        layout: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let focus_handle = self.state.read(cx).focus_handle.clone();
        let handler = ElementInputHandler::new(bounds, self.state.clone());
        window.handle_input(&focus_handle, handler, cx);

        let state = self.state.clone();
        window.on_mouse_event(move |event: &MouseMoveEvent, phase, _window, cx| {
            if phase == DispatchPhase::Bubble {
                state.update(cx, |state, cx| state.on_mouse_move(event, cx));
            }
        });

        let focused = focus_handle.is_focused(window) && window.is_window_active();
        let origin = point(
            bounds.left() + self.state.read(cx).scroll_offset,
            bounds.top(),
        );

        window.with_content_mask(Some(ContentMask { bounds }), |window| {
            self.paint_contents(layout, origin, focused, window, cx)
        });

        self.state.update(cx, |state, _cx| {
            state.last_layout = Some(layout.line.clone());
            state.last_bounds = Some(bounds);
        });
    }
}
