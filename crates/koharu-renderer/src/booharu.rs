//! Booharu's part of the renderer: the names font families go by in other
//! languages, which Booharu's font picker shows muted after a family's name
//! and searches, so typing 游ゴシック finds Yu Gothic, with Koharu's
//! `FontFamily` as it is; which of a family's faces sets a text asked for
//! bold or italic, so they stay apart (`faces`); where the runs of a line of
//! mixed sizes stand, and how far faked bold and slant reach past their
//! glyphs (`lines`); a span's own values, which a span of Koharu's text
//! carries in one field (`SpanStyle`): its second border, its gradient fill
//! with what the gradient spreads over (`Unit`), and its letter spacing,
//! which follows each cluster (`track`); and where gradient fills spread
//! and how they paint (`paint`).

use std::{
    collections::{BTreeMap, HashMap},
    path::PathBuf,
    sync::LazyLock,
};

use anyhow::Result;
use fontique::{Collection, CollectionOptions, SourceCache};
use koharu_scene::{CharStyle, Color, Gradient, Rgba};
use parking_lot::Mutex;
use skrifa::{FontRef, MetadataProvider, string::StringId};

pub(crate) use self::{
    faces::{Traits, better, repick},
    lines::{faux_reach, shared_baseline},
    paint::{opaque, paint},
};

mod faces;
mod lines;
mod paint;

/// A span's Booharu values, which a span of Koharu's text carries in one
/// field.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct SpanStyle {
    /// The colour and width of a second border around the stroke, which
    /// only a stroke has.
    pub outer_stroke: Option<(Color, f32)>,
    /// A gradient fill, drawn in place of the span's flat fill, with what it
    /// spreads over.
    pub gradient: Option<(Gradient, Unit)>,
    /// Letter spacing in thousandths of an em.
    pub tracking: f32,
}

/// What a gradient fill spreads over: the ink of the whole text, for a
/// gradient the text's style gives; or for a span's own gradient, the ink
/// of the run of consecutive spans given the same one, across their lines
/// and sizes, named by the run's first span.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Unit {
    Text,
    Own(usize),
}

impl SpanStyle {
    /// The values of the span at `index` of a text, styled `style`, its
    /// `own` values over the text's; `before` is the span before it.
    pub(crate) fn new(
        style: &CharStyle,
        own: &CharStyle,
        index: usize,
        before: Option<&Self>,
    ) -> Self {
        let gradient = style
            .fill_gradient
            .clone()
            .filter(|gradient| !gradient.stops.is_empty());
        let gradient = gradient.map(|gradient| {
            let unit = match (
                &own.fill_gradient,
                before.and_then(|before| before.gradient.as_ref()),
            ) {
                (None, _) => Unit::Text,
                // The same gradient as the span before's own carries its run on.
                (Some(_), Some((previous, Unit::Own(first)))) if *previous == gradient => {
                    Unit::Own(*first)
                }
                (Some(_), _) => Unit::Own(index),
            };
            (gradient, unit)
        });
        Self {
            outer_stroke: match (style.stroke, style.stroke_width, style.outer_stroke) {
                (Some(_), Some(inner), Some(color)) if inner > 0.0 => style
                    .outer_stroke_width
                    .filter(|width| *width > 0.0)
                    .map(|width| (Color::from(color), width as f32)),
                _ => None,
            },
            gradient,
            tracking: style
                .tracking
                .filter(|tracking| tracking.is_finite())
                .unwrap_or(0.0) as f32,
        }
    }

    /// The letter spacing after each cluster of the span set at `size`.
    pub(crate) fn gap(&self, size: f32) -> f32 {
        size * self.tracking / 1000.0
    }
}

/// The flat colour a span styled `style` fills with: its gradient fill's
/// first stop's, which shows wherever a gradient can't, or its fill.
pub(crate) fn flat_fill(style: &CharStyle) -> Option<Rgba> {
    match &style.fill_gradient {
        Some(gradient) => gradient.stops.first().map(|stop| stop.color).or(style.fill),
        None => style.fill,
    }
}

/// Adds `gap`, a letter spacing, to the advance of the last glyph of each
/// cluster of `glyphs`, each its cluster and advance in the order shaping
/// gave them, so the clusters after it move along and a ligature or a
/// letter with marks takes one gap, its marks staying on it.
pub(crate) fn track<'a>(glyphs: impl IntoIterator<Item = (usize, &'a mut f32)>, gap: f32) {
    if gap == 0.0 {
        return;
    }
    let mut glyphs = glyphs.into_iter().peekable();
    while let Some((cluster, advance)) = glyphs.next() {
        if glyphs.peek().is_none_or(|(next, _)| *next != cluster) {
            *advance += gap;
        }
    }
}

/// Each family with names in other languages, by the name the font list
/// shows, with those names: the installed families', and the bundled
/// catalog's whose faces are on disk already, as a bundled face downloads
/// only when a text asks for it, and never for this. A family with none is
/// left out. It's worked out on a blocking thread: an installed family's
/// names are read once, as Koharu lists the installed families once, and a
/// bundled face's once it's on disk.
pub async fn local_names() -> Result<BTreeMap<String, Vec<String>>> {
    Ok(tokio::task::spawn_blocking(|| {
        let mut found = BTreeMap::new();
        for (family, names) in INSTALLED.iter() {
            merge(&mut found, family, names);
        }
        for (family, path) in crate::fonts::stored_faces() {
            let names = BUNDLED
                .lock()
                .entry(path)
                .or_insert_with_key(|path| match fs_err::read(path) {
                    Ok(data) => local_family_names(&data, 0, &family),
                    Err(error) => {
                        tracing::warn!("couldn't read a bundled font's names: {error:#}");
                        Vec::new()
                    }
                })
                .clone();
            merge(&mut found, &family, &names);
        }
        found
            .into_values()
            .filter(|(_, names)| !names.is_empty())
            .collect()
    })
    .await?)
}

/// The names the font at `index` of the file `data` gives its family in
/// languages other than English, its typographic family names first, then
/// its family names, each once, leaving out its English names and `family`,
/// the name Koharu lists it by. None when the font can't be read.
pub fn local_family_names(data: &[u8], index: u32, family: &str) -> Vec<String> {
    let Ok(font) = FontRef::from_index(data, index) else {
        return Vec::new();
    };
    let ids = [StringId::TYPOGRAPHIC_FAMILY_NAME, StringId::FAMILY_NAME];
    let mut known: Vec<String> = vec![normalize(family)];
    for id in ids {
        known.extend(
            font.localized_strings(id)
                .filter(|string| string.language().is_none_or(english))
                .map(|string| normalize(&string.to_string())),
        );
    }
    let mut names = Vec::new();
    for id in ids {
        for string in font.localized_strings(id) {
            let name = string.to_string();
            let name = name.trim();
            if name.is_empty() || known.contains(&normalize(name)) {
                continue;
            }
            known.push(normalize(name));
            names.push(name.to_owned());
        }
    }
    names
}

/// Each installed family by its name, the system's aliases included, with
/// its names in other languages, as the system had them when first asked.
static INSTALLED: LazyLock<Vec<(String, Vec<String>)>> = LazyLock::new(|| {
    let mut collection = Collection::new(CollectionOptions {
        shared: false,
        system_fonts: true,
    });
    let mut sources = SourceCache::default();
    let families: Vec<String> = collection.family_names().map(str::to_owned).collect();
    families
        .into_iter()
        .filter_map(|family| {
            let info = collection.family_by_name(&family)?;
            let font = info.default_font()?;
            let data = font.load(Some(&mut sources))?;
            let names = local_family_names(data.as_ref(), font.index(), &family);
            Some((family, names))
        })
        .collect()
});

/// A bundled face's names in other languages, by its file, read once: a
/// file of the catalog's pinned snapshot never changes.
static BUNDLED: LazyLock<Mutex<HashMap<PathBuf, Vec<String>>>> = LazyLock::new(Mutex::default);

/// Whether `language`, a BCP 47 tag, is English.
fn english(language: &str) -> bool {
    language == "en" || language.starts_with("en-")
}

/// Adds `names`, `family`'s names in other languages, to `found`, under the
/// family's name as Koharu matches it, spelt as it was first found, each
/// once: Koharu lists an installed family and a bundled one of the same name
/// as one, by the installed one's spelling.
fn merge(found: &mut BTreeMap<String, (String, Vec<String>)>, family: &str, names: &[String]) {
    let (_, known) = found
        .entry(normalize(family))
        .or_insert_with(|| (family.to_owned(), Vec::new()));
    for name in names {
        if !known
            .iter()
            .any(|known| normalize(known) == normalize(name))
        {
            known.push(name.clone());
        }
    }
}

/// A family name as Koharu matches it: trimmed, in any case.
fn normalize(name: &str) -> String {
    name.trim().to_lowercase()
}

/// An installed sans-serif with no variation axes, as tests of faked bold
/// and slant need: the repository's own font is variable. `None` when none
/// is installed.
#[cfg(test)]
pub(crate) fn static_sans() -> Option<crate::fonts::Font> {
    let mut library = crate::fonts::Library::new();
    ["Arial", "DejaVu Sans", "Liberation Sans", "Helvetica"]
        .into_iter()
        .filter_map(|family| library.family(family).ok())
        .find(|font| font.coords.is_empty())
}

/// `static_sans` with what fontique fakes on its Regular face for text asked
/// for `style` and `weight`; `None`, saying why, without one.
#[cfg(test)]
pub(crate) fn faked(style: fontique::FontStyle, weight: f32) -> Option<crate::fonts::Font> {
    const FAMILY: &str = "Booharu Faked Test";
    let mut collection = test_family(FAMILY, &[(fontique::FontStyle::Normal, 400.0)])?;
    let face = collection.family_by_name(FAMILY)?.fonts().first()?.clone();
    let mut font = static_sans()?;
    font.synthesis = face.synthesis(
        fontique::FontWidth::NORMAL,
        style,
        fontique::FontWeight::new(weight),
    );
    Some(font)
}

/// A collection of only `family`, its `faces` each a slant and a weight given
/// to `static_sans`'s file, as tests of which face a text takes need; `None`,
/// saying why, when there's no such sans or its file holds several faces.
#[cfg(test)]
pub(crate) fn test_family(
    family: &str,
    faces: &[(fontique::FontStyle, f32)],
) -> Option<Collection> {
    let Some(sans) = static_sans() else {
        eprintln!("skipped: no static sans-serif is installed");
        return None;
    };
    let mut collection = Collection::new(CollectionOptions {
        shared: false,
        system_fonts: false,
    });
    for &(style, weight) in faces {
        let registered = collection.register_fonts(
            fontique::Blob::from(sans.data.data.as_ref().to_vec()),
            Some(fontique::FontInfoOverride {
                family_name: Some(family),
                style: Some(style),
                weight: Some(fontique::FontWeight::new(weight)),
                ..fontique::FontInfoOverride::default()
            }),
        );
        if registered
            .iter()
            .map(|(_, fonts)| fonts.len())
            .sum::<usize>()
            != 1
        {
            eprintln!(
                "skipped: {}'s file holds several faces",
                sans.post_script_name
            );
            return None;
        }
    }
    Some(collection)
}

#[cfg(test)]
mod tests {
    use super::*;

    const WINDOWS: u16 = 3;
    const MAC: u16 = 1;
    const ENGLISH: u16 = 0x0409;
    const JAPANESE: u16 = 0x0411;
    const KOREAN: u16 = 0x0412;
    const CHINESE: u16 = 0x0804;
    const FAMILY: u16 = 1;
    const TYPOGRAPHIC: u16 = 16;

    /// A font file holding only a `name` table of `records`, each a platform,
    /// a language, a name id, and the name.
    fn font(records: &[(u16, u16, u16, &str)]) -> Vec<u8> {
        let mut strings = Vec::new();
        let mut table = Vec::new();
        let count = u16::try_from(records.len()).expect("a few records");
        for value in [0, count, 6 + 12 * count] {
            table.extend(value.to_be_bytes());
        }
        for &(platform, language, id, name) in records {
            // Windows names are UTF-16 and the Mac's Roman, ASCII here.
            let (encoding, bytes): (u16, Vec<u8>) = match platform {
                WINDOWS => (1, name.encode_utf16().flat_map(u16::to_be_bytes).collect()),
                _ => (0, name.bytes().collect()),
            };
            let length = u16::try_from(bytes.len()).expect("a short name");
            let offset = u16::try_from(strings.len()).expect("short names");
            for value in [platform, encoding, language, id, length, offset] {
                table.extend(value.to_be_bytes());
            }
            strings.extend(bytes);
        }
        table.extend(strings);
        let mut file = Vec::new();
        file.extend(0x0001_0000_u32.to_be_bytes());
        for value in [1_u16, 16, 0, 0] {
            file.extend(value.to_be_bytes());
        }
        file.extend(*b"name");
        file.extend(0_u32.to_be_bytes());
        file.extend(28_u32.to_be_bytes());
        file.extend(
            u32::try_from(table.len())
                .expect("a small table")
                .to_be_bytes(),
        );
        file.extend(table);
        file
    }

    #[test]
    fn a_family_goes_by_its_names_in_other_languages() {
        let data = font(&[
            (WINDOWS, ENGLISH, FAMILY, "Yu Gothic Medium"),
            (WINDOWS, ENGLISH, TYPOGRAPHIC, "Yu Gothic"),
            (MAC, 0, FAMILY, "Yu Gothic Medium"),
            (WINDOWS, JAPANESE, FAMILY, "游ゴシック Medium"),
            (WINDOWS, JAPANESE, TYPOGRAPHIC, "游ゴシック"),
            // The same name twice, an English name, and the family's own.
            (WINDOWS, CHINESE, TYPOGRAPHIC, "游ゴシック"),
            (WINDOWS, KOREAN, FAMILY, "Yu Gothic Medium"),
            (WINDOWS, KOREAN, TYPOGRAPHIC, "yu gothic "),
        ]);
        assert_eq!(
            local_family_names(&data, 0, "Yu Gothic"),
            ["游ゴシック", "游ゴシック Medium"]
        );
        // A family with only English names has none, nor does a file that
        // isn't a font.
        let english = font(&[(WINDOWS, ENGLISH, FAMILY, "Arial")]);
        assert!(local_family_names(&english, 0, "Arial").is_empty());
        assert!(local_family_names(b"not a font", 0, "Arial").is_empty());
    }

    #[test]
    fn a_name_in_another_language_that_repeats_the_familys_is_left_out() {
        // The repository's Noto Sans CJK, whose Japanese family name is its
        // English one.
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../packages/koharu/app/fonts/NotoSansCJK-VF.otf"
        );
        let data = fs_err::read(path).expect("the repository's CJK font");
        assert!(local_family_names(&data, 0, "Noto Sans CJK JP").is_empty());
    }

    #[test]
    fn a_family_installed_and_bundled_is_one() {
        let mut found = BTreeMap::new();
        merge(&mut found, "Noto Sans JP", &["ノート".to_owned()]);
        merge(
            &mut found,
            "noto sans jp",
            &["ノート".to_owned(), "諾托".to_owned()],
        );
        assert_eq!(
            found.into_values().collect::<Vec<_>>(),
            [(
                "Noto Sans JP".to_owned(),
                vec!["ノート".to_owned(), "諾托".to_owned()]
            )]
        );
    }

    #[test]
    fn a_span_s_own_gradient_spreads_over_its_run_and_the_text_s_over_the_text() {
        let gradient = |red: f32| Gradient {
            kind: koharu_scene::GradientKind::Linear,
            angle: 0.0,
            stops: [Rgba([red, 0.0, 0.0, 1.0]), Rgba([0.0, 0.0, 1.0, 1.0])]
                .into_iter()
                .zip([0.0, 1.0])
                .map(|(color, offset)| koharu_scene::GradientStop { offset, color })
                .collect(),
        };
        let filled = |red: f32| CharStyle {
            fill_gradient: Some(gradient(red)),
            ..CharStyle::default()
        };
        let flat = CharStyle {
            fill: Some(Rgba([0.0, 0.5, 0.0, 1.0])),
            ..CharStyle::default()
        };
        // Each span's own values over the text's gradient, and what its
        // gradient spreads over: the text's own, given again, is the span's;
        // the same one carries a run on, and another one, or one after a
        // span without it, starts one.
        let spans = [
            (CharStyle::default(), Some(Unit::Text)),
            (filled(1.0), Some(Unit::Own(1))),
            (filled(1.0), Some(Unit::Own(1))),
            (filled(0.5), Some(Unit::Own(3))),
            (flat, None),
            (filled(0.5), Some(Unit::Own(5))),
        ];
        let text = filled(1.0);
        let mut before: Option<SpanStyle> = None;
        for (index, (own, unit)) in spans.into_iter().enumerate() {
            let style = SpanStyle::new(&own.or(&text), &own, index, before.as_ref());
            let spread = style.gradient.as_ref().map(|(_, unit)| *unit);
            assert_eq!(spread, unit, "span {index}");
            before = Some(style);
        }
    }

    /// `text` laid out in `font` with its style's letter spacing `tracking`.
    fn spaced(
        font: &crate::fonts::Font,
        text: &koharu_scene::Text,
        tracking: f64,
    ) -> Result<crate::text::TextLayout> {
        let mut text = text.clone();
        text.style.chars.tracking = Some(tracking);
        Ok(crate::text::layout(&text, |_, _| Ok(vec![font.clone()]))?.expect("a layout"))
    }

    /// Where each glyph of `layout` stands along a horizontal line.
    fn xs(layout: &crate::text::TextLayout) -> Vec<f32> {
        layout
            .runs
            .iter()
            .flat_map(|run| run.glyphs.iter().map(|glyph| glyph.x))
            .collect()
    }

    /// Where each cluster of `layout`'s first line lies along it.
    fn alongs(layout: &crate::text::TextLayout) -> Vec<std::ops::Range<f32>> {
        layout.lines[0]
            .clusters
            .iter()
            .map(|cluster| cluster.along.clone())
            .collect()
    }

    fn near(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-3
    }

    #[test]
    fn letter_spacing_follows_each_cluster_once_not_each_mark() -> Result<()> {
        // A cluster's last glyph takes the gap, so its marks stay on it.
        let (mut letter, mut mark, mut next) = (10.0, 0.0, 12.0);
        track([(0, &mut letter), (0, &mut mark), (3, &mut next)], 2.5);
        assert_eq!((letter, mark, next), (10.0, 2.5, 14.5));
        let Some(font) = static_sans() else {
            eprintln!("skipped: no static sans-serif is installed");
            return Ok(());
        };
        // An x with an acute, two glyphs of one cluster, then a y, at the
        // default 24 pixels: the y moves one gap, 24 × 100 / 1000.
        let text = koharu_scene::Text {
            content: "x\u{301}y".into(),
            ..koharu_scene::Text::default()
        };
        let (plain, spaced) = (
            xs(&spaced(&font, &text, 0.0)?),
            xs(&spaced(&font, &text, 100.0)?),
        );
        assert_eq!(plain.len(), 3, "{plain:?}");
        assert_eq!(spaced[..2], plain[..2]);
        assert!(
            near(spaced[2] - plain[2], 2.4),
            "{spaced:?} against {plain:?}"
        );
        Ok(())
    }

    #[test]
    fn each_size_spaces_its_letters_by_its_own_em() -> Result<()> {
        let Some(font) = static_sans() else {
            eprintln!("skipped: no static sans-serif is installed");
            return Ok(());
        };
        let span = |words: &str, size: f64| koharu_scene::Span {
            insert: words.to_owned(),
            attributes: CharStyle {
                size: Some(size),
                ..CharStyle::default()
            },
        };
        let text = koharu_scene::Text {
            content: koharu_scene::RichText(vec![span("AB", 20.0), span("CD", 40.0)]),
            ..koharu_scene::Text::default()
        };
        let (plain, spaced) = (
            xs(&spaced(&font, &text, 0.0)?),
            xs(&spaced(&font, &text, 100.0)?),
        );
        // A and B each add 2 pixels, a tenth of 20, and C adds 4.
        let moved: Vec<f32> = spaced.iter().zip(&plain).map(|(a, b)| a - b).collect();
        assert!(
            moved
                .iter()
                .zip([0.0, 2.0, 4.0, 8.0])
                .all(|(a, b)| near(*a, b)),
            "{moved:?}"
        );
        Ok(())
    }

    #[test]
    fn a_line_opening_with_a_bracket_starts_where_it_did_spaced_out() -> Result<()> {
        let Some(font) = static_sans() else {
            eprintln!("skipped: no static sans-serif is installed");
            return Ok(());
        };
        // A proportional bracket, which leads its line with no space to
        // take off, however far its letter spacing reaches.
        let text = koharu_scene::Text {
            content: "(a)".into(),
            ..koharu_scene::Text::default()
        };
        let (plain, spaced) = (
            xs(&spaced(&font, &text, 0.0)?),
            xs(&spaced(&font, &text, 1000.0)?),
        );
        assert_eq!((plain[0], spaced[0]), (0.0, 0.0));
        Ok(())
    }

    #[test]
    fn a_word_too_wide_for_its_box_fits_it_spaced_tighter() -> Result<()> {
        let Some(font) = static_sans() else {
            eprintln!("skipped: no static sans-serif is installed");
            return Ok(());
        };
        let point = koharu_scene::Text {
            content: "Lettering".into(),
            ..koharu_scene::Text::default()
        };
        let along = alongs(&spaced(&font, &point, 0.0)?);
        let width = along[along.len() - 1].end - along[0].start;
        // A box 10 pixels narrower than the word, which a tracking of -100
        // narrows by 2.4 pixels after each letter but the last.
        let boxed = koharu_scene::Text {
            frame: koharu_scene::Frame::Box {
                bounds: koharu_scene::Bounds {
                    x: 0.0,
                    y: 0.0,
                    width: f64::from(width) - 10.0,
                    height: 100.0,
                },
                fit: koharu_scene::Fit::Shrink,
                anchor: Default::default(),
            },
            ..point
        };
        let plain = spaced(&font, &boxed, 0.0)?;
        assert!(plain.resolved.scale < 1.0, "{}", plain.resolved.scale);
        let tight = spaced(&font, &boxed, -100.0)?;
        assert_eq!(
            (
                tight.resolved.scale,
                tight.resolved.overflowed,
                tight.lines.len()
            ),
            (1.0, false, 1)
        );
        Ok(())
    }

    #[test]
    fn a_centred_line_hangs_its_last_letter_s_spacing() -> Result<()> {
        let Some(font) = static_sans() else {
            eprintln!("skipped: no static sans-serif is installed");
            return Ok(());
        };
        let text = koharu_scene::Text {
            content: "Centred".into(),
            style: koharu_scene::TextStyle {
                align: Some(koharu_scene::Align::Center),
                ..koharu_scene::TextStyle::default()
            },
            frame: koharu_scene::Frame::Box {
                bounds: koharu_scene::Bounds {
                    x: 0.0,
                    y: 0.0,
                    width: 400.0,
                    height: 100.0,
                },
                fit: koharu_scene::Fit::None,
                anchor: Default::default(),
            },
            ..koharu_scene::Text::default()
        };
        // A gap of 12 pixels after each letter: the letters stand as far
        // from either side, the last one's gap hanging past them.
        let along = alongs(&spaced(&font, &text, 500.0)?);
        let (start, end) = (along[0].start, along[along.len() - 1].end - 12.0);
        assert!(near(start, 400.0 - end), "{start} against {end}");
        Ok(())
    }

    #[test]
    fn vertical_letter_spacing_runs_down_the_column() -> Result<()> {
        let Some(font) = static_sans() else {
            eprintln!("skipped: no static sans-serif is installed");
            return Ok(());
        };
        // Sideways letters around an upright `!`.
        let text = koharu_scene::Text {
            content: "a!b".into(),
            direction: koharu_scene::Direction::Vertical,
            ..koharu_scene::Text::default()
        };
        let spaced_out = spaced(&font, &text, 250.0)?;
        let sideways: Vec<bool> = spaced_out.runs.iter().map(|run| run.sideways).collect();
        assert_eq!(sideways, [true, false, true]);
        // A gap of 6 pixels after each, the upright mark's on its vertical
        // advance.
        let (plain, spaced) = (alongs(&spaced(&font, &text, 0.0)?), alongs(&spaced_out));
        for (at, (plain, spaced)) in plain.iter().zip(&spaced).enumerate() {
            assert!(
                near(spaced.start - plain.start, 6.0 * at as f32),
                "{spaced:?} against {plain:?}"
            );
            assert!(near(
                spaced.end - spaced.start,
                plain.end - plain.start + 6.0
            ));
        }
        Ok(())
    }

    #[test]
    fn an_upright_group_takes_no_letter_spacing() -> Result<()> {
        let Some(font) = static_sans() else {
            eprintln!("skipped: no static sans-serif is installed");
            return Ok(());
        };
        // 12 set upright in one cell between sideways words.
        let text = koharu_scene::Text {
            content: "x 12 y".into(),
            direction: koharu_scene::Direction::Vertical,
            ..koharu_scene::Text::default()
        };
        let (plain, spaced) = (spaced(&font, &text, 0.0)?, spaced(&font, &text, 250.0)?);
        // Its digits stand across the column as they did, and its cell is
        // one em long, the space after it following straight on.
        let digits = |layout: &crate::text::TextLayout| -> Vec<f32> {
            layout
                .runs
                .iter()
                .filter(|run| !run.sideways)
                .flat_map(|run| run.glyphs.iter().map(|glyph| glyph.x))
                .collect()
        };
        assert_eq!(digits(&plain).len(), 2);
        assert_eq!(digits(&spaced), digits(&plain));
        let along = alongs(&spaced);
        assert!(near(along[4].start - along[2].start, 24.0), "{along:?}");
        assert_eq!(along[2], along[3]);
        Ok(())
    }

    #[test]
    fn a_run_keeps_its_span_s_letter_spacing() -> Result<()> {
        let Some(font) = static_sans() else {
            eprintln!("skipped: no static sans-serif is installed");
            return Ok(());
        };
        // A fraction stays as set, and a span without one, or with one that
        // isn't a number, has none.
        let spans = [
            ("Spaced", Some(12.5)),
            (" plain", None),
            (" odd", Some(f64::INFINITY)),
        ];
        let text = koharu_scene::Text {
            content: koharu_scene::RichText(
                spans
                    .into_iter()
                    .map(|(words, tracking)| koharu_scene::Span {
                        insert: words.to_owned(),
                        attributes: CharStyle {
                            tracking,
                            ..CharStyle::default()
                        },
                    })
                    .collect(),
            ),
            ..koharu_scene::Text::default()
        };
        let layout = crate::text::layout(&text, |_, _| Ok(vec![font.clone()]))?.expect("a layout");
        let tracking: Vec<f32> = layout
            .resolved
            .runs
            .iter()
            .map(|run| run.tracking)
            .collect();
        assert_eq!(tracking, [12.5, 0.0, 0.0]);
        Ok(())
    }
}
