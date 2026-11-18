//! Booharu's part of the canvas: the O tool, whose box dragged out over the
//! page the canvas answers as `Effect::Booharu`, for the host to make a text
//! block of. The Booharu settings the canvas needs come with the first one it
//! does, pushed by the host whenever they're saved.

use koharu_scene::PageId;
use kurbo::{Point, Rect, Size};
use ui_events::keyboard::Modifiers;

use crate::{
    draw::overlay::Overlay,
    gesture::{self, Context, GestureLook, Press},
    view::Sample,
};

/// What one of Booharu's gestures did, for the host to act on.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Effect {
    /// A box drawn with the O tool over `rect` of `page`, in page pixels, cut
    /// to the page, with the modifiers held as the pointer lifted, by which
    /// the host picks what the box goes through.
    Box {
        page: PageId,
        rect: Rect,
        modifiers: BoxModifiers,
    },
}

/// The modifiers held as an O tool box's pointer lifted.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BoxModifiers {
    pub alt: bool,
    pub shift: bool,
}

/// A box shorter than this across or down on screen once cut to the page, in
/// device-independent pixels, is no box, as Booharu 0.0.4's O tool ignored
/// one.
const SMALLEST: f64 = 4.0;

/// A press with the O tool starts a box.
pub(crate) fn press(sample: Sample) -> Press {
    Press::starts(gesture::Gesture::Booharu(OcrBox {
        start: sample.page,
        current: sample.page,
        modifiers: sample.modifiers,
    }))
}

/// The O tool's box being dragged out, in page points, and the modifiers the
/// pointer last held. As every gesture, a cancel or another tool drops it
/// unmade.
pub(crate) struct OcrBox {
    start: Point,
    current: Point,
    modifiers: Modifiers,
}

impl OcrBox {
    pub fn moved(&mut self, samples: &[Sample]) {
        if let Some(last) = samples.last() {
            self.current = last.page;
            self.modifiers = last.modifiers;
        }
    }

    /// The box on a page of `size`, cut to it.
    fn cut(&self, size: Size) -> Rect {
        Rect::from_points(self.start, self.current).intersect(size.to_rect())
    }

    /// The box made as the pointer lifts, which the canvas moves it to first,
    /// with the modifiers held then; none when it's shorter than `SMALLEST`
    /// across or down.
    pub fn finish(self, cx: &Context) -> Vec<crate::Effect> {
        let rect = self.cut(cx.snapshot.size());
        let smallest = cx.view.page_length(SMALLEST);
        if rect.width() < smallest || rect.height() < smallest {
            return Vec::new();
        }
        vec![crate::Effect::Booharu(Effect::Box {
            page: cx.snapshot.page().id,
            rect,
            modifiers: BoxModifiers {
                alt: self.modifiers.alt(),
                shift: self.modifiers.shift(),
            },
        })]
    }

    /// The box as it would be made, cut to the page, dashed in the
    /// selection's colour as the Type tool's frame being dragged out is.
    pub fn preview(&self, cx: &Context, overlay: &mut Overlay) {
        let rect = self.cut(cx.snapshot.size());
        if rect.area() > 0.0 {
            overlay.dashed(rect);
        }
    }

    pub fn look(&self) -> GestureLook {
        GestureLook::Rect(Rect::from_points(self.start, self.current))
    }
}
