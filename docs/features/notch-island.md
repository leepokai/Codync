# Notch island (desktop)

The desktop app shows working bots in a black shape at the top center of the screen. On a
MacBook it grows out of the notch; on a Mac without a notch, and on Windows and Linux, it hangs
from the top edge. It draws Codync's own bot characters. Design inspired by
[@novra.design](https://www.instagram.com/novra.design/).

Turn it on or off in **Settings → General → System** (*Show in Notch* on macOS, *Show at top
of screen* elsewhere) or the menu bar / tray menu's **Settings**. It is off by default while it
is experimental. The setting belongs to the desktop installation (`showIsland` in `host-prefs.json`).

**Where it sits** (tray menu → Settings → Island position): the display with the notch (else the
main one), the display under the pointer (it moves when the pointer crosses to another screen),
or this computer's main display. The choice is saved as `islandPlacement` beside the switch, so a
laptop with an external monitor can keep it on the screen being used.

## What it shows

Open, the island is 640 pt wide, with a header of tabs beside the notch
(Bots, Message a bot, Usage, New bot; the working count, a usage meter and Open Codync on the
right) over a card. The card holds the bot in focus as a big character with a glow and a state
badge; alerts tint the card from the bottom (amber, green, red). The other bots sit in a column
on the right.

The working badge is Codync's thinking orb. The usage meter and the Usage tab show the tightest
local plan limit and every limit with its reset (amber from 70 %, red from 90 %, like the menu
bar). A bot can be messaged from the island, and ⌥Y / ⌥N answer an approval from any app.

A working character follows the pointer with its eyes: the openings move smoothly (`tanh` of the
distance, so they ease toward the edge of the face instead of snapping) and the whole character
breathes by about 1 %. An idle character is drawn once and never tracks, so a roster full of them
costs no frames; with Reduce Motion on, nothing moves at all.

| State | When | What it shows |
|---|---|---|
| Hidden | No bot works or waits | Invisible over the notch (a thin strip at a plain top edge still wakes it) |
| Peek | The pointer reaches the bare notch | The notch a little wider, the bot in focus peeking out; opens after 650 ms |
| Compact | A bot works or needs you | The notch widened: the bot in focus with its badge on the left, the others (or an orb) on the right |
| Overview | Pointer rest or a click, bots working | The bot in focus with its last three tool calls (newest brightest) and the other bots as pills; each opens its conversation |
| Nothing running | Opened with no bot working | "Nothing running right now." and Message a bot |
| Approval | A permission card is pending | The command, what an edit changes (`+N −M` per file, the changed lines on click) and the card's options, deny left and Allow once right (⌥N / ⌥Y from any app while it shows) |
| Dropping files | A file drag reaches the shape | A dashed outline and "Drop to send to *bot*"; the files are read here and sent to the bot in the big slot (or the one that needs you) |
| Finished | A bot stops working | "*Bot* is done" with its last message, Open conversation / OK, for 5.2 s |
| Failed | A bot stops on an error | "*Bot* stopped." with the error in red, until OK |
| Message a bot | The header's bubble, or the [voice input shortcut](voice-call.md#voice-input-shortcut) | The bot chip (click for the next bot) and a field with the mic (macOS); Enter sends, Esc closes. Until the shortcut is first used, a tip under the field says how to talk from any app |
| Usage | The header's gauge or the usage meter | The providers closest to a limit first, two to a page side by side with arrows (and "1 / 3") between pages: each window's bar, percent and reset, or its note when it has no percent |

Opened by hover, the island folds 0.8 s after the pointer leaves; opened by a click or a tab,
10 s after (a 2 pt line counts down). The message field stays until it sends, Esc or a click in
another app; switching to another tab while it dictates stops dictation and keeps the words. An approval opens by itself and stays until answered; the Bots tab puts it off
until another arrives. Answers and messages go through the chat window (`respondPermission`,
`send`), like the chat's own cards and composer. The states, sizes and motion are drawn in the
design canvas the island was built from.

Every open view is the same size, so switching tabs or an alert taking over never moves the
black shape: the next view cross-fades over the last. When another bot takes the focus it
springs in out of a blur, a bot joining the pills or the column does the same, and the others
slide to their new places.

## How it works

- **Window** (`apps/desktop/src/main/island.ts`): a fixed 720 × 300 transparent, frameless,
  non-focusable window at the top center of the chosen display (see *Where it sits*). macOS:
  a non-activating panel at the main-menu level + 3, so it covers the menu bar, every Space and
  full-screen apps. Windows: a toolbar window (out of Alt+Tab). It never resizes; the shape
  animates inside it with CSS (spring open, plain close).
- **Click-through**: the window takes the mouse only over the shape (+8 pt); the renderer
  reports the shape's target rect on every change. macOS and Windows: the main process polls
  the pointer (60 Hz near the panel, ~7 Hz elsewhere), toggles `setIgnoreMouseEvents` and tells
  the renderer when the pointer enters or leaves. Linux (X11) only reports the pointer over the
  app's own windows, so the window is shaped to the island instead (`setShape`, kept at the old
  outline until a shrink's animation ends) and the renderer's own mouseenter/mouseleave is the
  hover.
- **Notch size**: Electron has no notch API, so `osascript -l JavaScript` reads each
  `NSScreen`'s `safeAreaInsets.top` and `auxiliaryTopLeftArea` / `auxiliaryTopRightArea`; the
  notch is the width between them (a 14" MacBook Pro: 185 × 32 pt). It is read again when
  displays change.
- **Data**: the chat window owns the stores. `useIslandSummary` (next to the tray's summary)
  sends bots and pending permission cards when they change; the island window renders them
  (`views/island/`, state in `IslandModel`, pure rules in `shared/island.ts`). Buttons travel back
  as `WindowCommand`s (`openBot`, `respondPermission`, `sendMessage`, `sendFiles`) to the chat window, which
  calls the host. The island has no host connection of its own. A drop sends the OS paths of the files
  (`webUtils.getPathForFile`); the chat window reads them (at most 20 files, 100 MB each, like the composer) and
  sends them with no message of their own. It takes the keyboard when
  the message field opens (`setFocusable`) and gives it back once the island has folded
  (giving it back takes the window off screen for a moment on macOS); ⌥Y / ⌥N are global
  shortcuts only while an approval shows.

## Platforms and other clients

- **Linux on Wayland**: not available (a window can't place itself at the top or stay above
  others), so the setting is hidden. On X11 the transparent corners need a compositing window
  manager (GNOME, KDE and most desktops have one); without one they show as a solid margin.
- **iPhone**: Live Activities and the Dynamic Island already cover this
  ([notifications and Live Activities](../design/push-and-live-activity.md)); no change.
- **Terminal UI**: a terminal can't draw over other apps; its roster already shows working and
  waiting bots.
