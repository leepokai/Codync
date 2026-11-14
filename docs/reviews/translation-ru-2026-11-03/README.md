# Russian translation acceptance — 2026-11-03

PR #306 was integrated with current `dev`, preserving Brazilian Portuguese.
The original 1,513 Russian translations are unchanged; eight newer GitHub
sign-in phrases complete the current 1,521-key catalog. Placeholder parity,
locale detection, persistence, and language order are covered by tests.
Version 2.38.0 is already ahead of the latest release tag; both phone apps'
bilingual notes now include Russian in that pending release.

Three read-only reviews (correctness, completeness, refactor) reported no
findings after the repairs. No compatibility floor changes are needed.

## Automated checks

- Desktop: typecheck; 382 tests passed.
- iOS: 11 AppLanguage tests; signed iOS Dev simulator build passed.
- Android: 217 core and 89 app unit tests; debug APK and lint passed.
- Host: rustfmt, native dev build, nine TUI localization tests passed.
- User manual: 18 pages / 105 links passed.

## Actual application checks

| Platform | Flow | Result and evidence |
| --- | --- | --- |
| macOS Electron | Select Russian; reopen the app; select Follow system | Passed. Settings and main view updated to Russian, reopening retained Russian, Follow system restored this Mac's English. The Portuguese option remained available. Native Computer Use could open settings, but background animation/menu input was unreliable; Playwright connected to this worktree's real Electron DevTools endpoint completed the controls without replacing the app or mocking its bridge. |
| iOS 27 simulator | Select Russian; terminate/relaunch; follow English and Russian system locales | Passed through Device Hub. Settings immediately translated, reopening retained Russian, Follow system restored English and a Russian launch locale rendered Russian. [Screenshot](ios.png). |
| Android emulator | Select Russian; force-stop/relaunch; follow English and Russian per-app OS locales | Passed. Preference survived reopening; Follow system changed to English, then Russian after the emulator's per-app OS locale was set to `ru-RU`. Portuguese remained in the menu. Built-in Computer Use cannot target the emulator, so adb UI hierarchy/actions and screenshots were used. [Screenshot](android.png). |
| Terminal UI | Russian system locale; cycle to explicit Russian; restart under English locale | Passed in an actual PTY connected to the sandbox host. The command palette showed Russian and Follow system initially; after cycling it showed saved Русский, retained on restart with `LC_ALL=C.UTF-8`. |

The desktop screenshot shows the translated settings in the actual renderer;
background animation capture can retain translucent transition content.
Android's existing hard-coded push-configuration/footer and empty-state strings
remain English across languages and are outside Russian registration.
Tray/widget flows and Windows/Linux GUI checks were not walked in this minimal
macOS acceptance run; shared language contracts and packaged resource inclusion
are covered by tests and PR CI.
