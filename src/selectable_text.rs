use std::any::TypeId;
use std::ops::Range;
use std::panic::Location;

use gpui::accesskit::Node;
use gpui::prelude::*;
use gpui::{
    App, Bounds, ClipboardItem, Context, CursorStyle, DispatchPhase, Element, ElementId, Entity,
    FocusHandle, Focusable, GlobalElementId, HighlightStyle, Hitbox, HitboxBehavior,
    InspectorElementId, KeyBinding, KeyContext, LayoutId, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, Pixels, Point, Role, SharedString, StyledText, TextLayout, Window, actions, fill,
    point, size,
};
use unicode_segmentation::UnicodeSegmentation;

use crate::theme::ActiveTheme;

actions!(selection, [Copy]);

pub struct TextSelection {
    focus_handle: FocusHandle,
    text: SharedString,
    range: Range<usize>,
    drag: Option<Drag>,
}

#[derive(Clone)]
struct Drag {
    anchor: Range<usize>,
    by_word: bool,
}

impl TextSelection {
    pub const KEY_CONTEXT: &'static str = "SelectableText";

    pub fn init(cx: &mut App) {
        cx.bind_keys([KeyBinding::new("ctrl-c", Copy, Some(Self::KEY_CONTEXT))]);
    }

    fn new(text: SharedString, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();

        cx.on_blur(&focus_handle, window, |selection, _window, cx| {
            selection.drag = None;
            selection.range = 0..0;
            cx.notify();
        })
        .detach();

        Self {
            focus_handle,
            text,
            range: 0..0,
            drag: None,
        }
    }

    fn set_text(&mut self, text: &SharedString, cx: &mut Context<Self>) {
        if &self.text == text {
            return;
        }

        self.text = text.clone();
        self.range = Self::clamped(text, self.range.clone());

        if let Some(drag) = self.drag.as_mut() {
            drag.anchor = Self::clamped(text, drag.anchor.clone());
        }

        cx.notify();
    }

    fn clamped(text: &str, range: Range<usize>) -> Range<usize> {
        Self::clamped_index(text, range.start)..Self::clamped_index(text, range.end)
    }

    fn clamped_index(text: &str, index: usize) -> usize {
        (0..=index.min(text.len()))
            .rev()
            .find(|index| text.is_char_boundary(*index))
            .unwrap_or(0)
    }

    fn on_mouse_down(&mut self, event: &MouseDownEvent, index: usize, cx: &mut Context<Self>) {
        match event.click_count {
            1 if event.modifiers.shift => {
                let anchor = match index < self.range.start {
                    true => self.range.end,
                    false => self.range.start,
                };

                self.drag = Some(Drag {
                    anchor: anchor..anchor,
                    by_word: false,
                });
                self.drag_to(index, cx);
            }
            1 => {
                self.drag = Some(Drag {
                    anchor: index..index,
                    by_word: false,
                });
                self.select(index..index, cx);
            }
            2 => {
                let word = self.word_at(index);

                self.drag = Some(Drag {
                    anchor: word.clone(),
                    by_word: true,
                });
                self.select(word, cx);
            }
            _ => {
                self.drag = None;
                self.select(0..self.text.len(), cx);
            }
        }
    }

    fn on_mouse_up(&mut self, _cx: &mut Context<Self>) {
        self.drag = None;
    }

    fn drag_to(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(drag) = self.drag.clone() else {
            return;
        };

        let extent = match drag.by_word {
            true => self.word_at(index),
            false => index..index,
        };

        self.select(
            extent.start.min(drag.anchor.start)..extent.end.max(drag.anchor.end),
            cx,
        );
    }

    fn select(&mut self, range: Range<usize>, cx: &mut Context<Self>) {
        self.range = range;

        cx.notify();
    }

    fn word_at(&self, offset: usize) -> Range<usize> {
        self.text
            .split_word_bound_indices()
            .map(|(start, word)| start..start + word.len())
            .take_while(|word| word.start <= offset)
            .last()
            .unwrap_or(offset..offset)
    }

    fn selected(&self) -> Option<&str> {
        Some(&self.text[self.range.clone()]).filter(|selected| !selected.is_empty())
    }
}

#[derive(IntoElement)]
pub struct SelectableText {
    id: ElementId,
    text: SharedString,
    runs: Vec<(Range<usize>, HighlightStyle)>,
}

impl SelectableText {
    pub fn new(id: impl Into<ElementId>, text: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            text: text.into(),
            runs: Vec::new(),
        }
    }

    pub fn runs(mut self, runs: Vec<(Range<usize>, HighlightStyle)>) -> Self {
        self.runs = runs;
        self
    }
}

impl RenderOnce for SelectableText {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let text = self.text.clone();
        let state = window.use_keyed_state(self.id.clone(), cx, |window, cx| {
            TextSelection::new(text, window, cx)
        });

        state.update(cx, |selection, cx| selection.set_text(&self.text, cx));

        SelectableTextElement {
            id: self.id,
            state,
            text: self.text,
            runs: self.runs,
        }
    }
}

struct SelectableTextElement {
    id: ElementId,
    state: Entity<TextSelection>,
    text: SharedString,
    runs: Vec<(Range<usize>, HighlightStyle)>,
}

struct Row {
    top: Pixels,
    indices: Range<usize>,
}

enum PointedRow {
    Above,
    Below,
    Within(usize),
}

impl SelectableTextElement {
    const HIT_MARGIN_GLYPHS: f32 = 3.0;

    fn rows(layout: &TextLayout) -> Vec<Row> {
        let rendered = layout.text();
        let mut rows: Vec<Row> = Vec::new();

        let boundaries = rendered
            .grapheme_indices(true)
            .map(|(index, _)| index)
            .chain([rendered.len()]);

        for index in boundaries {
            let Some(position) = layout.position_for_index(index) else {
                continue;
            };

            let opening = rows.last().map_or(index, |row| row.indices.end);

            match rows.last_mut() {
                Some(row) if row.top == position.y => row.indices.end = index,
                _ => rows.push(Row {
                    top: position.y,
                    indices: opening..index,
                }),
            }
        }

        rows
    }

    fn hit_margin(width: Pixels, rendered: &str) -> Pixels {
        let glyphs = rendered.graphemes(true).count().max(1);

        width / glyphs as f32 * Self::HIT_MARGIN_GLYPHS
    }

    fn paint_highlight(&self, layout: &TextLayout, window: &mut Window, cx: &mut App) {
        let range = self.state.read(cx).range.clone();
        let rendered = layout.text();
        let selected = Self::rendered_index(&self.text, &rendered, range.start)
            ..Self::rendered_index(&self.text, &rendered, range.end);

        for row in Self::rows(layout) {
            let Some(band) = Self::band(layout, &row, &selected) else {
                continue;
            };

            window.paint_quad(fill(band, cx.theme().input_selection_bg));
        }
    }

    fn band(layout: &TextLayout, row: &Row, selected: &Range<usize>) -> Option<Bounds<Pixels>> {
        let start = selected.start.max(row.indices.start);
        let end = selected.end.min(row.indices.end);

        if start >= end {
            return None;
        }

        let left = match start == row.indices.start {
            true => layout.bounds().left(),
            false => layout.position_for_index(start)?.x,
        };
        let right = layout.position_for_index(end)?.x;

        Some(Bounds::from_corners(
            point(left, row.top),
            point(right, row.top + layout.line_height()),
        ))
    }

    fn begin_on_mouse_down(&self, layout: &TextLayout, hitbox: &Hitbox, window: &mut Window) {
        let state = self.state.clone();
        let text = self.text.clone();
        let layout = layout.clone();
        let hitbox = hitbox.clone();

        window.on_mouse_event(move |event: &MouseDownEvent, phase, window, cx| {
            if phase == DispatchPhase::Bubble && hitbox.is_hovered(window) {
                let index = Self::index_at(&text, &layout, event.position);
                let focus_handle = state.read(cx).focus_handle.clone();

                window.focus(&focus_handle, cx);
                window.prevent_default();
                state.update(cx, |selection, cx| {
                    selection.on_mouse_down(event, index, cx)
                });
            }
        });
    }

    fn extend_on_mouse_move(&self, layout: &TextLayout, window: &mut Window) {
        let state = self.state.clone();
        let text = self.text.clone();
        let layout = layout.clone();

        window.on_mouse_event(move |event: &MouseMoveEvent, phase, _window, cx| {
            if phase == DispatchPhase::Bubble && state.read(cx).drag.is_some() {
                let index = Self::index_at(&text, &layout, event.position);

                state.update(cx, |selection, cx| selection.drag_to(index, cx));
            }
        });
    }

    fn finish_on_mouse_up(&self, window: &mut Window) {
        let state = self.state.clone();

        window.on_mouse_event(move |_event: &MouseUpEvent, phase, _window, cx| {
            if phase == DispatchPhase::Bubble && state.read(cx).drag.is_some() {
                state.update(cx, |selection, cx| selection.on_mouse_up(cx));
            }
        });
    }

    fn dispatch_copy(&self, window: &mut Window) {
        let mut context = KeyContext::new_with_defaults();
        context.add(TextSelection::KEY_CONTEXT);
        window.set_key_context(context);

        let state = self.state.clone();

        window.on_action(
            TypeId::of::<Copy>(),
            move |_action, phase, _window, cx: &mut App| {
                if phase == DispatchPhase::Bubble
                    && let Some(selected) = state.read(cx).selected()
                {
                    cx.write_to_clipboard(ClipboardItem::new_string(selected.to_string()));
                }
            },
        );
    }

    fn index_at(text: &str, layout: &TextLayout, position: Point<Pixels>) -> usize {
        let rendered = layout.text();
        let rows = Self::rows(layout);
        let index = match Self::pointed_row(&rows, layout.line_height(), position) {
            PointedRow::Above => 0,
            PointedRow::Below => rendered.len(),
            PointedRow::Within(row) => Self::nearest(layout, &rendered, &rows[row], position.x),
        };

        Self::full_index(text, &rendered, index)
    }

    fn pointed_row(rows: &[Row], line_height: Pixels, position: Point<Pixels>) -> PointedRow {
        let (Some(first), Some(last)) = (rows.first(), rows.last()) else {
            return PointedRow::Above;
        };

        if rows.len() == 1 {
            return PointedRow::Within(0);
        }

        if position.y < first.top {
            return PointedRow::Above;
        }

        if position.y >= last.top + line_height {
            return PointedRow::Below;
        }

        PointedRow::Within(
            rows.iter()
                .rposition(|row| position.y >= row.top)
                .unwrap_or(0),
        )
    }

    fn nearest(layout: &TextLayout, rendered: &str, row: &Row, x: Pixels) -> usize {
        rendered
            .grapheme_indices(true)
            .map(|(index, _)| index)
            .chain([rendered.len()])
            .filter(|index| *index > row.indices.start && *index <= row.indices.end)
            .filter_map(|index| Some((index, layout.position_for_index(index)?.x)))
            .chain([(row.indices.start, layout.bounds().left())])
            .min_by_key(|(_, position)| (*position - x).abs())
            .map_or(row.indices.start, |(index, _)| index)
    }

    fn visible_len(text: &str, rendered: &str) -> usize {
        text.bytes()
            .zip(rendered.bytes())
            .take_while(|(full, shown)| full == shown)
            .count()
    }

    fn rendered_index(text: &str, rendered: &str, index: usize) -> usize {
        match index <= Self::visible_len(text, rendered) {
            true => index,
            false => rendered.len(),
        }
    }

    fn full_index(text: &str, rendered: &str, index: usize) -> usize {
        match index <= Self::visible_len(text, rendered) {
            true => index,
            false => text.len(),
        }
    }
}

impl IntoElement for SelectableTextElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for SelectableTextElement {
    type RequestLayoutState = StyledText;
    type PrepaintState = Hitbox;

    fn id(&self) -> Option<ElementId> {
        Some(self.id.clone())
    }

    fn source_location(&self) -> Option<&'static Location<'static>> {
        None
    }

    fn a11y_role(&self) -> Option<Role> {
        Some(Role::Label)
    }

    fn write_a11y_info(&self, node: &mut Node) {
        node.set_value(self.text.to_string());
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let mut styled =
            StyledText::new(self.text.clone()).with_highlights(self.runs.iter().cloned());
        let (layout_id, ()) = styled.request_layout(None, inspector_id, window, cx);

        (layout_id, styled)
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        styled: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        styled.prepaint(None, inspector_id, bounds, &mut (), window, cx);
        window.set_focus_handle(&self.state.read(cx).focus_handle, cx);

        let layout = styled.layout();
        let rows = Self::rows(layout);
        let width = match rows.len() > 1 {
            true => bounds.size.width,
            false => layout
                .position_for_index(layout.len())
                .map_or(bounds.size.width, |end| end.x - bounds.left()),
        };

        let margin = Self::hit_margin(width, &layout.text());

        window.insert_hitbox(
            Bounds::new(
                point(bounds.left() - margin, bounds.top()),
                size(width + margin * 2.0, bounds.size.height),
            ),
            HitboxBehavior::Normal,
        )
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        styled: &mut Self::RequestLayoutState,
        hitbox: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let layout = styled.layout().clone();

        window.set_cursor_style(CursorStyle::IBeam, hitbox);
        self.dispatch_copy(window);
        self.paint_highlight(&layout, window, cx);
        self.begin_on_mouse_down(&layout, hitbox, window);
        self.extend_on_mouse_move(&layout, window);
        self.finish_on_mouse_up(window);

        styled.paint(None, inspector_id, bounds, &mut (), &mut (), window, cx);
    }
}

impl Focusable for TextSelection {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}
