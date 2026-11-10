# Windows remote screen

Planned 2026-11-06, not built. Phones can view and control macOS and Linux computers; a Windows computer has no screen helper, so its phones have no remote screen. Bots can already operate Windows through the computer-use driver ([computer use](../features/computer-use.md)). This plan comes after [optional sign-in](optional-sign-in.md), which it shares no code with.

## Decisions so far

- **A helper, like the other two.** `codync-screen` on Windows speaks the same [helper protocol](../features/remote-screen.md#boundaries), so the host and both phone viewers stay as they are, apart from the "not yet supported" text.
- **Both routes come with it.** The helper takes `iceServers` and encoding limits from the host, so direct (no STUN or TURN) and relayed sessions need no Windows-specific work.

## What's there to build on

`apps/screen-linux` is 2,423 lines of Rust on GStreamer `webrtcbin`. By file, about 600 lines are tied to Linux: `portal.rs`, `input.rs`, `a11y.rs`. The sessions, trickle ICE, negotiation and H.264 code (`sessions.rs`, `candidates.rs`, `negotiation.rs`, `connection.rs`, `video.rs`, most of `stream.rs`) should carry over. This comes from reading the file layout; nothing has been ported yet.

## Work

| Area | Change |
| --- | --- |
| Capture | `d3d12screencapturesrc` (or `d3d11screencapturesrc`) in place of `pipewiresrc` |
| Encoding | A Windows encoder list ahead of the software ones: `mfh264enc`, then the vendor encoders, then `openh264enc` or `x264enc` |
| Input | `SendInput`, through a maintained crate such as `enigo`, since the helper forbids `unsafe` |
| Host | `serve_helpers` is empty on Windows: accept the helper on a named pipe (`screen/cua.rs` already uses one), start the helper beside the host as on Linux, and make `graphical_session` true |
| Packaging | Bundle the needed GStreamer plugins in both installers |
| Clients | Remove "phone screen viewing is not yet supported" from setup; update [desktop app](../architecture/desktop-app.md) and the user manual |
| Analytics | `screen_*` events gain nothing new, but failures need a Windows `stage` |

## Risks to check first

Run these as a spike in the Windows VM before building anything else:

1. **ARM64 coverage.** GStreamer's download page lists official MSVC installers for x86_64 and arm64 (checked for 1.28.8). It doesn't say whether the arm64 one has every plugin above: `gst-inspect-1.0` in the VM answers that. The pipeline to prove: capture, H.264, `webrtcbin`, a phone showing a picture.
2. **Installer size.** The whole GStreamer runtime is large; measure the plugin subset.
3. **Signing.** An unsigned program that captures the screen and injects input is what antivirus software looks for. [SignPath](../../docs/guides/windows-testing.md) isn't wired into CI yet, and this probably needs it first.
4. **Test hardware.** The UTM VM has no hardware encoder, so it only proves software encoding. `mfh264enc` and the vendor encoders need a real Windows PC.

## Limits to accept

These come from Windows itself, from memory rather than tested here: a helper running as the user can't capture the UAC prompt or the lock screen (the secure desktop), and can't click into a window running as administrator. A service running as SYSTEM could, at a much higher security cost. The first version doesn't try; the viewer says so when the picture stops.
