# Remote screen

Remote screen lets an authorized phone, or the desktop app, view and control a computer. Signaling travels through the end-to-end encrypted host API; video and input use WebRTC, including when a TURN server forwards the encrypted packets.

## Connection path

1. The phone calls `screenPrepare {relay, trickle: true}`. `relay` asks for TURN: when the host channel uses Cloudflare, and by the computer's Connection choice when an attempt fails (`ScreenRoute`, both phones): **Auto** goes direct, then through TURN; **Cloudflare first** the other way round; **Wi-Fi or Tailscale only** never asks. Direct LAN/Tailscale connections do not contact a STUN/TURN provider. A computer in no account has no TURN: its phones go direct only.
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
back to the bots, then reconnects when the app returns, with the control you last chose.

## Boundaries

- `host/src/screen/` coordinates access to the local helper and owns viewer sessions. Another device cannot renegotiate or close a session it does not own.
- `apps/screen-macos/` is the macOS capture/input helper (Xcode `Screen` target, `CodyncScreen.app`). The desktop app bundles it and registers it through `SMAppService` (`apps/desktop/src/main/screen.ts`); it is responsible for the OS permissions, including those of the computer-use driver it starts.
- `apps/screen-linux/` implements the Linux helper with GStreamer, including ICE URL conversion and TURN transport configuration. It captures and injects input through the desktop portals; on a plain X server without them (a cloud computer's virtual display) it uses `ximagesrc` and XTest on `$DISPLAY` (`backend.rs`, `x11/`), never another X socket (one may be another user's desktop) and not on a Wayland session. It checks for portals again until it finds them, so a desktop session that starts after the helper still gets them, and setup moves a helper that chose X over to portals that came up later. X needs no permission dialog. On X every session and every still for the bots takes its frames from one `ximagesrc` per display, through an `appsrc` each (`x11/capture.rs`), started with the first and stopped a second after the last: `ximagesrc` grabs the X server for each frame and, inside the grab, takes Xlib's process-wide lock, so a second one opening its display meanwhile (holding that lock while the server, held by the grab, can't answer) froze the X server for every app on that desktop. A session's task stops with the session, letting go of its data channels and its source. An `up` lets go of the modifiers its `down` pressed, and a viewer that leaves mid-press has its own button and modifiers let go (never another viewer's). A `view` naming another display swaps the session's capture source in place and moves its input there. The host starts it from beside its own executable (then `PATH`). Linux host releases ship `codync-screen` in the same archive (built on Ubuntu 24.04: glibc 2.39+ and GStreamer 1.22+ with the base, good and bad plugins, PipeWire and xdg-desktop-portal at run time); `install.sh`, Homebrew and `codync-host update` put it next to the host.
- Helpers communicate locally through `~/.codync/screen.sock`. Updated helpers advertise `trickle` in `status`, emit ordered `candidate {session, candidate}` notifications, and accept the host's `candidate` request. Input uses the `input` and `input-fast` data channels.
- Linux waits for an in-progress answer before acknowledging that viewer's close, and cancels pending helper requests when the socket ends. A late answer cannot restart closed capture; other viewers can still negotiate and close independently.
- macOS replies stay bound to the socket that received the request. Closed peers ignore queued input and channel callbacks. The phone retires a failed peer before preparing its replacement and checks peer identity after each native negotiation await.
- A trickle session accepts one offer and one candidate subscription. Each direction permits 128 candidates of at most 4096 bytes followed by one `complete` event. Candidates include `candidate`, `sdpMLineIndex` and optional `sdpMid`; `type` distinguishes `candidate` and `complete`. The subscription starts with `ready` and may report `error` before ending. It never uses the shared chat broadcast.
- Both directions buffer candidates until the phone installs its answer. Native candidate additions and upload acknowledgments are awaited in order. Completion follows every candidate; Apple WebRTC records completion at app level because its native API has no usable null-candidate operation. The finite connection deadline remains in place.
- Overflow and candidate protocol errors end the attempt. Signaling loss ends an unfinished negotiation; once ICE connects or both directions deliver completion (including the phone upload acknowledgment), it detaches the candidate subscription and leaves media and input running. Credentials, Screen authorization and the on/off switch continue to govern the viewer independently. Recovery reserves a fresh session rather than resubscribing, reoffering or replaying an uncertain candidate addition. The phone waits for a ready host channel before spending one of its three recovery attempts; a successful answer alone does not reset that budget. A permanently stopped channel ends recovery with its authorization or update error. Closing, disabling screen access, changing helpers or removing Screen authorization closes media.
- Android negotiates the same way (`core` `ScreenSignaling`, `ScreenCandidate`, `ScreenRoute`); the desktop app has its own viewer (below); terminal UI and Apple Watch have none. Their screen-helper management and permission controls are unaffected. Cloud and relay forwarding already carry the encrypted subscription and RPC frames; they need no signaling changes.
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
host/helper contract are covered by PR #40.

## Desktop viewer

The desktop app shows another computer's screen in a sheet: the **Screen** button on that computer's heading in the
sidebar, or on its card in Settings → Computers (not for the computer the app runs on, whose screen is right there).
`views/screen/ScreenViewer.tsx` draws it; `store/screen-session.ts` holds the WebRTC peer (receive-only H.264, data
channels `input` and `input-fast`), recovery and renewal like the iPhone's `ScreenSession`.

- **Signaling**: `screenPrepare {relay, trickle: false}`, then `screenOffer` with the complete offer once gathering
  ends (at most 10 s). `relay` is on for an account computer, reached over the relay's encrypted channel
  (`main/channel/`), so the host hands out TURN servers; off for an SSH computer on the loopback tunnel, which goes
  direct only. Complete SDP needs no `screenCandidates` stream on either route.
- **Mouse**: forwarded as it happens, not as gestures: `down`/`up` with the button, the click count (so a double-click
  stays one) and the modifiers held, `move` once per frame on `input-fast`, the wheel as `scroll` in pixels.
- **Keyboard**: a hidden text field holds focus. Named keys and shortcuts go as `key` (the letter the keyboard layout gives, so German Ctrl+Z stays Z, else
  the physical key, so ⌘⌥Z is Z), and the app's menu shortcuts (⌘F, ⌘R, …) stay off while the field has focus on a live screen (a dropped connection gives them back)
  (`main/remote-keyboard.ts`; paste keeps its menu item); typing, characters typed with Option or AltGr (`@` on a German keyboard) and input methods (Zhuyin,
  Pinyin) arrive as `text`. On a Mac viewing Linux or Windows, ⌘
  acts as Ctrl. Paste sends the local clipboard (`clipboard`) before the shortcut; **Copy from computer** appears
  when the computer copied text. Escape and Tab go to the computer while you have control; the close button ends the
  session.
- **Control**: opening takes over from the bots (`screenTakeover`), like the iPhone opened from Computers; opened from
  the Computer tab's picture of a bot that's using the computer (`AppModel.openScreen(id, watching)`), it watches first,
  like the phones' chat **Computer** button. **Hand back to bots** and closing return it. The last choice holds while it's
  open: a session that replaces one after an outage, or control allowed again on the computer, brings it back, and back
  live after a failure or a long outage it asks again if the host handed control back meanwhile. The phone viewers do
  the same. A view-only computer says so. The host ties a takeover to the device that asked: once that
  device has no screen session left, control goes back to the bots, even while another device (a Computer tab picture)
  still watches.
- **Other clients**: phones reach a computer's screen from Manage computers, from a chat's **Computer** button and
  from the Computer tab, which watch a bot at work before taking over; their computer headings keep only Extensions,
  since those entries already reach every computer. The desktop watches a bot from its Computer tab and has no chat
  **Computer** button. The TUI has no viewer.

## The bot details' Computer tab

Each bot runs on one computer, so bot details (desktop inspector, iPhone and Android sheets) have a **Computer** tab
between Details and Library: the computer's name and connection label, a live picture of its screen, and the Use the
computer switch (moved from Details). The picture is a view-only session of its own (no `screenTakeover`, no input) at
the picture's size, 16:10 and letterboxed. Opening the tab starts nothing: the picture offers "Click to see the screen"
("Tap…" on the phones) and connects only when clicked, again on each visit (leaving the tab, or the window or app going
to the background, asks again); from then it streams only while the tab shows, the window or app is in front and the
full viewer isn't open (one stream per device), and stops when any of that ends.

**Idle pause**: the picture and the full viewer stop streaming after 5 minutes of live picture with nobody touching the
app (counted from when it goes live; a connecting or failed session sends nothing and never pauses. Desktop: the
pointer, a key or the wheel in that window; phones: any touch in the app; watching alone doesn't count) and show
"Paused. Click to continue." ("Tap…" on the phones) until clicked, which starts a new session (the viewer keeps its
Take over or Hand back choice, and the display picked from Displays: desktop `displayToShow`, while the computer
still has it, else the main one; phones: kept in their viewer state); while the computer is offline or its screen unavailable, the viewer shows why instead.
One constant per app (`SCREEN_IDLE_MS` in `lib/screen-state.ts` on the desktop, `ScreenPause.idleLimit` on the
iPhone, `SCREEN_PAUSE_AFTER_MS` in `ScreenPause.kt` on Android); each pause reports `remote_screen_paused`. It bounds what a picture left open costs: TURN traffic for a relayed computer, and the helper's
capture and encoding on the computer. A bot using the computer shows as
"{name} is using the computer" over it. Clicking or tapping it opens the full viewer: watching when this bot is the one
using the computer, else as from Computers. Until it's live the picture shows the viewer's own notices
(`screenNotice` in `lib/screen-state.ts` on the desktop, `ScreenNotice` on the iPhone, `unavailableNotice` on Android): a sleeping cloud computer reads "Waking the computer…",
since a device connecting through the relay is what wakes it (no separate wake call), and "Connecting to the screen…" while the app is still connecting. A session sends its picture size again once its `input` channel opens, since the helper drops a `view` sent before. The desktop's own computer has
no picture. The TUI's **Computer** page has the name, state and switch without a picture. `screen_viewed` reports it
as `bot_computer`; each session's outcome is the host's `remote_screen_finished`.

## Cloud computers

A cloud computer's screen is a desktop of its own: Xvfb on `:20` (1920×1080) with an Xfce session, user units
`codync-display` and `codync-desktop` of the agents' user `codync-agent`, which the first-boot setup installs with
GStreamer (x264, libnice) and puts `DISPLAY=:20` in the host's service (`cloud/src/cloud-setup.ts`). The session runs
on that user's own bus, with the display in its user manager's environment, so the screen helper and the computer-use
driver read the apps' accessibility tree on the same accessibility bus as the apps. Phones, the
desktop app and bots all see that desktop. It is never the provider's own desktop (InstaBox's Xvnc on `:1`, Boat's
Xorg on `:0`): those run as the provider's user, which has passwordless sudo, so handing them to the agents would hand
them root. Computers made before this have no desktop until they are made again.

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

Run cloud tests/type checking, host formatting/Clippy/tests, desktop `npm run typecheck` and `npm test`, iOS Kit package tests, Android `:core:test` and `:app:testDebugUnitTest`, and builds of the iOS app, the Android app and the macOS `Screen` helper target. Run Linux helper tests in a Linux environment with GStreamer and PipeWire development packages.

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
