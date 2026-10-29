# Voice call

A hands-free call with one bot or group from the iPhone, Android or desktop chat (the waveform button in an empty composer).
On Linux the desktop app offers only the cloud engines (on-device speech is macOS only). The terminal
UI has no calls (no audio stack); it shows a call's line like every client and `o` unfolds its conversation.
The bot is the same agent on the computer; the call only changes how you talk to it. The host runs every call
([the call on the host](#the-call-on-the-host)); the clients only carry audio.
On iOS, [Open Live Mode in Shortcuts](ios-shortcuts.md) can select a bot and start this call directly (no bot: the lead).
Every other bot waits for you to speak; the [lead bot](lead-bot.md) speaks first: once a call's audio is live the
apps send `callStarted {botId, callId}` (once per call, only to a lead), and the host has a lead with no turn under
way open with a sentence or two, read aloud like any reply and part of the call (its reply carries the call's id).

## On-device voice

- `apps/ios/Kit/Sources/CodyncUI/Call/` (iPhone) and `apps/desktop/src/renderer/views/call/` (desktop): `CallView` (the call bar) over a `VoiceEngine`; `OnDeviceEngine` is the default audio loop. On the Mac the desktop app runs speech recognition in `codync-speech` (`apps/desktop/native/speech-macos/`, driven by `apps/desktop/src/main/speech.ts`).
- Android: `CallBar` (`apps/android/app/.../VoiceControls.kt`) over the same `VoiceEngine` set, ported into `apps/android/core` (`VoiceEngine.kt`, `CloudSpeechEngine.kt`, `GeminiLiveEngine.kt`, `OpenAIRealtimeEngine.kt`, `Calls.kt`) with the phone's audio in the app (`VoiceAudioPorts.kt`, `WebRtcPeer.kt`). On device is `VoiceSession` (`LocalVoiceAudio.kt`): the call's language (Voice chat settings → Language, the phone's by default) is heard by Android's on-device recognizer on Android 13+, fed the call's own microphone (`PlatformSpeech.kt`; Android asks before downloading a language), with the bundled Vosk model for English where Android has none; each reply is read by an installed offline voice for the language it's written in, and a reply with no installed voice stays in chat with a notice; `VoiceService` (a microphone foreground service) holds audio focus and the Bluetooth/wired route for the call. Settings keys match the iPhone's (`callEngine`, `callMode.<provider>`, …).
- UI follows Grok Bot: a floating capsule over the top of the chat (`ThreadView` overlay), and the
  chat stays readable and usable underneath. Left to right: the bot's avatar (pulses while
  speaking; tap it to interrupt), a dotted level line (your voice while listening, a ripple while
  the bot speaks, a slow breath while it works; while the engine connects, an indeterminate progress
  line instead: a short segment gliding back and forth along a faint track, easing at both ends
  (white in dark mode, black in light), still at the left under Reduce Motion, fading back to the dots
  once the call is really connected), gear (call settings), mic (mute), red ✕ (end). Connected means
  Gemini has answered the setup and the microphone delivers audio; OpenAI has confirmed the
  instructions and tools (`session.updated`, under the same 15-second start timeout); speech to text
  hears the microphone. Opening the socket or data channel isn't enough.
- Desktop: the capsule sits 16 CSS pixels below the window top and explicitly excludes itself
  from Electron's window-drag region so the gear, mute and end buttons receive mouse clicks.
  The iPhone places its capsule below native navigation; the terminal has no call controls.
- Speech → text with `SFSpeechRecognizer` (on-device when the language supports it) in the language
  picked under Voice chat → Language (`callLanguage`; empty follows the system's first language,
  which is often English on bilingual setups). A pause ends an
  utterance and sends it as the call's message (`BotStore.send` with the call's id), so it takes the normal
  path: queued while the bot works, folded into its next turn. It and the bot's replies show in the call's
  conversation, not in the chat (below).
- Each new final reply (`data.final`) is read aloud with `AVSpeechSynthesizer` in the reply's own
  language; markdown and code blocks are dropped (`SpokenText`). The mic pauses while it speaks.
  An approval request is announced, answered in the chat.
- Gear: voice engine (below), and for On device the pause length before sending (1 / 1.5 / 2.5 s) and reading speed; kept on the phone.
- Ending the call calls `endCall` (older apps `logCall`); the call's line is described below.
- The host receives recognized text; the Cloudflare transport sees encrypted channel frames.
  Codync does not forward microphone audio to the host. Speech recognition may use Apple services
  when on-device recognition is unavailable. `UIBackgroundModes: audio` permits
  background microphone capture and speech playback during an active call.
- While a call is listening or speaking, it retains the computer's transport across
  foreground/background transitions. Ending or failing the last call releases that
  connection when backgrounded, restoring normal push delivery. Removing the computer
  or switching accounts ends its calls and closes the transport.
- What to say arrives from the computer as the call's `call` events (see "How the voice talks"), delivered from
  the store's incoming events directly to the audio session, rather than waiting for a SwiftUI view update.

## Dictation

The composer's mic (beside send / call, as in Grok Bot) types what you say into the box with the
same on-device recognizer and the call's language setting; nothing is sent until you send it.
Words appear as they're recognized, after what's already typed. While it listens the mic becomes
a recording pill (stop, the time, dots that follow your voice), on the iPhone and the desktop, and
a call takes the microphone from it. On the desktop, dictation ends only on the pill's stop (or the
shortcut's tap), which waits for the last words, on an error (shown in the window), or when another
window takes the speech helper; it doesn't stop on a pause. On the iPhone the pill's stop keeps what
was recognized so far. The desktop island's message field has the same mic and pill.

A `chevron.up` beside the composer's mic picks dictation's engine, kept on the device apart from the
call engine (`dictationEngine`): **On device** (the above), or **OpenAI** / **Gemini** on the user's
own key. With a provider, the mic first checks that the computer holds its key (`voiceStatus`; no key
means an error before anything is recorded), then records; the pill (and its time) appears only once
the microphone is open, so nothing said before it is lost; nothing is typed while recording. The
pill's stop turns the pill into "Transcribing…" and the whole recording goes through
`voiceTranscribe` with the call settings' speech-to-text model; the text lands after what's in the
box when it arrives (typing goes on while it records). The recording goes in 20 s pieces (one
channel message each on the iPhone; under the host's 2 MB request body on the desktop), three at a
time, joined in order; a tail under half a second stays with the piece before, and the first piece
that fails fails the dictation. Under half a second of audio in all is dropped. Sending a message,
starting a call or switching chats drops a dictation still recording or transcribing. The
recording's length counts toward the provider's minutes this month. The island's mic stays
on-device (its window doesn't reach the host).

- Desktop (`views/thread/dictation.ts`, the model, tested with a stub platform; the helper,
  microphone and host wired in `dictation-platform.ts`; pieces in `dictation-pieces.ts`; menu in
  `DictationEngineMenu.tsx`): on-device speech is the speech helper, macOS only. Linux and Windows
  show the mic too, for a provider; with **On device** picked each press says on-device speech isn't
  available there. An unpackaged `npm run dev` can't use the helper: macOS stops it because the dev
  Electron has no speech usage description, so test it in a packaged build (`dist:mac:dev`), like
  calls.
- Desktop: the speech helper serves one window at a time. When another window starts it (the
  island while the chat window dictates or calls, or the other way round), `main/speech.ts` tells
  the window that lost it (`stopped`) and ignores that window's stop.
- iPhone (`Thread/Dictation.swift`, pill in `Thread/RecordingPill.swift`, menu in
  `Thread/DictationEngineMenu.swift`, dots shared with the call bar in `LevelDots.swift`):
  `SFSpeechRecognizer` on device; with no microphone route (some simulators) the mic does nothing.
  A provider's errors (no key, no connection) show in a dialog.
- Android has no composer mic yet.
- The terminal UI has no microphone, so no mic.

### Voice input shortcut

Desktop on macOS, set in **Settings → General → System** (*Voice input shortcut*: off, the right ⌘
twice (the default) or the left ⌘ twice; `voiceShortcut` in `host-prefs.json`). From any app, a
double tap of that key starts dictation into the [notch island](notch-island.md)'s message field
while the island is on, else into the chat window's composer (the top bot when no chat is open).
One more tap stops it and sends what was said once the last words are in; the pill's stop leaves
the words in the field unsent, and an error leaves nothing for the tap to send.
Double-tapping to stop counts as that one tap, not a new start. While a call is on (any engine),
the shortcut does nothing: the call has the microphone.

- `apps/desktop/src/main/voice-shortcut.ts` keeps an `osascript -l JavaScript` process with an
  `NSEvent` global monitor for modifier changes, plus the seconds since the last key press or
  mouse click. Neither needs the Accessibility or Input Monitoring grant, and the app never sees
  what is typed. The chat window reports calls (`voice:calling`) so taps are dropped meanwhile.
- `shared/voice-shortcut.ts` decides what counts: a press under 0.3 s with no other modifier held
  and no key pressed or click meanwhile is a tap; a second tap starting within 0.4 s of the first,
  with no key pressed or click since the first, is a double tap. ⌘C, ⌘V and ⌘-clicks with the
  same key are neither.
- The renderer side is `Dictation.shortcutKey` (`views/thread/dictation.ts`), shared by the
  composer and the island's model; `VoiceKeyRouter` (`shared/voice-shortcut.ts`) decides whether
  a tap starts, sends or does nothing. The chat window queues taps for its composer in
  `views/thread/voice-shortcut-keys.ts`.
- macOS only: Windows and Linux have no ⌘ or on-device speech, and a modifier-only shortcut
  there would need a native keyboard hook. The iPhone, Android and the terminal UI have no global
  keyboard shortcut.

## Background verification

Store regression tests cover sending a recognized utterance after backgrounding,
delivering a final reply without view updates, returning to the foreground without
restarting an active call's connection, ending the last call, and retiring the
store. They do not simulate iOS audio or background scheduling.

On a physical iPhone, start a voice call with a connected bot, return to the Home
Screen, then speak a request. Verify that it sends successfully and the reply is
read aloud while Codync remains backgrounded. Also start a long reply in Codync,
return Home during playback, and verify that speech continues. End the call and
confirm that ordinary background notification behavior resumes. For App Review,
the recording must capture both the physical device and the audible reply.

## App Review recording

The 2.2.1 (22) review requested a screen recording on a **physical device** for
Guideline 2.5.4. Record these steps using a connected computer and a bot:

1. Open its conversation and tap the waveform button with an empty composer.
2. Allow microphone and Speech Recognition access. Speak a short request and
   show the recognized message and the bot's spoken reply.
3. While a long reply is being read, return to the Home Screen. Keep recording
   long enough to demonstrate audible playback continuing in the background.
4. Return to Codync and end the call.

Ensure the resulting file contains the audible reply. If the screen recorder
cannot capture the call audio, record the physical phone externally with another
device so both its screen and speaker are audible. Include the recording and
these navigation steps in App Review Information for future submissions.

Simulator recordings are useful for UI checks but are not the physical-device
evidence Apple requested. A `simctl io ... recordVideo` probe on 2026-09-30
produced an H.264 video track with no audio track. Transport regression tests
verify background reply delivery and retirement, not iOS audio behavior.

## Realtime voice with your own key

Better recognition (mixed Chinese/English, code terms) and a real conversation (barge-in, "what are
you doing?" answered from the transcript) need a realtime speech model. Codync is free, so users
bring their own provider key; Codync never pays for or resells minutes.

### Engines

The gear's first section is **Voice engine**. Each client remembers what the running engine was built from
(engine, mode, models, voice; On device: its language; `engineChoice`). When the voice settings close and that
differs, the call ends that engine and starts the new one without hanging up: same call, one "Voice chat" line
at the end, cloud minutes counted up to the switch, mute kept, a fallback notice cleared. A call that fell back
to On device tries the picked cloud engine again the same way. Desktop and Android check whenever a voice
settings view closes (desktop: the gear, Settings; Android: the gear, the computer's card in Computer
management); on the iPhone only the gear is open during a call.
Where on-device speech is missing (desktop on Linux or Windows), the provider standing in for On device keeps
running while On device stays picked.

| Engine | Needs | Transport | Code |
| --- | --- | --- | --- |
| On device (default) | nothing | Apple speech, as above | `OnDeviceEngine` |
| OpenAI | OpenAI API key on the computer | WebRTC, device ↔ OpenAI (default `gpt-realtime-2.1`) | `OpenAIRealtimeEngine` |
| Gemini | Google AI Studio key on the computer | WebSocket, device ↔ Google (default `gemini-3.8-live`) | `GeminiLiveEngine` |

Each cloud engine has two **modes**:

- **Realtime**: a live conversation with the provider's realtime model (below), talk over replies.
- **Speech** (`CloudSpeechEngine`): the on-device loop with the provider's models instead of Apple's.
  The microphone is cut into utterances at a pause (energy VAD; under 0.4 s of voice is dropped as
  noise, and 0.4 s after playback is ignored as echo), each one goes to the provider's
  speech-to-text and is sent as an ordinary message; replies are read with its text-to-speech. Both
  run on the computer (`voiceTranscribe`, `voiceSpeak`), so the key never leaves it; utterances stop
  at 20 s and replies are spoken in clips (OpenAI 600 characters as AAC, Gemini 150 as WAV) so every
  request and answer fits one 1 MiB channel message. Speech-to-text gets the app's and
  the bots' names as hints (gpt-transcribe also as `keywords`). gpt-transcribe returns Chinese in
  Simplified characters whatever the prompt says, so the client turns it into Traditional
  (`Hans-Hant`) for a reader whose first Chinese language, or region, is Traditional. Only a
  model the user picks is saved; the default isn't, so a newer default reaches everyone.

**Default models follow the provider.** Nothing pins a model version: once a day (and whenever a
key is saved or the model list is opened) the host reads each keyed provider's model list and
keeps its newest model per mode (`voice::newest`, stored as `voice-defaults.<provider>`), and
`voiceStatus` / `voiceModels` report them as `defaults`. OpenAI's list is dated, so its newest
generation wins, full models over `mini`; Gemini's names carry the version, plain names over
`-lite` / variants; dated snapshots and previews never become defaults. Clients remember the
last `defaults` they saw (`callDefaults.<provider>`) and refresh them at every call. The ids
built into the host and the app (today `gpt-realtime-2.1`, `gpt-transcribe`, `gpt-4o-mini-tts`;
`gemini-3.8-live`, `gemini-3.8-flash`, `gemini-3.8-flash-tts`) are used only until the first
list arrives.

Voice settings (`VoiceChatSettingsView`) open from the call bar's gear and from settings: on the
iPhone and Android, **Voice chat** on the computer's card in **Manage computers**; on the desktop, **Settings → Voice chat** (this
computer) or the waveform button on a computer's card in **Computers & devices**. Keys are entered
on the computer only: the host takes `setVoiceKey` from loopback, and the iPhone shows whether a key
is there and can test it. Picking a cloud engine shows its key form instead of the on-device settings: a secure field, a link
to the provider's key page, **Test** (the computer checks a new key with the provider and keeps it
only if it works, otherwise the provider's error is shown; with a key saved, it checks Live model access or mints a Realtime/Gemini call
credential for the chosen model and voice), a model picker, a voice picker, and minutes used this month.
Speech mode adds speech-to-text and text-to-speech pickers and the pause length; dictating with
the provider shows its speech-to-text picker too. When dictation's engine is a provider the call
engine isn't, that provider gets its own section below with just the key and speech-to-text picker
("For dictation in the message box."), so its key can be added without switching the call engine; its **Test** lists the key's models instead of minting a call credential. The model list comes from the provider through the computer (`voiceModels`: OpenAI `GET /v1/models`
ids sorted into realtime, transcribe and tts models; Gemini `GET v1beta/models` by
`bidiGenerateContent` / `generateContent` and name; translation and diarization models left out), fetched after a successful test and each time the form opens. The call bar
shows the provider's name next to the dots whenever audio leaves the device.

### Where the key lives

- On the computer, in the host's vault (`host/src/voice.rs`, slot `voice`: encrypted in
  `codync.db` with the vault key from Keychain / Secret Service), so one key serves the iPhone and the
  desktop app. It reaches the host once, over the encrypted channel, and never leaves it again; it is never
  logged and never sent to the relay or our cloud.
- Host methods (control scope): `voiceStatus` (which providers have a key), `setVoiceKey
  {provider, key}` (validated against an available voice model before it's kept; Live-only keys use model access rather than a billable session; empty removes it),
  `voiceSession {provider, model, voice}` → `{credential}` for Realtime/Gemini or `{validated:true}` for GPT-Live, `voiceModels {provider, supportsLive?}` → `{models,
  transcribe, speech}`, `voiceTranscribe {provider, model, audio}` (base64 16 kHz WAV) → `{text}`,
  `voiceSpeak {provider, model, voice, text}` → `{audio, format}`.
- The long-lived key is used for minting a short-lived credential at call start (OpenAI
  `POST /v1/realtime/client_secrets`, 10 minutes; Gemini `POST v1alpha/auth_tokens`, one use) and
  listing models. GPT-Live uses the key on the host to create the SDP session instead; the client receives only the answer and session ID.
- Audio is not proxied through the host: it would cross the encrypted channel and the relay
  (latency, relay bandwidth). The device streams to the provider directly.

### The call on the host

Every call, whatever its engine, is run by the host (`host/src/chat/calls/`, stage 1 of
[host-owned voice](../plans/voice-architecture.md)); the clients' `CallLink` (iPhone, desktop, Android) only
carries audio and talks to these methods:

- **`startCall`** `{botId, callId, mode: realtime | speech | device, provider?, model?, voice?, language?}` keeps the
  call (in `kv`, so a host restart loses nothing) and returns `{credential?, instructions, tools}`: a realtime
  provider's short-lived credential, the voice's instructions and its tools. Called again with the same id when a
  provider session is resumed or replaced (Gemini's `goAway`, a new OpenAI session), so the background is fresh;
  the transcript so far stays. On-device and speech-to-text calls start one too (no credential, no voice model).
- **The instructions** (`calls::context`) put the voice's role first: it is the relay for the bot (or group, with
  its members), the switchboard operator, not the bot; it passes the user's requests on with `send_to_bot`, tells
  the user what the bot says and asks for what's missing; it speaks of the bot in the third person and never claims
  it did or saw something; anything about code, files, the computer or the project goes to the bot even when it
  thinks it knows. It answers in the language the user is speaking right now and switches when they switch (for
  Chinese, their variety and script), whatever the phone's language, the background or the bot's replies are in;
  the phone's language only covers speaking before the user has. Then the background, as named sections that say where they came from, each capped (about 16k
  characters in all on Gemini, plus up to 3k of the call so far for a session that replaces another; six tenths of
  that on OpenAI), empty ones left out: now (the computer's date, time,
  weekday and offset; the phone's language setting, as a fallback), about the user (the shared `MEMORY.md` and its notes' names and
  descriptions, when the bot uses it), this bot (name, description, harness and model, folder and project,
  permission mode, connectors, skills, computer use, routines; a group's members), what the bot remembers (its
  `MEMORY.md` and notes index), the project now (git branch, changed files, the last three commits; outside a
  repository, or without the command line tools on macOS, nothing), current work (status, activity, a pending
  approval and its options, queued messages, the last failure), the conversation (the last 20 chat messages in
  spoken form and the titles and gist of the last three calls), the other bots on this computer, files and
  artifacts shared in the chat, and, when `startCall` comes again for the same call (a reconnect, or an engine
  switched mid-call), this call so far (its newest lines). **All of it goes to the voice provider, on the user's own
  key.**
- **`callTool`** `{callId, name, arguments, lines}` → `{result}` runs the voice's tool calls on the host
  (`calls::tools`): `send_to_bot(text)`, `bot_status()` (a group's: its busy member's), `recent_messages(count)`,
  `answer_approval(option)` (only after the user said which; matched by option name or id), `stop_bot()`,
  `search_memory(query)`. A failure is the tool's answer. The engines hold replies while a tool runs, so the model
  gets each result first.
- **The user's own words:** `send_to_bot` attaches what the user said since the bot last heard from the call, word
  for word from the transcript ("[The user's own words on the call: «…»]"), so the bot sees both, and the message
  is marked `from: voice`: the voice may rephrase or translate, so memory never quotes it.
- **`callLines`** `{callId, lines: [{from: user | voice, text, at, id}]}` appends the transcript as it is spoken, turn
  by turn: Gemini's `inputAudioTranscription` / `outputAudioTranscription`, OpenAI's input transcription and
  `response.output_audio_transcript.done`. The iPhone and Android turn Chinese into the reader's script first
  (Gemini answers in Simplified for a Traditional speaker); the desktop can't (no ICU transform in Chromium).
- **In order** (`CallLink` in each client): `callLines`, `callTool` and `endCall` leave one at a time, and
  `callTool` and `endCall` carry the lines not sent yet (`lines`, a line still being said included), so a hand-off
  always has the words it hands on. Lines a request didn't deliver go with the next one; each line carries the app's
  id, and the host keeps a line once, so a request whose answer was lost can go again. OpenAI writes the user's
  words down after their turn: a tool call waits up to 2 s for them (`input_audio_buffer.committed` until the
  transcription arrives or fails).
- **Hidden in the chat:** everything a call sends the bot carries the call's id (`data.call`), and so do the bot's
  replies in those turns while the call lasts, a group's members' included (a turn that also answers a typed
  message stays visible, and a reply after the call ends shows). Every client leaves them out of the chat whichever
  device the call is on (`hidingCalls`): they show behind the call's line; a message that failed to send stays,
  with Resend, and so does a file or an artifact the bot sent, to be opened. A call whose app went away (killed,
  offline) still gets its line: the host keeps the open calls (`calls.open` in `kv`) and ends one nothing happened
  on for 2 hours: no lines from its app, no message or reply of it, its bot not at work (`chat::calls::sweep`). The
  roster's preview and unread count skip them too. The bot gets them as usual.
- **`endCall`** `{botId, callId, seconds, lines}` adds the last lines and writes the call's line: a notice with
  `call`, `callSeconds` and the `transcript`. A call ends once (`call.ended.<id>` in `kv`), so the apps retry it
  across a reconnect; an ended call can't start again. The call's actors (a group's members) hear `CallEnded`. A one-shot of the bot's harness (`chat::oneshot`) titles it from the conversation, in the user's
  language, into the notice's `text` ("Voice chat" until then, or when there's nothing to sum up); older apps
  show the title too. The line shows a waveform, the title and the length; tapping it opens the conversation
  (iPhone and Android sheets, the desktop `Sheet`, `o` in the terminal): the user's lines, the voice's lines, what
  it relayed and the bot's messages, in order. The chat an app holds may not reach back to an older call (the
  desktop mirror keeps the newest 200 entries), so opening the conversation fetches the call's own messages,
  `callEntries {botId, callId}` → the newest 500, and merges them with the loaded chat, each entry once; it shows
  a spinner until they arrive and the error if they can't. `logCall` stays for older apps.
- **Memory:** dreaming reads the call's line: the user's spoken lines are their own words, the voice's lines and
  its relayed messages context only, and a call's spoken lines count toward the dream trigger; the call's end
  asks for a run soon.

### How the voice talks

- The host decides what the voice hears while the call lasts (`chat::calls::announce`, one watcher per call): each
  new final reply in the bot's chat (one already final when the call began isn't; one that began before it and
  finishes during it is), an approval card waiting, a failure notice, and the bot's activity line while it works:
  at most one progress line every 15 s, only when it changed, and "<bot> is still working" after 45 s with nothing
  said. It sends each as a `call` event `{call, owner, kind: reply | notice | progress, text, seq}`, worded for the
  call's mode as `startCall` last gave it: a realtime voice gets replies shortened for speech, approvals with their
  options ("Mika needs the user's approval: … (options: …)") and progress; an on-device or speech-to-text call gets
  replies as written (its engine shortens them), approvals for the listener ("Mika needs your approval in the chat:
  …") and no progress. In a group, replies and notices name the member. `api::events` delivers each cue only to the
  device that holds the call (`owner`, its device key, set from the caller on `startCall`); older apps ignore the
  event type.
- Each cue's `seq` rises through the call (from the clock, so across host restarts too). The host keeps the call's
  last 10 replies and notices and adds them to that device's catch-up when its event stream reconnects; the app
  skips any `seq` it already heard (kept per call, across engine switches). A watcher that missed hub events
  rereads the chat's latest entries.
- The watcher starts with `startCall` and runs until `endCall`. As the host starts it watches again every call
  started in the last day (`rewatch`). A call none of its app's requests (`startCall`, `callLines`, `callTool`, a
  message said on it) touched for 2 hours stops being watched; the next one starts it again.
- The app hands each cue to its engine (`hear`): realtime voices get `[<bot> replied] …`, `[Notice] …` and
  `[Progress] …` (only the newest progress waits); on-device and speech-to-text calls read replies and notices
  aloud. They wait while the model is talking, so they don't cut it off.
- Barge-in is the provider's voice activity detection (OpenAI semantic VAD; Gemini's automatic
  VAD, with the voice-processing input cancelling the device's own playback). Tapping the avatar
  cancels the response locally.
- Gemini connections are replaced before the server's limit: `goAway` closes the socket and the
  session resumes on a fresh token with its resumption handle; context window compression lifts the
  15-minute audio session cap.
- On iPhone the Gemini engine restarts its audio engine on `AVAudioEngineConfigurationChange` (voice
  processing settling right after the start, a route change such as AirPods) and wires the player and
  speaker again each time: the engine stops itself then, and without both the microphone stays quiet, or the
  player never finishes its buffers so the phase stays "speaking" and the bot's replies are never said.
  A reply counts as played once its buffers are rendered (`.dataRendered`).

### Phases and recovery

- Every engine on every client reports the same phases: connecting (`starting`) until the provider confirmed the
  session and the microphone delivers audio, then listening or speaking; reconnecting after a dropped connection or a
  changed microphone; interrupted while another app has the audio (iPhone: a phone call, Siri, an alarm; Android:
  audio focus, a removed headset); failed. The connecting line shows while connecting and reconnecting; a reconnect or
  an interruption keeps the call and its cues (`holdsCall`).
- A drop mid-call is tried again three times (after 1, 2 and 4 s) before the call falls back. Gemini resumes with its
  session handle and a fresh `startCall` credential; OpenAI gets a new session. Either way the repeated `startCall`
  adds "This call so far" to the voice's instructions (the call's newest lines, newest last, capped), so a session
  without a handle picks up the conversation. A `goAway` hands over at once, without showing a reconnect. A close
  Gemini gives with code 1007 or 1008 (an invalid key, model or request) fails at once: trying again won't help.
- Interruptions: the iPhone's realtime, speech and on-device engines pause on `AVAudioSession.interruptionNotification`
  and pick up again when it ends with `shouldResume`; otherwise the bar says "Audio paused by another app. Tap to
  resume." (as Android does). Route changes (AirPods) restart the iPhone's audio engine (`AVAudioEngineConfigurationChange`);
  the desktop restarts the microphone (and moves OpenAI's track) on `devicechange`. The desktop's on-device engine
  uses the speech helper's own microphone and isn't restarted.
- After the voice's turn, playback that reports no progress for 4 s returns the call to listening, so a stuck player
  (#214) can't hold the bot's replies back (`onStall`, counted as stuck `speaking`).
- Each engine of a call sends one `voice_call_quality` when it's replaced or the call ends: milliseconds until ready,
  reconnects, the first phase stuck too long (connecting or reconnecting over 20 s, or the stalled player), and why
  it failed (`microphone`, `connection`, `provider`).
- Tests replay recorded provider streams through the engines with a scripted socket, microphone and speaker: kit
  `CallRecoveryTests`, desktop `call-recovery.test.ts`, Android `VoiceEnginesTest` (Gemini over MockWebServer, OpenAI
  over a fake peer).

### The call outside the app

- iPhone: the app owns the call's audio session (`CallAudioSession`): each engine activates it as its audio starts
  and releases it as it stops (OpenAI's WebRTC manages its own with the same options); interruptions and route
  changes come from `AVAudioSession` notifications (above). CallKit carried the call in a build that never shipped and
  was taken out: [CallKit (archived)](../plans/callkit.md).
- iPhone Live Activity: while a call is on, the bot's activity shows it (`ContentState.call`: connecting, listening,
  speaking, paused; `callStartedAt` for its length) and the bot's latest progress line from the computer until its
  reply (`CallActivity`). It uses the bot's activity if one is up and starts one for the call otherwise; a task sent
  during the call replaces the call's own activity with one the computer can update. The computer never sets the call
  fields: the app keeps them in memory and merges them into every update, puts them back after a push from the
  computer replaces the state, and gives the call its own activity again when the computer's last push ends the
  task's. When the call ends, an activity whose bot is idle or failed ends (at once if the call started it, else a
  minute later) and one with a task under way stays without the call.
  While the call lasts the activity takes the call's own layout (`CallLiveActivity.swift`): the bot's avatar, the
  call's length over its name, and two round buttons, mute (`mic` / `mic.slash`, from `callMuted`) and end, in the
  expanded island and on the Lock Screen card, which also shows the progress line; the compact island shows the
  avatar and the length, the minimal one the avatar. The buttons are `LiveActivityIntent`s (`MuteCallIntent`,
  `EndCallIntent`, built into the app and the widget extension) that run in the app without opening it and press
  the call bar's own mute and end (`CallActivity.toggleMute` / `hangUp`), so an ended call logs its line as usual;
  muting from the call bar updates the activity.
- Android: the call's foreground notification shows its state and the bot's latest progress line (cleared by its
  reply), updated in place, with the bot's name, the call's length (a chronometer from its start) and Mute and End call.
- The desktop and the terminal have no Live Activity or call notification.

### Failure and cost

- No key on the computer, invalid key, quota or network failure at start, a dropped connection that three tries
  don't bring back, or a refused session: the call switches to On device without hanging up, with a one-line notice
  under the bar.
- Usage is billed by the provider to the user. The key form shows minutes used this month
  (`VoiceUsage`, counted on each device while a cloud engine is live and for each recording
  dictated with the provider) as an estimate, not a bill.

### Verification

On 2026-10-04 the OpenAI path was checked with a real key from a Mac (aiortc driving the same
client secret → `/v1/realtime/calls` SDP → `oai-events` sequence): session update with the four tools,
a spoken reply, and `send_to_bot` called for a Chinese request. The iOS Simulator's voice-processing
audio unit can abort with an RPC timeout when WebRTC starts its audio; use a physical iPhone for audio.

On 2026-10-25 the iPhone Gemini path (#214) was checked on the iOS Simulator with a real key: speech
played through the Mac's speakers reached Gemini, which answered aloud and called `send_to_bot`, and
the bot's reply came back into the chat. The Simulator doesn't cancel echo, so Gemini also hears its own
voice there; echo cancellation needs a physical iPhone. The same day, on an iPhone 18 Pro Max (a diagnostic
build logging each step): the engine reconfigured 0.2 s after every start; before the player was wired again its
buffers never finished, so the phase stayed "speaking" and the bot's reply was never said. Wired again, each
answer returned to listening, and several turns in English and Chinese followed, without Gemini hearing itself.
A route change mid-call (AirPods) is not checked yet.

Host tests (`chat::calls`) cover a call's flow (start, lines, `send_to_bot` with the user's words, the line at the
end), the voice's instructions (the relay role and the bot's name first, each section present and within its cap,
OpenAI's smaller share), transcript checks and dreaming over a call. `chat::calls::announce` tests cover what the
voice hears: new replies, approvals and failures said once, a reply unfinished when the call began, a group's
member named, the wording per call mode, progress rate-limited with a "still working" after a silence, and the
replies and notices kept for the device holding the call; `api::events` tests check that only that device gets
them, and `calls_e2e` that an on-device call hears its approval and reply. `StoreTests` (iPhone) hand each cue to
the call once, also when a reconnect replays it after an engine switch; the desktop tests parse cues
(`parseCallCue`) and Android's decode them (`CallCue`). Each client tests hiding a call's messages and merging its
conversation. On Android, `VoiceEnginesTest` (core, JVM) drives
each engine through fake audio: utterance cutting and clip playback, OpenAI's event sequence and SDP
exchange (MockWebServer), and a Gemini Live session over a real WebSocket (setup, audio both ways,
a tool call, a close with the provider's reason). Provider audio isn't
simulated: on a physical iPhone, check both engines with a real key (talk over a reply, ask "what
are you doing?", approve by voice, and turn Wi-Fi off for a moment mid-call: the bar shows Reconnecting, comes back
to Listening and still says the replies; with no network at all, after three tries, the call falls back to on-device
voice).


## OpenAI GPT-Live (2.32.0)

GPT-Live is opt-in through the existing OpenAI realtime model picker. `voiceStatus.supportsLive` advertises support; clients check the current host before starting or testing a saved Live selection and show an update instruction for older hosts. New clients request `voiceModels` with `supportsLive: true`; only those responses include conversational `gpt-live-*` models (not transcription models). Cached defaults remain Realtime, so an older client never automatically receives an incompatible model. Explicit Realtime selections continue through their existing engine.

The host implements `startLiveCall`, `callLiveEvent` and `callLiveUpdate`. It creates `POST /v1/live/sessions` with the client's gathered SDP offer, the provider key from its vault, `store: false`, and Responses delegation through `gpt-6-luna`. The voice prompt handles conversation and delegation; the existing call instructions and six tools belong to the Responses backend. The API key and routing configuration never go to the client. Audio remains direct WebRTC between the device and OpenAI.

The host configures the provider's data-channel subscription to deliver transcripts, session start/close/errors, and only the Responses lifecycle/completed-item events it uses. Backend token and function-argument deltas never enter the clients' ordered Host RPC queues, avoiding backlog on remote connections. The host owns transcript persistence, completed function-call handling, duplicate protection, and response continuation. An owner and provider-session check rejects another device, an ended call, or a stale connection. Completed tool outputs are sent before a single continuation for their response; lifecycle snapshots cannot be used to discover tools because Live forwards empty `response.output` arrays. Bot reply/notice cues become `session.commentary.append`, progress becomes `session.thinking.append`; the host splits updates into conservative UTF-8 byte-bounded pieces below the 500-token append limit. Transcript handoffs preserve speaker, pause and session boundaries while retaining exact spelling within split utterances.

Clients reuse their WebRTC transport and only send host-produced protocol messages back to the corresponding connection. They become ready on `session.started`, derive speaking state from received media activity (Live has no speech-done event), and fence all asynchronous results when a connection is replaced. Ending sends `session.close` and waits briefly for `session.closed`; timeout is recorded as unconfirmed finalization. Manual interruption gates local playback until the next input transcript, without issuing unsupported Realtime cancellation commands.

The Live model test checks API model access, not audio or Responses routing availability, and does not create a billable no-media session. Voice duration and the Responses router are charged separately. Terminal UI does not run an audio engine or model picker; its existing call transcript display is unchanged.

Saving an OpenAI key discovers a suitable model when model-list access is allowed. If discovery fails, it retains the existing Realtime credential-mint validation against the saved default; a failed mint still rejects the key. Discovery and validation share a 20-second budget before saving. Successful discovery supplies cached defaults directly, with no second network request after the key is saved. Restricted keys do not need model-list permission just to keep using Realtime, but discovering or testing GPT-Live models requires the corresponding model-read permissions.

Provider references: [WebRTC](https://developers.openai.com/api/docs/guides/voice-webrtc), [delegation and tools](https://developers.openai.com/api/docs/guides/live-delegation), [migration](https://developers.openai.com/api/docs/guides/live-migration).
