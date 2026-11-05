use gpui::{
    AppContext as _, Context, Entity, IntoElement, Modifiers, MouseButton, Render, TestAppContext,
    Window, div, point, px,
};

use super::*;
use crate::{Inline, MarkdownRenderer};

struct Page {
    nodes: Rc<[Node]>,
    selection: Entity<MarkdownSelection>,
    links: Rc<std::cell::Cell<usize>>,
    custom: bool,
}

impl Render for Page {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let links = self.links.clone();
        let renderer = MarkdownRenderer::blocks("body", self.nodes.clone())
            .selection(&self.selection)
            .on_link(move |_, _, _| links.set(links.get() + 1));
        let renderer = if self.custom {
            renderer.code(|_, _, words| div().child(words).into_any_element())
        } else {
            renderer
        };
        div().w(px(400.0)).child(renderer)
    }
}

fn words(text: &str) -> Node {
    Node::Paragraph(Inline {
        text: text.to_owned(),
        ..Default::default()
    })
}

#[gpui::test]
fn rendered_selection_copies_across_blocks_in_both_directions(cx: &mut TestAppContext) {
    cx.update(crate::init_for_tests);
    cx.update(bind_keys);
    let selection = cx.new(MarkdownSelection::new);
    let nodes: Rc<[Node]> = vec![words("中文 first"), words("second 中文")].into();
    let (_, cx) = cx.add_window_view(|_, _| Page {
        nodes,
        selection: selection.clone(),
        links: Default::default(),
        custom: false,
    });
    cx.run_until_parked();
    let (first, last) = cx.update(|_, cx| {
        let state = selection.read(cx);
        let layouts = state.layouts.borrow();
        let start = layouts[0].1.position_for_index("中文 ".len()).unwrap();
        let end = layouts[1].1.position_for_index("second".len()).unwrap();
        (
            point(start.x, start.y + px(2.0)),
            point(end.x, end.y + px(2.0)),
        )
    });
    for (start, end) in [(first, last), (last, first)] {
        cx.simulate_mouse_down(start, MouseButton::Left, Modifiers::none());
        cx.simulate_mouse_move(end, MouseButton::Left, Modifiers::none());
        cx.simulate_mouse_up(end, MouseButton::Left, Modifiers::none());
        cx.run_until_parked();
        cx.simulate_keystrokes("secondary-c");
        assert_eq!(
            cx.read_from_clipboard().unwrap().text().as_deref(),
            Some("first\n\nsecond")
        );
    }
    cx.simulate_keystrokes("secondary-a secondary-c");
    assert_eq!(
        cx.read_from_clipboard().unwrap().text().as_deref(),
        Some("中文 first\n\nsecond 中文")
    );
    cx.simulate_keystrokes("backspace x secondary-v");
    assert_eq!(
        selection.read_with(cx, |state, _| state.selected_text()),
        "中文 first\n\nsecond 中文"
    );
}

#[gpui::test]
fn changed_blocks_drop_a_previous_selection(cx: &mut TestAppContext) {
    let selection = cx.new(MarkdownSelection::new);
    selection.update(cx, |state, cx| {
        state.prepare(vec![words("old")].into());
        state.select_all(cx);
        state.prepare(vec![words("new")].into());
        assert!(state.selected_text().is_empty());
    });
}

#[test]
fn copied_lists_keep_markers_and_unicode() {
    let text = Text::of(&[
        words("中文"),
        Node::List {
            start: Some(3),
            items: vec![(None, vec![words("one")]), (None, vec![words("二")])],
        },
    ]);
    assert_eq!(text.words, "中文\n\n3. one\n\n4. 二");
    for run in text.runs {
        assert!(text.words.is_char_boundary(run.start) && text.words.is_char_boundary(run.end));
    }
}

#[test]
fn code_line_endings_are_not_cut_from_selectable_runs() {
    let text = Text::of(&[Node::Code {
        language: None,
        text: "a\n".into(),
        from: 0,
    }]);
    assert_eq!(&text.words[text.runs[0].clone()], "a\n");
}

#[gpui::test]
fn dragging_a_link_does_not_open_it_and_the_next_click_still_works(cx: &mut TestAppContext) {
    cx.update(crate::init_for_tests);
    let selection = cx.new(MarkdownSelection::new);
    let links = Rc::new(std::cell::Cell::new(0));
    let line = Inline {
        text: "linked words".into(),
        links: vec![(0..12, "https://example.invalid".into())],
        ..Default::default()
    };
    let (_, cx) = cx.add_window_view(|_, _| Page {
        nodes: vec![Node::Paragraph(line)].into(),
        selection: selection.clone(),
        links: links.clone(),
        custom: false,
    });
    cx.run_until_parked();
    let (start, end) = cx.update(|_, app| {
        let state = selection.read(app);
        let layouts = state.layouts.borrow();
        let layout = &layouts[0].1;
        let point_at = |ix| {
            let p = layout.position_for_index(ix).unwrap();
            point(p.x, p.y + px(2.0))
        };
        (point_at(1), point_at(6))
    });
    cx.simulate_mouse_down(start, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(end, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_up(end, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    assert_eq!(links.get(), 0);
    assert!(!selection.read_with(cx, |state, _| state.dragged()));
    cx.simulate_mouse_down(start, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_up(start, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    assert_eq!(links.get(), 1);
    let jitter = start + point(px(1.0), px(0.0));
    cx.simulate_mouse_down(start, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(jitter, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_up(jitter, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    assert_eq!(links.get(), 2);
}

#[gpui::test]
fn special_code_and_math_keep_later_text_aligned(cx: &mut TestAppContext) {
    cx.update(crate::init_for_tests);
    {
        let selection = cx.new(MarkdownSelection::new);
        let nodes = vec![
            Node::Code {
                language: Some("mermaid".into()),
                text: "graph LR; A-->B".into(),
                from: 0,
            },
            Node::Paragraph(Inline {
                text: "before x after".into(),
                styles: vec![(7..8, crate::Style::Math)],
                ..Default::default()
            }),
            words("中文"),
        ]
        .into();
        let (_, cx) = cx.add_window_view(|_, _| Page {
            nodes,
            selection: selection.clone(),
            links: Default::default(),
            custom: false,
        });
        cx.run_until_parked();
        selection.update(cx, |state, cx| state.select_all(cx));
        cx.run_until_parked();
        selection.read_with(cx, |state, _| {
            let layouts = state.layouts.borrow();
            let mut ranges = layouts
                .iter()
                .map(|(range, _, _)| range.clone())
                .collect::<Vec<_>>();
            ranges.sort_by_key(|range| range.start);
            assert_eq!(
                ranges
                    .iter()
                    .map(|range| &state.text.words[range.clone()])
                    .collect::<Vec<_>>(),
                ["before x after", "中文"]
            );
            assert_eq!(state.regions.borrow().len(), 1);
            assert!(state.selected_text().ends_with("中文"));
        });
    }
}

#[test]
fn empty_table_cells_keep_the_previous_row_separator() {
    let text = Text::of(&[Node::Table {
        aligns: vec![],
        head: vec![vec![words("heading")]],
        rows: vec![vec![vec![]], vec![vec![words("last")]]],
    }]);
    assert_eq!(text.words, "heading\n\nlast");
}

#[gpui::test]
fn right_margin_drag_reaches_the_short_final_paragraph(cx: &mut TestAppContext) {
    cx.update(crate::init_for_tests);
    let selection = cx.new(MarkdownSelection::new);
    let (_, cx) = cx.add_window_view(|_, _| Page {
        nodes: vec![
            words("a long first paragraph that almost fills its line"),
            words("last"),
        ]
        .into(),
        selection: selection.clone(),
        links: Default::default(),
        custom: false,
    });
    cx.run_until_parked();
    let (start, end) = selection.read_with(cx, |state, _| {
        let layouts = state.layouts.borrow();
        let start = layouts[1].1.position_for_index(0).unwrap();
        (
            start + point(px(0.0), px(2.0)),
            point(px(500.0), start.y + px(2.0)),
        )
    });
    cx.simulate_mouse_down(start, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(end, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_up(end, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    assert_eq!(
        selection.read_with(cx, |state, _| state.selected_text()),
        "last"
    );
}

#[gpui::test]
fn list_markers_and_atomic_blocks_have_pointer_ranges(cx: &mut TestAppContext) {
    cx.update(crate::init_for_tests);
    let selection = cx.new(MarkdownSelection::new);
    let (_, cx) = cx.add_window_view(|_, _| Page {
        nodes: vec![
            Node::List {
                start: Some(1),
                items: vec![(None, vec![words("one")]), (Some(true), vec![words("two")])],
            },
            Node::Math("x+y".into()),
            Node::Toc(vec![crate::tree::DocumentHeading {
                depth: 1,
                title: "Title".into(),
                at: 0,
            }]),
            Node::Alert {
                kind: crate::tree::AlertKind::Note,
                body: vec![words("body")],
            },
            Node::Footnote {
                label: "1".into(),
                body: vec![words("note")],
            },
        ]
        .into(),
        selection: selection.clone(),
        links: Default::default(),
        custom: false,
    });
    cx.run_until_parked();
    let (start, end) = selection.read_with(cx, |state, _| {
        let regions = state.regions.borrow();
        assert_eq!(regions.len(), 3);
        let layouts = state.layouts.borrow();
        let words = layouts
            .iter()
            .map(|(range, _, _)| state.text.words[range.clone()].to_owned())
            .collect::<Vec<_>>();
        for expected in ["Title", "Note", "[1]", "note"] {
            assert!(
                words.iter().any(|word| word == expected),
                "{expected:?}: {words:?}"
            );
        }
        (
            regions[0].1.origin + point(px(1.0), px(1.0)),
            layouts[0].1.position_for_index(3).unwrap() + point(px(0.0), px(2.0)),
        )
    });
    cx.simulate_mouse_down(start, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(end, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_up(end, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    assert_eq!(
        selection.read_with(cx, |state, _| state.selected_text()),
        "1. one"
    );
}

#[gpui::test]
#[should_panic(expected = "selection cannot be combined")]
fn custom_code_rejects_renderer_selection(cx: &mut TestAppContext) {
    cx.update(crate::init_for_tests);
    let selection = cx.new(MarkdownSelection::new);
    let (_, cx) = cx.add_window_view(|_, _| Page {
        nodes: vec![words("body")].into(),
        selection,
        links: Default::default(),
        custom: true,
    });
    cx.run_until_parked();
}

#[gpui::test]
fn context_presses_report_the_link_picture_and_position(cx: &mut TestAppContext) {
    use crate::{HeldPicture, Pictures};
    struct ContextPage {
        selection: Entity<MarkdownSelection>,
        seen: Rc<RefCell<Vec<MarkdownContext>>>,
        pictures: Rc<Pictures>,
    }
    impl Render for ContextPage {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            let seen = self.seen.clone();
            MarkdownRenderer::blocks(
                "context-body",
                vec![
                    Node::Paragraph(Inline {
                        text: "linked words".into(),
                        links: vec![(0..12, "https://example.invalid/link".into())],
                        ..Default::default()
                    }),
                    Node::Image {
                        alt: "loaded".into(),
                        url: "cid:one".into(),
                        link: Some("https://example.invalid/picture".into()),
                        at: 0,
                    },
                ]
                .into(),
            )
            .selection(&self.selection)
            .pictures(self.pictures.clone())
            .on_context(move |context, _, _| seen.borrow_mut().push(context))
        }
    }
    cx.update(crate::init_for_tests);
    let selection = cx.new(MarkdownSelection::new);
    let seen = Rc::new(RefCell::new(Vec::new()));
    let mut bytes = std::io::Cursor::new(Vec::new());
    image::RgbaImage::from_pixel(4, 4, image::Rgba([10, 20, 30, 255]))
        .write_to(&mut bytes, image::ImageFormat::Png)
        .unwrap();
    let pictures = Rc::new(Pictures::from([(
        "cid:one".into(),
        Ok(HeldPicture::new(gpui::ImageFormat::Png, bytes.into_inner())),
    )]));
    let (_, cx) = cx.add_window_view(|_, _| ContextPage {
        selection: selection.clone(),
        seen: seen.clone(),
        pictures,
    });
    cx.run_until_parked();
    let link = selection.read_with(cx, |state, _| {
        let layouts = state.layouts.borrow();
        layouts[0].1.position_for_index(3).unwrap() + point(px(0.0), px(2.0))
    });
    cx.simulate_mouse_down(link, MouseButton::Right, Modifiers::none());
    cx.simulate_mouse_up(link, MouseButton::Right, Modifiers::none());
    cx.run_until_parked();
    let picture = cx
        .debug_bounds("markdown-picture")
        .expect("the held picture is loaded")
        .center();
    cx.simulate_mouse_down(picture, MouseButton::Right, Modifiers::none());
    cx.simulate_mouse_up(picture, MouseButton::Right, Modifiers::none());
    let seen = seen.borrow();
    assert_eq!(seen.len(), 2);
    assert_eq!(seen[0].position, link);
    assert_eq!(
        seen[0].link.as_deref(),
        Some("https://example.invalid/link")
    );
    assert!(seen[0].image.is_none());
    assert_eq!(seen[1].position, picture);
    assert_eq!(seen[1].image.as_deref(), Some("cid:one"));
    assert_eq!(
        seen[1].link.as_deref(),
        Some("https://example.invalid/picture")
    );
}

#[gpui::test]
fn upward_drag_past_an_atomic_first_block_includes_it(cx: &mut TestAppContext) {
    cx.update(crate::init_for_tests);
    let selection = cx.new(MarkdownSelection::new);
    let (_, cx) = cx.add_window_view(|_, _| Page {
        nodes: vec![Node::Math("x+y".into()), words("last")].into(),
        selection: selection.clone(),
        links: Default::default(),
        custom: false,
    });
    cx.run_until_parked();
    let (start, end) = selection.read_with(cx, |state, _| {
        let layouts = state.layouts.borrow();
        let start = layouts[0].1.position_for_index(4).unwrap() + point(px(0.0), px(2.0));
        let bounds = state.regions.borrow()[0].1;
        (start, point(bounds.right(), bounds.top() - px(10.0)))
    });
    cx.simulate_mouse_down(start, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(end, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_up(end, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    assert_eq!(
        selection.read_with(cx, |state, _| state.selected_text()),
        "x+y\n\nlast"
    );
}

#[gpui::test]
fn dragging_outside_the_body_keeps_tracking_until_release(cx: &mut TestAppContext) {
    cx.update(crate::init_for_tests);
    let selection = cx.new(MarkdownSelection::new);
    let (_, cx) = cx.add_window_view(|_, _| Page {
        nodes: vec![words("first"), words("last")].into(),
        selection: selection.clone(),
        links: Default::default(),
        custom: false,
    });
    cx.run_until_parked();
    let start = selection.read_with(cx, |state, _| {
        let layouts = state.layouts.borrow();
        let p = layouts[0].1.position_for_index(1).unwrap();
        point(p.x, p.y + px(2.0))
    });
    let outside = point(px(500.0), px(500.0));
    cx.simulate_mouse_down(start, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    cx.simulate_mouse_move(outside, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_up(outside, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    assert_eq!(
        selection.read_with(cx, |state, _| state.selected_text()),
        "irst\n\nlast"
    );
}
