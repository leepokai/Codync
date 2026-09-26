# Voice call

A hands-free call with one bot from the iPhone chat (phone icon in the header). The bot is the
same agent on the computer; the call only changes how you talk to it.

## Implemented: on the phone

- `kit/Sources/CodyncUI/Call/`: `CallSession` (audio loop) + `CallView` (full-screen layer).
- Speech → text with `SFSpeechRecognizer` (on-device when the language supports it). A 1.5 s pause
  ends an utterance and sends it as an ordinary message (`BotStore.send`), so it takes the normal
  path: queued while the bot works, folded into its next turn.
- Each new final reply (`data.final`) is read aloud with `AVSpeechSynthesizer` in the reply's own
  language; markdown and code blocks are dropped (`SpokenText`). The mic pauses while it speaks;
  tapping the avatar interrupts. An approval request is announced, answered in the chat.
- The host receives recognized text; the Cloudflare transport sees encrypted channel frames. Codync does not forward microphone audio to the host. Speech recognition may use Apple services when on-device recognition is unavailable. `UIBackgroundModes: audio` keeps the call
  alive with the screen locked.

## Proposal: cloud realtime voice (not implemented)

For better recognition (mixed Chinese/English, code terms) and a real conversation (barge-in,
"what are you doing?" answered from the transcript):

- A realtime speech model (OpenAI Realtime or Gemini Live) acts as the operator in front of the bot.
  Its tools: send a message to the bot, read the bot's status and recent transcript. The coding is
  still done by the bot's own agent.
- The phone connects to the provider directly (WebRTC). Our server only mints a short-lived client
  token after checking the account's Pro entitlement and remaining minutes; audio never passes
  through it, and the provider key never ships in the app.
- Bot traffic stays on the encrypted channel; only the voice part goes to the provider, which the
  call screen states.
- Billing: App Store subscription (RevenueCat entitlement), a monthly minute cap enforced when
  minting tokens.
