use koharu_scene::{Edit, EntityId, Origin, RunStyle, Snapshot, StyleRun, TextRuns};

pub(super) fn validate_range(text: &str, start: u32, end: u32) -> koharu_scene::Result<()> {
    if start >= end || end as usize > text.chars().count() {
        return Err(koharu_scene::Error::Invalid(
            "word range is empty or outside the text".into(),
        ));
    }
    Ok(())
}

pub(super) fn write(
    edit: &mut Edit,
    snapshot: &Snapshot,
    layer: EntityId,
    start: u32,
    end: u32,
    style: Option<&RunStyle>,
    origin: Origin,
) -> koharu_scene::Result<()> {
    let existing = snapshot
        .component::<TextRuns>(layer)?
        .map(|value| value.runs)
        .unwrap_or_default();
    let runs = patched(&existing, start, end, style);
    if runs.is_empty() {
        edit.remove::<TextRuns>(layer)
    } else {
        edit.set(layer, &TextRuns { origin, runs })
    }
}

pub(super) fn patched(
    existing: &[StyleRun],
    start: u32,
    end: u32,
    style: Option<&RunStyle>,
) -> Vec<StyleRun> {
    let mut edges = vec![start, end];
    for run in existing {
        edges.extend([run.start, run.end]);
    }
    edges.sort_unstable();
    edges.dedup();
    let mut result: Vec<StyleRun> = Vec::new();
    for pair in edges.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        let mut value = existing
            .iter()
            .find(|run| run.start <= a && b <= run.end)
            .map(|run| run.style.clone())
            .unwrap_or_default();
        if start <= a && b <= end {
            match style {
                Some(style) => koharu_translator::emphasis::overlay_style(&mut value, style),
                None => value = RunStyle::default(),
            }
        }
        if value == RunStyle::default() {
            continue;
        }
        if let Some(last) = result.last_mut()
            && last.end == a
            && last.style == value
        {
            last.end = b;
        } else {
            result.push(StyleRun {
                start: a,
                end: b,
                style: value,
            });
        }
    }
    result
}

/// Every hint claims the first matching occurrence that no earlier hint claimed.
pub(super) fn find_words(text: &str, words: &[String]) -> Vec<(u32, u32)> {
    let mut used: Vec<(u32, u32)> = Vec::new();
    for word in words {
        if word.trim().is_empty() {
            continue;
        }
        let cjk = word.chars().any(is_cjk);
        for (byte, _) in text.char_indices() {
            if !text[byte..].starts_with(word) {
                continue;
            }
            let end_byte = byte + word.len();
            if !cjk
                && ((word.chars().next().is_some_and(is_word)
                    && text[..byte].chars().next_back().is_some_and(is_word))
                    || (word.chars().next_back().is_some_and(is_word)
                        && text[end_byte..].chars().next().is_some_and(is_word)))
            {
                continue;
            }
            let start = text[..byte].chars().count() as u32;
            let end = start + word.chars().count() as u32;
            if used.iter().any(|&(a, b)| start < b && a < end) {
                continue;
            }
            used.push((start, end));
            break;
        }
    }
    used.sort_unstable();
    used
}

fn is_word(ch: char) -> bool {
    ch.is_alphanumeric()
        || ch == '_'
        || matches!(ch as u32,
        0x0300..=0x036f | 0x1ab0..=0x1aff | 0x1dc0..=0x1dff | 0x20d0..=0x20ff | 0xfe20..=0xfe2f)
}

fn is_cjk(ch: char) -> bool {
    matches!(ch as u32, 0x2e80..=0x9fff | 0xac00..=0xd7af | 0xf900..=0xfaff |
        0xff66..=0xff9f | 0x20000..=0x323af)
}
