//! Apply: the default text look given again to translations already made,
//! on some blocks, some pages, or the whole project, as one undo step, with
//! how many blocks it changed, kept, and found untranslated, which a count
//! answers beforehand without changing anything.
//!
//! A translation gets the look a new one would with nothing of its own
//! ([`booharu_core::styles::restyle`]): the default font, the first of
//! Koharu's Preferred fonts, in place of its family, and with
//! `detect_colors` off the default colours and outline in place of its own;
//! with it on, Apply never recolours. A block changed by hand
//! ([`booharu_core::edited`]) is kept unless the caller includes those, and
//! a locked block, or one inside a locked layer, is always kept, so a lock
//! never fails the edit. An included block also loses its marks for the
//! keys Apply writes, so the look shows on every character.
//!
//! Each block Apply changes gets its hand-edit record in the same edit: the
//! keys Apply wrote take their new values and the others keep theirs, so a
//! hand edit Apply left stays one, and the record's origin says who applied
//! the look.

use std::collections::BTreeSet;

use anyhow::{Result, ensure};
use booharu_core::{
    attrs::{self, Auto, Origin},
    edited,
    settings::LetteringSettings,
    styles,
};
use koharu_scene::{
    CharStyle, Content, Edit, Layer, LayerId, Op, PageId, Project, RichText, Role, Span,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

/// The undo step's label, which the History panel translates.
pub const LABEL: &str = "booharu_apply_text_style";

/// The blocks Apply looks at: `{"kind": "blocks", "blocks": [...]}`,
/// `{"kind": "pages", "pages": [...]}`, or `{"kind": "project"}`.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, specta::Type, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
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
    settings: &LetteringSettings,
    font: Option<&str>,
) -> Result<StyleCounts> {
    Ok(planned(project, scope, include_hand_edited, settings, font)?.counts)
}

/// The edit giving the translations of `scope` in `project` the default
/// look of `settings` and `font`, the first Preferred font, again, as one
/// undo step labelled [`LABEL`] over any number of pages, with its numbers;
/// none when it changes nothing. A block changed by hand takes it only with
/// `include_hand_edited`. `origin` goes in each changed block's record:
/// [`Origin::Agent`] makes the look the agent's choice, which a later
/// automatic step keeps as it keeps a hand edit.
pub fn apply(
    project: &Project,
    scope: &StyleScope,
    include_hand_edited: bool,
    settings: &LetteringSettings,
    font: Option<&str>,
    origin: Origin,
) -> Result<(Option<Edit>, StyleCounts)> {
    let plan = planned(project, scope, include_hand_edited, settings, font)?;
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
        let record = Auto {
            origin,
            ..Auto::of(restyled.layer)
        }
        .restyled(&restyled.after, &written);
        ops.push(attrs::set(id, &record)?);
    }
    let edit = (!ops.is_empty()).then(|| Edit {
        label: LABEL.to_owned(),
        ops,
    });
    Ok((edit, plan.counts))
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
}

fn planned<'p>(
    project: &'p Project,
    scope: &StyleScope,
    include_hand_edited: bool,
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
        let Some(translation) = &block.translation else {
            plan.counts.untranslated += 1;
            continue;
        };
        if locked(project, layer)? || (!include_hand_edited && edited(layer).hand_edited()) {
            plan.counts.kept_hand_edited += 1;
            continue;
        }
        let current = &translation.style.chars;
        let patch = styles::restyle(settings, font, block.role == Role::Sfx, current);
        let after = styles::patched(current, &patch)?;
        let spans = unmarked(&translation.content, &patch)?;
        if after == *current && spans.is_none() {
            continue;
        }
        plan.counts.changed += 1;
        plan.restyled.push(Restyled {
            layer,
            patch,
            after,
            spans,
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
fn locked(project: &Project, layer: &Layer) -> Result<bool> {
    let mut at = Some(layer);
    while let Some(layer) = at {
        if layer.locked {
            return Ok(true);
        }
        at = project.parent(layer.id)?.1;
    }
    Ok(false)
}

/// `content`'s spans without their marks for the keys `patch` writes, so its
/// values show on every character; none when no span has one.
fn unmarked(content: &RichText, patch: &Map<String, Value>) -> Result<Option<Vec<Span>>> {
    let cleared: Map<String, Value> = patch.keys().map(|key| (key.clone(), Value::Null)).collect();
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
    use booharu_core::{Edited, settings::DefaultTextStyle};
    use koharu_scene::{Created, LanguageIdentifier, Rgba, Session, Text};
    use tempfile::TempDir;

    use super::*;

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
                    project.layer(block)?,
                    "Why?",
                    &placed,
                    settings,
                    Some(OLD),
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

        /// Sets character values of the translation of `name` by hand.
        fn restyle(&mut self, name: &str, chars: Value) -> Result<()> {
            let block = self.id(name);
            self.edit(vec![Op::UpdateLayer {
                layer: block.into(),
                patch: json!({"translation": {"style": {"chars": chars}}}),
            }])
        }

        fn apply(
            &mut self,
            scope: &StyleScope,
            include_hand_edited: bool,
            settings: &LetteringSettings,
            origin: Origin,
        ) -> Result<StyleCounts> {
            let project = self.session.project();
            let counted = count(&project, scope, include_hand_edited, settings, Some(NEW))?;
            let (edit, counts) = apply(
                &project,
                scope,
                include_hand_edited,
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
        // legibility outline for the sound effect, as wide as Koharu's, its
        // page having no lettering size.
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
            (Some(YELLOW), Some(BLACK), Some(3.0))
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
}
