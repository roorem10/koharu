//! Which of a family's faces sets a text asked for a weight and a slant.
//! fontique matches faces as CSS does, the slant before the weight, so a
//! family of a Regular and a Bold Italic face, as comic fonts such as CC Wild
//! Words often are, sets italic text of weight 400 in its Bold Italic: bold
//! the text never asked for, which nothing can take off again. Booharu sets
//! it in the Regular face, slanted, which fontique fakes, so bold and italic
//! stay apart: italic alone slants the Regular face, bold alone emboldens it,
//! and both take the real Bold Italic. Faces are weighed as Booharu 0.0.4
//! weighed them, and CSS's choice stands unless it would set light text in a
//! heavy face.

use fontique::{Attributes, Collection, FontInfo, FontStyle, QueryFont, SourceCache};

/// What choosing among a family's faces reads of each.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Traits {
    pub style: FontStyle,
    /// Its CSS weight.
    pub weight: f32,
    /// Its width, a percentage of normal. CSS matching settles the width
    /// before anything else, and `better` keeps the width it settled.
    pub width: f32,
    /// It has a `wght` axis, which reaches any weight asked for.
    pub weight_axis: bool,
    /// It has a `slnt` or an `ital` axis, which slants it for real.
    pub slant_axis: bool,
}

/// From this weight up, a face or a text is heavy: Booharu 0.0.4's line.
const HEAVY: f32 = 600.0;

/// What one face can't be made into: costs this or more.
const WRONG: u32 = 10_000;

/// A face's traits as fontique has them.
pub(crate) fn traits(info: &FontInfo) -> Traits {
    Traits {
        style: info.style(),
        weight: info.weight().value(),
        width: info.width().percentage(),
        weight_axis: info.has_weight_axis(),
        slant_axis: info.has_slant_axis() || info.has_italic_axis(),
    }
}

/// What setting a text `asked` for in `face` costs, the lower the better, by
/// Booharu 0.0.4's numbers. A slant or a weight fontique can fake on a face
/// costs 2,000, and one it can't undo, a slanted face for upright text or a
/// heavy face for light text, 10,000; then the weights' distance. An axis
/// reaches what's asked of it, so it costs nothing. Italic and oblique are
/// one slant, as they are in Koharu's font list (`FontFace::italic`), so a
/// family's real oblique isn't passed over for a faked one.
pub(crate) fn cost(face: Traits, asked: Attributes) -> u32 {
    let slanted = |style: FontStyle| style != FontStyle::Normal;
    let style = if slanted(face.style) == slanted(asked.style)
        || (slanted(asked.style) && face.slant_axis)
    {
        0
    } else if !slanted(face.style) {
        2_000
    } else {
        WRONG
    };
    let weight = asked.weight.value();
    let side = if face.weight_axis || (face.weight >= HEAVY) == (weight >= HEAVY) {
        0
    } else if weight >= HEAVY {
        2_000
    } else {
        WRONG
    };
    let distance = match face.weight_axis {
        true => 0,
        false => (face.weight - weight).abs().round() as u32,
    };
    style + side + distance
}

/// Which of `faces` sets a text `asked` for, given `picked`, the face CSS
/// font matching picked: `picked`, unless it costs 10,000 or more, no face of
/// its width and slant costs less, and another face of its width does; then
/// the cheapest, the first of equals.
///
/// Where this differs from CSS: CSS settles the slant before the weight, so
/// light text asked slanted takes the heavy slanted face of a family with no
/// light slanted one, where this takes a light upright face, which fontique
/// slants, as it can fake a slant but can't take weight off. That's all: a
/// width CSS settled stands, and so does its weight within a slant, so text
/// of weight 550 in a Regular and Bold family takes the Bold as CSS has it,
/// and upright text never takes a slanted face.
pub(crate) fn better<T>(
    faces: &[T],
    picked: usize,
    traits: impl Fn(&T) -> Traits,
    asked: Attributes,
) -> usize {
    let Some(pick) = faces.get(picked).map(&traits) else {
        return picked;
    };
    if cost(pick, asked) < WRONG {
        return picked;
    }
    let slanted = |style: FontStyle| style != FontStyle::Normal;
    let others = faces
        .iter()
        .map(&traits)
        .enumerate()
        .filter(|(_, face)| face.width == pick.width);
    if others
        .clone()
        .any(|(_, face)| slanted(face.style) == slanted(pick.style) && cost(face, asked) < WRONG)
    {
        return picked;
    }
    others
        .map(|(index, face)| (cost(face, asked), index))
        .filter(|(cost, _)| *cost < WRONG)
        .min()
        .map_or(picked, |(_, index)| index)
}

/// Puts the face `better` chooses for each of the named `families` ahead of
/// the family's faces fontique matched for `asked` into `found`, from index
/// `from` on, where it isn't fontique's choice: an entry for each face of
/// the family of its width, slant and weight, as fontique gives one for
/// each of a choice's files, each with what fontique fakes of `asked` on
/// it. fontique's entries stay behind, where a character the chosen face
/// lacks still finds them, and `Library::chain` drops a face it already has.
pub(crate) fn repick(
    collection: &mut Collection,
    sources: &mut SourceCache,
    families: &[String],
    asked: Attributes,
    found: &mut Vec<QueryFont>,
    from: usize,
) {
    for family in families {
        // The lookup a query makes of a named family.
        let Some(id) = collection.family_id(family) else {
            continue;
        };
        let Some(at) = found
            .get(from..)
            .and_then(|found| found.iter().position(|font| font.family.0 == id))
        else {
            continue;
        };
        let at = from + at;
        let Some(info) = collection.family(id) else {
            continue;
        };
        let faces = info.fonts();
        let picked = found[at].family.1;
        let chosen = better(faces, picked, traits, asked);
        let Some(choice) = faces.get(chosen).filter(|_| chosen != picked) else {
            continue;
        };
        let (width, style, weight) = (choice.width(), choice.style(), choice.weight());
        let fonts: Vec<QueryFont> = faces
            .iter()
            .enumerate()
            .filter(|(_, face)| {
                face.width() == width && face.style() == style && face.weight() == weight
            })
            .filter_map(|(index, face)| {
                Some(QueryFont {
                    family: (id, index),
                    blob: face.load(Some(sources))?,
                    index: face.index(),
                    synthesis: face.synthesis(asked.width, asked.style, asked.weight),
                    charmap_index: face.charmap_index(),
                })
            })
            .collect();
        found.splice(at..at, fonts);
    }
}

#[cfg(test)]
mod tests {
    use fontique::{FontWeight, QueryFamily, QueryStatus, Synthesis};

    use super::*;
    use crate::booharu::test_family;

    fn face(style: FontStyle, weight: f32) -> Traits {
        Traits {
            style,
            weight,
            width: 100.0,
            weight_axis: false,
            slant_axis: false,
        }
    }

    fn asked(weight: f32, style: FontStyle) -> Attributes {
        Attributes {
            weight: FontWeight::new(weight),
            style,
            ..Attributes::default()
        }
    }

    const NORMAL: FontStyle = FontStyle::Normal;
    const ITALIC: FontStyle = FontStyle::Italic;

    #[test]
    fn a_light_italic_slants_the_regular_face_rather_than_take_the_bold_italic() {
        // CC Wild Words as Windows installs it: Regular and Bold Italic.
        let faces = [face(NORMAL, 400.0), face(ITALIC, 700.0)];
        let pick = |weight, style, css| better(&faces, css, |face| *face, asked(weight, style));
        // CSS picks the Bold Italic for any italic, the Regular otherwise.
        assert_eq!(pick(400.0, ITALIC, 1), 0, "I alone is a slanted Regular");
        assert_eq!(pick(700.0, ITALIC, 1), 1, "B and I take the Bold Italic");
        assert_eq!(
            pick(700.0, NORMAL, 0),
            0,
            "B alone is an emboldened Regular"
        );
        assert_eq!(pick(400.0, NORMAL, 0), 0);
        assert_eq!(cost(faces[0], asked(400.0, ITALIC)), 2_000);
        assert_eq!(cost(faces[1], asked(400.0, ITALIC)), 10_300);
        assert_eq!(cost(faces[0], asked(700.0, NORMAL)), 2_300);
    }

    #[test]
    fn css_s_pick_stands_unless_light_text_would_take_a_heavy_face() {
        // A Bold and an Italic: neither can be undone, so CSS's pick stands.
        let faces = [face(NORMAL, 700.0), face(ITALIC, 400.0)];
        let pick = |weight, style, css| better(&faces, css, |face| *face, asked(weight, style));
        assert_eq!(pick(400.0, NORMAL, 0), 0, "upright text never slants");
        assert_eq!(pick(700.0, ITALIC, 1), 1, "the Italic, emboldened");
        assert_eq!(pick(550.0, NORMAL, 0), 0);
        // CSS sets weight 550 in a Bold before a Regular, and so does this.
        let faces = [face(NORMAL, 400.0), face(NORMAL, 700.0)];
        assert_eq!(better(&faces, 1, |face| *face, asked(550.0, NORMAL)), 1);
        // A light face of another width is no way out of CSS's width.
        let mut condensed = face(NORMAL, 400.0);
        condensed.width = 75.0;
        let faces = [condensed, face(ITALIC, 700.0)];
        assert_eq!(better(&faces, 1, |face| *face, asked(400.0, ITALIC)), 1);
    }

    #[test]
    fn an_axis_reaches_the_weight_or_slant_asked_of_it() {
        let variable = Traits {
            weight_axis: true,
            ..face(NORMAL, 400.0)
        };
        assert_eq!(cost(variable, asked(900.0, NORMAL)), 0);
        assert_eq!(cost(variable, asked(100.0, NORMAL)), 0);
        assert_eq!(cost(variable, asked(300.0, ITALIC)), 2_000);
        let sloped = Traits {
            slant_axis: true,
            ..variable
        };
        assert_eq!(cost(sloped, asked(300.0, ITALIC)), 0);
        assert_eq!(cost(sloped, asked(300.0, NORMAL)), 0);
        // A real oblique is a slant as an italic is.
        let oblique = face(FontStyle::Oblique(None), 400.0);
        assert_eq!(cost(oblique, asked(400.0, ITALIC)), 0);
        assert_eq!(cost(oblique, asked(400.0, NORMAL)), 10_000);
        // A variable Roman and Italic set every weight as CSS picks.
        let italic = Traits {
            style: ITALIC,
            ..variable
        };
        let faces = [variable, italic];
        for weight in [100.0, 400.0, 900.0] {
            assert_eq!(better(&faces, 1, |face| *face, asked(weight, ITALIC)), 1);
            assert_eq!(better(&faces, 0, |face| *face, asked(weight, NORMAL)), 0);
        }
        // A variable Roman slants for a light italic before a Bold Italic.
        let faces = [variable, face(ITALIC, 700.0)];
        assert_eq!(better(&faces, 1, |face| *face, asked(400.0, ITALIC)), 0);
        assert_eq!(better(&faces, 1, |face| *face, asked(700.0, ITALIC)), 1);
    }

    const FAMILY: &str = "Booharu Faces Test";

    /// Each face of `found` by its index in its family, with what's faked
    /// on it.
    fn faces(found: &[QueryFont]) -> Vec<(usize, Synthesis)> {
        found
            .iter()
            .map(|font| (font.family.1, font.synthesis))
            .collect()
    }

    /// The faces a query of `FAMILY` matches for `asked`, as the library's
    /// does, and them after `repick`.
    fn matched(
        collection: &mut Collection,
        sources: &mut SourceCache,
        asked: Attributes,
    ) -> (Vec<QueryFont>, Vec<QueryFont>) {
        let mut found = Vec::new();
        let mut query = collection.query(sources);
        query.set_families([QueryFamily::Named(FAMILY)]);
        query.set_attributes(asked);
        query.matches_with(|font| {
            found.push(font.clone());
            QueryStatus::Continue
        });
        drop(query);
        let before = found.clone();
        repick(
            collection,
            sources,
            &[FAMILY.to_owned()],
            asked,
            &mut found,
            0,
        );
        (before, found)
    }

    #[test]
    fn a_regular_and_bold_italic_family_fakes_only_what_is_asked() {
        // Registered Regular first, then Bold Italic.
        let Some(mut collection) = test_family(FAMILY, &[(NORMAL, 400.0), (ITALIC, 700.0)]) else {
            return;
        };
        let mut sources = SourceCache::default();
        let (before, found) = matched(&mut collection, &mut sources, asked(400.0, ITALIC));
        assert_eq!(before[0].family.1, 1, "fontique picks the Bold Italic");
        assert_eq!(found[0].family.1, 0, "Booharu the Regular");
        assert_eq!(found[0].synthesis.skew(), Some(14.0));
        assert!(!found[0].synthesis.embolden());
        // fontique's own picks stay behind it.
        assert_eq!(faces(&found[1..]), faces(&before));
        let (_, found) = matched(&mut collection, &mut sources, asked(700.0, NORMAL));
        assert_eq!(found[0].family.1, 0);
        assert!(found[0].synthesis.embolden());
        assert_eq!(found[0].synthesis.skew(), None);
        let (_, found) = matched(&mut collection, &mut sources, asked(700.0, ITALIC));
        assert_eq!(found[0].family.1, 1);
        assert!(!found[0].synthesis.any());
    }

    #[test]
    fn a_family_with_every_face_is_matched_as_css_matches_it() {
        let every = [
            (NORMAL, 400.0),
            (NORMAL, 700.0),
            (ITALIC, 400.0),
            (ITALIC, 700.0),
        ];
        let Some(mut collection) = test_family(FAMILY, &every) else {
            return;
        };
        let mut sources = SourceCache::default();
        for style in [NORMAL, ITALIC] {
            for weight in (100..=900).step_by(100) {
                let asked = asked(weight as f32, style);
                let (before, found) = matched(&mut collection, &mut sources, asked);
                assert_eq!(faces(&found), faces(&before), "{weight} {style:?}");
            }
        }
    }
}
