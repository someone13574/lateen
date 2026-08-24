use std::rc::Rc;

use gpui::prelude::*;
use gpui::{
    Action, App, BoxShadow, Div, Entity, FocusHandle, Focusable, KeyBinding, MouseButton, Rgba,
    Role, SharedString, Stateful, Text, Window, actions, deferred, div, px, relative, svg,
};

use crate::theme::ActiveTheme;

actions!(
    select,
    [Close, FocusNext, FocusPrevious, Next, Previous, Toggle]
);

pub type SelectHandler = dyn Fn(usize, &mut Window, &mut App) + 'static;

pub struct SelectState {
    focus_handle: FocusHandle,
    open: bool,
}

impl SelectState {
    pub const KEY_CONTEXT: &'static str = "Select";

    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle().tab_stop(true);

        cx.on_blur(&focus_handle, window, |state, _window, cx| {
            state.open = false;
            cx.notify();
        })
        .detach();

        Self {
            focus_handle,
            open: false,
        }
    }

    pub fn init(cx: &mut App) {
        cx.bind_keys([
            KeyBinding::new("enter", Toggle, Some(Self::KEY_CONTEXT)),
            KeyBinding::new("space", Toggle, Some(Self::KEY_CONTEXT)),
            KeyBinding::new("escape", Close, Some(Self::KEY_CONTEXT)),
            KeyBinding::new("down", Next, Some(Self::KEY_CONTEXT)),
            KeyBinding::new("up", Previous, Some(Self::KEY_CONTEXT)),
            KeyBinding::new("tab", FocusNext, Some(Self::KEY_CONTEXT)),
            KeyBinding::new("shift-tab", FocusPrevious, Some(Self::KEY_CONTEXT)),
        ]);
    }
}

impl Focusable for SelectState {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

#[derive(IntoElement)]
pub struct Select {
    id: &'static str,
    label: &'static str,
    state: Entity<SelectState>,
    options: Vec<SharedString>,
    selected: usize,
    readonly: bool,
    on_select: Option<Rc<SelectHandler>>,
}

impl Select {
    pub fn new(
        id: &'static str,
        label: &'static str,
        state: Entity<SelectState>,
        options: Vec<SharedString>,
    ) -> Self {
        Self {
            id,
            label,
            state,
            options,
            selected: 0,
            readonly: false,
            on_select: None,
        }
    }

    pub fn selected(mut self, selected: usize) -> Self {
        self.selected = selected;
        self
    }

    pub fn readonly(mut self, readonly: bool) -> Self {
        self.readonly = readonly;
        self
    }

    pub fn on_select(mut self, on_select: impl Fn(usize, &mut Window, &mut App) + 'static) -> Self {
        self.on_select = Some(Rc::new(on_select));
        self
    }

    fn flip(state: &Entity<SelectState>) -> impl Fn(&Toggle, &mut Window, &mut App) + 'static {
        let state = state.clone();

        move |_action, _window, cx| {
            state.update(cx, |state, cx| {
                state.open = !state.open;
                cx.notify();
            });
        }
    }

    fn dismiss(state: &Entity<SelectState>) -> impl Fn(&Close, &mut Window, &mut App) + 'static {
        let state = state.clone();

        move |_action, _window, cx| Self::close(&state, cx)
    }

    fn close(state: &Entity<SelectState>, cx: &mut App) {
        state.update(cx, |state, cx| {
            state.open = false;
            cx.notify();
        });
    }

    fn menu(self, cx: &App) -> Div {
        let theme = *cx.theme();

        div()
            .occlude()
            .absolute()
            .top(relative(1.0))
            .left_0()
            .w_full()
            .mt(px(2.0))
            .py(px(2.0))
            .rounded(px(4.0))
            .border(px(1.0))
            .border_color(theme.button_border)
            .bg(theme.card_bg)
            .shadow(vec![
                BoxShadow::new(px(0.0), px(4.0), theme.window_shadow.into()).blur_radius(px(10.0)),
            ])
            .children(
                self.options
                    .iter()
                    .enumerate()
                    .map(|(index, option)| self.option(index, option.clone(), cx)),
            )
    }

    fn choose(&self, index: usize) -> impl Fn(&mut Window, &mut App) + 'static {
        let state = self.state.clone();
        let on_select = self.on_select.clone();

        move |window, cx| {
            Self::close(&state, cx);

            if let Some(on_select) = &on_select {
                on_select(index, window, cx);
            }
        }
    }

    fn option(&self, index: usize, label: SharedString, cx: &App) -> Stateful<Div> {
        let theme = *cx.theme();
        let selected = index == self.selected;

        div()
            .id((self.id, index))
            .role(Role::ListBoxOption)
            .aria_label(label.clone())
            .aria_selected(selected)
            .px(px(6.0))
            .py(px(4.0))
            .text_size(px(12.0))
            .cursor_pointer()
            .when(selected, |option| {
                option.bg(theme.chip_bg).text_color(theme.link_fg)
            })
            .hover(|style| style.bg(cx.theme().button_hover_bg))
            .on_mouse_down(MouseButton::Left, {
                let choose = self.choose(index);

                move |_event, window, cx| {
                    cx.stop_propagation();
                    choose(window, cx);
                }
            })
            .on_click({
                let choose = self.choose(index);

                move |_event, window, cx| choose(window, cx)
            })
            .child(Text::new_inaccessible(label))
    }

    fn caret(fg: Rgba) -> Div {
        div()
            .absolute()
            .top(px(2.0))
            .bottom_0()
            .right(px(9.0))
            .flex()
            .items_center()
            .child(
                svg()
                    .path("icons/caret-down.svg")
                    .w(px(8.0))
                    .h(px(4.0))
                    .flex_none()
                    .text_color(fg),
            )
    }

    fn ring(color: Rgba) -> Div {
        div()
            .absolute()
            .inset(px(-1.0))
            .border(px(2.0))
            .rounded(px(5.0))
            .border_color(color)
    }

    fn trigger(&self, focused: bool, cx: &App) -> Stateful<Div> {
        let theme = *cx.theme();
        let state = self.state.clone();
        let focus_handle = self.state.read(cx).focus_handle.clone();

        div()
            .id(self.id)
            .role(Role::ComboBox)
            .aria_label(self.label)
            .aria_expanded(self.state.read(cx).open)
            .relative()
            .flex()
            .items_center()
            .w_full()
            .pl(px(6.0))
            .pr(px(20.0))
            .py(px(4.0))
            .rounded(px(4.0))
            .border(px(1.0))
            .border_color(theme.input_border)
            .bg(theme.card_bg)
            .text_size(px(12.0))
            .text_color(theme.fg)
            .when(!self.readonly, |trigger| {
                trigger
                    .cursor_pointer()
                    .on_click(move |_event, window, cx| {
                        cx.stop_propagation();
                        window.focus(&focus_handle, cx);
                        state.update(cx, |state, cx| {
                            state.open = !state.open;
                            cx.notify();
                        });
                    })
            })
            .child(Text::new_inaccessible(
                self.options.get(self.selected).cloned().unwrap_or_default(),
            ))
            .child(Self::caret(theme.dim_fg))
            .when(focused, |trigger| {
                trigger
                    .border_color(theme.input_ring)
                    .child(Self::ring(theme.input_ring))
            })
    }

    fn step<A: Action>(&self, delta: i32) -> impl Fn(&A, &mut Window, &mut App) + use<A> {
        let on_select = self.on_select.clone();
        let last = self.options.len().saturating_sub(1) as i32;
        let selected = self.selected as i32;

        move |_action, window, cx| {
            if let Some(on_select) = &on_select {
                on_select((selected + delta).clamp(0, last) as usize, window, cx);
            }
        }
    }
}

impl RenderOnce for Select {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let focus_handle = self.state.read(cx).focus_handle.clone();
        let focused = focus_handle.is_focused(window) && !self.readonly;
        let open = self.state.read(cx).open && !self.readonly;
        let dismiss = self.state.clone();
        let field = div().relative().w_full().child(self.trigger(focused, cx));

        if self.readonly {
            return field.into_any_element();
        }

        field
            .key_context(SelectState::KEY_CONTEXT)
            .track_focus(&focus_handle)
            .on_action(Self::flip(&self.state))
            .on_action(Self::dismiss(&self.state))
            .on_action(self.step::<Next>(1))
            .on_action(self.step::<Previous>(-1))
            .on_action(|_: &FocusNext, window: &mut Window, cx: &mut App| window.focus_next(cx))
            .on_action(|_: &FocusPrevious, window: &mut Window, cx: &mut App| window.focus_prev(cx))
            .on_mouse_down_out(move |_event, _window, cx| Self::close(&dismiss, cx))
            .when(open, |select| {
                select.child(deferred(self.menu(cx)).with_priority(1))
            })
            .into_any_element()
    }
}
