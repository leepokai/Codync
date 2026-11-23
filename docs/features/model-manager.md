# Model Manager

One agent, one model and that model's settings applied to many bots at once, on every computer the
account reaches. Desktop app only for now: batch swapping is a desktop-shaped workflow, and each
phone's bot editor already picks a model per bot (see *Notes*).

## Opening it

The sliders button at the top of the bot list, left of the computer filter (`ChatWindow`'s sidebar
header). It opens a sheet, top to bottom: the agent and model picker, a search field over the bots,
`Select all` / `Unselect all`, the bot list with a checkbox on each row, and `Apply`.

## Choosing what to apply

The top section is the same picker the bot editor uses (`AgentModelPicker`, not a copy): pick the
agent first (`Search agents`), then the model (`Search models` — typing `free` lists the free
models), and the model's own settings under it, such as the reasoning slider. Leaving the model at
`Default` applies the agent's default to every checked bot, which every computer allows.

## Choosing the bots

The list is the whole roster across all the account's computers — the sidebar's computer filter and
hidden-computer choice don't limit it; a bot's computer is named under it when the account has more
than one. Groups are not listed: they carry members, not a model of their own. The search field
filters by name, description and last message, reusing the sidebar search's matcher (`matchBots`);
`Select all` / `Unselect all` act on what the filter is showing.

## Apply

Each checked bot is saved one at a time, in roster order. The bot's state is read as it is at its
turn, so an unrelated change that landed while the batch ran is kept — only the agent, model and
model options are this apply's business. Changing the agent goes through the bot editor's own
semantics (`withBackend`: the mode becomes the new agent's Auto mode, the old agent's login profile
and same-agent fallbacks go). The draft is then saved to that bot's own computer, exactly as the bot
editor's Save does (`BotStore.save`); Fast mode and session notes are not touched.

Before anything is saved, each involved computer's catalog is read for the chosen model
(`agentModels`, the same call the picker makes, at the same 300 s patience). A bot on a computer
whose agent doesn't offer the model is skipped and marked *Not available on {computer}*; a computer
whose catalog couldn't be read is not written either — its rows are marked *Couldn't check
{computer}'s models*, because saving an unverified model id would be worse than doing nothing. A
closed sheet or a changed pick stops the run: bots already saved keep their new model, the rest are
left alone. Row results (✓ *Applied*, a skip reason, or the save's error) stay on the rows until the
sheet closes.

Cross-computer catalogs load once per computer per apply, so swapping a model onto twenty bots costs
one catalog read per computer plus the saves. Choosing the agent's default model skips the catalog
checks entirely and is allowed on every computer.

## Analytics

One `model_manager_applied` per Apply — including a run that ended early (sheet closed, pick
changed): `requested`, `applied` and `failed` counts, where catalog skips, unverifiable rows and
save errors all count in `failed` ([analytics events](../reference/analytics-events.md)).

## Notes

- The picker's Fast-mode control is deliberately not wired for batch applies; sessions aren't
  touched either.
- A custom command is not offered as a target: a batch apply has no field for the command a custom
  backend needs; that stays with the bot editor.
- Phones and the terminal UI keep their per-bot editors; a batch UI there is a possible follow-up,
  not part of this change. Per the repo's cross-platform UI rule this desktop-only scope is
  documented here and in the change summary with that reason.
