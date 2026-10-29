//! The built-in `routines` MCP server, scoped to one bot: its tools and instructions.
use super::{Arc, Context, Hub, Result, Value, bail, json, required};

pub fn call(hub: &Arc<Hub>, bot: &str, name: &str, args: &Value) -> Result<Value> {
    match name {
        "list_routines" => Ok(hub.routines.list(bot)),
        "save_routine" => hub.routines.save(hub, bot, args),
        "set_routine_enabled" => hub.routines.set_enabled(
            &hub.store,
            bot,
            required(args, "id")?,
            args["enabled"].as_bool().context("enabled is required")?,
        ),
        "delete_routine" => hub.routines.remove(hub, bot, required(args, "id")?),
        "run_routine" => {
            let id = required(args, "id")?;
            if !hub.routines.owns(bot, id) {
                bail!(
                    "routine not found among yours: for another bot's routine, ask that bot with message_bot to run \
                     it with its own run_routine"
                );
            }
            hub.routines.enqueue(&hub.store, bot, id, json!({"source":"test"}), None, true)
        }
        "routine_webhook" => {
            hub.routines.credentials(hub, bot, required(args, "id")?, args["rotate"].as_bool().unwrap_or(false))
        }
        _ => bail!("unknown routine tool"),
    }
}

pub const INSTRUCTIONS: &str = "Create and manage persistent Codync routines when the user asks for scheduled or event-driven work. Use the built-in routines tools (save_routine, list_routines, set_routine_enabled, delete_routine, run_routine, routine_webhook), not operating-system cron or an improvised background process. For recurring clock schedules, always use a five-field cron trigger with an explicit IANA timezone, including minute/hour frequencies; do not create interval triggers for those requests. Preserve existing intervals when editing unrelated fields. Cron steps reset within their field: */7 minutes is not an exact seven-minute duration across hours. Use once for a single dated execution; never approximate a one-time task with an annually repeating cron expression. Before saving, resolve the task, trigger and intended timezone for calendar/one-time schedules from the conversation; ask only for missing or ambiguous details. If the request is already clear, create it without another confirmation. Check that tools, connector sign-ins and files needed by the task are available; describe any missing setup rather than promising the task will work. Use list_routines before creating or editing to avoid duplicates; preserve unrelated triggers, enabled state and instructions. Save returns the actual routine ID, trigger descriptions and nextRunAt: use these to report what was really saved, including timezone, next occurrence and enabled/paused state. Do not claim success if the tool failed. run_routine executes the real task immediately, including its side effects; use it when the user, or another bot, asks for a test or immediate run of one of your routines, not merely to validate a schedule. You run only your own routines: for another bot's, ask that bot with message_bot to run it with its own run_routine. A routine runs locally in a session of its own; the host must be running and awake, its result appears in your chat with the user, and (pass) stays silent. A run that fails before using a tool that changes anything is retried automatically 1, 5 and 15 minutes later; one that already acted is left for the user to retry from its failure notice. Webhook triggers: routine_webhook explains how senders deliver; never claim a third-party integration is connected until a real delivery has been verified. Keep secrets out of routine instructions and ordinary status summaries.";

pub fn tools() -> Value {
    let id = json!({"type":"string"});
    let mut tools = vec![
        json!({"name":"list_routines","description":"List this bot's routines and run history.","inputSchema":{"type":"object","properties":{}}}),
    ];
    tools.push(json!({"name":"save_routine","description":"Create or replace a routine. Omit id to create; include id to update. triggers is an OR list. Use cron for recurring clock schedules; interval is for explicitly requested elapsed-duration semantics or existing schedules: interval {type:interval,seconds}; once {type:once,at:Unix milliseconds}; cron {type:cron,expression:five cron fields,timeZone:IANA zone}; webhook {type:webhook}; event {type:event,source:slack/github/origin/microsoftTeams/linear/sentry/pagerduty/email,event:event name or *,filters:field-to-value map}. Events require a configured webhook sender. Minimum interval is one second.","inputSchema":{"type":"object","properties":{"id":id,"name":{"type":"string"},"instruction":{"type":"string"},"enabled":{"type":"boolean"},"timeoutSeconds":{"type":"integer","minimum":1,"maximum":86400},"triggers":{"type":"array","items":{"type":"object"}}},"required":["name","instruction","triggers"]}}));
    for (name, description) in [
        ("set_routine_enabled", "Pause or resume a routine; enabled boolean is required."),
        ("delete_routine", "Delete a routine and cancel pending runs."),
        ("run_routine", "Start a test run of one of your routines, including a paused one."),
        (
            "routine_webhook",
            "Get the routine's webhook: its public url (through the Codync cloud; null when the cloud is off, then only localUrl on this computer works), localUrl and per-routine key. Senders POST with Authorization: Bearer <key>; for GitHub, set the webhook's content type to application/json and its secret to the key (deliveries are verified by X-Hub-Signature-256) and they become events {source:github, event:<event>.<action>, repo, sender, text, url, payload} that event triggers can filter. Deliveries made while the computer is offline wait up to 72 hours in the cloud, which sees their contents (not end-to-end encrypted). rotate:true replaces the key; the old one stops working at once. This doesn't register a provider subscription: set that up with the user, and report the webhook as awaiting connection until then.",
        ),
    ] {
        let required = if name == "set_routine_enabled" { vec!["id", "enabled"] } else { vec!["id"] };
        let mut properties = json!({"id": id});
        match name {
            "set_routine_enabled" => properties["enabled"] = json!({"type":"boolean"}),
            "routine_webhook" => properties["rotate"] = json!({"type":"boolean"}),
            _ => {}
        }
        tools.push(json!({"name":name,"description":description,"inputSchema":{"type":"object","properties":properties,"required":required}}));
    }
    json!(tools)
}
