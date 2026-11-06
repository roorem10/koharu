use anyhow::Result;
use booharu_context::LetteringCategory;
use koharu_pipeline::{PipelineConfig, Stage};
use koharu_renderer::TextStyles;
use koharu_translator::{ModelConfigurationIssue, ModelSelection, Translator};
use serde::Serialize;
use specta::Type;
use tauri::State;

use super::{Error, context, output::ExportFormat, project::CurrentProject};

mod models;
use models::ModelDownload;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ReadyStatus {
    Ready,
    Warning,
    Missing,
}

/// Stable reason ids are translated by the UI and remain machine-readable for agents.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ReadyReason {
    TranslationReady,
    TranslationUnset,
    ReadingModelUnset,
    ReadingNeedsVision,
    TargetLanguageReady,
    TargetLanguageUnset,
    ProjectClosed,
    SeriesUnset,
    SeriesReady,
    FontsReady,
    FontsFallback,
    ModelsReady,
    ModelsMissing,
    ExportUnavailable,
}

#[derive(Debug, Serialize, Type)]
pub struct ReadyCheckItem {
    pub status: ReadyStatus,
    pub reason: ReadyReason,
    pub data: ReadyData,
}

#[derive(Debug, Serialize, Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ReadyData {
    Translation {
        model: ModelSelection,
        vision: bool,
        reading_model: ModelSelection,
        reading_vision: bool,
        reading_enabled: bool,
        provider_configured: bool,
        reading_provider_configured: bool,
    },
    TargetLanguage {
        language: String,
    },
    Series {
        project_name: Option<String>,
        series_id: Option<String>,
        series_name: Option<String>,
    },
    Fonts {
        categories: Vec<ReadyFont>,
    },
    Models {
        models: Vec<ReadyModel>,
    },
    Export {
        enabled: bool,
        format: Option<ExportFormat>,
        folder: Option<String>,
        available: bool,
    },
}

#[derive(Debug, Serialize, Type)]
pub struct ReadyFont {
    pub category: LetteringCategory,
    pub font: Option<String>,
    pub own_font: bool,
    pub fallback_to_dialogue: bool,
}

#[derive(Debug, Serialize, Type)]
pub struct ReadyModel {
    pub stage: Stage,
    pub name: String,
    pub downloaded: bool,
    #[specta(type = Option<f64>)]
    pub size_bytes: Option<u64>,
    #[specta(type = f64)]
    pub cached_bytes: u64,
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn get_ready_check(
    project: State<'_, CurrentProject>,
) -> std::result::Result<Vec<ReadyCheckItem>, Error> {
    Ok(inspect(&project).await?)
}

pub(crate) async fn inspect(project: &CurrentProject) -> Result<Vec<ReadyCheckItem>> {
    let pipeline = PipelineConfig::load()?.read()?.clone();
    let reading = koharu_config::load::<context::settings::ContextSettings>("chapter-context")?
        .read()?
        .clone();
    // Loading TextStyles through its migration entry point writes settings; this command is read-only.
    let styles = koharu_config::load::<TextStyles>("text-styles")?.read()?.clone();
    let (project_name, series_id, linked) = {
        let current = project.project.lock().await;
        match current.as_ref() {
            Some(project) => {
                let context = project.load_context()?;
                let linked = context::linked_series(&context)?;
                (Some(project.info().name), context.series, linked)
            }
            None => (None, None, None),
        }
    };
    let model = pipeline.translation.model.clone();
    let reading_model = reading.read_model.unwrap_or_else(|| model.clone());
    let reading_enabled = reading.read_when != context::settings::ReadWhen::Never;
    let vision = context::reading::can_read_images(&model);
    let reading_vision = context::reading::can_read_images(&reading_model);
    let translation_issue = Translator::configuration_issue(&model)?;
    let reading_issue = Translator::configuration_issue(&reading_model)?;
    let (status, reason) = translation_status(
        translation_issue.is_none(),
        reading_enabled,
        reading_issue.is_none(),
        reading_vision,
    );
    let mut items = vec![ReadyCheckItem {
        status,
        reason,
        data: ReadyData::Translation {
            model,
            vision,
            reading_model,
            reading_vision,
            reading_enabled,
            provider_configured: translation_issue != Some(ModelConfigurationIssue::Provider),
            reading_provider_configured: reading_issue != Some(ModelConfigurationIssue::Provider),
        },
    }];
    let language = pipeline.translation.target_language.tag().to_owned();
    items.push(ReadyCheckItem {
        status: if language.is_empty() {
            ReadyStatus::Missing
        } else {
            ReadyStatus::Ready
        },
        reason: if language.is_empty() {
            ReadyReason::TargetLanguageUnset
        } else {
            ReadyReason::TargetLanguageReady
        },
        data: ReadyData::TargetLanguage { language },
    });
    let (status, reason) = if project_name.is_none() {
        (ReadyStatus::Warning, ReadyReason::ProjectClosed)
    } else if linked.is_none() {
        (ReadyStatus::Warning, ReadyReason::SeriesUnset)
    } else {
        (ReadyStatus::Ready, ReadyReason::SeriesReady)
    };
    items.push(ReadyCheckItem {
        status,
        reason,
        data: ReadyData::Series {
            project_name,
            series_id,
            series_name: linked.as_ref().map(|series| series.name.clone()),
        },
    });
    let categories = font_readiness(&styles);
    let fonts_ready = categories.iter().all(|category| category.own_font);
    items.push(ReadyCheckItem {
        status: if fonts_ready {
            ReadyStatus::Ready
        } else {
            ReadyStatus::Warning
        },
        reason: if fonts_ready {
            ReadyReason::FontsReady
        } else {
            ReadyReason::FontsFallback
        },
        data: ReadyData::Fonts { categories },
    });
    let models = models::selected(&pipeline)
        .iter()
        .map(model_readiness)
        .collect::<Result<Vec<_>>>()?;
    let downloaded = models.iter().all(|model| model.downloaded);
    items.push(ReadyCheckItem {
        status: if downloaded {
            ReadyStatus::Ready
        } else {
            ReadyStatus::Missing
        },
        reason: if downloaded {
            ReadyReason::ModelsReady
        } else {
            ReadyReason::ModelsMissing
        },
        data: ReadyData::Models { models },
    });
    // This branch has manual export only. Do not report an invented automatic export preference.
    items.push(ReadyCheckItem {
        status: ReadyStatus::Ready,
        reason: ReadyReason::ExportUnavailable,
        data: ReadyData::Export {
            enabled: false,
            format: None,
            folder: None,
            available: false,
        },
    });
    Ok(items)
}

fn translation_status(
    configured: bool,
    reading_enabled: bool,
    reading_configured: bool,
    reading_vision: bool,
) -> (ReadyStatus, ReadyReason) {
    if !configured {
        (ReadyStatus::Missing, ReadyReason::TranslationUnset)
    } else if reading_enabled && !reading_configured {
        (ReadyStatus::Warning, ReadyReason::ReadingModelUnset)
    } else if reading_enabled && !reading_vision {
        (ReadyStatus::Warning, ReadyReason::ReadingNeedsVision)
    } else {
        (ReadyStatus::Ready, ReadyReason::TranslationReady)
    }
}

fn font_readiness(styles: &TextStyles) -> Vec<ReadyFont> {
    LetteringCategory::ALL
        .into_iter()
        .map(|category| {
            let style = styles.category_style(category.as_str());
            let fallback_to_dialogue = category != LetteringCategory::NormalDialogue
                && std::ptr::eq(style, &styles.shape_styles.round);
            let font = style
                .font
                .as_ref()
                .and_then(|part| part.preferred_font.as_deref())
                .filter(|font| !font.trim().is_empty())
                .map(str::to_owned);
            ReadyFont {
                category,
                own_font: !fallback_to_dialogue && font.is_some(),
                font,
                fallback_to_dialogue,
            }
        })
        .collect()
}

fn model_readiness(model: &ModelDownload) -> Result<ReadyModel> {
    let sizes = model
        .files
        .iter()
        .map(|file| file.cached_size())
        .collect::<Result<Vec<_>>>()?;
    let downloaded = sizes.iter().all(Option::is_some);
    let cached_bytes = sizes.into_iter().flatten().sum();
    Ok(ReadyModel {
        stage: model.stage,
        name: model.name.to_owned(),
        downloaded,
        // Unknown remote sizes remain unknown; a partial cache is never shown as the total.
        size_bytes: downloaded.then_some(cached_bytes),
        cached_bytes,
    })
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn download_ready_models() -> std::result::Result<(), Error> {
    Ok(download_models().await?)
}

pub(crate) async fn download_models() -> Result<()> {
    let pipeline = PipelineConfig::load()?.read()?.clone();
    for model in models::selected(&pipeline) {
        for file in model.files {
            file.resolve().await?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use koharu_renderer::{FontPart, TextStyle};

    use super::*;

    #[test]
    fn readiness_uses_the_actual_reading_override_and_respects_disabled_reading() {
        assert_eq!(
            translation_status(false, false, true, true),
            (ReadyStatus::Missing, ReadyReason::TranslationUnset)
        );
        assert_eq!(
            translation_status(true, true, false, true),
            (ReadyStatus::Warning, ReadyReason::ReadingModelUnset)
        );
        assert_eq!(
            translation_status(true, true, true, false),
            (ReadyStatus::Warning, ReadyReason::ReadingNeedsVision)
        );
        assert_eq!(
            translation_status(true, true, true, true),
            (ReadyStatus::Ready, ReadyReason::TranslationReady)
        );
        assert_eq!(
            translation_status(true, false, false, false),
            (ReadyStatus::Ready, ReadyReason::TranslationReady)
        );
    }

    #[test]
    fn fonts_distinguish_dialogue_fallback_from_nonfont_overrides() {
        let mut styles = TextStyles::default();
        styles.shape_styles.round = TextStyle {
            font: Some(FontPart {
                preferred_font: Some("Dialogue".into()),
                font_weight: None,
                font_style: None,
            }),
            ..Default::default()
        };
        styles.category_styles.whisper.fill = Some([1, 2, 3, 255]);
        styles.shape_styles.spiky = styles.shape_styles.round.clone();
        let fonts = font_readiness(&styles);
        let dialogue = fonts
            .iter()
            .find(|font| font.category == LetteringCategory::NormalDialogue)
            .unwrap();
        assert!(dialogue.own_font);
        let thoughts = fonts
            .iter()
            .find(|font| font.category == LetteringCategory::Thoughts)
            .unwrap();
        assert!(thoughts.fallback_to_dialogue);
        assert!(!thoughts.own_font);
        assert_eq!(thoughts.font.as_deref(), Some("Dialogue"));
        let whisper = fonts
            .iter()
            .find(|font| font.category == LetteringCategory::Whisper)
            .unwrap();
        assert!(!whisper.fallback_to_dialogue);
        assert!(!whisper.own_font);
        assert!(whisper.font.is_none());
        let shout = fonts
            .iter()
            .find(|font| font.category == LetteringCategory::Shout)
            .unwrap();
        assert!(shout.own_font);
        assert!(!shout.fallback_to_dialogue);
    }
}
