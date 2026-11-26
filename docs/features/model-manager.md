# Model Manager

One agent, one model and that model's settings applied to many bots at once, on every computer the
account reaches. Desktop app only for now: batch swapping is a desktop-shaped workflow, and each
phone's bot editor already picks a model per bot (see *Notes*).

## Opening it

The sliders button at the top of the bot list, left of the computer filter (`ChatWindow`'s sidebar
header); in a narrow sidebar, the same sliders button sits in the rail actions under the roster. It
opens a sheet, top to bottom: the agent and model picker, a search field over the bots, `Select all`
/ `Unselect all`, the bot list with a checkbox on each row, and `Apply`.

## Choosing what to apply

The top section is the same picker the bot editor uses (`AgentModelPicker`, not a copy): pick the
agent first (`Search agents`), then the model (`Search models` — typing `free` lists the free
models), and the model's own settings under it, such as the reasoning slider. Leaving the model at
`Default` applies the agent's default to every checked bot, which every computer allows.

## Choosing the bots

The list is every bot on every computer the account reaches — the sidebar's computer filter, hidden
computers and bots hidden from the roster don't limit it; a bot's computer is named under it when
the account has more than one. Groups are not listed: they carry members, not a model of their own.
The search field filters by name, description and last message, reusing the sidebar search's matcher
(`matchBots`); `Select all` / `Unselect all` act on what the filter is showing.

## Apply

Each checked bot is saved one at a time, in roster order, as a **minimal patch**: only the bot's id,
backend, login profile, model and model options (plus `command: null` when the target is a listed
agent, as the bot editor's agent change does — a stale custom command would otherwise keep
launching). The login profile is the bot's own when its agent stays the same (or the default login
when it moves to another agent, because a profile belongs to one agent). The picker's rail still
shows one seat per agent (its default login), because that seat chooses the model; each row is then
checked and saved on the login it keeps.
The host merges
the patch over the bot's state as it is at that moment under one lock (`updateBot`'s patched merge),
so an unrelated change that landed while the batch ran is never written back, and on another agent
the bot moves to that agent's Auto mode on the host's own rule while a login profile that doesn't
fit the new agent is dropped by the host's profile settle. The patch itself doesn't send the
fallback list, but the host's fallback tidy runs on every bot update: a fallback step that would
duplicate the new primary agent (same backend and login) is dropped then, exactly as the bot
editor's agent change does. As with any model change in the bot editor, applying restarts the bot's
agent session — the chat history stays and the agent starts fresh with the new model. Fast mode is
not touched.

Before anything is saved, the catalog for each bot's own login is read for the chosen model
(`agentModels`, the same call the picker makes, at the same 300 s patience). A bot whose
agent-on-that-login doesn't offer the model is skipped and marked *Not available on {computer}*; a
bot whose catalog couldn't be read or hasn't loaded yet is not written either — its row is marked
*Couldn't check {computer}'s models*, because saving an unverified model id would be worse than
doing nothing. Only the agent's default model (and a custom command, which the manager doesn't
offer) skips the catalog checks. A closed sheet or a changed pick stops the run: bots already saved
keep their new model, the rest are left alone. Row results (✓ *Applied*, a skip reason, or the
save's error) stay on the rows until the sheet closes.

Catalogs load once per computer and login profile per apply, so swapping a model onto twenty bots
costs one read per distinct login plus the saves. Choosing the agent's default model skips the
catalog checks entirely and is allowed on every computer.

## Analytics

One `model_manager_applied` per Apply — including a run that ended early (sheet closed, pick
changed): `requested`, `applied` and `failed` counts. `failed` counts the catalog skips, the
unverifiable rows and the save errors; rows the run never reached are the remainder
(`requested - applied - failed`) ([analytics events](../reference/analytics-events.md)).

## Notes

- The picker's Fast-mode control is deliberately not wired for batch applies.
- A custom command is not offered as a target: a batch apply has no field for the command a custom
  backend needs; that stays with the bot editor.
- Phones and the terminal UI keep their per-bot editors; a batch UI there is a possible follow-up,
  not part of this change. Per the repo's cross-platform UI rule this desktop-only scope is
  documented here and in the change summary with that reason.
