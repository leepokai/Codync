//! The host API methods of routine flows, with strict validation of every input.

use super::{MAX_INSTRUCTION_BYTES, canvas, engine, graph};
use crate::LockExt;
use crate::chat::team::flow::recipient;
use crate::hub::Hub;
use crate::routines::{Routine, default_timeout};
use crate::store::{FlowLink, now_ms};
use anyhow::{Context, Result, anyhow, bail};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

/// The sample reply a flow test hands on when the caller gives none.
const SAMPLE_REPLY: &str = "This is a test of the routine's flow: a sample reply from the routine's own bot.";
const MAX_ID_CHARS: usize = 200;
/// How many model options (effort and the like) a link may set.
const MAX_MODEL_OPTIONS: usize = 10;

pub const METHODS: &[&str] = &[
    "routineFlowCanvas",
    "saveRoutineFlowPosition",
    "removeRoutineFlowPosition",
    "placeRoutineFlowBot",
    "connectRoutineFlow",
    "disconnectRoutineFlow",
    "updateRoutineFlowLink",
    "testRoutineFlow",
];

fn str_arg<'a>(b: &'a Value, key: &str) -> Result<&'a str> {
    b[key].as_str().ok_or_else(|| anyhow!("`{key}` is required"))
}

/// A canvas coordinate: a finite number within the limit.
fn f64_arg(b: &Value, key: &str) -> Result<f64> {
    let v = b[key].as_f64().ok_or_else(|| anyhow!("`{key}` must be a number"))?;
    if !v.is_finite() || v.abs() > crate::store::flows::MAX_COORDINATE {
        bail!("`{key}` must be within ±{}", crate::store::flows::MAX_COORDINATE);
    }
    Ok(v)
}

/// An id from a request: present, short and free of control characters.
fn id_arg<'a>(b: &'a Value, key: &str) -> Result<&'a str> {
    let id = str_arg(b, key)?;
    if id.is_empty() || id.chars().count() > MAX_ID_CHARS || id.chars().any(char::is_control) {
        bail!("`{key}` must be an id of 1 to {MAX_ID_CHARS} characters");
    }
    Ok(id)
}

/// The model a link's step runs on, when it names one instead of the target bot's own.
fn model_arg(b: &Value) -> Result<Option<String>> {
    let model = match b.get("model") {
        None | Some(Value::Null) => return Ok(None),
        Some(Value::String(model)) => model.trim(),
        _ => bail!("`model` must be text"),
    };
    if model.is_empty() {
        return Ok(None);
    }
    if model.chars().count() > MAX_ID_CHARS || model.chars().any(char::is_control) {
        bail!("`model` must be a model id of at most {MAX_ID_CHARS} characters");
    }
    Ok(Some(model.to_owned()))
}

/// A link's model options (effort…): short option ids and values, a handful at most.
fn model_options_arg(b: &Value) -> Result<BTreeMap<String, String>> {
    let Some(value) = b.get("modelOptions").filter(|v| !v.is_null()) else { return Ok(BTreeMap::new()) };
    let Some(options) = value.as_object() else { bail!("`modelOptions` must be an object of option ids to values") };
    if options.len() > MAX_MODEL_OPTIONS {
        bail!("`modelOptions` must have at most {MAX_MODEL_OPTIONS} options");
    }
    options
        .iter()
        .map(|(id, value)| {
            let Some(value) = value.as_str() else { bail!("`modelOptions` values must be text") };
            let short = |s: &str| !s.is_empty() && s.chars().count() <= MAX_ID_CHARS;
            if !short(id) || !short(value) {
                bail!("`modelOptions` ids and values must be 1 to {MAX_ID_CHARS} characters");
            }
            Ok((id.clone(), value.to_owned()))
        })
        .collect()
}

fn instruction_arg(b: &Value, required: bool) -> Result<String> {
    let text = match b.get("instruction") {
        Some(Value::String(text)) => text.trim(),
        None | Some(Value::Null) if !required => "",
        _ => bail!("`instruction` must be text"),
    };
    if text.len() > MAX_INSTRUCTION_BYTES {
        bail!("`instruction` must be at most {MAX_INSTRUCTION_BYTES} bytes");
    }
    Ok(text.to_owned())
}

/// A bot whose canvas can be opened: it exists and takes routines (a group has none).
fn canvas_bot(hub: &Hub, id: &str) -> Result<()> {
    hub.store.bot(id)?.filter(|b| !b.deleted && !b.config.is_group()).context("unknown bot")?;
    Ok(())
}

pub(super) fn routine(hub: &Hub, id: &str) -> Result<Routine> {
    let state = hub.routines.state.locked();
    state.routines.iter().find(|r| r.id == id && !r.deleted).cloned().context("routine not found")
}

fn node_exists(hub: &Hub, key: &str) -> Result<()> {
    let known = match key.split_once(':') {
        Some(("routine", id)) => routine(hub, id).is_ok(),
        Some(("bot", id)) => hub.store.bot(id)?.is_some_and(|b| !b.deleted),
        _ => false,
    };
    if !known {
        bail!("nodeKey must name an existing routine or bot (routine:<id> or bot:<id>)");
    }
    Ok(())
}

/// Creates a link in `routine` from `from` to `to`; the routine's own bot is the flow's start.
pub(super) fn connect(hub: &Hub, routine: &Routine, from: &str, to: &str, instruction: String) -> Result<FlowLink> {
    recipient(hub, to)?;
    if from != routine.bot_id {
        recipient(hub, from).context("the bot handing work on can't take part in a flow")?;
    }
    let link = FlowLink {
        id: uuid::Uuid::new_v4().to_string(),
        routine_id: routine.id.clone(),
        owner_bot_id: routine.bot_id.clone(),
        from_bot_id: from.to_owned(),
        to_bot_id: to.to_owned(),
        instruction,
        model: None,
        model_options: BTreeMap::new(),
        created_at: now_ms(),
    };
    let max = super::max_depth();
    hub.emit_flow_change(|| {
        let change = hub.store.flow_add_link(&link, |links| {
            match graph::connect_problem(&routine.bot_id, links, from, to, max) {
                Some(problem) => Err(problem.into()),
                None => Ok(()),
            }
        })?;
        Ok(((), Some(change)))
    })?;
    Ok(link)
}

/// Removes the link from `from` to `to` in `routine`. Whether there was one.
pub(super) fn disconnect(hub: &Hub, link_id: &str) -> Result<bool> {
    let Some(link) = hub.store.flow_link(link_id)? else { return Ok(false) };
    hub.routines.flows.cancel_routine(&link.routine_id);
    hub.routines.cancel_flow_deliveries(&hub.store, &link.routine_id)?;
    hub.emit_flow_change(|| Ok(((), hub.store.flow_remove_link(link_id)?)))?;
    Ok(true)
}

/// A deleted routine leaves its flow, and a running one stops.
pub fn forget_routine(hub: &Hub, routine_id: &str, owner: &str) {
    hub.routines.flows.cancel_routine(routine_id);
    if let Err(error) = hub.routines.cancel_flow_deliveries(&hub.store, routine_id) {
        tracing::error!(routine = routine_id, %error, "could not cancel a deleted routine's deliveries");
    }
    if let Err(error) = hub.emit_flow_change(|| Ok(((), hub.store.flow_forget_routine(routine_id, owner)?))) {
        tracing::error!(routine = routine_id, error = format!("{error:#}"), "couldn't clear a deleted routine's flow");
    }
}

/// A deleted bot leaves every flow.
pub fn forget_bot(hub: &Hub, bot: &str) {
    hub.routines.flows.cancel_bot(bot);
    if let Err(error) = hub.emit_flow_change(|| Ok(((), hub.store.flow_forget_bot(bot)?))) {
        tracing::error!(bot, error = format!("{error:#}"), "couldn't clear a deleted bot's flows");
    }
}

/// Starts the flow of `routine` as if its own bot had just replied with `reply`, without running the
/// routine. Returns the run id its steps carry.
fn test(hub: &Arc<Hub>, routine: &Routine, reply: String) -> Result<String> {
    if hub.store.flow_links(&routine.id)?.is_empty() {
        bail!("this routine has no flow: connect it to a bot first");
    }
    let run_id = format!("flow-test-{}", uuid::Uuid::new_v4());
    let start = engine::Start {
        run_id: run_id.clone(),
        routine_id: routine.id.clone(),
        routine_name: routine.name.clone(),
        owner: routine.bot_id.clone(),
        run_at: now_ms(),
        timeout: Duration::from_secs(if routine.timeout_seconds == 0 {
            default_timeout()
        } else {
            routine.timeout_seconds
        }),
        max_depth: super::max_depth(),
        instruction: routine.instruction.clone(),
        outputs: routine.outputs.clone(),
        event: if routine.outputs.iter().any(|output| output.kind == "whatsapp") {
            hub.store.snapshot_whatsapp_output(&json!({"source":"test"}))?
        } else {
            json!({"source":"test"})
        },
        root: engine::Root::Replied(reply),
    };
    hub.routines.stage_flow(&hub.store, start.clone())?;
    tokio::spawn(engine::execute(hub.clone(), start));
    Ok(run_id)
}

/// Answers a routine-flow method, or hands the body back (`None`) when it isn't one.
pub fn call(hub: &Arc<Hub>, method: &str, b: &Value) -> Result<Option<Value>> {
    if !METHODS.contains(&method) {
        return Ok(None);
    }
    Ok(Some(match method {
        "routineFlowCanvas" => {
            let bot = id_arg(b, "botId")?;
            canvas_bot(hub, bot)?;
            canvas::canvas(hub, bot)?
        }
        "saveRoutineFlowPosition" => {
            let bot = id_arg(b, "botId")?;
            canvas_bot(hub, bot)?;
            let key = id_arg(b, "nodeKey")?;
            node_exists(hub, key)?;
            let (x, y) = (f64_arg(b, "x")?, f64_arg(b, "y")?);
            hub.emit_flow_change(|| Ok(((), Some(hub.store.flow_save_position(bot, key, x, y)?))))?;
            json!({"ok": true})
        }
        "removeRoutineFlowPosition" => {
            let (bot, key) = (id_arg(b, "botId")?, id_arg(b, "nodeKey")?);
            canvas_bot(hub, bot)?;
            hub.emit_flow_change(|| Ok(((), hub.store.flow_remove_position(bot, key)?)))?;
            json!({"ok": true})
        }
        "placeRoutineFlowBot" => {
            let (bot, placed) = (id_arg(b, "botId")?, id_arg(b, "placedBotId")?);
            canvas_bot(hub, bot)?;
            if bot == placed {
                bail!("the canvas's own bot is already on it");
            }
            recipient(hub, placed)?;
            let (x, y) = (f64_arg(b, "x")?, f64_arg(b, "y")?);
            hub.emit_flow_change(|| Ok(((), Some(hub.store.flow_place_bot(bot, placed, x, y)?))))?;
            json!({"ok": true})
        }
        "connectRoutineFlow" => {
            let (routine_id, from, to) = (id_arg(b, "routineId")?, id_arg(b, "fromBotId")?, id_arg(b, "toBotId")?);
            let r = routine(hub, routine_id)?;
            json!({"link": connect(hub, &r, from, to, instruction_arg(b, false)?)?})
        }
        "disconnectRoutineFlow" => {
            disconnect(hub, id_arg(b, "linkId")?)?;
            json!({"ok": true})
        }
        "updateRoutineFlowLink" => {
            let (id, instruction) = (id_arg(b, "linkId")?, instruction_arg(b, true)?);
            let current = hub.store.flow_link(id)?.context("link not found")?;
            // Left out, each keeps what the link already had; given empty, it clears.
            let model = if b.get("model").is_some() { model_arg(b)? } else { current.model };
            let options = if b.get("modelOptions").is_some() { model_options_arg(b)? } else { current.model_options };
            let updated = hub.emit_flow_change(|| {
                match hub.store.flow_update_link(id, &instruction, model.as_deref(), &options)? {
                    Some((link, change)) => Ok((Some(link), Some(change))),
                    None => Ok((None, None)),
                }
            })?;
            json!({"link": updated.context("link not found")?})
        }
        _ => {
            let (bot, routine_id) = (id_arg(b, "botId")?, id_arg(b, "routineId")?);
            if !hub.routines.owns(bot, routine_id) {
                bail!("routine not found among this bot's routines");
            }
            let reply = match b.get("input") {
                None | Some(Value::Null) => SAMPLE_REPLY.to_owned(),
                Some(Value::String(text)) if !text.trim().is_empty() && text.len() <= MAX_INSTRUCTION_BYTES => {
                    text.clone()
                }
                _ => bail!("`input` must be text of at most {MAX_INSTRUCTION_BYTES} bytes"),
            };
            json!({"runId": test(hub, &routine(hub, routine_id)?, reply)?})
        }
    }))
}
