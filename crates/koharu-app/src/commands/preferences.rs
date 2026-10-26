use std::{collections::BTreeMap, fmt};

use anyhow::Result;
use koharu_pipeline::PipelineConfig;
use koharu_renderer::TypesettingConfig;
use koharu_secrets::ExposeSecret as _;
use koharu_translator::{Language, Model, Provider, ProviderConfig, ProvidersConfig};
use serde::{Deserialize, Serialize};
use specta::Type;

use super::Error;

#[derive(Clone, Debug, Serialize, Type)]
pub struct Preferences {
    pub pipeline: PipelineConfig,
    pub providers: ProviderPreferences,
    pub typesetting: TypesettingConfig,
    pub languages: Vec<LanguageChoice>,
}

impl Preferences {
    pub(crate) fn load() -> Result<Self> {
        let pipeline = PipelineConfig::load()?;
        let providers = ProvidersConfig::load()?;
        let typesetting = TypesettingConfig::load()?;
        let pipeline = pipeline.read()?;
        let providers = providers.read()?;
        let typesetting = typesetting.read()?;
        Ok(Self {
            pipeline: pipeline.clone(),
            providers: ProviderPreferences::from_config(&providers)?,
            typesetting: typesetting.clone(),
            languages: Language::ALL
                .iter()
                .map(|language| LanguageChoice {
                    tag: language.tag().to_owned(),
                    name: language.to_string(),
                })
                .collect(),
        })
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, Type)]
pub struct ProviderPreferences {
    pub entries: Vec<ProviderPreference>,
}

#[derive(Clone, Debug, Deserialize, Serialize, Type)]
pub struct ProviderPreference {
    pub name: String,
    pub config: ProviderConfig,
    pub credential: Option<CredentialInput>,
}

impl ProviderPreferences {
    fn from_config(config: &ProvidersConfig) -> Result<Self> {
        let entries = config
            .entries()
            .into_iter()
            .map(|config| {
                let provider = config.provider();
                let credential = if provider == Provider::Local {
                    None
                } else {
                    let key: &'static str = provider.into();
                    Some(CredentialInput::load(key)?)
                };
                Ok(ProviderPreference {
                    name: provider.name().to_owned(),
                    config,
                    credential,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(Self { entries })
    }

    fn into_config(self) -> Result<ProvidersConfig> {
        let mut configs = Vec::with_capacity(self.entries.len());
        let mut credentials = Vec::with_capacity(self.entries.len().saturating_sub(1));
        for entry in self.entries {
            let provider = entry.config.provider();
            match entry.credential {
                None if provider == Provider::Local => {}
                Some(credential) if provider != Provider::Local => {
                    let key: &'static str = provider.into();
                    credentials.push((key, credential));
                }
                None => anyhow::bail!("missing credential input for {provider}"),
                Some(_) => anyhow::bail!("local translation does not accept credentials"),
            }
            configs.push(entry.config);
        }
        let config = ProvidersConfig::from_entries(configs)?;
        for (key, credential) in credentials {
            credential.save(key)?;
        }
        Ok(config)
    }
}

#[derive(Clone, Default, Deserialize, Serialize, Type)]
pub struct CredentialInput {
    pub configured: bool,
    pub value: Option<String>,
    pub clear: bool,
}

impl fmt::Debug for CredentialInput {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CredentialInput")
            .field("configured", &self.configured)
            .field("value", &self.value.as_ref().map(|_| "[REDACTED]"))
            .field("clear", &self.clear)
            .finish()
    }
}

impl CredentialInput {
    fn load(key: &str) -> Result<Self> {
        Ok(Self {
            configured: koharu_secrets::get(key)?
                .is_some_and(|secret| !secret.expose_secret().trim().is_empty()),
            value: None,
            clear: false,
        })
    }

    fn save(self, key: &str) -> Result<()> {
        if self.clear {
            koharu_secrets::delete(key)?;
        } else if let Some(value) = self.value {
            if value.trim().is_empty() {
                koharu_secrets::delete(key)?;
            } else {
                koharu_secrets::set(key, &value.into())?;
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Type)]
pub struct LanguageChoice {
    pub tag: String,
    pub name: String,
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn save_preferences(
    mut pipeline: PipelineConfig,
    providers: ProviderPreferences,
    typesetting: TypesettingConfig,
) -> std::result::Result<Preferences, Error> {
    remember_pipeline_profiles(&mut pipeline);
    let providers = providers.into_config()?;
    let pipeline_config = PipelineConfig::load()?;
    let providers_config = ProvidersConfig::load()?;
    let typesetting_config = TypesettingConfig::load()?;
    {
        let mut current = pipeline_config.write()?;
        *current = pipeline;
        current.save()?;
    }
    {
        let mut current = providers_config.write()?;
        *current = providers;
        current.save()?;
    }
    {
        let mut current = typesetting_config.write()?;
        *current = typesetting;
        current.save()?;
    }
    Ok(Preferences::load()?)
}

fn remember_pipeline_profiles(config: &mut PipelineConfig) {
    let koharu_pipeline::DetectionModel::KoharuLayoutRFDetrSeg2XL(settings) = &config.detection;
    config.processor.koharu_layout_rfdetr_seg_2xl = Some(settings.clone());
    if let koharu_pipeline::InpaintingModel::Flux2Klein(settings) = &config.inpainting {
        config.processor.flux2_klein = Some(settings.clone());
    }
    if let koharu_pipeline::InpaintingModel::RoremMixed(settings) = &config.inpainting {
        config.processor.rorem_mixed = Some(settings.clone());
    }
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn get_preferences() -> std::result::Result<Preferences, Error> {
    Ok(Preferences::load()?)
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn get_translation_models() -> std::result::Result<Vec<Model>, Error> {
    Ok(koharu_translator::Translator::models().await?)
}

/// Keyboard shortcuts the user changed, by action name. Actions left out use the editor's
/// built-in defaults, so new actions added later keep their default keys.
#[derive(Default, Deserialize, Serialize)]
#[serde(default)]
struct ShortcutPreferences {
    keys: BTreeMap<String, String>,
    #[serde(default)]
    wheel_zoom: WheelZoom,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, Type, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WheelZoom {
    #[default]
    Ctrl,
    Wheel,
}

const MAX_SHORTCUTS: usize = 64;

#[tauri::command]
#[specta::specta]
pub(crate) async fn get_shortcuts() -> std::result::Result<BTreeMap<String, String>, Error> {
    let config = koharu_config::load::<ShortcutPreferences>("shortcuts")?;
    Ok(config.read()?.keys.clone())
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn get_wheel_zoom() -> std::result::Result<WheelZoom, Error> {
    let config = koharu_config::load::<ShortcutPreferences>("shortcuts")?;
    Ok(config.read()?.wheel_zoom)
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn save_wheel_zoom(
    wheel_zoom: WheelZoom,
) -> std::result::Result<WheelZoom, Error> {
    let config = koharu_config::load::<ShortcutPreferences>("shortcuts")?;
    let mut preferences = config.write()?;
    preferences.wheel_zoom = wheel_zoom;
    preferences.save()?;
    Ok(wheel_zoom)
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn save_shortcuts(
    shortcuts: BTreeMap<String, String>,
) -> std::result::Result<BTreeMap<String, String>, Error> {
    let valid = shortcuts.len() <= MAX_SHORTCUTS
        && shortcuts.iter().all(|(action, key)| {
            (1..=32).contains(&action.len())
                && action
                    .chars()
                    .all(|character| character.is_ascii_alphanumeric() || character == '_')
                && valid_shortcut_binding(key)
        });
    if !valid {
        return Err(anyhow::anyhow!("invalid keyboard shortcuts").into());
    }
    let config = koharu_config::load::<ShortcutPreferences>("shortcuts")?;
    let mut preferences = config.write()?;
    preferences.keys = shortcuts.clone();
    preferences.save()?;
    Ok(shortcuts)
}

fn valid_shortcut_binding(binding: &str) -> bool {
    if binding.is_empty() {
        return true;
    }
    if binding.chars().count() > 32 || binding != binding.to_lowercase() {
        return false;
    }
    let mut parts = binding.split('+');
    let key = parts.next_back().unwrap_or_default();
    let mut previous = None;
    for modifier in parts {
        let Some(index) = ["ctrl", "alt", "shift"]
            .iter()
            .position(|value| *value == modifier)
        else {
            return false;
        };
        if previous.is_some_and(|previous| index <= previous) {
            return false;
        }
        previous = Some(index);
    }
    let printable = key.chars().count() == 1
        && !key
            .chars()
            .any(|character| character.is_control() || character.is_whitespace());
    let named = key == "plus"
        || key == "numpadadd"
        || key == "numpadsubtract"
        || (key
            .strip_prefix("numpad")
            .is_some_and(|number| number.len() == 1 && number.as_bytes()[0].is_ascii_digit()))
        || (1..=12).any(|number| key == format!("f{number}"));
    printable || named
}

#[cfg(test)]
mod shortcut_tests {
    use super::{ShortcutPreferences, WheelZoom, valid_shortcut_binding};

    #[test]
    fn wheel_preference_defaults_without_replacing_saved_keys() {
        let mut preferences: ShortcutPreferences =
            serde_json::from_str(r#"{"keys":{"text":"alt+z"}}"#).unwrap();
        assert_eq!(preferences.wheel_zoom, WheelZoom::Ctrl);
        preferences.wheel_zoom = WheelZoom::Wheel;
        let saved = serde_json::to_value(&preferences).unwrap();
        assert_eq!(saved["keys"]["text"], "alt+z");
        assert_eq!(saved["wheel_zoom"], "wheel");
        let loaded: ShortcutPreferences = serde_json::from_value(saved).unwrap();
        assert_eq!(loaded.wheel_zoom, WheelZoom::Wheel);
    }

    #[test]
    fn validates_shortcut_bindings() {
        for binding in [
            "",
            "v",
            "-",
            "=",
            ",",
            "é",
            "ctrl+plus",
            "ctrl+shift+=",
            "alt+z",
            "ctrl+alt+shift+numpad9",
            "numpadadd",
            "ctrl+numpadsubtract",
            "f1",
            "f12",
        ] {
            assert!(valid_shortcut_binding(binding), "{binding:?}");
        }
        for binding in [
            "Ctrl+z",
            "ctrl+ctrl+z",
            "shift+ctrl+z",
            "meta+z",
            "ctrl+",
            "ctrl++",
            "abc",
            "f0",
            "f13",
            "numpad10",
            "numpadmultiply",
            "\n",
            " ",
            "delete",
            "ctrl+alt+shift+numpadsubtractextra",
        ] {
            assert!(!valid_shortcut_binding(binding), "{binding:?}");
        }
    }
}
