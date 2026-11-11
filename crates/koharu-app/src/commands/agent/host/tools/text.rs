//! Adding text boxes and editing their source text and translation.

use super::*;

pub(super) fn tools() -> Vec<Tool> {
    vec![
            definition::<AddTextBox>("add_text_box", "Add a paragraph text box to a page."),
            definition::<SetText>("set_source_text", "Replace an element's source text."),
            definition::<SetTranslation>(
                "set_translation",
                "Set an element's translation, or pass null to remove it.",
            ),
    ]
}

pub(super) async fn invoke(
    host: &KoharuHost,
    call: &ToolCall,
    _control: &Control,
) -> Result<Option<Invocation>> {
    let result: Result<Invocation> = match call.name.as_str() {
        "add_text_box" => {
            let arguments: AddTextBox = arguments(call)?;
            let page = entity(&arguments.page)?;
            let frame = Frame {
                x: arguments.x,
                y: arguments.y,
                width: arguments.width,
                height: arguments.height,
                angle_degrees: arguments.angle_degrees,
            };
            host.mutate(|project| {
                Box::pin(async move {
                    let (commit, element) = project.add_text_box(page, frame).await?;
                    Ok((commit, json!({ "page": page, "element": element })))
                })
            })
            .await
        }
        "set_source_text" => {
            let arguments: SetText = arguments(call)?;
            let element = entity(&arguments.element)?;
            host.mutate(|project| {
                Box::pin(async move {
                    Ok((
                        project.set_source_text(element, arguments.text).await?,
                        json!({ "element": element }),
                    ))
                })
            })
            .await
        }
        "set_translation" => {
            let arguments: SetTranslation = arguments(call)?;
            let element = entity(&arguments.element)?;
            host.mutate(|project| {
                Box::pin(async move {
                    Ok((
                        project.set_translation(element, arguments.text).await?,
                        json!({ "element": element }),
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
struct AddTextBox {
    page: String,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    #[serde(default)]
    angle_degrees: f32,
}

#[derive(Deserialize, JsonSchema)]
struct SetText {
    element: String,
    text: String,
}

#[derive(Deserialize, JsonSchema)]
struct SetTranslation {
    element: String,
    text: Option<String>,
}
