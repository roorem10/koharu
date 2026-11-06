use booharu_context::{
    LabelSource, LetteringCategory, LineCategory, SourceBold, SourceDifference, SourceStyle,
};
use koharu_pipeline::bubble_shape::BubbleShape;

use super::{CategoryReason, EffectiveCategory, LetteringSettings, ShoutSignal, winning_label};

pub(super) fn decide(
    label: Option<LineCategory>,
    shape: Option<BubbleShape>,
    small: bool,
    source_style: Option<SourceStyle>,
    settings: &LetteringSettings,
) -> EffectiveCategory {
    use LetteringCategory::*;
    let winner = winning_label(label, settings);
    let source_bold = source_style.is_some_and(|source| source.source_bold == SourceBold::Yes);
    let mut signals = Vec::new();
    if source_bold {
        signals.push(ShoutSignal::SourceBold);
    }
    if shape == Some(BubbleShape::Spiky) {
        signals.push(ShoutSignal::SpikyBubble);
    }
    if winner.is_some_and(|label| matches!(label.value, Shout | WeakShout)) {
        signals.push(ShoutSignal::YellingLine);
    }
    let (category, reason, bold) = if let Some(label) =
        winner.filter(|label| label.source == LabelSource::User)
    {
        (label.value, CategoryReason::User, label.value == Shout)
    } else if signals.len() >= usize::from(settings.shout_signals_needed) {
        (Shout, CategoryReason::ShoutSignals, true)
    } else if let Some(difference) = source_style
        .filter(|source| source.source_different && !source_bold)
        .and_then(|source| source.difference)
    {
        match difference {
            SourceDifference::Thin => (Whisper, CategoryReason::SourceStyle, source_bold),
            SourceDifference::Shaky => (Crying, CategoryReason::SourceStyle, source_bold),
            SourceDifference::Heavy => (Shout, CategoryReason::SourceStyle, false),
        }
    } else {
        // A yelling label and a spiky contour each contribute one vote; neither
        // bypasses the user's threshold through the old category ladder.
        let (category, reason) = if let Some(label) =
            winner.filter(|label| !matches!(label.value, Shout | WeakShout))
        {
            (label.value, CategoryReason::Model)
        } else if small {
            (SmallText, CategoryReason::SmallText)
        } else {
            (
                match shape {
                    Some(BubbleShape::Rectangle) => NarrationBox,
                    Some(BubbleShape::Round | BubbleShape::Spiky) => NormalDialogue,
                    None => Floating,
                },
                CategoryReason::BubbleShape,
            )
        };
        (
            category,
            if source_bold {
                CategoryReason::SourceStyle
            } else {
                reason
            },
            source_bold,
        )
    };
    EffectiveCategory {
        category,
        reason,
        bold,
        signals,
        source_style,
    }
}
