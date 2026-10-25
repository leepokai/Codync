# CallKit for voice calls (archived)

Status: built for 2.26.0 as part of stage 4 of [voice calls owned by the host](voice-architecture.md), then taken out
before release by the owner's decision (2026-10-25). The iPhone app keeps its own audio session, as before; the
Live Activity call state and Android's call notification from the same stage stayed. Current behavior:
[the call outside the app](../features/voice-call.md#the-call-outside-the-app).

## What it did

- **The call as a phone call**: one `CXProvider` (audio only, one call group, `includesCallsInRecents = false`,
  generic handle) reported each Codync voice call as an outgoing call to the bot (the handle was the bot's name),
  connecting at once and connected when the engine reached ready (`reportOutgoingCall(connectedAt:)`). It showed on the
  Lock Screen and in the Dynamic Island.
- **System actions**: ending it from the Lock Screen, the Dynamic Island, AirPods or by answering another call
  (`CXEndCallAction`) hung up the Codync call; `CXSetMutedCallAction` synced both ways with the call bar's mute; a phone
  call holding it (`CXSetHeldCallAction`) paused the engine the way an audio interruption does (a `callHeld`
  notification into `AudioSessionEvents`), and off hold it resumed once CallKit had activated the audio session again.
- **CallKit owned the audio session**: the call view waited for `provider(_:didActivate:)` before starting the engine,
  at most 1.5 s (`CallKitCall.audioTimeout`; the activation never comes on the Simulator). The engines set the
  category but neither activated nor deactivated the session while CallKit ran the call (`CallAudioSession.systemRuns`).
- **OpenAI's WebRTC audio** followed CallKit's activation: `RTCAudioSession.useManualAudio = true` while CallKit ran the
  call, `audioSessionDidActivate`/`DidDeactivate` and `isAudioEnabled` from the provider's activate/deactivate, so a
  reconnect or an engine switch didn't deactivate the session under the call.
- **Fallback**: CallKit refusing the call, resetting, or ending it before it got audio or connected (the Simulator does
  this to every call) left the call going on without it, on the app's own audio session.
- **Mainland China**: CallKit isn't allowed in apps offered on mainland China's App Store, so it stayed off when the
  storefront was `CHN` (`Storefront.current`; without one, the region `CN`): `SystemCallPolicy`.
- **Background mode**: `voip` in `UIBackgroundModes` (`apps/ios/Resources/Info.plist`); without it CallKit refused
  calls as unentitled. App Review may ask about it.

Code: `apps/ios/Kit/Sources/CodyncUI/Call/SystemCall.swift` (`SystemCallPolicy`, `CallKitCalls`, `CallKitCall`), the
CallKit wiring in `CallView.swift` (start, `audioReady`, `connected`, `setMuted`, `end`), `CallAudioSession`'s
CallKit half in `CallAudio.swift`, tests in `CodyncUITests/SystemCallTests.swift`.

## What review found and fixed in it

- OpenAI's WebRTC activated and deactivated the session CallKit owned, so a reconnect could leave the call's audio
  dead after a hold: fixed with WebRTC's manual audio (above).
- Taken off hold before CallKit gave the audio back, the engine resumed on an inactive session: it waited for the
  activation.
- The first build closed the whole call when the Simulator's CallKit ended it at once (a `00:00` line): CallKit ending
  a call that never connected became "go on without it".

## Not verified

On a physical iPhone: the system call UI, ending and muting from it and from AirPods, holds from a real phone call,
and OpenAI under CallKit. The Simulator ends every CallKit call right after it starts.

## Bringing it back

- Added in `9b6b9ede` (feat(apps): calls show on the Lock Screen, in the Dynamic Island and in the notification) and
  `1ecd8353` (fix(apps): calls keep their audio and Live Activity under CallKit).
- Taken out in "fix(ios): take CallKit out of voice calls, keep the Live Activity" (the commit that added this page).
  Reverting it restores the code, the `voip` background mode, the tests and the docs.
- Before shipping it: walk the list under "Not verified" on a phone, and expect App Review to ask about `voip`.
