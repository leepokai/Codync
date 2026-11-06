# Voice call transcripts

Status: superseded and shipped as stage 1 of [voice calls owned by the host](voice-architecture.md) (2.25.0):
transcripts, the titled call line, hidden call messages and dreaming over calls, with the host running the call.
Current behavior: [voice call](../features/voice-call.md#the-call-on-the-host). The original plan stays below for
the record.

## Goal

1. Every call keeps its transcript: what the user said and what the voice answered, in order.
2. The call's line in the chat becomes a short title that sums the call up ("Plans the 2.26 release · 03:12"),
   and opens the transcript.
3. Dreaming reads the transcript like the rest of the chat, so what the user said on a call reaches long-term memory.

On-device and speech-to-text calls already send each utterance as an ordinary message; they get the title (2) and
nothing else changes for them.

## Steps

### 1. Capture (iPhone, Android, desktop)

- Gemini Live: `inputAudioTranscription` and `outputAudioTranscription` in the setup; the server sends
  `serverContent.inputTranscription` / `outputTranscription` pieces, joined per turn (checked on a phone on
  2026-10-25: accurate, but Chinese comes back in Simplified script for a Traditional speaker).
- OpenAI Realtime: input transcription in `session.update`; `conversation.item.input_audio_transcription.completed`
  and `response.output_audio_transcript.done`.
- Each engine keeps `[{from: "user" | "voice", text, at}]` for the call. Chinese goes through the reader's script
  (`VoiceSettings.inReadersScript` on iPhone, its ports elsewhere). A turn cut short by barge-in keeps what was said.

### 2. Host: the call entry

- `logCall` takes an optional `transcript` (additive; older apps send none). The host keeps it on the call's
  notice entry (`data.transcript`) and validates it (length caps, `from` limited to the two values).
- A one-shot agent of the bot's harness (`chat::oneshot`, as `chat::naming` uses) writes a title of a few words in
  the user's language from the transcript; the host puts it in the entry's `text` (it stays "Voice chat" until then,
  and when there's nothing to sum up) and emits the update. Older apps already show `text · mm:ss`, so they get the
  title for free; no `minApp` change.

### 3. Memory

- `dream::message` expands a call entry: the user's lines count as the user's own words (spoken, not pasted), the
  voice's lines are context only. A call counts its user lines toward the eight-message trigger.
- `send_to_bot` messages are written by the voice model, not typed by the user (it may translate or rephrase), so
  the apps mark them (`from: "voice"`) and dreaming quotes the transcript instead.
- Update the "Voice transcripts aren't marked yet" limit in [context and memory](../features/context-and-memory.md).

### 4. The chat line (iPhone, Android, desktop, terminal)

- The line shows `title · 00:26`; tapping it opens the transcript (a sheet on iPhone and Android, the desktop
  `Sheet`, a key in the terminal). Without a transcript it isn't tappable.
- States: "Voice chat" while the title is being written; the transcript sheet lists "You" and the voice's lines.

### 5. Ship

- Minor version (protocol addition). WhatsNew for iPhone and Android, the voice-call and memory docs, the user
  manual's voice page. Tests: transcript validation and title on the host, dreaming over a call entry, each
  client's transcript parsing; e2e a Gemini call on a phone and on the desktop.

## Open questions

- The transcript under the call line (planned) or each utterance as its own chat bubble?
- The title from the bot's harness (planned: same as naming, works for on-device calls) or from the voice provider's
  key?
- Thumbs up / down on the call line, for rating the voice?
