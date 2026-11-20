//! Booharu's tests of the Translation stage's turn when its run asks the
//! model which words to stress (S10-e), mounted by a marked line in
//! `mod.rs`, as they drive the stage's own turn, private to it, with rig's
//! scripted model in place of a real one.

use anyhow::{Context as _, Result};
use booharu_core::{emphasis::EmphasisSettings, settings::Snapshot};
use parking_lot::Mutex;
use rig_core::{
    catalog::ModelSpec,
    completion::{CompletionRequest, Message},
    providers::registry::ProviderId,
    test_utils::{MockCompletionModel, MockTurn},
};
use tokio_util::sync::CancellationToken;

use super::{
    Conversation, Conversations, TranslatorConfig, system,
    turn::{Turn, keys},
};
use crate::{
    Stage,
    booharu::{
        emphasis::{INSTRUCTION, asked},
        emphasis_schema,
    },
};

/// A model giving each of `answers` in turn, as rig's scripted model does,
/// under a spec the conversation's context can fit.
fn model(answers: &[&str]) -> MockCompletionModel {
    let vendor = ProviderId::declare("mock").expect("a vendor");
    let spec = ModelSpec::new(vendor, "mock").with_context_window(16_384);
    MockCompletionModel::from_turns(answers.iter().map(|answer| MockTurn::text(*answer)))
        .priced_as(spec)
}

/// A project's translation conversation, new, as its first page finds it.
fn conversation() -> Result<tokio::sync::OwnedMutexGuard<Conversation>> {
    let project = "00000000-0000-0000-0000-000000000001".parse()?;
    Conversations::default()
        .lock(project)
        .context("a conversation no one holds")
}

/// The system text `request` sends.
fn preamble(request: &CompletionRequest) -> Option<&str> {
    request
        .chat_history
        .iter()
        .find_map(|message| match &**message {
            Message::System { content } => Some(content.as_str()),
            _ => None,
        })
}

/// What the turn of a page of `texts`, its system text `system`, takes from
/// `model` in `conversation`, made as the stage makes it: asking for the
/// words to stress into `picks` when given one.
async fn take(
    model: &MockCompletionModel,
    system: String,
    texts: &[String],
    picks: Option<&Mutex<Vec<Vec<String>>>>,
    conversation: &mut Conversation,
) -> Result<Option<Vec<String>>> {
    let cancel = CancellationToken::new();
    Turn::new(system, texts, None, None, &cancel)
        .asking_emphasis(picks)
        .take(conversation, &model.clone().erase())
        .await
}

fn owned(texts: &[&str]) -> Vec<String> {
    texts.iter().map(|text| (*text).to_owned()).collect()
}

#[tokio::test]
async fn a_turn_asking_for_emphasis_asks_for_its_shape_and_keeps_the_words_apart() -> Result<()> {
    let texts = owned(&["まさか!", "どうして"]);
    let model = model(&[r#"{"1": {"text": " No way! ", "emphasis": ["No way"]}, "2": "Why?"}"#]);
    let picks = Mutex::new(Vec::new());
    let mut conversation = conversation()?;
    let taken = take(
        &model,
        "Translate.".to_owned(),
        &texts,
        Some(&picks),
        &mut conversation,
    )
    .await?;
    // The texts as Koharu's turn gives them; the words in the slot.
    assert_eq!(taken, Some(owned(&["No way!", "Why?"])));
    assert_eq!(*picks.lock(), [owned(&["No way"]), Vec::new()]);
    // The request asked for the shape, with the system text as it was.
    let requests = model.requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(
        requests[0].output_schema.as_ref(),
        Some(&emphasis_schema(&keys(2))?)
    );
    assert_eq!(preamble(&requests[0]), Some("Translate."));
    // The conversation's last page holds the plain texts, which the next
    // turn's echo check compares its answer with.
    assert_eq!(
        conversation.last,
        Some((texts, owned(&["No way!", "Why?"])))
    );
    Ok(())
}

#[tokio::test]
async fn the_answer_asked_again_after_an_echo_gives_the_words() -> Result<()> {
    let echo =
        r#"{"1": {"text": "One", "emphasis": ["One"]}, "2": {"text": "Two", "emphasis": []}}"#;
    let again =
        r#"{"1": {"text": "Uno", "emphasis": []}, "2": {"text": "Dos", "emphasis": ["Dos"]}}"#;
    let model = model(&[echo, again]);
    let picks = Mutex::new(Vec::new());
    let mut conversation = conversation()?;
    // The page before, as its turn left it, which the first answer echoes.
    conversation.last = Some((owned(&["いち", "に"]), owned(&["One", "Two"])));
    let taken = take(
        &model,
        "Translate.".to_owned(),
        &owned(&["一", "二"]),
        Some(&picks),
        &mut conversation,
    )
    .await?;
    assert_eq!(model.request_count(), 2);
    assert_eq!(taken, Some(owned(&["Uno", "Dos"])));
    assert_eq!(*picks.lock(), [Vec::new(), owned(&["Dos"])]);
    Ok(())
}

#[tokio::test]
async fn a_run_on_a_translation_model_sends_koharus_request_byte_for_byte() -> Result<()> {
    let koharu = TranslatorConfig {
        instructions: "Keep honorifics.".to_owned(),
        ..TranslatorConfig::default()
    };
    let texts = owned(&["まさか!"]);
    let answer = r#"{"1": "No way!"}"#;
    // Koharu's own turn.
    let plain = model(&[answer]);
    let cancel = CancellationToken::new();
    let theirs = Turn::new(system(&koharu)?, &texts, None, None, &cancel)
        .take(&mut *conversation()?, &plain.clone().erase())
        .await?;
    // The run queued with emphasis on, its model Index-Translate, and the
    // turn the stage makes for it.
    let translating = std::collections::BTreeSet::from([Stage::Translate]);
    let on = || anyhow::Ok(EmphasisSettings::default());
    let (booharu, sent) = asked(Snapshot::default(), koharu.clone(), &translating, on)?;
    assert!(!booharu.asks_emphasis);
    let picks = Mutex::new(Vec::new());
    let ours = model(&[answer]);
    let taken = take(
        &ours,
        system(&sent)?,
        &texts,
        booharu.asks_emphasis.then_some(&picks),
        &mut *conversation()?,
    )
    .await?;
    assert_eq!(taken, theirs);
    assert_eq!(ours.requests(), plain.requests());
    assert!(picks.lock().is_empty());

    // A general model's run asks: the instruction after the Instructions,
    // and the answer's shape.
    let general = TranslatorConfig {
        model: "koharu/qwen3.5-9b".to_owned(),
        ..koharu
    };
    let (booharu, sent) = asked(Snapshot::default(), general, &translating, on)?;
    assert!(booharu.asks_emphasis);
    let asking = model(&[r#"{"1": {"text": "No way!", "emphasis": ["No"]}}"#]);
    take(
        &asking,
        system(&sent)?,
        &texts,
        booharu.asks_emphasis.then_some(&picks),
        &mut *conversation()?,
    )
    .await?;
    let request = &asking.requests()[0];
    let instructed = format!("Keep honorifics.\n\n{INSTRUCTION}");
    assert!(
        preamble(request).is_some_and(|text| text.ends_with(&instructed)),
        "{request:?}"
    );
    assert_eq!(
        request.output_schema.as_ref(),
        Some(&emphasis_schema(&keys(1))?)
    );
    assert_eq!(*picks.lock(), [owned(&["No"])]);
    Ok(())
}

/// How a turn of one text fails on `answer`, asking for the words to stress
/// when `asks`.
async fn failure(answer: &str, asks: bool) -> Result<String> {
    let model = model(&[answer]);
    let picks = Mutex::new(Vec::new());
    let taken = take(
        &model,
        "Translate.".to_owned(),
        &owned(&["まさか!"]),
        asks.then_some(&picks),
        &mut *conversation()?,
    )
    .await;
    match taken {
        Ok(taken) => anyhow::bail!("{answer} passed as {taken:?}"),
        Err(error) => Ok(error.to_string()),
    }
}

#[tokio::test]
async fn an_answer_in_the_shape_fails_koharus_checks_in_koharus_words() -> Result<()> {
    // Each answer as Koharu's turn gets it, then in the shape.
    let cases = [
        (
            r#"{"1": " "}"#,
            r#"{"1": {"text": " ", "emphasis": ["No"]}}"#,
            "the answer leaves text 1 blank",
        ),
        (
            r#"{"1": 3}"#,
            r#"{"1": {"text": 3, "emphasis": []}}"#,
            "the answer for text 1 isn't text",
        ),
        (
            "{}",
            r#"{"1": {"emphasis": ["No"]}}"#,
            "the answer leaves text 1 out",
        ),
        (
            r#"{"1": "No", "2": "Why"}"#,
            r#"{"1": {"text": "No", "emphasis": []}, "2": {"text": "Why", "emphasis": []}}"#,
            "the answer has texts the page doesn't",
        ),
        ("No way!", "No way!", "the answer isn't JSON"),
        (
            r#"["No way!"]"#,
            r#"[{"text": "No way!", "emphasis": []}]"#,
            "the answer isn't a JSON object",
        ),
    ];
    for (plain, shaped, words) in cases {
        assert_eq!(failure(plain, false).await?, words, "{plain}");
        assert_eq!(failure(shaped, true).await?, words, "{shaped}");
    }
    Ok(())
}
