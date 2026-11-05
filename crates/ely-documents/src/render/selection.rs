use std::{cell::RefCell, ops::Range, rc::Rc};

use gpui::{
    App, Bounds, ClipboardItem, Context, DispatchPhase, Div, Entity, FocusHandle, Focusable,
    HighlightStyle, InteractiveElement as _, IntoElement, KeyBinding, MouseButton, MouseMoveEvent,
    MouseUpEvent, ParentElement as _, Pixels, Point, Role, SharedString, Stateful,
    StatefulInteractiveElement as _, Styled as _, StyledText, TextLayout, Window, actions, canvas,
};

use crate::{i18n, primitives::FocusRing as _, theme::ActiveTheme as _, tree::Node};

mod geometry;
#[cfg(test)]
mod tests;
mod text;

use text::Text;

actions!(markdown_selection, [Copy, SelectAll]);

pub(crate) fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new(ely_core::secondary!("c"), Copy, Some("MarkdownSelection")),
        KeyBinding::new(
            ely_core::secondary!("a"),
            SelectAll,
            Some("MarkdownSelection"),
        ),
    ]);
}

/// A context-menu request on rendered Markdown.
#[derive(Clone, Debug)]
pub struct MarkdownContext {
    /// The pointer's window position.
    pub position: Point<Pixels>,
    /// The picture's address, without fetching it.
    pub image: Option<String>,
    /// The link under the pointer, when present.
    pub link: Option<String>,
}

pub(super) type OnContext = Rc<dyn Fn(MarkdownContext, &mut Window, &mut App)>;

type Mapping = Rc<dyn Fn(usize) -> usize>;
type Layout = (Range<usize>, TextLayout, Option<Mapping>);

/// Read-only selection shared by a renderer's blocks.
///
/// ```
/// use ely_documents::{MarkdownRenderer, MarkdownSelection};
/// use gpui::{App, AppContext as _};
/// fn page(cx: &mut App) -> MarkdownRenderer {
///     let selection = cx.new(MarkdownSelection::new);
///     MarkdownRenderer::new("page", "One\n\nTwo").selection(&selection)
/// }
/// ```
pub struct MarkdownSelection {
    focus: FocusHandle,
    nodes: Rc<[Node]>,
    text: Text,
    layouts: RefCell<Vec<Layout>>,
    regions: RefCell<Vec<(Range<usize>, Bounds<Pixels>)>>,
    anchor: usize,
    head: usize,
    dragging: bool,
    dragged: bool,
    pressed: Option<Point<Pixels>>,
}

impl MarkdownSelection {
    /// An empty selection, populated by its renderer.
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self {
            focus: cx.focus_handle(),
            nodes: Rc::from([]),
            text: Text::default(),
            layouts: RefCell::new(Vec::new()),
            regions: RefCell::new(Vec::new()),
            anchor: 0,
            head: 0,
            dragging: false,
            dragged: false,
            pressed: None,
        }
    }

    /// The selected rendered text, with block separators.
    pub fn selected_text(&self) -> SharedString {
        self.text.words[self.range()].to_owned().into()
    }

    /// Copies selected text; an empty selection leaves the clipboard untouched.
    pub fn copy(&self, cx: &mut App) {
        if self.anchor != self.head {
            cx.write_to_clipboard(ClipboardItem::new_string(self.selected_text().to_string()));
        }
    }

    /// Selects this renderer's whole body.
    pub fn select_all(&mut self, cx: &mut Context<Self>) {
        self.anchor = 0;
        self.head = self.text.words.len();
        cx.notify();
    }

    pub(super) fn prepare(&mut self, nodes: Rc<[Node]>) {
        if !Rc::ptr_eq(&self.nodes, &nodes) && self.nodes != nodes {
            self.text = Text::of(&nodes);
            self.nodes = nodes;
            self.anchor = 0;
            self.head = 0;
            self.dragging = false;
            self.dragged = false;
            self.pressed = None;
        }
        self.layouts.borrow_mut().clear();
        self.regions.borrow_mut().clear();
    }

    fn range(&self) -> Range<usize> {
        self.anchor.min(self.head)..self.anchor.max(self.head)
    }

    pub(super) fn dragged(&self) -> bool {
        self.dragged
    }

    pub(super) fn highlight(&self, piece: usize, cx: &App) -> super::Over {
        let Some(run) = self.text.runs.get(piece) else {
            return Vec::new();
        };
        let range = self.range();
        let (start, end) = (range.start.max(run.start), range.end.min(run.end));
        if start >= end {
            return Vec::new();
        }
        vec![(
            start - run.start..end - run.start,
            HighlightStyle {
                background_color: Some(cx.theme().colors.selection),
                ..Default::default()
            },
        )]
    }

    pub(super) fn layout(&self, piece: usize, layout: TextLayout) {
        self.mapped_layout(piece, layout, None);
    }

    pub(super) fn mapped_layout(&self, piece: usize, layout: TextLayout, mapping: Option<Mapping>) {
        if let Some(range) = self.text.runs.get(piece) {
            self.layouts
                .borrow_mut()
                .push((range.clone(), layout, mapping));
        }
    }

    fn context(&self, position: Point<Pixels>) -> MarkdownContext {
        let at = self.index_at(position);
        let over_text = self.layouts.borrow().iter().any(|(_, layout, _)| {
            layout.bounds().contains(&position) && layout.index_for_position(position).is_ok()
        });
        MarkdownContext {
            position,
            image: None,
            link: self
                .text
                .links
                .iter()
                .find(|(range, _)| over_text && range.contains(&at))
                .map(|(_, link)| link.clone()),
        }
    }

    fn follow(&mut self, position: Point<Pixels>, cx: &mut Context<Self>) {
        self.dragged |= self
            .pressed
            .is_some_and(|pressed| (position - pressed).magnitude() > 2.0);
        if !self.dragged {
            return;
        }
        let head = self.index_at(position);
        if head != self.head {
            self.head = head;
            cx.notify();
        }
    }
}

impl Focusable for MarkdownSelection {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

pub(super) fn listen(
    root: Stateful<Div>,
    selection: Entity<MarkdownSelection>,
    on_context: Option<OnContext>,
    cx: &App,
) -> Stateful<Div> {
    let focus = selection.read(cx).focus.clone();
    let (down, keys, all, right) = (
        selection.clone(),
        selection.clone(),
        selection.clone(),
        selection.clone(),
    );
    root.track_focus(&focus)
        .role(Role::Document)
        .aria_label(i18n::text(cx, "documents.document", &[]))
        .tab_index(0)
        .border_1()
        .border_color(gpui::transparent_black())
        .focus_ring(cx)
        .key_context("MarkdownSelection")
        .cursor_text()
        .child(drag_listener(selection))
        .on_mouse_down(MouseButton::Left, move |event, window, cx| {
            window.prevent_default();
            cx.stop_propagation();
            down.update(cx, |state, cx| {
                window.focus(&state.focus, cx);
                let at = state.index_at(event.position);
                if !event.modifiers.shift {
                    state.anchor = at;
                }
                state.head = at;
                state.dragging = true;
                state.dragged = false;
                state.pressed = Some(event.position);
                cx.notify();
            });
        })
        .on_action(move |_: &Copy, _, cx| {
            keys.update(cx, |state, cx| state.copy(cx));
            cx.stop_propagation();
        })
        .on_action(move |_: &SelectAll, _, cx| {
            all.update(cx, |state, cx| state.select_all(cx));
            cx.stop_propagation();
        })
        .on_mouse_down(MouseButton::Right, move |event, window, cx| {
            let Some(handler) = &on_context else { return };
            window.prevent_default();
            cx.stop_propagation();
            let focus = right.read(cx).focus.clone();
            window.focus(&focus, cx);
            handler(right.read(cx).context(event.position), window, cx);
        })
}

fn drag_listener(selection: Entity<MarkdownSelection>) -> impl IntoElement {
    canvas(
        |_, _, _| {},
        move |_, _, window, _| {
            let moving = selection.clone();
            window.on_mouse_event(move |event: &MouseMoveEvent, phase, _, cx| {
                if phase != DispatchPhase::Bubble || !moving.read(cx).dragging {
                    return;
                }
                moving.update(cx, |state, cx| {
                    if event.dragging() {
                        state.follow(event.position, cx);
                    } else {
                        state.dragging = false;
                        state.dragged = false;
                    }
                });
            });
            let up = selection.clone();
            window.on_mouse_event(move |event: &MouseUpEvent, phase, window, cx| {
                if phase != DispatchPhase::Bubble
                    || event.button != MouseButton::Left
                    || !up.read(cx).dragging
                {
                    return;
                }
                up.update(cx, |state, cx| {
                    state.follow(event.position, cx);
                    state.dragging = false;
                });
                let up = up.clone();
                window.defer(cx, move |_, cx| {
                    up.update(cx, |state, _| state.dragged = false)
                });
            });
        },
    )
    .absolute()
}

impl super::Draw<'_> {
    pub(super) fn select_code(&self, text: &str, mut styles: super::Over, cx: &App) -> StyledText {
        let piece = self.piece.get();
        self.piece.set(piece + 1);
        if let Some(selection) = &self.selection {
            styles.extend(selection.read(cx).highlight(piece, cx));
        }
        let code = StyledText::new(text.to_owned()).with_highlights(crate::editor::stack(styles));
        if let Some(selection) = &self.selection {
            selection.read(cx).layout(piece, code.layout().clone());
        }
        code
    }

    pub(super) fn context_image(
        &self,
        picture: Stateful<Div>,
        url: &str,
        link: &Option<String>,
        _: &App,
    ) -> Stateful<Div> {
        let Some(handler) = self.on_context.clone() else {
            return picture;
        };
        let (url, link, selection) = (url.to_owned(), link.clone(), self.selection.clone());
        picture.on_mouse_down(MouseButton::Right, move |event, window, cx| {
            window.prevent_default();
            cx.stop_propagation();
            if let Some(selection) = &selection {
                let focus = selection.read(cx).focus.clone();
                window.focus(&focus, cx);
            }
            handler(
                MarkdownContext {
                    position: event.position,
                    image: Some(url.clone()),
                    link: link.clone(),
                },
                window,
                cx,
            );
        })
    }
}
