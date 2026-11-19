//! The quality check's edits, each one undo step built from the project as
//! the session holds it, which the caller applies under the same lock, as
//! `boxes`' builders are: marking a finding OK and taking the mark back,
//! the agent's flags on lines, and dismissing them. One that would change a
//! locked layer, or a layer inside a locked one, refuses with
//! [`Refusal::Locked`](crate::boxes::Refusal::Locked), but for
//! [`flag_line`], which leaves locked lines out and says which.

use std::collections::BTreeMap;

use anyhow::{Result, bail, ensure};
use booharu_core::{
    attrs::{self, Flag, Flags, QcOk},
    qc::{self as marks, GLOSSARY},
};
use koharu_scene::{Content, Edit, Layer, LayerId, Op, Project};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{CHECKS, FLAG};
use crate::{boxes::unlocked, styles::locked};

/// The undo step's label for marking a finding OK.
pub const MARK_QC_OK: &str = "booharu_mark_qc_ok";
/// The undo step's label for taking back a finding's OK mark.
pub const CLEAR_QC_OK: &str = "booharu_clear_qc_ok";
/// The undo step's label for the agent's flags on lines.
pub const FLAG_LINE: &str = "booharu_flag_line";
/// The undo step's label for dismissing a line's flags.
pub const DISMISS_FLAG: &str = "booharu_dismiss_flag";

/// The longest a flag's message may be, in characters, trimmed.
pub const MAX_FLAG_MESSAGE: usize = 500;

/// The edit marking the finding of `check` on `layer`, a block or a text,
/// OK while the layer stays as it is now, when `ok`, labelled
/// [`MARK_QC_OK`], or taking back every mark of `check` there, labelled
/// [`CLEAR_QC_OK`]; none when the layer's marks stay as they are. Either
/// drops the marks the layer no longer has, made before it changed, and
/// those that don't read. `check` is a finding's check: one of [`CHECKS`],
/// or `glossary:` and a term's source. The agent's flags aren't marked OK
/// but dismissed ([`dismiss_flag`]).
pub fn mark_qc_ok(
    project: &Project,
    layer: LayerId,
    check: &str,
    ok: bool,
) -> Result<Option<Edit>> {
    let found = checked(project, layer)?;
    known(check)?;
    unlocked(project, layer)?;
    let fingerprint = marks::fingerprint(found)?;
    let held = attrs::get::<QcOk>(found).map_or_else(Vec::new, |QcOk(entries)| entries);
    let mine = |entry: &str| {
        marks::parse(entry).is_some_and(|(marked, _)| marks::same_check(marked, check))
    };
    let mut kept: Vec<String> = Vec::new();
    for entry in &held {
        let current = marks::parse(entry).is_some_and(|(_, at)| at == fingerprint);
        let again = marks::parse(entry).is_some_and(|(marked, _)| {
            kept.iter()
                .filter_map(|kept| marks::parse(kept))
                .any(|(before, _)| marks::same_check(before, marked))
        });
        if current && !again && (ok || !mine(entry)) {
            kept.push(entry.clone());
        }
    }
    if ok && !kept.iter().any(|entry| mine(entry)) {
        kept.push(marks::entry(check, &fingerprint));
    }
    if kept == held {
        return Ok(None);
    }
    let op = if kept.is_empty() {
        attrs::clear::<QcOk>(layer)
    } else {
        attrs::set(layer, &QcOk(kept))?
    };
    Ok(Some(Edit {
        label: if ok { MARK_QC_OK } else { CLEAR_QC_OK }.to_owned(),
        ops: vec![op],
    }))
}

/// What [`flag_line`] did with each line it was given, by its layer, once
/// for each line.
#[derive(
    Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize, specta::Type, JsonSchema,
)]
pub struct Flagged {
    /// The lines flagged.
    pub flagged: Vec<LayerId>,
    /// The lines that already had a flag with that message, left as they
    /// were.
    pub duplicates: Vec<LayerId>,
    /// The lines left out because they, or a layer holding them, are
    /// locked.
    pub locked: Vec<LayerId>,
}

/// The edit flagging each of `lines`, a block or a text and a message, for
/// a person to review, as the agent marks a text layer for human review
/// with a short message, labelled [`FLAG_LINE`], with what it did; none
/// when it flags nothing. Each message is trimmed and must be 1 to
/// [`MAX_FLAG_MESSAGE`] characters, and each layer a block or a text, or
/// the whole call is refused. A locked line, or one inside a locked layer,
/// is left out, as is a flag the line already has.
pub fn flag_line(
    project: &Project,
    lines: &[(LayerId, String)],
) -> Result<(Option<Edit>, Flagged)> {
    ensure!(!lines.is_empty(), "flagging lines needs at least one line");
    let mut checked_lines = Vec::with_capacity(lines.len());
    for (layer, message) in lines {
        let found = checked(project, *layer)?;
        let message = message.trim();
        let length = message.chars().count();
        ensure!(
            (1..=MAX_FLAG_MESSAGE).contains(&length),
            "a flag's message is 1 to {MAX_FLAG_MESSAGE} characters once trimmed, and layer \
             {layer}'s is {length}"
        );
        checked_lines.push((found, message));
    }
    let mut answer = Flagged::default();
    let mut flags: BTreeMap<LayerId, Vec<Flag>> = BTreeMap::new();
    let mut changed: Vec<LayerId> = Vec::new();
    for (found, message) in checked_lines {
        if locked(project, found)? {
            answer.locked.push(found.id);
            continue;
        }
        let held = flags.entry(found.id).or_insert_with(|| {
            attrs::get::<Flags>(found).map_or_else(Vec::new, |Flags(flags)| flags)
        });
        if held.iter().any(|flag| flag.message.trim() == message) {
            answer.duplicates.push(found.id);
            continue;
        }
        held.push(Flag::review(message));
        if !changed.contains(&found.id) {
            changed.push(found.id);
        }
        answer.flagged.push(found.id);
    }
    if changed.is_empty() {
        return Ok((None, answer));
    }
    let ops = changed
        .into_iter()
        .map(|layer| attrs::set(layer, &Flags(flags.remove(&layer).unwrap_or_default())))
        .collect::<Result<Vec<Op>>>()?;
    Ok((
        Some(Edit {
            label: FLAG_LINE.to_owned(),
            ops,
        }),
        answer,
    ))
}

/// The edit dismissing the flag on `layer`, a block or a text, whose
/// message is `message`, trimmed, or every flag there with no message,
/// labelled [`DISMISS_FLAG`], with how many it dismisses; none when no flag
/// matches. The last flag's going removes the layer's `flags`.
pub fn dismiss_flag(
    project: &Project,
    layer: LayerId,
    message: Option<&str>,
) -> Result<(Option<Edit>, usize)> {
    let found = checked(project, layer)?;
    unlocked(project, layer)?;
    let held = attrs::get::<Flags>(found).map_or_else(Vec::new, |Flags(flags)| flags);
    let (gone, kept): (Vec<Flag>, Vec<Flag>) = held
        .into_iter()
        .partition(|flag| message.is_none_or(|message| flag.message.trim() == message.trim()));
    if gone.is_empty() {
        return Ok((None, 0));
    }
    let op = if kept.is_empty() {
        attrs::clear::<Flags>(layer)
    } else {
        attrs::set(layer, &Flags(kept))?
    };
    Ok((
        Some(Edit {
            label: DISMISS_FLAG.to_owned(),
            ops: vec![op],
        }),
        gone.len(),
    ))
}

/// `layer` in `project`, a block or a text, which the quality check checks;
/// any other layer fails.
fn checked(project: &Project, layer: LayerId) -> Result<&Layer> {
    let found = project.layer(layer)?;
    ensure!(
        matches!(found.content, Content::Block(_) | Content::Text(_)),
        "layer {layer} is neither a block nor a text, which are all the quality check checks"
    );
    Ok(found)
}

/// Fails unless Mark OK takes `check`: one of [`CHECKS`], or a glossary
/// slip's, `glossary:` and a term's source.
fn known(check: &str) -> Result<()> {
    if check == FLAG {
        bail!("an agent's flag isn't marked OK: dismiss it instead (booharu_dismiss_flag)");
    }
    if let Some(term) = check.strip_prefix(GLOSSARY) {
        ensure!(
            !term.trim().is_empty(),
            "a glossary slip's check is glossary: and its term's source, which this one lacks"
        );
        return Ok(());
    }
    ensure!(
        CHECKS.contains(&check),
        "the quality check has no check \"{check}\"; its checks are {}, and glossary:<term \
         source> for a glossary slip",
        CHECKS.join(", ")
    );
    Ok(())
}
