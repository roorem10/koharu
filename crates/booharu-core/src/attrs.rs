//! Booharu's data on scene layers: the registry of every key Booharu keeps
//! in a layer's attributes, all under [`NAMESPACE`], each a type whose serde
//! form is its value's shape. The renderer never reads attributes, so what it
//! draws is a scene key instead.
//!
//! - A writer builds its ops with [`set`] and [`clear`] and applies them in
//!   the one scene edit of its action, under a Booharu label; a layer the
//!   edit adds may carry its keys in its `AddLayer` instead, each the serde
//!   form of its registry type under its `KEY`, as a sound effect's subtitle
//!   is added (`sfx::subtitle_ops`), so the new layer needs no reference of
//!   its own for a second op. A locked layer fails the whole edit, so a
//!   writer leaves locked layers out and says how many it left.
//! - A reader takes a value that doesn't read as its key's shape for absent,
//!   since any agent can write these keys through the generic edit tool. That
//!   is the deliberate exception to the scene's rule that a namespace's
//!   owner fails closed on a value it can't read.
//! - The content keys, [`Lettering`], [`Auto`], [`QcOk`], and [`ReadNote`],
//!   describe a block's text, so a join or split that rewrites a block drops
//!   them; a join keeps the joined block's hand-edit record, and a split
//!   gives its halves one of their own, drawn by hand, with the kept mark,
//!   as they keep the clean-up as it was.

use anyhow::ensure;
use koharu_scene::{Layer, LayerId, Op, Ref, Rgba, Role};
use serde::{Deserialize, Serialize, de::DeserializeOwned};

use crate::lettering::LetteringCategory;

/// The attribute namespace of every Booharu key.
pub const NAMESPACE: &str = "booharu";

/// A Booharu attribute: its key under [`NAMESPACE`], with the implementing
/// type's serde form as its value's shape.
pub trait Attr: Serialize + DeserializeOwned {
    const KEY: &'static str;

    /// Whether a value of the right shape is in range too: a reader takes
    /// one that isn't for absent, and [`set`] refuses it.
    fn valid(&self) -> bool {
        true
    }
}

/// `lettering`: `{"category": category id, "source": "user" | "model",
/// "confidence"?: number, "role"?: "text" | "sfx"}`, on a block: its
/// lettering category, such as `"shout"`, who chose it, how sure a model was
/// of its choice, from 0 to 1, and the block's Koharu role before a category
/// chosen by hand changed it, so choosing Automatic restores it. A user's
/// choice is never replaced by a model's. An id Booharu doesn't know reads as
/// no label, as a bad value does.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(deny_unknown_fields)]
pub struct Lettering {
    pub category: LetteringCategory,
    pub source: LetteringSource,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f32>,
    /// The block's role before a category chosen by hand changed it, which
    /// the first such choice records and later ones keep; none when no
    /// choice changed it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<Role>,
}

impl Attr for Lettering {
    const KEY: &'static str = "lettering";

    fn valid(&self) -> bool {
        self.confidence
            .is_none_or(|confidence| (0.0..=1.0).contains(&confidence))
    }
}

/// Who chose a [`Lettering`] category.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum LetteringSource {
    User,
    Model,
}

/// `auto`: `{"origin"?: "agent", "source"?: hash, "text"?: hash, "style"?:
/// {family, weight, italic, fill, stroke, stroke_width, outer_stroke,
/// outer_stroke_width}, "kept"?: true, "drawn"?: true}`, on a block: the
/// hand-edit record, what Booharu's automatic steps last wrote there, so
/// `styles::edited` tells a hand edit, a part that now differs, from their
/// own, whether Booharu hid its clean-up to keep the original art, and
/// whether a hand cut its box where Koharu's records say detection made it.
/// Hand edits win. Each part is left out until a step writes it; later
/// stages add their own, such as the marks and the category they applied,
/// which is why it takes keys it doesn't know.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, specta::Type)]
pub struct Auto {
    /// Who chose the style: Booharu's automatic steps, or the agent on
    /// purpose, which makes it the agent's choice, kept as a hand edit is.
    #[serde(default, skip_serializing_if = "Origin::is_automatic")]
    pub origin: Origin,
    /// The source wording the Recognition stage wrote, as its
    /// `styles::text_hash`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// The translation the Translation stage wrote, as its
    /// `styles::text_hash`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// The style values an automatic step left on the translation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style: Option<AutoStyle>,
    /// Whether Booharu hid the block's clean-up to keep the original art:
    /// a sound effect a run skips, or one given a subtitle (`sfx`). A
    /// translation, typed or written by a stage, or a line given back,
    /// drops it; a clean-up hidden with no mark was hidden by hand.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub kept: bool,
    /// Whether its box was drawn by hand though it carries the detection
    /// record it was copied with: a split's halves, whose boxes the split
    /// cut. `styles::edited` then reports it drawn, as a box drawn with the O
    /// tool is, so Detect again keeps it. Automatic steps keep it.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub drawn: bool,
}

impl Attr for Auto {
    const KEY: &'static str = "auto";

    fn valid(&self) -> bool {
        let hashed = |hash: &Option<String>| {
            hash.as_ref().is_none_or(|hash| {
                hash.len() == 16
                    && hash
                        .chars()
                        .all(|digit| matches!(digit, '0'..='9' | 'a'..='f'))
            })
        };
        hashed(&self.source) && hashed(&self.text)
    }
}

/// Who chose an [`Auto`] record's style.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum Origin {
    /// One of Booharu's automatic steps, which a later one may redo.
    #[default]
    Automatic,
    /// The agent, on purpose, as a person restyles a box by hand: a later
    /// automatic step leaves it.
    Agent,
}

impl Origin {
    fn is_automatic(&self) -> bool {
        *self == Self::Automatic
    }
}

/// The character style values an automatic step left on a translation, each
/// null when unset: the keys a hand restyle changes.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(default)]
pub struct AutoStyle {
    pub family: Option<String>,
    pub weight: Option<u16>,
    pub italic: Option<bool>,
    pub fill: Option<Rgba>,
    pub stroke: Option<Rgba>,
    pub stroke_width: Option<f64>,
    pub outer_stroke: Option<Rgba>,
    pub outer_stroke_width: Option<f64>,
}

/// `qc_ok`: `[string]`, on a block or a text layer: the quality check's
/// findings a person marked OK, each `"<check>@<fingerprint>"` (see
/// [`crate::qc`]): the check's id, such as `"overflow"`, or
/// `"glossary:<term source>"` for a glossary slip, and the layer's
/// fingerprint when it was marked, so a mark hides its finding only until
/// what the checks read on the layer changes. An entry that doesn't read
/// so, or whose fingerprint is stale, hides nothing, and the key's next write
/// drops it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(transparent)]
pub struct QcOk(pub Vec<String>);

impl Attr for QcOk {
    const KEY: &'static str = "qc_ok";
}

/// `flags`: `[{"kind": "review", "message": string}]`, on a block or a text
/// layer: what an agent's review asks a person to look at, which the quality
/// check shows as a finding of its own, never hidden, until a person
/// dismisses it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(transparent)]
pub struct Flags(pub Vec<Flag>);

impl Attr for Flags {
    const KEY: &'static str = "flags";
}

/// One of a layer's [`Flags`]: its kind, always [`Flag::REVIEW`] as Booharu
/// writes it, and what to look at.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(deny_unknown_fields)]
pub struct Flag {
    pub kind: String,
    pub message: String,
}

impl Flag {
    /// The kind of every flag Booharu writes: an agent's review.
    pub const REVIEW: &'static str = "review";

    /// An agent's review flag asking a person to look at `message`.
    pub fn review(message: impl Into<String>) -> Self {
        Self {
            kind: Self::REVIEW.to_owned(),
            message: message.into(),
        }
    }
}

/// `read_note`: string, on a block: Read chapter's note on its line.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(transparent)]
pub struct ReadNote(pub String);

impl Attr for ReadNote {
    const KEY: &'static str = "read_note";
}

/// `review_note`: string, on a block or a text layer: a reviewer's note.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(transparent)]
pub struct ReviewNote(pub String);

impl Attr for ReviewNote {
    const KEY: &'static str = "review_note";
}

/// `sfx_subtitle`: `{"block": layer id}`, on a subtitle's layer: the sound
/// effect's block it subtitles.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(deny_unknown_fields)]
pub struct SfxSubtitle {
    pub block: LayerId,
}

impl Attr for SfxSubtitle {
    const KEY: &'static str = "sfx_subtitle";
}

/// `original_scan`: `true`, on the hidden layer that keeps a page's scan as
/// it was imported, before page levels changed it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(transparent)]
pub struct OriginalScan(pub bool);

impl Attr for OriginalScan {
    const KEY: &'static str = "original_scan";

    fn valid(&self) -> bool {
        self.0
    }
}

/// `skew`: `{"x": number, "y": number}`, on a text: the FX popup's skew, in
/// degrees along each axis, kept only to show it again: what's drawn is the
/// scene's.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(deny_unknown_fields)]
pub struct Skew {
    pub x: f64,
    pub y: f64,
}

impl Attr for Skew {
    const KEY: &'static str = "skew";

    fn valid(&self) -> bool {
        self.x.is_finite() && self.y.is_finite()
    }
}

/// Every Booharu attribute a layer holds, under its key, as the project
/// overview lists them: other namespaces, unknown keys, and bad values are
/// left out.
#[derive(Clone, Debug, Default, PartialEq, Serialize, specta::Type)]
pub struct Attrs {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lettering: Option<Lettering>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auto: Option<Auto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub qc_ok: Option<QcOk>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flags: Option<Flags>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub read_note: Option<ReadNote>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub review_note: Option<ReviewNote>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sfx_subtitle: Option<SfxSubtitle>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub original_scan: Option<OriginalScan>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skew: Option<Skew>,
}

/// `layer`'s Booharu attributes.
pub fn read(layer: &Layer) -> Attrs {
    Attrs {
        lettering: get(layer),
        auto: get(layer),
        qc_ok: get(layer),
        flags: get(layer),
        read_note: get(layer),
        review_note: get(layer),
        sfx_subtitle: get(layer),
        original_scan: get(layer),
        skew: get(layer),
    }
}

/// `layer`'s `A`, or none when it holds none or one that isn't `A`.
pub fn get<A: Attr>(layer: &Layer) -> Option<A> {
    let value = layer.attributes.get(NAMESPACE)?.get(A::KEY)?;
    A::deserialize(value).ok().filter(A::valid)
}

/// The op that sets `layer`'s `A` to `value`. A value out of its key's range
/// fails, as readers would take it for absent.
pub fn set<A: Attr>(layer: impl Into<Ref>, value: &A) -> anyhow::Result<Op> {
    let json = serde_json::to_value(value)?;
    ensure!(value.valid(), "{NAMESPACE}.{} can't hold {json}", A::KEY);
    Ok(Op::SetAttribute {
        layer: layer.into(),
        namespace: NAMESPACE.to_owned(),
        key: A::KEY.to_owned(),
        value: Some(json),
    })
}

/// The op that removes `layer`'s `A`.
pub fn clear<A: Attr>(layer: impl Into<Ref>) -> Op {
    Op::SetAttribute {
        layer: layer.into(),
        namespace: NAMESPACE.to_owned(),
        key: A::KEY.to_owned(),
        value: None,
    }
}

#[cfg(test)]
mod tests {
    use koharu_scene::{Created, Edit, Session};
    use serde_json::{Value, json};

    use super::*;

    /// A project of one page holding one layer.
    fn project() -> (tempfile::TempDir, Session, LayerId) {
        let dir = tempfile::tempdir().expect("a folder");
        let mut session = Session::create(dir.path().join("test.khr")).expect("a project");
        let ops = [
            json!({"op": "add_page", "id": "$page", "index": 0, "size": {"width": 64, "height": 64}}),
            json!({"op": "add_layer", "page": "$page", "index": 0, "layer": {"kind": "raster"}}),
        ];
        let change = session
            .apply(edit(
                ops.into_iter()
                    .map(|op| serde_json::from_value(op).expect("an op"))
                    .collect(),
            ))
            .expect("a page with a layer");
        let layer = change.created.iter().find_map(|created| match created {
            Created::Layer { id, .. } => Some(*id),
            _ => None,
        });
        (dir, session, layer.expect("the layer"))
    }

    fn edit(ops: Vec<Op>) -> Edit {
        Edit {
            label: "booharu_test".to_owned(),
            ops,
        }
    }

    fn layer(session: &Session, id: LayerId) -> Layer {
        session.project().layer(id).expect("the layer").clone()
    }

    #[test]
    fn every_key_is_set_in_one_edit_and_cleared_in_another() {
        let (_dir, mut session, id) = project();
        let lettering = Lettering {
            category: LetteringCategory::Shout,
            source: LetteringSource::Model,
            confidence: Some(0.75),
            role: Some(Role::Text),
        };
        let auto = Auto {
            origin: Origin::Agent,
            source: Some("0123456789abcdef".to_owned()),
            text: None,
            style: Some(AutoStyle {
                family: Some("CCWildWords".to_owned()),
                stroke_width: Some(1.5),
                ..AutoStyle::default()
            }),
            kept: true,
            drawn: true,
        };
        let qc_ok = QcOk(vec!["overflow".to_owned(), "glossary:Shale".to_owned()]);
        let flags = Flags(vec![Flag {
            kind: "review".to_owned(),
            message: "Check the name".to_owned(),
        }]);
        let read_note = ReadNote("A pun on the name".to_owned());
        let review_note = ReviewNote("Reads well".to_owned());
        let sfx_subtitle = SfxSubtitle { block: id };
        let original_scan = OriginalScan(true);
        let skew = Skew { x: 10.0, y: 0.0 };
        let ops = [
            set(id, &lettering),
            set(id, &auto),
            set(id, &qc_ok),
            set(id, &flags),
            set(id, &read_note),
            set(id, &review_note),
            set(id, &sfx_subtitle),
            set(id, &original_scan),
            set(id, &skew),
        ];
        let ops = ops.into_iter().collect::<anyhow::Result<_>>().expect("ops");
        let steps = session.history().steps.len();

        session.apply(edit(ops)).expect("the attributes");

        assert_eq!(session.history().steps.len(), steps + 1);
        let written = layer(&session, id);
        assert_eq!(
            read(&written),
            Attrs {
                lettering: Some(lettering),
                auto: Some(auto),
                qc_ok: Some(qc_ok),
                flags: Some(flags),
                read_note: Some(read_note),
                review_note: Some(review_note),
                sfx_subtitle: Some(sfx_subtitle),
                original_scan: Some(original_scan),
                skew: Some(skew),
            }
        );
        // The overview names each value by its key.
        assert_eq!(json!(read(&written)), json!(written.attributes[NAMESPACE]));

        let ops = vec![
            clear::<Lettering>(id),
            clear::<Auto>(id),
            clear::<QcOk>(id),
            clear::<Flags>(id),
            clear::<ReadNote>(id),
            clear::<ReviewNote>(id),
            clear::<SfxSubtitle>(id),
            clear::<OriginalScan>(id),
            clear::<Skew>(id),
        ];
        session.apply(edit(ops)).expect("no attributes");
        assert!(layer(&session, id).attributes.is_empty());
    }

    #[test]
    fn a_reader_leaves_out_other_keys_and_bad_values() {
        let (_dir, mut session, id) = project();
        let raw = |namespace: &str, key: &str, value: Value| Op::SetAttribute {
            layer: id.into(),
            namespace: namespace.to_owned(),
            key: key.to_owned(),
            value: Some(value),
        };
        let ops = vec![
            raw("c2pa", "read_note", json!("not Booharu's")),
            raw(NAMESPACE, "unknown", json!(1)),
            raw(
                NAMESPACE,
                "lettering",
                json!({"category": "shout", "source": "robot"}),
            ),
            raw(NAMESPACE, "qc_ok", json!("overflow")),
            raw(NAMESPACE, "auto", json!({"text": "Hello"})),
            raw(NAMESPACE, "skew", json!({"x": 1.0})),
            raw(NAMESPACE, "original_scan", json!(false)),
            raw(NAMESPACE, "review_note", json!(5)),
            raw(NAMESPACE, "read_note", json!("A pun on the name")),
        ];
        session.apply(edit(ops)).expect("the attributes");

        assert_eq!(
            read(&layer(&session, id)),
            Attrs {
                read_note: Some(ReadNote("A pun on the name".to_owned())),
                ..Attrs::default()
            }
        );
        for value in [
            json!({"category": "shout", "source": "model", "confidence": 1.5}),
            json!({"category": "tone", "source": "user"}),
            json!({"category": "", "source": "user"}),
        ] {
            let ops = vec![raw(NAMESPACE, "lettering", value.clone())];
            session.apply(edit(ops)).expect("the attribute");
            assert_eq!(get::<Lettering>(&layer(&session, id)), None, "{value}");
        }
    }

    #[test]
    fn a_value_out_of_range_is_never_written() {
        let id = LayerId(koharu_scene::Uuid::now_v7());
        let unsure = Lettering {
            category: LetteringCategory::Shout,
            source: LetteringSource::Model,
            confidence: Some(1.5),
            role: None,
        };
        assert!(set(id, &unsure).is_err());
        assert!(set(id, &OriginalScan(false)).is_err());
        let unhashed = Auto {
            text: Some("0123456789ABCDEF".to_owned()),
            ..Auto::default()
        };
        assert!(set(id, &unhashed).is_err());
        let skew = Skew {
            x: f64::NAN,
            y: 0.0,
        };
        assert!(set(id, &skew).is_err());
    }

    #[test]
    fn the_kept_mark_is_written_only_when_set() {
        let (_dir, mut session, id) = project();
        let kept = Auto {
            kept: true,
            ..Auto::default()
        };
        assert_eq!(json!(kept), json!({"kept": true}));
        assert_eq!(json!(Auto::default()), json!({}));
        session
            .apply(edit(vec![set(id, &kept).expect("the mark")]))
            .expect("the record");
        let written = layer(&session, id);
        assert_eq!(get::<Auto>(&written), Some(kept));
        assert_eq!(written.attributes[NAMESPACE]["auto"], json!({"kept": true}));
        // A record with no mark reads as unmarked.
        let ops = vec![set(id, &Auto::default()).expect("no mark")];
        session.apply(edit(ops)).expect("the record");
        assert_eq!(
            get::<Auto>(&layer(&session, id)).map(|auto| auto.kept),
            Some(false)
        );
    }

    #[test]
    fn a_box_drawn_by_hand_is_written_only_when_set() {
        let drawn = Auto {
            drawn: true,
            ..Auto::default()
        };
        assert_eq!(json!(drawn), json!({"drawn": true}));
        assert_eq!(
            serde_json::from_value::<Auto>(json!({"drawn": true})).expect("a record"),
            drawn
        );
        assert!(
            !serde_json::from_value::<Auto>(json!({"kept": true}))
                .expect("a record")
                .drawn
        );
    }

    #[test]
    fn a_labels_role_reads_back_and_is_absent_by_default() {
        let (_dir, mut session, id) = project();
        let label = Lettering {
            category: LetteringCategory::Sfx,
            source: LetteringSource::User,
            confidence: None,
            role: Some(Role::Text),
        };
        assert_eq!(
            json!(label),
            json!({"category": "sfx", "source": "user", "role": "text"})
        );
        session
            .apply(edit(vec![set(id, &label).expect("a label")]))
            .expect("the label");
        assert_eq!(get::<Lettering>(&layer(&session, id)), Some(label));

        // A label no hand-picked category changed the role of holds none,
        // and leaves it out.
        let raw = Op::SetAttribute {
            layer: id.into(),
            namespace: NAMESPACE.to_owned(),
            key: Lettering::KEY.to_owned(),
            value: Some(json!({"category": "shout", "source": "model", "confidence": 0.75})),
        };
        session.apply(edit(vec![raw])).expect("a model's label");
        let read = get::<Lettering>(&layer(&session, id)).expect("a label");
        assert_eq!(read.role, None);
        assert_eq!(
            json!(read),
            json!({"category": "shout", "source": "model", "confidence": 0.75})
        );
        // A role Koharu doesn't have reads as no label.
        let raw = Op::SetAttribute {
            layer: id.into(),
            namespace: NAMESPACE.to_owned(),
            key: Lettering::KEY.to_owned(),
            value: Some(json!({"category": "shout", "source": "user", "role": "speech"})),
        };
        session.apply(edit(vec![raw])).expect("a bad label");
        assert_eq!(get::<Lettering>(&layer(&session, id)), None);
    }
}
