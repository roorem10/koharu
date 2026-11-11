//! One text box per speech bubble: finding split bubbles and joining them.

use super::*;

pub(super) fn tools() -> Vec<Tool> {
    vec![
            definition::<crate::commands::boxes::FindSplitBubblesArgs>(
                "find_split_bubbles",
                "Find bubbles with multiple text boxes, including box IDs, source text, and translations. Pass a page ID to inspect one page; omit it to inspect the project. Report split bubbles to the user before joining unless the user asked to fix them.",
            ),
            definition::<crate::commands::boxes::JoinBubbleBoxesArgs>(
                "join_bubble_boxes",
                "Join all boxes in the selected text layer's bubble in reading order, preserving their text in one undoable edit. Report the split bubbles first unless the user asked to fix them.",
            ),
    ]
}

pub(super) async fn invoke(
    host: &KoharuHost,
    call: &ToolCall,
    _control: &Control,
) -> Result<Option<Invocation>> {
    let result: Result<Invocation> = match call.name.as_str() {
        "find_split_bubbles" => {
            let arguments: crate::commands::boxes::FindSplitBubblesArgs = arguments(call)?;
            let page = arguments.page.as_deref().map(entity).transpose()?;
            let current = host.handle.state::<CurrentProject>();
            let current = current.project.lock().await;
            let project = current.as_ref().context("no project is open")?;
            Invocation::read(project.find_split_bubbles(page)?)
        }
        "join_bubble_boxes" => {
            let arguments: crate::commands::boxes::JoinBubbleBoxesArgs = arguments(call)?;
            let layer = entity(&arguments.layer)?;
            host.mutate_optional(|project| {
                Box::pin(async move {
                    let (commit, kept) = project.join_bubble_boxes(layer).await?;
                    Ok((commit, json!({ "layer": kept })))
                })
            })
            .await
        }
        _ => return Ok(None),
    };
    result.map(Some)
}
