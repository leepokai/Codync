# Risks of requests from other computers

What a [codync-network](../features/codync-network.md) rule protects this computer from, and what it doesn't. A
computer in your network can have its bots message the bots here, and their answers to your bots come back here too.
Whatever sends that text (the other computer's user, its agent, a web page its agent read, or whoever runs code
there, such as a cloud provider or anyone with its key) gets to put words in front of an agent on this computer.

## What the levels do

| Level | What a request from that computer can make a bot here do |
|---|---|
| Allow | Anything you could, as the bot is set: with Approve automatically and a free agent mode, run commands without asking. |
| Auto (the default) | What the agent judges safe by itself; it asks you about the rest. Never more than the bot does for you. |
| Ask | Nothing that changes a file or runs a command without your approval on a card. Never more than the bot does for you. |
| Deny | Nothing: the message is refused. |

Under Auto and Ask the host also enforces, whatever the request says
([ceilings](../features/codync-network.md#ceilings-auto-and-ask)):

- Codync approves nothing by itself, even for a bot set to Approve automatically.
- The agent is put in a mode no freer than the level, and back in the bot's own mode afterwards.
- The bot can only read and answer you with Codync's own tools: no message to another bot, no follow-up, routine,
  background work, dev server, team change, and no change to its instructions, skills, fallbacks or memory. So the
  text can't travel on to a bot, or a later turn, that isn't held to the level.
- An agent that can't be held to a level (it lists no modes) doesn't take the request.

A computer no rule matches gets Auto. The computers already in a network when this default arrived keep Allow, each
with a rule of its own you can change.

## What the levels don't protect

These hold at every level but Deny.

- **Reading, and sending back what it read.** Reading files doesn't ask, in Auto or in an agent's asking mode
  (Claude's Manual reads without a card). A request can have the bot read anything its agent can read, in its folder
  and outside it (a `.env`, an SSH key, its memory, your conversation with it), and put it in the reply that goes back
  to the sender. This is the largest risk: Ask stops changes, not leaks.
- **Your conversation with the bot.** The request runs in a background task that starts as a copy of the bot's
  conversation where its agent can, so it sees what you said there. An answer to the bot's own request, and a
  request to a bot with computer use or Manage the team on, runs in the bot's chat itself: its text stays in that
  session afterwards, and a later turn of yours can act on what it planted, without the level.
- **What the agent itself allows.** A command or tool you allowed in the agent's own settings (Claude Code's
  `permissions.allow`, a Codex sandbox setting, an MCP server set to trusted) never reaches Codync, so no card shows.
- **Auto is the agent's judgment.** In Auto the agent decides what is safe. A well-written request can look safe:
  a connector call that sends email, an edit that looks routine.
- **Your connectors and computer use** are the agent's tools: under Ask they show cards (unless the agent allows them
  by itself), under Auto the agent decides.
- **Approval fatigue.** Ask is only as good as your reading of each card. A computer that sends many requests trains
  you to press Allow.
- **Work that outlives the turn.** A background subagent the agent started keeps the level only until the bot's next
  queued item; after that it is answered as the bot is set.
- **Rules name a computer.** A rule matches a computer by its name in the network. Check the network's computers when
  one is renamed.
- **Older Codync.** A computer decides what it takes by its own rules. One on a version from before these levels lets
  in every member at Allow: update every computer in the network.

## The other direction

A computer someone else can run code on also sees what you send to it: your requests, your bots' answers, and anything
you put there (code, agent sign-ins, keys). A cloud computer's provider and anyone with its key can read all of it.

## What to do

- Give a cloud computer, or any computer other people can run code on, **Ask** or **Deny**. Prefer Deny when you
  don't need its bots to ask yours anything.
- Keep secrets out of the folders of bots other computers can reach, or turn **Offered to the network** off in those
  bots' settings.
- Keep Allow for computers only you use.
- Read an approval card's command before you allow it, and stop the bot if a request asks for something you didn't
  expect.
- Keep every computer in the network on the current Codync.

What's planned next (a session of its own per sender, cloud computers capped at Ask, a level per cloud assistant):
[requests from other computers](../plans/network-request-trust.md).
