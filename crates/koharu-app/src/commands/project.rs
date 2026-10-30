use std::{
    collections::HashSet,
    io::Cursor,
    path::{Path, PathBuf},
};

use anyhow::{Context as _, Result, anyhow, bail};
use image::{DynamicImage, ImageFormat, RgbaImage};
use koharu_desktop::Frame;
use koharu_scene::{
    AssetInput, AssetMetadata, AssetRole, At, Authored, Commit, EntityId, EntityOrigin,
    Geometry as SceneGeometry, Group as SceneGroup, Origin, PageDraft, Point as ScenePoint,
    Presents, RasterLayer as SceneRasterLayer, RasterLayerKind, Region as SceneRegion,
    RemovePolicy, Revision, Session, Snapshot, SourceText as SceneSourceText,
    TextGroup as SceneTextGroup, TextLayout as SceneTextLayout, TextLayoutKind,
    Translation as SceneTranslation, Typography as SceneTypography, Visibility as SceneVisibility,
};
use serde::Serialize;
use specta::Type;
use tokio::sync::Mutex;

use super::{
    canvas::Point,
    editing::{GeometryUpdate, TypographyUpdate},
    import,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RasterStrokeMode {
    Paint,
    Erase,
}

#[derive(Clone, Debug, Serialize, Type)]
pub struct ProjectInfo {
    pub name: String,
    pub revision: Revision,
    pub active_page: Option<EntityId>,
    pub can_undo: bool,
    pub can_redo: bool,
}

#[derive(Clone, Debug, Serialize, Type)]
pub struct ProjectSummary {
    pub name: String,
}

#[derive(Clone, Copy, Debug, Serialize, Type)]
pub struct PageSize {
    pub width: f64,
    pub height: f64,
}

#[derive(Clone, Debug, Serialize, Type)]
pub struct PageSummary {
    pub id: EntityId,
    pub label: String,
    pub size: PageSize,
    pub source_asset: Option<String>,
    #[specta(type = f64)]
    pub layer_count: usize,
}

#[derive(Clone, Debug, Serialize, Type)]
pub struct Page {
    pub id: EntityId,
    pub label: String,
    pub size: PageSize,
    pub layers: Vec<Layer>,
    pub regions: Vec<AnalysisRegion>,
}

#[derive(Clone, Debug, Serialize, Type)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Layer {
    Group {
        id: EntityId,
        parent: Option<EntityId>,
        visibility: LayerVisibility,
        name: String,
        role: Option<GroupRole>,
    },
    Text {
        id: EntityId,
        parent: Option<EntityId>,
        geometry: Option<Geometry>,
        angle_degrees: Option<f32>,
        visibility: LayerVisibility,
        content: Box<TextContent>,
        typography: Option<Typography>,
        layout: TextLayoutKind,
        automatic_region: Option<EntityId>,
    },
    Raster {
        id: EntityId,
        parent: Option<EntityId>,
        visibility: LayerVisibility,
        image: Option<String>,
        name: String,
        kind: RasterLayerKind,
    },
    Image {
        id: EntityId,
        parent: Option<EntityId>,
        geometry: Geometry,
        visibility: LayerVisibility,
        image: String,
    },
    Artwork {
        id: EntityId,
        parent: Option<EntityId>,
        geometry: Geometry,
        visibility: LayerVisibility,
        image: String,
    },
}

#[derive(Clone, Copy, Debug, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum GroupRole {
    Text,
}

#[derive(Clone, Debug, Serialize, Type)]
pub struct TextContent {
    pub id: EntityId,
    pub source: Option<SourceText>,
    pub translation: Option<Translation>,
    pub role: Option<String>,
    pub source_region: Option<EntityId>,
}

#[derive(Clone, Debug, Serialize, Type)]
pub struct AnalysisRegion {
    pub id: EntityId,
    pub parent: Option<EntityId>,
    pub geometry: Geometry,
    pub kind: String,
    pub label: Option<String>,
}

#[derive(Clone, Debug, Serialize, Type)]
pub struct Geometry {
    pub points: Vec<Point>,
}

#[derive(Clone, Copy, Debug, Serialize, Type)]
pub struct LayerVisibility {
    pub visible: bool,
    pub opacity: f32,
}

#[derive(Clone, Debug, Serialize, Type)]
pub struct SourceText {
    pub text: String,
    pub language: Option<String>,
}

#[derive(Clone, Debug, Serialize, Type)]
pub struct Translation {
    pub text: String,
    pub language: Option<String>,
}

#[derive(Clone, Debug, serde::Deserialize, Serialize, Type)]
pub struct Typography {
    pub preferred_font: Option<String>,
    pub font_weight: Option<u16>,
    pub font_style: Option<koharu_scene::FontStyle>,
    pub size: Option<f32>,
    pub auto_fit: bool,
    pub color: Option<[u8; 4]>,
    pub stroke_color: Option<[u8; 4]>,
    pub stroke_width: Option<f32>,
    pub stroke_method: Option<koharu_scene::StrokeMethod>,
    pub stroke_position: Option<koharu_scene::StrokePosition>,
    pub stroke_join: Option<koharu_scene::StrokeJoin>,
    pub outer_stroke_color: Option<[u8; 4]>,
    pub outer_stroke_width: Option<f32>,
    pub alignment: Option<koharu_scene::TextAlignment>,
    pub writing_mode: Option<koharu_scene::WritingMode>,
}

pub(crate) struct CurrentProject {
    pub(crate) project: Mutex<Option<Project>>,
}

#[derive(Clone)]
pub(crate) struct ProjectLibrary {
    root: PathBuf,
}

impl ProjectLibrary {
    pub(crate) fn new() -> Result<Self> {
        let root = match koharu_config::preview::root()? {
            Some(root) => root.join("projects"),
            None => dirs::document_dir()
                .context("the Documents directory is unavailable")?
                .join("Koharu"),
        };
        std::fs::create_dir_all(&root)
            .with_context(|| format!("failed to create {}", root.display()))?;
        Ok(Self { root })
    }

    pub(crate) fn list(&self) -> Result<Vec<ProjectSummary>> {
        let mut projects = std::fs::read_dir(&self.root)
            .with_context(|| format!("failed to read {}", self.root.display()))?
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
            .filter_map(|entry| {
                let path = entry.path();
                let is_project_directory = path
                    .extension()
                    .and_then(|extension| extension.to_str())
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("khrproj"))
                    && (path.join("state-a.khr").is_file() || path.join("state-b.khr").is_file());
                if !is_project_directory {
                    return None;
                }
                let last_used = ["state-a.khr", "state-b.khr"]
                    .into_iter()
                    .filter_map(|file| std::fs::metadata(path.join(file)).ok()?.modified().ok())
                    .max()
                    .unwrap_or(std::time::UNIX_EPOCH);
                Some((
                    last_used,
                    ProjectSummary {
                        name: path.file_stem()?.to_str()?.to_owned(),
                    },
                ))
            })
            .collect::<Vec<_>>();
        projects.sort_unstable_by(|(left_used, left), (right_used, right)| {
            right_used
                .cmp(left_used)
                .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
        });
        Ok(projects.into_iter().map(|(_, project)| project).collect())
    }

    pub(crate) async fn create(&self, name: &str) -> Result<Project> {
        let (name, path) = self.resolve(name)?;
        Project::create(name, path).await
    }

    pub(crate) async fn open(&self, name: &str) -> Result<Project> {
        let (name, path) = self.resolve(name)?;
        Project::open(name, path).await
    }

    pub(crate) fn delete(&self, name: &str) -> Result<()> {
        let (_, path) = self.resolve(name)?;
        if !path.is_dir() {
            bail!("project {name:?} does not exist");
        }
        std::fs::remove_dir_all(&path)
            .with_context(|| format!("failed to delete {}", path.display()))
    }

    fn resolve(&self, name: &str) -> Result<(String, PathBuf)> {
        let name = validate_project_name(name)?;
        Ok((name.clone(), self.root.join(format!("{name}.khrproj"))))
    }
}

pub(crate) struct Project {
    pub(crate) session: Session,
    pub(crate) name: String,
    pub(crate) active_page: Option<EntityId>,
    pub(crate) undo: Vec<Vec<Revision>>,
    pub(crate) redo: Vec<Vec<Revision>>,
    /// The project folder on disk; `None` for in-memory test projects.
    pub(crate) directory: Option<PathBuf>,
}

impl Project {
    pub(crate) async fn create(name: String, path: PathBuf) -> Result<Self> {
        let session = Session::create(&path)
            .await
            .with_context(|| format!("failed to create {}", path.display()))?;
        Ok(Self::new(session, name).with_directory(path))
    }

    pub(crate) async fn open(name: String, path: PathBuf) -> Result<Self> {
        let session = Session::open(&path)
            .await
            .with_context(|| format!("failed to open {}", path.display()))?;
        Ok(Self::new(session, name).with_directory(path))
    }

    pub(crate) fn new(session: Session, name: String) -> Self {
        let active_page = session.snapshot().pages().next().map(|page| page.id());
        Self {
            session,
            name,
            active_page,
            undo: Vec::new(),
            redo: Vec::new(),
            directory: None,
        }
    }

    fn with_directory(mut self, directory: PathBuf) -> Self {
        self.directory = Some(directory);
        self
    }

    pub(crate) fn directory(&self) -> Option<&Path> {
        self.directory.as_deref()
    }

    /// The Booharu sidecar for this project. In-memory projects have an empty one.
    pub(crate) fn load_context(&self) -> Result<booharu_context::ProjectContext> {
        match self.directory() {
            Some(directory) => booharu_context::ProjectContext::load(directory),
            None => Ok(Default::default()),
        }
    }

    /// Changes the sidecar in one locked load-change-save step.
    pub(crate) fn update_context<R>(
        &self,
        change: impl FnOnce(&mut booharu_context::ProjectContext) -> R,
    ) -> Result<R> {
        let directory = self
            .directory()
            .context("this project has no folder for its context file")?;
        booharu_context::ProjectContext::update(directory, change)
    }

    pub(crate) fn snapshot(&self) -> Snapshot {
        self.session.snapshot()
    }

    pub(crate) fn revision(&self) -> Revision {
        self.snapshot().revision()
    }

    pub(crate) fn active_page(&self) -> Option<EntityId> {
        self.active_page
    }

    pub(super) async fn import_pages(&mut self, pages: Vec<import::Page>) -> Result<Commit> {
        let source = AssetRole::new("source")?;
        let patch = self.snapshot().patch(|edit| {
            for imported in pages {
                let page = edit.add_page(
                    PageDraft::new(
                        imported.name,
                        f64::from(imported.width),
                        f64::from(imported.height),
                    ),
                    At::End,
                )?;
                edit.set_asset(
                    page,
                    &source,
                    AssetInput::new(
                        imported.bytes,
                        imported.format.to_mime_type(),
                        AssetMetadata {
                            width: Some(imported.width),
                            height: Some(imported.height),
                            attributes: Default::default(),
                        },
                    ),
                )?;
            }
            Ok(())
        })?;
        self.commit(patch).await
    }

    pub(crate) fn select_page(&mut self, page: EntityId) -> Result<()> {
        self.snapshot().page(page)?;
        self.active_page = Some(page);
        Ok(())
    }

    pub(crate) fn reconcile_page(&mut self) {
        let snapshot = self.snapshot();
        if self
            .active_page
            .is_none_or(|page| snapshot.page(page).is_err())
        {
            self.active_page = snapshot.pages().next().map(|page| page.id());
        }
    }

    pub(crate) fn info(&self) -> ProjectInfo {
        ProjectInfo {
            name: self.name.clone(),
            revision: self.revision(),
            active_page: self.active_page,
            can_undo: !self.undo.is_empty(),
            can_redo: !self.redo.is_empty(),
        }
    }

    pub(crate) fn pages(snapshot: &Snapshot) -> Result<Vec<PageSummary>> {
        snapshot
            .pages()
            .map(|page| {
                let value = page.page()?;
                let source_asset = Self::asset_id(snapshot, page.id(), "source")?;
                let layer_count = snapshot
                    .descendants(page.id())?
                    .map(|entity| Self::is_content_layer(snapshot, entity.id()))
                    .collect::<Result<Vec<_>>>()?
                    .into_iter()
                    .filter(|present| *present)
                    .count()
                    + usize::from(source_asset.is_some());
                Ok(PageSummary {
                    id: page.id(),
                    label: value.label,
                    size: PageSize {
                        width: value.width,
                        height: value.height,
                    },
                    source_asset,
                    layer_count,
                })
            })
            .collect()
    }

    pub(crate) async fn rename_page(&mut self, page: EntityId, label: String) -> Result<Commit> {
        let snapshot = self.snapshot();
        let current = snapshot.page(page)?.page()?;
        let patch = snapshot.patch(|edit| {
            edit.set_page(page, PageDraft::new(label, current.width, current.height))
        })?;
        self.commit(patch).await
    }

    pub(crate) async fn delete_pages(&mut self, pages: Vec<EntityId>) -> Result<Commit> {
        let snapshot = self.snapshot();
        let pages = Self::unique_roots(&snapshot, pages)?;
        let patch = snapshot.patch(|edit| {
            for page in pages {
                edit.remove_entity(page, RemovePolicy::Cascade)?;
            }
            Ok(())
        })?;
        self.commit(patch).await
    }

    pub(crate) async fn move_page(&mut self, page: EntityId, index: usize) -> Result<Commit> {
        let snapshot = self.snapshot();
        let siblings = snapshot.pages().map(|page| page.id()).collect::<Vec<_>>();
        let at = Self::placement(&siblings, page, index);
        let patch = snapshot.patch(|edit| edit.move_entity(page, None, at))?;
        self.commit(patch).await
    }

    pub(crate) async fn add_point_text(
        &mut self,
        page: EntityId,
        point: Point,
    ) -> Result<(Commit, EntityId)> {
        if !point.x.is_finite() || !point.y.is_finite() {
            bail!("text position must contain finite coordinates");
        }
        let value = self.snapshot().page(page)?.page()?;
        if point.x < 0.0 || point.y < 0.0 || point.x >= value.width || point.y >= value.height {
            bail!("text position must be inside the page");
        }
        self.add_text(
            page,
            Frame {
                x: point.x as f32,
                y: point.y as f32,
                width: (value.width - point.x).clamp(1.0, 320.0) as f32,
                height: (value.height - point.y).clamp(1.0, 120.0) as f32,
                angle_degrees: 0.0,
            },
            TextLayoutKind::Point,
        )
        .await
    }

    pub(crate) async fn add_text_box(
        &mut self,
        page: EntityId,
        frame: Frame,
    ) -> Result<(Commit, EntityId)> {
        self.add_text(page, frame, TextLayoutKind::Paragraph).await
    }

    async fn add_text(
        &mut self,
        page: EntityId,
        frame: Frame,
        kind: TextLayoutKind,
    ) -> Result<(Commit, EntityId)> {
        let styles = super::styles::load_text_styles()?;
        let default_style = styles.read()?.default_style.clone();
        let snapshot = self.snapshot();
        let geometry = Self::geometry_from_frame(frame)?;
        let mut layer = None;
        let patch = snapshot.patch(|edit| {
            let content = edit.add_text_content(page, At::End)?;
            edit.set(
                content,
                &SceneSourceText {
                    text: Authored::user(String::new()),
                    language: None,
                },
            )?;
            let added_layer = edit.add_text_layer(
                page,
                At::End,
                content,
                &SceneTextLayout {
                    origin: Origin::User,
                    kind,
                    angle_degrees: Some(frame.angle_degrees),
                },
            )?;
            layer = Some(added_layer);
            edit.set(added_layer, &geometry)?;
            let mut typography = SceneTypography {
                origin: Origin::User,
                preferred_font: None,
                font_weight: None,
                font_style: None,
                size: None,
                auto_fit: true,
                color: None,
                stroke_color: None,
                stroke_width: None,
                stroke_method: Some(koharu_scene::StrokeMethod::Contour),
                stroke_position: None,
                stroke_join: None,
                outer_stroke_color: None,
                outer_stroke_width: None,
                alignment: match kind {
                    TextLayoutKind::Point => Some(koharu_scene::TextAlignment::Start),
                    TextLayoutKind::Paragraph => None,
                },
                writing_mode: None,
                extensions: Default::default(),
            };
            default_style.apply_to(&mut typography);
            edit.set(added_layer, &typography)?;
            Ok(())
        })?;
        Ok((
            self.commit(patch).await?,
            layer.expect("text layer was added while building the patch"),
        ))
    }

    pub(crate) async fn set_source_text(
        &mut self,
        layer: EntityId,
        text: String,
    ) -> Result<Commit> {
        let snapshot = self.snapshot();
        let content = Self::text_content(&snapshot, layer)?;
        let language = snapshot
            .component::<SceneSourceText>(content)?
            .and_then(|source| source.language);
        let patch = snapshot.patch(|edit| {
            edit.promote_entity_to_user(layer)?;
            edit.promote_entity_to_user(content)?;
            edit.set(
                content,
                &SceneSourceText {
                    text: Authored::user(text),
                    language,
                },
            )
        })?;
        self.commit(patch).await
    }

    pub(crate) async fn set_translation(
        &mut self,
        layer: EntityId,
        text: Option<String>,
    ) -> Result<Commit> {
        let snapshot = self.snapshot();
        let content = Self::text_content(&snapshot, layer)?;
        let language = snapshot
            .component::<SceneTranslation>(content)?
            .and_then(|translation| translation.language);
        let patch = snapshot.patch(|edit| {
            edit.promote_entity_to_user(layer)?;
            edit.promote_entity_to_user(content)?;
            match text {
                Some(text) => edit.set(
                    content,
                    &SceneTranslation {
                        text: Authored::user(text),
                        language,
                    },
                ),
                None => edit.remove::<SceneTranslation>(content),
            }
        })?;
        self.commit(patch).await
    }

    pub(crate) async fn set_typography(
        &mut self,
        updates: Vec<TypographyUpdate>,
    ) -> Result<Commit> {
        let snapshot = self.snapshot();
        let updates = updates
            .into_iter()
            .map(|update| {
                let content = Self::text_content(&snapshot, update.layer)?;
                Ok((update, content))
            })
            .collect::<Result<Vec<_>>>()?;
        let patch = snapshot.patch(|edit| {
            for (update, content) in updates {
                edit.promote_entity_to_user(update.layer)?;
                edit.promote_entity_to_user(content)?;
                edit.set(
                    update.layer,
                    &SceneTypography {
                        origin: Origin::User,
                        preferred_font: update.typography.preferred_font,
                        font_weight: update.typography.font_weight,
                        font_style: update.typography.font_style,
                        size: update.typography.size.filter(|size| *size > 0.0),
                        auto_fit: update.typography.auto_fit,
                        color: update.typography.color,
                        stroke_color: update.typography.stroke_color,
                        stroke_width: update.typography.stroke_width,
                        stroke_method: update.typography.stroke_method,
                        stroke_position: update.typography.stroke_position,
                        stroke_join: update.typography.stroke_join,
                        outer_stroke_color: update.typography.outer_stroke_color,
                        outer_stroke_width: update
                            .typography
                            .outer_stroke_width
                            .filter(|width| *width > 0.0),
                        alignment: update.typography.alignment,
                        writing_mode: update.typography.writing_mode,
                        extensions: Default::default(),
                    },
                )?;
            }
            Ok(())
        })?;
        self.commit(patch).await
    }

    pub(crate) fn style_targets(
        snapshot: &Snapshot,
        scope: super::styles::StyleScope,
    ) -> Result<Vec<EntityId>> {
        use super::styles::StyleScope;
        let mut targets = Vec::new();
        match scope {
            StyleScope::Layers(layers) => targets = layers,
            StyleScope::Page(page) => {
                targets.extend(snapshot.page(page)?.text_layers()?.map(|layer| layer.id()))
            }
            StyleScope::Project => {
                for page in snapshot.pages() {
                    targets.extend(page.text_layers()?.map(|layer| layer.id()));
                }
            }
        }
        let mut seen = HashSet::new();
        targets.retain(|layer| seen.insert(*layer));
        Ok(targets)
    }

    pub(crate) async fn apply_shape_rules(
        &mut self,
        scope: super::styles::StyleScope,
        styles: &koharu_renderer::TextStyles,
        detect_colors: bool,
        include_user: bool,
    ) -> Result<(Option<Commit>, super::styles::StyleApplyResult)> {
        use koharu_pipeline::bubble_shape::text_bubble_shape;
        styles.validate()?;
        if !styles.shape_styles.enabled {
            return Ok((None, Default::default()));
        }
        let context = self.load_context()?;
        self.apply_style(scope, false, include_user, |snapshot, layer, typography| {
            let info = text_bubble_shape(snapshot, layer)?;
            anyhow::ensure!(
                info.bubble.is_none() || info.measures.is_some(),
                "bubble polygon cannot be measured"
            );
            styles
                .default_style
                .apply_to_detected(typography, detect_colors);
            if let Some(style) =
                super::lettering::pick_style(styles, &context, snapshot, layer, &info)?
            {
                style.apply_to_detected(typography, detect_colors);
            }
            Ok(())
        })
        .await
    }

    pub(crate) async fn apply_style(
        &mut self,
        scope: super::styles::StyleScope,
        promote_user: bool,
        include_user: bool,
        mut update: impl FnMut(&Snapshot, EntityId, &mut SceneTypography) -> Result<()>,
    ) -> Result<(Option<Commit>, super::styles::StyleApplyResult)> {
        use koharu_renderer::{StyleGroup, TextStyle};
        let snapshot = self.snapshot();
        let mut result = super::styles::StyleApplyResult::default();
        let mut updates = Vec::new();
        let keep_manual = !include_user && !matches!(scope, super::styles::StyleScope::Layers(_));
        for layer in Self::style_targets(&snapshot, scope)? {
            if keep_manual
                && snapshot
                    .component::<SceneTypography>(layer)?
                    .is_some_and(|t| t.origin == Origin::User)
            {
                result.kept_manual += 1;
                continue;
            }
            let prepared = (|| -> Result<_> {
                let content = Self::text_content(&snapshot, layer)?;
                snapshot.text_content(content)?;
                let original = snapshot
                    .component::<SceneTypography>(layer)?
                    .context("text layer has no typography")?;
                let mut typography = original.clone();
                update(&snapshot, layer, &mut typography)?;
                TextStyle::capture(
                    &typography,
                    &[
                        StyleGroup::Font,
                        StyleGroup::Size,
                        StyleGroup::Fill,
                        StyleGroup::Outline,
                        StyleGroup::OuterOutline,
                        StyleGroup::Alignment,
                        StyleGroup::WritingMode,
                    ],
                )
                .validate()?;
                Ok((content, original, typography))
            })();
            let Ok((content, original, mut typography)) = prepared else {
                result.skipped += 1;
                continue;
            };
            if original == typography {
                continue;
            }
            if promote_user {
                typography.origin = Origin::User;
            }
            updates.push((layer, content, typography));
        }
        if updates.is_empty() {
            return Ok((None, result));
        }
        result.changed = u32::try_from(updates.len())?;
        let patch = snapshot.patch(|edit| {
            for (layer, content, typography) in updates {
                if promote_user {
                    edit.promote_entity_to_user(layer)?;
                    edit.promote_entity_to_user(content)?;
                    edit.set(layer, &typography)?;
                } else {
                    edit.set_typography_preserving_origin(layer, &typography)?;
                }
            }
            Ok(())
        })?;
        Ok((Some(self.commit(patch).await?), result))
    }

    pub(crate) async fn set_geometry(&mut self, updates: Vec<GeometryUpdate>) -> Result<Commit> {
        let snapshot = self.snapshot();
        let updates = updates
            .into_iter()
            .map(|update| {
                let layout = snapshot
                    .component::<SceneTextLayout>(update.layer)?
                    .context("only text layers can change geometry")?;
                let content = Self::text_content(&snapshot, update.layer)?;
                if update.points.is_none()
                    && snapshot
                        .text_layer(update.layer)?
                        .automatic_target()?
                        .is_none()
                {
                    bail!("only automatically placed text can reset its geometry");
                }
                Ok((update, content, layout))
            })
            .collect::<Result<Vec<_>>>()?;
        let patch = snapshot.patch(|edit| {
            for (update, content, mut layout) in updates {
                edit.promote_entity_to_user(update.layer)?;
                edit.promote_entity_to_user(content)?;
                layout.origin = Origin::User;
                layout.angle_degrees = None;
                edit.set(update.layer, &layout)?;
                match update.points {
                    Some(points) => edit.set(
                        update.layer,
                        &SceneGeometry {
                            origin: Origin::User,
                            points: points
                                .into_iter()
                                .map(|point| ScenePoint {
                                    x: point.x,
                                    y: point.y,
                                })
                                .collect(),
                        },
                    )?,
                    None => edit.remove::<SceneGeometry>(update.layer)?,
                }
            }
            Ok(())
        })?;
        self.commit(patch).await
    }

    pub(crate) async fn set_visibility(
        &mut self,
        layers: Vec<EntityId>,
        visible: Option<bool>,
        opacity: Option<f32>,
    ) -> Result<Commit> {
        let snapshot = self.snapshot();
        let patch = snapshot.patch(|edit| {
            for layer in layers {
                let mut value =
                    snapshot
                        .component::<SceneVisibility>(layer)?
                        .unwrap_or(SceneVisibility {
                            origin: Origin::User,
                            visible: true,
                            opacity: 1.0,
                        });
                if let Some(visible) = visible {
                    value.visible = visible;
                }
                if let Some(opacity) = opacity {
                    value.opacity = opacity;
                }
                value.origin = Origin::User;
                edit.set(layer, &value)?;
            }
            Ok(())
        })?;
        self.commit(patch).await
    }

    pub(crate) async fn delete_layers(&mut self, layers: Vec<EntityId>) -> Result<Commit> {
        let snapshot = self.snapshot();
        let mut expanded = Vec::new();
        for layer in layers {
            if snapshot.component::<SceneTextGroup>(layer)?.is_some() {
                expanded.extend(snapshot.children(layer)?);
            } else {
                expanded.push(layer);
            }
        }
        let layers = Self::unique_roots(&snapshot, expanded)?;
        let mut orphaned_contents = Vec::new();
        for layer in &layers {
            let Some(relation) = snapshot.relation_from::<Presents>(*layer)? else {
                continue;
            };
            let content = relation.value().target;
            if snapshot.relations_to_as::<Presents>(content).count() == 1 {
                orphaned_contents.push(content);
            }
        }
        let patch = snapshot.patch(|edit| {
            for layer in layers {
                if snapshot.page(layer).is_ok() {
                    return Err(koharu_scene::Error::Invalid(
                        "delete pages with delete_pages".to_owned(),
                    ));
                }
                edit.remove_entity(layer, RemovePolicy::Cascade)?;
            }
            for content in orphaned_contents {
                if snapshot.entity(content).is_ok() {
                    edit.remove_entity(content, RemovePolicy::Cascade)?;
                }
            }
            Ok(())
        })?;
        self.commit(patch).await
    }

    #[tracing::instrument(level = "info", skip_all, fields(layer = %layer, parent = %parent))]
    pub(crate) async fn move_layer(
        &mut self,
        layer: EntityId,
        parent: EntityId,
        index: usize,
    ) -> Result<Commit> {
        let snapshot = self.snapshot();
        let siblings = snapshot
            .children(parent)?
            .map(|candidate| Ok(Self::is_layer(&snapshot, candidate)?.then_some(candidate)))
            .collect::<Result<Vec<_>>>()?
            .into_iter()
            .flatten()
            .collect::<Vec<_>>();
        let at = Self::placement(&siblings, layer, index);
        let patch = snapshot.patch(|edit| {
            edit.promote_entity_to_user(layer)?;
            if snapshot.component::<SceneTextGroup>(parent)?.is_some() {
                edit.promote_entity_to_user(parent)?;
            }
            edit.move_entity(layer, Some(parent), at)
        })?;
        self.commit(patch).await
    }

    pub(crate) async fn set_geometries(
        &mut self,
        geometries: impl IntoIterator<Item = (EntityId, SceneGeometry, f32)>,
    ) -> Result<Commit> {
        let snapshot = self.snapshot();
        let geometries = geometries
            .into_iter()
            .map(|(element, geometry, angle_degrees)| {
                let mut layout = snapshot
                    .component::<SceneTextLayout>(element)?
                    .context("only text layers can change geometry")?;
                layout.origin = Origin::User;
                layout.angle_degrees = Some(angle_degrees);
                let content = Self::text_content(&snapshot, element)?;
                Ok((element, geometry, content, layout))
            })
            .collect::<Result<Vec<_>>>()?;
        let patch = snapshot.patch(|edit| {
            for (element, mut geometry, content, layout) in geometries {
                edit.promote_entity_to_user(element)?;
                edit.promote_entity_to_user(content)?;
                geometry.origin = Origin::User;
                edit.set(element, &geometry)?;
                edit.set(element, &layout)?;
            }
            Ok(())
        })?;
        self.commit(patch).await
    }

    pub(crate) async fn apply_raster_stroke(
        &mut self,
        page: EntityId,
        layer: Option<EntityId>,
        mode: RasterStrokeMode,
        color: [u8; 4],
        diameter: f32,
        points: Vec<ScenePoint>,
    ) -> Result<(Commit, EntityId)> {
        if !diameter.is_finite() || diameter <= 0.0 || points.is_empty() {
            bail!("a raster stroke requires a positive diameter and at least one point");
        }
        if points
            .iter()
            .any(|point| !point.x.is_finite() || !point.y.is_finite())
        {
            bail!("raster stroke points must be finite");
        }
        let snapshot = self.snapshot();
        let page_value = snapshot.page(page)?.page()?;
        let width = page_value.width.round() as u32;
        let height = page_value.height.round() as u32;
        if width == 0 || height == 0 {
            bail!("page dimensions must be positive");
        }
        let mut raster_layer = None;
        let mut promote_layer = false;
        let mut image = if let Some(layer) = layer {
            if snapshot.parent(layer)? != Some(page) {
                bail!("the raster target must be a layer on the active page");
            }
            let target = snapshot
                .component::<SceneRasterLayer>(layer)?
                .context("the raster target must be a layer on the active page")?;
            promote_layer = target.origin != Origin::User
                || snapshot
                    .component::<EntityOrigin>(layer)?
                    .is_some_and(|origin| origin.origin != Origin::User);
            raster_layer = Some(target);
            match snapshot.asset(layer, &AssetRole::new("source")?)? {
                Some(asset) => {
                    let bytes = snapshot.read_blob(asset.blob).await?;
                    image::load_from_memory(&bytes)?.to_rgba8()
                }
                None => RgbaImage::new(width, height),
            }
        } else {
            if mode == RasterStrokeMode::Erase {
                bail!("eraser requires a raster layer target");
            }
            RgbaImage::new(width, height)
        };
        if image.dimensions() != (width, height) {
            bail!("raster layer dimensions must match the page");
        }
        rasterize_stroke(&mut image, mode, color, diameter, &points);
        let mut bytes = Cursor::new(Vec::new());
        DynamicImage::ImageRgba8(image).write_to(&mut bytes, ImageFormat::Png)?;
        let source = AssetRole::new("source")?;
        let name = format!(
            "Paint {}",
            snapshot
                .children(page)?
                .filter(|entity| {
                    snapshot
                        .component::<SceneRasterLayer>(*entity)
                        .ok()
                        .flatten()
                        .is_some_and(|layer| layer.kind == RasterLayerKind::Paint)
                })
                .count()
                + 1
        );
        let mut committed_layer = layer;
        let patch = snapshot.patch(|edit| {
            let layer = if let Some(layer) = committed_layer {
                if promote_layer {
                    edit.promote_entity_to_user(layer)?;
                    let raster = raster_layer
                        .as_mut()
                        .expect("an existing raster target has a layer component");
                    raster.origin = Origin::User;
                    edit.set(layer, raster)?;
                }
                layer
            } else {
                let at = snapshot
                    .page(page)?
                    .text_group()?
                    .map_or(At::End, |group| At::Before(group.id()));
                let layer = edit.add_entity(page, at)?;
                edit.set(
                    layer,
                    &SceneRasterLayer {
                        origin: Origin::User,
                        name,
                        kind: RasterLayerKind::Paint,
                    },
                )?;
                committed_layer = Some(layer);
                layer
            };
            edit.set_asset(
                layer,
                &source,
                AssetInput::new(
                    bytes.into_inner(),
                    "image/png",
                    AssetMetadata {
                        width: Some(width),
                        height: Some(height),
                        attributes: Default::default(),
                    },
                ),
            )
        })?;
        Ok((
            self.commit(patch).await?,
            committed_layer.expect("raster layer was selected or added while building the patch"),
        ))
    }

    pub(crate) async fn undo(&mut self) -> Result<Commit> {
        let revisions = self.undo.pop().ok_or_else(|| anyhow!("nothing to undo"))?;
        let commit = match self.session.undo_many(revisions.iter().copied()).await {
            Ok(commit) => commit,
            Err(error) => {
                self.undo.push(revisions);
                return Err(error.into());
            }
        };
        self.redo.push(vec![commit.revision]);
        Ok(commit)
    }

    pub(crate) async fn redo(&mut self) -> Result<Commit> {
        let revisions = self.redo.pop().ok_or_else(|| anyhow!("nothing to redo"))?;
        let commit = match self.session.undo_many(revisions.iter().copied()).await {
            Ok(commit) => commit,
            Err(error) => {
                self.redo.push(revisions);
                return Err(error.into());
            }
        };
        self.undo.push(vec![commit.revision]);
        Ok(commit)
    }

    pub(crate) fn record(&mut self, revisions: Vec<Revision>) {
        if !revisions.is_empty() {
            self.undo.push(revisions);
            self.redo.clear();
        }
    }

    pub(crate) fn record_commit(&mut self, commit: &Commit) {
        if commit.changes.to != commit.changes.from {
            self.record(vec![commit.revision]);
        }
    }

    pub(crate) async fn commit_rebased(
        &mut self,
        patch: koharu_scene::Patch,
    ) -> Result<Option<Commit>> {
        let current = self.snapshot();
        let patch = match patch.rebase_on(&current) {
            Ok(patch) => patch,
            Err(
                error @ (koharu_scene::Error::PatchConflict(_)
                | koharu_scene::Error::EntityNotFound(_)
                | koharu_scene::Error::RelationNotFound(_)),
            ) => {
                tracing::debug!(%error, "pipeline output was superseded by a document edit");
                return Ok(None);
            }
            Err(error) => return Err(error.into()),
        };
        if patch.is_empty() {
            return Ok(None);
        }
        Ok(Some(self.commit(patch).await?))
    }

    async fn commit(&mut self, patch: koharu_scene::Patch) -> Result<Commit> {
        Ok(self.session.commit(patch).await?)
    }

    pub(crate) fn page(snapshot: &Snapshot, page: EntityId) -> Result<Page> {
        let value = snapshot.page(page)?.page()?;
        let source = Self::asset_id(snapshot, page, "source")?;
        let mut layers = Vec::new();
        Self::collect_layer_views(snapshot, page, &mut layers)?;
        if let Some(image) = source.clone() {
            layers.insert(
                0,
                Layer::Artwork {
                    id: page,
                    parent: None,
                    geometry: Geometry {
                        points: vec![
                            Point { x: 0.0, y: 0.0 },
                            Point {
                                x: value.width,
                                y: 0.0,
                            },
                            Point {
                                x: value.width,
                                y: value.height,
                            },
                            Point {
                                x: 0.0,
                                y: value.height,
                            },
                        ],
                    },
                    visibility: LayerVisibility {
                        visible: true,
                        opacity: 1.0,
                    },
                    image,
                },
            );
        }
        let regions = snapshot
            .descendants(page)?
            .map(|entity| {
                let id = entity.id();
                snapshot
                    .component::<SceneRegion>(id)?
                    .map(|region| Self::analysis_region(snapshot, id, region))
                    .transpose()
            })
            .collect::<Result<Vec<_>>>()?
            .into_iter()
            .flatten()
            .collect();
        Ok(Page {
            id: page,
            label: value.label,
            size: PageSize {
                width: value.width,
                height: value.height,
            },
            layers,
            regions,
        })
    }

    fn layer_view(snapshot: &Snapshot, layer: EntityId) -> Result<Layer> {
        let parent = snapshot.parent(layer)?;
        let visibility = Self::layer_visibility(snapshot, layer)?;
        if let Some(group) = snapshot.component::<SceneGroup>(layer)? {
            return Ok(Layer::Group {
                id: layer,
                parent,
                visibility,
                name: group.name,
                role: snapshot
                    .component::<SceneTextGroup>(layer)?
                    .map(|_| GroupRole::Text),
            });
        }
        if let Some(layout) = snapshot.component::<SceneTextLayout>(layer)? {
            let text_layer = snapshot.text_layer(layer)?;
            let content = text_layer.content()?;
            let source = content.source()?.map(|source| SourceText {
                text: source.text.value,
                language: source.language.map(|language| language.to_string()),
            });
            let translation = content.translation()?.map(|translation| Translation {
                text: translation.text.value,
                language: translation.language.map(|language| language.to_string()),
            });
            let role = content.role()?.map(|role| role.role);
            let source_region = content.source_region()?.map(|region| region.id());
            let automatic_region = text_layer.automatic_target()?.map(|region| region.id());
            let typography = text_layer.typography()?.map(Self::typography_view);
            return Ok(Layer::Text {
                id: layer,
                parent,
                geometry: snapshot
                    .component::<SceneGeometry>(layer)?
                    .map(Self::geometry_view),
                angle_degrees: layout.angle_degrees,
                visibility,
                content: Box::new(TextContent {
                    id: content.id(),
                    source,
                    translation,
                    role,
                    source_region,
                }),
                typography,
                layout: layout.kind,
                automatic_region,
            });
        }
        if let Some(raster) = snapshot.component::<SceneRasterLayer>(layer)? {
            return Ok(Layer::Raster {
                id: layer,
                parent,
                visibility,
                image: Self::asset_id(snapshot, layer, "source")?,
                name: raster.name,
                kind: raster.kind,
            });
        }
        let geometry = snapshot
            .component::<SceneGeometry>(layer)?
            .map(Self::geometry_view)
            .context("image layer has no geometry")?;
        let image = Self::asset_id(snapshot, layer, "source")?
            .context("image layer has no source asset")?;
        Ok(Layer::Image {
            id: layer,
            parent,
            geometry,
            visibility,
            image,
        })
    }

    fn analysis_region(
        snapshot: &Snapshot,
        id: EntityId,
        region: SceneRegion,
    ) -> Result<AnalysisRegion> {
        let geometry = snapshot
            .component::<SceneGeometry>(id)?
            .map(Self::geometry_view)
            .context("analysis region has no geometry")?;
        Ok(AnalysisRegion {
            id,
            parent: snapshot.parent(id)?,
            geometry,
            kind: region.kind.as_str().to_owned(),
            label: region.label,
        })
    }

    fn layer_visibility(snapshot: &Snapshot, layer: EntityId) -> Result<LayerVisibility> {
        Ok(snapshot.component::<SceneVisibility>(layer)?.map_or(
            LayerVisibility {
                visible: true,
                opacity: 1.0,
            },
            |visibility| LayerVisibility {
                visible: visibility.visible,
                opacity: visibility.opacity,
            },
        ))
    }

    fn typography_view(typography: SceneTypography) -> Typography {
        let writing_mode = if matches!(&typography.origin, Origin::User) {
            typography.writing_mode
        } else {
            None
        };
        Typography {
            preferred_font: typography.preferred_font,
            font_weight: typography.font_weight,
            font_style: typography.font_style,
            size: typography.size,
            auto_fit: typography.auto_fit,
            color: typography.color,
            stroke_color: typography.stroke_color,
            stroke_width: typography.stroke_width,
            stroke_method: typography.stroke_method,
            stroke_position: typography.stroke_position,
            stroke_join: typography.stroke_join,
            outer_stroke_color: typography.outer_stroke_color,
            outer_stroke_width: typography.outer_stroke_width,
            alignment: typography.alignment,
            writing_mode,
        }
    }

    fn geometry_view(geometry: SceneGeometry) -> Geometry {
        Geometry {
            points: geometry
                .points
                .into_iter()
                .map(|point| Point {
                    x: point.x,
                    y: point.y,
                })
                .collect(),
        }
    }

    fn asset_id(snapshot: &Snapshot, entity: EntityId, role: &str) -> Result<Option<String>> {
        Ok(snapshot
            .asset(entity, &AssetRole::new(role)?)?
            .map(|asset| asset.blob.to_string()))
    }

    fn is_layer(snapshot: &Snapshot, entity: EntityId) -> Result<bool> {
        Ok(snapshot.component::<SceneGroup>(entity)?.is_some()
            || snapshot.component::<SceneTextLayout>(entity)?.is_some()
            || snapshot.component::<SceneRasterLayer>(entity)?.is_some()
            || (snapshot.component::<SceneGeometry>(entity)?.is_some()
                && snapshot
                    .asset(entity, &AssetRole::new("source")?)?
                    .is_some()))
    }

    fn is_content_layer(snapshot: &Snapshot, entity: EntityId) -> Result<bool> {
        Ok(
            snapshot.component::<SceneGroup>(entity)?.is_none()
                && Self::is_layer(snapshot, entity)?,
        )
    }

    fn collect_layer_views(
        snapshot: &Snapshot,
        parent: EntityId,
        layers: &mut Vec<Layer>,
    ) -> Result<()> {
        for child in snapshot.children(parent)? {
            if !Self::is_layer(snapshot, child)? {
                continue;
            }
            let group = snapshot.component::<SceneGroup>(child)?.is_some();
            layers.push(Self::layer_view(snapshot, child)?);
            if group {
                Self::collect_layer_views(snapshot, child, layers)?;
            }
        }
        Ok(())
    }

    fn text_content(snapshot: &Snapshot, layer: EntityId) -> Result<EntityId> {
        snapshot
            .text_layer(layer)?
            .content()
            .map(|content| content.id())
            .map_err(Into::into)
    }

    fn placement(siblings: &[EntityId], moving: EntityId, index: usize) -> At {
        siblings
            .iter()
            .copied()
            .filter(|entity| *entity != moving)
            .nth(index)
            .map_or(At::End, At::Before)
    }

    fn unique_roots(snapshot: &Snapshot, entities: Vec<EntityId>) -> Result<Vec<EntityId>> {
        let selected = entities.into_iter().collect::<HashSet<_>>();
        let mut roots = Vec::new();
        for entity in selected.iter().copied() {
            let mut parent = snapshot.parent(entity)?;
            let mut nested = false;
            while let Some(value) = parent {
                if selected.contains(&value) {
                    nested = true;
                    break;
                }
                parent = snapshot.parent(value)?;
            }
            if !nested {
                roots.push(entity);
            }
        }
        roots.sort_unstable();
        Ok(roots)
    }

    fn geometry_from_frame(frame: Frame) -> Result<SceneGeometry> {
        if !frame.x.is_finite()
            || !frame.y.is_finite()
            || !frame.width.is_finite()
            || !frame.height.is_finite()
            || !frame.angle_degrees.is_finite()
            || frame.width <= 0.0
            || frame.height <= 0.0
        {
            bail!("frame must contain finite coordinates and positive dimensions");
        }
        let center_x = f64::from(frame.x + frame.width * 0.5);
        let center_y = f64::from(frame.y + frame.height * 0.5);
        let half_width = f64::from(frame.width) * 0.5;
        let half_height = f64::from(frame.height) * 0.5;
        let (sin, cos) = f64::from(frame.angle_degrees).to_radians().sin_cos();
        Ok(SceneGeometry {
            origin: Origin::User,
            points: [
                (-half_width, -half_height),
                (half_width, -half_height),
                (half_width, half_height),
                (-half_width, half_height),
            ]
            .map(|(x, y)| ScenePoint {
                x: center_x + x * cos - y * sin,
                y: center_y + x * sin + y * cos,
            })
            .into(),
        })
    }
}

fn validate_project_name(name: &str) -> Result<String> {
    let name = name.trim();
    if name.is_empty() {
        bail!("project name cannot be empty");
    }
    if name.ends_with(['.', ' '])
        || name
            .chars()
            .any(|character| character.is_control() || r#"<>:"/\|?*"#.contains(character))
    {
        bail!("project name contains characters that cannot be used in a file name");
    }
    let stem = name.split('.').next().unwrap_or(name).to_ascii_uppercase();
    if matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || stem
            .strip_prefix("COM")
            .or_else(|| stem.strip_prefix("LPT"))
            .is_some_and(|number| {
                matches!(number, "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9")
            })
    {
        bail!("project name is reserved by Windows");
    }
    Ok(name.to_owned())
}

fn rasterize_stroke(
    image: &mut RgbaImage,
    mode: RasterStrokeMode,
    color: [u8; 4],
    diameter: f32,
    points: &[ScenePoint],
) {
    let radius = f64::from(diameter) * 0.5;
    for (start, end) in points
        .iter()
        .zip(points.iter().skip(1))
        .chain(points.last().map(|point| (point, point)))
    {
        let left = (start.x.min(end.x) - radius - 0.5).floor().max(0.0) as u32;
        let top = (start.y.min(end.y) - radius - 0.5).floor().max(0.0) as u32;
        let right = (start.x.max(end.x) + radius + 0.5)
            .ceil()
            .min(f64::from(image.width())) as u32;
        let bottom = (start.y.max(end.y) + radius + 0.5)
            .ceil()
            .min(f64::from(image.height())) as u32;
        let dx = end.x - start.x;
        let dy = end.y - start.y;
        let length_squared = dx * dx + dy * dy;
        for y in top..bottom {
            for x in left..right {
                let px = f64::from(x) + 0.5;
                let py = f64::from(y) + 0.5;
                let t = if length_squared == 0.0 {
                    0.0
                } else {
                    (((px - start.x) * dx + (py - start.y) * dy) / length_squared).clamp(0.0, 1.0)
                };
                let distance =
                    ((px - (start.x + t * dx)).powi(2) + (py - (start.y + t * dy)).powi(2)).sqrt();
                let coverage = (radius + 0.5 - distance).clamp(0.0, 1.0) as f32;
                if coverage == 0.0 {
                    continue;
                }
                let pixel = image.get_pixel_mut(x, y);
                match mode {
                    RasterStrokeMode::Paint => {
                        let source_alpha = f32::from(color[3]) / 255.0 * coverage;
                        let destination_alpha = f32::from(pixel[3]) / 255.0;
                        let output_alpha = source_alpha + destination_alpha * (1.0 - source_alpha);
                        if output_alpha > 0.0 {
                            for channel in 0..3 {
                                let source = f32::from(color[channel]) / 255.0;
                                let destination = f32::from(pixel[channel]) / 255.0;
                                pixel[channel] = (((source * source_alpha
                                    + destination * destination_alpha * (1.0 - source_alpha))
                                    / output_alpha)
                                    * 255.0)
                                    .round() as u8;
                            }
                        }
                        pixel[3] = (output_alpha * 255.0).round() as u8;
                    }
                    RasterStrokeMode::Erase => {
                        pixel[3] = (f32::from(pixel[3]) * (1.0 - coverage)).round() as u8;
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use koharu_scene::{Generation, ProducerId, WritingMode};

    use super::*;

    fn style_test_typography() -> SceneTypography {
        SceneTypography {
            origin: Origin::User,
            preferred_font: Some("Original".into()),
            font_weight: Some(400),
            font_style: None,
            size: Some(24.0),
            auto_fit: false,
            color: Some([0, 0, 0, 255]),
            stroke_color: Some([255, 255, 255, 255]),
            stroke_width: Some(0.0),
            stroke_method: Some(koharu_scene::StrokeMethod::Contour),
            stroke_position: None,
            stroke_join: None,
            outer_stroke_color: None,
            outer_stroke_width: None,
            alignment: Some(koharu_scene::TextAlignment::Center),
            writing_mode: Some(WritingMode::Horizontal),
            extensions: [("dev.booharu.test".into(), "kept".into())].into(),
        }
    }

    #[tokio::test]
    async fn batch_text_styles_change_three_boxes_in_one_undo_and_ignore_noops() {
        let mut session = Session::memory().await.unwrap();
        let original = style_test_typography();
        let mut layers = Vec::new();
        let patch = session
            .snapshot()
            .patch(|edit| {
                for count in [2, 1] {
                    let page = edit.add_page(PageDraft::new("page", 100.0, 100.0), At::End)?;
                    for _ in 0..count {
                        let content = edit.add_text_content(page, At::End)?;
                        let layer = edit.add_text_layer(
                            page,
                            At::End,
                            content,
                            &SceneTextLayout {
                                origin: Origin::User,
                                kind: TextLayoutKind::Paragraph,
                                angle_degrees: None,
                            },
                        )?;
                        edit.set(layer, &original)?;
                        layers.push(layer);
                    }
                }
                Ok(())
            })
            .unwrap();
        session.commit(patch).await.unwrap();
        let mut project = Project::new(session, "styles".into());
        let style = koharu_renderer::TextStyle {
            fill: Some([255, 255, 255, 255]),
            ..Default::default()
        };
        let (commit, result) = project
            .apply_style(
                super::super::styles::StyleScope::Project,
                true,
                true,
                |_, _, typography| {
                    style.apply_to(typography);
                    Ok(())
                },
            )
            .await
            .unwrap();
        assert_eq!(result.changed, 3);
        assert_eq!(result.skipped, 0);
        project.record_commit(&commit.unwrap());
        assert_eq!(project.undo.len(), 1);
        for &layer in &layers {
            let typography = project
                .snapshot()
                .component::<SceneTypography>(layer)
                .unwrap()
                .unwrap();
            assert_eq!(typography.color, style.fill);
            assert_eq!(typography.size, original.size);
            assert_eq!(typography.extensions, original.extensions);
        }
        let (commit, result) = project
            .apply_style(
                super::super::styles::StyleScope::Project,
                true,
                true,
                |_, _, typography| {
                    style.apply_to(typography);
                    Ok(())
                },
            )
            .await
            .unwrap();
        assert!(commit.is_none());
        assert_eq!(result.changed, 0);
        assert_eq!(project.undo.len(), 1);
        project.undo().await.unwrap();
        for layer in layers {
            assert_eq!(
                project
                    .snapshot()
                    .component::<SceneTypography>(layer)
                    .unwrap()
                    .unwrap(),
                original
            );
        }
    }

    #[tokio::test]
    async fn batch_text_styles_skip_a_missing_content_relation() {
        let mut session = Session::memory().await.unwrap();
        let mut layers = Vec::new();
        let patch = session
            .snapshot()
            .patch(|edit| {
                let page = edit.add_page(PageDraft::new("page", 100.0, 100.0), At::End)?;
                for _ in 0..2 {
                    let content = edit.add_text_content(page, At::End)?;
                    let layer = edit.add_text_layer(
                        page,
                        At::End,
                        content,
                        &SceneTextLayout {
                            origin: Origin::User,
                            kind: TextLayoutKind::Paragraph,
                            angle_degrees: None,
                        },
                    )?;
                    edit.set(layer, &style_test_typography())?;
                    layers.push(layer);
                }
                Ok(())
            })
            .unwrap();
        let snapshot = session.commit(patch).await.unwrap().snapshot;
        let relation = snapshot
            .relation_from::<Presents>(layers[0])
            .unwrap()
            .unwrap()
            .id();
        let patch = snapshot
            .patch(|edit| edit.remove_relation(relation))
            .unwrap();
        session.commit(patch).await.unwrap();
        let mut project = Project::new(session, "styles".into());
        let style = koharu_renderer::TextStyle {
            fill: Some([255, 255, 255, 255]),
            ..Default::default()
        };
        let (commit, result) = project
            .apply_style(
                super::super::styles::StyleScope::Layers(layers),
                true,
                true,
                |_, _, typography| {
                    style.apply_to(typography);
                    Ok(())
                },
            )
            .await
            .unwrap();
        assert!(commit.is_some());
        assert_eq!(result.changed, 1);
        assert_eq!(result.skipped, 1);
    }

    #[tokio::test]
    async fn shape_styles_apply_across_pages_with_flows_in_and_inside_in_one_undo() {
        use koharu_renderer::{FontPart, ShapeStyles, TextStyle, TextStyles};
        use koharu_scene::{BubbleRegion, FlowsIn, Inside, RecognizedFrom, TextRegion};
        let mut session = Session::memory().await.unwrap();
        let mut layers = Vec::new();
        let original = style_test_typography();
        let patch = session
            .snapshot()
            .patch(|edit| {
                for page_index in 0..2 {
                    let page = edit.add_page(PageDraft::new("page", 300.0, 300.0), At::End)?;
                    let bubble = edit.add_analysis_region::<BubbleRegion>(
                        page,
                        At::End,
                        &SceneGeometry::rectangle(10.0, 10.0, 200.0, 100.0),
                        None,
                    )?;
                    let content = edit.add_text_content(page, At::End)?;
                    let layer = edit.add_text_layer(
                        page,
                        At::End,
                        content,
                        &SceneTextLayout {
                            origin: Origin::User,
                            kind: TextLayoutKind::Paragraph,
                            angle_degrees: None,
                        },
                    )?;
                    edit.set(layer, &original)?;
                    if page_index == 0 {
                        edit.relate::<FlowsIn>(layer, bubble)?;
                    } else {
                        let region = edit.add_analysis_region::<TextRegion>(
                            page,
                            At::End,
                            &SceneGeometry::rectangle(20.0, 20.0, 50.0, 50.0),
                            None,
                        )?;
                        edit.relate::<RecognizedFrom>(content, region)?;
                        edit.relate::<Inside>(region, bubble)?;
                    }
                    layers.push(layer);
                    if page_index == 1 {
                        let content = edit.add_text_content(page, At::End)?;
                        let layer = edit.add_text_layer(
                            page,
                            At::End,
                            content,
                            &SceneTextLayout {
                                origin: Origin::User,
                                kind: TextLayoutKind::Paragraph,
                                angle_degrees: None,
                            },
                        )?;
                        edit.set(layer, &original)?;
                        layers.push(layer);
                    }
                }
                Ok(())
            })
            .unwrap();
        session.commit(patch).await.unwrap();
        let font = |name: &str| TextStyle {
            font: Some(FontPart {
                preferred_font: Some(name.into()),
                font_weight: None,
                font_style: None,
            }),
            ..Default::default()
        };
        let mut styles = TextStyles {
            default_style: TextStyle {
                fill: Some([12, 34, 56, 255]),
                ..Default::default()
            },
            shape_styles: ShapeStyles {
                enabled: true,
                rectangle: font("narration"),
                free: font("free"),
                ..Default::default()
            },
            ..Default::default()
        };
        let mut project = Project::new(session, "shapes".into());
        let (commit, result) = project
            .apply_shape_rules(
                super::super::styles::StyleScope::Project,
                &styles,
                false,
                true,
            )
            .await
            .unwrap();
        assert_eq!(result.changed, 3);
        assert_eq!(result.skipped, 0);
        project.record_commit(&commit.unwrap());
        assert_eq!(project.undo.len(), 1);
        for (index, &layer) in layers.iter().enumerate() {
            let typography = project
                .snapshot()
                .component::<SceneTypography>(layer)
                .unwrap()
                .unwrap();
            assert_eq!(
                typography.preferred_font.as_deref(),
                Some(if index == 2 { "free" } else { "narration" })
            );
            assert_eq!(typography.extensions, original.extensions);
            assert_eq!(typography.color, styles.default_style.fill);
        }
        project.undo().await.unwrap();
        for &layer in &layers {
            assert_eq!(
                project
                    .snapshot()
                    .component::<SceneTypography>(layer)
                    .unwrap()
                    .unwrap(),
                original
            );
        }
        styles.shape_styles.enabled = false;
        let (commit, result) = project
            .apply_shape_rules(
                super::super::styles::StyleScope::Project,
                &styles,
                false,
                true,
            )
            .await
            .unwrap();
        assert!(commit.is_none());
        assert_eq!(result.changed, 0);
    }

    #[tokio::test]
    async fn shape_rules_keep_manual_boxes_and_generated_ownership() {
        use super::super::styles::StyleScope;
        use koharu_renderer::{ShapeStyles, TextStyle, TextStyles};
        let mut session = Session::memory().await.unwrap();
        let generation = Generation::new(ProducerId::new("dev.koharu.pipeline.detection").unwrap());
        let mut edit = session.snapshot().edit_as(generation.clone());
        let mut layers = Vec::new();
        for _ in 0..2 {
            let page = edit
                .add_page(PageDraft::new("page", 300.0, 300.0), At::End)
                .unwrap();
            let content = edit.add_text_content(page, At::End).unwrap();
            let layer = edit
                .add_text_layer(
                    page,
                    At::End,
                    content,
                    &SceneTextLayout {
                        origin: Origin::Generated(generation.clone()),
                        kind: TextLayoutKind::Paragraph,
                        angle_degrees: None,
                    },
                )
                .unwrap();
            edit.set(layer, &style_test_typography()).unwrap();
            layers.push((layer, content));
        }
        session.commit(edit.finish().unwrap()).await.unwrap();
        let mut project = Project::new(session, "manual rules".into());
        let manual_style = TextStyle {
            fill: Some([1, 2, 3, 255]),
            ..Default::default()
        };
        project
            .apply_style(
                StyleScope::Layers(vec![layers[0].0]),
                true,
                true,
                |_, _, typography| {
                    manual_style.apply_to(typography);
                    Ok(())
                },
            )
            .await
            .unwrap();
        let styles = TextStyles {
            default_style: TextStyle {
                fill: Some([4, 5, 6, 255]),
                ..Default::default()
            },
            shape_styles: ShapeStyles {
                enabled: true,
                ..Default::default()
            },
            ..Default::default()
        };
        let (commit, result) = project
            .apply_shape_rules(StyleScope::Project, &styles, false, false)
            .await
            .unwrap();
        assert_eq!(
            (result.changed, result.skipped, result.kept_manual),
            (1, 0, 1)
        );
        project.record_commit(&commit.unwrap());
        let snapshot = project.snapshot();
        assert_eq!(
            snapshot
                .component::<SceneTypography>(layers[0].0)
                .unwrap()
                .unwrap()
                .color,
            manual_style.fill
        );
        assert!(matches!(
            snapshot
                .component::<SceneTypography>(layers[1].0)
                .unwrap()
                .unwrap()
                .origin,
            Origin::Generated(_)
        ));
        for id in [layers[1].0, layers[1].1] {
            assert!(matches!(
                snapshot
                    .component::<koharu_scene::EntityOrigin>(id)
                    .unwrap()
                    .unwrap()
                    .origin,
                Origin::Generated(_)
            ));
        }
        project.undo().await.unwrap();
        let (commit, result) = project
            .apply_shape_rules(StyleScope::Project, &styles, false, true)
            .await
            .unwrap();
        assert_eq!((result.changed, result.kept_manual), (2, 0));
        project.record_commit(&commit.unwrap());
        project.undo().await.unwrap();
        let (_, result) = project
            .apply_shape_rules(StyleScope::Layers(vec![layers[0].0]), &styles, false, false)
            .await
            .unwrap();
        assert_eq!((result.changed, result.kept_manual), (1, 0));
        // A second automatic run still targets generated boxes.
        let mut changed_styles = styles.clone();
        changed_styles.default_style.fill = Some([7, 8, 9, 255]);
        let (_, result) = project
            .apply_shape_rules(StyleScope::Project, &changed_styles, false, false)
            .await
            .unwrap();
        assert_eq!((result.changed, result.kept_manual), (1, 1));
    }

    #[tokio::test]
    async fn imported_pages_append_with_source_assets_and_one_undo() {
        let mut session = Session::memory().await.unwrap();
        let create = session
            .snapshot()
            .patch(|edit| {
                edit.add_page(PageDraft::new("existing", 20.0, 30.0), At::End)?;
                Ok(())
            })
            .unwrap();
        session.commit(create).await.unwrap();
        let mut project = Project::new(session, "import test".to_owned());
        let mut bytes = Cursor::new(Vec::new());
        DynamicImage::ImageRgba8(RgbaImage::from_pixel(2, 3, image::Rgba([10, 20, 30, 255])))
            .write_to(&mut bytes, ImageFormat::Png)
            .unwrap();
        let pages = ["01.png", "02.png", "03.png"]
            .into_iter()
            .map(|name| import::Page {
                name: name.to_owned(),
                bytes: bytes.get_ref().clone().into(),
                format: ImageFormat::Png,
                width: 2,
                height: 3,
            })
            .collect();
        let commit = project.import_pages(pages).await.unwrap();
        project.record_commit(&commit);
        project.reconcile_page();
        let summary = Project::pages(&project.snapshot()).unwrap();
        assert_eq!(
            summary
                .iter()
                .map(|page| page.label.as_str())
                .collect::<Vec<_>>(),
            ["existing", "01.png", "02.png", "03.png"]
        );
        for page in &summary[1..] {
            assert_eq!((page.size.width, page.size.height), (2.0, 3.0));
            assert!(page.source_asset.is_some());
        }
        assert_eq!(project.undo.len(), 1);
        project.undo().await.unwrap();
        let summary = Project::pages(&project.snapshot()).unwrap();
        assert_eq!(summary.len(), 1);
        assert_eq!(summary[0].label, "existing");
        project.redo().await.unwrap();
        assert_eq!(Project::pages(&project.snapshot()).unwrap().len(), 4);
    }

    #[test]
    fn only_user_authored_direction_is_projected_as_an_override() {
        let mut typography = SceneTypography {
            origin: Origin::Generated(Generation::new(
                ProducerId::new("dev.koharu.pipeline.detection").expect("valid producer"),
            )),
            preferred_font: None,
            font_weight: None,
            font_style: None,
            size: None,
            auto_fit: true,
            color: None,
            stroke_color: None,
            stroke_width: None,
            stroke_method: None,
            stroke_position: None,
            stroke_join: None,
            outer_stroke_color: None,
            outer_stroke_width: None,
            alignment: None,
            writing_mode: Some(WritingMode::Vertical),
            extensions: Default::default(),
        };

        assert_eq!(
            Project::typography_view(typography.clone()).writing_mode,
            None
        );
        typography.origin = Origin::User;
        assert_eq!(
            Project::typography_view(typography).writing_mode,
            Some(WritingMode::Vertical)
        );
    }

    #[tokio::test]
    async fn fixing_bubble_geometry_is_one_undo_and_single_reset_still_works() {
        let mut session = Session::memory().await.unwrap();
        let mut edit = session.snapshot().edit();
        let page = edit
            .add_page(PageDraft::new("page", 200.0, 160.0), At::End)
            .unwrap();
        let bubble = edit
            .add_analysis_region::<koharu_scene::BubbleRegion>(
                page,
                At::End,
                &SceneGeometry::rectangle(40.0, 50.0, 120.0, 60.0),
                None,
            )
            .unwrap();
        let mut layers = Vec::new();
        for text in ["First flow", "Second flow"] {
            let content = edit.add_text_content(page, At::End).unwrap();
            edit.set(
                content,
                &SceneSourceText {
                    text: Authored::user(text.to_owned()),
                    language: None,
                },
            )
            .unwrap();
            edit.set(
                content,
                &SceneTranslation {
                    text: Authored::user(text.to_owned()),
                    language: None,
                },
            )
            .unwrap();
            let layer = edit
                .add_text_layer(
                    page,
                    At::End,
                    content,
                    &SceneTextLayout {
                        origin: Origin::User,
                        kind: TextLayoutKind::Paragraph,
                        angle_degrees: None,
                    },
                )
                .unwrap();
            edit.relate::<koharu_scene::FlowsIn>(layer, bubble).unwrap();
            layers.push(layer);
        }
        session.commit(edit.finish().unwrap()).await.unwrap();
        let mut project = Project::new(session, "test".to_owned());
        let updates = layers.iter().map(|&layer| {
            (
                layer,
                SceneGeometry::rectangle(40.0, 50.0, 60.0, 60.0),
                27.0,
            )
        });
        let commit = project.set_geometries(updates).await.unwrap();
        project.record_commit(&commit);
        assert_eq!(project.undo.len(), 1);
        for &layer in &layers {
            assert!(
                project
                    .snapshot()
                    .component::<SceneGeometry>(layer)
                    .unwrap()
                    .is_some()
            );
            assert_eq!(
                project
                    .snapshot()
                    .text_layer(layer)
                    .unwrap()
                    .layout()
                    .unwrap()
                    .angle_degrees,
                Some(27.0)
            );
        }
        project.undo().await.unwrap();
        for &layer in &layers {
            assert!(
                project
                    .snapshot()
                    .component::<SceneGeometry>(layer)
                    .unwrap()
                    .is_none()
            );
            assert_eq!(
                project
                    .snapshot()
                    .text_layer(layer)
                    .unwrap()
                    .layout()
                    .unwrap()
                    .angle_degrees,
                None
            );
        }
        assert!(project.undo.is_empty());
        project.redo().await.unwrap();
        project
            .set_geometry(vec![GeometryUpdate {
                layer: layers[0],
                points: None,
            }])
            .await
            .unwrap();
        assert!(
            project
                .snapshot()
                .component::<SceneGeometry>(layers[0])
                .unwrap()
                .is_none()
        );
        assert!(
            project
                .snapshot()
                .component::<SceneGeometry>(layers[1])
                .unwrap()
                .is_some()
        );
    }

    #[tokio::test]
    async fn pipeline_commit_rebases_or_yields_to_the_manual_edit() {
        let mut session = Session::memory().await.unwrap();
        let mut setup = session.snapshot().edit();
        let pipeline_page = setup
            .add_page(PageDraft::new("pipeline", 100.0, 100.0), At::End)
            .unwrap();
        let manual_page = setup
            .add_page(PageDraft::new("manual", 100.0, 100.0), At::End)
            .unwrap();
        session.commit(setup.finish().unwrap()).await.unwrap();
        let mut project = Project::new(session, "test".to_owned());

        let base = project.snapshot();
        let pipeline = base
            .patch(|edit| {
                edit.set_page(
                    pipeline_page,
                    PageDraft::new("pipeline result", 100.0, 100.0),
                )
            })
            .unwrap();
        let manual = base
            .patch(|edit| edit.set_page(manual_page, PageDraft::new("manual edit", 100.0, 100.0)))
            .unwrap();
        project.commit(manual).await.unwrap();

        let commit = project.commit_rebased(pipeline).await.unwrap().unwrap();
        assert_eq!(
            commit
                .snapshot
                .page(pipeline_page)
                .unwrap()
                .page()
                .unwrap()
                .label,
            "pipeline result"
        );
        assert_eq!(
            commit
                .snapshot
                .page(manual_page)
                .unwrap()
                .page()
                .unwrap()
                .label,
            "manual edit"
        );

        let base = project.snapshot();
        let pipeline = base
            .patch(|edit| {
                edit.set_page(
                    pipeline_page,
                    PageDraft::new("stale pipeline", 100.0, 100.0),
                )
            })
            .unwrap();
        let manual = base
            .patch(|edit| {
                edit.set_page(pipeline_page, PageDraft::new("latest manual", 100.0, 100.0))
            })
            .unwrap();
        project.commit(manual).await.unwrap();

        assert!(project.commit_rebased(pipeline).await.unwrap().is_none());
        assert_eq!(
            project
                .snapshot()
                .page(pipeline_page)
                .unwrap()
                .page()
                .unwrap()
                .label,
            "latest manual"
        );
    }

    #[test]
    fn raster_strokes_are_continuous_and_erasable() {
        let mut image = RgbaImage::new(32, 16);
        let points = [
            ScenePoint { x: 4.0, y: 8.0 },
            ScenePoint { x: 28.0, y: 8.0 },
        ];
        rasterize_stroke(
            &mut image,
            RasterStrokeMode::Paint,
            [210, 40, 20, 255],
            5.0,
            &points,
        );
        for x in 4..28 {
            assert_eq!(image.get_pixel(x, 8).0, [210, 40, 20, 255]);
        }

        rasterize_stroke(
            &mut image,
            RasterStrokeMode::Erase,
            [0, 0, 0, 0],
            5.0,
            &[ScenePoint { x: 16.0, y: 8.0 }],
        );
        assert_eq!(image.get_pixel(16, 8)[3], 0);
        assert_eq!(image.get_pixel(4, 8)[3], 255);

        let mut white = RgbaImage::new(4, 4);
        rasterize_stroke(
            &mut white,
            RasterStrokeMode::Paint,
            [255, 255, 255, 255],
            3.0,
            &[ScenePoint { x: 2.0, y: 2.0 }],
        );
        assert_eq!(white.get_pixel(2, 2).0, [255, 255, 255, 255]);
    }
}
