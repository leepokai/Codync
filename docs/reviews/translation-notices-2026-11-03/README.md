# Notice localization review — 2026-11-03

PR #287, based on dev `054edeff890e12c51804b9e709d6846dd41fb33a`, reviewed and repaired in the isolated `Codync-i18n-287` worktree.

## Repairs and review

- Completed the five new notice/trace phrases in all nine catalogs, including Portuguese and Russian; all 1,526 keys and placeholder sets agree.
- Template substitution preserves names and message content. Only explicit UI-owned placeholder roles translate recursively, and substitution runs once, preserving literal braces and replacement characters in user data.
- Localized queued-message labels, routine notices, generic tool titles, plan headings and turn labels across desktop, iPhone, Android and TUI. Custom tool titles, plan items and message bodies stay verbatim.
- Added regression coverage in all four localization runtimes, both phones' bilingual 2.38.0 notes, localization guidance and the user manual. Version 2.38.0 was already ahead of the latest release tag.
- Three read-only reviews found no correctness or completeness issues; a documentation placement suggestion was applied. Visual E2E found a remaining desktop trace notice omission, which was fixed and retested.

## Validation

- Desktop: typecheck and 384 tests passed, rerun after the final trace repair.
- Host: native development build and 134 TUI tests passed.
- iPhone: 12 language tests and signed simulator development build passed.
- Android: 217 core tests (one skipped), 90 app tests, debug APK and lint passed.
- User docs: 18 pages and 105 links checked.

The first remote iPhone run timed out while UIKit initialization waited on SpringBoard. A rerun timed out in another screen-recovery test while the diagnostic reported cooperative/dispatch thread exhaustion; its other 359 tests passed. The same head passed all 361 tests locally, including the initially reported test alone. CI now waits for the selected simulator to finish booting and disables parallel test simulators. Its complete boot-and-test command passed all 361 tests locally from a shut-down task simulator, with retries enabled and no tests skipped. This CI-only adjustment changes no shipped app behavior; the app walkthroughs below remain applicable. Remote confirmation is required by the merge gate.

## Real-app E2E

All clients connected to this worktree's real host. A local deterministic ACP fixture produced a routine named `Plan`, a generic `Tool`, a custom tool named `Plan`, and a plan item named `Plan`. A real composer submission queued `Standby` while the routine was busy. Releasing the fixture completed both turns; no external AI call or mocked client bridge was used.

| Platform | Flow | Result and evidence |
| --- | --- | --- |
| macOS desktop | Simplified Chinese chat and completed trace | Pass: translated routine, queue, tool, plan and turn labels; original names and messages preserved. [Chat](desktop-chat.png), [trace](desktop-trace.png). |
| iPhone simulator | Same host conversation and trace through Device Hub | Pass: matching labels and preserved content. [Chat](ios-chat.png), [trace](ios-trace.png). |
| Android emulator | Same host conversation and trace | Pass: matching labels and preserved content. [Chat](android-chat.png), [trace](android-trace.png). |
| macOS TUI | Actual PTY, bot selection, queued transcript and trace navigation | Pass: localized routine, queue, tool, plan and numbered turn headings; `Plan` and `Standby` preserved. |

Desktop native accessibility could not obtain this Electron window, so Playwright attached to the actual Electron process over its local debugging port; reduced-motion emulation avoided animation stability waits. Android used adb because the native computer-use surface could not target the emulator. iPhone interaction used Device Hub.

GUI checks exercised Simplified Chinese; automated catalog and substitution checks covered all nine translations. This was the minimal macOS E2E run, without Linux/Windows GUI checks. Unchanged widgets/tray surfaces were not walked. The older `Created routine: Plan` template remains English in this PR and is part of the separate untranslated-strings PR #308; agent replies remain original content.
