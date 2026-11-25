//! The open project's conversation with the agent, kept beside the project
//! in its agent file (`booharu_core::context::agent`) so it comes back with
//! the project (D18): the chat, after Settings, a reopen or a restart, and
//! what the model remembers of it.
//!
//! Each run of the agent has a [`Recorder`], which `run_agent` starts. It
//! folds the agent's events into the answer as the chat panel folds them
//! (`AgentPanel`'s `told` and `settle`), keeps what the model remembers once
//! the run ends, answered or failed, and [`finish`](Recorder::finish) writes
//! the user's message, the answer and that memory in one update before the
//! panel hears the end. A run that never ends, as Stop, a run in its place,
//! New chat and closing the project drop it, is written as cancelled as it's
//! dropped, the memory left as the last run that ended left it, as in a
//! conversation still open. [`new_chat`] puts a divider below what was said
//! and forgets the memory, and a conversation made again, after an open,
//! New chat or a change of model, goes on from the memory saved, whatever
//! model it was had with ([`restore`]), or, when the memory no longer
//! reads, as after an update whose rig reads it no more, afresh below a
//! divider saying so. The model remembers only what is below the last
//! divider; the transcript keeps everything.
//!
//! The memory is saved without the pages `view_page` showed the model, each
//! left as a note saying so, and with every secret in it redacted. Each
//! write tells the interface `booharu_agent_history_changed`, and the
//! interface reads the transcript with [`booharu_get_agent_history`], never
//! the memory.

use std::{
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

use booharu_core::context::agent::{
    AgentDividerReason, AgentEntry, AgentFile, AgentMemory, AgentPart, AgentToolState,
    AgentTurnState, RIG_VERSION, entry_id, now_millis,
};
use koharu_agent::Event;
use koharu_llm::{
    Context,
    rig::{conversation::Conversation, redact},
};
use koharu_scene::Session;
use parking_lot::Mutex;
use serde::Deserialize as _;
use serde_json::{Value, json};
use tauri::State;

use super::events::AGENT_HISTORY_CHANGED;
use crate::{
    app::{App, Project},
    commands::Result,
    screen::Screen,
};

/// What a page a tool showed the model becomes in its saved memory.
const PAGE_NOT_KEPT: &str = "page image not kept; call view_page again";

/// How many times New chat was pressed in this process, so a run that ends
/// after one, its turn written as New chat cancelled it, never saves the
/// memory New chat forgot.
static CHATS: AtomicU64 = AtomicU64::new(0);

/// Every entry of the open project's conversation with the agent, earliest
/// first, old conversations' too; never the model's memory. A file that
/// doesn't read fails, naming it.
#[tauri::command]
#[specta::specta]
pub async fn booharu_get_agent_history(app: State<'_, Arc<App>>) -> Result<Vec<AgentEntry>> {
    let path = app.session()?.lock().path().to_owned();
    Ok(
        tokio::task::spawn_blocking(move || AgentFile::load(&path).map(|file| file.entries))
            .await??,
    )
}

/// Tells the interface the conversation changed.
type Tell = Arc<dyn Fn() + Send + Sync>;

/// A run's turn: what the user asked, and the agent's answer as it comes,
/// kept in the project's agent file as the run ends, or, dropped first, as
/// cancelled.
pub(crate) struct Recorder {
    /// Tells the interface once the turn is written.
    tell: Tell,
    /// The project's file, which the agent file sits beside; never the
    /// session, which deleting the project closes first.
    path: PathBuf,
    prompt: String,
    /// The agent's model, as its settings name it.
    model: String,
    /// When the run started, which both its entries are timed at, so a turn
    /// written late, as a cancelled one is, goes where it began.
    at: u64,
    /// The New chats there had been as the run started ([`CHATS`]).
    chats: u64,
    /// The answer, as the agent's events fold it.
    parts: Arc<Mutex<Vec<Part>>>,
    /// What the model remembered as the run ended.
    memory: Option<Conversation>,
    /// Whether the turn was written as the run ended.
    ended: bool,
}

impl Recorder {
    /// The turn of a run of `prompt` with the agent on `model` in the open
    /// `project`, starting now.
    pub(crate) fn start(app: &App, project: &Project, prompt: &str, model: &str) -> Self {
        let path = project.session.lock().path().to_owned();
        let screen = app.screen.clone();
        Self::new(Arc::new(move || tell(&screen)), path, prompt, model)
    }

    fn new(tell: Tell, path: PathBuf, prompt: &str, model: &str) -> Self {
        Self {
            tell,
            path,
            prompt: prompt.to_owned(),
            model: model.to_owned(),
            at: now_millis(),
            chats: CHATS.load(Ordering::SeqCst),
            parts: Arc::default(),
            memory: None,
            ended: false,
        }
    }

    /// Folds each event the agent tells into the answer, as the chat panel
    /// folds it.
    pub(crate) fn fold(&self) -> impl Fn(&Event) + Send + Sync + 'static {
        let parts = Arc::clone(&self.parts);
        move |event| fold(&mut parts.lock(), event, now_millis())
    }

    /// Keeps what the model remembers now, in `context`, the conversation
    /// it goes on in, to save as the run ends: it changes only as the run
    /// answers, but making room for the run may have changed it before a
    /// failure.
    pub(crate) fn remember(&mut self, context: &Context) {
        self.memory = Some(context.conversation().view());
    }

    /// Writes the turn as the run ended with `result`, its answer or why it
    /// failed, and what the model remembers then, in one update of the agent
    /// file, off the async threads, and tells the interface. It's written
    /// before the panel hears the end, so a New chat pressed then is never
    /// undone by this write; a New chat pressed since the run started,
    /// which cancels it, keeps the memory forgotten. Once begun, the write
    /// goes on whether or not the run is aborted meanwhile.
    pub(crate) async fn finish(mut self, result: &anyhow::Result<String>) {
        let ending = match result {
            Ok(answer) => Ending::Answered(answer.clone()),
            Err(error) => Ending::Failed(format!("{error:#}")),
        };
        let turn = self.turn(ending);
        let memory = self.memory.take();
        self.ended = true;
        let (tell, path, model, chats) = (
            Arc::clone(&self.tell),
            self.path.clone(),
            self.model.clone(),
            self.chats,
        );
        let writing = tokio::task::spawn_blocking(move || {
            let memory = memory.and_then(|conversation| {
                saved(&conversation, &model)
                    .inspect_err(|error| {
                        tracing::warn!("couldn't save what the agent remembers: {error:#}");
                    })
                    .ok()
            });
            if written(&path, turn, memory, chats) {
                tell();
            }
        });
        if let Err(error) = writing.await {
            tracing::warn!("couldn't keep the agent's turn: {error}");
        }
    }

    /// The user's message and the agent's answer, ended as `ending` says.
    fn turn(&self, ending: Ending) -> [AgentEntry; 2] {
        let parts = std::mem::take(&mut *self.parts.lock());
        let (state, error, parts) = ended(parts, ending, now_millis());
        [
            AgentEntry::User {
                id: entry_id(),
                at: self.at,
                text: self.prompt.clone(),
            },
            AgentEntry::Assistant {
                id: entry_id(),
                at: self.at,
                model: self.model.clone(),
                state,
                error,
                parts,
            },
        ]
    }
}

impl Drop for Recorder {
    /// A run dropped before it ended, as Stop, a run in its place, New chat
    /// and closing the project drop its task, is written as cancelled, with
    /// what it told, and no memory: the model remembers what the last run
    /// that ended left it, as a conversation still open does. An aborted
    /// run's task is dropped on one of the runtime's threads, so the write
    /// goes to a blocking thread when there's a runtime.
    fn drop(&mut self) {
        if self.ended {
            return;
        }
        let turn = self.turn(Ending::Cancelled);
        let (tell, path) = (Arc::clone(&self.tell), self.path.clone());
        let write = move || {
            if written(&path, turn, None, 0) {
                tell();
            }
        };
        match tokio::runtime::Handle::try_current() {
            Ok(runtime) => drop(runtime.spawn_blocking(write)),
            Err(_) => write(),
        }
    }
}

/// Writes `turn` into the agent file of the project file `path`, and
/// `memory` in place of the file's unless a New chat came since `chats` were
/// counted; nothing once the project's file is gone, as after a delete.
/// Answers whether it wrote. A failure is logged, as nothing waits for it.
fn written(path: &Path, turn: [AgentEntry; 2], memory: Option<AgentMemory>, chats: u64) -> bool {
    let update = AgentFile::update(path, |file| {
        for entry in turn {
            file.insert(entry);
        }
        // Under the file's lock, as New chat counts itself, so its memory
        // and this one can't cross.
        if let Some(memory) = memory
            && CHATS.load(Ordering::SeqCst) == chats
        {
            file.memory = Some(memory);
        }
    });
    update
        .inspect_err(|error| tracing::warn!("couldn't keep the agent's turn: {error:#}"))
        .is_ok_and(|written| written.is_some())
}

/// Starts the open `project`'s conversation with the agent over in its
/// agent file, as New chat does: what was said stays above a divider, and
/// the model's memory is forgotten, so the conversation made for the next
/// run starts afresh. No divider goes below one with nothing said since,
/// unless a run is going, which New chat cancels and whose turn is written
/// above it once it's dropped.
pub(crate) fn new_chat(app: &App, project: &Project) {
    let path = project.session.lock().path().to_owned();
    match divided(&path, project.run.is_some()) {
        Ok(true) => tell(&app.screen),
        Ok(false) => {}
        Err(error) => tracing::warn!("couldn't start the agent's chat over: {error:#}"),
    }
}

/// [`new_chat`]'s update of the agent file of the project file `path`, with
/// a run going or not; answers whether it changed the file.
fn divided(path: &Path, running: bool) -> anyhow::Result<bool> {
    let changed = AgentFile::update(path, |file| {
        CHATS.fetch_add(1, Ordering::SeqCst);
        let forgot = file.memory.take().is_some();
        let said = running || said(&file.entries);
        if said {
            let last = file.entries.last().map_or(0, AgentEntry::at);
            file.insert(AgentEntry::Divider {
                id: entry_id(),
                at: now_millis().max(last),
                reason: AgentDividerReason::NewChat,
            });
        }
        forgot || said
    })?;
    Ok(changed.unwrap_or(false))
}

/// Whether the user or the agent said anything below the last divider of
/// `entries`, Explain this line's notes aside.
fn said(entries: &[AgentEntry]) -> bool {
    entries
        .iter()
        .rev()
        .take_while(|entry| !matches!(entry, AgentEntry::Divider { .. }))
        .any(|entry| {
            matches!(
                entry,
                AgentEntry::User { .. } | AgentEntry::Assistant { .. }
            )
        })
}

/// The conversation the open project's agent goes on in, made from `fresh`,
/// an empty one with its summary prompts: `fresh` going on with the memory
/// saved in the agent file of the project `session` holds, whatever model it
/// was had with, as the chat goes on across a change of model; `fresh` as
/// it is when none is saved, as after New chat, or when it no longer reads,
/// which leaves it forgotten below a divider saying so, the transcript
/// kept. A file that doesn't read is logged and left as it is.
pub(crate) async fn restore(app: &App, session: &Mutex<Session>, fresh: Context) -> Context {
    let path = session.lock().path().to_owned();
    let remembered = tokio::task::spawn_blocking(move || remembered(&path))
        .await
        .map_err(anyhow::Error::from)
        .and_then(|remembered| remembered);
    match remembered {
        Ok((conversation, lost)) => {
            if lost {
                tell(&app.screen);
            }
            match conversation {
                Some(conversation) => fresh.continuing(conversation),
                None => fresh,
            }
        }
        Err(error) => {
            tracing::warn!("couldn't read the agent's saved conversation: {error:#}");
            fresh
        }
    }
}

/// The memory saved in the agent file of the project file `path`, read
/// back; none when none is saved, or the project's file is gone. A memory
/// that no longer reads is forgotten below a `memory_lost` divider timed
/// just after it was saved, so the turns it held stay above it, and
/// answers that it was lost.
fn remembered(path: &Path) -> anyhow::Result<(Option<Conversation>, bool)> {
    let read = AgentFile::update(path, |file| {
        let memory = file.memory.as_ref()?;
        match Conversation::deserialize(&memory.conversation) {
            Ok(conversation) => Some((Some(conversation), false)),
            Err(error) => {
                tracing::warn!(
                    "the agent's memory, saved with rig {}, no longer reads, so the agent \
                     starts afresh: {error}",
                    memory.rig
                );
                let at = memory.saved_at.saturating_add(1).min(now_millis());
                file.memory = None;
                file.insert(AgentEntry::Divider {
                    id: entry_id(),
                    at,
                    reason: AgentDividerReason::MemoryLost,
                });
                Some((None, true))
            }
        }
    })?;
    Ok(read.flatten().unwrap_or((None, false)))
}

/// `conversation`, had with `model`, as the agent file keeps what the model
/// remembers: as rig writes it, but with each page a tool showed the model
/// left out for a note saying so, as one is a few hundred kilobytes and the
/// file is written after every run, and with every secret in it redacted.
fn saved(conversation: &Conversation, model: &str) -> anyhow::Result<AgentMemory> {
    let mut conversation = serde_json::to_value(conversation)?;
    unseen(&mut conversation);
    redact::json(&mut conversation);
    Ok(AgentMemory {
        model: model.to_owned(),
        saved_at: now_millis(),
        rig: RIG_VERSION.to_owned(),
        conversation,
    })
}

/// `value`, a conversation as rig writes it, with each image in a tool's
/// result replaced by a note that the page wasn't kept.
fn unseen(value: &mut Value) {
    match value {
        Value::Object(fields) => {
            if fields.get("type").and_then(Value::as_str) == Some("toolresult")
                && let Some(Value::Array(content)) = fields.get_mut("content")
            {
                for item in content {
                    if item.get("type").and_then(Value::as_str) == Some("image") {
                        *item = json!({"type": "text", "text": PAGE_NOT_KEPT});
                    }
                }
            }
            fields.values_mut().for_each(unseen);
        }
        Value::Array(items) => items.iter_mut().for_each(unseen),
        _ => {}
    }
}

/// Tells the interface the conversation changed, once the window opened.
fn tell(screen: &Screen) {
    if let Some(handle) = screen.handle() {
        AGENT_HISTORY_CHANGED.tell(&handle, ());
    }
}

/// How a run ended.
enum Ending {
    /// It answered with this text.
    Answered(String),
    /// It failed, as this says.
    Failed(String),
    /// It was dropped before it ended.
    Cancelled,
}

/// A part of the answer as it comes, as the chat panel folds it.
#[derive(Clone, Debug, PartialEq)]
enum Part {
    Text(String),
    /// The model's reasoning, with when it started and, once text or a tool
    /// follows, ended, in milliseconds since the epoch.
    Thinking {
        text: String,
        started: u64,
        ended: Option<u64>,
    },
    /// A tool call, with how it ended once it did.
    Tool {
        tool: String,
        arguments: Value,
        ended: Option<AgentToolState>,
        error: Option<String>,
    },
}

/// `parts` with `event`, told at `now`, as the chat panel's `told` folds it:
/// text and reasoning run on in their part, a tool's end settles the last
/// call of that tool still running, and text or a tool ends the thinking
/// before it.
fn fold(parts: &mut Vec<Part>, event: &Event, now: u64) {
    match event {
        Event::Text { delta } => {
            if let Some(Part::Text(text)) = parts.last_mut() {
                text.push_str(delta);
            } else {
                settle(parts, now);
                parts.push(Part::Text(delta.clone()));
            }
        }
        Event::Reasoning { delta } => {
            if let Some(Part::Thinking {
                text, ended: None, ..
            }) = parts.last_mut()
            {
                text.push_str(delta);
            } else {
                parts.push(Part::Thinking {
                    text: delta.clone(),
                    started: now,
                    ended: None,
                });
            }
        }
        Event::ToolStarted { tool, arguments } => {
            settle(parts, now);
            parts.push(Part::Tool {
                tool: tool.clone(),
                arguments: arguments.clone(),
                ended: None,
                error: None,
            });
        }
        Event::ToolFinished { tool, error } => {
            let running = parts.iter().rposition(|part| {
                matches!(part, Part::Tool { tool: called, ended: None, .. } if called == tool)
            });
            if let Some(Part::Tool {
                ended, error: told, ..
            }) = running.map(|at| &mut parts[at])
            {
                // An empty error is none, as the panel takes it.
                let failed = error.as_deref().is_some_and(|error| !error.is_empty());
                *ended = Some(match failed {
                    true => AgentToolState::Failed,
                    false => AgentToolState::Done,
                });
                told.clone_from(error);
            }
        }
    }
}

/// `parts` with their thinking ended at `now`, as when the run moves on from
/// it or ends.
fn settle(parts: &mut [Part], now: u64) {
    for part in parts {
        if let Part::Thinking {
            ended: ended @ None,
            ..
        } = part
        {
            *ended = Some(now);
        }
    }
}

/// The answer `parts` make once the run ended as `ending` says, at `now`, as
/// the chat panel ends it, with how it ended and why it failed: its
/// thinking ended and, when it answered with no text told, as a provider
/// that doesn't stream tells only its answer, that answer's text. A tool
/// still running counts as failed.
fn ended(
    mut parts: Vec<Part>,
    ending: Ending,
    now: u64,
) -> (AgentTurnState, Option<String>, Vec<AgentPart>) {
    settle(&mut parts, now);
    let (state, error) = match ending {
        Ending::Answered(answer) => {
            if !parts.iter().any(|part| matches!(part, Part::Text(_))) {
                parts.push(Part::Text(answer));
            }
            (AgentTurnState::Done, None)
        }
        Ending::Failed(error) => (AgentTurnState::Failed, Some(error)),
        Ending::Cancelled => (AgentTurnState::Cancelled, None),
    };
    let parts = parts
        .into_iter()
        .map(|part| match part {
            Part::Text(text) => AgentPart::Text { text },
            Part::Thinking {
                text,
                started,
                ended,
            } => AgentPart::Thinking {
                text,
                started,
                ended,
            },
            Part::Tool {
                tool,
                arguments,
                ended,
                error,
            } => AgentPart::Tool {
                tool,
                arguments,
                state: ended.unwrap_or(AgentToolState::Failed),
                error,
            },
        })
        .collect();
    (state, error, parts)
}

#[cfg(test)]
mod tests {
    use koharu_llm::rig::{
        completion::Usage,
        message::{
            AssistantContent, AssistantMessage, ImageMediaType, Message, ToolName,
            ToolResultContent,
        },
    };

    use super::*;

    /// Held by each test that counts on no New chat coming while its run
    /// goes, as the count of them is the process's.
    static CHATTING: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

    /// A folder with the project file `Ch.1.khr` in it.
    fn project() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().expect("a folder");
        let project = dir.path().join("Ch.1.khr");
        fs_err::write(&project, "project").expect("a project file");
        (dir, project)
    }

    fn file(project: &Path) -> AgentFile {
        AgentFile::load(project).expect("the agent file")
    }

    fn text(delta: &str) -> Event {
        Event::Text {
            delta: delta.to_owned(),
        }
    }

    fn reasoning(delta: &str) -> Event {
        Event::Reasoning {
            delta: delta.to_owned(),
        }
    }

    fn started(tool: &str, page: u32) -> Event {
        Event::ToolStarted {
            tool: tool.to_owned(),
            arguments: json!({"page": page}),
        }
    }

    fn finished(tool: &str, error: Option<&str>) -> Event {
        Event::ToolFinished {
            tool: tool.to_owned(),
            error: error.map(str::to_owned),
        }
    }

    fn tool(page: u32, state: AgentToolState, error: Option<&str>) -> AgentPart {
        AgentPart::Tool {
            tool: "view_page".to_owned(),
            arguments: json!({"page": page}),
            state,
            error: error.map(str::to_owned),
        }
    }

    fn thinking(text: &str, started: u64, ended: u64) -> AgentPart {
        AgentPart::Thinking {
            text: text.to_owned(),
            started,
            ended: Some(ended),
        }
    }

    /// `events`, told one a millisecond from 1, then ended as `ending` says.
    fn folded(
        events: &[Event],
        ending: Ending,
    ) -> (AgentTurnState, Option<String>, Vec<AgentPart>) {
        let mut parts = Vec::new();
        for (now, event) in (1..).zip(events) {
            fold(&mut parts, event, now);
        }
        ended(parts, ending, 100)
    }

    #[test]
    fn the_answer_folds_as_the_chat_panel_folds_it() {
        let events = [
            reasoning("Look"),
            reasoning(" closer."),
            // A tool ends the thinking before it.
            started("view_page", 1),
            started("view_page", 2),
            // A tool's end settles the last call of it still running.
            finished("view_page", None),
            reasoning("Hmm."),
            // Text ends the thinking before it, and runs on.
            text("It's "),
            text("Hina."),
            finished("view_page", Some("no such page")),
            finished("get_page", None),
        ];
        let answer = Ending::Answered("It's Hina.".to_owned());
        assert_eq!(
            folded(&events, answer),
            (
                AgentTurnState::Done,
                None,
                vec![
                    thinking("Look closer.", 1, 3),
                    tool(1, AgentToolState::Failed, Some("no such page")),
                    tool(2, AgentToolState::Done, None),
                    thinking("Hmm.", 6, 7),
                    AgentPart::Text {
                        text: "It's Hina.".to_owned()
                    },
                ]
            )
        );
    }

    #[test]
    fn an_answer_ends_as_the_chat_panel_ends_it() {
        // Answered with no text told, as a provider that doesn't stream
        // answers, the answer is its text, after its thinking, ended.
        let events = [started("view_page", 1), finished("view_page", Some(""))];
        let (state, error, parts) = folded(
            &[&events[..], &[reasoning("Done.")]].concat(),
            Ending::Answered("All done.".to_owned()),
        );
        assert_eq!((state, error), (AgentTurnState::Done, None));
        assert_eq!(
            parts,
            [
                tool(1, AgentToolState::Done, Some("")),
                thinking("Done.", 3, 100),
                AgentPart::Text {
                    text: "All done.".to_owned()
                },
            ]
        );
        // A failed run says why; a tool still running counts as failed.
        let failed = Ending::Failed("the model didn't answer".to_owned());
        assert_eq!(
            folded(&events[..1], failed),
            (
                AgentTurnState::Failed,
                Some("the model didn't answer".to_owned()),
                vec![tool(1, AgentToolState::Failed, None)]
            )
        );
        assert_eq!(
            folded(&[text("Lo")], Ending::Cancelled),
            (
                AgentTurnState::Cancelled,
                None,
                vec![AgentPart::Text {
                    text: "Lo".to_owned()
                }]
            )
        );
    }

    #[test]
    fn a_run_dropped_before_it_ends_is_kept_as_cancelled() {
        let (_dir, project) = project();
        let mut recorder = Recorder::new(Arc::new(|| {}), project.clone(), "Read p. 1", "m/1");
        let fold = recorder.fold();
        fold(&started("view_page", 1));
        fold(&text("Page 1 is"));
        let mut context = Context::default();
        context.record([(vec![Message::user("Read p. 1")], Usage::default())]);
        recorder.remember(&context);
        // No runtime here: it's written as it drops.
        drop(recorder);
        let kept = file(&project);
        let [
            AgentEntry::User { text, at, .. },
            AgentEntry::Assistant {
                at: answered,
                model,
                state,
                error,
                parts,
                ..
            },
        ] = &kept.entries[..]
        else {
            panic!("{:?}", kept.entries);
        };
        assert_eq!((text.as_str(), model.as_str()), ("Read p. 1", "m/1"));
        assert_eq!(at, answered, "both where the run began");
        assert_eq!((*state, error), (AgentTurnState::Cancelled, &None));
        assert_eq!(
            parts,
            &[
                tool(1, AgentToolState::Failed, None),
                AgentPart::Text {
                    text: "Page 1 is".to_owned()
                }
            ]
        );
        assert_eq!(kept.memory, None, "no memory from a run that didn't end");
    }

    /// The page `view_page` shows, as it answers a call.
    fn viewed(call: &AssistantContent, content: ToolResultContent) -> Message {
        let AssistantContent::ToolCall(call) = call else {
            panic!("a call");
        };
        Message::tool_results(vec![call.result(vec![content])])
    }

    #[tokio::test]
    async fn the_memory_saved_goes_on_but_for_the_pages() {
        let _held = CHATTING.lock().await;
        let (_dir, project) = project();
        let name = ToolName::new("view_page").expect("a name");
        let call = AssistantContent::tool_call("call_1", name, json!({"page": 1}));
        let said = |page| {
            vec![
                Message::user("What does page 1 say? My key is sk-proj-abcdefghijklmnop1234"),
                Message::Assistant(AssistantMessage::new(vec![call.clone()])),
                viewed(&call, page),
                Message::assistant("It says hello."),
            ]
        };
        let page = ToolResultContent::image_base64("aGVsbG8=", Some(ImageMediaType::JPEG), None);
        let mut context = Context::default();
        context.record([(said(page), Usage::default())]);

        let mut recorder = Recorder::new(Arc::new(|| {}), project.clone(), "Hi", "m/1");
        recorder.fold()(&text("It says hello."));
        recorder.remember(&context);
        recorder.finish(&Ok("It says hello.".to_owned())).await;
        let kept = file(&project);
        assert!(matches!(
            &kept.entries[..],
            [
                AgentEntry::User { .. },
                AgentEntry::Assistant {
                    state: AgentTurnState::Done,
                    ..
                }
            ]
        ));
        let memory = kept.memory.as_ref().expect("the memory");
        assert_eq!(
            (memory.model.as_str(), memory.rig.as_str()),
            ("m/1", "0.46.0")
        );
        let saved = memory.conversation.to_string();
        assert!(!saved.contains("aGVsbG8="), "no page");
        assert!(!saved.contains("sk-proj-"), "no secret");

        let (conversation, lost) = remembered(&project).expect("the memory");
        assert!(!lost);
        let restored = Context::default().continuing(conversation.expect("a conversation"));
        let mut kept = said(ToolResultContent::text(PAGE_NOT_KEPT));
        kept[0] = Message::user("What does page 1 say? My key is [REDACTED]");
        let kept: Vec<Arc<Message>> = kept.into_iter().map(Arc::new).collect();
        assert_eq!(restored.messages(), &kept[..]);
    }

    #[test]
    fn new_chat_forgets_the_memory_below_one_divider() {
        let _held = CHATTING.blocking_lock();
        let (_dir, project) = project();
        // Nothing said: no divider, nothing written.
        assert!(!divided(&project, false).expect("divided"));
        assert!(file(&project).entries.is_empty());

        let memory = AgentMemory {
            model: "m/1".to_owned(),
            saved_at: 5,
            rig: RIG_VERSION.to_owned(),
            conversation: json!({}),
        };
        AgentFile::update(&project, |file| {
            file.insert(AgentEntry::User {
                id: "u1".to_owned(),
                at: 1,
                text: "Hi".to_owned(),
            });
            file.memory = Some(memory);
        })
        .expect("written");
        assert!(divided(&project, false).expect("divided"));
        let kept = file(&project);
        assert_eq!(kept.memory, None);
        assert!(matches!(
            &kept.entries[..],
            [
                AgentEntry::User { .. },
                AgentEntry::Divider {
                    reason: AgentDividerReason::NewChat,
                    ..
                }
            ]
        ));
        // Twice in a row adds no second divider, unless a run is going,
        // whose turn goes above it once it's dropped.
        assert!(!divided(&project, false).expect("divided"));
        assert_eq!(file(&project).entries.len(), 2);
        assert!(divided(&project, true).expect("divided"));
        assert_eq!(file(&project).entries.len(), 3);
    }

    #[tokio::test]
    async fn a_run_ending_after_new_chat_leaves_the_memory_forgotten() {
        let _held = CHATTING.lock().await;
        let (_dir, project) = project();
        let mut context = Context::default();
        context.record([(vec![Message::user("Hi")], Usage::default())]);
        let mut recorder = Recorder::new(Arc::new(|| {}), project.clone(), "Hi", "m/1");
        recorder.remember(&context);
        // New chat, a click later, while the run goes; the run ends before
        // it's dropped.
        std::thread::sleep(std::time::Duration::from_millis(5));
        divided(&project, true).expect("divided");
        recorder.finish(&Ok("Hello.".to_owned())).await;
        let kept = file(&project);
        assert_eq!(kept.memory, None);
        let roles: Vec<&str> = kept
            .entries
            .iter()
            .map(|entry| match entry {
                AgentEntry::User { .. } => "user",
                AgentEntry::Assistant { .. } => "assistant",
                AgentEntry::Divider { .. } => "divider",
                AgentEntry::Note { .. } => "note",
            })
            .collect();
        assert_eq!(roles, ["user", "assistant", "divider"], "the turn above it");
    }

    #[test]
    fn a_memory_that_no_longer_reads_starts_afresh_below_a_divider() {
        let (_dir, project) = project();
        AgentFile::update(&project, |file| {
            for (id, at) in [("u1", 10), ("u2", 30)] {
                file.insert(AgentEntry::User {
                    id: id.to_owned(),
                    at,
                    text: "Hi".to_owned(),
                });
            }
            file.memory = Some(AgentMemory {
                model: "m/1".to_owned(),
                saved_at: 20,
                rig: "0.45.0".to_owned(),
                conversation: json!({"messages": [{"role": "robot"}], "meta": []}),
            });
        })
        .expect("written");
        let (conversation, lost) = remembered(&project).expect("read");
        assert!(conversation.is_none() && lost);
        let kept = file(&project);
        assert_eq!(kept.memory, None);
        let at: Vec<(&str, u64)> = kept
            .entries
            .iter()
            .map(|entry| match entry {
                AgentEntry::Divider {
                    reason: AgentDividerReason::MemoryLost,
                    at,
                    ..
                } => ("lost", *at),
                entry => (entry.id(), entry.at()),
            })
            .collect();
        assert_eq!(
            at,
            [("u1", 10), ("lost", 21), ("u2", 30)],
            "the transcript stays"
        );
        // Forgotten, it's lost no more.
        assert_eq!(
            remembered(&project)
                .map(|(conversation, lost)| (conversation.is_none(), lost))
                .expect("read"),
            (true, false)
        );
    }
}
