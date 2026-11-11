//! Geometry, visibility, order and removal of page elements.

use super::*;

pub(super) fn tools() -> Vec<Tool> {
    vec![
            definition::<SetGeometry>(
                "set_geometry",
                "Replace an element's page-space polygon geometry.",
            ),
            definition::<SetVisibility>(
                "set_visibility",
                "Change visibility and/or opacity for elements.",
            ),
            definition::<DeleteElements>("delete_elements", "Delete elements and descendants."),
            definition::<MoveElement>(
                "move_element",
                "Move an element under a page or element at a zero-based index.",
            ),
    ]
}

pub(super) async fn invoke(
    host: &KoharuHost,
    call: &ToolCall,
    _control: &Control,
) -> Result<Option<Invocation>> {
    let result: Result<Invocation> = match call.name.as_str() {
        "set_geometry" => {
            let arguments: SetGeometry = arguments(call)?;
            let element = entity(&arguments.element)?;
            let points = arguments
                .points
                .into_iter()
                .map(|point| Point {
                    x: point.x,
                    y: point.y,
                })
                .collect();
            host.mutate(|project| {
                Box::pin(async move {
                    Ok((
                        project
                            .set_geometry(vec![GeometryUpdate {
                                layer: element,
                                points: Some(points),
                            }])
                            .await?,
                        json!({ "element": element }),
                    ))
                })
            })
            .await
        }
        "set_visibility" => {
            let arguments: SetVisibility = arguments(call)?;
            let elements = entities(&arguments.elements)?;
            host.mutate(|project| {
                Box::pin(async move {
                    Ok((
                        project
                            .set_visibility(
                                elements.clone(),
                                arguments.visible,
                                arguments.opacity,
                            )
                            .await?,
                        json!({ "elements": elements }),
                    ))
                })
            })
            .await
        }
        "delete_elements" => {
            let arguments: DeleteElements = arguments(call)?;
            let elements = entities(&arguments.elements)?;
            host.mutate(|project| {
                Box::pin(async move {
                    Ok((
                        project.delete_layers(elements.clone()).await?,
                        json!({ "elements": elements }),
                    ))
                })
            })
            .await
        }
        "move_element" => {
            let arguments: MoveElement = arguments(call)?;
            let element = entity(&arguments.element)?;
            let parent = entity(&arguments.parent)?;
            host.mutate(|project| {
                Box::pin(async move {
                    Ok((
                        project.move_layer(element, parent, arguments.index).await?,
                        json!({ "element": element, "parent": parent }),
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
struct SetGeometry {
    element: String,
    points: Vec<AgentPoint>,
}

#[derive(Deserialize, JsonSchema)]
struct AgentPoint {
    x: f64,
    y: f64,
}

#[derive(Deserialize, JsonSchema)]
struct SetVisibility {
    elements: Vec<String>,
    visible: Option<bool>,
    opacity: Option<f32>,
}

#[derive(Deserialize, JsonSchema)]
struct DeleteElements {
    elements: Vec<String>,
}

#[derive(Deserialize, JsonSchema)]
struct MoveElement {
    element: String,
    parent: String,
    index: usize,
}
