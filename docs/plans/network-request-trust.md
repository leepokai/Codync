# Requests from other computers: what's left

Status (2026-11-15): the levels are built. A network rule is `allow`, `auto`, `ask` or `deny`, and `auto` and `ask`
are ceilings on how a bot works on that sender's text (`auto` by default)
([codync-network: ceilings](../features/codync-network.md#ceilings-auto-and-ask)). This page lists what that doesn't
cover yet; what it means for the user: [risks](../reference/network-request-risks.md). Nothing below is built.

## Why more

A turn under a ceiling works no more freely than its level and can only read and answer the user with the host's
tools, but it still starts from the bot's conversation: a request's task forks the main session where the agent can,
and an answer (or a request to a bot that keeps them in its chat) runs in the main session itself:

- It sees the conversation the user had with that bot, so a sender can ask for what was said there.
- An answer's text stays in the main session, where it can act in a later turn of the user's own.
- It can read what the agent can read and send it back in its reply. A session of its own doesn't change that; only
  a folder the bot can't read outside of would.

That matters for a computer someone else can run code on: a cloud computer (its provider and anyone with its key),
or a server shared with others.

## Packages, in order

1. **A session per sender.** A request under a ceiling runs in a session of its own, slot
   `peer-<network>-<computer>-<agent>` (`Actor::slot`, next to a routine run's `routine-<run>`), kept in kv so a
   restart resumes it and closed when unused like any session. Fresh, never a fork of the main one as a request's
   task is now: a fork would carry the user's conversation along. It lasts across that sender's requests, so "now
   the second half" works. The conversation sheet with that peer shows the turn's trace and gets "Forget this
   conversation". Answers to this computer's own requests stay in the main session: the turn that works on the task
   has to read them.
2. **A level when a computer is added.** A computer no rule matches gets `auto` now, and the computers already in a
   network kept `allow` with a rule each. The desktop app and the terminal could still ask "What may this computer's
   bots do here?" when they make the invite, and save the answer as a rule `<level> *@<computer> → *`.
3. **Cloud computers are never above `ask`.** The host can't take a sender's word for what it is, so this is a
   check on this side: a computer the account lists as a Codync Cloud machine can't be given `allow` or `auto`, in
   the apps or by `setNetworkRules`.
4. **The cloud assistant's level.** One setting per computer beside its switch (`setAssistants {level}`), `ask` by
   default; a letter from `cloud-assistant@<computer>` takes the stricter of it and its computer's rule.
5. **Less in a turn under a ceiling:** no computer use, no `start_dev_server`; an unanswered card declined after 10
   minutes with the sender told; "who asked" on the approval card.
6. **A ceiling that outlives its turn.** It stands until the next thing in the bot's queue and isn't kept across a
   host restart (a reply turn under one isn't resumed). Work a background subagent goes on with after that is
   answered as the bot is set. Tie the ceiling to the session (package 1) instead of the queue.
7. **An agent with no modes.** It takes no turn under a ceiling, so it also can't read an answer from a computer
   under `ask`, `auto` or `deny`. Decide whether such an answer should run with every request on a card instead.

## Open questions

- **Rules name a computer by its roster name.** If that can change without this computer's user doing it, a rule
  should hold the endpoint id instead. Check `roster.rs` before package 2.
- **The queue.** A request waiting on a card holds the bot's queue, so the user's own messages wait behind it.
  Running peer sessions beside the main turn would fix it and is a much larger change to the actor.
