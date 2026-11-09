# Agent fallbacks

When a bot's agent can't answer (its usage ran out, it's rate limited or overloaded, signed out, or won't start), the
next agent in the bot's list carries the same turn on, in the same chat, with the same memory, workspace and tools.
The bot goes back to its own agent once that one recovers ([#184](https://github.com/leepokai/Codync/issues/184)).

The chain is of **agents** (Claude Code → Codex → Gemini CLI), not of models. Falling back between one agent's models
stays that agent's own setting (Claude Code's `fallbackModel`). Only Codync can move a conversation from one agent to
another.

Code: `host/src/agent/fallback/` (the chain, `reason`, `rest`, `carry`), `host/src/agent/bot/fallback.rs` (the
actor's side; each agent's parked session in `fallback/park.rs`), `host/src/chat/fallbacks.rs` (the bot's own tools).

## What other projects taught

Read from their source on 2026-10-21. They all chain models inside one agent:

| | Chain | Moves on for | Same model first | Next turn |
|---|---|---|---|---|
| OpenCode (v1.19, v2 `dev`) | None built in; plugins add per-agent `fallback_models` | Plugins: 429/5xx, quota, missing key, model not found | 2 s doubling to 10 s, 10 times (~84 s); never a quota error (v2) | Plugin: the primary once its cooldown passed |
| OpenClaw | `model.fallbacks` per agent | Auth, rate limit, overload, timeout, billing, model not found | Up to 10 rate-limit attempts, then other credentials | The primary; cooling credentials skipped |
| Claude Code | `fallbackModel`, up to 3 | Overload and unavailable models only | Up to 10 retries | The primary again |
| pi | None built in ("can be done as an extension", #3213); virtual models and `pi-fallback-provider` | Any error, once pi's own retries and compaction are done | 3 retries, 2 s doubling to 60 s; never quota or billing 429s | Stays on the new model |

What their issues taught, and the answer here:

- **A usage limit isn't a transient 429.** OpenCode once retried "quota exceeded" a week later (#28431). → Usage-limit
  text is checked before rate limits.
- **Shared cooldowns.** Sessions each retried one dead provider: 2,610 errors in seven weeks (#55075). → Rests are the
  computer's, not the bot's.
- **Bound everything.** v1 retried forever (#41848); a malformed entry looped (#34043). → Each agent is tried at most
  once per turn, at most three fallbacks, entries checked when saved.
- **Say who answered.** A plugin moved users to expensive models silently (#46894). → A chat line on every switch and
  on going back, the agent in use in Details, a cost note.
- **A full context isn't a reason to switch.** All of them compact instead.
- **Let the agent finish its own recovery.** → Codync acts only on the error the agent returns. The agents retry first
  (Claude Code up to 10 times; Codex 4 requests and 5 streams, not 429s), so Codync never asks the same agent again.
- **Carry the work, not just the prompt.** oh-my-openagent resent only the last message's text. → The next agent hears
  what the turn already did.

## The chain

- Per bot: its agent and model (step 0), then `fallbacks`, up to 3, each `{backend, model, modelOptions}` (a custom
  `command` is accepted on the wire, not offered in the apps). Empty by default.
- Each agent appears once, the bot's own included. Making a fallback the bot's own agent drops it from the list
  (`fallback::tidy`).
- Everything else is the bot's for every step: folder, approvals, connectors, skills, computer use, memory, fast mode
  where the step's agent has it (`fallback::effective`). Its mode too ([agent modes](marketplace.md#agent-modes)):
  `auto` is each step's own Auto mode, and a step whose agent lacks any other mode fails the turn rather than work in
  its default.

## When the bot moves on (`fallback::reason`)

Read from the ACP answer: the JSON-RPC code, the adapter's category (Claude's `data.errorKind`; codex-acp's
`data.codexErrorInfo`, kept as `usage_limit_exceeded`), the host's own `Failure`:

| Reason | How Codync knows | Rest |
|---|---|---|
| `usageLimit` | Claude: `rate_limit`/`billing_error` with the Agent SDK's usage-limit text ("You've hit your…"). Codex: `usageLimitExceeded` | Until the reset the agent reported (`usage` windows at 100%); unknown: 1 h |
| `outOfCredits` | Claude `billing_error`, `account_on_hold` | 6 h |
| `rateLimited`, `overloaded` | Claude `rate_limit`, `overloaded`, `server_error`; codex-acp `rateLimitExceeded`, `serverOverloaded` | 1, 5, 25, then 60 min for failures in a row |
| `signedOut` | ACP `-32000`, `authentication_failed`, CodeBuddy's `auth` category | Until it's signed in, looking again after 1 h; an agent Codync can't sign in to (no sign-in card): 10 min. The chat of each bot that moves past it gets the agent's sign-in card (holding no message: the fallback answers it), and signing in there brings the bot back |
| `notStarting` | `FailureKind::NotSetUp`, `Timeout`, `Exited` | 5 min |
| `modelMissing` | `model_not_found`, or the agent refusing the step's model (`FailureKind::NoModel`) | None: it's the bot's setup, not the agent's. The bot skips that step until its settings change |

Stop, refusal, output or step limits, a routine's time limit, compactions and uncategorized internal errors don't move
on. CodeBuddy's CLI (CodeBuddy Code, WorkBuddy) answers a failed prompt as a refusal with the error in
`_meta["codebuddy.ai/errorMessage"]`: that one counts as the error it holds (`acp::codebuddy::prompt_result`), so a
signed-out WorkBuddy moves on. codex-acp still sends its 429s and overloads as reply text ([codex-acp#647](https://github.com/agentclientprotocol/codex-acp/issues/647)),
so only its usage limit moves a Codex bot on today.

**Rests** (`fallback::rest`, `kv` `rest.<account>`: the agent, or a custom command's own) are the computer's: once one
bot finds Claude Code out of usage, every bot skips it until it resets. A rest ends at its time, when any turn on that agent ends normally, or with Switch back.
A signed-out rest also ends when the agent is signed in (`agent_sign_in::agent_signed_in`: the card, Extensions, a turn of it
finishing, a re-check), for each of its accounts; a usage rest stays.
Usage windows only time a usage limit's rest; they never skip an agent on their own (extra usage can go past 100%).

## Carrying the turn over

When a turn fails with a reason, the actor (`Actor::fall_back`) records the rest and, if an agent is left, starts the
same turn again on the first step, in order, that this turn hasn't tried and that isn't resting. The
turn keeps its number, messages, routine run, room turn or bot request; permission cards still open expire.

- **Sessions per agent.** `bots.session_id` is always the session of the agent the bot is on. Moving to another agent
  parks it (`kv` `fallback.session.<bot>.<agent>`) and resumes that agent's own parked session, if any. `newSession`
  clears them all, and no agent is told what was said before it (`kv` `fallback.fresh.<bot>`); changing a step's agent
  or model forgets its session.
- **What the next agent hears**, before the turn's messages (`fallback::carry`), decided once its session is up. Only
  the user's messages and the bot's replies count:
  - a new session (also when its parked session won't load): a recap of the chat (the last summary of the session it
    takes over from, the latest 12 messages, the open plan, a `search_history` bookmark);
  - a session it had: a catch-up of what was said since it left (an agent that never started keeps its mark);
  - and when agents failed this turn: who stopped last and why, then each failed agent's tool calls, the messages it
    sent and its last note, with "check what's already done before you redo any step". It follows a handoff the user
    approved, which waits for the next turn if this one can't start at all.
- **Every agent failed** (the turn ran out of agents): the failure notice as before, saying how each agent is ("No agent in this bot's list could
  answer: Claude Code reached its usage limit (resets in 2 h); Codex …"). The turn's messages wait on the sign-in card
  of the first agent it left signed out, and go out again once that agent is signed in. A routine run's 1/5/15-minute retries still
  follow. A bot without fallbacks whose agent ran out of usage, was rate limited or overloaded gets `addFallback` on the
  notice.
- **Going back.** Each turn starts on the first step that isn't resting, so the bot goes back on its first turn after
  the reset, never mid-turn; after signing in to a signed-out agent, on its next message. Switch back (`switchBack`)
  clears its own agent's rest and moves an idle bot now.

## The bot sets its own

The `chat` server's `list_agents` (agents on this computer, signed in, resting; `agent` adds its model ids) and
`set_fallbacks` (replaces the list) let a bot set up its chain, when the user asks or wants it to keep working through
limits. On by default; the user turns it off per bot ("Let the bot change its fallbacks", `selfFallbacks`). A notice
in its chat records each change. A bot that manages the team sets any bot's with `update_bot`'s `fallbacks`, its own
only while `selfFallbacks` is on. Both name agents and models only: an agent already in the list keeps its custom
command, and its model options while the model stays.

## Apps

iPhone, Android, desktop and terminal show the same things:

- **Details → Agent card**: while on a fallback, a first row "Using Codex until Claude Code resets at 3:00 PM" with
  **Switch back**; under Model and Fast mode, **Fallbacks** with an info button and **+** (an Add fallback sheet with
  the Agent and Model pickers, the bot's own agent and those already listed left out); a row per fallback with its
  number, logo, "Agent · Model", "Usage resets 3:00 PM" / "Resting until 3:00 PM" while resting, an **In use** chip,
  and a ⋯ menu (Move up, Move down, Remove); "No fallbacks yet."; a caption; and the "Let the bot change its
  fallbacks" switch. The bot's own Agent row says when it resets.
- **Chat**: one line where another agent took over ("Claude Code reached its usage limit (resets in 2 h). Codex carried
  on.") or the bot's own came back ("Back on Claude Code."); it unfolds to what the agent was told. Moving past a
  signed-out agent also posts its sign-in card (`connectionRequest` kind `agent`), whose Sign in opens the agent's
  sign-in sheet. A failure with
  `addFallback` offers **Add a fallback**, which opens the Agent card.
- **Terminal**: the bot form's Fallbacks field (←→ pick, space adds the next free agent or cycles a fallback's model,
  `a` changes its agent, `[`/`]` move, `x` removes) and "Bot edits" (whether the bot may change them); Details shows the agent in use and the chain,
  `B` switches back; in the chat `↪` lines unfold with `o`, and `A` on a picked failure opens that bot's Fallbacks field
  (in a group, the member whose agent failed).

## Wire

All additive (no `minApp` change):

- Bot: `fallbacks`, `selfFallbacks` (settings; `createBot`/`updateBot`); `agentStep: {step, agent, model, since,
  reason} | null` and `resting: [{agent, until, reason}]` (agent bots only).
- Method: `switchBack {botId}`.
- Notices: `agentSwitch: {from, to, back, reason, resetsAt, carried}`; failure notices' `addFallback: true`.
- Analytics: `agent_fell_back {agent, reason}`; `turn_finished` once per agent the turn ran on.
