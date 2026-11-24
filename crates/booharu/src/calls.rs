//! Asking a model for JSON a schema describes, with the pages it sees, as
//! Read chapter and Explain this line do.
//!
//! A question goes as Koharu's translator sends a page's turn: the page
//! images, then the text, in one user message, with no history, asking for
//! answers through koharu-llm's `answers`, which holds each vendor to the
//! schema its own way; a local model gets an explicit answer length, as its
//! default is a quarter of its context. It races its job's cancel, a Stop of
//! the pipeline's, which the agent's and MCP's stop tool reach only through
//! the pipeline's count of Stops, looked at every 250 ms, and, for a hosted
//! model alone, ten minutes ([`Unanswered`]); a cancel or a Stop answers
//! none, never an error.
//!
//! The schema sent is Koharu's style, every property required, with no
//! numeric bounds, no `oneOf` and no nullable types, so rig's strict rewrite
//! leaves it as it is. An answer is read strictly by a second schema with
//! the bounds ([`parse`]): it must be a JSON object that fits it and reads
//! as the answer's type; nothing is repaired.

use std::{
    future::{Future, IntoFuture as _},
    sync::Arc,
    time::Duration,
};

use anyhow::{Context as _, Result, bail, ensure};
use base64::Engine as _;
use image::{DynamicImage, codecs::jpeg::JpegEncoder};
use koharu_llm::rig::{
    catalog::ModelSpec,
    completion::{
        Message,
        message::{DocumentSourceKind, Image, ImageMediaType, UserContent},
    },
    driver::DynModel,
    operation::Completion,
    tool::validate::validate,
};
use koharu_pipeline::Pipeline;
use koharu_scene::{PageId, Session};
use parking_lot::Mutex;
use serde::de::DeserializeOwned;
use serde_json::Value;
use tokio_util::sync::CancellationToken;

/// How long a hosted model has to answer one question, as the translator's
/// hosted requests have.
pub const ANSWER_LIMIT: Duration = Duration::from_secs(10 * 60);

/// How often a question looks for a Stop of the pipeline's.
const STOP_POLL: Duration = Duration::from_millis(250);

/// The quality of the JPEG a model sees a page as: `image`'s default, the
/// translator's.
const QUALITY: u8 = 75;

/// The most schema problems an error about an answer names.
const PROBLEMS: usize = 5;

/// A hosted model's question that went unanswered for [`ANSWER_LIMIT`].
#[derive(Debug, thiserror::Error)]
#[error("The model didn't answer within 10 minutes.")]
pub struct Unanswered;

/// What ends a question before its answer: its job's cancel, and a Stop of
/// the pipeline's since `since`, the pipeline's count of Stops as the job
/// began.
#[derive(Clone)]
pub struct Stops {
    pub cancel: CancellationToken,
    pub pipeline: Pipeline,
    pub since: u64,
}

impl Stops {
    /// The stops of a job with `cancel` beginning now on `pipeline`.
    pub fn new(cancel: CancellationToken, pipeline: Pipeline) -> Self {
        let since = pipeline.stops();
        Self {
            cancel,
            pipeline,
            since,
        }
    }

    /// Whether the job was cancelled, or the pipeline stopped since it
    /// began.
    pub fn stopped(&self) -> bool {
        self.cancel.is_cancelled() || self.pipeline.stops() != self.since
    }

    /// Waits until the job is cancelled or the pipeline stops.
    pub async fn wait(&self) {
        while !self.stopped() {
            tokio::select! {
                () = self.cancel.cancelled() => return,
                () = tokio::time::sleep(STOP_POLL) => {}
            }
        }
    }
}

/// A question to a model.
#[derive(Clone, Debug)]
pub struct Question {
    pub system: String,
    /// The pages it sees, each a JPEG in base 64 as [`sight`] makes it,
    /// which go before its text.
    pub images: Vec<String>,
    pub text: String,
    /// The schema the model keeps its answer to, in Koharu's style.
    pub schema: Value,
    /// The most tokens it may answer with; none for the model's own limit.
    /// A local model needs one large enough for the answer, as its own is a
    /// quarter of its context.
    pub max_tokens: Option<u64>,
}

/// The answer `model` gives `question`, as it wrote it; none once `stops`
/// stops it, which drops the request. A question with images fails, sending
/// nothing, when the model reads none, as rig would drop them silently. A
/// hosted model that doesn't answer within [`ANSWER_LIMIT`] fails as
/// [`Unanswered`].
pub async fn ask(
    model: &DynModel<Completion>,
    question: Question,
    stops: &Stops,
) -> Result<Option<String>> {
    let spec = model.spec();
    if !question.images.is_empty() {
        ensure!(
            reads_images(model),
            "{} reads no images",
            spec.map_or_else(|| "the model".to_owned(), ModelSpec::reference)
        );
    }
    let mut content: Vec<UserContent> = question
        .images
        .into_iter()
        .map(|image| {
            UserContent::Image(Image {
                data: DocumentSourceKind::Base64(image),
                media_type: Some(ImageMediaType::JPEG),
                detail: None,
                native: None,
            })
        })
        .collect();
    content.push(UserContent::text(question.text));
    let schema =
        schemars::Schema::try_from(question.schema).context("the answer's schema isn't valid")?;
    let mut builder = koharu_llm::answers(model, schema).preamble(question.system);
    if let Some(tokens) = question.max_tokens {
        builder = builder.max_tokens(tokens);
    }
    let request = builder.build().prompt(Message::from(content));
    tokio::select! {
        biased;
        () = stops.wait() => Ok(None),
        response = within(ANSWER_LIMIT, hosted(spec), request.into_future()) => {
            Ok(Some(response?.output()))
        }
    }
}

/// Checks that `model`, connected from `reference`, can answer a question
/// about the chapter: a model trained only to translate can't; with
/// `images`, neither can one that reads no images.
pub fn usable(model: &DynModel<Completion>, reference: &str, images: bool) -> Result<()> {
    let translates = model
        .spec()
        .is_some_and(|spec| spec.details.kind.as_deref() == Some(koharu_llm::TRANSLATION));
    ensure!(
        !translates,
        "{reference} only translates, so it can't analyze or explain the chapter"
    );
    ensure!(
        !images || reads_images(model),
        "{reference} reads no images"
    );
    Ok(())
}

/// Whether `model` reads images, as its spec says once it's connected: a
/// local model only with its projector loaded.
pub fn reads_images(model: &DynModel<Completion>) -> bool {
    model.spec().is_some_and(|spec| spec.input.image)
}

/// `answer` as `T`: a JSON object that fits `check`, the schema with its
/// bounds, and reads as `T`. Nothing is repaired.
pub fn parse<T: DeserializeOwned>(answer: &str, check: &Value) -> Result<T> {
    let value: Value = serde_json::from_str(answer).context("the answer isn't JSON")?;
    ensure!(value.is_object(), "the answer isn't a JSON object");
    if let Err(problems) = validate(check, &value) {
        let mut named: Vec<String> = problems
            .iter()
            .take(PROBLEMS)
            .map(ToString::to_string)
            .collect();
        if problems.len() > PROBLEMS {
            named.push(format!("{} more", problems.len() - PROBLEMS));
        }
        bail!("the answer doesn't fit its schema: {}", named.join("; "));
    }
    serde_json::from_value(value).context("the answer doesn't read")
}

/// `scan` as a model sees it: its longest side fitted to `longest` pixels,
/// never enlarged, each pixel the average of the scan's under it, as a JPEG
/// of quality 75 in base 64.
pub fn sight(scan: &DynamicImage, longest: u32) -> Result<String> {
    let page = match scan.width().max(scan.height()) > longest {
        true => scan.thumbnail(longest, longest).to_rgb8(),
        false => scan.to_rgb8(),
    };
    let mut bytes = Vec::new();
    page.write_with_encoder(JpegEncoder::new_with_quality(&mut bytes, QUALITY))?;
    Ok(base64::engine::general_purpose::STANDARD.encode(bytes))
}

/// The sight of `page`'s scan in the project `session` holds, at `longest`
/// pixels, read and encoded on a blocking thread, which holds the session
/// only while it reads the scan's bytes; none when the page has no scan.
pub async fn page_sight(
    session: Arc<Mutex<Session>>,
    page: PageId,
    longest: u32,
) -> Result<Option<String>> {
    tokio::task::spawn_blocking(move || {
        crate::boxes::read_scan(|| session.lock(), page)?
            .map(|scan| sight(&scan.image, longest))
            .transpose()
    })
    .await?
}

/// Whether the model `spec` describes is hosted: any vendor's but Koharu's
/// own, which waits only on this computer.
fn hosted(spec: Option<&ModelSpec>) -> bool {
    spec.is_none_or(|spec| spec.provider.vendor() != koharu_llm::KOHARU)
}

/// `request`, failing as [`Unanswered`] once `limit` passes when `limited`.
async fn within<T, E: Into<anyhow::Error>>(
    limit: Duration,
    limited: bool,
    request: impl Future<Output = Result<T, E>>,
) -> Result<T> {
    if !limited {
        return request.await.map_err(Into::into);
    }
    match tokio::time::timeout(limit, request).await {
        Ok(answer) => answer.map_err(Into::into),
        Err(_) => Err(Unanswered.into()),
    }
}

#[cfg(test)]
mod tests {
    use image::{GenericImageView as _, Rgb, RgbImage};
    use serde::Deserialize;
    use serde_json::json;

    use super::*;

    #[derive(Debug, Deserialize, PartialEq)]
    #[serde(deny_unknown_fields)]
    struct Answer {
        text: String,
        sure: f32,
    }

    fn check() -> Value {
        json!({
            "type": "object",
            "properties": {
                "text": {"type": "string"},
                "sure": {"type": "number", "minimum": 0, "maximum": 1},
            },
            "required": ["text", "sure"],
            "additionalProperties": false,
        })
    }

    #[test]
    fn an_answer_is_read_strictly() {
        let read: Answer = parse(r#"{"text": "Hi", "sure": 0.5}"#, &check()).expect("an answer");
        assert_eq!(
            read,
            Answer {
                text: "Hi".to_owned(),
                sure: 0.5
            }
        );
        let refused = [
            ("{\"text\": \"Hi\"", "isn't JSON"),
            (
                "```json\n{\"text\": \"Hi\", \"sure\": 0.5}\n```",
                "isn't JSON",
            ),
            ("[1]", "isn't a JSON object"),
            (r#"{"text": "Hi"}"#, "/sure: missing"),
            (r#"{"text": "Hi", "sure": 0.5, "more": 1}"#, "doesn't fit"),
            (r#"{"text": "Hi", "sure": 1.5}"#, "/sure"),
            (r#"{"text": 1, "sure": 0.5}"#, "/text"),
        ];
        for (answer, why) in refused {
            let error = parse::<Answer>(answer, &check()).expect_err(answer);
            assert!(format!("{error:#}").contains(why), "{answer}: {error:#}");
        }
    }

    #[test]
    fn a_page_is_fitted_never_enlarged() -> Result<()> {
        let decoded = |sight: String| -> Result<DynamicImage> {
            let bytes = base64::engine::general_purpose::STANDARD.decode(sight)?;
            Ok(image::load_from_memory(&bytes)?)
        };
        let tall = DynamicImage::ImageRgb8(RgbImage::from_pixel(1000, 2000, Rgb([255, 0, 0])));
        assert_eq!(decoded(sight(&tall, 1024)?)?.dimensions(), (512, 1024));
        let small = DynamicImage::ImageRgb8(RgbImage::from_pixel(300, 200, Rgb([0, 0, 255])));
        assert_eq!(decoded(sight(&small, 1024)?)?.dimensions(), (300, 200));
        Ok(())
    }

    #[tokio::test]
    async fn a_stop_or_a_cancel_ends_the_wait() {
        let pipeline = Pipeline::new(koharu_pipeline::PipelineConfig::default());
        let stops = Stops::new(CancellationToken::new(), pipeline.clone());
        assert!(!stops.stopped());
        pipeline.stop();
        assert!(stops.stopped());
        tokio::time::timeout(Duration::from_secs(5), stops.wait())
            .await
            .expect("a Stop ends the wait");

        let cancel = CancellationToken::new();
        let stops = Stops::new(cancel.clone(), pipeline);
        cancel.cancel();
        tokio::time::timeout(Duration::from_secs(5), stops.wait())
            .await
            .expect("a cancel ends the wait");
    }

    #[test]
    fn only_a_model_of_koharus_own_isnt_hosted() {
        use koharu_llm::rig::providers::registry::ProviderId;

        let spec =
            |vendor: &str| ModelSpec::new(ProviderId::declare(vendor).expect("a vendor"), "model");
        assert!(!hosted(Some(&spec(koharu_llm::KOHARU))));
        for vendor in ["openai", "lmstudio", "ollama"] {
            assert!(hosted(Some(&spec(vendor))), "{vendor}");
        }
        assert!(hosted(None));
    }

    #[tokio::test]
    async fn only_a_hosted_question_has_a_time_limit() {
        let limit = Duration::from_millis(20);
        let stalled = std::future::pending::<Result<(), anyhow::Error>>();
        let error = within(limit, true, stalled).await.expect_err("the limit");
        assert!(error.is::<Unanswered>());
        assert_eq!(
            error.to_string(),
            "The model didn't answer within 10 minutes."
        );
        let slow = async {
            tokio::time::sleep(limit * 3).await;
            Ok::<_, anyhow::Error>(7)
        };
        assert_eq!(within(limit, false, slow).await.ok(), Some(7));
    }
}
