//! The quality check's renderer half: a text's sizes and whether it runs
//! past its box, as the renderer draws it, in page pixels.
//!
//! It lays the text out as a render does, through the caller's renderer,
//! whose `[renderer]` fonts and retained layouts the canvas and exports
//! share, so a text the canvas drew lays out again from what it kept. It
//! never takes `Renderer::get()`, whose fonts may not be the user's.

use std::ops::Range;

use anyhow::Result;
use koharu_renderer::Renderer;
use koharu_scene::{LayerId, Text};
use kurbo::Affine;

/// The size the renderer sets a text at whose style sets none.
const DEFAULT_SIZE: f64 = 24.0;

/// A text as it's drawn, in page pixels.
#[derive(Clone, Debug, PartialEq)]
pub struct Measured {
    /// Whether it runs past its box.
    pub overflowed: bool,
    /// The size its box sets it at: its style's size, 24 when it sets
    /// none, at the scale fitting drew it at.
    pub base: f32,
    /// Its text in runs of one size, in order: the renderer's runs, those
    /// of one size side by side merged, as a fallback font or a sideways
    /// glyph splits a run without changing its size, each trimmed, and the
    /// blank ones left out.
    pub runs: Vec<Sized>,
}

/// Text drawn at one size, in page pixels.
#[derive(Clone, Debug, PartialEq)]
pub struct Sized {
    pub text: String,
    pub size: f32,
}

/// `text`, which `layer` holds, as `renderer` draws it; none when it has no
/// characters. A run's size is in the text's own units, which its transform
/// scales by the square root of its determinant on the page, as PSD export
/// scales it.
///
/// It lays out on the calling thread, which must be one that may block: a
/// bundled font a text asks for is downloaded the first time.
pub fn measure(renderer: &Renderer, layer: LayerId, text: &Text) -> Result<Option<Measured>> {
    let Some(laid) = renderer.layout(layer, text)? else {
        return Ok(None);
    };
    let page = Affine::from(text.transform).determinant().abs().sqrt() as f32;
    let units: Vec<u16> = text.content.text().encode_utf16().collect();
    let mut merged: Vec<(Range<usize>, f32)> = Vec::new();
    for run in &laid.runs {
        match merged.last_mut() {
            Some((range, size)) if *size == run.size && run.range.start >= range.end => {
                range.end = run.range.end;
            }
            _ => merged.push((run.range.clone(), run.size)),
        }
    }
    let runs = merged
        .into_iter()
        .filter_map(|(range, size)| {
            let text = String::from_utf16_lossy(units.get(range)?);
            let text = text.trim();
            (!text.is_empty()).then(|| Sized {
                text: text.to_owned(),
                size: size * page,
            })
        })
        .collect();
    let set = text.style.chars.size.unwrap_or(DEFAULT_SIZE) as f32;
    Ok(Some(Measured {
        overflowed: laid.overflowed,
        base: set * laid.scale * page,
        runs,
    }))
}
