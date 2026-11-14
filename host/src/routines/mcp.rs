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
        "save_progress" => hub.routines.save_progress(hub, bot, required(args, "id")?, required(args, "content")?),
        "read_progress" => hub.routines.progress(bot, required(args, "id")?),
        "follow_up" => hub.routines.follow_up(hub, bot, args),
        "cancel_follow_up" => hub.routines.cancel_follow_up(hub, bot, required(args, "id")?),
        _ => bail!("unknown routine tool"),
    }
}

pub const INSTRUCTIONS: &str = "Create and manage persistent Codync routines when the user asks for scheduled or event-driven work. Use the built-in routines tools (save_routine, list_routines, set_routine_enabled, delete_routine, run_routine, routine_webhook, save_progress, read_progress), not operating-system cron or an improvised background process. For recurring clock schedules, always use a five-field cron trigger with an explicit IANA timezone, including minute/hour frequencies; do not create interval triggers for those requests. Preserve existing intervals when editing unrelated fields. Cron steps reset within their field: */7 minutes is not an exact seven-minute duration across hours. Use once for a single dated execution; never approximate a one-time task with an annually repeating cron expression. Before saving, resolve the task, trigger and intended timezone for calendar/one-time schedules from the conversation; ask only for missing or ambiguous details. If the request is already clear, create it without another confirmation. Check that tools, connector sign-ins and files needed by the task are available; describe any missing setup rather than promising the task will work. Use list_routines before creating or editing to avoid duplicates; preserve unrelated triggers, enabled state and instructions. Save returns the actual routine ID, trigger descriptions and nextRunAt: use these to report what was really saved, including timezone, next occurrence and enabled/paused state. Do not claim success if the tool failed. run_routine executes the real task immediately, including its side effects; use it when the user, or another bot, asks for a test or immediate run of one of your routines, not merely to validate a schedule. You run only your own routines: for another bot's, ask that bot with message_bot to run it with its own run_routine. A routine runs locally in a session of its own; the host must be running and awake, its result appears in your chat with the user, and (pass) stays silent. A run that fails before using a tool that changes anything is retried automatically 1, 5 and 15 minutes later; one that already acted is left for the user to retry from its failure notice. Webhook triggers: routine_webhook explains how senders deliver; never claim a third-party integration is connected until a real delivery has been verified. Keep secrets out of routine instructions and ordinary status summaries. Progress: a run starts in a fresh session, so its prompt opens with that routine's progress note and says how to replace it with save_progress; read_progress reads the note from any other conversation. Follow-ups: in your own chat with the user, when something needs another look later (a build to finish, a reply to arrive, a job to check), call follow_up with the minutes to wait and a note saying what to check and why, written to make sense with nothing else to go on. When it is due you get one turn in that chat that opens with the note: tell the user what they need to know, or return exactly (pass) when there is nothing yet, and set another follow_up to keep watching. Never tell the user you will check back later without setting one. follow_up is refused in a group room and while answering another bot's request, where its turn would not be seen: say there that you can't check back. A follow-up happens once and is gone, so anything recurring, or anything the user wants as a standing task, is a routine. list_routines also shows the follow-ups still waiting; cancel_follow_up drops one.";

/// The part of a bot's own instructions about coming back later; a session whose instructions predate it hears it
/// each turn (`agent/bot/turn.rs`). An agent's own timers end with its process (which stops when idle), survive no
/// restart and show nowhere, and a model reaches for them first unless told.
pub const BRIEF: &str = "Coming back later: to look at something again later in your own chat with the user (a build, a reply, a job), set a follow-up with follow_up(minutes, note); in a group room or while answering another bot's request it isn't available, so say there that you can't check back. For recurring or standing work, save a routine with save_routine. Don't use your engine's own wake-ups, timers, loops, sleeps or cron for either: they end with its process, and the user can't see or cancel them.";

pub fn tools() -> Value {
    let id = json!({"type":"string"});
    let mut tools = vec![
        json!({"name":"list_routines","description":"List this bot's routines, their run history and the follow-ups still waiting.","inputSchema":{"type":"object","properties":{}}}),
        json!({"name":"save_progress","description":"Replace a routine's progress note: where its runs stand and what is left, for the next run to pick up from. Send the whole note; it holds at most 2,000 tokens, so keep a position, not a list of everything done. An empty content clears it.","inputSchema":{"type":"object","properties":{"id":id,"content":{"type":"string"}},"required":["id","content"]}}),
        json!({"name":"read_progress","description":"Read a routine's progress note. A run's own prompt already carries it.","inputSchema":{"type":"object","properties":{"id":id},"required":["id"]}}),
        json!({"name":"follow_up","description":"Get one turn in your own chat with the user later, to look at something again: minutes from now (1 to 43200, 30 days) and a note saying what to check and why, written to make sense with nothing else to go on. It happens once. Refused in a group room and while answering another bot's request. Returns its id and atLocal, the time on this computer's clock.","inputSchema":{"type":"object","properties":{"minutes":{"type":"integer","minimum":1,"maximum":43200},"note":{"type":"string"}},"required":["minutes","note"]}}),
        json!({"name":"cancel_follow_up","description":"Drop a follow-up that hasn't happened yet.","inputSchema":{"type":"object","properties":{"id":id},"required":["id"]}}),
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
