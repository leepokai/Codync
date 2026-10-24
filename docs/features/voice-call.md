# Voice call

A hands-free call with one bot or group from the iPhone, Android or desktop chat (the waveform button in an empty composer).
On Linux the desktop app offers only the cloud engines (on-device speech is macOS only). The terminal
UI has no calls (no audio stack); it shows the "Voice chat · 00:16" notice like every client.
The bot is the same agent on the computer; the call only changes how you talk to it.
On iOS, [Open Live Mode in Shortcuts](ios-shortcuts.md) can select a bot and start this call directly.

## On-device voice

- `apps/ios/Kit/Sources/CodyncUI/Call/` (iPhone) and `apps/desktop/src/renderer/views/call/` (desktop): `CallView` (the call bar) over a `VoiceEngine`; `OnDeviceEngine` is the default audio loop. On the Mac the desktop app runs speech recognition in `codync-speech` (`apps/desktop/native/speech-macos/`, driven by `apps/desktop/src/main/speech.ts`).
- Android: `CallBar` (`apps/android/app/.../VoiceControls.kt`) over the same `VoiceEngine` set, ported into `apps/android/core` (`VoiceEngine.kt`, `CloudSpeechEngine.kt`, `GeminiLiveEngine.kt`, `OpenAIRealtimeEngine.kt`, `CallOperator.kt`) with the phone's audio in the app (`VoiceAudioPorts.kt`, `WebRtcPeer.kt`). On device is `VoiceSession` (`LocalVoiceAudio.kt`): the call's language (Voice chat settings → Language, the phone's by default) is heard by Android's on-device recognizer on Android 13+, fed the call's own microphone (`PlatformSpeech.kt`; Android asks before downloading a language), with the bundled Vosk model for English where Android has none; each reply is read by an installed offline voice for the language it's written in, and a reply with no installed voice stays in chat with a notice; `VoiceService` (a microphone foreground service) holds audio focus and the Bluetooth/wired route for the call. Settings keys match the iPhone's (`callEngine`, `callMode.<provider>`, …).
- UI follows Grok Bot: a floating capsule over the top of the chat (`ThreadView` overlay), and the
  chat stays readable and usable underneath. Left to right: the bot's avatar (pulses while
  speaking; tap it to interrupt), a dotted level line (your voice while listening, a ripple while
  the bot speaks, a slow breath while it works), gear (call settings), mic (mute), red ✕ (end).
- Desktop: the capsule sits 16 CSS pixels below the window top and explicitly excludes itself
  from Electron's window-drag region so the gear, mute and end buttons receive mouse clicks.
  The iPhone places its capsule below native navigation; the terminal has no call controls.
- Speech → text with `SFSpeechRecognizer` (on-device when the language supports it) in the language
  picked under Voice chat → Language (`callLanguage`; empty follows the system's first language,
  which is often English on bilingual setups). A pause ends an
  utterance and sends it as an ordinary message (`BotStore.send`), so it takes the normal path:
  queued while the bot works, folded into its next turn. What you said appears in the chat as your
  message; the bot's reply appears as its message.
- Each new final reply (`data.final`) is read aloud with `AVSpeechSynthesizer` in the reply's own
  language; markdown and code blocks are dropped (`SpokenText`). The mic pauses while it speaks.
  An approval request is announced, answered in the chat.
- Gear: voice engine (below), and for On device the pause length before sending (1 / 1.5 / 2.5 s) and reading speed; kept on the phone.
- Ending the call calls `logCall {botId, seconds}`; the host adds a notice (`callSeconds`) that
  every client shows as "Voice chat · 00:16".
- The host receives recognized text; the Cloudflare transport sees encrypted channel frames.
  Codync does not forward microphone audio to the host. Speech recognition may use Apple services
  when on-device recognition is unavailable. `UIBackgroundModes: audio` permits
  background microphone capture and speech playback during an active call.
- While a call is listening or speaking, it retains the computer's transport across
  foreground/background transitions. Ending or failing the last call releases that
  connection when backgrounded, restoring normal push delivery. Removing the computer
  or switching accounts ends its calls and closes the transport.
- Final replies and approval notices are delivered from the store's incoming events
  directly to the audio session, rather than waiting for a SwiftUI view update.
  Replayed final replies and other bots' replies are not read into this call.

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

The gear's first section is **Voice engine** (applies from the next call):

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
iPhone, **Computers & settings → Voice chat**; on Android, **Settings → Voice and API keys**; on the desktop, **Settings → Voice chat** (this
computer) or the waveform button on a computer's card in **Computers & devices**. Keys are entered
on the computer only: the host takes `setVoiceKey` from loopback, and the iPhone shows whether a key
is there and can test it. Picking a cloud engine shows its key form instead of the on-device settings: a secure field, a link
to the provider's key page, **Test** (the computer checks a new key with the provider and keeps it
only if it works, otherwise the provider's error is shown; with a key saved, it mints a call
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
  {provider, key}` (checked with one credential mint before it's kept; empty removes it),
  `voiceSession {provider, model, voice}` → `{credential}`, `voiceModels {provider}` → `{models,
  transcribe, speech}`, `voiceTranscribe {provider, model, audio}` (base64 16 kHz WAV) → `{text}`,
  `voiceSpeak {provider, model, voice, text}` → `{audio, format}`.
- The long-lived key is used for minting a short-lived credential at call start (OpenAI
  `POST /v1/realtime/client_secrets`, 10 minutes; Gemini `POST v1alpha/auth_tokens`, one use) and
  listing models. The client's audio session only ever sees that credential.
- Audio is not proxied through the host: it would cross the encrypted channel and the relay
  (latency, relay bandwidth). The device streams to the provider directly.

### How the call works

The realtime model is an operator in front of the bot, not a replacement for it (`CallOperator`):

- Session instructions: who the bot is (name, description), that coding is done by the bot, reply
  in the user's language, keep spoken answers short.
- Tools the client implements against `BotStore`: `send_to_bot(text)` (an ordinary message, same path
  as on-device calls), `bot_status()` (status, activity line, pending approval and its options),
  `recent_messages(count)` (chat-visible entries only), `answer_approval(option)` (only after the
  user said which; matched by option name).
- Each new final reply is added to the realtime conversation as `[<bot> replied] …` and an approval
  as `[Notice] …` (`BotStore` hands them over as `speak` / `announce`); the model says them,
  shortened for speech. They wait while the model is talking, so they don't cut it off.
- Barge-in is the provider's voice activity detection (OpenAI semantic VAD; Gemini's automatic
  VAD, with the voice-processing input cancelling the device's own playback). Tapping the avatar
  cancels the response locally.
- Gemini connections are replaced before the server's limit: `goAway` closes the socket and the
  session resumes on a fresh token with its resumption handle; context window compression lifts the
  15-minute audio session cap.
- The chat stays the record: only `send_to_bot` messages and the bot's replies are entries. The
  operator's small talk isn't saved; the call still ends with the "Voice chat" notice.

### Failure and cost

- No key on the computer, invalid key, quota or network failure at start, or a dropped connection mid-call: the
  call switches to On device without hanging up, with a one-line notice under the bar.
- Usage is billed by the provider to the user. The key form shows minutes used this month
  (`VoiceUsage`, counted on each device while a cloud engine is live and for each recording
  dictated with the provider) as an estimate, not a bill.

### Verification

On 2026-10-04 the OpenAI path was checked with a real key from a Mac (aiortc driving the same
client secret → `/v1/realtime/calls` SDP → `oai-events` sequence): session update with the four tools,
a spoken reply, and `send_to_bot` called for a Chinese request. The iOS Simulator's voice-processing
audio unit can abort with an RPC timeout when WebRTC starts its audio; use a physical iPhone for audio.

`StoreTests` cover the operator's tools against a scripted host (send, status with an approval,
option matching, recent messages) and approvals reaching a call as notices. On Android, `VoiceEnginesTest` (core, JVM) drives
each engine through fake audio: utterance cutting and clip playback, OpenAI's event sequence and SDP
exchange (MockWebServer), and a Gemini Live session over a real WebSocket (setup, audio both ways,
a tool call, a close with the provider's reason). Provider audio isn't
simulated: on a physical iPhone, check both engines with a real key (talk over a reply, ask "what
are you doing?", approve by voice, and turn off Wi-Fi mid-call to see the fallback).
