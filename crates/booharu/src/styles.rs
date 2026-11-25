//! Apply: the default text look given again to translations already made,
//! on some blocks, some pages, or the whole project, as one undo step, with
//! how many blocks it changed, kept, and found untranslated, which a count
//! answers beforehand without changing anything.
//!
//! A translation gets the look a new one would with nothing of its own
//! ([`booharu_core::styles::restyle`]): the default font, the first of
//! Koharu's Preferred fonts, in place of its family, or while Pick fonts
//! automatically is on its block's category's font in place of its family,
//! weight, and italic, and with `detect_colors` off the default colours and
//! outline in place of its own; with it on, Apply never recolours. Either
//! way it takes off a gradient fill and letter spacing, the words' with the
//! box's, so the look shows flat and evenly spaced, while a word's flat
//! colour stays unless Apply writes a fill. The
//! agent may name the category every block of the scope is lettered as
//! instead, whatever its own and whether the switch is on. A block
//! changed by hand ([`booharu_core::edited`]) is kept unless the caller
//! includes those, and a locked block, or one inside a locked layer, is
//! always kept, so a lock never fails the edit. A box styled before it's
//! translated, whose translation is only the empty placeholder the style
//! left ([`booharu_core::status::placeholder`]), counts as untranslated and
//! is left alone, the values set on it the user's. An included block also
//! loses its marks for the keys Apply writes, so the look shows on every
//! character, but for a category's font, which never touches the marks, so
//! word styles survive it.
//!
//! Each block Apply changes gets its hand-edit record in the same edit: the
//! keys Apply wrote take their new values and the others keep theirs, so a
//! hand edit Apply left stays one, the record's origin says who applied the
//! look, and the words a run marked for emphasis lose in the record the
//! keys Apply took off them, a mark left setting nothing going.
//!
//! Reset to lettering default ([`reset_lettering`]) gives chosen boxes and
//! text layers their lettering default back, worked out from the scene
//! ([`booharu_core::resolver`]): every look key, hand-set or not, its font,
//! colours, gradient and letter spacing among them, and every word mark,
//! and it takes the skew off, as one undo step, leaving the words, size,
//! frame, position, rotation and alignment. It is how a box whose role a
//! picked category changed gets the outline of its new role at once,
//! rather than at its next translation. It reads a box styled before it's
//! translated as it is, so it clears that style too, and the record it
//! writes keeps no text, so the box stays a placeholder, still translatable.

use std::collections::BTreeSet;

use anyhow::{Result, ensure};
use booharu_core::{
    attrs::{self, Auto, AutoMark, AutoStyle, Origin},
    edited,
    lettering::{self, LetteringCategory},
    resolver,
    settings::LetteringSettings,
    status, styles,
};
use koharu_scene::{
    CharStyle, Content, Edit, Layer, LayerId, Op, PageId, Project, RichText, Role, Span,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use crate::effects;

/// The undo step's label, which the History panel translates.
pub const LABEL: &str = "booharu_apply_text_style";

/// The blocks Apply looks at: `{"kind": "blocks", "blocks": [...]}`,
/// `{"kind": "pages", "pages": [...]}`, or `{"kind": "project"}`.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, specta::Type, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum StyleScope {
    /// These blocks.
    Blocks { blocks: Vec<LayerId> },
    /// Every block on these pages.
    Pages { pages: Vec<PageId> },
    /// Every block on every page.
    Project,
}

/// How many blocks of a scope Apply changed, or would change, kept, and
/// found untranslated. A block whose translation has the look already
/// counts in none.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize, specta::Type)]
pub struct StyleCounts {
    /// Translated blocks that take the look.
    pub changed: u32,
    /// Translated blocks left as they are: changed by hand, when those
    /// aren't included, or locked.
    pub kept_hand_edited: u32,
    /// Blocks without a translation, which Apply leaves alone.
    pub untranslated: u32,
}

/// What [`apply`] would answer for the same values, changing nothing.
pub fn count(
    project: &Project,
    scope: &StyleScope,
    include_hand_edited: bool,
    category: Option<LetteringCategory>,
    settings: &LetteringSettings,
    font: Option<&str>,
) -> Result<StyleCounts> {
    Ok(planned(
        project,
        scope,
        include_hand_edited,
        category,
        settings,
        font,
    )?
    .counts)
}

/// The edit giving the translations of `scope` in `project` the default
/// look of `settings` and `font`, the first Preferred font, again, with no
/// gradient fill or letter spacing, as one undo step labelled [`LABEL`]
/// over any number of pages, with its numbers; none when it changes
/// nothing. A block changed by hand takes it only with
/// `include_hand_edited`. A block whose translation is only a placeholder
/// ([`status::unwritten`]) counts as untranslated and is never restyled,
/// even then. With `category`, every block is lettered in that
/// category's font, whatever its own and whether Pick fonts automatically is
/// on. `origin` goes in each changed block's record: [`Origin::Agent`] makes
/// the look the agent's choice, which a later automatic step keeps as it
/// keeps a hand edit.
pub fn apply(
    project: &Project,
    scope: &StyleScope,
    include_hand_edited: bool,
    category: Option<LetteringCategory>,
    settings: &LetteringSettings,
    font: Option<&str>,
    origin: Origin,
) -> Result<(Option<Edit>, StyleCounts)> {
    let plan = planned(
        project,
        scope,
        include_hand_edited,
        category,
        settings,
        font,
    )?;
    let mut ops = Vec::new();
    for restyled in plan.restyled {
        let id = restyled.layer.id;
        let written: BTreeSet<String> = restyled.patch.keys().cloned().collect();
        ops.push(Op::UpdateLayer {
            layer: id.into(),
            patch: json!({"translation": {"style": {"chars": restyled.patch}}}),
        });
        if let Some(spans) = restyled.spans {
            ops.push(Op::SetRichText {
                layer: id.into(),
                spans,
            });
        }
        let mut record = Auto {
            origin,
            ..Auto::of(restyled.layer)
        }
        .restyled(&restyled.after, &written);
        // The words lost their marks for the keys Apply wrote, so the run's
        // record of them does too.
        record.marks = without_keys(record.marks, &restyled.cleared)?;
        ops.push(attrs::set(id, &record)?);
    }
    let edit = (!ops.is_empty()).then(|| Edit {
        label: LABEL.to_owned(),
        ops,
    });
    Ok((edit, plan.counts))
}

/// Reset to lettering default's undo step label, which the History panel
/// translates.
pub const RESET_LABEL: &str = "booharu_reset_lettering";

/// How many layers Reset to lettering default changed, left alone as
/// locked, and found untranslated. A layer that has its default already
/// counts in none.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize, specta::Type)]
pub struct ResetCounts {
    /// Layers given their lettering default.
    pub changed: u32,
    /// Layers left as they are because they, or a layer holding them, are
    /// locked.
    pub locked: u32,
    /// Blocks without a translation, which have no lettering to reset.
    pub untranslated: u32,
}

/// The edit giving each of `layers` in `project` its lettering default
/// ([`resolver::lettering_default`]) under `settings` and `font`, the first
/// Preferred font, as one undo step labelled [`RESET_LABEL`], with its
/// numbers; none when nothing changes.
///
/// For each unlocked layer that has a default, only the ops it needs: its
/// text's [`resolver::LOOK`] keys and border method, every one written, null
/// where the default sets none, so a gradient and letter spacing go; its
/// words as one unmarked span when a span has marks; for a block its
/// hand-edit record, automatic, with the new look as its style, no word
/// marks, and its other fields as they were, so a translation typed by hand
/// stays one; and the skew it shows taken off its transform, about its
/// record's pivot when the record still holds, with the record removed
/// ([`crate::effects`]), which leaves its position, rotation and size as
/// they are. Never the words, the language, the size, the frame, the
/// position or rotation, the direction, the alignment, the line height, the
/// clean-up, the source, the role, or any other attribute. A locked block
/// or text layer, or one inside a locked layer, is counted and left, and an
/// untranslated block counted; any other layer is left uncounted. A block
/// whose translation is only the placeholder a style set before translating
/// left ([`status::placeholder`]) is reset as it is: its style goes, and its
/// record keeps no text, so it stays untranslated. A layer
/// named twice counts once, and a layer not in `project` fails.
pub fn reset_lettering(
    project: &Project,
    layers: &[LayerId],
    settings: &LetteringSettings,
    font: Option<&str>,
) -> Result<(Option<Edit>, ResetCounts)> {
    let mut counts = ResetCounts::default();
    let mut ops = Vec::new();
    let mut seen = BTreeSet::new();
    for &id in layers {
        if !seen.insert(id) {
            continue;
        }
        let layer = project.layer(id)?;
        let (text, block) = match &layer.content {
            // A placeholder is read as it is, so its style can go too.
            Content::Block(block) => (block.translation.as_ref(), true),
            Content::Text(text) => (Some(&**text), false),
            _ => continue,
        };
        if locked(project, layer)? {
            counts.locked += 1;
            continue;
        }
        let Some(text) = text else {
            counts.untranslated += 1;
            continue;
        };
        let Some(look) = resolver::lettering_default(project, id, settings, font)? else {
            continue;
        };
        let at = ops.len();
        let style = &text.style;
        // A method stored as Koharu's own counts too, so Reset writes it as
        // none, which Koharu can read.
        if AutoStyle::of(&style.chars) != AutoStyle::of(&look.chars)
            || style.stroke_method != look.stroke_method
        {
            let patch = resolver::reset_patch(&look);
            ops.push(Op::UpdateLayer {
                layer: id.into(),
                patch: if block {
                    json!({"translation": {"style": patch}})
                } else {
                    json!({"style": patch})
                },
            });
        }
        if text
            .content
            .0
            .iter()
            .any(|span| span.attributes != CharStyle::default())
        {
            ops.push(Op::SetRichText {
                layer: id.into(),
                spans: vec![Span {
                    insert: text.content.text(),
                    attributes: CharStyle::default(),
                }],
            });
        }
        if block {
            let record = Auto {
                origin: Origin::Automatic,
                style: Some(AutoStyle::of(&look.chars)),
                marks: Vec::new(),
                ..Auto::of(layer)
            };
            if attrs::get::<Auto>(layer).as_ref() != Some(&record) {
                ops.push(attrs::set(id, &record)?);
            }
        }
        ops.extend(effects::unskewed(layer, text, block));
        counts.changed += u32::from(ops.len() > at);
    }
    let edit = (!ops.is_empty()).then(|| Edit {
        label: RESET_LABEL.to_owned(),
        ops,
    });
    Ok((edit, counts))
}

/// What Apply does to a scope.
struct Plan<'p> {
    counts: StyleCounts,
    restyled: Vec<Restyled<'p>>,
}

/// A block Apply changes.
struct Restyled<'p> {
    layer: &'p Layer,
    /// The patch on its translation's `style.chars`.
    patch: Map<String, Value>,
    /// Its translation's character values after the patch.
    after: CharStyle,
    /// Its spans without their marks for the patch's keys, when one had one.
    spans: Option<Vec<Span>>,
    /// The keys whose marks it takes off.
    cleared: Vec<String>,
}

fn planned<'p>(
    project: &'p Project,
    scope: &StyleScope,
    include_hand_edited: bool,
    named: Option<LetteringCategory>,
    settings: &LetteringSettings,
    font: Option<&str>,
) -> Result<Plan<'p>> {
    let mut plan = Plan {
        counts: StyleCounts::default(),
        restyled: Vec::new(),
    };
    for layer in blocks(project, scope)? {
        let Content::Block(block) = &layer.content else {
            continue;
        };
        // A placeholder is no translation, so a style set on the box before
        // it's translated stays the user's.
        let translation = block
            .translation
            .as_ref()
            .filter(|_| !status::unwritten(layer, block));
        let Some(translation) = translation else {
            plan.counts.untranslated += 1;
            continue;
        };
        if locked(project, layer)? || (!include_hand_edited && edited(layer).hand_edited()) {
            plan.counts.kept_hand_edited += 1;
            continue;
        }
        let current = &translation.style.chars;
        let category = match named {
            Some(named) => Some(named),
            None => lettering::auto_category(project.parent(layer.id)?.0, layer.id, settings),
        };
        let sfx = block.role == Role::Sfx;
        let mut patch = styles::restyle(settings, font, sfx, category, current);
        patch.extend(FLAT.map(|key| (key.to_owned(), Value::Null)));
        let after = styles::patched(current, &patch)?;
        let marked = |key: &String| category.is_none() || !FONT.contains(&key.as_str());
        let cleared: Vec<String> = patch.keys().filter(|key| marked(key)).cloned().collect();
        let spans = unmarked(&translation.content, &cleared)?;
        if after == *current && spans.is_none() {
            continue;
        }
        plan.counts.changed += 1;
        plan.restyled.push(Restyled {
            layer,
            patch,
            after,
            spans,
            cleared,
        });
    }
    Ok(plan)
}

/// The blocks `scope` names in `project`, each once, a page's in reading
/// order. A layer named that isn't a block is refused.
fn blocks<'p>(project: &'p Project, scope: &StyleScope) -> Result<Vec<&'p Layer>> {
    let mut found = Vec::new();
    match scope {
        StyleScope::Blocks { blocks } => {
            for id in blocks {
                let layer = project.layer(*id)?;
                ensure!(
                    matches!(layer.content, Content::Block(_)),
                    "layer {id} isn't a block"
                );
                found.push(layer);
            }
        }
        StyleScope::Pages { pages } => {
            for page in pages {
                found.extend(project.layers_of(*page)?);
            }
        }
        StyleScope::Project => {
            for page in project.pages() {
                found.extend(project.layers_of(page.id)?);
            }
        }
    }
    let mut seen = BTreeSet::new();
    found.retain(|layer| matches!(layer.content, Content::Block(_)) && seen.insert(layer.id));
    Ok(found)
}

/// Whether `layer`, or a layer holding it, is locked, which an edit of it
/// fails on.
pub(crate) fn locked(project: &Project, layer: &Layer) -> Result<bool> {
    let mut at = Some(layer);
    while let Some(layer) = at {
        if layer.locked {
            return Ok(true);
        }
        at = project.parent(layer.id)?.1;
    }
    Ok(false)
}

/// The keys a category's font writes, whose marks it leaves.
const FONT: [&str; 3] = ["family", "weight", "italic"];

/// The keys Apply always clears, so its look shows flat and evenly spaced:
/// a gradient fill, which would win over the fill beside it, and letter
/// spacing. Their marks go with them, as the marks of every key Apply
/// writes do, while a word's flat colour stays unless Apply writes a fill.
const FLAT: [&str; 2] = ["fill_gradient", "tracking"];

/// `marks`, a hand-edit record's, without their values for `keys`, which
/// Apply took off the words, and without the marks left setting nothing.
fn without_keys(marks: Vec<AutoMark>, keys: &[String]) -> Result<Vec<AutoMark>> {
    let cleared: Map<String, Value> = keys.iter().map(|key| (key.clone(), Value::Null)).collect();
    let mut kept = Vec::with_capacity(marks.len());
    for mark in marks {
        let style = styles::patched(&mark.style, &cleared)?;
        if style != CharStyle::default() {
            kept.push(AutoMark { style, ..mark });
        }
    }
    Ok(kept)
}

/// `content`'s spans without their marks for `keys`, so the values Apply
/// writes for them show on every character; none when no span has one.
fn unmarked<'k>(
    content: &RichText,
    keys: impl IntoIterator<Item = &'k String>,
) -> Result<Option<Vec<Span>>> {
    let cleared: Map<String, Value> = keys
        .into_iter()
        .map(|key| (key.clone(), Value::Null))
        .collect();
    let mut found = false;
    let mut spans = Vec::with_capacity(content.0.len());
    for span in &content.0 {
        let attributes = styles::patched(&span.attributes, &cleared)?;
        found |= attributes != span.attributes;
        spans.push(Span {
            insert: span.insert.clone(),
            attributes,
        });
    }
    Ok(found.then_some(spans))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use anyhow::Context as _;
    use booharu_core::{
        Edited,
        attrs::Skew,
        emphasis::{EmphasisSettings, EmphasisStyle, Marking},
        settings::DefaultTextStyle,
    };
    use koharu_scene::{
        Created, Gradient, GradientKind, GradientStop, LanguageIdentifier, Rgba, Session, Text,
        Transform,
    };
    use tempfile::TempDir;

    use super::*;
    use crate::testing::{FILL, type_bar_style};

    const RED: Rgba = Rgba([1.0, 0.0, 0.0, 1.0]);
    const BLUE: Rgba = Rgba([0.0, 0.0, 1.0, 1.0]);
    const WHITE: Rgba = Rgba([1.0; 4]);
    const BLACK: Rgba = Rgba([0.0, 0.0, 0.0, 1.0]);
    const YELLOW: Rgba = Rgba([1.0, 1.0, 0.0, 1.0]);

    /// The old default font, which the project was translated with.
    const OLD: &str = "Comic Neue";
    /// The new default font, which Apply gives.
    const NEW: &str = "CCWildWords";

    fn look(detect_colors: bool, fill: Option<Rgba>) -> LetteringSettings {
        LetteringSettings {
            default: DefaultTextStyle {
                fill,
                stroke: None,
                stroke_width: 1.5,
            },
            detect_colors,
            ..LetteringSettings::default()
        }
    }

    /// A project of two pages: on the first the blocks `a`, `b`, `c`, and
    /// `d` inside the group `g`; on the second the sound effect `e`. Each was
    /// found and read by Koharu's stages, lettered in blue outlined in white
    /// at size 32, and none is translated yet.
    struct Fixture {
        _dir: TempDir,
        session: Session,
        layers: BTreeMap<String, LayerId>,
        pages: BTreeMap<String, PageId>,
    }

    impl Fixture {
        fn new() -> Result<Self> {
            let dir = tempfile::tempdir()?;
            let mut session = Session::create(dir.path().join("test.khr"))?;
            let block = |x: f64, role: &str| {
                json!({"kind": "block", "role": role, "source": {
                    "frame": {"box": {"bounds": {"x": x, "y": 20.0, "width": 40.0, "height": 120.0}}},
                    "content": "どうして",
                    "style": {"chars": {"size": 32.0, "fill": BLUE, "stroke": WHITE, "stroke_width": 3.0}},
                }, "attributes": {"c2pa": {
                    "detection": {"action": "c2pa.created"},
                    "ocr": {"action": "c2pa.created"},
                }}})
            };
            let ops = [
                json!({"op": "add_page", "id": "$p1", "index": 0, "size": {"width": 400, "height": 400}}),
                json!({"op": "add_page", "id": "$p2", "index": 1, "size": {"width": 400, "height": 400}}),
                json!({"op": "add_layer", "id": "$a", "page": "$p1", "index": 0, "layer": block(10.0, "text")}),
                json!({"op": "add_layer", "id": "$b", "page": "$p1", "index": 1, "layer": block(60.0, "text")}),
                json!({"op": "add_layer", "id": "$c", "page": "$p1", "index": 2, "layer": block(110.0, "text")}),
                json!({"op": "add_layer", "id": "$g", "page": "$p1", "index": 3, "layer": {"kind": "group"}}),
                json!({"op": "add_layer", "id": "$d", "page": "$p1", "parent": "$g", "index": 0, "layer": block(160.0, "text")}),
                json!({"op": "add_layer", "id": "$e", "page": "$p2", "index": 0, "layer": block(10.0, "sfx")}),
            ];
            let change = session.apply(Edit {
                label: "test".to_owned(),
                ops: ops
                    .into_iter()
                    .map(serde_json::from_value)
                    .collect::<Result<_, _>>()?,
            })?;
            let mut layers = BTreeMap::new();
            let mut pages = BTreeMap::new();
            for created in change.created {
                match created {
                    Created::Layer {
                        id,
                        label: Some(label),
                    } => {
                        layers.insert(label, id);
                    }
                    Created::Page {
                        id,
                        label: Some(label),
                    } => {
                        pages.insert(label, id);
                    }
                    _ => {}
                }
            }
            Ok(Self {
                _dir: dir,
                session,
                layers,
                pages,
            })
        }

        fn id(&self, name: &str) -> LayerId {
            self.layers[name]
        }

        fn page(&self, name: &str) -> PageId {
            self.pages[name]
        }

        fn layer(&self, name: &str) -> Result<Layer> {
            Ok(self.session.project().layer(self.id(name))?.clone())
        }

        fn translation(&self, name: &str) -> Result<Text> {
            match self.layer(name)?.content {
                Content::Block(block) => block.translation.context("a translation"),
                _ => anyhow::bail!("{name} isn't a block"),
            }
        }

        fn edit(&mut self, ops: Vec<Op>) -> Result<()> {
            self.session.apply(Edit {
                label: "test".to_owned(),
                ops,
            })?;
            Ok(())
        }

        /// Translates the blocks `names` as Koharu's Translation stage does,
        /// into English, with Booharu's hook giving them the look of
        /// `settings` and the default font [`OLD`].
        fn translate(&mut self, names: &[&str], settings: &LetteringSettings) -> Result<()> {
            let project = self.session.project();
            let english: LanguageIdentifier = "en".parse()?;
            let mut ops = Vec::new();
            for name in names {
                let block = self.id(name);
                let placed = koharu_pipeline::placement(&project, block, &english)?;
                ops.push(Op::SetText {
                    layer: block.into(),
                    text: "Why?".to_owned(),
                });
                ops.push(placed.clone());
                ops.extend(styles::translated(
                    &project,
                    block,
                    "Why?",
                    &placed,
                    settings,
                    Some(OLD),
                    &booharu_core::emphasis::Marking::default(),
                )?);
                ops.push(Op::UpdateLayer {
                    layer: block.into(),
                    patch: json!({"translation": {"language": english}, "cleanup": {"visible": true}}),
                });
                ops.push(Op::SetAttribute {
                    layer: block.into(),
                    namespace: "c2pa".to_owned(),
                    key: "translation".to_owned(),
                    value: Some(json!({"action": "c2pa.translated"})),
                });
            }
            self.edit(ops)
        }

        /// Translates `name` as [`Fixture::translate`] does, as `text`, its
        /// model picking `words` to stress as `emphasis` says.
        fn emphasize(
            &mut self,
            name: &str,
            text: &str,
            words: &[&str],
            emphasis: &EmphasisSettings,
        ) -> Result<()> {
            let project = self.session.project();
            let english: LanguageIdentifier = "en".parse()?;
            let block = self.id(name);
            let placed = koharu_pipeline::placement(&project, block, &english)?;
            let words: Vec<String> = words.iter().map(|&word| word.to_owned()).collect();
            let marking = Marking {
                settings: Some(emphasis),
                words: &words,
            };
            let mut ops = vec![
                Op::SetText {
                    layer: block.into(),
                    text: text.to_owned(),
                },
                placed.clone(),
            ];
            ops.extend(styles::translated(
                &project,
                block,
                text,
                &placed,
                &look(true, None),
                Some(OLD),
                &marking,
            )?);
            ops.push(Op::SetAttribute {
                layer: block.into(),
                namespace: "c2pa".to_owned(),
                key: "translation".to_owned(),
                value: Some(json!({"action": "c2pa.translated"})),
            });
            self.edit(ops)
        }

        /// Sets character values of the translation of `name` by hand.
        fn restyle(&mut self, name: &str, chars: Value) -> Result<()> {
            let block = self.id(name);
            self.edit(vec![Op::UpdateLayer {
                layer: block.into(),
                patch: json!({"translation": {"style": {"chars": chars}}}),
            }])
        }

        /// Skews the translation of `name` `x` and `y` degrees, as the FX
        /// window does.
        fn skew(&mut self, name: &str, x: f64, y: f64) -> Result<()> {
            let layers = [self.id(name)];
            let change =
                crate::effects::set_skew(&self.session.project(), &layers, Some(x), Some(y))?;
            self.session.apply(change.edit.context("a skew")?)?;
            Ok(())
        }

        /// Resets the lettering of `names` under `settings` with the default
        /// font [`OLD`], the project's, and answers what it did.
        fn reset(&mut self, names: &[&str], settings: &LetteringSettings) -> Result<ResetCounts> {
            let layers: Vec<LayerId> = names.iter().map(|name| self.id(name)).collect();
            let (edit, counts) =
                reset_lettering(&self.session.project(), &layers, settings, Some(OLD))?;
            assert_eq!(edit.is_some(), counts.changed > 0);
            if let Some(edit) = edit {
                assert_eq!(edit.label, RESET_LABEL);
                self.session.apply(edit)?;
            }
            Ok(counts)
        }

        fn apply(
            &mut self,
            scope: &StyleScope,
            include_hand_edited: bool,
            settings: &LetteringSettings,
            origin: Origin,
        ) -> Result<StyleCounts> {
            self.apply_as(scope, include_hand_edited, None, settings, origin)
        }

        /// Applies the look as [`Fixture::apply`] does, every block lettered
        /// as `category` when it names one.
        fn apply_as(
            &mut self,
            scope: &StyleScope,
            include_hand_edited: bool,
            category: Option<LetteringCategory>,
            settings: &LetteringSettings,
            origin: Origin,
        ) -> Result<StyleCounts> {
            let project = self.session.project();
            let counted = count(
                &project,
                scope,
                include_hand_edited,
                category,
                settings,
                Some(NEW),
            )?;
            let (edit, counts) = apply(
                &project,
                scope,
                include_hand_edited,
                category,
                settings,
                Some(NEW),
                origin,
            )?;
            assert_eq!(counted, counts, "the count answers what Apply does");
            assert_eq!(edit.is_some(), counts.changed > 0);
            if let Some(edit) = edit {
                assert_eq!(edit.label, LABEL);
                self.session.apply(edit)?;
            }
            Ok(counts)
        }
    }

    fn counts(changed: u32, kept_hand_edited: u32, untranslated: u32) -> StyleCounts {
        StyleCounts {
            changed,
            kept_hand_edited,
            untranslated,
        }
    }

    #[test]
    fn apply_restyles_every_page_as_one_undo_step() -> Result<()> {
        let mut fixture = Fixture::new()?;
        let detected = look(true, None);
        fixture.translate(&["a", "b", "d", "e"], &detected)?;
        fixture.restyle("b", json!({"weight": 700}))?;
        let before = fixture.session.project();
        let steps = fixture.session.history().steps.len();

        // a, d, and e take the new font; b was changed by hand, and c has
        // no translation.
        let applied = fixture.apply(&StyleScope::Project, false, &detected, Origin::Automatic)?;
        assert_eq!(applied, counts(3, 1, 1));
        assert_eq!(fixture.session.history().steps.len(), steps + 1);
        for name in ["a", "d", "e"] {
            let translation = fixture.translation(name)?;
            assert_eq!(translation.style.chars.family.as_deref(), Some(NEW));
            assert_eq!(edited(&fixture.layer(name)?), Edited::default(), "{name}");
        }
        assert_eq!(
            fixture.translation("b")?.style.chars.family.as_deref(),
            Some(OLD)
        );
        assert!(fixture.layer("c")? == *before.layer(fixture.id("c"))?);

        // Nothing is left to change, so there's no edit.
        let again = fixture.apply(&StyleScope::Project, false, &detected, Origin::Automatic)?;
        assert_eq!(again, counts(0, 1, 1));
        assert_eq!(fixture.session.history().steps.len(), steps + 1);

        // One undo puts every page back.
        fixture.session.undo()?;
        assert!(fixture.session.project() == before);
        Ok(())
    }

    #[test]
    fn a_block_changed_by_hand_is_kept_unless_included() -> Result<()> {
        let mut fixture = Fixture::new()?;
        fixture.translate(&["a", "b"], &look(true, None))?;
        // b's fill set by hand, and a word of a marked yellow.
        fixture.restyle("b", json!({"fill": YELLOW}))?;
        let a = fixture.id("a");
        let spans = vec![
            Span {
                insert: "Why".to_owned(),
                attributes: CharStyle {
                    fill: Some(YELLOW),
                    ..CharStyle::default()
                },
            },
            Span {
                insert: "?".to_owned(),
                attributes: CharStyle::default(),
            },
        ];
        fixture.edit(vec![Op::SetRichText {
            layer: a.into(),
            spans,
        }])?;
        let page = StyleScope::Pages {
            pages: vec![fixture.page("p1")],
        };

        let kept = fixture.apply(&page, false, &look(true, None), Origin::Automatic)?;
        assert_eq!(kept, counts(0, 2, 2));

        // Included, with detection on: the font, but the hand colours stay,
        // and so they stay hand edits.
        let included = fixture.apply(&page, true, &look(true, None), Origin::Automatic)?;
        assert_eq!(included, counts(2, 0, 2));
        let b = fixture.translation("b")?;
        assert_eq!(b.style.chars.family.as_deref(), Some(NEW));
        assert_eq!(b.style.chars.fill, Some(YELLOW));
        assert!(edited(&fixture.layer("b")?).style);
        assert_eq!(
            fixture.translation("a")?.content.0[0].attributes.fill,
            Some(YELLOW)
        );
        assert!(edited(&fixture.layer("a")?).style);

        // With detection off, the default colour replaces them, the mark
        // included, and they're Booharu's look again.
        let off = look(false, Some(RED));
        let recoloured = fixture.apply(&page, true, &off, Origin::Automatic)?;
        assert_eq!(recoloured, counts(2, 0, 2));
        for name in ["a", "b"] {
            let translation = fixture.translation(name)?;
            assert_eq!(translation.style.chars.fill, Some(RED), "{name}");
            assert!(
                translation
                    .content
                    .0
                    .iter()
                    .all(|span| span.attributes == CharStyle::default()),
                "{name}"
            );
            assert!(!edited(&fixture.layer(name)?).hand_edited(), "{name}");
        }
        Ok(())
    }

    #[test]
    fn detection_on_never_recolours() -> Result<()> {
        let mut fixture = Fixture::new()?;
        fixture.translate(&["a", "e"], &look(true, None))?;

        // On: the font alone, whatever colour the default look holds.
        let on = look(true, Some(YELLOW));
        let applied = fixture.apply(&StyleScope::Project, false, &on, Origin::Automatic)?;
        assert_eq!(applied.changed, 2);
        for name in ["a", "e"] {
            let chars = fixture.translation(name)?.style.chars;
            assert_eq!(
                (chars.fill, chars.stroke),
                (Some(BLUE), Some(WHITE)),
                "{name}"
            );
        }

        // Off: the default fill, with no outline for dialogue and a
        // legibility outline for the sound effect, a sixteenth of the size
        // its source gave it, as its first translation starts as its
        // source's blank (Koharu's #1494), its page having no lettering size.
        let off = look(false, Some(YELLOW));
        fixture.apply(&StyleScope::Project, false, &off, Origin::Automatic)?;
        let a = fixture.translation("a")?.style.chars;
        assert_eq!(
            (a.fill, a.stroke, a.stroke_width),
            (Some(YELLOW), None, None)
        );
        let e = fixture.translation("e")?.style.chars;
        assert_eq!(
            (e.fill, e.stroke, e.stroke_width),
            (Some(YELLOW), Some(BLACK), Some(2.0))
        );
        Ok(())
    }

    #[test]
    fn a_locked_block_is_counted_as_kept() -> Result<()> {
        let mut fixture = Fixture::new()?;
        fixture.translate(&["a", "d"], &look(true, None))?;
        // a is locked, and d is inside a locked group.
        let (a, g) = (fixture.id("a"), fixture.id("g"));
        fixture.edit(vec![
            Op::UpdateLayer {
                layer: a.into(),
                patch: json!({"locked": true}),
            },
            Op::UpdateLayer {
                layer: g.into(),
                patch: json!({"locked": true}),
            },
        ])?;
        let applied = fixture.apply(
            &StyleScope::Project,
            true,
            &look(true, None),
            Origin::Automatic,
        )?;
        assert_eq!(applied, counts(0, 2, 3));
        for name in ["a", "d"] {
            assert_eq!(
                fixture.translation(name)?.style.chars.family.as_deref(),
                Some(OLD)
            );
        }
        Ok(())
    }

    #[test]
    fn the_scope_names_blocks_or_pages() -> Result<()> {
        let mut fixture = Fixture::new()?;
        fixture.translate(&["a", "b", "e"], &look(true, None))?;
        let detected = look(true, None);

        // A block named twice counts once.
        let a = fixture.id("a");
        let blocks = StyleScope::Blocks { blocks: vec![a, a] };
        assert_eq!(
            fixture.apply(&blocks, false, &detected, Origin::Automatic)?,
            counts(1, 0, 0)
        );
        assert_eq!(
            fixture.translation("b")?.style.chars.family.as_deref(),
            Some(OLD)
        );
        let second = StyleScope::Pages {
            pages: vec![fixture.page("p2")],
        };
        assert_eq!(
            fixture.apply(&second, false, &detected, Origin::Automatic)?,
            counts(1, 0, 0)
        );
        assert_eq!(
            fixture.translation("e")?.style.chars.family.as_deref(),
            Some(NEW)
        );

        // A layer that isn't a block is refused.
        let group = StyleScope::Blocks {
            blocks: vec![fixture.id("g")],
        };
        assert!(
            count(
                &fixture.session.project(),
                &group,
                false,
                None,
                &detected,
                Some(NEW)
            )
            .is_err()
        );

        // The interface's shape.
        assert_eq!(
            serde_json::to_value(&blocks)?,
            json!({"kind": "blocks", "blocks": [a, a]})
        );
        let project: StyleScope = serde_json::from_value(json!({"kind": "project"}))?;
        assert_eq!(project, StyleScope::Project);
        Ok(())
    }

    #[test]
    fn the_agents_apply_is_its_choice() -> Result<()> {
        let mut fixture = Fixture::new()?;
        fixture.translate(&["a"], &look(true, None))?;
        let a = StyleScope::Blocks {
            blocks: vec![fixture.id("a")],
        };
        fixture.apply(&a, false, &look(true, None), Origin::Agent)?;
        let layer = fixture.layer("a")?;
        assert_eq!(Auto::of(&layer).origin, Origin::Agent);
        assert!(edited(&layer).style);

        // So the user's Apply keeps it, unless it includes hand edits.
        let off = look(false, Some(YELLOW));
        assert_eq!(
            fixture.apply(&a, false, &off, Origin::Automatic)?,
            counts(0, 1, 0)
        );
        assert_eq!(
            fixture.apply(&a, true, &off, Origin::Automatic)?,
            counts(1, 0, 0)
        );
        assert_eq!(Auto::of(&fixture.layer("a")?).origin, Origin::Automatic);
        Ok(())
    }

    #[test]
    fn with_the_switch_on_apply_letters_each_block_in_its_categorys_font() -> Result<()> {
        use booharu_core::lettering::CategoryFont;
        let mut fixture = Fixture::new()?;
        fixture.translate(&["a", "b", "e"], &look(true, None))?;
        // b's font changed by hand, and a word of it marked light.
        fixture.restyle("b", json!({"family": "Comic Sans"}))?;
        let b = fixture.id("b");
        let spans = vec![
            Span {
                insert: "Why".to_owned(),
                attributes: CharStyle {
                    weight: Some(300),
                    ..CharStyle::default()
                },
            },
            Span {
                insert: "?".to_owned(),
                attributes: CharStyle::default(),
            },
        ];
        fixture.edit(vec![Op::SetRichText {
            layer: b.into(),
            spans,
        }])?;
        // a and b float outside any balloon; e is Koharu's sound effect.
        let lettered = LetteringSettings {
            auto: true,
            categories: BTreeMap::from([
                (
                    LetteringCategory::Floating,
                    CategoryFont {
                        family: Some("Wild Float".to_owned()),
                        bold: true,
                        italic: false,
                    },
                ),
                (
                    LetteringCategory::Sfx,
                    CategoryFont {
                        family: None,
                        bold: false,
                        italic: true,
                    },
                ),
            ]),
            ..look(true, None)
        };
        type Font = (Option<String>, Option<u16>, Option<bool>);
        let font = |fixture: &Fixture, name: &str| -> Result<Font> {
            let chars = fixture.translation(name)?.style.chars;
            Ok((chars.family, chars.weight, chars.italic))
        };

        // b's hand-changed font is kept unless the user confirms.
        let kept = fixture.apply(&StyleScope::Project, false, &lettered, Origin::Automatic)?;
        assert_eq!(kept, counts(2, 1, 2));
        assert_eq!(
            font(&fixture, "a")?,
            (Some("Wild Float".to_owned()), Some(700), None)
        );
        assert_eq!(
            font(&fixture, "e")?,
            (Some(NEW.to_owned()), None, Some(true))
        );
        assert_eq!(font(&fixture, "b")?.0.as_deref(), Some("Comic Sans"));
        for name in ["a", "e"] {
            let found = fixture.layer(name)?;
            assert_eq!(edited(&found), Edited::default(), "{name}");
            let style = Auto::of(&found).style.context("a style")?;
            assert_eq!(
                (style.family, style.weight, style.italic),
                font(&fixture, name)?,
                "{name}"
            );
        }

        // Confirmed: b takes the category's font, and its word keeps its
        // mark, as a category's font never touches the marks.
        let included = fixture.apply(&StyleScope::Project, true, &lettered, Origin::Automatic)?;
        assert_eq!(included, counts(1, 0, 2));
        assert_eq!(
            font(&fixture, "b")?,
            (Some("Wild Float".to_owned()), Some(700), None)
        );
        assert_eq!(
            fixture.translation("b")?.content.0[0].attributes.weight,
            Some(300)
        );

        // Off again, Apply is stage 3's: the default font, the category's
        // weight left as it is.
        let off = LetteringSettings {
            auto: false,
            ..lettered
        };
        let restyled = fixture.apply(&StyleScope::Project, true, &off, Origin::Automatic)?;
        assert_eq!(restyled, counts(2, 0, 2));
        assert_eq!(
            font(&fixture, "a")?,
            (Some(NEW.to_owned()), Some(700), None)
        );
        Ok(())
    }

    #[test]
    fn a_named_category_letters_every_block_of_the_scope_even_with_the_switch_off() -> Result<()> {
        use booharu_core::lettering::CategoryFont;
        let mut fixture = Fixture::new()?;
        fixture.translate(&["a", "b", "c"], &look(true, None))?;
        // Shout has a font; the switch stays off.
        let shout = LetteringSettings {
            categories: BTreeMap::from([(
                LetteringCategory::Shout,
                CategoryFont {
                    family: Some("Badaboom BB".to_owned()),
                    bold: true,
                    italic: false,
                },
            )]),
            ..look(true, None)
        };
        let font = |fixture: &Fixture, name: &str| -> Result<_> {
            let chars = fixture.translation(name)?.style.chars;
            Ok((chars.family, chars.weight, chars.italic))
        };
        let a = StyleScope::Blocks {
            blocks: vec![fixture.id("a")],
        };
        let applied = fixture.apply_as(
            &a,
            false,
            Some(LetteringCategory::Shout),
            &shout,
            Origin::Agent,
        )?;
        assert_eq!(applied, counts(1, 0, 0));
        assert_eq!(
            font(&fixture, "a")?,
            (Some("Badaboom BB".to_owned()), Some(700), None)
        );
        // The agent's choice, kept as a hand edit is.
        let layer = fixture.layer("a")?;
        assert_eq!(Auto::of(&layer).origin, Origin::Agent);
        assert!(edited(&layer).style);
        assert_eq!(font(&fixture, "b")?.0.as_deref(), Some(OLD));

        // A category with no font, nor Normal Dialogue, letters in the
        // default font, neither bold nor italic.
        let b = StyleScope::Blocks {
            blocks: vec![fixture.id("b")],
        };
        fixture.apply_as(
            &b,
            false,
            Some(LetteringCategory::Whisper),
            &shout,
            Origin::Agent,
        )?;
        assert_eq!(font(&fixture, "b")?, (Some(NEW.to_owned()), None, None));
        Ok(())
    }

    #[test]
    fn reset_gives_a_box_its_first_look_back_in_one_undo_step() -> Result<()> {
        let mut fixture = Fixture::new()?;
        let settings = look(true, None);
        fixture.translate(&["a", "b"], &settings)?;
        let first = fixture.translation("a")?;
        let a = fixture.id("a");
        // By hand: another font, colours, a second border, the method, a
        // word mark and other words, then a frame, an alignment and a line
        // height of its own.
        fixture.restyle(
            "a",
            json!({
                "family": "Comic Sans", "weight": 700, "italic": true,
                "fill": RED, "stroke": YELLOW, "stroke_width": 5.0,
                "outer_stroke": BLACK, "outer_stroke_width": 2.0,
            }),
        )?;
        let frame = json!({"box": {"bounds": {"x": 5.0, "y": 5.0, "width": 80.0, "height": 60.0}}});
        fixture.edit(vec![
            Op::SetRichText {
                layer: a.into(),
                spans: vec![
                    Span {
                        insert: "Why".to_owned(),
                        attributes: CharStyle {
                            fill: Some(YELLOW),
                            ..CharStyle::default()
                        },
                    },
                    Span {
                        insert: " not?".to_owned(),
                        attributes: CharStyle::default(),
                    },
                ],
            },
            Op::UpdateLayer {
                layer: a.into(),
                patch: json!({"translation": {
                    "style": {"stroke_method": "contour", "align": "end", "line_height": 1.5},
                    "frame": frame,
                }}),
            },
        ])?;
        let hand = fixture.translation("a")?;
        assert_eq!(
            edited(&fixture.layer("a")?),
            Edited {
                text: true,
                style: true,
                ..Edited::default()
            }
        );
        let before = fixture.session.project();
        let steps = fixture.session.history().steps.len();

        let counts = fixture.reset(&["a", "a"], &settings)?;
        assert_eq!(
            counts,
            ResetCounts {
                changed: 1,
                ..ResetCounts::default()
            }
        );
        assert_eq!(fixture.session.history().steps.len(), steps + 1);
        assert_eq!(
            fixture.session.history().steps.last().map(String::as_str),
            Some(RESET_LABEL)
        );
        let reset = fixture.translation("a")?;
        // The first look, every mark gone, the border drawn Koharu's way.
        assert_eq!(
            AutoStyle::of(&reset.style.chars),
            AutoStyle::of(&first.style.chars)
        );
        assert_eq!(reset.style.stroke_method, None);
        assert_eq!(
            reset.content.0,
            vec![Span {
                insert: "Why not?".to_owned(),
                attributes: CharStyle::default(),
            }]
        );
        // The words, size, frame, alignment and line height stay.
        assert_eq!(reset.style.chars.size, hand.style.chars.size);
        assert_eq!(reset.frame, hand.frame);
        assert_eq!(reset.style.align, hand.style.align);
        assert_eq!(reset.style.line_height, Some(1.5));
        assert_eq!(reset.language, hand.language);
        assert_eq!(reset.direction, hand.direction);
        // Booharu's look again; the words typed by hand stay a hand edit.
        assert_eq!(
            edited(&fixture.layer("a")?),
            Edited {
                text: true,
                ..Edited::default()
            }
        );

        // A second press changes nothing and adds no undo step.
        assert_eq!(fixture.reset(&["a"], &settings)?, ResetCounts::default());
        assert_eq!(fixture.session.history().steps.len(), steps + 1);
        fixture.session.undo()?;
        assert!(fixture.session.project() == before);
        Ok(())
    }

    #[test]
    fn reset_keeps_a_new_size_and_outlines_at_it() -> Result<()> {
        let mut fixture = Fixture::new()?;
        let settings = look(true, None);
        fixture.translate(&["b"], &settings)?;
        fixture.restyle("b", json!({"size": 24.0, "stroke_width": 9.0}))?;
        fixture.reset(&["b"], &settings)?;
        let chars = fixture.translation("b")?.style.chars;
        // The source's outline, 3 at size 32, scaled to the size kept.
        assert_eq!(
            (chars.size, chars.stroke, chars.stroke_width),
            (Some(24.0), Some(WHITE), Some(2.25))
        );
        assert!(!edited(&fixture.layer("b")?).style);
        Ok(())
    }

    #[test]
    fn reset_counts_locked_and_untranslated_layers_and_letters_a_text_layer() -> Result<()> {
        let mut fixture = Fixture::new()?;
        let settings = look(false, Some(BLACK));
        fixture.translate(&["a", "d"], &settings)?;
        let p1 = fixture.page("p1");
        let change = fixture.session.apply(Edit {
            label: "test".to_owned(),
            ops: [
                json!({"op": "add_layer", "id": "$t", "page": p1, "index": 4, "layer": {
                    "kind": "text",
                    "content": [{"insert": "Hi", "attributes": {"italic": true}}, {"insert": "!"}],
                    "style": {"chars": {"family": "Papyrus", "size": 20.0, "fill": RED}},
                    "frame": {"box": {"bounds": {"x": 200.0, "y": 200.0, "width": 80.0, "height": 40.0}}},
                }}),
                json!({"op": "add_layer", "id": "$h", "page": p1, "index": 5, "layer": {"kind": "group"}}),
            ]
            .into_iter()
            .map(serde_json::from_value)
            .collect::<Result<_, _>>()?,
        })?;
        for created in change.created {
            if let Created::Layer {
                id,
                label: Some(label),
            } = created
            {
                fixture.layers.insert(label, id);
            }
        }
        let g = fixture.id("g");
        fixture.edit(vec![Op::UpdateLayer {
            layer: g.into(),
            patch: json!({"locked": true}),
        }])?;
        let d = fixture.layer("d")?;

        // d is in the locked group g, c has no translation, and the group h
        // has no lettering.
        let counts = fixture.reset(&["c", "d", "h", "t"], &settings)?;
        assert_eq!(
            counts,
            ResetCounts {
                changed: 1,
                locked: 1,
                untranslated: 1,
            }
        );
        assert!(fixture.layer("d")? == d);
        // The text layer gets the default look at its own size, unmarked,
        // and no record.
        let t = fixture.layer("t")?;
        let Content::Text(text) = &t.content else {
            anyhow::bail!("t isn't a text layer");
        };
        assert_eq!(
            AutoStyle::of(&text.style.chars),
            AutoStyle::of(&styles::default_chars(&settings, Some(OLD)))
        );
        assert_eq!(text.style.chars.size, Some(20.0));
        assert_eq!(text.content.text(), "Hi!");
        assert!(
            text.content
                .0
                .iter()
                .all(|span| span.attributes == CharStyle::default())
        );
        assert_eq!(attrs::get::<Auto>(&t), None);
        Ok(())
    }

    #[test]
    fn a_box_styled_before_its_translated_is_left_out_of_apply_as_untranslated() -> Result<()> {
        let mut fixture = Fixture::new()?;
        let settings = look(false, Some(BLACK));
        fixture.edit(vec![type_bar_style(fixture.id("a"))])?;
        let before = fixture.layer("a")?;
        let scope = StyleScope::Blocks {
            blocks: vec![fixture.id("a")],
        };

        // Hand edits included or not, the style set before translating is
        // no translation to restyle, and stays the user's.
        for include_hand_edited in [false, true] {
            let applied =
                fixture.apply(&scope, include_hand_edited, &settings, Origin::Automatic)?;
            assert_eq!(applied, counts(0, 0, 1));
            assert!(fixture.layer("a")? == before);
        }
        Ok(())
    }

    #[test]
    fn reset_clears_a_style_set_before_translating_and_the_box_stays_translatable() -> Result<()> {
        let mut fixture = Fixture::new()?;
        let settings = look(false, Some(BLACK));
        fixture.edit(vec![type_bar_style(fixture.id("a"))])?;

        let counts = fixture.reset(&["a"], &settings)?;
        assert_eq!(
            counts,
            ResetCounts {
                changed: 1,
                locked: 0,
                untranslated: 0,
            }
        );
        let a = fixture.layer("a")?;
        let Content::Block(block) = &a.content else {
            anyhow::bail!("a isn't a block");
        };
        let translation = block.translation.as_ref().context("the placeholder")?;
        assert_eq!(translation.style.chars.fill, Some(BLACK));
        assert_ne!(Some(FILL), translation.style.chars.fill);
        // Its record keeps no text, so the box is still one to translate.
        assert_eq!(attrs::get::<Auto>(&a).context("a record")?.text, None);
        assert!(status::placeholder(&a, block));
        assert!(status::untranslated(&a, block));
        Ok(())
    }

    fn span(insert: &str, attributes: CharStyle) -> Span {
        Span {
            insert: insert.to_owned(),
            attributes,
        }
    }

    #[test]
    fn apply_takes_the_keys_it_clears_off_the_runs_marks() -> Result<()> {
        let mut fixture = Fixture::new()?;
        let red = EmphasisSettings {
            default: EmphasisStyle {
                fill: Some(RED),
                ..EmphasisSettings::default().default
            },
            ..EmphasisSettings::default()
        };
        fixture.emphasize("a", "Why NO?", &["NO"], &red)?;
        let marks = |fixture: &Fixture| -> Result<Vec<AutoMark>> {
            Ok(Auto::of(&fixture.layer("a")?).marks)
        };
        let run = marks(&fixture)?;
        assert_eq!(run.len(), 1);
        assert_eq!(
            (run[0].style.weight, run[0].style.fill),
            (Some(700), Some(RED))
        );
        // The run's marks are no hand edit, so Apply takes the block.
        assert!(!edited(&fixture.layer("a")?).hand_edited());
        let a = StyleScope::Blocks {
            blocks: vec![fixture.id("a")],
        };

        // With detection on, the font alone: the words keep their marks, and
        // the record its.
        let on = fixture.apply(&a, false, &look(true, None), Origin::Automatic)?;
        assert_eq!(on, counts(1, 0, 0));
        assert_eq!(marks(&fixture)?, run);

        // Off, the default colour replaces the words' red, in the record
        // too, so they're still the run's.
        let off = fixture.apply(&a, false, &look(false, Some(YELLOW)), Origin::Automatic)?;
        assert_eq!(off, counts(1, 0, 0));
        let bold = CharStyle {
            fill: None,
            ..run[0].style.clone()
        };
        assert_eq!(
            fixture.translation("a")?.content.0,
            [
                span("Why ", CharStyle::default()),
                span("NO", bold.clone()),
                span("?", CharStyle::default()),
            ]
        );
        assert_eq!(
            marks(&fixture)?,
            [AutoMark {
                style: bold,
                ..run[0].clone()
            }]
        );
        assert!(!edited(&fixture.layer("a")?).hand_edited());

        // A mark left setting nothing goes.
        let only_red = EmphasisSettings {
            default: EmphasisStyle {
                fill: Some(RED),
                ..EmphasisStyle::default()
            },
            ..EmphasisSettings::default()
        };
        fixture.emphasize("b", "Why NO?", &["NO"], &only_red)?;
        assert_eq!(Auto::of(&fixture.layer("b")?).marks.len(), 1);
        let b = StyleScope::Blocks {
            blocks: vec![fixture.id("b")],
        };
        fixture.apply(&b, false, &look(false, Some(YELLOW)), Origin::Automatic)?;
        assert!(Auto::of(&fixture.layer("b")?).marks.is_empty());
        assert_eq!(fixture.translation("b")?.content, RichText::from("Why NO?"));
        Ok(())
    }

    #[test]
    fn reset_drops_the_runs_marks_with_the_words_marks() -> Result<()> {
        let mut fixture = Fixture::new()?;
        fixture.emphasize("a", "Why NO?", &["NO"], &EmphasisSettings::default())?;
        assert_eq!(Auto::of(&fixture.layer("a")?).marks.len(), 1);
        let counts = fixture.reset(&["a"], &look(true, None))?;
        assert_eq!(counts.changed, 1);
        assert!(Auto::of(&fixture.layer("a")?).marks.is_empty());
        assert_eq!(fixture.translation("a")?.content, RichText::from("Why NO?"));
        assert!(!edited(&fixture.layer("a")?).hand_edited());
        Ok(())
    }

    /// A gradient from red at the bottom to yellow at the top.
    fn gradient() -> Gradient {
        Gradient {
            kind: GradientKind::Linear,
            angle: 90.0,
            stops: vec![
                GradientStop {
                    offset: 0.0,
                    color: RED,
                },
                GradientStop {
                    offset: 1.0,
                    color: YELLOW,
                },
            ],
        }
    }

    fn gradient_mark() -> CharStyle {
        CharStyle {
            fill_gradient: Some(gradient()),
            ..CharStyle::default()
        }
    }

    fn close(a: Transform, b: Transform) -> bool {
        a.0.iter()
            .flatten()
            .zip(b.0.iter().flatten())
            .all(|(a, b)| (a - b).abs() <= 1e-9)
    }

    #[test]
    fn reset_takes_gradients_letter_spacing_and_the_skew_off_in_one_undo_step() -> Result<()> {
        let mut fixture = Fixture::new()?;
        let settings = look(true, None);
        fixture.translate(&["a", "b", "d"], &settings)?;
        let placed = [fixture.translation("a")?, fixture.translation("b")?];
        // By hand: a's gradient, letter spacing, a gradient on one of its
        // words and a skew; b's skew alone; and d's skew, before its group
        // g is locked.
        fixture.restyle("a", json!({"fill_gradient": gradient(), "tracking": 300.0}))?;
        let a = fixture.id("a");
        fixture.edit(vec![Op::SetRichText {
            layer: a.into(),
            spans: vec![
                span("Why", gradient_mark()),
                span("?", CharStyle::default()),
            ],
        }])?;
        fixture.skew("a", 20.0, 5.0)?;
        fixture.skew("b", -10.0, 0.0)?;
        fixture.skew("d", 15.0, 0.0)?;
        let g = fixture.id("g");
        fixture.edit(vec![Op::UpdateLayer {
            layer: g.into(),
            patch: json!({"locked": true}),
        }])?;
        let d = fixture.layer("d")?;
        let before = fixture.session.project();
        let steps = fixture.session.history().steps.len();

        let counts = fixture.reset(&["a", "b", "d"], &settings)?;

        assert_eq!(
            counts,
            ResetCounts {
                changed: 2,
                locked: 1,
                untranslated: 0,
            }
        );
        assert_eq!(fixture.session.history().steps.len(), steps + 1);
        for (name, placed) in ["a", "b"].into_iter().zip(&placed) {
            let reset = fixture.translation(name)?;
            let chars = &reset.style.chars;
            assert_eq!(
                (&chars.fill_gradient, chars.tracking),
                (&None, None),
                "{name}"
            );
            assert!(
                reset
                    .content
                    .0
                    .iter()
                    .all(|span| span.attributes == CharStyle::default()),
                "{name}"
            );
            // The skew comes off: the transform as placed, the record gone.
            assert!(close(reset.transform, placed.transform), "{name}");
            let layer = fixture.layer(name)?;
            assert_eq!(attrs::get::<Skew>(&layer), None, "{name}");
            assert_eq!(edited(&layer), Edited::default(), "{name}");
        }
        // The locked box keeps all of it.
        assert!(fixture.layer("d")? == d);
        // A second press changes nothing; one undo puts every effect back.
        assert_eq!(
            fixture.reset(&["a", "b"], &settings)?,
            ResetCounts::default()
        );
        fixture.session.undo()?;
        assert!(fixture.session.project() == before);
        Ok(())
    }

    #[test]
    fn apply_takes_gradients_and_letter_spacing_off_with_the_look() -> Result<()> {
        let mut fixture = Fixture::new()?;
        fixture.translate(&["a", "b"], &look(true, None))?;
        // By hand: a's gradient and letter spacing, a gradient on one of its
        // words and a flat colour on another, and wider spacing on b's word.
        fixture.restyle("a", json!({"fill_gradient": gradient(), "tracking": 300.0}))?;
        let (a, b) = (fixture.id("a"), fixture.id("b"));
        let yellow = CharStyle {
            fill: Some(YELLOW),
            ..CharStyle::default()
        };
        let wide = CharStyle {
            tracking: Some(100.0),
            ..CharStyle::default()
        };
        fixture.edit(vec![
            Op::SetRichText {
                layer: a.into(),
                spans: vec![span("Why", gradient_mark()), span("?", yellow.clone())],
            },
            Op::SetRichText {
                layer: b.into(),
                spans: vec![span("Why", wide), span("?", CharStyle::default())],
            },
        ])?;
        let page = StyleScope::Pages {
            pages: vec![fixture.page("p1")],
        };
        // Hand edits, so kept unless included.
        let kept = fixture.apply(&page, false, &look(true, None), Origin::Automatic)?;
        assert_eq!(kept, counts(0, 2, 2));

        // Included, with detection on: the gradients and the spacing go,
        // so the box's own fill shows, flat; a word's colour stays, and so
        // stays a hand edit.
        let on = fixture.apply(&page, true, &look(true, None), Origin::Automatic)?;
        assert_eq!(on, counts(2, 0, 2));
        let chars = fixture.translation("a")?.style.chars;
        assert_eq!(
            (chars.fill, &chars.fill_gradient, chars.tracking),
            (Some(BLUE), &None, None)
        );
        assert_eq!(
            fixture.translation("a")?.content.0,
            [span("Why", CharStyle::default()), span("?", yellow)]
        );
        assert!(edited(&fixture.layer("a")?).style);
        assert_eq!(fixture.translation("b")?.content, RichText::from("Why?"));
        assert!(!edited(&fixture.layer("b")?).hand_edited());

        // Off: the default colour is the box's one paint, on every word.
        let off = look(false, Some(RED));
        fixture.apply(&page, true, &off, Origin::Automatic)?;
        let translation = fixture.translation("a")?;
        let chars = &translation.style.chars;
        assert_eq!((chars.fill, &chars.fill_gradient), (Some(RED), &None));
        assert_eq!(translation.content, RichText::from("Why?"));
        assert!(!edited(&fixture.layer("a")?).hand_edited());
        Ok(())
    }
}
