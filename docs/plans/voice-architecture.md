# Voice calls owned by the host

Status: all four stages shipped (stage 1 in 2.25.0, stages 2–4 in 2.26.0; stage 4 without CallKit). Supersedes [voice call transcripts](voice-call-transcripts.md),
which became stage 1 of this plan. Current behavior: [voice call](../features/voice-call.md#the-call-on-the-host).

A realtime voice model (Gemini Live, OpenAI Realtime) stays in front of the bot: coding agents are text-only and
slow, so the voice talks and the bot works. What changes is how it hands over. Today each client (iPhone, Android,
desktop) has its own `CallOperator` with the instructions, tools and reply hand-off; the voice knows nothing of the
user, the bot's memory or the chat; it may rephrase or translate what the user said; and nothing of the call is kept.

```
phone / desktop (audio only)        host (all logic)                     the bot
 mic ⇄ Gemini / OpenAI         ←→    Call: context, tools, transcript ←→   does the work
 (audio never crosses the host)      progress and replies pushed back
```

## Stages

Each stage ships on its own (after-change, merged into `dev`), all clients together.

### 1. The call on the host (shipped)

- **`startCall`** `{botId, callId, provider?, model?, voice?, mode}` creates the call and returns everything a
  client needs: the provider credential (realtime and speech modes), the voice's instructions and the tool
  declarations. On-device calls get a call too (no credential). The rules open with the voice's role: it is the
  relay between the user and the bot, not the bot; it passes requests to the bot, tells the user what the bot says
  and asks for what's missing, speaks of the bot in the third person ("I'll ask Echo"), never claims work as its
  own, and sends anything about code, files or the computer to the bot even when it thinks it knows. The host writes the instructions, rules first, then
  the background as named sections, each capped (about 16k characters in all, less where the provider's context is
  smaller), empty ones left out, refreshed when the provider session is resumed or replaced:
  1. Now: the computer's date, time, weekday and time zone; the user's language.
  2. About the user: the shared "yours" `MEMORY.md` and its notes' names and descriptions.
  3. This bot: name, description, harness and model, folder and project, permission mode, connectors and skills,
     computer use, routines; for a group, every member.
  4. What the bot remembers: its `MEMORY.md` and notes index.
  5. The project now: git branch, changed files, the last commits (skipped outside a repository).
  6. Current work: status, activity, a pending approval and its options, queued messages, the last failure.
  7. The conversation: the last ~20 chat messages in spoken form, and the titles and gist of recent calls.
  8. The other bots on this computer.
  9. Artifacts and files recently shared in the chat (titles).
- **`callTool`** `{callId, name, arguments}` runs the voice's tool calls on the host: `send_to_bot`, `bot_status`,
  `recent_messages`, `answer_approval`, `stop_bot`, `search_memory`. The clients' `CallOperator`s go.
- **`callLines`** `{callId, lines: [{from: user | voice, text, at}]}` appends the transcript as it is spoken (both
  providers' transcription events; Chinese in the reader's script). Kept on the host, so a crash or a dropped
  connection loses nothing.
- **Original words:** `send_to_bot` carries what the user said since the last hand-off, word for word, beside the
  voice's wording, so the bot sees both and memory quotes only the user.
- **Hidden in the chat:** everything a call sends to the bot, and the bot's replies in those turns, carry the call's
  id and stay out of the chat on every client; the bot gets them as usual.
- **`endCall`** `{callId, seconds}` writes the call's line: a waveform icon, a title a one-shot of the bot's harness
  writes from the transcript (`chat::oneshot`), and the duration. Tapping it opens the transcript (the user's lines,
  the voice's lines and the bot's messages, in order). `logCall` stays for older apps.
- **Memory:** dreaming reads the transcript; the user's spoken lines are the user's own words, the voice's lines and
  the voice-written `send_to_bot` text are context only. A call's user lines count toward the dream trigger.

### 2. The bot's work reaches the call (shipped)

- The host pushes what the voice should say to the client holding the call (a `call` event, which older clients
  ignore): the bot's final replies, approvals waiting, failures, and short progress while it works ("running the
  tests", "changed three files"), rate-limited. The client hands each to the provider as text. "What are you doing?"
  is answered from real progress, and a long silence gets a short update.
- Rules in the instructions: anything about code or the computer goes to the bot; the voice talks, reads results and
  asks for what's missing, and never invents results.

### 3. One engine state machine

Shipped in 2.26.0 ([phases and recovery](../features/voice-call.md#phases-and-recovery)). Not covered: the desktop's
on-device engine on a device change (the speech helper owns its microphone), and OpenAI's WebRTC transport isn't
replayed end to end in the iPhone and desktop tests (its events are).

- Every client's engines share the phases: connecting → ready (the provider confirmed the session and the microphone
  is delivering audio) → listening / speaking, with the connecting line until ready.
- Drops resume (Gemini session resumption, a fresh OpenAI session with the transcript so far); route changes and
  interruptions (a phone call, another app's audio) recover without hanging up.
- Recorded provider event streams replay through each client's engine in tests, including the device cases found so
  far (#214: the engine reconfigured at start, the player stuck).
- Call quality in analytics, never content: time to ready, reconnects, a phase stuck too long.

### 4. Platform

Shipped in 2.26.0 ([the call outside the app](../features/voice-call.md#the-call-outside-the-app)), without CallKit:
it was built, then taken out before release by the owner's decision (2026-10-25); the design and how to bring it back
are in [CallKit (archived)](callkit.md).

- iPhone: the bot's progress and the call's state in the bot's Live Activity; the app keeps its own audio session.
- Android: the call's foreground notification shows the same progress.

## Compatibility

New methods and fields are additive. The apps need a host with `startCall`, so the iPhone, Android and desktop apps
raise `minHost` to the release that ships stage 1 (a Linux desktop or an SSH computer may run an older host).
`logCall` stays until `minApp` passes it.
