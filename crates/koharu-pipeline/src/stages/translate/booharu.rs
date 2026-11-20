//! Booharu's tests of the Translation stage when its run asks the model
//! which words to stress (S10-e), when it translates a block skewed by hand
//! again (S12-h), and when it translates, or is given back, a block styled
//! by hand before it was translated, mounted by a marked line in `mod.rs`,
//! as they drive the stage's own turn, private to it, and the stage itself,
//! with rig's scripted model in place of a real one.

use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
    time::Duration,
};

use anyhow::{Context as _, Result};
use booharu_core::{
    attrs::{self, Auto, Skew},
    edited,
    emphasis::EmphasisSettings,
    lettering::{CategoryFont, LetteringCategory},
    settings::{DefaultTextStyle, LetteringSettings, Snapshot},
    sfx, skew, status,
    styles::text_hash,
};
use koharu_scene::{
    Block, Content, Created, Direction, Edit, Frame, Layer, LayerId, Op, PageId, Rgba, RichText,
    Session, Text,
};
use kurbo::{Affine, Point};
use parking_lot::{Mutex, RwLock};
use rig_core::{
    catalog::ModelSpec,
    completion::{CompletionRequest, Message},
    providers::registry::ProviderId,
    test_utils::{MockCompletionModel, MockTurn},
};
use serde_json::json;
use tokio_util::sync::CancellationToken;

use super::{
    Conversation, Conversations, TranslatorConfig, system,
    turn::{Turn, keys},
};
use crate::{
    Event, Outcome, PipelineConfig, RunId, Stage, Target,
    booharu::{
        emphasis::{INSTRUCTION, asked},
        emphasis_schema,
        stand_in::Hosted,
    },
    event::Events,
    scheduler::{Command, Plan, Run, Scheduler},
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

/// A project of two pages, each with a block of a line read on it, as the
/// Detection and Recognition stages leave one.
struct Chapter {
    _dir: tempfile::TempDir,
    session: Arc<Mutex<Session>>,
    pages: Vec<PageId>,
    blocks: Vec<LayerId>,
}

impl Chapter {
    fn new(lines: [&str; 2]) -> Result<Self> {
        let dir = tempfile::tempdir()?;
        let mut session = Session::create(dir.path().join("test.khr"))?;
        let mut ops = Vec::new();
        for (index, line) in lines.iter().enumerate() {
            let page = format!("$p{index}");
            ops.push(json!({"op": "add_page", "id": page, "index": index,
                "size": {"width": 200, "height": 200}}));
            let block = json!({"kind": "block", "role": "text", "source": {
                "frame": {"box": {"bounds": {"x": 20.0, "y": 20.0, "width": 20.0, "height": 40.0}}},
                "content": line,
            }, "attributes": {"c2pa": {
                "detection": {"action": "c2pa.created"},
                "ocr": {"action": "c2pa.created"},
            }}});
            ops.push(
                json!({"op": "add_layer", "id": format!("$b{index}"), "page": page,
                "index": 0, "layer": block}),
            );
        }
        let ops = ops
            .into_iter()
            .map(serde_json::from_value)
            .collect::<Result<Vec<Op>, _>>()?;
        let change = session.apply(Edit {
            label: "test".to_owned(),
            ops,
        })?;
        let (mut pages, mut blocks) = (BTreeMap::new(), BTreeMap::new());
        for created in change.created {
            match created {
                Created::Page {
                    id,
                    label: Some(label),
                } => {
                    pages.insert(label, id);
                }
                Created::Layer {
                    id,
                    label: Some(label),
                } => {
                    blocks.insert(label, id);
                }
                _ => {}
            }
        }
        // A block's parts are labelled after it, as `$b0.mask` is.
        let (pages, blocks) = ["0", "1"]
            .iter()
            .map(|at| {
                Some((
                    *pages.get(&format!("p{at}"))?,
                    *blocks.get(&format!("b{at}"))?,
                ))
            })
            .collect::<Option<(Vec<_>, Vec<_>)>>()
            .context("each page and its block")?;
        Ok(Self {
            _dir: dir,
            session: Arc::new(Mutex::new(session)),
            pages,
            blocks,
        })
    }

    /// Translates the pages `target` names, as a run with `booharu`, its
    /// model `model`, through a scheduler of its own, as `Pipeline::run`
    /// queues a run but with no setting read, to its end, done.
    async fn translate(&self, target: Target, model: &str, booharu: Snapshot) -> Result<()> {
        let events = Events::new();
        let config = PipelineConfig {
            translator: TranslatorConfig {
                model: model.to_owned(),
                ..TranslatorConfig::default()
            },
            booharu,
            ..PipelineConfig::default()
        };
        let commands = Scheduler::spawn(Arc::new(RwLock::new(config.clone())), events.clone());
        let stages = BTreeSet::from([Stage::Translate]);
        let project = self.session.lock().project();
        let plan = Plan::new(&project, &target, &stages)?;
        let run = RunId(1);
        events.send(Event::Queued {
            run,
            steps: plan.pages(),
            stages: vec![Stage::Translate],
            pages: true,
        });
        let queued = Run::new(
            run,
            Arc::clone(&self.session),
            project.id(),
            config,
            plan,
            &stages,
        );
        commands
            .send(Command::Run(queued))
            .map_err(|_| anyhow::anyhow!("the scheduler ended"))?;
        let told = events
            .history(run, Duration::from_secs(60))
            .await
            .context("a history")?;
        let done = Event::Ended {
            run,
            outcome: Outcome::Done,
        };
        anyhow::ensure!(told.last() == Some(&done), "{told:#?}");
        Ok(())
    }

    /// The translation of the block on the page at `index`, and the words
    /// its run's marks cover.
    fn translation(&self, index: usize) -> Result<(RichText, Vec<String>)> {
        let project = self.session.lock().project();
        let layer = project.layer(self.blocks[index])?;
        let Content::Block(block) = &layer.content else {
            anyhow::bail!("not a block");
        };
        let translation = block.translation.as_ref().context("a translation")?;
        let text = translation.content.text();
        let marked = Auto::of(layer)
            .marks
            .iter()
            .map(|mark| text[mark.start..mark.end].to_owned())
            .collect();
        Ok((translation.content.clone(), marked))
    }

    /// The translation of the block on the page at `index`, as it's drawn,
    /// and the record of the skew the FX window set on it.
    fn shown(&self, index: usize) -> Result<(Text, Option<Skew>)> {
        let project = self.session.lock().project();
        let layer = project.layer(self.blocks[index])?;
        let Content::Block(block) = &layer.content else {
            anyhow::bail!("not a block");
        };
        let translation = block.translation.clone().context("a translation")?;
        Ok((translation, attrs::get::<Skew>(layer)))
    }

    /// The layer of the block on the page at `index`, as it is now.
    fn layer(&self, index: usize) -> Result<Layer> {
        let project = self.session.lock().project();
        Ok(project.layer(self.blocks[index])?.clone())
    }

    /// Applies `ops` as one edit, as the user's.
    fn edit(&self, ops: Vec<Op>) -> Result<()> {
        self.session.lock().apply(Edit {
            label: "test".to_owned(),
            ops,
        })?;
        Ok(())
    }
}

/// The block `layer` holds.
fn block(layer: &Layer) -> Result<&Block> {
    match &layer.content {
        Content::Block(block) => Ok(block),
        _ => anyhow::bail!("not a block"),
    }
}

/// A run's snapshot with `[booharu-emphasis]` as it starts, asking its model
/// for the words to stress when `asks`, as `queued` leaves one.
fn emphasis(asks: bool) -> Snapshot {
    Snapshot {
        emphasis: Some(EmphasisSettings::default()),
        asks_emphasis: asks,
        ..Snapshot::default()
    }
}

#[tokio::test]
async fn a_translating_run_that_asks_marks_its_models_picks_and_seeds_the_page_before_in_its_shape()
-> Result<()> {
    let chapter = Chapter::new(["まさか!", "どうして"])?;
    let reference = "mock/s10e-asking";
    let model = model(&[
        r#"{"1": {"text": "No way!", "emphasis": ["No"]}}"#,
        r#"{"1": {"text": "Why?", "emphasis": []}}"#,
    ]);
    let _hosted = Hosted::new(reference, model.clone().erase());
    // The first page, alone: asked for the words to stress, and the one it
    // picks marked, as the run's own.
    let first = Target::Pages(vec![chapter.pages[0]]);
    chapter.translate(first, reference, emphasis(true)).await?;
    let requests = model.requests();
    assert_eq!(
        requests[0].output_schema.as_ref(),
        Some(&emphasis_schema(&keys(1))?)
    );
    let (translation, marked) = chapter.translation(0)?;
    assert_eq!(translation.text(), "No way!");
    assert_eq!(translation.0[0].insert, "No");
    assert_eq!(translation.0[0].attributes.weight, Some(700));
    assert_eq!(marked, ["No"]);

    // The second page by a run of its own, in a conversation of its own:
    // the page before goes ahead of it, its answer in the shape asked for,
    // with the words its marks cover.
    let second = Target::Pages(vec![chapter.pages[1]]);
    chapter.translate(second, reference, emphasis(true)).await?;
    let requests = model.requests();
    assert_eq!(requests.len(), 2);
    let history: Vec<&Message> = requests[1].chat_history.iter().map(|m| &**m).collect();
    let seeded = [
        Message::user("{\n  \"1\": \"まさか!\"\n}"),
        Message::assistant("{\n  \"1\": {\"text\": \"No way!\", \"emphasis\": [\"No\"]}\n}"),
    ];
    assert!(
        history
            .windows(2)
            .any(|pair| pair[0] == &seeded[0] && pair[1] == &seeded[1]),
        "{history:#?}"
    );
    let (translation, marked) = chapter.translation(1)?;
    assert_eq!(translation.text(), "Why?");
    assert_eq!(translation.0.len(), 1);
    assert!(marked.is_empty());
    Ok(())
}

#[tokio::test]
async fn a_translating_run_that_doesnt_ask_takes_its_old_marks_off_and_seeds_koharus_shape()
-> Result<()> {
    let chapter = Chapter::new(["まさか!", "どうして"])?;
    let (asking, plain) = ("mock/s10e-asked", "mock/s10e-plain");
    let marking = model(&[r#"{"1": {"text": "No way!", "emphasis": ["No"]}}"#]);
    let _marking = Hosted::new(asking, marking.clone().erase());
    let first = Target::Pages(vec![chapter.pages[0]]);
    chapter
        .translate(first.clone(), asking, emphasis(true))
        .await?;
    assert_eq!(chapter.translation(0)?.1, ["No"]);
    // A run that doesn't ask, as one on a translation model: Koharu's plain
    // answer, and the run's old mark taken off.
    let model = model(&[r#"{"1": "No way!"}"#, r#"{"1": "Why?"}"#]);
    let _plain = Hosted::new(plain, model.clone().erase());
    chapter.translate(first, plain, emphasis(false)).await?;
    let requests = model.requests();
    assert_ne!(
        requests[0].output_schema.as_ref(),
        Some(&emphasis_schema(&keys(1))?)
    );
    let (translation, marked) = chapter.translation(0)?;
    assert_eq!(translation.text(), "No way!");
    assert_eq!(translation.0.len(), 1, "{translation:?}");
    assert!(marked.is_empty());
    // Its page before is seeded in Koharu's shape.
    let second = Target::Pages(vec![chapter.pages[1]]);
    chapter.translate(second, plain, emphasis(false)).await?;
    let requests = model.requests();
    let history: Vec<&Message> = requests[1].chat_history.iter().map(|m| &**m).collect();
    let seeded = Message::assistant("{\n  \"1\": \"No way!\"\n}");
    assert!(history.contains(&&seeded), "{history:#?}");
    Ok(())
}

#[tokio::test]
async fn a_block_skewed_by_hand_keeps_its_skew_when_translated_again() -> Result<()> {
    let chapter = Chapter::new(["まさか!", "どうして"])?;
    let reference = "mock/s12h-skewed";
    let model = model(&[r#"{"1": "No way!"}"#, r#"{"1": "No way at all!"}"#]);
    let _hosted = Hosted::new(reference, model.clone().erase());
    let first = Target::Pages(vec![chapter.pages[0]]);
    chapter
        .translate(first.clone(), reference, Snapshot::default())
        .await?;
    // Skewed in the FX window, as `booharu_set_skew` skews it: about its
    // box's centre, with the skew's record.
    let (shown, record) = chapter.shown(0)?;
    let pivot = skew::pivot(&shown.frame);
    assert_eq!(pivot, Point::new(30.0, 40.0));
    let transform = skew::reskewed(shown.transform, record.as_ref(), (12.0, -3.0), pivot);
    let block = chapter.blocks[0];
    let skewed = Skew {
        x: 12.0,
        y: -3.0,
        pivot: [pivot.x, pivot.y],
    };
    chapter.session.lock().apply(Edit {
        label: "booharu_set_skew".to_owned(),
        ops: vec![
            Op::UpdateLayer {
                layer: block.into(),
                patch: json!({"translation": {"transform": transform}}),
            },
            attrs::set(block, &skewed)?,
        ],
    })?;
    // Translated again: the new text in placement's fresh transform with
    // the skew composed in about the box's centre, and the record kept.
    chapter
        .translate(first, reference, Snapshot::default())
        .await?;
    let (written, record) = chapter.shown(0)?;
    assert_eq!(written.content.text(), "No way at all!");
    assert_eq!(record, Some(skewed));
    assert_eq!(skew::shown(&written, record.as_ref()), (12.0, -3.0));
    let about = Affine::translate(pivot.to_vec2())
        * skew::matrix(12.0, -3.0)
        * Affine::translate(-pivot.to_vec2());
    let drawn = Affine::from(written.transform);
    assert!(
        drawn
            .as_coeffs()
            .iter()
            .zip(about.as_coeffs())
            .all(|(a, b)| (a - b).abs() <= 1e-9),
        "{drawn:?}"
    );
    Ok(())
}

const GREEN: Rgba = Rgba([0.0, 0.5, 0.0, 1.0]);
const RED: Rgba = Rgba([1.0, 0.0, 0.0, 1.0]);
const BLACK: Rgba = Rgba([0.0, 0.0, 0.0, 1.0]);

/// The op Koharu's type bar sends to style `block`, which has no translation
/// yet, with `patch` on its translation, as a family or a colour picked
/// before translating: the scene starts the translation from empty point
/// text, a placeholder (`status::placeholder`).
fn type_bar(block: LayerId, patch: serde_json::Value) -> Op {
    Op::UpdateLayer {
        layer: block.into(),
        patch: json!({"translation": patch}),
    }
}

/// A run's snapshot lettering new text in CCWildWords, Floating, which every
/// block of a [`Chapter`] is, italic in it while Pick fonts automatically is
/// on, and with colour detection off, so the default look's red fill and
/// black outline 2.5 wide replace what placement measured.
fn lettered() -> Snapshot {
    let floating = CategoryFont {
        family: None,
        bold: false,
        italic: true,
    };
    Snapshot {
        lettering: LetteringSettings {
            default: DefaultTextStyle {
                fill: Some(RED),
                stroke: Some(BLACK),
                stroke_width: 2.5,
            },
            detect_colors: false,
            auto: true,
            categories: BTreeMap::from([(LetteringCategory::Floating, floating)]),
            ..LetteringSettings::default()
        },
        font: Some("CCWildWords".to_owned()),
        ..Snapshot::default()
    }
}

#[tokio::test]
async fn a_box_styled_before_its_translated_is_translated_in_the_style_set_by_hand() -> Result<()> {
    let chapter = Chapter::new(["まさか!", "どうして"])?;
    let reference = "mock/pfix-styled";
    let model = model(&[r#"{"1": "No way!"}"#, r#"{"1": "Why?"}"#]);
    let _hosted = Hosted::new(reference, model.clone().erase());
    // Styled on the type bar before either is translated: the first in a
    // family and a colour of the user's, vertical, the second in a colour
    // alone. Each is a placeholder the planner still translates.
    chapter.edit(vec![
        type_bar(
            chapter.blocks[0],
            json!({"style": {"chars": {"family": "Comic Neue", "fill": GREEN}},
                "direction": "vertical"}),
        ),
        type_bar(
            chapter.blocks[1],
            json!({"style": {"chars": {"fill": GREEN}}}),
        ),
    ])?;
    for index in 0..2 {
        let layer = chapter.layer(index)?;
        let found = block(&layer)?;
        assert!(status::placeholder(&layer, found), "{index}");
        assert!(status::untranslated(&layer, found), "{index}");
    }
    let pages = Target::Pages(chapter.pages.clone());
    chapter.translate(pages, reference, lettered()).await?;
    for (index, text) in [(0, "No way!"), (1, "Why?")] {
        let layer = chapter.layer(index)?;
        let found = block(&layer)?;
        let translation = found.translation.as_ref().context("a translation")?;
        // Sent and written, in placement's place: its source's box filled,
        // horizontal for English whatever was set before (D13).
        assert_eq!(translation.content.text(), text, "{index}");
        assert!(
            matches!(translation.frame, Frame::Box { .. }),
            "{index}: {:?}",
            translation.frame
        );
        assert_eq!(translation.direction, Direction::Horizontal, "{index}");
        assert!(found.cleanup.visible, "{index}");
        assert!(!status::unwritten(&layer, found), "{index}");
        // The colour set by hand stays; the default look's outline fills
        // the keys left unset.
        let chars = &translation.style.chars;
        assert_eq!(
            (chars.fill, chars.stroke, chars.stroke_width),
            (Some(GREEN), Some(BLACK), Some(2.5)),
            "{index}"
        );
        // The record holds the run's text and the keys the look wrote as
        // automatic, and none of the user's, which stay hand edits.
        let record = Auto::of(&layer);
        assert_eq!(record.text, Some(text_hash(text)), "{index}");
        let style = record.style.context("a style")?;
        assert_eq!(
            (style.fill, style.stroke, style.stroke_width),
            (None, Some(BLACK), Some(2.5)),
            "{index}"
        );
        let hand = edited(&layer);
        assert_eq!((hand.text, hand.style), (false, true), "{index}");
    }
    // The family set by hand stays, with no category font, and isn't
    // recorded; the box without one takes its category's, Floating's
    // italic in the default font, recorded as automatic.
    type Font = (Option<String>, Option<bool>);
    let font = |index: usize| -> Result<(Font, Font)> {
        let layer = chapter.layer(index)?;
        let translation = block(&layer)?
            .translation
            .clone()
            .context("a translation")?;
        let chars = translation.style.chars;
        let style = Auto::of(&layer).style.context("a style")?;
        Ok(((chars.family, chars.italic), (style.family, style.italic)))
    };
    assert_eq!(
        font(0)?,
        ((Some("Comic Neue".to_owned()), None), (None, None))
    );
    let floating = (Some("CCWildWords".to_owned()), Some(true));
    assert_eq!(font(1)?, (floating.clone(), floating));
    Ok(())
}

#[tokio::test]
async fn a_box_styled_before_its_translated_and_given_back_keeps_its_style() -> Result<()> {
    let chapter = Chapter::new(["まさか!", "どうして"])?;
    let reference = "mock/pfix-given-back";
    // Each answer is its source: a line given back unchanged.
    let model = model(&[r#"{"1": "まさか!"}"#, r#"{"1": "どうして"}"#]);
    let _hosted = Hosted::new(reference, model.clone().erase());
    let styled = json!({"style": {"chars": {"family": "Comic Neue", "fill": GREEN}}});
    chapter.edit(vec![type_bar(chapter.blocks[0], styled)])?;
    let pages = Target::Pages(chapter.pages.clone());
    chapter.translate(pages, reference, lettered()).await?;
    // The first keeps its placeholder and the style set by hand on it (D1),
    // while its clean-up hides, so the page's own lettering shows, and it
    // reads as given back: neither hidden by hand nor still to translate,
    // so the planner plans nothing more for it.
    let layer = chapter.layer(0)?;
    let found = block(&layer)?;
    let translation = found.translation.as_ref().context("the placeholder")?;
    assert!(status::placeholder(&layer, found));
    assert_eq!(
        (
            translation.style.chars.family.as_deref(),
            translation.style.chars.fill
        ),
        (Some("Comic Neue"), Some(GREEN))
    );
    assert!(!found.cleanup.visible);
    assert_eq!(Auto::of(&layer).text, Some(text_hash("")));
    assert!(status::given_back(&layer, found));
    assert!(!sfx::hand_kept(&layer, found));
    assert!(!status::untranslated(&layer, found));
    assert!(!Auto::of(&layer).kept);
    assert!(edited(&layer).style, "its values stay the user's");
    // A block with no translation is given back as Koharu gives one back.
    let layer = chapter.layer(1)?;
    let found = block(&layer)?;
    assert_eq!(found.translation, None);
    assert!(!found.cleanup.visible);
    assert!(status::given_back(&layer, found));
    Ok(())
}
