use super::*;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct RunPageCheck {
    /// Page IDs in this project; empty means all pages in project order.
    pages: Vec<String>,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct ApplyPageCheckFix {
    page: String,
    finding_id: String,
}

impl KoharuHost {
    pub(super) async fn page_check_tool(
        &self,
        call: &ToolCall,
        control: &Control,
    ) -> Result<Invocation> {
        use crate::commands::page_check;

        if control.is_cancelled() {
            bail!("Page check tool call was cancelled");
        }
        match call.name.as_str() {
            "run_page_check" => {
                let arguments: RunPageCheck = arguments(call)?;
                let job =
                    page_check::start_page_check(self.handle.clone(), entities(&arguments.pages)?)
                        .await
                        .map_err(|error| error.0)?;
                Invocation::changed(json!({ "job": job }))
            }
            "get_page_check_results" => {
                let _: InspectProject = arguments(call)?;
                let report =
                    page_check::get_page_check_results(self.handle.state::<CurrentProject>())
                        .await
                        .map_err(|error| error.0)?;
                Invocation::read(report)
            }
            "apply_page_check_fix" => {
                let arguments: ApplyPageCheckFix = arguments(call)?;
                let changed = page_check::fixes::apply_page_check_fix(
                    self.handle.clone(),
                    entity(&arguments.page)?,
                    arguments.finding_id,
                )
                .await
                .map_err(|error| error.0)?;
                if changed {
                    Invocation::changed(json!({ "applied": true }))
                } else {
                    Invocation::read(json!({ "applied": false }))
                }
            }
            name => bail!("unknown Page check tool {name}"),
        }
    }
}
