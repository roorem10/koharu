use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::OnceLock,
};

use booharu_context::ProjectContext;
use koharu_scene::{EntityId, ProjectId, Revision, Snapshot};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

use super::{QcFinding, QcReport};

const FILE: &str = "booharu-qc-cache.json";
struct OpenCache {
    directory: Option<PathBuf>,
    results: Cache,
}

static CACHE: OnceLock<Mutex<Option<OpenCache>>> = OnceLock::new();
pub(super) static CHECK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(super) struct PageResults {
    pub signature: u64,
    pub findings: Vec<QcFinding>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(super) struct Cache {
    version: u8,
    project: ProjectId,
    revision: Revision,
    pub pages: BTreeMap<EntityId, PageResults>,
}

impl Cache {
    fn empty(snapshot: &Snapshot) -> Self {
        Self {
            version: 1,
            project: snapshot.project_id(),
            revision: snapshot.revision(),
            pages: BTreeMap::new(),
        }
    }

    fn read(directory: Option<&Path>, snapshot: &Snapshot) -> Self {
        directory
            .and_then(|dir| std::fs::read(dir.join(FILE)).ok())
            .and_then(|bytes| serde_json::from_slice::<Self>(&bytes).ok())
            .filter(|cache| {
                cache.version == 1
                    && cache.project == snapshot.project_id()
                    && cache.revision == snapshot.revision()
            })
            .unwrap_or_else(|| Self::empty(snapshot))
    }

    pub fn report(&self, snapshot: &Snapshot, context: &ProjectContext) -> QcReport {
        let mut report = QcReport {
            revision: snapshot.revision(),
            findings: Vec::new(),
            hidden: Vec::new(),
            pages_checked: 0,
        };
        for page in snapshot.pages() {
            if let Some(results) = self.pages.get(&page.id()) {
                report.pages_checked += 1;
                for finding in &results.findings {
                    if snapshot.text_layer(finding.layer).is_err() {
                        continue;
                    }
                    if super::finding_hidden(context, finding) {
                        report.hidden.push(finding.clone());
                    } else {
                        report.findings.push(finding.clone());
                    }
                }
            }
        }
        report
    }

    pub fn save(&mut self, directory: Option<&Path>, snapshot: &Snapshot, complete: bool) {
        self.pages.retain(|page, _| snapshot.page(*page).is_ok());
        // Only a full sweep certifies the scene revision for a future process.
        if complete {
            self.revision = snapshot.revision();
        }
        if let Some(directory) = directory
            && let Err(error) = booharu_context::atomic_json(&directory.join(FILE), self)
        {
            tracing::debug!(%error, "could not save disposable QC cache");
        }
    }
}

pub(super) fn with_cache<T>(
    snapshot: &Snapshot,
    directory: Option<&Path>,
    action: impl FnOnce(&mut Cache) -> T,
) -> T {
    let mut memory = CACHE.get_or_init(|| Mutex::new(None)).lock();
    if memory.as_ref().is_none_or(|entry| {
        entry.results.project != snapshot.project_id() || entry.directory.as_deref() != directory
    }) {
        *memory = Some(OpenCache {
            directory: directory.map(Path::to_path_buf),
            results: Cache::read(directory, snapshot),
        });
    }
    action(&mut memory.as_mut().expect("cache initialized").results)
}

#[cfg(test)]
mod tests {
    use super::*;
    use koharu_scene::{At, PageDraft, Session};

    #[tokio::test]
    async fn cache_roundtrips_and_ignores_bad_files_other_projects_and_revisions() {
        let directory = tempfile::tempdir().unwrap();
        let mut session = Session::memory().await.unwrap();
        let snapshot = session.snapshot();
        let mut cache = Cache::empty(&snapshot);
        let patch = snapshot
            .patch(|edit| {
                edit.add_page(PageDraft::new("page", 8.0, 8.0), At::End)?;
                Ok(())
            })
            .unwrap();
        session.commit(patch).await.unwrap();
        let snapshot = session.snapshot();
        let page = snapshot.pages().next().unwrap().id();
        cache.pages.insert(
            page,
            PageResults {
                signature: 42,
                findings: vec![],
            },
        );
        let flag = booharu_context::Flag::Review {
            message: "Review this line".into(),
            page: 1,
        };
        cache
            .pages
            .get_mut(&page)
            .unwrap()
            .findings
            .push(QcFinding {
                page,
                page_label: "page".into(),
                layer: EntityId::new(),
                check: super::super::QcCheck::Flag,
                detail: "Review this line".into(),
                translation: "Hello".into(),
                flag: Some(flag.clone()),
            });
        cache.save(Some(directory.path()), &snapshot, true);
        assert_eq!(
            Cache::read(Some(directory.path()), &snapshot).pages[&page].signature,
            42
        );
        let restored = Cache::read(Some(directory.path()), &snapshot);
        assert_eq!(restored.pages[&page].findings[0].translation, "Hello");
        assert_eq!(restored.pages[&page].findings[0].flag, Some(flag));
        let old = serde_json::to_vec(&cache).unwrap();
        cache.revision = cache.revision.next().unwrap();
        std::fs::write(
            directory.path().join(FILE),
            serde_json::to_vec(&cache).unwrap(),
        )
        .unwrap();
        assert!(
            Cache::read(Some(directory.path()), &snapshot)
                .pages
                .is_empty()
        );
        std::fs::write(directory.path().join(FILE), old).unwrap();
        let other = Session::memory().await.unwrap();
        assert!(
            Cache::read(Some(directory.path()), &other.snapshot())
                .pages
                .is_empty()
        );
        std::fs::write(directory.path().join(FILE), b"broken").unwrap();
        assert!(
            Cache::read(Some(directory.path()), &snapshot)
                .pages
                .is_empty()
        );
    }
}
