# Remote screen

Remote screen lets an authorized phone view and control a computer. Signaling travels through the end-to-end encrypted host API; video and input use WebRTC, including when a TURN server forwards the encrypted packets.

## Connection path

1. The phone calls `screenPrepare {relay, trickle: true}`. `relay` is true when its active host channel uses Cloudflare; direct LAN/Tailscale connections do not contact a STUN/TURN provider.
2. The host checks the `screen` scope and that screen access is enabled. For relay connections it signs `POST /v1/host/screen-ice {deviceKey}`. The Worker requires a registered, active computer belonging to an active account. The host attests the device's local authorization, so account grants and local QR pairing both work on a claimed computer.
3. The Worker issues Cloudflare Realtime credentials with a one-hour TTL, tags usage with `computerId:deviceFingerprint` (the fingerprint is the base64url encoding of the first 16 bytes of SHA-256 of the device public key; the resulting 45-character tag fits Cloudflare’s 64-character limit), and returns `Cache-Control: no-store`. Permanent TURN keys stay in Worker secrets.
4. The host reserves a random session ID bound to the calling device and helper connection, and keeps the exact ICE configuration in memory. It returns `trickle: true` only when the phone requests it and the helper advertises support. The negotiated mode stays with that session. Clients cannot supply arbitrary ICE servers to the host.
5. For trickle sessions, the phone opens a private `screenCandidates {session}` subscription on its encrypted channel and waits for `ready` before sending `screenOffer {session, sdp, display?}`. Both peers send their original candidate-free SDP after installing their local description, while gathering continues concurrently. The helper's candidates arrive on that subscription; the phone uploads its candidates through `screenCandidate {session, candidate}` after installing the answer, one acknowledged request at a time. Late TURN TCP/TLS candidates remain available. Older phones, hosts or helpers use complete SDP and gather for up to ten seconds. ICE prefers direct candidates and uses TURN when necessary, with UDP, TCP and TLS on port 443 available.
6. The phone reconnects with fresh credentials five minutes before expiry. The host checks active sessions every five seconds and closes expired sessions, revoked devices and expired account leases. A prepared session must start within one minute.

Connections prepared through the cloud use a **4 Mbps / 30 fps** encoding limit even if ICE subsequently finds a direct path. Direct sessions retain their platform defaults (Mac: 16 Mbps / 60 fps). These are encoding limits, not exact bandwidth or billing caps. TURN does not carry plaintext desktop content; WebRTC DTLS-SRTP and data-channel encryption remain between the phone and helper.

A cloud outage or missing TURN configuration produces an explicit error. A successful chat relay connection alone does not prove that media or input works.

## Phone orientation

Opening the iPhone viewer enables portrait and both landscape orientations without
requesting a forced turn. UIKit chooses the orientation based on how the phone is
held and the system rotation lock. The Rotate button remains available for an
explicit override. Closing the last viewer restores the app's portrait layout.
The video fits the available bounds, and touch coordinates use the resized viewport.

The iPad already supports all orientations. The desktop app and the TUI do not
use the iPhone viewer or its orientation policy; the capture helpers stream the
computer display without choosing the phone's orientation.

The Android viewer follows auto-rotate like the rest of the Android app. Rotate locks
the other orientation until the viewer closes. The activity handles orientation and
keyboard changes itself, so turning the phone or attaching a hardware keyboard keeps
the stream. In the background the Android viewer closes its stream and hands control
back to the bots, then reconnects when the app returns.

## Boundaries

- `host/src/screen/` coordinates access to the local helper and owns viewer sessions. Another device cannot renegotiate or close a session it does not own.
- `apps/screen-macos/` is the macOS capture/input helper (Xcode `Screen` target, `CodyncScreen.app`). The desktop app bundles it and registers it through `SMAppService` (`apps/desktop/src/main/screen.ts`); it is responsible for the OS permissions, including those of the computer-use driver it starts.
- `apps/screen-linux/` implements the Linux helper using desktop portals and GStreamer, including ICE URL conversion and TURN transport configuration. The host starts it from beside its own executable (then `PATH`). Linux host releases ship `codync-screen` in the same archive (built on Ubuntu 24.04: glibc 2.39+ and GStreamer 1.22+ with the base, good and bad plugins, PipeWire and xdg-desktop-portal at run time); `install.sh`, Homebrew and `codync-host update` put it next to the host.
- Helpers communicate locally through `~/.codync/screen.sock`. Updated helpers advertise `trickle` in `status`, emit ordered `candidate {session, candidate}` notifications, and accept the host's `candidate` request. Input uses the `input` and `input-fast` data channels.
- Linux waits for an in-progress answer before acknowledging that viewer's close, and cancels pending helper requests when the socket ends. A late answer cannot restart closed capture; other viewers can still negotiate and close independently.
- macOS replies stay bound to the socket that received the request. Closed peers ignore queued input and channel callbacks. The phone retires a failed peer before preparing its replacement and checks peer identity after each native negotiation await.
- A trickle session accepts one offer and one candidate subscription. Each direction permits 128 candidates of at most 4096 bytes followed by one `complete` event. Candidates include `candidate`, `sdpMLineIndex` and optional `sdpMid`; `type` distinguishes `candidate` and `complete`. The subscription starts with `ready` and may report `error` before ending. It never uses the shared chat broadcast.
- Both directions buffer candidates until the phone installs its answer. Native candidate additions and upload acknowledgments are awaited in order. Completion follows every candidate; Apple WebRTC records completion at app level because its native API has no usable null-candidate operation. The finite connection deadline remains in place.
- Overflow and candidate protocol errors end the attempt. Signaling loss ends an unfinished negotiation; once ICE connects or both directions deliver completion (including the phone upload acknowledgment), it detaches the candidate subscription and leaves media and input running. Credentials, Screen authorization and the on/off switch continue to govern the viewer independently. Recovery reserves a fresh session rather than resubscribing, reoffering or replaying an uncertain candidate addition. The phone waits for a ready host channel before spending one of its three recovery attempts; a successful answer alone does not reset that budget. A permanently stopped channel ends recovery with its authorization or update error. Closing, disabling screen access, changing helpers or removing Screen authorization closes media.
- Android negotiates the same way (`core` `ScreenSignaling`, `ScreenCandidate`, `ScreenRoute`); desktop, terminal UI and Apple Watch have no remote-screen viewer. Their screen-helper management and permission controls are unaffected. Cloud and relay forwarding already carry the encrypted subscription and RPC frames; they need no signaling changes.
- Phone viewers: iOS in `apps/ios/Kit/Sources/CodyncUI/Screen/`. Android keeps the same structure in `apps/android/app/.../Screen*.kt`, with gesture-to-input and viewport logic in `core` (`ScreenControl`, `ScreenViewport`). Both offer hardware H.264 first, High then Constrained Baseline.
- The Linux helper answers with the packetized H.264 payload the phone offers, feeds the encoder 8-bit 4:2:0 video and timestamps capture on the system clock. It answers before capture starts, so the profile is negotiated before the encoder fixes its output.
- Screen access is on by default on macOS and on Linux while a graphical session (Wayland or X11) is running; headless servers stay off. Turning it on or off is remembered. Enabling through `setScreenEnabled` requires a loopback caller. Interactive OS permission prompts must be completed on the computer.
- Bots with their computer capability enabled receive the built-in `computer` MCP tools, run by the computer-use driver that Codync Screen starts on macOS ([computer use](computer-use.md)). Their permission policy still applies. An interactive phone can take over; bots may still look.
- Each device may hold four sessions; the host allows 32 total, including pending sessions. Credential issuance is limited to 12 requests per minute per account by `TURN_LIMITER` (Cloudflare's per-location rate limiter).
- Closing a session stops its media; issued TURN credentials remain usable until their one-hour TTL. Issuance limits and encoder limits do not constitute a hard monthly spending cap. Track Realtime egress by the credential's custom identifier.

## Android incremental ICE

Android advertises `trickle: true` in `screenPrepare`, then uses incremental ICE
only when the host returns `trickle: true`. An omitted or false capability keeps
complete-SDP negotiation for older hosts and helpers. The host/helper protocol
is introduced in [PR #40](https://github.com/leepokai/Codync/pull/40).

The Android viewer subscribes to `screenCandidates` before offering, waits for
`ready`, and installs the answer before applying or uploading candidates. Each
direction permits 128 candidates of at most 4096 UTF-8 bytes, followed by one
completion. A full screen-signaling queue fails that session without blocking
unrelated RPCs. Protocol errors remain fatal; a transport disconnect after media
connects or both directions acknowledge completion leaves WebRTC running.

Android core owns `ScreenCandidate` and `ScreenSignaling`; the app's
`ScreenSession` connects them to native WebRTC. The iOS implementation and
host/helper contract are covered by PR #40. Desktop, terminal UI and Watch have
no viewer and receive no client signaling changes here.

## Cloud setup

Create a separate Cloudflare Realtime TURN key for each environment. Save its key ID as `TURN_KEY_ID` and its credential-issuing API token as `TURN_KEY_API_TOKEN`, both Worker secrets. Every environment needs the `TURN_LIMITER` binding in `cloud/wrangler.toml`.

From `cloud/`, for development:

```sh
npx wrangler secret put TURN_KEY_ID --env dev
npx wrangler secret put TURN_KEY_API_TOKEN --env dev
npm run deploy:dev
```

For production, omit `--env dev` and use `npm run deploy:main`. Secret prompts accept the values without putting them into shell history. Do not put them in the app, host database, checked-in files, or logs. Link the computer to a signed-in account before using TURN; unclaimed computers can still use direct screen connections.

References: [Cloudflare credential issuance](https://developers.cloudflare.com/realtime/turn/generate-credentials/), [usage analytics](https://developers.cloudflare.com/realtime/turn/analytics/), [pricing](https://developers.cloudflare.com/realtime/sfu/platform/pricing/).

## Verification

Run cloud tests/type checking, host formatting/Clippy/tests, iOS Kit package tests, Android `:core:test` and `:app:testDebugUnitTest`, and builds of the iOS app, the Android app and the macOS `Screen` helper target. Run Linux helper tests in a Linux environment with GStreamer and PipeWire development packages.

Live acceptance requires the configured Worker and fresh host/helper/phone builds:

- Check screen-recording and input permissions, capture, click/type/scroll, clipboard, display changes and closing the viewer.
- Run the checks below on an Android phone too, including a hardware keyboard and IME composition (Zhuyin, Pinyin).
- Measure first-frame connection time and input-to-visible-frame delay, including after several seconds of idle. Verify video continues through pinch/pan/reset and frame-size changes.
- Open the iPhone viewer while upright and while already held sideways, then rotate
  in both directions. Check video fitting and direct taps after each turn. Repeat
  with rotation lock on and with the Rotate button; closing returns to portrait.
- Repeat with the phone on cellular and the computer on a different network, without Tailscale. Verify a selected ICE candidate pair with `candidateType=relay` in WebRTC diagnostics and observe Realtime egress; signaling via Cloudflare alone is insufficient.
- Verify UDP-blocked connectivity using TURN over TCP/TLS 443 on both Linux and macOS. Check the selected relay's actual server URL and relay transport; a candidate's `tcp` token alone does not establish that TURN used TLS.
- Confirm that offer/answer exchange finishes before gathering completes, that late candidates reach both peers, and that candidates generated before the answer are applied afterward in order. Completion must come last.
- Check mixed old/new phone, host and helper combinations, duplicate offers/subscriptions, another device's session ID, queue overflow, signaling loss, fresh-session recovery and stale-session rejection.
- Verify Wi-Fi/cellular changes, reconnect after sleep and renewal before the one-hour expiry.
- Revoke the phone or disable screen access while streaming; input and media must stop. Verify a control-only device cannot call `screenPrepare`, `screenOffer`, `screenCandidate` or subscribe to `screenCandidates`.
- Verify LAN/direct-only mode without internet access, and the explicit error when relay secrets are absent.

See [Cloudflare testing](../guides/cloudflare-testing.md) for transport checks and [architecture](../architecture/overview.md) for data flow.
