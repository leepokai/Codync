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

The list is the roster, so it spans all the account's computers; a bot's computer is named under it
when the account has more than one (`RosterItem`, the same rows the sidebar search shows). The
search field filters by name, description and last message, reusing the sidebar search's matcher
(`matchBots`); `Select all` / `Unselect all` act on what the filter is showing.

## Apply

Each checked bot is saved one at a time, in roster order: its draft is rebuilt with the chosen
backend, model and model options and saved to that bot's own computer, exactly as the bot editor's
Save does (`BotStore.save`). Fast mode and session notes are not touched. Before anything is saved,
each involved computer's catalog is consulted for the chosen model (`agentModels`, the same call the
picker makes); a bot on a computer whose agent doesn't offer it is skipped and marked
*Not available on {computer}* instead of being written, and the rest still save. A computer whose
catalog couldn't be read (offline) is left to the host: the save decides there. Row results
(✓ *Applied*, the skip reason, or the save's error) stay on the rows until the sheet closes.

Cross-computer catalogs load once per computer per apply, so swapping a model onto twenty bots costs
one catalog read per computer plus the saves. Choosing the agent's default model skips the catalog
checks entirely.

## Analytics

One `model_manager_applied` per Apply (desktop `track`): `requested`, `applied` and `failed` counts,
where a skipped bot whose computer lacks the model counts in `failed`
([analytics events](../reference/analytics-events.md)).

## Notes

- The picker's Fast-mode control is deliberately not wired for batch applies; sessions aren't
  touched either.
- Phones and the terminal UI keep their per-bot editors; a batch UI there is a possible follow-up,
  not part of this change.
