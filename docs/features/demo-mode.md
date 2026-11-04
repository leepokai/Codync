# Demo mode

**Settings → Try demo mode** (desktop and Android) or **Demo mode: try it, or leave it** in the terminal's command list shows
sample data in place of the computer's: three bots (Scout, Pacer, Scribe), a session with steps and a diff, an approval
waiting for you, and a routine ("Morning check"). A badge ("Demo mode · sample bots, nothing here is real") stays on screen
with an **Exit demo** button.

- **Nothing real is touched.** The demo reads and writes nothing on the device, sends nothing over the network and calls no
  host. Anything that would change something (sending, creating, answering an approval) is refused with "Demo mode: nothing
  is sent or changed", and the app does not count what is done in the demo (analytics are off meanwhile).
- **Leaving restores exactly what was there.** The real model keeps running underneath while the demo is on; the demo is a
  second model that is thrown away on exit.

| Client | How |
|---|---|
| Desktop | `store/demo-mode.ts` builds a second `AppModel` on an in-memory `FakeHost` (the website demo's story, `renderer/demo/`) through `DemoTransport`; its stores keep their cache and drafts in memory (`volatile`); `main.tsx` shows it in place of the real model |
| Android | `core/DemoScript.kt` is the data; `DemoMode.shown` puts it over the real state's shell in `MainActivity`; `AppStore.ready()` refuses every call and `Analytics` drops events while it is on |
| Terminal | `tui/demo.rs` is the script; a second `App` on `net::Client::demo()` (it never connects) is drawn and fed keys while the real one keeps running |
| iPhone | not done (no iOS work was asked for) |

## The guard

A test fails if the demo reaches for the network, the disk or the real host: desktop `demo/demo-isolation.test.ts` reads the demo's files
and rejects `fetch`, `XMLHttpRequest`, `WebSocket`, `localStorage`, `window.codync.host`...; Android `DemoScriptTest` rejects
`java.io`, `java.net`, `SharedPreferences`...; terminal `demo::tests::the_script_is_made_of_data_only` rejects `std::fs`,
`reqwest`, `TcpStream`... These are checks of the source, not of a run: they keep the demo's own files honest.

## Known limits

- Device-wide settings changed while in the demo (appearance, language, text size) are saved as always: they belong to the
  device, not to the demo.
- The desktop demo's data is the website demo's story (a macOS-flavored computer name); the Android and terminal scripts are
  smaller.
