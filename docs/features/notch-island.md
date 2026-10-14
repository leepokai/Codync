# Notch island (desktop)

The desktop app shows working bots in a black shape at the top center of the screen. On a
MacBook it grows out of the notch; on a Mac without a notch, and on Windows and Linux, it hangs
from the top edge. It draws Codync's own bot characters. Design inspired by
[@novra.design](https://www.instagram.com/novra.design/).

Turn it on or off in **Settings → General → System** (*Show in notch* on macOS, *Show at top
of screen* elsewhere) or the menu bar / tray menu's **Settings**. It is off by default while it
is experimental. The setting belongs to the desktop installation (`showIsland` in `host-prefs.json`).

## What it shows

Open, the island is 640 pt wide, with a header of tabs beside the notch
(Bots, Message a bot, Usage, New bot; the working count, a usage meter and Open Codync on the
right) over a card. The card holds the bot in focus as a big character with a glow and a state
badge; alerts tint the card from the bottom (amber, green, red). The other bots sit in a column
on the right.

The working badge is Codync's thinking orb. The usage meter and the Usage tab show the tightest
local plan limit and every limit with its reset (amber from 70 %, red from 90 %, like the menu
bar). A bot can be messaged from the island, and ⌥Y / ⌥N answer an approval from any app.

| State | When | What it shows |
|---|---|---|
| Hidden | No bot works or waits | Invisible over the notch (a thin strip at a plain top edge still wakes it) |
| Peek | The pointer reaches the bare notch | The notch a little wider, the bot in focus peeking out; opens after 650 ms |
| Compact | A bot works or needs you | The notch widened: the bot in focus with its badge on the left, the others (or an orb) on the right |
| Overview | Pointer rest or a click, bots working | The bot in focus with its last three tool calls (newest brightest) and the other bots as pills; each opens its conversation |
| Nothing running | Opened with no bot working | "Nothing running right now." and Message a bot |
| Approval | A permission card is pending | The command and the card's options, deny left and Allow once right (⌥N / ⌥Y from any app while it shows) |
| Finished | A bot stops working | "*Bot* is done" with its last message, Open conversation / OK, for 5.2 s |
| Failed | A bot stops on an error | "*Bot* stopped." with the error in red, until OK |
| Message a bot | The header's bubble | The bot chip (click for the next bot) and a field; Enter sends, Esc closes |
| Usage | The header's gauge or the usage meter | Claude and Codex limits side by side: each window's bar, percent and reset |

Opened by hover, the island folds 0.8 s after the pointer leaves; opened by a click or a tab,
10 s after (a 2 pt line counts down). The message field stays until it sends, Esc or a click in
another app. An approval opens by itself and stays until answered; the Bots tab puts it off
until another arrives. Answers and messages go through the chat window (`respondPermission`,
`send`), like the chat's own cards and composer. The states, sizes and motion are drawn in the
design canvas the island was built from.

Every open view is the same size, so switching tabs or an alert taking over never moves the
black shape: the next view cross-fades over the last. When another bot takes the focus it
springs in out of a blur, a bot joining the pills or the column does the same, and the others
slide to their new places.

## How it works

- **Window** (`apps/desktop/src/main/island.ts`): a fixed 720 × 300 transparent, frameless,
  non-focusable window at the top center of the notch's display (else the main display). macOS:
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
  as `WindowCommand`s (`openBot`, `respondPermission`, `sendMessage`) to the chat window, which
  calls the host. The island has no host connection of its own. It takes the keyboard when
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
