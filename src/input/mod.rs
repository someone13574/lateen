use std::ops::Range;

use gpui::{
    App, Bounds, ClipboardItem, Context, EntityInputHandler, EventEmitter, FocusHandle, Focusable,
    KeyBinding, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels, Point, ShapedLine,
    SharedString, UTF16Selection, Window, actions, point, px,
};
use unicode_segmentation::UnicodeSegmentation;

pub use element::Input;
pub use entry::Entry;

mod element;
mod entry;

actions!(
    input,
    [
        Backspace,
        Copy,
        Cut,
        Delete,
        End,
        Enter,
        FocusNext,
        FocusPrevious,
        Home,
        Left,
        Paste,
        Redo,
        Right,
        SelectAll,
        SelectEnd,
        SelectHome,
        SelectLeft,
        SelectRight,
        Undo,
    ]
);

pub enum InputEvent {
    Changed,
    Committed,
}

pub struct InputState {
    focus_handle: FocusHandle,
    entry: Entry,
    invalid: bool,
    content: SharedString,
    selected_range: Range<usize>,
    selection_reversed: bool,
    marked_range: Option<Range<usize>>,
    last_layout: Option<ShapedLine>,
    last_bounds: Option<Bounds<Pixels>>,
    scroll_offset: Pixels,
    drag: Option<Drag>,
    undo_stack: Vec<Revision>,
    redo_stack: Vec<Revision>,
    edit_run: Option<EditRun>,
}

#[derive(Clone, Copy)]
enum EditRun {
    Insert(usize),
    Delete(usize),
}

#[derive(Clone)]
struct Drag {
    anchor: Range<usize>,
    by_word: bool,
}

struct Revision {
    content: SharedString,
    selected_range: Range<usize>,
    selection_reversed: bool,
}

impl InputState {
    pub const KEY_CONTEXT: &'static str = "Input";

    pub fn new(entry: Entry, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle().tab_stop(true);

        cx.on_blur(&focus_handle, window, |_state, window, cx| {
            cx.defer_in(window, |state, _window, cx| {
                state.drag = None;
                state.move_to(state.cursor_offset(), cx);
                cx.emit(InputEvent::Committed);
            });
        })
        .detach();

        Self {
            focus_handle,
            entry,
            invalid: false,
            content: SharedString::default(),
            selected_range: 0..0,
            selection_reversed: false,
            marked_range: None,
            last_layout: None,
            last_bounds: None,
            scroll_offset: px(0.0),
            drag: None,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            edit_run: None,
        }
    }

    pub fn init(cx: &mut App) {
        cx.bind_keys([
            KeyBinding::new("backspace", Backspace, Some(Self::KEY_CONTEXT)),
            KeyBinding::new("delete", Delete, Some(Self::KEY_CONTEXT)),
            KeyBinding::new("left", Left, Some(Self::KEY_CONTEXT)),
            KeyBinding::new("right", Right, Some(Self::KEY_CONTEXT)),
            KeyBinding::new("shift-left", SelectLeft, Some(Self::KEY_CONTEXT)),
            KeyBinding::new("shift-right", SelectRight, Some(Self::KEY_CONTEXT)),
            KeyBinding::new("home", Home, Some(Self::KEY_CONTEXT)),
            KeyBinding::new("up", Home, Some(Self::KEY_CONTEXT)),
            KeyBinding::new("end", End, Some(Self::KEY_CONTEXT)),
            KeyBinding::new("down", End, Some(Self::KEY_CONTEXT)),
            KeyBinding::new("shift-home", SelectHome, Some(Self::KEY_CONTEXT)),
            KeyBinding::new("shift-up", SelectHome, Some(Self::KEY_CONTEXT)),
            KeyBinding::new("shift-end", SelectEnd, Some(Self::KEY_CONTEXT)),
            KeyBinding::new("shift-down", SelectEnd, Some(Self::KEY_CONTEXT)),
            KeyBinding::new("ctrl-a", SelectAll, Some(Self::KEY_CONTEXT)),
            KeyBinding::new("ctrl-c", Copy, Some(Self::KEY_CONTEXT)),
            KeyBinding::new("ctrl-v", Paste, Some(Self::KEY_CONTEXT)),
            KeyBinding::new("ctrl-x", Cut, Some(Self::KEY_CONTEXT)),
            KeyBinding::new("ctrl-z", Undo, Some(Self::KEY_CONTEXT)),
            KeyBinding::new("ctrl-shift-z", Redo, Some(Self::KEY_CONTEXT)),
            KeyBinding::new("ctrl-y", Redo, Some(Self::KEY_CONTEXT)),
            KeyBinding::new("enter", Enter, Some(Self::KEY_CONTEXT)),
            KeyBinding::new("tab", FocusNext, Some(Self::KEY_CONTEXT)),
            KeyBinding::new("shift-tab", FocusPrevious, Some(Self::KEY_CONTEXT)),
        ]);
    }

    pub fn content(&self) -> &SharedString {
        &self.content
    }

    pub fn set_content(&mut self, content: impl Into<SharedString>, cx: &mut Context<Self>) {
        self.content = content.into();
        self.selected_range = self.content.len()..self.content.len();
        self.selection_reversed = false;
        self.marked_range = None;
        self.edit_run = None;
        self.invalid = false;

        cx.notify();
    }

    pub fn set_invalid(&mut self, invalid: bool, cx: &mut Context<Self>) {
        self.invalid = invalid;

        cx.notify();
    }

    fn enter(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.move_to(self.content.len(), cx);
        cx.emit(InputEvent::Committed);
    }

    fn left(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.move_to(self.previous_boundary(self.cursor_offset()), cx);
        } else {
            self.move_to(self.selected_range.start, cx);
        }
    }

    fn right(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.move_to(self.next_boundary(self.cursor_offset()), cx);
        } else {
            self.move_to(self.selected_range.end, cx);
        }
    }

    fn select_left(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.previous_boundary(self.cursor_offset()), cx);
    }

    fn select_right(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.next_boundary(self.cursor_offset()), cx);
    }

    fn home(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.move_to(0, cx);
    }

    fn end(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.move_to(self.content.len(), cx);
    }

    fn select_home(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.select_to(0, cx);
    }

    fn select_end(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.content.len(), cx);
    }

    fn select_all(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.select_range(0..self.content.len(), cx);
    }

    fn backspace(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let range = if self.selected_range.is_empty() {
            self.previous_boundary(self.cursor_offset())..self.cursor_offset()
        } else {
            self.selected_range.clone()
        };

        if !range.is_empty() {
            self.replace(range, "", cx);
        }
    }

    fn delete(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let range = if self.selected_range.is_empty() {
            self.cursor_offset()..self.next_boundary(self.cursor_offset())
        } else {
            self.selected_range.clone()
        };

        if !range.is_empty() {
            self.replace(range, "", cx);
        }
    }

    fn copy(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if !self.selected_range.is_empty() {
            let selection = self.content[self.selected_range.clone()].to_string();
            cx.write_to_clipboard(ClipboardItem::new_string(selection));
        }
    }

    fn cut(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.copy(window, cx);

        if !self.selected_range.is_empty() {
            self.replace(self.selected_range.clone(), "", cx);
        }
    }

    fn paste(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
            self.replace(self.pending_range(None), &text.replace('\n', " "), cx);
        }
    }

    fn focus_next(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        window.focus_next(cx);
    }

    fn focus_previous(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        window.focus_prev(cx);
    }

    fn undo(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(revision) = self.undo_stack.pop() {
            let current = self.revision();

            self.redo_stack.push(current);
            self.restore(revision, cx);
        }
    }

    fn redo(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(revision) = self.redo_stack.pop() {
            let current = self.revision();

            self.undo_stack.push(current);
            self.restore(revision, cx);
        }
    }

    fn on_mouse_down(&mut self, event: &MouseDownEvent, cx: &mut Context<Self>) {
        let offset = self.index_for_position(event.position);

        match event.click_count {
            1 if event.modifiers.shift => {
                let anchor = if self.selection_reversed {
                    self.selected_range.end
                } else {
                    self.selected_range.start
                };

                self.drag = Some(Drag {
                    anchor: anchor..anchor,
                    by_word: false,
                });
                self.drag_to(offset, cx);
            }
            1 => {
                self.drag = Some(Drag {
                    anchor: offset..offset,
                    by_word: false,
                });
                self.move_to(offset, cx);
            }
            2 => {
                let word = self.word_at(offset);

                self.drag = Some(Drag {
                    anchor: word.clone(),
                    by_word: true,
                });
                self.select_range(word, cx);
            }
            _ => {
                self.drag = None;
                self.select_range(0..self.content.len(), cx);
            }
        }
    }

    fn on_mouse_up(&mut self, _event: &MouseUpEvent, _cx: &mut Context<Self>) {
        self.drag = None;
    }

    fn on_mouse_move(&mut self, event: &MouseMoveEvent, cx: &mut Context<Self>) {
        if self.drag.is_some() {
            self.drag_to(self.index_for_position(event.position), cx);
        }
    }

    fn index_for_position(&self, position: Point<Pixels>) -> usize {
        let Some(bounds) = self.last_bounds else {
            return 0;
        };

        self.index_for_x(position.x - bounds.left() - self.scroll_offset)
    }

    fn index_for_x(&self, x: Pixels) -> usize {
        let Some(line) = self.last_layout.as_ref() else {
            return 0;
        };

        self.content
            .grapheme_indices(true)
            .map(|(index, _)| index)
            .chain([self.content.len()])
            .min_by_key(|index| (line.x_for_index(*index) - x).abs())
            .unwrap_or(0)
    }

    fn offset_from_utf16(text: &str, offset: usize) -> usize {
        let mut utf8_offset = 0;
        let mut utf16_count = 0;

        for character in text.chars() {
            if utf16_count >= offset {
                break;
            }

            utf16_count += character.len_utf16();
            utf8_offset += character.len_utf8();
        }

        utf8_offset
    }

    fn range_from_utf16(text: &str, range: &Range<usize>) -> Range<usize> {
        Self::offset_from_utf16(text, range.start)..Self::offset_from_utf16(text, range.end)
    }

    fn offset_to_utf16(text: &str, offset: usize) -> usize {
        let mut utf16_offset = 0;
        let mut utf8_count = 0;

        for character in text.chars() {
            if utf8_count >= offset {
                break;
            }

            utf8_count += character.len_utf8();
            utf16_offset += character.len_utf16();
        }

        utf16_offset
    }

    fn range_to_utf16(text: &str, range: &Range<usize>) -> Range<usize> {
        Self::offset_to_utf16(text, range.start)..Self::offset_to_utf16(text, range.end)
    }

    fn replace(&mut self, range: Range<usize>, text: &str, cx: &mut Context<Self>) {
        if !self.permitted(&range, text) {
            return;
        }

        let cursor = range.start + text.len();

        self.record(&range, text);
        self.splice(&range, text);
        self.selected_range = cursor..cursor;
        self.marked_range = None;

        cx.emit(InputEvent::Changed);
        cx.notify();
    }

    fn permitted(&self, range: &Range<usize>, text: &str) -> bool {
        if text.is_empty() {
            return true;
        }

        let candidate = self.content[..range.start].to_owned() + text + &self.content[range.end..];

        self.entry.permits(&self.content, &candidate)
    }

    fn revision(&self) -> Revision {
        Revision {
            content: self.content.clone(),
            selected_range: self.selected_range.clone(),
            selection_reversed: self.selection_reversed,
        }
    }

    fn restore(&mut self, revision: Revision, cx: &mut Context<Self>) {
        self.content = revision.content;
        self.selected_range = revision.selected_range;
        self.selection_reversed = revision.selection_reversed;
        self.marked_range = None;
        self.edit_run = None;

        cx.emit(InputEvent::Changed);
        cx.notify();
    }

    fn record(&mut self, range: &Range<usize>, new_text: &str) {
        let run = match (range.is_empty(), new_text.is_empty()) {
            (true, false) => Some(EditRun::Insert(range.start + new_text.len())),
            (false, true) => Some(EditRun::Delete(range.start)),
            _ => None,
        };

        let continues = match (self.edit_run, run) {
            (Some(EditRun::Insert(at)), Some(EditRun::Insert(_))) => at == range.start,
            (Some(EditRun::Delete(at)), Some(EditRun::Delete(_))) => {
                at == range.start || at == range.end
            }
            _ => false,
        };

        if self.marked_range.is_none() && !continues {
            let current = self.revision();

            self.undo_stack.push(current);
        }

        self.redo_stack.clear();
        self.edit_run = run;
    }

    fn pending_range(&self, range_utf16: Option<Range<usize>>) -> Range<usize> {
        range_utf16
            .map(|range| Self::range_from_utf16(&self.content, &range))
            .or_else(|| self.marked_range.clone())
            .unwrap_or_else(|| self.selected_range.clone())
    }

    fn splice(&mut self, range: &Range<usize>, text: &str) {
        self.content =
            (self.content[..range.start].to_owned() + text + &self.content[range.end..]).into();
    }

    fn previous_boundary(&self, offset: usize) -> usize {
        self.content
            .grapheme_indices(true)
            .rev()
            .find_map(|(index, _)| (index < offset).then_some(index))
            .unwrap_or(0)
    }

    fn word_at(&self, offset: usize) -> Range<usize> {
        self.content
            .split_word_bound_indices()
            .map(|(start, word)| start..start + word.len())
            .take_while(|word| word.start <= offset)
            .last()
            .unwrap_or(offset..offset)
    }

    fn next_boundary(&self, offset: usize) -> usize {
        self.content
            .grapheme_indices(true)
            .find_map(|(index, _)| (index > offset).then_some(index))
            .unwrap_or(self.content.len())
    }

    fn cursor_offset(&self) -> usize {
        if self.selection_reversed {
            self.selected_range.start
        } else {
            self.selected_range.end
        }
    }

    fn move_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        self.selected_range = offset..offset;
        self.edit_run = None;
        cx.notify();
    }

    fn select_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        if self.selection_reversed {
            self.selected_range.start = offset;
        } else {
            self.selected_range.end = offset;
        }

        if self.selected_range.end < self.selected_range.start {
            self.selection_reversed = !self.selection_reversed;
            self.selected_range = self.selected_range.end..self.selected_range.start;
        }

        self.edit_run = None;
        cx.notify();
    }

    fn select_range(&mut self, range: Range<usize>, cx: &mut Context<Self>) {
        self.move_to(range.start, cx);
        self.select_to(range.end, cx);
    }

    fn drag_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        let Some(drag) = self.drag.clone() else {
            return;
        };

        let extent = if drag.by_word {
            self.word_at(offset)
        } else {
            offset..offset
        };

        self.selection_reversed = extent.start < drag.anchor.start;
        self.selected_range = extent.start.min(drag.anchor.start)..extent.end.max(drag.anchor.end);

        cx.notify();
    }
}

impl EventEmitter<InputEvent> for InputState {}

impl Focusable for InputState {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl EntityInputHandler for InputState {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        adjusted_range: &mut Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<String> {
        let range = Self::range_from_utf16(&self.content, &range_utf16);
        adjusted_range.replace(Self::range_to_utf16(&self.content, &range));

        Some(self.content[range].to_string())
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: Self::range_to_utf16(&self.content, &self.selected_range),
            reversed: self.selection_reversed,
        })
    }

    fn marked_text_range(
        &self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Range<usize>> {
        self.marked_range
            .as_ref()
            .map(|range| Self::range_to_utf16(&self.content, range))
    }

    fn unmark_text(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        self.marked_range = None;
    }

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = self.pending_range(range_utf16);

        self.replace(range, new_text, cx);
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        new_selected_range_utf16: Option<Range<usize>>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = self.pending_range(range_utf16);

        if !self.permitted(&range, new_text) {
            return;
        }

        let marked = range.start..range.start + new_text.len();

        self.record(&range, new_text);
        self.splice(&range, new_text);
        self.selected_range = match new_selected_range_utf16 {
            Some(selection) => {
                let selection = Self::range_from_utf16(new_text, &selection);
                marked.start + selection.start..marked.start + selection.end
            }
            None => marked.end..marked.end,
        };
        self.marked_range = (!new_text.is_empty()).then_some(marked);

        cx.emit(InputEvent::Changed);
        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        element_bounds: Bounds<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let line = self.last_layout.as_ref()?;
        let range = Self::range_from_utf16(&self.content, &range_utf16);
        let left = element_bounds.left() + self.scroll_offset;

        Some(Bounds::from_corners(
            point(left + line.x_for_index(range.start), element_bounds.top()),
            point(left + line.x_for_index(range.end), element_bounds.bottom()),
        ))
    }

    fn character_index_for_point(
        &mut self,
        point: Point<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        let bounds = self.last_bounds?;
        let index = self.index_for_x(point.x - bounds.left() - self.scroll_offset);

        Some(Self::offset_to_utf16(&self.content, index))
    }
}
