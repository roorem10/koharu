use super::Error;
use koharu_renderer::TextStyles;

pub(crate) fn load_text_styles() -> anyhow::Result<koharu_config::Config<TextStyles>> {
    TextStyles::load(|| {
        let pipeline = koharu_pipeline::PipelineConfig::load()?;
        let color = pipeline.read()?.default_text_color;
        Ok(color)
    })
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn get_text_styles() -> Result<TextStyles, Error> {
    Ok(load_text_styles()?.read()?.clone())
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn save_text_styles(mut styles: TextStyles) -> Result<TextStyles, Error> {
    styles.validate()?;
    styles.legacy_color_migrated = true;
    let config = load_text_styles()?;
    let mut current = config.write()?;
    *current = styles.clone();
    current.save()?;
    Ok(styles)
}
