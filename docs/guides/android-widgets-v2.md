# Android widgets v2: the "Codync" overview widget

Branch `feat/android-widgets-v2` (from `integrate/upstream-2.29`). One adaptive widget that gathers what the five older ones show. They
all stay as they are, so widgets already on a home screen keep working; the lock screen one stays apart and never shows bot names.

## What it shows

Sections (`WidgetSection`): **Status** (how many bots need you and how many work), **Attention** (the bots that need you), **Usage**
(the tightest limits), **Team** (four bots as faces), **Recent** (the last bot that moved). Which ones a widget draws is decided by
`overviewPlan` in `core` (`WidgetOverview.kt`), not by the views:

| Size (dp) | Class | Draws |
|---|---|---|
| under 230 wide and high | Small | Status |
| 230+ wide or high | Medium | Status + the first other section with something to say |
| 230×230+ | Large | Status + sections in the chosen order, within a row budget (6 rows; 8 in compact density) |
| 380×230+ | Extra large | Two columns: Status and the first half of the sections, then the rest |

Sections with nothing to show are skipped; a widget never ends up empty (it falls back to Status). Compact density adds rows.
Declared launcher sizes: 110×110, 110×250, 250×110, 250×250 and 400×250 dp; first placement 4×2 cells, `maxResize` 560 dp.

## Settings (per widget)

Placing it opens the settings screen (also from the launcher's reconfigure): sections on or off and in what order, which computer,
which bots (all, or a picked list), theme (system, light, dark) and density (comfortable, compact). Saved per widget id in the
widgets' preferences (`overview:<id>`); a damaged value reads as the defaults. The screen also lists, with today's data, the sections
each size would keep.

## Freshness and trust

`overviewState` says how far to trust the numbers: **Fresh** (under 15 min), **Aging** (15–60 min: shown with "Updated N min"),
**Stale** (over an hour) and **Offline** (the last background read failed). Stale and offline show only the status with dashes and the
words "Old data" or "Offline · updated …": old numbers are never shown as current. The app republishes the report whenever the bots,
the usage or the pending requests change (350 ms after the event), and a periodic worker (15 minutes, battery not low, so battery
saver postpones it) covers the app being closed. A failed read marks the report offline; the next good one clears it.

## Taps and actions

Every zone opens its exact destination: a bot's chat (`codync://bot/<id>`), usage (`codync://usage`), the team and the approvals
(`codync://team`, `codync://approvals`: the bot list, where the requests waiting on you are). Links carry the account and computer
and the app checks both.

From the widget (large sizes): **Message** a bot (a small sheet, typed or dictated through the system recognizer; it goes out through
durable work and a failure becomes a notification that keeps the text), **Deny**, and **Allow** only when safe.

**Safety.** The report holds, for each pending request, only its id, the answering options and a risk, never the command or the file.
`WidgetActions.approval` decides:

- **Deny** is always one tap.
- **Allow** is only for requests marked low risk (`read`, `search`, `fetch`, `think`) and only while the report is under 2 minutes
  old. Anything that changes the computer (`execute`, `edit`, `delete`, `move`) and any kind the widget doesn't know is high risk:
  the button reads **Review** and opens the bot, where the app asks for the fingerprint as always. The callback checks again when
  tapped; what it can't do from the widget opens the app.
- One request is answered once: the work is named after the request, so the first tap wins and the card says "Sending…" until the
  computer answered. Nothing is reported as done before that.

## Picker previews

`providePreview` draws sample data (Android 15+ generated previews, published at launch for every widget); `previewLayout` is a static
layout with sample numbers and no names for older versions; `widgetCategory` is `home_screen`; `targetCell` 4×2.

## Limits and MIUI

MIUI launchers limit cells and may ignore `resizeMode`, `maxResize` and `previewLayout`: the widget then keeps the size it was placed
at and the picker shows the static layout. Widgets can't be placed with `adb` reliably, so the end-to-end check needs the person to
place it. Xiaomi M2010J19CG, Android 12: not yet observed (see below).

## Not done

- iOS, Mac and iPhone widgets: skipped on request. The desktop and terminal apps have no widgets and are not affected.
- Translations: new phrases show in English until added to the shared language tables.

## Gates and checks

| Check | Result |
|---|---|
| `:core:test` (plan by size, freshness, config, safe actions, privacy, links, old reports decode) | passed |
| `assembleDebug testDebugUnitTest :core:test lintDebug` | passed (build successful, 14 min) |
| `:app:assembleRelease` signed with the release key | built and signed, `app-release` → `codync-widgets-release.apk` |
| Install on the phone, place each size, screenshots of data / empty / offline / error | **not done yet**: needs the phone connected and the MIUI prompt accepted |
