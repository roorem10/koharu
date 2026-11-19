//! A sound effect's subtitle (S9-g): a short line of its translation set
//! beside the effect as a text layer of its own, while the effect keeps its
//! original art, which the desktop's command and the agent's tool share.
//!
//! [`add_sfx_subtitle`] answers what adding one comes to: the subtitle the
//! effect has already, the edit adding one from the words given or from
//! the effect's own translation, or that a translation is needed first,
//! which the caller gets from a Translate run in subtitle mode. The edit
//! comes from `booharu_core::sfx::subtitle_ops`: the text layer right after
//! the effect in reading order, in the subtitle look of `[booharu-lettering]
//! subtitle`, and the effect's translation taken away with its clean-up
//! hidden and the kept mark, all as one undo step.

use anyhow::{Result, bail, ensure};
use booharu_core::{settings::LetteringSettings, sfx};
use koharu_pipeline::RunId;
use koharu_scene::{Change, Content, Created, Edit, LanguageIdentifier, LayerId, Project};
use serde::{Deserialize, Serialize};

use crate::boxes::unlocked;

/// The undo step's label for adding a sound effect's subtitle, which the
/// History panel translates.
pub const LABEL: &str = "booharu_add_sfx_subtitle";

/// The most characters a subtitle given in words may have, once trimmed.
pub const MAX_TEXT: usize = 500;

/// What adding a subtitle to a sound effect comes to.
#[derive(Clone, Debug)]
pub enum Subtitle {
    /// The effect has a subtitle already: this text layer, left as it is.
    Existing(LayerId),
    /// The edit adding the subtitle, labelled [`LABEL`], for the caller to
    /// apply; [`created`] finds the subtitle in its change.
    Edit(Edit),
    /// No words were given and the effect has no translation to subtitle it
    /// with: a Translate run in subtitle mode on the effect makes one.
    NeedsTranslation,
}

/// What the desktop's Add subtitle answers: the subtitle, the one the
/// effect had or the one just added, or else the Translate run in subtitle
/// mode that makes it.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize, specta::Type)]
pub struct SfxSubtitleAnswer {
    pub layer: Option<LayerId>,
    pub run: Option<RunId>,
}

/// What adding a subtitle to the sound effect `block` of `project` comes
/// to, under `settings` and `font`, the default font, with `target` the
/// configured target language.
///
/// It refuses a layer that isn't a block, a block that isn't a sound effect
/// (`sfx::sound_effect`), and a block that is locked or inside a locked
/// layer ([`crate::boxes::Refusal::Locked`]). An effect with a subtitle
/// answers [`Subtitle::Existing`]. Otherwise the words `text`, trimmed and
/// 1 to [`MAX_TEXT`] characters, or else the effect's own translation when
/// it isn't blank, give [`Subtitle::Edit`], in the translation's language
/// unless that is undetermined, else in `target`; with neither, the answer
/// is [`Subtitle::NeedsTranslation`].
pub fn add_sfx_subtitle(
    project: &Project,
    block: LayerId,
    text: Option<String>,
    settings: &LetteringSettings,
    font: Option<&str>,
    target: &LanguageIdentifier,
) -> Result<Subtitle> {
    let layer = project.layer(block)?;
    let Content::Block(found) = &layer.content else {
        bail!("layer {block} isn't a block");
    };
    let (page, _) = project.parent(block)?;
    ensure!(
        sfx::sound_effect(page, block, settings),
        "layer {block} isn't a sound effect"
    );
    unlocked(project, block)?;
    if let Some(existing) = sfx::subtitle_of(page, block) {
        return Ok(Subtitle::Existing(existing));
    }
    let given = match &text {
        Some(text) => {
            let text = text.trim();
            let length = text.chars().count();
            ensure!(
                (1..=MAX_TEXT).contains(&length),
                "a subtitle has 1 to {MAX_TEXT} characters, not {length}"
            );
            Some(text.to_owned())
        }
        None => None,
    };
    let translated = found
        .translation
        .as_ref()
        .map(|translation| translation.content.text().trim().to_owned())
        .filter(|words| !words.is_empty());
    let Some(words) = given.or(translated) else {
        return Ok(Subtitle::NeedsTranslation);
    };
    let language = found
        .translation
        .as_ref()
        .map(|translation| &translation.language)
        .filter(|language| language.to_string() != "und")
        .unwrap_or(target);
    let ops = sfx::subtitle_ops(page, block, &words, language, &settings.subtitle, font)?;
    Ok(Subtitle::Edit(Edit {
        label: LABEL.to_owned(),
        ops,
    }))
}

/// The subtitle a [`Subtitle::Edit`]'s change made, by the reference
/// `subtitle_ops` gives it.
pub fn created(change: &Change) -> Option<LayerId> {
    change.created.iter().find_map(|created| match created {
        Created::Layer {
            id,
            label: Some(label),
        } if label == sfx::SUBTITLE_REF => Some(*id),
        _ => None,
    })
}

#[cfg(test)]
mod tests {
    use booharu_core::{
        attrs::{self, Auto, SfxSubtitle},
        lettering::LetteringCategory,
        settings::SfxMode,
    };
    use koharu_scene::{Layer, Op, Session, Text};
    use serde_json::json;
    use tempfile::TempDir;

    use super::*;
    use crate::boxes::Refusal;

    const FONT: &str = "CCWildWords";

    /// A page 400 by 400 holding the sound effects `boom` and `bang` and the
    /// text block `talk`, the locked sound effect `frozen`, the locked group
    /// `g` holding the sound effect `inner`, and the text layer `note`, each
    /// block read as `ドン` with no translation.
    struct Fixture {
        _dir: TempDir,
        session: Session,
        layers: std::collections::BTreeMap<String, LayerId>,
    }

    impl Fixture {
        fn new() -> Result<Self> {
            let dir = tempfile::tempdir()?;
            let mut session = Session::create(dir.path().join("test.khr"))?;
            let block = |x: f64, role: &str| {
                json!({"kind": "block", "role": role, "source": {
                    "content": "ドン",
                    "frame": {"box": {"bounds": {"x": x, "y": 100.0, "width": 60.0, "height": 40.0}}},
                    "style": {"chars": {"size": 32.0}},
                }, "attributes": {"c2pa": {
                    "detection": {"action": "c2pa.created"},
                    "ocr": {"action": "c2pa.created"},
                }}})
            };
            let ops = [
                json!({"op": "add_page", "id": "$p", "index": 0, "size": {"width": 400, "height": 400}}),
                json!({"op": "add_layer", "id": "$boom", "page": "$p", "index": 0, "layer": block(10.0, "sfx")}),
                json!({"op": "add_layer", "id": "$bang", "page": "$p", "index": 1, "layer": block(80.0, "sfx")}),
                json!({"op": "add_layer", "id": "$talk", "page": "$p", "index": 2, "layer": block(150.0, "text")}),
                json!({"op": "add_layer", "id": "$frozen", "page": "$p", "index": 3, "layer": block(220.0, "sfx")}),
                json!({"op": "add_layer", "id": "$g", "page": "$p", "index": 4, "layer": {"kind": "group"}}),
                json!({"op": "add_layer", "id": "$inner", "page": "$p", "parent": "$g", "index": 0,
                    "layer": block(290.0, "sfx")}),
                json!({"op": "add_layer", "id": "$note", "page": "$p", "index": 5,
                    "layer": {"kind": "text", "content": "Note"}}),
            ];
            let change = session.apply(Edit {
                label: "test".to_owned(),
                ops: ops
                    .into_iter()
                    .map(serde_json::from_value)
                    .collect::<Result<_, _>>()?,
            })?;
            let layers = change
                .created
                .into_iter()
                .filter_map(|created| match created {
                    Created::Layer {
                        id,
                        label: Some(label),
                    } => Some((label, id)),
                    _ => None,
                })
                .collect();
            let mut fixture = Self {
                _dir: dir,
                session,
                layers,
            };
            let locks = ["frozen", "g"].map(|name| Op::UpdateLayer {
                layer: fixture.id(name).into(),
                patch: json!({"locked": true}),
            });
            fixture.edit(locks.into())?;
            Ok(fixture)
        }

        fn id(&self, name: &str) -> LayerId {
            self.layers[name]
        }

        fn layer(&self, id: LayerId) -> Result<Layer> {
            Ok(self.session.project().layer(id)?.clone())
        }

        fn edit(&mut self, ops: Vec<Op>) -> Result<()> {
            self.session.apply(Edit {
                label: "test".to_owned(),
                ops,
            })?;
            Ok(())
        }

        /// Gives the block `name` the translation `words` in `language`.
        fn translate(&mut self, name: &str, words: &str, language: &str) -> Result<()> {
            let id = self.id(name);
            self.edit(vec![
                Op::SetText {
                    layer: id.into(),
                    text: words.to_owned(),
                },
                Op::UpdateLayer {
                    layer: id.into(),
                    patch: json!({"translation": {"language": language}}),
                },
            ])
        }

        /// What adding a subtitle to `name` with `text` comes to, under
        /// `settings`, translating into French.
        fn add(
            &self,
            name: &str,
            text: Option<&str>,
            settings: &LetteringSettings,
        ) -> Result<Subtitle> {
            add_sfx_subtitle(
                &self.session.project(),
                self.id(name),
                text.map(str::to_owned),
                settings,
                Some(FONT),
                &"fr".parse()?,
            )
        }

        /// Adds a subtitle to `name` with `text`, and answers its text.
        fn subtitle(&mut self, name: &str, text: Option<&str>) -> Result<(LayerId, Text)> {
            let Subtitle::Edit(edit) = self.add(name, text, &LetteringSettings::default())? else {
                bail!("no edit");
            };
            assert_eq!(edit.label, LABEL);
            let change = self.session.apply(edit)?;
            let id = created(&change).expect("the subtitle");
            match self.layer(id)?.content {
                Content::Text(text) => Ok((id, *text)),
                other => bail!("{other:?} isn't a text"),
            }
        }
    }

    #[test]
    fn it_refuses_what_isnt_an_unlocked_sound_effect() -> Result<()> {
        let fixture = Fixture::new()?;
        let settings = LetteringSettings::default();
        for name in ["note", "g", "talk"] {
            assert!(
                fixture.add(name, Some("Boom"), &settings).is_err(),
                "{name}"
            );
        }
        for name in ["frozen", "inner"] {
            let refused = fixture.add(name, Some("Boom"), &settings).unwrap_err();
            assert!(
                matches!(refused.downcast_ref(), Some(Refusal::Locked { .. })),
                "{name}: {refused}"
            );
        }
        // Words blank once trimmed, or too many, are refused.
        assert!(fixture.add("boom", Some("  "), &settings).is_err());
        let long = "a".repeat(MAX_TEXT + 1);
        assert!(fixture.add("boom", Some(&long), &settings).is_err());
        assert!(matches!(
            fixture.add("boom", Some(&long[1..]), &settings)?,
            Subtitle::Edit(_)
        ));
        Ok(())
    }

    #[test]
    fn the_category_decides_what_is_a_sound_effect() -> Result<()> {
        let mut fixture = Fixture::new()?;
        let settings = LetteringSettings::default();
        // The user's label makes the text block a sound effect, and a label
        // of another category makes a sound effect by role text.
        let label = |category| attrs::Lettering {
            category,
            source: attrs::LetteringSource::User,
            confidence: None,
            role: None,
        };
        let (talk, bang) = (fixture.id("talk"), fixture.id("bang"));
        fixture.edit(vec![
            attrs::set(talk, &label(LetteringCategory::Sfx))?,
            attrs::set(bang, &label(LetteringCategory::Shout))?,
        ])?;
        assert!(matches!(
            fixture.add("talk", Some("Boom"), &settings)?,
            Subtitle::Edit(_)
        ));
        assert!(fixture.add("bang", Some("Boom"), &settings).is_err());
        Ok(())
    }

    #[test]
    fn given_words_win_over_the_translation_in_its_language() -> Result<()> {
        let mut fixture = Fixture::new()?;
        fixture.translate("boom", "BOOM", "en")?;
        let boom = fixture.id("boom");
        let steps = fixture.session.history().steps.len();
        let (id, text) = fixture.subtitle("boom", Some("  Thud  "))?;
        assert_eq!(fixture.session.history().steps.len(), steps + 1);
        assert_eq!(text.content.text(), "Thud");
        assert_eq!(text.language.to_string(), "en");
        let subtitle = fixture.layer(id)?;
        assert_eq!(
            attrs::get::<SfxSubtitle>(&subtitle),
            Some(SfxSubtitle { block: boom })
        );
        // The effect keeps its art: no translation, its clean-up hidden,
        // with the kept mark.
        let layer = fixture.layer(boom)?;
        let Content::Block(block) = &layer.content else {
            bail!("boom isn't a block");
        };
        assert!(block.translation.is_none());
        assert!(!block.cleanup.visible);
        assert!(Auto::of(&layer).kept);

        // Asked again, the subtitle it has answers.
        let again = fixture.add("boom", Some("Crash"), &LetteringSettings::default())?;
        assert!(matches!(again, Subtitle::Existing(existing) if existing == id));
        Ok(())
    }

    #[test]
    fn the_translation_gives_the_words_in_the_target_when_its_language_is_undetermined()
    -> Result<()> {
        let mut fixture = Fixture::new()?;
        fixture.translate("boom", "  Boom  ", "und")?;
        let (_, text) = fixture.subtitle("boom", None)?;
        assert_eq!(text.content.text(), "Boom");
        assert_eq!(text.language.to_string(), "fr");

        // Given words with no translation are in the target too.
        let (_, text) = fixture.subtitle("bang", Some("Bang"))?;
        assert_eq!(text.language.to_string(), "fr");
        Ok(())
    }

    #[test]
    fn with_no_words_and_no_translation_it_needs_one() -> Result<()> {
        let mut fixture = Fixture::new()?;
        let settings = LetteringSettings::default();
        assert!(matches!(
            fixture.add("boom", None, &settings)?,
            Subtitle::NeedsTranslation
        ));
        // A blank translation is none.
        fixture.translate("boom", "  ", "en")?;
        assert!(matches!(
            fixture.add("boom", None, &settings)?,
            Subtitle::NeedsTranslation
        ));
        // Whatever the mode: a subtitle is asked for by hand.
        let all = LetteringSettings {
            sfx: SfxMode::TranslateAll,
            ..LetteringSettings::default()
        };
        assert!(matches!(
            fixture.add("bang", None, &all)?,
            Subtitle::NeedsTranslation
        ));
        Ok(())
    }
}
