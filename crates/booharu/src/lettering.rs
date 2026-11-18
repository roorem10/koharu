//! Changing blocks' lettering categories, and what the interface shows of
//! each block's category.
//!
//! A change is one undo step labelled [`LABEL`] over any number of blocks,
//! so one undo brings back each block's label and font together (#78). Each
//! block gets its new label, `booharu.lettering`, or loses it for Automatic,
//! and, while Pick fonts automatically is on and it has a translation, the
//! font of the category it then has, with its hand-edit record. The step is
//! built even when a label stays as it is, so picking the same category
//! again applies a font changed in Settings since; a step that changes
//! nothing adds no undo step.
//!
//! - A user's choice is an explicit pick: it restyles a block even when its
//!   font was changed by hand, and Automatic clears any label.
//! - A model's choice, the agent's, counts as automatic (T18): it never
//!   replaces a user's label, and never restyles a block whose style was
//!   changed by hand, though its label changes; Automatic clears only a
//!   model's label.
//! - A locked block, or one inside a locked layer, is left out and counted,
//!   so a lock never fails the step (D7).
//!
//! A category's font is written as the `lettering` module of booharu-core
//! resolves it: the translation's family, weight, and italic, never other
//! keys and never the marks.
//!
//! What the interface and the agent read of it: each block's category with
//! why, its label, its balloon's shape, and whether it was changed by hand
//! ([`info`] for a page, [`blocks`] for blocks named), and the font each
//! category letters its text in, with where it comes from ([`fonts`]).

use std::collections::BTreeSet;

use anyhow::{Context as _, Result, bail};
use booharu_core::{
    attrs::{self, Auto, AutoStyle, Lettering, LetteringSource},
    edited,
    lettering::{self, CategoryFont, CategoryReason, LetteringCategory},
    settings::LetteringSettings,
    shape::{self, BlockShape},
    styles,
};
use koharu_scene::{Content, Edit, Layer, LayerId, Op, Page, PageId, Project};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::styles::locked;

/// The undo step's label, which the History panel translates.
pub const LABEL: &str = "booharu_set_lettering_category";

/// Who chooses a category.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Chooser {
    /// A user, in the interface.
    User,
    /// A model, such as the agent, `confidence` sure of its choice, from 0
    /// to 1; none counts as sure.
    Model { confidence: Option<f32> },
}

/// What a change of category did to its blocks. A block can count in more
/// than one: a model's choice changes the label of a block whose style was
/// changed by hand, which counts in `changed` and in `kept_hand_edited`.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize, specta::Type)]
pub struct CategoryCounts {
    /// Blocks whose label, font, or record changed.
    pub changed: u32,
    /// Blocks left out because they, or a layer holding them, are locked.
    pub locked: u32,
    /// Blocks a model's choice left out because a user chose their category.
    pub kept_user_label: u32,
    /// Blocks whose font a model's choice kept because their style was
    /// changed by hand.
    pub kept_hand_edited: u32,
    /// Blocks without a translation, which take only their label.
    pub untranslated: u32,
}

/// A block's lettering category as the interface shows it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, specta::Type)]
pub struct BlockLettering {
    pub block: LayerId,
    /// The category it's lettered in.
    pub category: LetteringCategory,
    /// Why it has that category.
    pub reason: CategoryReason,
    /// Its label, who chose it and how sure a model was; none when it has
    /// none, or one that doesn't read.
    pub label: Option<Lettering>,
    /// Its balloon's shape and what decided it; none outside any balloon.
    pub shape: Option<BlockShape>,
    /// Whether its translation's text or style was changed by hand.
    pub hand_edited: bool,
}

/// The edit setting the category of each of `blocks` in `project` to
/// `category`, or to Automatic for none, as `chooser` chooses, under
/// `settings` and `font`, the default font, with what it did; none when
/// every block is left out. A layer that isn't a block fails.
pub fn set_category(
    project: &Project,
    blocks: &[LayerId],
    category: Option<LetteringCategory>,
    chooser: Chooser,
    settings: &LetteringSettings,
    font: Option<&str>,
) -> Result<(Option<Edit>, CategoryCounts)> {
    let mut counts = CategoryCounts::default();
    let mut ops = Vec::new();
    let mut seen = BTreeSet::new();
    for &id in blocks.iter().filter(|id| seen.insert(**id)) {
        let layer = project.layer(id)?;
        let Content::Block(block) = &layer.content else {
            bail!("layer {id} isn't a block");
        };
        if locked(project, layer)? {
            counts.locked += 1;
            continue;
        }
        let held = attrs::get::<Lettering>(layer);
        let users = held
            .as_ref()
            .is_some_and(|label| label.source == LetteringSource::User);
        let model = matches!(chooser, Chooser::Model { .. });
        if model && users {
            counts.kept_user_label += 1;
            continue;
        }
        let label = category.map(|category| match chooser {
            Chooser::User => Lettering {
                category,
                source: LetteringSource::User,
                confidence: None,
            },
            Chooser::Model { confidence } => Lettering {
                category,
                source: LetteringSource::Model,
                confidence,
            },
        });
        let mut changed = false;
        let raw = |label: Option<&Lettering>| label.map(serde_json::to_value).transpose();
        let before = layer
            .attributes
            .get(attrs::NAMESPACE)
            .and_then(|keys| keys.get(<Lettering as attrs::Attr>::KEY))
            .cloned();
        changed |= before != raw(label.as_ref())?;
        ops.push(match &label {
            Some(label) => attrs::set(id, label)?,
            None => attrs::clear::<Lettering>(id),
        });

        let Some(translation) = &block.translation else {
            counts.untranslated += 1;
            counts.changed += u32::from(changed);
            continue;
        };
        if !settings.auto {
            counts.changed += u32::from(changed);
            continue;
        }
        if model && edited(layer).style {
            counts.kept_hand_edited += 1;
            counts.changed += u32::from(changed);
            continue;
        }
        let (page, _) = project.parent(id)?;
        let lettered = lettering::effective_with(page, id, label.as_ref(), settings)
            .context("a block has a category")?;
        let patch = styles::category_chars(settings, font, lettered.category);
        let current = &translation.style.chars;
        let after = styles::patched(current, &patch)?;
        if after != *current {
            changed = true;
            ops.push(Op::UpdateLayer {
                layer: id.into(),
                patch: json!({"translation": {"style": {"chars": patch}}}),
            });
        }
        // The values written are Booharu's from now on. A translation no
        // stage wrote keeps its other values hand edits.
        let mut record = Auto::of(layer);
        if record.style.is_none() && !styles::recorded(layer, "translation") {
            record.style = Some(AutoStyle::default());
        }
        let written = patch.keys().cloned().collect();
        let record = record.restyled(&after, &written);
        if attrs::get::<Auto>(layer) != Some(record.clone()) {
            changed = true;
            ops.push(attrs::set(id, &record)?);
        }
        counts.changed += u32::from(changed);
    }
    let edit = (!ops.is_empty()).then(|| Edit {
        label: LABEL.to_owned(),
        ops,
    });
    Ok((edit, counts))
}

/// The lettering of every block on `page` in `project`, in reading order,
/// under `settings`.
pub fn info(
    project: &Project,
    page: PageId,
    settings: &LetteringSettings,
) -> Result<Vec<BlockLettering>> {
    let found = project.page(page)?;
    project
        .blocks(page)?
        .into_iter()
        .map(|(id, _)| lettered(found, project.layer(id)?, settings))
        .collect()
}

/// The lettering of `blocks` in `project`, each once in the order named,
/// under `settings`, as [`info`] answers a page's. A layer that isn't a
/// block fails.
pub fn blocks(
    project: &Project,
    blocks: &[LayerId],
    settings: &LetteringSettings,
) -> Result<Vec<BlockLettering>> {
    let mut seen = BTreeSet::new();
    blocks
        .iter()
        .filter(|id| seen.insert(**id))
        .map(|&id| lettered(project.parent(id)?.0, project.layer(id)?, settings))
        .collect()
}

/// The lettering of `layer`, a block on `page`, under `settings`.
fn lettered(page: &Page, layer: &Layer, settings: &LetteringSettings) -> Result<BlockLettering> {
    let label = attrs::get::<Lettering>(layer);
    let effective = lettering::effective_with(page, layer.id, label.as_ref(), settings)
        .with_context(|| format!("layer {} isn't a block", layer.id))?;
    let edited = edited(layer);
    Ok(BlockLettering {
        block: layer.id,
        category: effective.category,
        reason: effective.reason,
        label,
        shape: shape::block_shape(page, layer.id),
        hand_edited: edited.text || edited.style,
    })
}

/// Where a category's font comes from.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FontSource {
    /// Its own entry in `[booharu-lettering] categories`, whose family, when
    /// it has none, is Normal Dialogue's, else the default font.
    Own,
    /// Normal Dialogue's entry, as it has none of its own.
    NormalDialogue,
    /// Neither: the default look's font, a copy of the default font, neither
    /// bold nor italic.
    Default,
}

/// The font a category letters its text in.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ResolvedFont {
    pub category: LetteringCategory,
    /// Its English name.
    pub name: &'static str,
    /// The family; none without a default font, for Koharu's font chain.
    pub family: Option<String>,
    /// Whether it's bold, at weight 700.
    pub bold: bool,
    pub italic: bool,
    pub from: FontSource,
}

/// The font each category letters its text in under `settings` and `font`,
/// the default font, in Settings' order, as a block in it is lettered while
/// Pick fonts automatically is on, with where the font comes from.
pub fn fonts(settings: &LetteringSettings, font: Option<&str>) -> Vec<ResolvedFont> {
    LetteringCategory::ALL
        .into_iter()
        .map(|category| {
            let resolved = settings.category_font(category, font);
            let from = match &resolved {
                _ if settings.categories.contains_key(&category) => FontSource::Own,
                Some(_) => FontSource::NormalDialogue,
                None => FontSource::Default,
            };
            let resolved = resolved.unwrap_or_else(|| CategoryFont {
                family: font.map(str::to_owned),
                ..CategoryFont::default()
            });
            ResolvedFont {
                category,
                name: category.name(),
                family: resolved.family,
                bold: resolved.bold,
                italic: resolved.italic,
                from,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use booharu_core::shape::BubbleShape;
    use koharu_scene::{CharStyle, Created, LanguageIdentifier, Session, Span, Text};
    use serde_json::Value;
    use tempfile::TempDir;

    use super::*;

    /// The default font.
    const FONT: &str = "CCWildWords";

    /// The look with Pick fonts automatically on, Normal Dialogue in Comic
    /// Neue, and Shout in a bold Badaboom BB.
    fn lettered() -> LetteringSettings {
        let font = |family: &str, bold| CategoryFont {
            family: Some(family.to_owned()),
            bold,
            italic: false,
        };
        LetteringSettings {
            auto: true,
            categories: BTreeMap::from([
                (LetteringCategory::NormalDialogue, font("Comic Neue", false)),
                (LetteringCategory::Shout, font("Badaboom BB", true)),
            ]),
            ..LetteringSettings::default()
        }
    }

    /// A page 400 by 400 with a round balloon holding the block `a` by its
    /// centre, the blocks `b` and `c` outside it, the block `d` inside the
    /// group `g`, and the block `e`, each found and read by Koharu's stages,
    /// at size 32. All but `c` are translated, as the Translation stage
    /// translates with the switch off, and then `g` and `e` are locked.
    struct Fixture {
        _dir: TempDir,
        session: Session,
        layers: BTreeMap<String, LayerId>,
        page: PageId,
    }

    impl Fixture {
        fn new() -> Result<Self> {
            let dir = tempfile::tempdir()?;
            let mut session = Session::create(dir.path().join("test.khr"))?;
            let block = |x: f64, y: f64| {
                json!({"kind": "block", "source": {
                    "frame": {"box": {"bounds": {"x": x, "y": y, "width": 60.0, "height": 60.0}}},
                    "content": "どうして",
                    "style": {"chars": {"size": 32.0}},
                }, "attributes": {"c2pa": {
                    "detection": {"action": "c2pa.created"},
                    "ocr": {"action": "c2pa.created"},
                }}})
            };
            let balloon = "M10 100 C10 20 190 20 190 100 C190 180 10 180 10 100 Z";
            let ops = [
                json!({"op": "add_page", "id": "$p", "index": 0, "size": {"width": 400, "height": 400}}),
                json!({"op": "add_layer", "id": "$balloon", "page": "$p", "index": 0,
                    "layer": {"kind": "balloon", "outline": balloon}}),
                json!({"op": "add_layer", "id": "$a", "page": "$p", "index": 1, "layer": block(70.0, 70.0)}),
                json!({"op": "add_layer", "id": "$b", "page": "$p", "index": 2, "layer": block(250.0, 20.0)}),
                json!({"op": "add_layer", "id": "$c", "page": "$p", "index": 3, "layer": block(250.0, 200.0)}),
                json!({"op": "add_layer", "id": "$g", "page": "$p", "index": 4, "layer": {"kind": "group"}}),
                json!({"op": "add_layer", "id": "$d", "page": "$p", "parent": "$g", "index": 0, "layer": block(20.0, 250.0)}),
                json!({"op": "add_layer", "id": "$e", "page": "$p", "index": 5, "layer": block(150.0, 300.0)}),
            ];
            let change = session.apply(Edit {
                label: "test".to_owned(),
                ops: ops
                    .into_iter()
                    .map(serde_json::from_value)
                    .collect::<Result<_, _>>()?,
            })?;
            let mut layers = BTreeMap::new();
            for created in &change.created {
                if let Created::Layer {
                    id,
                    label: Some(label),
                } = created
                {
                    layers.insert(label.clone(), *id);
                }
            }
            let mut fixture = Self {
                _dir: dir,
                session,
                layers,
                page: change.pages.added[0],
            };
            fixture.translate(&["a", "b", "d", "e"])?;
            let locks = ["g", "e"].map(|name| Op::UpdateLayer {
                layer: fixture.id(name).into(),
                patch: json!({"locked": true}),
            });
            fixture.edit(locks.into())?;
            Ok(fixture)
        }

        fn id(&self, name: &str) -> LayerId {
            self.layers[name]
        }

        fn layer(&self, name: &str) -> Result<Layer> {
            Ok(self.session.project().layer(self.id(name))?.clone())
        }

        fn translation(&self, name: &str) -> Result<Text> {
            match self.layer(name)?.content {
                Content::Block(block) => block.translation.context("a translation"),
                _ => bail!("{name} isn't a block"),
            }
        }

        /// The family, weight, and italic of `name`'s translation.
        fn font(&self, name: &str) -> Result<(Option<String>, Option<u16>, Option<bool>)> {
            let chars = self.translation(name)?.style.chars;
            Ok((chars.family, chars.weight, chars.italic))
        }

        fn label(&self, name: &str) -> Result<Option<Lettering>> {
            Ok(attrs::get(&self.layer(name)?))
        }

        fn edit(&mut self, ops: Vec<Op>) -> Result<()> {
            self.session.apply(Edit {
                label: "test".to_owned(),
                ops,
            })?;
            Ok(())
        }

        /// Translates `names` as Koharu's Translation stage does, with
        /// Booharu's hook and the switch off.
        fn translate(&mut self, names: &[&str]) -> Result<()> {
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
                    &LetteringSettings::default(),
                    Some(FONT),
                )?);
                ops.push(Op::SetAttribute {
                    layer: block.into(),
                    namespace: "c2pa".to_owned(),
                    key: "translation".to_owned(),
                    value: Some(json!({"action": "c2pa.translated"})),
                });
            }
            self.edit(ops)
        }

        /// Sets the category of `names` as `chooser` chooses, under
        /// `settings`, and answers what it did.
        fn set(
            &mut self,
            names: &[&str],
            category: Option<LetteringCategory>,
            chooser: Chooser,
            settings: &LetteringSettings,
        ) -> Result<CategoryCounts> {
            let blocks: Vec<LayerId> = names.iter().map(|name| self.id(name)).collect();
            let (edit, counts) = set_category(
                &self.session.project(),
                &blocks,
                category,
                chooser,
                settings,
                Some(FONT),
            )?;
            if let Some(edit) = edit {
                assert_eq!(edit.label, LABEL);
                self.session.apply(edit)?;
            }
            Ok(counts)
        }
    }

    fn user(category: LetteringCategory) -> Option<Lettering> {
        Some(Lettering {
            category,
            source: LetteringSource::User,
            confidence: None,
        })
    }

    fn font(family: &str, weight: Option<u16>) -> (Option<String>, Option<u16>, Option<bool>) {
        (Some(family.to_owned()), weight, None)
    }

    #[test]
    fn one_undo_brings_back_the_label_and_the_font_together() -> Result<()> {
        let mut fixture = Fixture::new()?;
        let before = fixture.session.project();
        let steps = fixture.session.history().steps.len();

        let counts = fixture.set(
            &["a", "b"],
            Some(LetteringCategory::Shout),
            Chooser::User,
            &lettered(),
        )?;
        assert_eq!(
            counts,
            CategoryCounts {
                changed: 2,
                ..CategoryCounts::default()
            }
        );
        assert_eq!(fixture.session.history().steps.len(), steps + 1);
        assert_eq!(
            fixture.session.history().steps.last().map(String::as_str),
            Some(LABEL)
        );
        for name in ["a", "b"] {
            assert_eq!(fixture.label(name)?, user(LetteringCategory::Shout));
            assert_eq!(fixture.font(name)?, font("Badaboom BB", Some(700)));
            // Booharu's font, recorded as automatic.
            assert!(!edited(&fixture.layer(name)?).style, "{name}");
        }

        fixture.session.undo()?;
        assert!(fixture.session.project() == before);
        assert_eq!(fixture.label("a")?, None);
        assert_eq!(fixture.font("a")?, font(FONT, None));
        Ok(())
    }

    #[test]
    fn picking_the_same_category_again_applies_a_font_changed_since() -> Result<()> {
        let mut fixture = Fixture::new()?;
        let mut settings = lettered();
        fixture.set(
            &["a"],
            Some(LetteringCategory::Shout),
            Chooser::User,
            &settings,
        )?;
        let steps = fixture.session.history().steps.len();

        // The same again changes nothing, and adds no undo step.
        let again = fixture.set(
            &["a"],
            Some(LetteringCategory::Shout),
            Chooser::User,
            &settings,
        )?;
        assert_eq!(again, CategoryCounts::default());
        assert_eq!(fixture.session.history().steps.len(), steps);

        // Shout's font changed in Settings: picking it again applies it.
        settings.categories.insert(
            LetteringCategory::Shout,
            CategoryFont {
                family: Some("Another".to_owned()),
                bold: false,
                italic: true,
            },
        );
        let again = fixture.set(
            &["a"],
            Some(LetteringCategory::Shout),
            Chooser::User,
            &settings,
        )?;
        assert_eq!(again.changed, 1);
        assert_eq!(
            fixture.font("a")?,
            (Some("Another".to_owned()), None, Some(true))
        );
        assert_eq!(fixture.session.history().steps.len(), steps + 1);
        Ok(())
    }

    #[test]
    fn automatic_clears_a_users_label_and_letters_the_block_as_its_rules_say() -> Result<()> {
        let mut fixture = Fixture::new()?;
        let settings = lettered();
        fixture.set(
            &["a", "b"],
            Some(LetteringCategory::Shout),
            Chooser::User,
            &settings,
        )?;

        let counts = fixture.set(&["a", "b"], None, Chooser::User, &settings)?;
        assert_eq!(counts.changed, 2);
        // a is in a round balloon, Normal Dialogue's; b floats, with no
        // font of its own, so Normal Dialogue's too.
        for name in ["a", "b"] {
            assert_eq!(fixture.label(name)?, None);
            assert_eq!(fixture.font(name)?, font("Comic Neue", None));
        }
        let info = info(&fixture.session.project(), fixture.page, &settings)?;
        let a = info.iter().find(|found| found.block == fixture.id("a"));
        assert_eq!(
            a.map(|found| (found.category, found.reason)),
            Some((LetteringCategory::NormalDialogue, CategoryReason::Shape))
        );
        Ok(())
    }

    #[test]
    fn a_models_choice_never_replaces_a_users_label_nor_a_hand_changed_font() -> Result<()> {
        let mut fixture = Fixture::new()?;
        let settings = lettered();
        let model = Chooser::Model {
            confidence: Some(0.9),
        };
        // a labelled by the user; b's font changed by hand.
        fixture.set(
            &["a"],
            Some(LetteringCategory::Whisper),
            Chooser::User,
            &settings,
        )?;
        let b = fixture.id("b");
        fixture.edit(vec![Op::UpdateLayer {
            layer: b.into(),
            patch: json!({"translation": {"style": {"chars": {"family": "Comic Sans"}}}}),
        }])?;
        let a = fixture.layer("a")?;

        let counts = fixture.set(
            &["a", "b"],
            Some(LetteringCategory::Shout),
            model,
            &settings,
        )?;
        assert_eq!(
            counts,
            CategoryCounts {
                changed: 1,
                kept_user_label: 1,
                kept_hand_edited: 1,
                ..CategoryCounts::default()
            }
        );
        assert!(fixture.layer("a")? == a);
        // b takes the model's label, but keeps its font.
        assert_eq!(
            fixture.label("b")?,
            Some(Lettering {
                category: LetteringCategory::Shout,
                source: LetteringSource::Model,
                confidence: Some(0.9),
            })
        );
        assert_eq!(fixture.font("b")?.0.as_deref(), Some("Comic Sans"));

        // Automatic from a model clears only a model's label.
        let counts = fixture.set(&["a", "b"], None, model, &settings)?;
        assert_eq!((counts.changed, counts.kept_user_label), (1, 1));
        assert_eq!(fixture.label("a")?, user(LetteringCategory::Whisper));
        assert_eq!(fixture.label("b")?, None);

        // A user's pick restyles even the hand-changed font.
        fixture.set(
            &["b"],
            Some(LetteringCategory::Shout),
            Chooser::User,
            &settings,
        )?;
        assert_eq!(fixture.font("b")?, font("Badaboom BB", Some(700)));
        Ok(())
    }

    #[test]
    fn locked_blocks_are_left_out_and_counted() -> Result<()> {
        let mut fixture = Fixture::new()?;
        let before = fixture.session.project();
        // d is inside the locked group g, and e is locked.
        let counts = fixture.set(
            &["d", "e", "d"],
            Some(LetteringCategory::Shout),
            Chooser::User,
            &lettered(),
        )?;
        assert_eq!(
            counts,
            CategoryCounts {
                locked: 2,
                ..CategoryCounts::default()
            }
        );
        assert!(fixture.session.project() == before);

        // Beside an unlocked block, the step is made without them.
        let counts = fixture.set(
            &["a", "e"],
            Some(LetteringCategory::Shout),
            Chooser::User,
            &lettered(),
        )?;
        assert_eq!((counts.changed, counts.locked), (1, 1));
        assert_eq!(fixture.label("e")?, None);
        // A layer that isn't a block fails.
        let group = fixture.id("g");
        let refused = set_category(
            &fixture.session.project(),
            &[group],
            None,
            Chooser::User,
            &lettered(),
            Some(FONT),
        );
        assert!(refused.is_err());
        Ok(())
    }

    #[test]
    fn a_block_without_a_translation_gets_only_its_label() -> Result<()> {
        let mut fixture = Fixture::new()?;
        let counts = fixture.set(
            &["c"],
            Some(LetteringCategory::Shout),
            Chooser::User,
            &lettered(),
        )?;
        assert_eq!(
            counts,
            CategoryCounts {
                changed: 1,
                untranslated: 1,
                ..CategoryCounts::default()
            }
        );
        assert_eq!(fixture.label("c")?, user(LetteringCategory::Shout));
        let layer = fixture.layer("c")?;
        let Content::Block(block) = &layer.content else {
            bail!("c isn't a block");
        };
        assert!(block.translation.is_none());
        assert_eq!(attrs::get::<Auto>(&layer), None);
        Ok(())
    }

    #[test]
    fn with_the_switch_off_only_labels_change() -> Result<()> {
        let mut fixture = Fixture::new()?;
        let off = LetteringSettings {
            auto: false,
            ..lettered()
        };
        let a = fixture.layer("a")?;
        let counts = fixture.set(&["a"], Some(LetteringCategory::Shout), Chooser::User, &off)?;
        assert_eq!(counts.changed, 1);
        assert_eq!(fixture.label("a")?, user(LetteringCategory::Shout));
        assert_eq!(fixture.font("a")?, font(FONT, None));
        let after = fixture.layer("a")?;
        assert_eq!(attrs::get::<Auto>(&after), attrs::get::<Auto>(&a));
        Ok(())
    }

    #[test]
    fn a_category_font_leaves_the_marks_and_other_style_values() -> Result<()> {
        let mut fixture = Fixture::new()?;
        let a = fixture.id("a");
        let spans = vec![
            Span {
                insert: "Why".to_owned(),
                attributes: CharStyle {
                    italic: Some(true),
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
            spans: spans.clone(),
        }])?;
        let size = fixture.translation("a")?.style.chars.size;
        fixture.set(
            &["a"],
            Some(LetteringCategory::Shout),
            Chooser::User,
            &lettered(),
        )?;
        let translation = fixture.translation("a")?;
        assert_eq!(translation.content.0, spans);
        assert_eq!(translation.style.chars.size, size);
        Ok(())
    }

    #[test]
    fn with_no_default_font_a_font_without_a_family_takes_the_one_before_away() -> Result<()> {
        let mut fixture = Fixture::new()?;
        // No default font; Whisper is italic, with no family of its own, and
        // Normal Dialogue has no font, so Title has none either.
        let mut settings = lettered();
        settings
            .categories
            .remove(&LetteringCategory::NormalDialogue);
        settings.categories.insert(
            LetteringCategory::Whisper,
            CategoryFont {
                family: None,
                bold: false,
                italic: true,
            },
        );
        let a = fixture.id("a");
        let mut letter = |category| {
            let (edit, _) = set_category(
                &fixture.session.project(),
                &[a],
                Some(category),
                Chooser::User,
                &settings,
                None,
            )?;
            fixture.session.apply(edit.context("a change")?)?;
            assert!(!edited(&fixture.layer("a")?).style, "{category:?}");
            fixture.font("a")
        };
        assert_eq!(
            letter(LetteringCategory::Shout)?,
            font("Badaboom BB", Some(700))
        );
        // Shout's family goes, for Koharu's font chain, rather than staying
        // in Whisper's italic.
        assert_eq!(
            letter(LetteringCategory::Whisper)?,
            (None, None, Some(true))
        );
        letter(LetteringCategory::Shout)?;
        assert_eq!(letter(LetteringCategory::Title)?, (None, None, None));
        Ok(())
    }

    #[test]
    fn info_answers_each_blocks_category_reason_label_and_shape() -> Result<()> {
        let mut fixture = Fixture::new()?;
        let settings = lettered();
        fixture.set(
            &["b"],
            Some(LetteringCategory::Title),
            Chooser::User,
            &settings,
        )?;
        let b = fixture.id("b");
        fixture.edit(vec![Op::SetText {
            layer: b.into(),
            text: "Why...?".to_owned(),
        }])?;

        let info = info(&fixture.session.project(), fixture.page, &settings)?;
        let ids: Vec<LayerId> = info.iter().map(|found| found.block).collect();
        assert_eq!(ids, ["a", "b", "c", "d", "e"].map(|name| fixture.id(name)));
        let a = &info[0];
        assert_eq!(
            (a.category, a.reason, a.label.as_ref(), a.hand_edited),
            (
                LetteringCategory::NormalDialogue,
                CategoryReason::Shape,
                None,
                false
            )
        );
        let shape = a.shape.context("a's balloon")?;
        assert_eq!(shape.shape, BubbleShape::Round);
        assert!(shape.measures.rectangularity < 0.9, "{shape:?}");
        let b = &info[1];
        assert_eq!(
            (
                b.category,
                b.reason,
                b.label.clone(),
                b.shape,
                b.hand_edited
            ),
            (
                LetteringCategory::Title,
                CategoryReason::User,
                user(LetteringCategory::Title),
                None,
                true
            )
        );
        let c = &info[2];
        assert_eq!(
            (c.category, c.reason),
            (LetteringCategory::Floating, CategoryReason::Floating)
        );
        // As the interface reads it.
        let json = serde_json::to_value(b)?;
        assert_eq!(json["category"], Value::from("title"));
        assert_eq!(json["reason"], Value::from("user"));
        assert_eq!(
            json["label"],
            json!({"category": "title", "source": "user"})
        );
        Ok(())
    }

    #[test]
    fn blocks_answers_the_blocks_named_once_each_in_order() -> Result<()> {
        let fixture = Fixture::new()?;
        let settings = lettered();
        let project = fixture.session.project();
        let (a, c) = (fixture.id("a"), fixture.id("c"));
        let named = blocks(&project, &[c, a, c], &settings)?;
        let ids: Vec<LayerId> = named.iter().map(|found| found.block).collect();
        assert_eq!(ids, [c, a]);
        // As their page's answers them.
        let page = info(&project, fixture.page, &settings)?;
        assert_eq!(page.iter().find(|found| found.block == a), Some(&named[1]));
        // A layer that isn't a block fails.
        assert!(blocks(&project, &[fixture.id("g")], &settings).is_err());
        Ok(())
    }

    #[test]
    fn each_categorys_font_is_resolved_with_where_it_comes_from() {
        use LetteringCategory::{NormalDialogue, Shout, Whisper};
        let resolved = fonts(&lettered(), Some(FONT));
        let order: Vec<LetteringCategory> = resolved.iter().map(|found| found.category).collect();
        assert_eq!(order, LetteringCategory::ALL);
        let of = |category| {
            resolved
                .iter()
                .find(|found| found.category == category)
                .map(|found| {
                    (
                        found.family.as_deref(),
                        found.bold,
                        found.italic,
                        found.from,
                    )
                })
        };
        assert_eq!(
            of(Shout),
            Some((Some("Badaboom BB"), true, false, FontSource::Own))
        );
        assert_eq!(
            of(NormalDialogue),
            Some((Some("Comic Neue"), false, false, FontSource::Own))
        );
        assert_eq!(
            of(Whisper),
            Some((Some("Comic Neue"), false, false, FontSource::NormalDialogue))
        );
        assert_eq!(resolved[5].name, "Shout");
        // With none, every category takes the default font, plain; an entry
        // without a family takes it too, with its own bold and italic.
        let mut settings = LetteringSettings::default();
        assert!(fonts(&settings, Some(FONT)).iter().all(|found| {
            (
                found.family.as_deref(),
                found.bold,
                found.italic,
                found.from,
            ) == (Some(FONT), false, false, FontSource::Default)
        }));
        settings.categories.insert(
            Whisper,
            CategoryFont {
                family: None,
                bold: false,
                italic: true,
            },
        );
        let resolved = fonts(&settings, None);
        let whisper = |resolved: &[ResolvedFont]| {
            resolved
                .iter()
                .find(|found| found.category == Whisper)
                .map(|found| (found.family.clone(), found.italic, found.from))
        };
        assert_eq!(whisper(&resolved), Some((None, true, FontSource::Own)));
        // With Normal Dialogue's family, an entry without one takes it.
        settings.categories.insert(
            NormalDialogue,
            CategoryFont {
                family: Some("Comic Neue".to_owned()),
                bold: false,
                italic: false,
            },
        );
        assert_eq!(
            whisper(&fonts(&settings, Some(FONT))),
            Some((Some("Comic Neue".to_owned()), true, FontSource::Own))
        );
    }
}
