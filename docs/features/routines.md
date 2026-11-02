# Routines

Routines belong to an agent bot and are usually created by talking to it ("I want
a routine that…"); the host gives the [lead bot](lead-bot.md) one of its own, *Check in*. Every agent receives the built-in `routines` MCP server. The Details page
of the bot's details (the desktop's details panel; the phones' details sheet, which tapping the
chat's title or a routine's notice opens) shows a compact list
(name, schedule, on/off switch). **+** opens the form for doing it by hand;
requests to the bot are typed directly in the chat. A row opens
the same form filled in, with delete and test run beside Save; the row's switch
pauses and resumes. Run results appear in the chat. The form is name, instruction, a **When to run** type (Schedule or Webhook, the two
Claude Code routines also offer besides GitHub events) and a run timeout. A schedule
picks how often it repeats (Hourly, Daily, Weekdays, Weekly, or Custom for a cron
written as five fields), the time and day, and an IANA time zone chosen from a list
filtered as you type (the device's own zone by default); the host compiles the
cron, checks it and shows the summary and next run. One-off times, intervals, events and multiple
triggers are left to the bot: editing such a routine shows its current triggers as
the selected type and keeps them until Schedule or Webhook replaces them. The TUI (`n` form with a typed When: `daily 09:00`, `weekdays 09:00`,
`weekly mon 09:00`, `hourly :30`, `every 2h`, a cron or `webhook`, optionally ending
in a time zone that Tab completes) offers the same actions.
A saved webhook routine's form shows its public URL and key (copy, reveal, replace
with confirmation; TUI `w` copies a curl command and `W` replaces the
key). Creation/update notices open the routine.

## Apple setup interface

The editor keeps task fields first, then a When to run type menu and its settings.
Inputs use the shared palette with a visible filled surface. New recurring schedules
default to Daily at 09:00 in the device's time zone; Custom takes a typed cron. The
host describes the schedule and shows the next run. Run timeout is under an animated Run settings
expansion. A fixed footer keeps Create routine / Save changes and inline errors
visible while the form scrolls.
The layout uses the same light/dark tokens, type family and custom controls as the
rest of Codync, with Reduce Motion support.

## Conversational setup and tools

Every agent receives the `routines` MCP server automatically; no marketplace
installation is needed. Its tools are `list_routines`, `save_routine`,
`set_routine_enabled`, `delete_routine`, `run_routine` and `routine_webhook`.
They reach only the bot's own routines, so every bot test-runs its own with
`run_routine`, also when another bot asks it to (such as a
[team manager](bot-collaboration.md#team-management)). A bot never
runs another bot's routine: `run_routine` with an id that isn't its own fails
with a hint to ask that bot with `message_bot` instead.
The same setup guidance is included in newly rendered agent context and MCP
initialization instructions.

For chat setup, the bot should resolve the task, trigger and timezone from the
conversation, asking only about missing or ambiguous details. It should check the
task's required tools, sign-ins and files, list existing routines to avoid
accidental duplication, then save and report the returned configuration and next
occurrence. A saved webhook whose external sender is not connected must be
reported as awaiting connection. Testing runs the actual instruction, so creating
a schedule alone should not trigger an unsolicited test execution.

A deterministic ACP fixture exercises the actual advertised MCP subprocess:
initialize, discover all six tools, reject an invalid schedule, create, read,
edit, pause/resume, fetch webhook credentials, delete, refuse a routine that
isn't the bot's, and execute a test run.
This verifies tool wiring and persisted behavior; it does not guarantee how every
provider model will interpret an ambiguous natural-language request.

## Triggers

A routine has an OR list of triggers:

- `interval`: `seconds` (1 second through 365 days).
- `once`: `at` (Unix milliseconds, in the future when created).
- `cron`: a five-field `expression` and IANA `timeZone`, including DST.
- `webhook`: every authenticated JSON delivery runs the routine.
- `event`: `source`, `event` (`*` matches all), and optional exact-match `filters`.
  `messageContains` matches a substring of `text`. Source names are `slack`,
  `github`, `origin`, `microsoftTeams`, `linear`, `sentry`, `pagerduty`, and `email`.

For example, Sunday at 14:20 in Taipei is `20 14 * * 0` with `Asia/Taipei`.
The host computes and displays the next occurrence. Calendar parsing uses
[Croner](https://docs.rs/croner/3.0.1/croner/), with chrono-tz for named zones.

## Execution

The host checks deadlines every second, even with no client connected. Definitions,
next deadlines and run records are saved in SQLite (`routines.v1`). Due triggers
are coalesced into one run per routine; only one routine per bot is dispatched at a
time. Bot actors serialize routine work with ordinary conversation work.

A run talks in the bot's chat: a "Routine · <name>" notice, then the run's trace and
its reply, which is the result, all in the turn the notice takes when the run begins (a
message sent meanwhile gets the next one). It runs in a fresh agent session of its own
(`routines::session_slot`, kept in kv `lane.session.<bot>@<bot>/routine-<run>` so a
restart can resume it, and forgotten once the run's outcome is saved). When the run ends, the agent stops if nothing else has a
session open (it ran just for the run; not on Windows, see
[agent processes](../architecture/conversation-runtime.md)); otherwise only the run's
session closes (agents that can close sessions). Either way a frequent schedule
doesn't keep a set of MCP servers alive per run. A successful run alerts with its
reply through normal encrypted push delivery; a failed or interrupted one leaves a
"Routine could not finish" notice. Returning `(pass)` or no text completes silently.
Replying to a run's result goes to the bot's main session, with the result quoted. A test run can run a paused routine. Pausing or
deleting cancels pending runs; work already running can finish. Deleting a bot
cancels its pending work. Missed intervals coalesce rather than replaying a backlog.

Pending runs survive a restart. The actor claims a run only when it reaches the
front of its queue, using the latest saved instruction. Runs interrupted during
execution become `recovering` and load their existing agent session. If the
provider cannot load it, they become `interrupted` instead of replaying the task
in a new session (the retry rules below still apply). Session recovery cannot
guarantee exactly-once external side effects; inspect the transcript before retrying
an interrupted action.

### Failed runs (`routines/retry.rs`)

A failed or interrupted run is tried again only when that can't repeat work. The
host checks the run's turn for a tool call whose ACP kind isn't `read`, `search`,
`fetch` or `think` (an unknown kind counts as acting; `store::acted_in_turn`).

- **Nothing acted** (a rate limit, an agent that didn't start, a session it couldn't
  reload): a new run with the same event waits 1, 5, then
  15 minutes (`attempt`, `notBefore`). The failure's notice is an info line ending in
  "Trying again in N minutes." with no alert; the routines list shows "Retrying at …"
  (TUI: `retry <when>`). A scheduled fire or a test run while it waits runs it at once,
  and a scheduled run that fails while the next fire is already queued has that fire
  as its retry ("Trying again now."). A waiting retry doesn't make the routine busy
  (deliveries queue as usual) and the bot's Stop leaves it; pausing or deleting the
  routine cancels it. A paused routine (unless it was a test run) isn't retried
  automatically.
- **It acted, hit its time limit (it could hold the bot that long again; this includes an
  agent whose session setup ran past it), or the three
  retries failed**: the failure notice (error style, alert)
  carries `retry: true` and the run keeps its event (`retryable`). The notice's retry
  button (TUI: `F`, or `F` on a picked notice) calls `retryRoutineRun`, which runs the
  work again with that event, also for a paused test run, and takes the button off.
  Only a routine's latest such failure keeps its event (an older one's button goes),
  so failed webhook payloads can't pile up; deleting the routine (or its bot) drops the
  event and the button. Retrying while the routine has an active run is refused
  (busy); a paused routine is refused until it's resumed.

Each run has a timeout (default one hour, configurable from 1 to 86400 seconds).
A hung agent is terminated so subsequent work can proceed. When the time limit cuts
session setup short, an agent that can load sessions is stopped too, so a session it was
still opening can't stay open unknown to the host; one that can't keeps running with the
sessions it has. A run's outcome is
saved before it is reported, and a failure notice has a stable transcript ID, so a
restart never reports it twice. Failed reporting retries without rerunning the agent.
The latest 1000 terminal runs are retained globally, plus active, unpublished and retryable runs;
the panel returns the latest 200 runs per bot. Deduplication lasts while the
corresponding run record is retained.

Intervals retain their original cadence; missed deadlines coalesce into one
pending wake, including when a prior run is still active. Enabled time schedules
keep the computer from idle sleep. Explicit sleep, closing the lid and powering
off still suspend local execution. The host uses an exclusive data-directory lock
to prevent two daemons from firing the same schedules. Shutdown stops scheduling
before stopping actors.

## Webhook delivery

Every webhook routine has a key and two addresses (`routineWebhook`): the public
`{cloud}/v1/hooks/{computerId}/{routineId}` through the Codync cloud (null while the
cloud is off) and the local `http://127.0.0.1:<port>/hooks/routines/<id>`. Both run
the same checks (`host/src/routines/hooks.rs`); the protocol is
[spec §7.8](../reference/remote-relay.md).

- **Authentication**: `Authorization: Bearer <key>`, or `X-Hub-Signature-256` (an
  HMAC-SHA256 of the body with the key). For GitHub: payload URL = the public URL,
  content type `application/json`, secret = the key. `routineWebhook` with
  `rotate: true` replaces the key; the old one stops working at once, including in
  the cloud.
- **Events**: a JSON object is the event as-is; a GitHub delivery becomes
  `{source:"github", event:"<event>.<action>", repo, sender, text, url, payload}`
  (GitHub `ping` succeeds without running); other bodies become `{text}`. Event
  triggers filter on these fields, for example:

```json
{"source":"github","event":"pull_request.opened","repo":"owner/repo","text":"A PR opened"}
```

- **Delivery**: `X-Delivery-Id`, `X-GitHub-Delivery` or `Idempotency-Key`
  deduplicates retries against retained run records. Bodies are limited to 64 KB.
  A paused routine refuses (`409`); a non-matching event is accepted and ignored.
  While the routine has a run under way or queued (a retry waiting for its time
  doesn't count), the local endpoint answers `409` and the cloud
  queue retries later (30 s doubling to 15 min).
- **Offline**: the cloud keeps public deliveries for up to 72 hours and hands them
  over one at a time once the host is back; the host acknowledges each.
- **Trust**: deliveries are not end-to-end encrypted. The cloud terminates HTTPS,
  sees the content and stores the key to check signatures. The host checks the key
  again before anything runs.

Codync does not create provider subscriptions: the user (or the bot, with the user)
pastes the URL and key into the sending service. Do not describe an event routine as
connected before a real delivery has arrived.

## API

All normal methods use the existing authenticated host API / authorized E2E channel:

| Method | Body |
|---|---|
| `routines` | `botId` |
| `routineSchedule` | `triggers` + preferred `timeZone` to load, or `draft` to preview; returns hydrated form, validated triggers, summary, warning, next run |
| `saveRoutine` | `botId`, optional `id`, `name`, `instruction`, `schedule` form or `triggers`, optional `enabled`, `timeoutSeconds` |
| `setRoutineEnabled` | `botId`, `id`, `enabled` |
| `deleteRoutine` | `botId`, `id` |
| `runRoutine` | `botId`, `id` (a waiting retry runs now instead) |
| `retryRoutineRun` | `botId`, `runId` of a failed run whose notice offers a retry; returns the new `run` |
| `routineWebhook` | `botId`, `id`, optional `rotate`; returns `url` (public or null), `localUrl`, `key`, `connected` |

The local-only `routineCall` backs the MCP tools. Each MCP instance is scoped to
its bot. Phone and desktop panels refresh while visible; definitions do not currently
participate in offline client caches. Run transcript entries use normal rev/SSE.

## Reference and parity

Observed on the installed Grok Bot on 2026-09-26: a routine list in conversation
details, instruction and trigger details, pause/resume, delete, and chat-based
editing. Creation produces a linked transcript card. The shipped renderer also
contains test-run and webhook credential controls, interval/one-shot/cron schedule
formatting and multiple provider trigger types.

The local reconstruction at
`/Users/libokai/mycode/nimplex/sandbox/grok-bot-architecture-2026-09-15` contains a
partial automation fire consumer with event matching, durable wake delivery and
completion reporting. Most automation modules it imports are absent. Its renderer
and private cloud are not source-complete references.

Remaining differences from the reference: provider subscription provisioning,
cloud execution while the host is offline (deliveries wait in the cloud instead), and
inactivity auto-pause. The Apple UI
uses Codync's shared native controls. This is not a verified complete one-to-one
reconstruction of every Grok Bot routine behavior.

## Validation

`host/tests/routines_e2e.rs` starts isolated hosts and deterministic ACP agents,
checks real scheduled execution, webhook credentials, delivery deduplication,
pause/delete/edit while queued, crash recovery in the same session, unsupported
session recovery, hung-agent timeout, recurring work after failure, single-host
ownership, silent completion, and the advertised routine MCP server (including key
rotation). `host/tests/routine_retry_e2e.rs` checks a webhook run that failed before
acting waiting 1 minute with its event (and running at once on a test run), and one
that acted waiting for `retryRoutineRun`, which reuses the event once.
`host/tests/routine_unread_e2e.rs` checks that a routine result counts one unread in the
main chat, that `markRead` clears it once the run has released its session, and that a
follow-up reply counts once again when it is final. Unit tests cover
the retry ladder, a queued scheduled fire standing in for the retry, paused routines,
keeping only the latest failure's event, the user retry's busy/paused refusals and
deleting the routine (`routines/retry.rs`), waiting retries vs. Stop, the tick and a
scheduled fire (`routines/runner.rs`), timezone calculation, invalid schedules, event filters,
persistence, pause/resume, bot ownership, bearer/HMAC checks, GitHub event mapping and
the relay's hook registration, acks and retries. `cloud/test/hooks.test.ts` covers the
edge checks, queue limits, deduplication, ordering, retry and expiry; the cloud e2e
(`npm run e2e`, step 5b) delivers through a real Worker and host, including a signed
GitHub delivery queued while the host was down. Swift tests cover the wire model and
transcript links.

## Public webhook ingress without a fixed IP

Research checked on 2026-09-26: [OpenClaw](https://docs.openclaw.ai/cli/webhooks)
uses Tailscale Funnel and the [Hermes webhook adapter](https://github.com/NousResearch/hermes-agent/blob/main/website/docs/user-guide/messaging/webhooks.md)
needs a reachable server URL. Codync instead reuses the host's outbound relay socket
(above): no fixed IP, open port or per-user tunnel, and deliveries survive the
computer being off. A user's own Cloudflare Tunnel to the local endpoint also works
but needs their own account, domain and `cloudflared`, and drops deliveries while the
computer is off.

## Usability comparison (2026-09-27)

The installed Grok Bot was inspected through Computer Use. Its sidebar emphasizes
the routine name and a human-readable schedule (for example, Every Sunday at
2:20 PM). Details expose Instruction, When to run, Pause, Edit and Delete. The
conversation contains a linked creation notice; an existing edit draft refers to
the routine by name. No reference routine was run, paused or deleted during this
comparison.

Codync retains that compact sidebar and the chat-based setup path, while providing
a direct editor: a light Repeat choice with the time and day for the common
schedules, and Custom for a cron typed as written. Existing duration intervals keep their cadence until
explicitly replaced. Editing hydrates the original expression, timezone and interval
exactly; compound and provider-event triggers retain a lossless Keep existing
triggers fallback. Expired one-shot timestamps are preserved when editing other fields.

Saved definitions appear immediately. Paused schedules omit next-run deadlines.
List rows and details expose queued/running/recovery/failure states.
Edit with bot includes the routine ID in a draft without sending it.

## Cron scheduling contract

All newly configured recurring schedules in the editors use `cron` and run through
the host's existing Croner scheduler. The editors send the Repeat choice as
`calendarStyle` (`hourly`, `daily`, `weekdays`, `weekly`) with the hour, minute and
weekday, or `custom` with the typed expression, and the host compiles it. A loaded
schedule in a style the editors don't offer (every few minutes, several days,
monthly) opens as Custom with its cron unchanged; switching to Custom starts from
the cron the host compiled (empty while the host is still checking the last change).
Cron generation, parsing, validation, timezone checks, descriptions and next-run
calculation all live in `host/src/routines/editor.rs` and `schedule.rs`. The Swift
draft is only a Codable form DTO. The editor debounces preview requests, discards
stale responses, shows host errors and disables Save until the current draft is
validated. Save sends the form to the host, which independently compiles and
validates it again. Both direct trigger API calls and Bot MCP calls use the same
host scheduler. Run timeout text is also parsed and validated by the host.
The built-in agent instructions also select cron for recurring clock schedules.

Existing elapsed-duration intervals remain readable/editable without silently
changing their execution times. An interval anchored to creation time cannot be
converted to a wall-clock cron without changing its cadence. One-time schedules
retain their absolute timestamp: five-field cron has no year or run-count guard.
Webhook/event triggers remain event-driven. These paths are intentionally not
misrepresented as cron. The host must still be running and awake.

Rust regression tests cover default cron creation, preset round trips, arbitrary
minute selection, timezone validation, custom expression preservation, existing
interval and expired one-shot preservation, and next occurrences across month
boundaries. An isolated HTTP integration test previews, saves and reloads the same
form and verifies direct saves reject invalid drafts. Swift tests cover wire DTO
decoding and millisecond date binding; there is no Swift cron implementation.
