//! Mark OK's record: the entries of a layer's `qc_ok` attribute ([`QcOk`]),
//! each `<check>@<fingerprint>`, so a mark hides its finding only until the
//! box changes (T1).
//!
//! The check is the finding's id, such as `overflow`, or
//! `glossary:<term source>` for a glossary slip, whose term is matched as
//! terms are ([`same_source`]). The fingerprint is a short hash of what the
//! checks read on the layer ([`fingerprint`]): a block's source, its
//! translation whole, and whether its clean-up shows, or a text layer's
//! text. Never its attributes, which the mark's own write changes, nor its
//! name. An entry that doesn't read so, or carries a fingerprint the layer
//! no longer has, hides nothing, as a reader takes a bad value for absent,
//! and the next write of the layer's `qc_ok` drops it. 0.0.4's plain check
//! ids hide nothing.

use anyhow::{Result, anyhow, bail};
use koharu_scene::{Content, Hash, Layer};
use serde_json::json;

use crate::{attrs::QcOk, context::same_source};

/// The prefix of a glossary slip's check, before the term's source.
pub const GLOSSARY: &str = "glossary:";

/// The entry marking the finding of `check` fine while its layer's
/// fingerprint is `fingerprint`.
pub fn entry(check: &str, fingerprint: &str) -> String {
    format!("{check}@{fingerprint}")
}

/// `entry`'s check and fingerprint; none for an entry that isn't
/// `<check>@<16 lowercase hex digits>`.
pub fn parse(entry: &str) -> Option<(&str, &str)> {
    let (check, fingerprint) = entry.rsplit_once('@')?;
    (!check.is_empty() && hashed(fingerprint)).then_some((check, fingerprint))
}

/// Whether `a` and `b` name one check: the same id, or glossary slips of
/// terms with the same source.
pub fn same_check(a: &str, b: &str) -> bool {
    match (a.strip_prefix(GLOSSARY), b.strip_prefix(GLOSSARY)) {
        (Some(a), Some(b)) => same_source(a, b),
        (None, None) => a == b,
        _ => false,
    }
}

/// Whether `marks` holds an entry marking `check` fine at `fingerprint`, its
/// layer's now.
pub fn marked(marks: Option<&QcOk>, check: &str, fingerprint: &str) -> bool {
    marks.is_some_and(|QcOk(entries)| {
        entries
            .iter()
            .filter_map(|entry| parse(entry))
            .any(|(marked, at)| at == fingerprint && same_check(marked, check))
    })
}

/// The fingerprint of what the checks read on `layer`: 16 lowercase hex
/// digits of the BLAKE3 hash, as `styles::text_hash` makes them, of the
/// canonical JSON, its keys sorted, of a block's source and translation,
/// each whole, its content, style, frame, direction, language and
/// transform, and whether its clean-up shows, or of a text layer's text.
/// A layer of another kind holds nothing QC checks.
pub fn fingerprint(layer: &Layer) -> Result<String> {
    let read = match &layer.content {
        Content::Block(block) => json!({
            "source": block.source,
            "translation": block.translation,
            "cleanup_visible": block.cleanup.visible,
        }),
        Content::Text(text) => json!(text),
        _ => bail!("layer {} is neither a block nor a text", layer.id),
    };
    let bytes = serde_json_canonicalizer::to_vec(&read)
        .map_err(|error| anyhow!("layer {} has no canonical JSON: {error}", layer.id))?;
    Ok(short_hash(&bytes))
}

/// The first 8 bytes of `bytes`' BLAKE3 hash as 16 lowercase hex digits, the
/// same in every build and on every machine.
pub(crate) fn short_hash(bytes: &[u8]) -> String {
    Hash::of(bytes).as_bytes()[..8]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// Whether `text` is a short hash as [`short_hash`] writes one.
pub(crate) fn hashed(text: &str) -> bool {
    text.len() == 16
        && text
            .chars()
            .all(|digit| matches!(digit, '0'..='9' | 'a'..='f'))
}

#[cfg(test)]
mod tests {
    use koharu_scene::{Created, Edit, LayerId, Op, Session};
    use serde_json::Value;

    use super::*;
    use crate::attrs;

    /// A page holding a translated block and a text layer.
    fn page() -> (tempfile::TempDir, Session, [LayerId; 2]) {
        let dir = tempfile::tempdir().expect("a folder");
        let mut session = Session::create(dir.path().join("test.khr")).expect("a project");
        let frame = json!({"box": {"bounds": {"x": 0, "y": 0, "width": 100, "height": 40}}});
        let ops = [
            json!({"op": "add_page", "id": "$page", "index": 0,
                "size": {"width": 400, "height": 400}}),
            json!({"op": "add_layer", "id": "$block", "page": "$page", "index": 0,
                "layer": {"kind": "block", "source": {"content": "魔王です", "frame": frame},
                    "translation": {"content": "The Demon King", "frame": frame,
                        "language": "en"}}}),
            json!({"op": "add_layer", "id": "$text", "page": "$page", "index": 1,
                "layer": {"kind": "text", "content": "BOOM", "frame": frame}}),
        ];
        let change = apply(&mut session, ops.into_iter().collect());
        let named = |name: &str| {
            change
                .created
                .iter()
                .find_map(|created| match created {
                    Created::Layer {
                        id,
                        label: Some(label),
                    } if label == name => Some(*id),
                    _ => None,
                })
                .expect("a layer")
        };
        (dir, session, [named("block"), named("text")])
    }

    fn apply(session: &mut Session, ops: Vec<Value>) -> koharu_scene::Change {
        session
            .apply(Edit {
                label: "test".to_owned(),
                ops: ops
                    .into_iter()
                    .map(|op| serde_json::from_value(op).expect("an op"))
                    .collect(),
            })
            .expect("an edit")
    }

    fn print(session: &Session, id: LayerId) -> String {
        fingerprint(session.project().layer(id).expect("the layer")).expect("a fingerprint")
    }

    #[test]
    fn an_entry_reads_back_and_a_bad_one_reads_as_none() {
        let fingerprint = "0123456789abcdef";
        assert_eq!(entry("overflow", fingerprint), "overflow@0123456789abcdef");
        assert_eq!(
            parse("overflow@0123456789abcdef"),
            Some(("overflow", fingerprint))
        );
        // The last @ splits, so a term's source may hold one.
        assert_eq!(
            parse("glossary:a@b@0123456789abcdef"),
            Some(("glossary:a@b", fingerprint))
        );
        for bad in [
            "overflow",
            "overflow@",
            "@0123456789abcdef",
            "overflow@0123456789ABCDEF",
            "overflow@0123456789abcde",
            "overflow@0123456789abcdefa",
        ] {
            assert_eq!(parse(bad), None, "{bad}");
        }
    }

    #[test]
    fn a_mark_hides_only_its_own_check_at_its_fingerprint() {
        let now = "0123456789abcdef";
        let before = "fedcba9876543210";
        let marks = QcOk(vec![
            entry("overflow", now),
            entry("too_small", before),
            entry("glossary:魔 王", now),
            "missing_translation".to_owned(),
            "odd_font@nothex".to_owned(),
        ]);
        assert!(marked(Some(&marks), "overflow", now));
        assert!(!marked(Some(&marks), "overflow", before));
        assert!(!marked(Some(&marks), "too_small", now), "a stale mark");
        assert!(marked(Some(&marks), "glossary:魔王", now));
        assert!(!marked(Some(&marks), "glossary:王", now));
        assert!(
            !marked(Some(&marks), "missing_translation", now),
            "0.0.4's id"
        );
        assert!(!marked(Some(&marks), "odd_font", now), "malformed");
        assert!(!marked(None, "overflow", now));
        assert!(!same_check("glossary:overflow", "overflow"));
    }

    #[test]
    fn the_fingerprint_changes_with_the_box_and_never_with_its_attributes() {
        let (_dir, mut session, [block, text]) = page();
        let first = print(&session, block);
        assert!(hashed(&first), "{first}");
        assert_eq!(
            print(&session, block),
            first,
            "the same box, the same print"
        );
        let printed = print(&session, text);
        assert_ne!(printed, first);

        // An attribute, such as the mark's own, leaves it.
        let mark = attrs::set(block, &QcOk(vec![entry("overflow", &first)])).expect("an op");
        let named = json!({"op": "update_layer", "layer": block, "patch": {"name": "Hello"}});
        session
            .apply(Edit {
                label: "test".to_owned(),
                ops: vec![mark, serde_json::from_value::<Op>(named).expect("an op")],
            })
            .expect("a mark");
        assert_eq!(print(&session, block), first);

        // The translation's wording, its style and its frame each change it.
        apply(
            &mut session,
            vec![json!({"op": "set_text", "layer": block, "text": "The king"})],
        );
        let worded = print(&session, block);
        assert_ne!(worded, first);
        apply(
            &mut session,
            vec![json!({"op": "update_layer", "layer": block,
                "patch": {"translation": {"style": {"chars": {"size": 30}}}}})],
        );
        let styled = print(&session, block);
        assert_ne!(styled, worded);
        apply(
            &mut session,
            vec![json!({"op": "update_layer", "layer": block,
                "patch": {"translation": {"frame": {"box": {"bounds":
                    {"x": 0, "y": 0, "width": 50, "height": 40}}}}}})],
        );
        let framed = print(&session, block);
        assert_ne!(framed, styled);

        // A text layer's text too.
        apply(
            &mut session,
            vec![json!({"op": "set_text", "layer": text, "text": "BANG"})],
        );
        assert_ne!(print(&session, text), printed);
    }
}
