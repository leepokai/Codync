# PostHog error triage — 2026-11-03

Four error signals in PostHog (project "Codync App", 649427), 2026-10-04 to 2026-11-03, traced to a
root cause and fixed in 2.39.0. Method as in the [2026-10-19 triage](posthog-errors-2026-10-19.md):
`$exception` events by `process`, `app_version` and `$os`, persons checked against cohort 618410
(internal and test hosts); every person below is outside it. Error tracking itself is described in
[usage analytics](../features/analytics.md#error-tracking).

## Findings

| # | Signal | Users | Root cause | Fix |
|---|---|---|---|---|
| 1 | Android `NoSuchElementException`, Android 16, builds 2.17.5 to 2.32.0 (`env = dev`: the internal Android build) | 4 | `EncryptedChannel` emptied its pending calls and streams with `keys.toList()` when a connection dropped. On a one-entry map `toList()` calls `iterator().next()`, which throws when a reply or an unsubscribe on another IO thread removes that entry meanwhile. Thrown from the reader coroutine's `catch`, under a `SupervisorJob` with no handler, it ended the app. | 2.39.0: the maps' live key views (`drain`), which never throw on a concurrent removal |
| 2 | Desktop renderer `TypeError`, mechanism `onerror`, in `computerSelection` (`ComputerFilterHeader.tsx`), macOS 2.34.2, 3 events | 1 | `pref()` trusted `JSON.parse` of localStorage: a stored `null` for `hiddenComputers` reached `hidden.split` and took the whole chat window down, on every launch. | 2.39.0: a stored value of another kind than the preference's default reads as the default |
| 3 | Desktop `RendererGone`, reason `crashed` (7 of 8 on Windows) or `oom` (2, Windows), 2.20.0 to 2.34.2, 18 events | 9 | Unknown: the report carries only Chromium's reason. The window then stayed blank until the app was restarted. | 2.39.0: the window loads its page again (at most twice in a row before it has stayed up a minute); a view that throws while rendering shows *Something went wrong* with *Try again* (reported with mechanism `react`) |

| 4 | iPhone `EXC_CRASH` (SIGABRT from an Objective-C exception in AVFAudio) in `OnDeviceEngine.listen()`, after `finishedSpeaking`, 2.34.2, iOS 26.7.2 and 27.0.1 (issue `01a19565`) | 2 | Inferred (the report carries no exception reason): the microphone tap was installed at the node's output rate, which stays at the old rate when the route changed while the engine was stopped for the reply, and AVFAudio asserts `format.sampleRate == inputHWFormat.sampleRate`. | 2.39.0: `MicTap.format(of:)` taps at the input node's hardware rate, for every call engine and dictation |

## Verification

- 1: `:core:test`. Reproduce: an Android emulator paired with a sandbox host, a chat open while
  the host is stopped and started a few times; the app shows the lost connection and reconnects.
- 2: `apps/desktop/src/renderer/lib/prefs.test.ts`. Reproduce: in a dev sandbox, set localStorage
  `hiddenComputers` to `null` over CDP and reload; the chat window shows every computer.
- 3: Reproduce with CDP `Page.crash` on a dev sandbox's chat window: it reloads by itself; its
  third crash in a row within a minute leaves it down.

## Open

- 3: why the renderer crashes on Windows is still unknown; watch `RendererGone` after 2.39.0. A
  window that reloads keeps reporting each crash.
- 4: the cause is inferred and the rate-changing path never runs on a simulator (input and hardware
  are both 48 kHz there; `MicTapTests` covers it): watch issue `01a19565` after 2.39.0.
- Not ours: a desktop renderer `TypeError` (issue `01a1a14b`), one Windows user on 2.34.2, four
  times in two minutes while a group chat was answering. That install's renderer bundle had been
  edited on the computer: the same user's earlier `ReferenceError` on 2.33.6 (issue `01a19268`)
  runs through `window.__antigravity_handle_composer_keydown` at line 55425, a function no Codync
  release or commit has, in a bundle whose shipped copy ends at line 55267, called from an
  `onKeyDown` at a column the shipped line doesn't reach. On 2.34.2 every React frame matches the
  shipped Windows bundle (taken from the release's installer), but the composer's frame lands
  inside the `@`-mention parser's inner function, which can't throw there and would have its own
  frame: the edited composer code is what threw. Since 2.39.0 the window shows *Something went
  wrong* with *Try again* instead of going blank. To check a Windows report against the shipped
  code, cut the 7z payload out of `codync-windows-x64.exe` (it starts at the `7z\xBC\xAF\x27\x1C`
  signature), extract `resources/app.asar` with `bsdtar` and read it with `@electron/asar`.
