# Bot Manager

One agent, one model and that model's settings applied to many bots at once — and many bots
paused or resumed at once — on every computer the account reaches. Desktop app only for now:
batch work is a desktop-shaped workflow, and each phone's bot editor already picks a model per
bot (see *Notes*).

## Opening it

The sliders button in the bot list's shared header, left of the computer filter (`ChatWindow`'s
sidebar header — the one header every page shares, so the sheet opens from anywhere). It opens a
sheet, top to bottom: the agent and model picker, a search field over the bots, `Select all`
/ `Unselect all`, `Pause checked` / `Resume checked`, the bot list with a checkbox on each row, and
`Apply`.

## Choosing what to apply

The top section is the same picker the bot editor uses (`ModelPicker`, not a copy): its rail
carries the account's agents — one seat per agent, its default login, because a batch writes no
login profile (each bot keeps its own, per the credential safety policy) — and an agent that isn't
installed is named "(not installed)" exactly as the editor names it. The panel picks the model
(`Search models` — typing `free` lists the free models) with the model's own settings under it,
such as the reasoning slider. Leaving the model at `Default` applies the agent's default to every
checked bot, which every computer allows.

## Choosing the bots

The list is every bot on every computer the account reaches — the sidebar's computer filter, hidden
computers and bots hidden from the roster don't limit it; a bot's computer is named under it when the
account has more than one. Groups are not listed: they carry members, not a model of their own.
Under each bot's name is what it runs, in the app's muted small text: the agent as the model picker
names it (its raw id when hello doesn't name it), the login profile when the bot picks one, then the
model id exactly as the computer stores it — `OpenCode · opendesign/deepseek-v4.1-flash-eco`, or
`OpenCode · Work · opendesign/deepseek-v4.1-flash-eco` on a bot that picks a login profile. The
string is `botModelPath` (`apps/desktop/src/renderer/lib/bot-model-path.ts`), the same builder the
roster row's hover tip calls and the same one this sheet's search words come from, so the list, its
search and the sidebar can never name a bot differently. The model id is the agent's own
`provider/model`, so the provider is already inside it; the model's display name (what the picker
calls it) belongs to that agent's catalog, which the host only answers by starting the agent, so the
row does not guess one. A custom-command bot, or one whose model isn't readable, shows no line rather
than a wrong one.
When the row is too narrow to show the whole path — the sheet is draggable and the window can be
small — the line ends in an ellipsis and **hovering it shows the whole path** in the app's own
tooltip (a `title`, the same mechanism as the roster row). The tip is set only while the line is
really clipped: a path that fits carries none.
The search field filters by name, description and last message, through the same word list and
rule the window's search uses (`botWords` over `matchWords`, extracted from the search palette's
own bot rows). Beside it, the `Search models too` switch (on to begin with) adds each bot's agent,
login profile and model path to the words a row is matched by — the whole path, each part of it,
and the model id read as words, so `deepseek`, `opendesign` or `deepseek v4.1 flash` finds the
bots that run it. Off, the field searches names, descriptions and last messages alone, exactly as
the window's search does; the shared word list is extended, never forked, so the palette's bot
rows keep their own words. On is the design that makes sense rather than a feature forced on: this
switch narrows one search, it changes nothing else in the app, and a bot list searched for
`deepseek` is looking for the bots that run it — off would hide what the sheet exists to show.
`Select all` checks what the filter is showing, and `Unselect all` clears the whole selection,
hidden bots included. A bot that is paused says `Paused` under its name, so a fleet's state reads
at a glance.

## Apply

Each checked bot is saved one at a time, in roster order, as a **minimal patch**: only the bot's id,
backend, model and model options (plus `command: null` when the target is a listed agent, as the bot
editor's agent change does — a stale custom command would otherwise keep launching). The host merges
the patch over the bot's state as it is at that moment under one lock (`updateBot`'s patched merge),
so an unrelated change that landed while the batch ran is never written back, and on another agent
the bot moves to that agent's Auto mode on the host's own rule while a login profile that doesn't
fit the new agent is dropped by the host's profile settle. The patch itself doesn't send the
fallback list, but the host's fallback tidy runs on every bot update: a fallback step that would
duplicate the new primary agent (same backend and login) is dropped then, exactly as the bot
editor's agent change does. As with any model change in the bot editor, applying restarts the bot's
agent session — the chat history stays and the agent starts fresh with the new model. Fast mode is
not touched.

Before anything is saved, each involved computer's catalog is read for the chosen model
(`agentModels`, the same call the picker makes, at the same 300 s patience). A bot on a computer
whose agent doesn't offer the model is skipped and marked *Not available on {computer}*; a computer
whose catalog couldn't be read or hasn't loaded yet is not written either — its rows are marked
*Couldn't check {computer}'s models*, because saving an unverified model id would be worse than
doing nothing. Only the agent's default model (and a custom command, which the manager doesn't
offer) skips the catalog checks. A closed sheet or a changed pick stops the run: bots already saved
keep their new model, the rest are left alone. Row results (✓ *Applied*, a skip reason, or the
save's error) stay on the rows until the sheet closes or a new pick clears them.

Cross-computer catalogs load once per computer per apply, so swapping a model onto twenty bots costs
one catalog read per computer plus the saves. Choosing the agent's default model skips the catalog
checks entirely and is allowed on every computer.

## Pause and resume in bulk

`Pause checked` and `Resume checked` act on the same checked bots Apply does — only the checked
bots the filter still shows, one at a time, each on the same `updateBot` path the roster's own
pause toggle uses, and more minimal: `{ id, paused }` only, so the host's patched merge keeps every
other setting the bot has. The host's pause rules apply exactly as they do from
the roster, in every class: a paused bot never *starts* anything new (no turn, no spend, no
one-shot work like titling or dreaming), messages sent to it are refused with a notice, its queued
routine runs are cancelled, a bot that is mid-turn follows the host's own pause semantics — the
in-flight turn stops and the held message is cancelled with a notice — and a host restart leaves
the paused state in place: recovery marks interrupted work and does not resume it, exactly as a
single roster pause does. Nothing new is invented here, and nothing wakes a sleeping bot to save
anything: `paused` is one persisted config field, written once by the patch. Pausing also survives
an Apply: a model patch never touches the flag unless it names it.

The sheet acts as the user, like Apply, so no team-permission gate applies to these clicks. A bot
pausing another bot's setup goes through the manage tool's own *Manage the team* gate and approval
card instead. The save rides the host's existing `updateBot` path, so it lands on the pause
change's own audit record (actor, target, old→new, timestamp) — this sheet adds no second trail.

Row results (✓ *Paused*, ✓ *Resumed*, or the save's error) stay on the rows until the sheet closes
or a new pick clears them; a bot that was already in the target state is saved the same way, and its
row outcome says where it ended.

## Analytics

One `model_manager_applied` per Apply — including a run that ended early (sheet closed, pick
changed): `requested`, `applied` and `failed` counts. `failed` counts the catalog skips, the
unverifiable rows and the save errors; rows the run never reached are the remainder
(`requested - applied - failed`). One `bot_manager_paused` per bulk pause or resume, with the same
counts plus `paused` (true when the action paused) ([analytics events](../reference/analytics-events.md)).

## Notes

- The picker's Fast-mode control is deliberately not wired for batch applies.
- A custom command is not offered as a target: a batch apply has no field for the command a custom
  backend needs; that stays with the bot editor.
- Phones and the terminal UI keep their per-bot editors; a batch UI there is a possible follow-up,
  not part of this change. Per the repo's cross-platform UI rule this desktop-only scope is
  documented here and in the change summary with that reason.
