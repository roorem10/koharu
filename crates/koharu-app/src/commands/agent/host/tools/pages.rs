//! Reading the project and managing its pages.

use super::*;

pub(super) fn tools() -> Vec<Tool> {
    vec![
            definition::<InspectProject>(
                "inspect_project",
                "Read the latest complete semantic project state after edits. This does not include page images.",
            ),
            definition::<ViewPage>(
                "view_page",
                "Render and inspect one page image. Call this only for pages whose visual appearance matters to the request.",
            ),
            definition::<RenamePage>("rename_page", "Rename a project page."),
            definition::<MovePage>("move_page", "Move a page to a zero-based project index."),
            definition::<DeletePages>("delete_pages", "Delete pages and their contents."),
    ]
}

pub(super) async fn invoke(
    host: &KoharuHost,
    call: &ToolCall,
    _control: &Control,
) -> Result<Option<Invocation>> {
    let result: Result<Invocation> = match call.name.as_str() {
        "inspect_project" => {
            let _: InspectProject = arguments(call)?;
            Invocation::read(host.project_context().await?)
        }
        "view_page" => {
            let arguments: ViewPage = arguments(call)?;
            let page = entity(&arguments.page)?;
            let (label, snapshot) = {
                let current = host.handle.state::<CurrentProject>();
                let current = current.project.lock().await;
                let project = current.as_ref().context("no project is open")?;
                let snapshot = project.snapshot();
                let label = snapshot.page(page)?.page()?.label;
                (label, snapshot)
            };
            let desktop = host.handle.state::<Desktop>();
            let renderer = desktop.renderer();
            let rasterizer = desktop.rasterizer().await?;
            let bytes =
                output::rendered_preview(&renderer, rasterizer, &snapshot, page).await?;
            Ok(
                Invocation::read(json!({ "page": page, "label": label }))?.with_image(
                    format!("Rendered page {label} ({page})"),
                    format!("data:image/webp;base64,{}", STANDARD.encode(bytes)),
                ),
            )
        }
        "rename_page" => {
            let arguments: RenamePage = arguments(call)?;
            let page = entity(&arguments.page)?;
            host.mutate(|project| {
                Box::pin(async move {
                    Ok((
                        project.rename_page(page, arguments.label).await?,
                        json!({ "page": page }),
                    ))
                })
            })
            .await
        }
        "move_page" => {
            let arguments: MovePage = arguments(call)?;
            let page = entity(&arguments.page)?;
            host.mutate(|project| {
                Box::pin(async move {
                    Ok((
                        project.move_page(page, arguments.index).await?,
                        json!({ "page": page }),
                    ))
                })
            })
            .await
        }
        "delete_pages" => {
            let arguments: DeletePages = arguments(call)?;
            let pages = entities(&arguments.pages)?;
            host.mutate(|project| {
                Box::pin(async move {
                    Ok((
                        project.delete_pages(pages.clone()).await?,
                        json!({ "pages": pages }),
                    ))
                })
            })
            .await
        }
        _ => return Ok(None),
    };
    result.map(Some)
}

#[derive(Deserialize, JsonSchema)]
struct InspectProject {}

#[derive(Deserialize, JsonSchema)]
struct ViewPage {
    page: String,
}

#[derive(Deserialize, JsonSchema)]
struct RenamePage {
    page: String,
    label: String,
}

#[derive(Deserialize, JsonSchema)]
struct MovePage {
    page: String,
    index: usize,
}

#[derive(Deserialize, JsonSchema)]
struct DeletePages {
    pages: Vec<String>,
}
