//! Tools a bot on this computer uses to read and rewrite its own workflows.

use super::model::Graph;
use super::service::Workflows;
use super::store;
use crate::agent::bot::Cmd;
use crate::chat::manage::{APPROVAL_TIMEOUT, MAX_CARD_DETAIL};
use crate::hub::Hub;
use anyhow::{Context, Result, bail};
use serde_json::{Value, json};
use std::sync::Arc;
use tokio::sync::oneshot;
use uuid::Uuid;

pub const INSTRUCTIONS: &str = "Workflows you created on this computer are graphs you can also read and rewrite as a script. list_workflows and read_workflow show only yours. save_workflow and run_workflow wait for the user to allow them, and a new workflow starts enabled. Every agent() call names your own bot; an empty or another bot's id is refused. For another bot's workflow, ask that bot with message_bot. Use list_workflows and read_workflow before changing one. save_workflow sends the whole script and the revision you read; a conflict means someone else saved first, so read it again. A save changes the next run, not the one already going. Do not put webhook secrets in the script. In a thread the user opened beside one workflow to build it, these tools work on that workflow only: read_workflow without an id reads it and lists the bots on this computer, and its agent() calls may name any of those bots. Keep the script one the canvas can draw: the shape read_workflow gives, one `// @node` line and one `await agent(botId, text, { label })` per step, a step's task at the start of its text (\"Task\\n\\n\" + step1); save_workflow refuses any other. Save once the user has said what to change.";

pub fn tools() -> Value {
    json!([
        {"name":"list_workflows","description":"List this bot's workflows.","inputSchema":{"type":"object","properties":{}}},
        {"name":"read_workflow","description":"Read one of this bot's workflows, including its script and revision.","inputSchema":{"type":"object","properties":{"id":{"type":"string"}},"required":["id"]}},
        {"name":"save_workflow","description":"Replace one of this bot's scripts, after the user allows it. Send baseRevision from read_workflow. Omit id to create one.","inputSchema":{"type":"object","properties":{"id":{"type":"string"},"baseRevision":{"type":"integer"},"script":{"type":"string"},"title":{"type":"string"},"triggers":{"type":"array","description":"The workflow's triggers, each {\"id\":\"t1\",\"kind\":\"trigger\",\"at\":{\"x\":80,\"y\":80},\"fields\":{\"trigger\":\"manual\"}} (\"schedule\" or \"webhook\" in the place of \"manual\"); left out, they stay as they are"}},"required":["script"]}},
        {"name":"run_workflow","description":"Start one of this bot's workflows, after the user allows it. Fails when one run is already open.","inputSchema":{"type":"object","properties":{"id":{"type":"string"},"input":{"type":"string"}},"required":["id"]}}
    ])
}

/// Whose workflows a caller's tools reach: a bot's own, or the one workflow a thread was opened to build.
enum Scope<'a> {
    Owner(&'a str),
    Bound(String),
}

fn scope<'a>(hub: &Hub, bot: &'a str) -> Result<Scope<'a>> {
    let workflow = hub.store.bot(bot)?.filter(|row| !row.deleted).and_then(|row| row.config.workflow);
    Ok(workflow.map_or(Scope::Owner(bot), Scope::Bound))
}

pub async fn call(workflows: &Workflows, hub: &Arc<Hub>, bot: &str, name: &str, args: &Value) -> Result<Value> {
    match scope(hub, bot)? {
        Scope::Owner(bot) => owned(workflows, hub, bot, name, args).await,
        Scope::Bound(workflow) => bound(workflows, hub, bot, &workflow, name, args).await,
    }
}

/// The tools of a thread building `workflow`: that workflow only, whoever owns it, its steps on any bot here.
async fn bound(
    workflows: &Workflows,
    hub: &Arc<Hub>,
    thread: &str,
    workflow: &str,
    name: &str,
    args: &Value,
) -> Result<Value> {
    if let Some(id) = args["id"].as_str().filter(|id| !id.is_empty() && *id != workflow) {
        bail!("this thread builds one workflow ({workflow}), not {id}");
    }
    match name {
        "list_workflows" => Ok(json!({"workflows": [workflows.workflow(hub, workflow, false)?["workflow"].clone()]})),
        "read_workflow" => {
            let mut read = workflows.workflow(hub, workflow, false)?;
            read["bots"] = Value::Array(step_bots(hub)?);
            Ok(read)
        }
        "save_workflow" => {
            let script = args["script"].as_str().context("`script` is required")?;
            if !super::script::draws(script)? {
                bail!(
                    "the canvas can't draw this script, so the user couldn't edit it there: keep the shape read_workflow \
                     gave, one `// @node` line and one `await agent(botId, text, {{ label }})` per step, with a step's \
                     task at the start of its text (`\"Task\\n\\n\" + step1`), not in the options"
                );
            }
            let names = named_bots(hub, script)?;
            let base = args["baseRevision"].as_i64().context("`baseRevision` is required")?;
            // Everything is checked before the card: the user is never asked for a save that then fails.
            let triggers = triggers_of(args)?;
            let title = args["title"].as_str().unwrap_or("Workflow");
            confirm(
                hub,
                thread,
                format!("Save workflow {title}"),
                format!(
                    "Replace this workflow's script ({len} characters). Its steps message {names}.\n\n{script}",
                    len = script.chars().count(),
                    names = if names.is_empty() { "no bot".to_owned() } else { names.join(", ") },
                    script = clip(script)
                ),
            )
            .await?;
            workflows.save_script(hub, workflow, base, Some(script), triggers)
        }
        "run_workflow" => {
            confirm(
                hub,
                thread,
                "Run this workflow".to_owned(),
                "Start this workflow. It can message the bots in its steps, including their tools and computer use."
                    .into(),
            )
            .await?;
            workflows.start_run(hub, workflow, &Uuid::new_v4().to_string(), None, args["input"].as_str().unwrap_or(""))
        }
        _ => bail!("unknown workflow tool"),
    }
}

/// The bots a step may name: this computer's, not its threads, groups or the box of threads without a bot.
fn step_bots(hub: &Hub) -> Result<Vec<Value>> {
    Ok(hub
        .store
        .bots()?
        .into_iter()
        .filter(|row| !row.deleted && !row.config.plain && !row.config.is_group())
        .map(|row| json!({"id": row.config.id, "name": row.config.name}))
        .collect())
}

/// The names of the bots `script`'s steps message, each checked to be one a step may name.
fn named_bots(hub: &Hub, script: &str) -> Result<Vec<String>> {
    let bots = step_bots(hub)?;
    let mut names = Vec::new();
    for id in super::script::agent_ids(script)? {
        let Some(bot) = bots.iter().find(|bot| bot["id"] == id.as_str()) else {
            bail!("agent() names {id}, which isn't a bot on this computer: read_workflow lists them");
        };
        let name = bot["name"].as_str().unwrap_or(&id).to_owned();
        if !names.contains(&name) {
            names.push(name);
        }
    }
    Ok(names)
}

async fn owned(workflows: &Workflows, hub: &Arc<Hub>, bot: &str, name: &str, args: &Value) -> Result<Value> {
    match name {
        "list_workflows" => {
            let rows = store::list_owned(&hub.store, bot)?;
            let workflows: Vec<_> = rows.into_iter().filter(|row| !row.is_null()).collect();
            Ok(json!({"workflows": workflows}))
        }
        "read_workflow" => {
            let id = required(args, "id")?;
            require_owner(hub, bot, id)?;
            workflows.workflow(hub, id, false)
        }
        "save_workflow" => {
            let script = args["script"].as_str().context("`script` is required")?;
            own_agents(script, bot)?;
            let title = args["title"].as_str().unwrap_or("Workflow");
            if let Some(id) = args["id"].as_str() {
                require_owner(hub, bot, id)?;
                let base = args["baseRevision"].as_i64().context("`baseRevision` is required")?;
                confirm(
                    hub,
                    bot,
                    format!("Save workflow {title}"),
                    format!(
                        "Replace this workflow's script ({len} characters). It stays available to you.\n\n{script}",
                        len = script.chars().count(),
                        script = clip(script)
                    ),
                )
                .await?;
                workflows.save_script(hub, id, base, Some(script), triggers_of(args)?)
            } else {
                confirm(
                    hub,
                    bot,
                    format!("Save workflow {title}"),
                    format!(
                        "Create this workflow ({len} characters). It starts enabled.\n\n{script}",
                        len = script.chars().count(),
                        script = clip(script)
                    ),
                )
                .await?;
                workflows.create(hub, title, None, Some(script), triggers_of(args)?, Some(bot))
            }
        }
        "run_workflow" => {
            let id = required(args, "id")?;
            require_owner(hub, bot, id)?;
            let title =
                store::load(&hub.store, id)?.map(|(summary, _)| summary["title"].as_str().unwrap_or(id).to_owned());
            let title = title.unwrap_or_else(|| id.to_owned());
            confirm(
                hub,
                bot,
                format!("Run workflow {title}"),
                "Start this workflow. It can message the bots in its steps, including their tools and computer use."
                    .into(),
            )
            .await?;
            workflows.start_run(hub, id, &Uuid::new_v4().to_string(), None, args["input"].as_str().unwrap_or(""))
        }
        _ => bail!("unknown workflow tool"),
    }
}

fn require_owner(hub: &Hub, bot: &str, id: &str) -> Result<()> {
    match store::owner(&hub.store, id)? {
        Some(owner) if owner == bot => Ok(()),
        _ => bail!(
            "workflow not found among yours: for another bot's workflow, ask that bot with message_bot to run it with \
             its own run_workflow"
        ),
    }
}

fn own_agents(script: &str, bot: &str) -> Result<()> {
    if super::script::agent_ids(script)?.iter().any(|id| id != bot) {
        bail!("Name your own bot in agent()");
    }
    Ok(())
}

async fn confirm(hub: &Hub, bot: &str, title: String, detail: String) -> Result<()> {
    let (reply, answer) = oneshot::channel();
    hub.send_cmd(bot, Cmd::Approve { title, detail: clip(&detail), reply })?;
    match tokio::time::timeout(APPROVAL_TIMEOUT, answer).await {
        Ok(Ok(true)) => Ok(()),
        Ok(_) => bail!("the user didn't allow this change; nothing changed"),
        Err(_) => bail!("the user didn't answer the approval card in time; nothing changed"),
    }
}

fn clip(text: &str) -> String {
    let mut count = 0usize;
    let mut end = text.len();
    for (index, ch) in text.char_indices() {
        let next = count + ch.len_utf16();
        if next > MAX_CARD_DETAIL.saturating_sub(1) {
            end = index;
            break;
        }
        count = next;
    }
    if end == text.len() { text.to_owned() } else { format!("{}…", &text[..end]) }
}

fn required<'a>(args: &'a Value, key: &str) -> Result<&'a str> {
    args[key].as_str().filter(|s| !s.is_empty()).with_context(|| format!("`{key}` is required"))
}

fn triggers_of(args: &Value) -> Result<Option<Graph>> {
    if args.get("triggers").is_none() || args["triggers"].is_null() {
        return Ok(None);
    }
    let nodes = serde_json::from_value(args["triggers"].clone()).context("`triggers` is invalid")?;
    Ok(Some(Graph { nodes, edges: Vec::new(), strokes: Vec::new() }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::remote::identity::Identity;
    use crate::store::{BotConfig, Store};

    fn hub() -> Arc<Hub> {
        let dir = std::env::temp_dir().join(format!("codync-wf-tools-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).expect("a temp dir");
        let store = Store::open(&dir.join("t.db")).expect("a store");
        Hub::new(store, "host".into(), Identity::load_or_create(&dir).expect("an identity"), "t".into(), 0)
    }

    fn bot(hub: &Hub, cfg: Value) {
        let cfg: BotConfig = serde_json::from_value(cfg).expect("a bot config");
        hub.store.save_bot(&cfg).expect("saved");
    }

    #[tokio::test]
    async fn a_thread_building_a_workflow_reaches_that_one_only() {
        let hub = hub();
        bot(&hub, json!({"id": "dev", "name": "Dev", "backend": "claude", "cwd": "/tmp"}));
        let made = hub.workflows.create(&hub, "Mine", None, None, None, None).expect("created");
        let id = made["workflow"]["id"].as_str().expect("an id").to_owned();
        let other = hub.workflows.create(&hub, "Other", None, None, None, None).expect("created");
        let other = other["workflow"]["id"].as_str().expect("an id").to_owned();
        bot(
            &hub,
            json!({"id": "thread-1", "name": "t", "backend": "claude", "cwd": "/tmp", "threadOf": "dev", "workflow": id}),
        );

        let read = call(&hub.workflows, &hub, "thread-1", "read_workflow", &json!({})).await.expect("read");
        assert_eq!(read["workflow"]["id"], id.as_str());
        assert_eq!(read["bots"], json!([{"id": "dev", "name": "Dev"}]));
        let listed = call(&hub.workflows, &hub, "thread-1", "list_workflows", &json!({})).await.expect("listed");
        assert_eq!(listed["workflows"].as_array().map(Vec::len), Some(1));
        assert!(call(&hub.workflows, &hub, "thread-1", "read_workflow", &json!({"id": other})).await.is_err());
        // A bot's own tools still see only its own workflows, and a person's isn't one.
        let own = call(&hub.workflows, &hub, "dev", "list_workflows", &json!({})).await.expect("listed");
        assert_eq!(own["workflows"], json!([]));
    }

    #[test]
    fn a_step_names_a_bot_on_this_computer() {
        let hub = hub();
        bot(&hub, json!({"id": "dev", "name": "Dev", "backend": "claude", "cwd": "/tmp"}));
        let script = |bot: &str| {
            let graph: Graph = serde_json::from_value(json!({
                "nodes": [
                    {"id": "t1", "kind": "trigger", "at": {"x": 0, "y": 0}, "fields": {"trigger": "manual"}},
                    {"id": "a1", "kind": "agent", "at": {"x": 400, "y": 0}, "fields": {"botId": bot, "task": "Hi"}}
                ],
                "edges": [{"id": "t1-a1", "from": "t1", "to": "a1", "type": "text"}]
            }))
            .expect("a graph");
            super::super::script::generate("T", &graph)
        };
        assert_eq!(named_bots(&hub, &script("dev")).expect("named"), vec!["Dev".to_owned()]);
        assert!(named_bots(&hub, &script("nobody")).is_err());
        assert!(super::super::script::draws(&script("dev")).expect("read"));
        // A task in the options instead of the text leaves the canvas nothing to draw.
        let decided = "export const meta = {\n  name: \"T\",\n  description: \"\"\n}\n\nexport default async function ({ agent, args }) {\n  // @node a1\n  const step1 = await agent(\"dev\", args.input, { label: \"Agent\", task: \"Hi\" })\n  return step1\n}\n";
        assert!(!super::super::script::draws(decided).expect("read"));
    }
}
