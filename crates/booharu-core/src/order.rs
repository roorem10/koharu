//! Reading order among what a balloon, a panel or a page holds: where a
//! block a box adds goes to be read in order among its neighbours, as
//! Booharu 0.0.4's O tool ordered a balloon's boxes. Koharu's scene reads a
//! page's layers depth first (`Project::blocks`, `Project::story`), so a
//! layer's place among its siblings is its place in the reading: Story
//! numbers a block there and a run's translation reads it there. The
//! agent's box (`booharu::boxes::add_box`) and the K tool's box
//! (koharu-canvas's `booharu::in_reading_order`) both place their block by
//! this one rule, so the two never drift apart.

use koharu_scene::{Content, Layer};

/// Where among `siblings` a block at `rect` goes to be read in order: just
/// before the first sibling whose first text it is read before, else last.
/// Among a balloon's texts, `in_balloon`, vertical text reads right to left
/// then top to bottom, and horizontal text top to bottom then left to right,
/// as Booharu 0.0.4 ordered a balloon's boxes. Among a panel's or the page's
/// balloons and texts, as a manga page reads, those sharing a row read right
/// to left and the rows top to bottom, so a vertical line low on the page
/// isn't read before a balloon above it. A sibling holding no text is no
/// part of the reading.
#[must_use]
pub fn reading_index(
    siblings: &[Layer],
    rect: kurbo::Rect,
    vertical: bool,
    in_balloon: bool,
) -> usize {
    siblings
        .iter()
        .position(|sibling| {
            first_text(sibling).is_some_and(|text| {
                let order = match (in_balloon, vertical) {
                    (true, true) => text
                        .x0
                        .total_cmp(&rect.x0)
                        .then(rect.y0.total_cmp(&text.y0)),
                    (true, false) => rect
                        .y0
                        .total_cmp(&text.y0)
                        .then(rect.x0.total_cmp(&text.x0)),
                    // One row: the one further right first.
                    (false, _) if rect.y0 < text.y1 && text.y0 < rect.y1 => text
                        .center()
                        .x
                        .total_cmp(&rect.center().x)
                        .then(rect.y0.total_cmp(&text.y0)),
                    (false, _) => rect.y0.total_cmp(&text.y0),
                };
                order.is_lt()
            })
        })
        .unwrap_or(siblings.len())
}

/// Where the first text in `layer`, itself or one inside it, lies on the
/// page.
fn first_text(layer: &Layer) -> Option<kurbo::Rect> {
    let own = match &layer.content {
        Content::Block(block) => block.source.bounds(),
        Content::Text(text) => text.bounds(),
        _ => None,
    };
    own.or_else(|| layer.children.iter().find_map(first_text))
}

#[cfg(test)]
mod tests {
    use koharu_scene::{Edit, Op, Session};
    use serde_json::{Value, json};

    use super::*;

    /// A text block whose source fills `(x, y, width, height)`.
    fn block(x: f64, y: f64, width: f64, height: f64) -> Value {
        json!({"kind": "block", "source": {"content": "あ", "frame": {"box": {"bounds":
            {"x": x, "y": y, "width": width, "height": height}}}}})
    }

    #[test]
    fn a_block_goes_where_it_reads_among_a_balloons_texts_and_a_pages_rows() {
        let dir = tempfile::tempdir().expect("a folder");
        let mut session = Session::create(dir.path().join("test.khr")).expect("a project");
        // A balloon at the top holding two vertical lines, right then left,
        // and a text block lower down at the left, on the page.
        let ops = [
            json!({"op": "add_page", "id": "$page", "index": 0,
                "size": {"width": 800, "height": 800}}),
            json!({"op": "add_layer", "id": "$balloon", "page": "$page", "index": 0,
                "layer": {"kind": "balloon", "outline": "M300 0 H700 V300 H300 Z"}}),
            json!({"op": "add_layer", "page": "$page", "parent": "$balloon", "index": 0,
                "layer": block(600.0, 20.0, 40.0, 200.0)}),
            json!({"op": "add_layer", "page": "$page", "parent": "$balloon", "index": 1,
                "layer": block(400.0, 20.0, 40.0, 200.0)}),
            json!({"op": "add_layer", "page": "$page", "index": 1,
                "layer": block(20.0, 500.0, 200.0, 40.0)}),
        ]
        .into_iter()
        .map(|op| serde_json::from_value::<Op>(op).expect("an op"))
        .collect();
        let change = session
            .apply(Edit {
                label: "test".to_owned(),
                ops,
            })
            .expect("a page");
        let project = session.project();
        let page = project.page(change.pages.added[0]).expect("the page");
        let balloon = &page.layers[0].children;
        // Vertical text in a balloon reads right to left.
        let line = |x: f64| kurbo::Rect::new(x, 20.0, x + 40.0, 220.0);
        assert_eq!(reading_index(balloon, line(650.0), true, true), 0);
        assert_eq!(reading_index(balloon, line(500.0), true, true), 1);
        assert_eq!(reading_index(balloon, line(320.0), true, true), 2);
        // Horizontal text in a balloon reads top to bottom.
        let row = |y: f64| kurbo::Rect::new(350.0, y, 650.0, y + 10.0);
        assert_eq!(reading_index(balloon, row(5.0), false, true), 0);
        assert_eq!(reading_index(balloon, row(100.0), false, true), 2);
        // On the page, a row reads right to left and the rows top to bottom:
        // a line right of the balloon, in its row, comes before it, though
        // lower; one left of it, though higher, after it; and one low at
        // the right after the text above it, though right of it.
        let page_line = |x: f64, y: f64| kurbo::Rect::new(x, y, x + 40.0, y + 200.0);
        let reads = |rect| reading_index(&page.layers, rect, true, false);
        assert_eq!(reads(page_line(740.0, 50.0)), 0);
        assert_eq!(reads(page_line(100.0, 10.0)), 1);
        assert_eq!(reads(page_line(700.0, 580.0)), 2);
        // A holder with nothing in it takes the block as its only child.
        assert_eq!(reading_index(&[], line(500.0), true, true), 0);
    }
}
