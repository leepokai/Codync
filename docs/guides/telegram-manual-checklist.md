# Telegram: manual checklist with a real bot

Nothing below has been run against real Telegram. The host's tests use a fake Bot API and real scripted agents
(`host/src/telegram/tests.rs`). Real Telegram is yours to check.

## Set up

1. In Telegram, talk to @BotFather: `/newbot`, pick a name, copy the token (`123456:ABC…`).
2. In Codync (until the Channels screen exists, with the local API): `addTelegramBot` with
   `{name: "home", token, botId: "<one of your bots>"}`. The answer shows the bot's `@username`; the token is never
   shown again.
3. `pairTelegram {name: "home"}` returns a one-time code (valid 10 minutes).
4. In Telegram, open your bot and send `/start CODE` (or `/pair CODE`).

## Check each of these

- [ ] After step 4 the bot answers "Paired…" and `telegramStatus` lists your Telegram user id under `allowed`.
- [ ] The same code a second time, a wrong code, and an expired code are all refused ("That code didn't work").
- [ ] A second Telegram account that was never paired gets **no** answer to ordinary messages.
- [ ] A message from you reaches the bot: it shows in the bot's chat in Codync, and the reply arrives in Telegram,
  growing in the same message while the agent writes.
- [ ] A reply over 4096 characters arrives cut in the message and complete as `reply.txt`.
- [ ] `/estado`, `/rutinas`, `/detener` (also `/status`, `/routines`, `/stop`) answer at once, and no model turn or token
  usage appears for them.
- [ ] Ask the bot to run a command with Approvals on "ask": a message with buttons arrives; tapping one lets the agent
  continue and the buttons are replaced by "✓ choice".
- [ ] Answer the same approval in Codync first, then tap the Telegram button: it says the approval was already answered.
- [ ] Messages in a group chat where the bot was added are ignored.
- [ ] `setTelegramAllowed {allowed: false}` for yourself: you get no more answers.
- [ ] `removeTelegramBot`: the bot stops answering; the token is gone from the vault.
- [ ] Two Telegram bots with different names, each assigned to a different Codync bot, answer independently.
- [ ] Stop the host mid-reply and start it again: no message is delivered twice (the poll offset is kept).
