use gpui::{
    AnyElement, App, Bounds, IntoElement as _, ParentElement as _, Pixels, Point, Styled as _,
    canvas, div, prelude::*,
};

use super::MarkdownSelection;

impl MarkdownSelection {
    pub(super) fn index_at(&self, point: Point<Pixels>) -> usize {
        let layouts = self.layouts.borrow();
        let regions = self.regions.borrow();
        let text = layouts.iter().map(|(range, layout, mapping)| {
            let at = layout.index_for_position(point).unwrap_or_else(|at| at);
            let at = mapping.as_ref().map_or(at, |map| map(at));
            (layout.bounds(), range.start + at.min(range.len()))
        });
        let atomic = regions.iter().map(|(range, bounds)| {
            let at = if point.y < bounds.top()
                || (point.y < bounds.bottom() && point.x < bounds.center().x)
            {
                range.start
            } else {
                range.end
            };
            (*bounds, at)
        });
        let nearest = text.chain(atomic).min_by(|(a, _), (b, _)| {
            let (ay, ax) = distance(*a, point);
            let (by, bx) = distance(*b, point);
            ay.total_cmp(&by).then_with(|| ax.total_cmp(&bx))
        });
        let mut at = nearest.map_or(0, |(_, at)| at);
        while !self.text.words.is_char_boundary(at) {
            at -= 1;
        }
        at
    }
}

fn distance(bounds: Bounds<Pixels>, point: Point<Pixels>) -> (f32, f32) {
    let x = (bounds.left() - point.x)
        .max(point.x - bounds.right())
        .max(Pixels::ZERO);
    let y = (bounds.top() - point.y)
        .max(point.y - bounds.bottom())
        .max(Pixels::ZERO);
    (y.into(), x.into())
}

impl super::super::Draw<'_> {
    pub(in super::super) fn select_region(&self, element: AnyElement, cx: &App) -> AnyElement {
        let piece = self.piece.get();
        self.piece.set(piece + 1);
        let Some(selection) = self.selection.clone() else {
            return element;
        };
        let highlight = selection.read(cx).highlight(piece, cx);
        div()
            .relative()
            .when_some(
                highlight
                    .first()
                    .and_then(|(_, style)| style.background_color),
                |root, color| root.bg(color),
            )
            .child(element)
            .child(
                canvas(
                    move |bounds, _, cx| {
                        let state = selection.read(cx);
                        if let Some(range) = state.text.runs.get(piece) {
                            state.regions.borrow_mut().push((range.clone(), bounds));
                        }
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .size_full(),
            )
            .into_any_element()
    }
}
