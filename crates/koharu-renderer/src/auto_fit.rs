//! Durable limits for automatic layout, stored in Typography's extension map.

use koharu_scene::Typography;
use serde::{Deserialize, Serialize};

use crate::{Error, Result};

const SIZE_LIMIT_KEY: &str = "dev.booharu.auto_fit_size_limit";

/// Page-pixel limits captured when typesetting is applied. Rendering and export
/// consume this same intent without consulting mutable application settings.
#[derive(Clone, Copy, Debug, PartialEq, Deserialize, Serialize)]
pub struct AutoFitSizeLimit {
    pub maximum: f32,
    pub minimum: f32,
}

impl AutoFitSizeLimit {
    fn validate(self) -> Result<()> {
        if !self.maximum.is_finite()
            || !self.minimum.is_finite()
            || self.minimum <= 0.0
            || self.maximum < self.minimum
        {
            return Err(Error::invalid("automatic font size limits are invalid"));
        }
        Ok(())
    }
}

pub fn auto_fit_size_limit(typography: &Typography) -> Result<Option<AutoFitSizeLimit>> {
    typography
        .extensions
        .get(SIZE_LIMIT_KEY)
        .map(|value| {
            let limit: AutoFitSizeLimit = serde_json::from_str(value).map_err(|error| {
                Error::invalid(format!("invalid automatic font size limit: {error}"))
            })?;
            limit.validate()?;
            Ok(limit)
        })
        .transpose()
}

pub fn set_auto_fit_size_limit(
    typography: &mut Typography,
    limit: Option<AutoFitSizeLimit>,
) -> Result<()> {
    if let Some(limit) = limit {
        limit.validate()?;
        let value = serde_json::to_string(&limit).map_err(|error| {
            Error::invalid(format!("invalid automatic font size limit: {error}"))
        })?;
        typography.extensions.insert(SIZE_LIMIT_KEY.to_owned(), value);
    } else {
        typography.extensions.remove(SIZE_LIMIT_KEY);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use koharu_scene::Origin;

    use super::*;

    fn typography() -> Typography {
        Typography {
            origin: Origin::User,
            preferred_font: None,
            font_weight: None,
            font_style: None,
            size: None,
            auto_fit: true,
            color: None,
            stroke_color: None,
            stroke_width: None,
            alignment: None,
            writing_mode: None,
            extensions: Default::default(),
            stroke_method: None,
            stroke_position: None,
            stroke_join: None,
            outer_stroke_color: None,
            outer_stroke_width: None,
            line_spacing: None,
            letter_spacing: None,
            fill_gradient: None,
        }
    }

    #[tokio::test]
    async fn changed_limits_rebuild_the_same_frame_and_crops_as_a_full_render() {
        use koharu_rasterizer::{RasterOptions, Rasterizer};
        use koharu_scene::{
            At, Authored, Geometry, PageDraft, Session, TextLayout, TextLayoutKind, Translation,
        };

        let mut session = Session::memory().await.unwrap();
        let mut ids = None;
        let patch = session
            .snapshot()
            .patch(|edit| {
                let page = edit.add_page(PageDraft::new("page", 300.0, 200.0), At::End)?;
                let content = edit.add_text_content(page, At::End)?;
                edit.set(
                    content,
                    &Translation {
                        text: Authored::user("OK.".into()),
                        language: None,
                    },
                )?;
                let layer = edit.add_text_layer(
                    page,
                    At::End,
                    content,
                    &TextLayout {
                        origin: Origin::User,
                        kind: TextLayoutKind::Paragraph,
                        angle_degrees: None,
                        warp: None,
                    },
                )?;
                edit.set(layer, &Geometry::rectangle(20.0, 20.0, 240.0, 120.0))?;
                edit.set(layer, &typography())?;
                ids = Some((page, layer));
                Ok(())
            })
            .unwrap();
        let snapshot = session.commit(patch).await.unwrap().snapshot;
        let (page, layer) = ids.unwrap();
        let renderer = crate::Renderer::default();
        let rasterizer = Rasterizer::new().unwrap();
        let raster = |frame: &crate::Frame| {
            rasterizer
                .rasterize(&frame.raster_frame().unwrap(), RasterOptions::default())
                .unwrap()
                .image
        };
        let original = renderer.render(&snapshot, page).await.unwrap();
        let mut previous = original.clone();
        for maximum in [Some(28.0), Some(18.0), None] {
            let snapshot = session.snapshot();
            let mut typography = snapshot.component::<Typography>(layer).unwrap().unwrap();
            set_auto_fit_size_limit(
                &mut typography,
                maximum.map(|maximum| AutoFitSizeLimit {
                    maximum,
                    minimum: 12.0,
                }),
            )
            .unwrap();
            let patch = snapshot.patch(|edit| edit.set(layer, &typography)).unwrap();
            let commit = session.commit(patch).await.unwrap();
            let incremental = renderer
                .update(&previous, &commit.snapshot, &commit.changes)
                .await
                .unwrap();
            let full = renderer.render(&commit.snapshot, page).await.unwrap();
            assert_eq!(
                incremental.layer(layer).unwrap().kind(),
                full.layer(layer).unwrap().kind(),
            );
            assert_eq!(raster(&incremental), raster(&full));
            assert_eq!(
                raster(&incremental.cropped(layer).unwrap().unwrap()),
                raster(&full.cropped(layer).unwrap().unwrap()),
            );
            if let Some(maximum) = maximum {
                let crate::LayerKind::Text(metadata) = incremental.layer(layer).unwrap().kind() else {
                    panic!("expected text layer");
                };
                assert_eq!(metadata.font_size, maximum);
            } else {
                assert_eq!(raster(&incremental), raster(&original));
            }
            previous = incremental;
        }
    }

    #[test]
    fn extension_roundtrip_preserves_other_intent_and_rejects_corruption() {
        let mut typography = typography();
        typography
            .extensions
            .insert("dev.example.unrelated".into(), "kept".into());
        let original = typography.clone();
        let limit = AutoFitSizeLimit {
            maximum: 40.0,
            minimum: 12.0,
        };
        set_auto_fit_size_limit(&mut typography, Some(limit)).unwrap();
        assert_eq!(auto_fit_size_limit(&typography).unwrap(), Some(limit));
        set_auto_fit_size_limit(&mut typography, None).unwrap();
        assert_eq!(typography, original);
        for value in [
            r#"{"maximum":10,"minimum":12}"#,
            r#"{"maximum":40,"minimum":0}"#,
            "invalid",
        ] {
            typography
                .extensions
                .insert(SIZE_LIMIT_KEY.into(), value.into());
            assert!(auto_fit_size_limit(&typography).is_err());
        }
        assert!(
            set_auto_fit_size_limit(
                &mut typography,
                Some(AutoFitSizeLimit {
                    maximum: f32::INFINITY,
                    minimum: 12.0,
                }),
            )
            .is_err()
        );
    }
}
