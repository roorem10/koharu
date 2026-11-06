use image::{DynamicImage, GrayImage};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum InkEvidence {
    Blank,
    NearBlank,
    PossibleText,
}

impl InkEvidence {
    pub(super) fn is_blank(self) -> bool {
        self == Self::Blank
    }

    pub(super) fn reject_output(self, text: &str) -> bool {
        if self == Self::PossibleText {
            return false;
        }
        let mut visible = text.chars().filter(|character| !character.is_whitespace());
        let Some(first) = visible.next() else {
            return false;
        };
        let mut count = 1;
        let mut repeated = true;
        let mut punctuation = is_punctuation(first);
        for character in visible {
            count += 1;
            repeated &= character == first;
            punctuation &= is_punctuation(character);
        }
        count >= 2 && (repeated || punctuation)
    }
}

fn is_punctuation(character: char) -> bool {
    character.is_ascii_punctuation()
        || matches!(
            character,
            '\u{ff01}'..='\u{ff0f}' | '\u{ff1a}'..='\u{ff20}'
                | '\u{ff3b}'..='\u{ff40}' | '\u{ff5b}'..='\u{ff65}'
                | '。' | '、' | '…' | '‥' | '・' | '「' | '」' | '『' | '』'
                | '〈' | '〉' | '《' | '》' | '【' | '】' | '〜' | 'ー' | '—' | '–' | '―'
                | '“' | '”' | '‘' | '’' | '☐' | '□' | '▢' | '▣' | '�'
        )
}

pub(super) fn measure(image: &DynamicImage) -> InkEvidence {
    let image = image.to_luma8();
    let (width, height) = image.dimensions();
    if width == 0 || height == 0 {
        return InkEvidence::Blank;
    }
    let (low, high) = image.as_raw().iter().fold((u8::MAX, u8::MIN), |(low, high), &value| {
        (low.min(value), high.max(value))
    });
    if high - low <= 2 || fine_dot_screen(&image) {
        return InkEvidence::Blank;
    }

    // A 9x9 median follows the local paper/shading level without making a halo
    // around dust. Count either ink polarity, at only >2/255 contrast: even a
    // three-level one-pixel stroke survives. Never use a crop-area percentage;
    // a small glyph in a large hand-drawn box must still reach the model.
    let mut foreground = 0;
    let mut sharp_edges = 0;
    for y in 0..height {
        let top = y.saturating_sub(4);
        let bottom = y.saturating_add(5).min(height);
        let mut histogram = [0_u32; 256];
        for row in top..bottom {
            for column in 0..width.min(5) {
                histogram[usize::from(image.get_pixel(column, row)[0])] += 1;
            }
        }
        for x in 0..width {
            let left = x.saturating_sub(4);
            let right = x.saturating_add(5).min(width);
            let middle = (right - left) * (bottom - top) / 2;
            let mut count = 0;
            let background = histogram
                .iter()
                .position(|frequency| {
                    count += *frequency;
                    count > middle
                })
                .unwrap_or(0) as i16;
            let contrast = i16::from(image.get_pixel(x, y)[0]) - background;
            if contrast.abs() > 2 {
                foreground += 1;
            }
            let pixel = image.get_pixel(x, y)[0];
            sharp_edges += u32::from(x > 0 && pixel.abs_diff(image.get_pixel(x - 1, y)[0]) > 2);
            sharp_edges += u32::from(y > 0 && pixel.abs_diff(image.get_pixel(x, y - 1)[0]) > 2);
            // Two pixels can be a faint stroke, ellipsis or dakuten. A lone dust
            // pixel has at most four sharp edges; more also preserves straight
            // broad strokes spanning the crop, which a median does not alter.
            if foreground >= 2 || sharp_edges > 4 {
                return InkEvidence::PossibleText;
            }
            if x >= 4 {
                for row in top..bottom {
                    histogram[usize::from(image.get_pixel(x - 4, row)[0])] -= 1;
                }
            }
            if let Some(column) = x.checked_add(5).filter(|column| *column < width) {
                for row in top..bottom {
                    histogram[usize::from(image.get_pixel(column, row)[0])] += 1;
                }
            }
        }
    }
    if foreground == 0 && sharp_edges == 0 {
        InkEvidence::Blank
    } else if foreground == 1 && u64::from(width) * u64::from(height) >= 256 {
        // A single isolated pixel in a >=256-pixel crop still runs OCR. Only
        // repetitive output is rejected; ordinary text and a lone mark survive.
        InkEvidence::NearBlank
    } else {
        InkEvidence::PossibleText
    }
}

fn fine_dot_screen(image: &GrayImage) -> bool {
    let (width, height) = image.dimensions();
    // Recognize only a dense grid of single-pixel dots with >=8 repeats on BOTH
    // axes. Letters, a row of ellipsis dots, coarse/rotated screens and any added
    // stroke fail open. One level of phase error tolerates rounding, but cannot
    // hide the three-level faint strokes accepted by the contrast check above.
    if width < 32 || height < 32 {
        return false;
    }
    (2..=4).any(|period| {
        let tile: Vec<_> = (0..period)
            .flat_map(|y| (0..period).map(move |x| image.get_pixel(x, y)[0]))
            .collect();
        let low = *tile.iter().min().unwrap();
        let high = *tile.iter().max().unwrap();
        if high - low < 8 {
            return false;
        }
        let dark = tile.iter().filter(|&&value| value.abs_diff(low) <= 1).count();
        let light = tile.iter().filter(|&&value| value.abs_diff(high) <= 1).count();
        if dark + light != tile.len() || (dark != 1 && light != 1) {
            return false;
        }
        image.enumerate_pixels().all(|(x, y, pixel)| {
            pixel[0].abs_diff(image.get_pixel(x % period, y % period)[0]) <= 1
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Luma;

    fn evidence(image: GrayImage) -> InkEvidence {
        measure(&DynamicImage::ImageLuma8(image))
    }

    fn screen(period: u32, inverted: bool) -> GrayImage {
        GrayImage::from_fn(64, 64, |x, y| {
            let value = if x % period == 0 && y % period == 0 { 40 } else { 220 };
            Luma([if inverted { 255 - value } else { value }])
        })
    }

    #[test]
    fn flat_paper_black_fill_and_low_noise_have_no_ink() {
        for value in [0, 64, 128, 255] {
            assert!(evidence(GrayImage::from_pixel(64, 64, Luma([value]))).is_blank());
        }
        let noise = GrayImage::from_fn(64, 64, |x, y| {
            Luma([127 + ((x * 13 + y * 7) % 3) as u8])
        });
        assert!(evidence(noise).is_blank());
        let gradient = GrayImage::from_fn(64, 64, |x, y| {
            Luma([80 + (x / 4 + y / 8) as u8])
        });
        assert!(evidence(gradient).is_blank());
    }

    #[test]
    fn thin_faint_strokes_and_small_real_punctuation_survive_both_polarities() {
        for (paper, ink) in [(250, 247), (5, 8), (255, 0), (0, 255)] {
            let mut image = GrayImage::from_pixel(128, 128, Luma([paper]));
            for y in 50..58 {
                image.put_pixel(63, y, Luma([ink]));
            }
            assert_eq!(evidence(image), InkEvidence::PossibleText);

            let mut image = GrayImage::from_pixel(128, 128, Luma([paper]));
            for x in 60..62 {
                for y in 60..62 {
                    image.put_pixel(x, y, Luma([ink]));
                }
            }
            let measured = evidence(image);
            assert_eq!(measured, InkEvidence::PossibleText);
            assert!(!measured.reject_output("…！？"));
            assert!(!measured.reject_output("あああ"));
        }
    }

    #[test]
    fn straight_crop_spanning_strokes_are_not_hidden_by_the_median() {
        for (paper, ink) in [(250, 247), (5, 8), (255, 0), (0, 255)] {
            let image = GrayImage::from_fn(64, 64, |x, _| {
                Luma([if (28..36).contains(&x) { ink } else { paper }])
            });
            assert_eq!(evidence(image), InkEvidence::PossibleText);
        }
    }

    #[test]
    fn isolated_dust_is_only_rejected_when_output_is_repetitive() {
        let mut image = GrayImage::from_pixel(64, 64, Luma([255]));
        image.put_pixel(32, 32, Luma([0]));
        let measured = evidence(image);
        assert_eq!(measured, InkEvidence::NearBlank);
        assert!(!measured.is_blank());
        for output in ["あ あ\nあ", "....", "！？…", "．，！", "□ ☐"] {
            assert!(measured.reject_output(output));
        }
        for output in ["", " ", "あ", "!", "待って…", "こんにちは"] {
            assert!(!measured.reject_output(output));
        }
        let mut tiny = GrayImage::from_pixel(8, 8, Luma([255]));
        tiny.put_pixel(4, 4, Luma([0]));
        assert_eq!(evidence(tiny), InkEvidence::PossibleText);
    }

    #[test]
    fn only_fine_regular_dot_screens_are_blank() {
        for inverted in [false, true] {
            for period in 2..=4 {
                assert!(evidence(screen(period, inverted)).is_blank());
                let original = screen(period, inverted);
                for offset in 0..period {
                    let partial = GrayImage::from_fn(33, 35, |x, y| {
                        *original.get_pixel(x + offset, y + offset)
                    });
                    assert!(evidence(partial).is_blank());
                }
                let mut with_text = screen(period, inverted);
                // A very faint, one-pixel-wide stroke over the tone must break
                // the screen match even when the tone already has dark pixels.
                for y in 20..30 {
                    let old = with_text.get_pixel(21, y)[0];
                    with_text.put_pixel(21, y, Luma([old + 3]));
                }
                assert_eq!(evidence(with_text), InkEvidence::PossibleText);
            }
            assert_eq!(evidence(screen(6, inverted)), InkEvidence::PossibleText);
        }
        let irregular = GrayImage::from_fn(64, 64, |x, y| {
            Luma([if (x * 13 + y * 7) % 11 == 0 { 40 } else { 220 }])
        });
        assert_eq!(evidence(irregular), InkEvidence::PossibleText);
    }

    #[test]
    fn repeated_punctuation_is_not_a_two_dimensional_screen() {
        for count in [2, 3, 14] {
            let mut image = GrayImage::from_pixel(64, 64, Luma([255]));
            for dot in 0..count {
                image.put_pixel(4 + dot * 4, 32, Luma([0]));
            }
            let measured = evidence(image);
            assert_eq!(measured, InkEvidence::PossibleText);
            assert!(!measured.reject_output("…………"));
        }
    }
}
