# Follow-ups

A follow-up is a deadline a bot sets for itself: "look at the build again in 20 minutes", "see whether they
replied tomorrow morning". When it is due the bot gets one turn in its own chat, in the session the conversation
runs in, and the follow-up is gone. It is how a bot keeps its word when it says it will check back, with the
computer doing the remembering instead of the model.

| | Routine | Follow-up |
|---|---|---|
| Whose | The user's standing task | The bot's own reminder |
| How often | Every time a trigger fires | Once |
| Session | A fresh one per run | The bot's chat, with what was said before |
| In the apps | A row under Details, with a switch | A line in the chat |
| When it fails | Retried, with a Try again button | A failure line, like any turn |

Code: `host/src/routines/follow_up.rs`; the turn itself is the bot actor's (`agent/bot/queue.rs`).

## Tools

On the built-in `routines` MCP server every agent has:

- `follow_up {minutes, note}`: `minutes` from now, 1 to 43,200 (30 days). A delay, not a clock time: an agent
  doesn't reliably know what time it is. The answer has the follow-up (`id`, `at`) and `atLocal`, the time on the
  computer's clock, for the bot to tell the user.
- `cancel_follow_up {id}`. The user's own cancel, from the routines list, is the host method `cancelFollowUp`
  ([below](#in-the-routines-list)).
- `list_routines` also answers `followUps`, the ones still waiting.

`follow_up` works only from a turn in the bot's own chat. In a group room, or while the bot answers another bot's
`message_bot` request, it is refused (`own_chat`: the running turn's lane is another chat, or the team's turn is
a request): the line and the later turn go to the bot's own chat, where the room or the asking bot would never
see them. The refusal tells the bot to say in its reply that it can't check back from there.

The server's instructions tell the bot to set one whenever something in its own chat needs another look later,
never to say it will check back without setting one, and to use a routine for anything recurring.

The bot's own instructions say it too (`routines::BRIEF` in the snapshot, `chat/context.rs`), and name what not to
use: the engine's own wake-ups, timers, loops, sleeps and cron. Without that line a model reaches for them first.
Claude Code has a wake-up tool of its own, and a bot asked to check back in a minute used it instead of
`follow_up`: it works only while the agent's process lives (it stops when idle, and with the host), and the user
sees no line and can't cancel it. A session whose instructions were written before follow-ups existed is told the
same line on each turn of its own chat instead, until its instructions are rendered again (`agent/bot/turn.rs`).

## Limits

- **Ten waiting per bot.** The eleventh is refused until one is cancelled or happens.
- **The note** is at most 500 tokens (counted as memory counts them) and passes memory's checks for hidden
  characters, prompt-carrying links, planted instructions and secrets (`routines/kept.rs`): the host puts it into
  a later prompt, so what a web page planted in one turn must not ride into the next. A refusal tells the bot why.
- **25 in a row.** A follow-up set during a follow-up's own turn continues its chain: its `chain` is one more
  than that follow-up's, and one set in any other turn starts at 0. Past `chain` 24 the tool refuses and tells the
  bot to say where things stand and ask the user, so a watch that never ends can't spend the user's plan
  unattended. A follow-up the bot sets after the user answers starts a new chain.

## When it is due

The routine scheduler's one-second tick (`routines/runner.rs`) hands each due follow-up to its bot
(`Cmd::FollowUp`). It waits its turn in the bot's queue like a message; nothing is interrupted.

- **Its turn begins** (`begin_follow_up`): the follow-up leaves the list for good, a `Follow-up · <note>` line goes
  into the chat, and the bot is prompted in its main session with the note, how long ago it was set, and that
  the note is its own reminder, not a message from the user and no new permission.
- **The reply** is an ordinary one: it shows in the chat and alerts like any finished turn. `(pass)` says nothing
  and alerts no one, for a look that found nothing yet; the bot sets another follow-up to keep watching.
- **It is a host turn** (`HostTurn::FollowUp`, analytics source `follow_up`), not the user's: memory may only gain
  lines, and `request_decision` isn't available.
- **Restart mid-turn:** resumed, since the follow-up is already gone, and as the follow-up it is: the running
  turn's mark (`turn.inflight`, `agent/bot/inflight.rs`) says so and keeps its place in the chain, the resume
  prompt allows `(pass)` again, a pass still alerts no one, `turn_finished` still reports `follow_up`, and a
  follow-up set after the restart continues the chain, so a restart never resets the limit on a watch.
- **The state can't be saved** (the disk is full, SQLite is busy) when its turn begins or Stop drops it: the
  follow-up stays in the list and is handed to the bot again on the next tick.
- **The computer was off or asleep:** it goes out on the first tick after. A waiting follow-up keeps the computer
  from idle sleep like a scheduled routine, and a cloud computer wakes for it (`sleep_window`).
- **Stop** drops follow-ups already due and waiting behind the running turn, with a `Follow-up cancelled` line;
  ones set for later stay.
- **The bot is deleted:** its follow-ups go with its routines.
- **The turn fails:** the usual failure line and alert. It isn't retried: the bot or the user sets another.

Follow-ups are saved with the routines (`routines.v1`), so they survive a restart.

## In the chat

The host writes three lines, as plain info notices every client already shows:

- `Follow-up in 20 minutes: <note>` when it is set. The wait is relative, so it is right in any time zone:
  minutes as given below two hours, then `about N hours`, then `about N days`.
- `Follow-up · <note>` when its turn begins.
- `Follow-up cancelled: <note>`.

They show the note's first line, at most 120 bytes. The phrases are in the shared language catalogs
([interface languages](../guides/localization.md)), so iPhone, Android, desktop and terminal translate them with
no code of their own. The first line is one phrase, `Follow-up in {when}: {note}`, and the wait is a phrase of
its own inside it (`1 minute`, `{count} minutes`, `about {count} hours`, `about {count} days`): the wait ends at
the first colon, so a note that reads like a wait ("wait 5 minutes: then check") stays a note.

## In the routines list

The follow-ups still waiting are listed where the bot's routines are (the Details page of its details; the
terminal's routines sheet), above the routines, so the user sees what the bot will come back to and can drop one
without asking it. The `routines` method answers them as `followUps` beside `routines` and `runs`, the next one
to happen first, and the apps
already read that listing every three seconds.

- **A card of its own look**, so it doesn't read as a routine: a dashed outline (a routine's is solid), a clock,
  the note (two lines at most), then when it happens: `Follow-up at 15:42` today, with the date on another day,
  and `Follow-up due now` once it is due and waits behind what the bot is doing. No switch and nothing to open:
  it is the bot's own, and it happens once.
- **✕ cancels it** (`cancelFollowUp {botId, id}`, a method phones may call, unlike the bot's tools). The chat gets
  the `Follow-up cancelled` line, and `follow_up_cancelled` says `by: user`. There is no confirmation: nothing has
  run, and the bot can be asked to set it again. A card can be a moment old: for one that began or was cancelled
  since, the method answers `gone: true` instead of an error (the card leaves on the next load), with no line in
  the chat and no event. The bot's own `cancel_follow_up` still gets an error for an id that isn't waiting.
- **The time is in the app's language**, like the routine editor's dates (`useIntlLocale`, the iPhone's
  `\.locale`, Android's `appLocale`), not the device's.
- **Terminal:** the follow-ups come first in the list (`RoutineList::rows`), marked `◷` where a routine has its
  on/off dot; `x` cancels the one picked, and a routine's other keys do nothing on it. The row leaves once the
  host has dropped it; a cancel that failed says why, and the follow-up stays in the list. The sheet loads again
  when a notice arrives in that chat, so one that fires or is dropped leaves the list; the cursor stays on the
  row it picked, so a key pressed just then never lands on another row.
- Code: desktop `views/routines/RoutinesView.tsx` (`followUpLine` in `routine.ts`), iPhone
  `CodyncUI/Routines/RoutinesView.swift` (`FollowUp.line`), Android `RoutinesView.kt` (`FollowUp.line` in core),
  terminal `tui/sheets.rs` and `tui/manage/routines.rs`. The dashed card is `outlinedCard(dashed:)` on both
  phones and `.follow-up-card` on the desktop.

## Analytics

`follow_up_set` (`outcome`, `minutes`, `chain`), `follow_up_fired` (`late`), `follow_up_cancelled` (`by`; the
user's comes from `api::dispatch` like every API call, so it also says which app), and
`turn_finished` with `source: follow_up` for how the turn went:
[analytics events](../reference/analytics-events.md).

## Tests

- The list: `the_user_cancels_a_waiting_follow_up_from_the_routines_list` (host), `followUpLine` (desktop
  `routine.test.ts`), `FollowUp.line` and the listing's decoding (iPhone `RoutineTests.swift`, Android
  `RoutineTest.kt`), `a_follow_up_sits_above_the_routines_and_x_cancels_it` (also: a cancel that failed leaves
  the row), `a_reload_keeps_the_cursor_on_the_row_it_picked` and the sheet's render test (terminal).
- Unit (`routines/follow_up.rs`, `kept.rs`): the wait's wording, the note's checks, the ten and the chain limit,
  only from the bot's own chat, surviving a restart, leaving with the bot, what Stop drops, one that couldn't be
  taken or dropped is handed over again. `agent/bot/inflight.rs`: the running turn's mark keeps a follow-up's
  place in its chain; what is no mark resumes nothing.
- End to end (`host/tests/routines_e2e/follow_ups.rs`, a real host and a fixture agent): a follow-up that came due
  while the host was off takes one turn in the chat's own session and is gone; one with nothing to say leaves no
  reply; one set from that turn has `chain: 1`; one asked for in a room turn is refused and the same call in the
  bot's own chat is taken; one cut off by a restart is resumed as a follow-up, and one it sets then continues the
  chain (`chain: 3`, then 4); Stop drops the one queued behind a running turn; the tools over the real MCP
  subprocess (`routine_agent.py`).
- Each client's language test: the lines translate around the wait, also when the note reads like a wait.
- With a real agent (Claude Code, 2026-11-14, an isolated host and `codync-host send --wait`): asked to check
  back in a minute, the bot set a follow-up, the line appeared, and a minute later it answered in the same chat; a
  routine told to count two numbers a run answered 1 and 2, then 3 and 4, from its progress note.
