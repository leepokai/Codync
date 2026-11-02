# UI conventions

## iPhone navigation

The bot list and conversation use the system `NavigationStack` toolbar. `ToolbarItem` owns the account button, centered computer status/filter menu, and New button (New bot, New group chat). Computer management is inside the centered menu. The conversation copies Grok Bot: the title (the bot's avatar and name) is a plain button that opens its details sheet on Details, a group's editor for a group (VoiceOver: "<name>, view details"); the other bot actions live where the desktop has them (Pin, Make lead agent and Delete in the bot list's touch-and-hold menu, one list with no dividers, and no New session anywhere: sessions start over on their own, Full conversation through a message's Show what it did, Reconnect through pull to refresh and the computer menu), the only trailing button is the computer (remote screen; it pulses while this bot operates the screen), and the call button sits in the composer as a filled waveform capsule while the box is empty. Menus opened from toolbar buttons (New, the computer filter) are native `Menu`s, like every iOS menu; the conversation uses the system back button. iOS supplies Liquid Glass, control sizing, grouping and interaction feedback.

The conversation keeps the native glass buttons over the chat with a soft top scroll-edge effect, so messages blur out under the header instead of running sharp behind it. Root error dialogs use an in-place, full-screen scrim so presenting them does not change the underlying glass controls' appearance.

iPhone conversation scrolling leaves dragging, deceleration, and bottom rubber-band return to UIKit. Passive bottom-following must not move the offset during those gestures, and returning from bottom overscroll does not count as reading history or show Jump to latest. Desktop keeps browser scrolling; the terminal uses line-based scrolling, so neither shares this UIKit-specific behavior.

Do not wrap those toolbar controls in `IconButtonStyle`, hand-sized rounded backgrounds or another glass effect. The account avatar is toolbar content, with the system supplying the enclosing surface. This is the explicit exception to the custom-chrome rule below.

Connecting and reconnecting are background work. `BotStore` gives initial offline reports 1 second to recover and holds drops from online (including relay "computer offline" reports) for 5 seconds. "No access" shows immediately. Automatic read acknowledgements never open an error dialog and visible conversations are acknowledged again after reconnecting. An action taken during a reconnect (back in the foreground, a new network, a drop still inside its grace) waits for the link, up to 20 seconds, instead of failing; while one waits, headers read `Connecting…` and the conversation's connection label shows a spinner. Calls that are safe to repeat (send, stop, approvals, pin, hide, mark as read, delete) are tried again when the link drops under them. Tapping while the header says Offline because the computer can't be reached starts a fresh connection attempt and waits for it, instead of failing until the backoff gets there; a computer the relay reports as off fails at once, since the relay says when it is back. A permission card shows a spinner on the chosen option, dims the others and takes no second answer until the computer has it. The desktop app's `BotStore` (link handling in `apps/desktop/src/renderer/store/bot-sync.ts`) applies the same graces and 20-second wait. The TUI follows the same rule over loopback: a command the host can't be reached for is retried for up to 20 seconds behind its "Reconnecting…" indicator (the `hello` probe that explains an unreachable host still answers at once), and its permission cards spin on the chosen option. The "Something went wrong" dialog is left for a computer that is off, a refused device, the computer's own answers, and reconnects that outlast the wait. Background work (cloud refresh, device registration) logs connection, busy-cloud and stale-token failures instead of showing them. The iOS app disconnects only in `.background`, not `.inactive`. Connection status lives only in headers and their menus. The roster header shows a small summary (`1 connected`, `1/2 connected`, `2 offline`, or `Connecting…`). Its computer menu supports multiple selections, Show only, All computers, reconnect, and management. At least one available computer stays selected; if saved exclusions would hide every computer, the list falls back to all. Filters persist on the device. Bots sort in bands, the [lead bot](../features/lead-bot.md) (starred, never dragged) first, then pinned bots, then the rest; within a band, in the order the user dragged them into, then by recency ([bot order](../features/computer-order.md#bot-order)). Bots the user put in a section sit under a foldable section heading (name, count, and the unread total while folded; folded state saved on the device), with sections and the plain "Bots" group in device-local order after pinned bots (initially sections sorted by name, then Bots). A bot's long-press menu has Move to section (not the lead's); a heading's has Rename section and Remove section; a move or rename the computer refuses is put back. On iPhone, when more than one computer is shown, each computer gets a light heading (badge, name, connection, and "No bots yet" when empty) above its own bots; with one computer the list stays flat. Tapping a heading folds that computer's bots away (chevron; saved on the device). Long-press a heading to drag the computer with its bots up or down: while it is lifted every computer's bots fold into their headings, they unfold on release, and the order is saved on the device (`AccountStore.move`; VoiceOver: Move up / Move down). A bot moves only among its own computer's bots, within its band, and an unpinned bot only inside its section (pinned bots ignore sections). The desktop app's sidebar has the same headings, folding and heading drag (a lifted copy follows the pointer, plus Alt+Up/Down), and the same section headings. There are no connection banners. The compact desktop rail uses a computer icon for the same menu. Conversation headers show a small connection label; the desktop names the computer in its help text, the phones in the bot details' Settings (Workspace → Computer). Pull-to-refresh remains available on iPhone. Computer setup and access requests live under Manage computers; a phone asks an account computer for access only when the user taps Ask for access, never on its own.

Sources: `apps/ios/Views/BotListView.swift`, `AccountSwitcherView.swift`, `apps/ios/Kit/Sources/CodyncUI/Thread/ThreadView.swift`, `Bots/ComputerFilterHeader.swift`, `Store/ComputerSelection.swift`, `Bots/BotRow.swift`; desktop: `apps/desktop/src/renderer/views/ChatWindow.tsx`, `ComputerFilterHeader.tsx`, `BotRow.tsx`.

## Desktop menu bar (tray)

The iPhone App Store update reminder uses a hierarchical `arrow.down.circle.fill`
symbol in the primary text color, keeping the download arrow clear against its
softer circle in both appearances. Desktop updates live in Settings and the native
tray menu; the TUI uses a text action and status message. Neither has this App Store
reminder card, so its decorative icon treatment is iPhone-only.

Release update controls live in the tray's Settings → Updates submenu; the chat
window's Settings has the same controls on its Updates page. One action carries the
whole flow (`apps/desktop/src/shared/update-action.ts`, the same label in the tray, the
Updates page and the update-needed card): **Check for updates** → **Update to <version>**
→ **Downloading… N%** (the Updates page adds a progress bar) → **Restart to update** →
**Restarting…**. A check the person starts that finds nothing opens a **You're up to date**
dialog in the chat window (scheduled checks stay silent). A finished download never quits the app on its own; the person restarts,
or it installs once idle (once no bot is working, when another device chose **Update now** for
this computer). While a release is available, a filled arrow badge sits right
of **Extensions** in the sidebar footer (under the
Extensions icon in the collapsed rail), its tooltip the same action label; one click
restarts into the release, downloading it first if needed (a ring fills with the download
while the app stays usable, then a spinner while it restarts), and it is gone once the app
runs the latest version. The TUI action list has **Check for updates**. Download and
install go through electron-updater (`apps/desktop/src/main/updates.ts`); there is no
separate update window.

The tray menu is Electron's native `Menu` (`apps/desktop/src/main/tray.ts`), the only system menu in the desktop app. Its commands, submenus, checkboxes, usage-icon radio items, separators and keyboard navigation use system styling. Do not add custom cards, hover backgrounds, icon buttons or a window-style menu panel here. Action rows (Open, Pair iPhone, host actions, approval review, Remote screen, Settings, Quit) carry an SF Symbol: on macOS through `nativeImage.createMenuSymbol`, on Linux and Windows as the symbol's Lucide fallback the renderer draws (`TRAY_SYMBOLS`, `tray-summary.ts`). The Network row carries the codync-network mark instead, as in the app (SF Symbols has none): the renderer draws it on every platform, a template image on macOS. Status, bot and usage rows get none. The app version is a row in Settings → Updates, so Settings and Quit end the menu together.

The menu retains host installation/restart, bot conversation/stop actions, approval review, usage limits, remote-screen permissions, open-at-login and quit. Pairing opens a separate titled window because its QR needs a persistent scanning surface. The chat window retains its own UI conventions.

Usage limits appear directly in the top-level menu, grouped by provider with native section headers. Show each window's used percentage (`{percent}% used`), reset time when available, and the provider's last update; do not hide them in submenus. The account menu's
Usage row shows no number; it opens Settings' Usage page with the same per-provider bars.

## Account menu and Settings (desktop)

The chat window's account menu copies Grok Bot's: Usage, Get Codync for mobile, Support, Settings, then the signed-in account and Sign out. Everything else is a page of **Settings**, laid out like ChatGPT's desktop settings (references in `reference/chatgpt-settings-*.png`): a sidebar of pages under small group labels (Personal: General, Voice chat, Usage; This Mac, or This computer on Linux: Computers, Updates) and a large page title over the page's sections. Manage computers and approval badges lead to the Computers page. The phone apps' Settings tab (iPhone and Android), to the right of State in the same bar, holds only what belongs to the phone itself: appearance, language, thinking details, notification permission, usage analytics and Start over. Anything that belongs to one computer lives in Computer management, the sheet Manage computers and links open: one card per computer with its name and status, then Extensions, Voice chat (its keys live on that computer), Screen, Push notifications (whether that computer alerts this phone), Connection and Codync as rows (Connection shows the route's short name, its menu the whole route; Codync shows the version the computer runs, whether it's up to date, and **Update** when a newer release is out or this app needs one), then Hidden bots, and Pair a computer in a card of its own. The desktop's Computers page shows the same Codync row on each SSH computer's card, and on this computer's on Linux (on macOS and Windows this computer's Codync is the app, on the Updates page). Source: `apps/desktop/src/renderer/views/settings/SettingsView.tsx`, account menu in `apps/desktop/src/renderer/views/PanelMenus.tsx`.

## iPhone and desktop code

The iPhone and the desktop no longer share UI code. `CodyncUI` (`apps/ios/Kit/`) is the iPhone app's own package; the desktop app (`apps/desktop/src/renderer/`) is a TypeScript port of the same screens with its own `BotStore`, chat items (`views/thread/chat-items.ts`) and Markdown blocks (`markdown-blocks.ts`).

- The iPhone chat's chrome is in `ThreadView.swift` (`body`, `chrome`, `header`). The iPhone conversation is a UIKit collection view of hosted SwiftUI rows (`ConversationList.swift`): it loads earlier messages from the computer as the reader scrolls up, runs under the bars and the composer (the list sits in the safe area and reaches past it, so rows never get a safe area of their own), follows the newest message until they scroll away (with a round "Jump to latest" button), and pops each new message in ([chat messages](../features/chat-messages.md)).
- The desktop UI rules (custom controls, no system menus except the tray, icon-only buttons, fill or border) are implemented in `apps/desktop/src/renderer/components/` (`Controls.tsx`, `Overlay.tsx`, `Icon.tsx`, `Text.tsx`, `Stack.tsx`); how views use them and the theme tokens: [desktop styles](../guides/desktop-styles.md). Chrome on the sidebar (its header actions, the computer filter, Extensions) is outlined; chrome floating over the conversation (the title pill, the composer) is filled with a shadow, and the composer also carries the floating-layer outline. Corner radii and shadows come from the `--radius-*` and `--shadow-*` scale in `styles/theme.css`. Status text uses `--attention` (`--warning` is for dots and fills only), and destructive fills use `--danger-fill` with `--on-danger`, so both stay readable in light and dark. Sheets and dialogs hold focus while open: the window behind goes inert, Tab cycles inside, and focus returns to where it was on close.
- The Android app (`apps/android/`) ports the iPhone screens one to one in Compose. Where iOS uses system UI (the tab bar, menus, sheets, Liquid Glass), Android draws the same look with its `design` module: glass from Kyant's backdrop library (blur, lens refraction and highlight on RenderEffect/RuntimeShader), the iOS sheet with its grabber and drag-to-close, glass menus and context menus, Inter for SF Pro and Material Symbols for SF Symbols (`Symbol("sf.name")`), and the soft scroll edge under its bars (`ScrollEdge`: the content behind the header, composer and tab bar blurs and washes out). Android has no Liquid Glass of its own; Material widgets aren't used.
- A change to one lands in the other and the TUI in the same change; see [native client parity](#native-client-parity).

## Shared application controls

Other custom iPhone screens use `Chrome.swift` and `Controls.swift`. The desktop equivalents are in `Overlay.tsx` (`Sheet`, `Dialog`, `AnchoredMenu`, `ModalHeader`) and `Controls.tsx` (`IconButton`, `DropdownMenu`, `ChoicePicker`, `ChipChoice`, `Switch`, `CardForm`, `CardSection`, `ValueRow`).

| Need | Component |
|---|---|
| Modal or full-window overlay | `.codyncSheet` (system sheet), `.codyncOverlay` |
| Header outside the native bot navigation flow | `ModalHeader`, `ScreenHeader` |
| Icon action | `IconButton` |
| iPhone app tabs | System `TabView` (Liquid Glass tab bar, hidden inside a chat). Settings is the tab to the right of State. |
| Menu / confirmation | `DropdownMenu` / `.contextActions` (system menus), `.codyncDialog` |
| Toggle | `ToggleStyle.codync` |
| Form-like content | `CardForm`, `CardSection` (a small bold heading over a filled, rounded group; rows split by inset hairlines; a row is `ValueRow` with an optional one-line `detail`, and its picker is an outlined `ChoicePicker`) |

Avoid adding stock `Menu`, `Picker`, `Form`/`List` styling, switch toggles, alerts, `ProgressView` or system sheets to these flows. The iPhone bot list is a `List` with its styling removed (plain, no separators or row backgrounds), only for the system's drag-to-reorder alongside each row's context menu ([bot order](../features/computer-order.md#bot-order)). System presentation APIs inside the shared chrome implementation are implementation details, not permission to bypass the components in feature screens. Native WidgetKit/ActivityKit containers and OS authentication/permission flows remain system integrations.

No scroll bars: scroll indicators never show (`.scrollIndicators(.never)` on the iPhone app's root, so every scroll view inherits it; hidden scrollbars in `apps/desktop/src/renderer/styles/theme.css`).

Anything with a background fill gets no extra drawn border. Use `Palette` and `Motion`; visibility changes animate and honor Reduce Motion. Icon-only actions have an accessibility label and desktop help where applicable. Reserve text for actions an icon cannot clearly express.

## Motion and status

`CharacterAvatar` identifies a bot. `ThinkingOrb` conveys working, searching, listening or connecting and needs adjacent text or an accessible outer label. In-app animation pauses off-screen, when inactive, or with Reduce Motion. Widgets/activities use static frames.

iPhone cold launch: the system launch screen is plain `Palette.background` (`LaunchBackground`), then `LaunchSplash` pops the app-icon bot (squircle, working mood) in while every paired computer connects and finishes its first catch-up (`BotStore.caughtUp`: the `hello` rev reached). It stays at least 0.7 s and at most 1.8 s, then zooms out over the cached list. Without a paired computer (first launch) it is skipped. Android does the same (`LaunchSplash.kt`). The desktop app and TUI reach their host over loopback at once, so they have no splash; while the desktop's own host is still starting, its "Starting the host…" screen (`HostStateView`, full and compact in `StartupRecovery`) shows the same bot popping in and bobbing above the text, still with Reduce Motion.

On iPhone, provider identity and activity presentation are shared with widgets in `CodyncKit/Design`; use those components rather than re-creating mappings per target. The desktop draws them in `components/AgentIcon.tsx`, `Avatar.tsx` and `ThinkingOrb.tsx`. See [mobile widgets](mobile-widgets.md) for current rendering and previews.

The [2026-09-25 audit](../archive/ui-audit-2026-09-25.md) is historical. Its old line numbers and recommendations are not the current UI policy or a list of confirmed open bugs.

Model discovery keeps loading and refresh in one fixed-size slot beside the Model picker. Refresh retains the current catalog while loading, and loading does not add a separate row to the settings card. The Model label keeps its intrinsic width; the model picker takes the remaining width and truncates long names, with the full name available in its tooltip and menu. Fast mode is a separate switch row below it (see [model selection](../features/marketplace.md#fast-mode)).

## Desktop conversation details

Desktop conversations start with the details inspector closed; the title menu opens it on demand. Keep the transcript and composer aligned in one reading column, 820 points wide and scaled with the text size (`--column`). The inspector is resized by dragging its edge or with ←/→ while the edge has focus, like the sidebar (`ResizeHandle`); it keeps its width, at least 260 points, leaving the conversation at least 380. The roster selection, composer and outgoing messages have separate surface tokens so selection and input do not compete with the conversation. Roster names use body text and previews use callout text; previews show the first nonempty line without common Markdown heading, quote, bold or code delimiters, matching the terminal roster.

Preserve the original character avatar design: dotted shading, highlight-only color, silhouettes and animation. Desktop roster avatars remain 30 points and iPhone roster avatars remain 46 points. iPhone shares readable secondary text and message previews; its native navigation remains platform-specific. Terminal avatars already use solid color and the terminal has no persistent details inspector; its secondary text uses the same readable palette.

The details inspector (`apps/desktop/src/renderer/views/thread/DetailsPanel.tsx`) copies Grok Bot's: share (Create template) and close (X) as round buttons in the corner, in the page color with a soft ring, over a 54-point band that drags the window; the bot's 60-point avatar, name and runtime centered on top; then page tabs (`TabChoice`: plain labels, the chosen one on a light capsule) for **Details**, **Library** (the chat's [artifacts and files](../features/artifacts.md)), **Memory** and **Settings**, all in one scroll. The chat's `<<` button (in the same ringed style) and the title pill open it; hovering the pill shows a `>` after the name. Details holds what changes often: Instructions, Agent (the agent with its logo, its model, fast mode and its [fallbacks](../features/agent-fallback.md#apps)), the Use the computer switch and Routines (one card per routine); Settings holds the rest: Profile (avatar row, name, and "Created by" for a bot another bot created), Workspace (computer, workspace, permissions), Activity, Connectors and Skills. Both are parts of `BotSettingsForm` (`part`), saved as you edit; the editor sheet shows all of it. Every bot settings form (these, the editor sheets, the terminal's form) keeps a change made elsewhere while it is open (a bot that manages the team, another device): it takes the new value in each field not edited there, or the terminal sends only what it changed. Memory is the read-only memory card, its sections in the same tabs; each layer's note ends with "To change it, tell the bot." and the Dreams tab has an info button. Every group in the panel is Grok Bot's outlined card (an outline on the page, no fill, inputs filled instead), a row with a note puts its control under the note, and long notes (skill descriptions, explanations, a note's description) stop at two lines with an ellipsis. A group shows its avatar, name, bot count and members instead of tabs. Use 16-point horizontal insets and 28-point section gaps; sections use the `CardSection` look: filled 16-point groups without borders, hairline-split rows, a note under each option. The iPhone's Details sheet (`BotDetailsView`, from tapping the chat's title, a routine notice or a failure's Add a fallback) has the same corner buttons, header, tabs, pages, outlined cards (`cardStyle`) and two-line notes (`noteLines`), and saves as you edit too. Android's (`BotDetailsView.kt`) ports it, without the Memory tab until the read-only memory screen is ported ([Android plan](../plans/android-app.md)).

Routines are a compact grouped list: name, a one-line schedule (or the live run state), and an on/off switch per row; tapping a row opens the same form as +, filled in, with delete and test run beside Save. On the desktop the form card fits its content. Requests to the bot are typed directly in chat; + opens the form, where When to run is a type choice (a segmented control on the desktop, the system menu on iPhone) and a schedule is a Repeat choice (Hourly, Daily, Weekdays, Weekly, Custom) with only the fields that choice needs (minute, time, weekday, or a typed cron for Custom) and a time zone picked from a searchable list. On the desktop these sit in one `CardSection` of labeled rows, Repeat and the weekday as `ChipChoice` chips with the chosen one on a light capsule. The empty state is one line of text. A saved webhook routine's form shows the public URL and key as monospaced rows (in the same group on the desktop) with copy, show/hide and replace-key (confirmed) icon buttons, and one caption on how to send and that deliveries pass through the Codync cloud. Agent metadata uses Runtime and Workspace labels for personal bots, or Project folder for explicitly configured projects; project paths remain selectable and wrap. Personal workspace paths stay out of the default details UI. Keep status and section headings readable in both appearances through `Palette`.

On iPhone, the routine editor keeps the full-width save action in a fixed footer on the sheet background. When required fields are empty, the footer explains why saving is unavailable.

New bot and New group chat (the + menu on every client) are one short sheet each, in the same layout: a large face on top (the avatar, or the picked members' group avatar), a centered optional name field, then the choices as wrapping centered grids (the avatar's shapes and colors; the computer's bots, each a face with its name under it), ringed when picked. The desktop puts Cancel and Create in a footer; the phones put Create in the sheet header. Everything else starts on the defaults and changes afterwards in Settings or the group editor; an unnamed bot is named by the host after its first conversations, an unnamed group after its bots. With more than one computer, the sheet also picks which one.

New bots use automatically allocated personal workspaces. Settings offer Personal or an optional project folder; see [bot workspaces](../features/bot-workspaces.md).

Desktop menus (`AnchoredMenu` in `Overlay.tsx`) scroll vertically when their content exceeds 420 points or the available space beside the trigger. Menus open toward the side with more vertical room; short lists keep their content height. Menu width is capped at 320 points and constrained to the window, and long option titles remain available through help text. This applies to Agent, Model, and other choice menus on the desktop. On iOS every menu is the system one: `DropdownMenu` and `ChoicePicker` present the native `Menu` (choices as checkmarked toggles) and `.contextActions` the native `contextMenu` (reactions as a palette row), because an overlay anchored by global frame lands in the wrong place inside `.codyncSheet`, scroll views and the composer (the + menu opened mid-screen on iPhone, 2026-09-30). Each computer has its own marketplace; its header names the computer and, with several online, switches between them. With more than one computer that can take a bot, the New bot sheet on iPhone has a Computer dropdown listing them (every computer in the current account that doesn't need an update, including computers hidden by the roster filter and offline computers). Offline choices stay selectable and are labeled; creation waits until the selected computer is online.

## Reading conversations

A visible conversation registers its own read scope while its scene is active. New chat-visible entries (including final updates to an existing message ID) and later roster unread updates acknowledge that scope through the host. Opening the view, returning to the foreground, and reconnecting also acknowledge it. Never gate a receipt on the cached unread count: entry and roster events can arrive separately. Leaving a view or backgrounding the scene unregisters it. The host remains authoritative and broadcasts the resulting unread state to all clients.

The iPhone conversation navigation title includes a compact second line with the connection label and the current connection route icon: cloud for Cloudflare, Wi-Fi for direct Wi-Fi/Tailscale. It follows the live route, not the preferred route setting; offline and reconnecting states replace the route icon. VoiceOver also reads the connection description.


## Native client parity

UI changes apply to iOS SwiftUI (`apps/ios/`, `apps/ios/Kit/Sources/CodyncUI/`), the
desktop app (`apps/desktop/src/renderer/`) and the terminal client (`host/src/tui/`)
together. Use each platform's controls while preserving the same actions, information
and states.

- A chat shows the latest non-final agent segment only while its turn is
  working and connected. Earlier narration, tools, empty text and `(pass)` stay out
  of the message list: the running turn's thinking, narration, tool calls (not the
  bot's bookkeeping) and plan appear only in the working pill's unfolded steps, with
  Show thinking details on (Settings › Chat;
  [while it works](../features/chat-messages.md#while-it-works)), and every turn's in
  Full conversation. Completed replies remain visible.
- TUI keeps text and attachments in the composer until sending succeeds. A failed
  send can be retried with Enter; it reuses the nonce. A late response must not erase
  a draft edited while the request was running.
- Connection requests in the chat are cards shaped like an Extensions row (logo, name,
  reason, one action on the right; at most 400 pt wide), with no dismiss button: a
  request just stays until it's done. TUI **C** opens a pending connection request in
  the current lane; selecting a request and pressing Enter opens that request.
  Credentials, connector installation/sign-in and hosted apps use the secure setup
  APIs; Escape closes the editor. Secrets are masked and never sent as chat text. An
  agent sign-in request opens that agent's sign-in sheet instead, which always checks
  the sign-in again.
- A bot-to-bot exchange is a compact centered row ("Messaged [avatar] Owen" / "Message from
  [avatar] Egan"; same verbs for asks; the TUI draws one ` · ` line), never a bubble. Only a
  failure is marked: danger color, a warning mark (triangle icon; the TUI uses a
  glyph), "failed" in the accessibility label. The row
  opens a read-only "A ⇄ B" sheet. In the main chat consecutive exchanges with the same peer
  (either direction; trace entries don't break the run, any other visible entry does) are one
  row, "{N} messages with [avatar] Owen", failed if any failed, keyed by the first exchange
  (TUI: one pick, `c` on it says it is read-only). The sheet matches "Full conversation" (same size;
  `ModalHeader` with the pair, own bot first, as its left title and the close X on the right):
  the pair's whole history as author-labelled grey bubbles (avatar and name above each run),
  time separators after gaps over an hour, "Waiting for {name}…" or the failure detail under
  a pending or failed exchange, no composer. TUI:
  `v`/`r` picks a message, j/k moves to the row, ↵ (or a click) opens it, j/k ↑↓ and
  ^u/^d scroll, esc closes; `r`, `f` and `1…6` on the row say it is read-only. The data contract is
  [bot collaboration](../features/bot-collaboration.md#notice-data).
- TUI bot settings include **Use computer** and **Manage team**, matching the iPhone and desktop apps. The
  remote-screen viewer and voice-call UI are not in the TUI.
- A bot's pages in the TUI match the inspector's tabs: `R` opens Details (the agent, model and first line of the instructions, which `i` edits in
  Settings; Use the computer, which `u` switches; then Routines), `L` Library, `M` Memory and `e` Settings, each with the
  Details / Library / Memory / Settings line on top; ⇧← ⇧→ moves between them. Settings with unsaved edits asks for ^S (or Esc) first.

### Verification

Run Rust format, Clippy and tests for `host/`, `npm run typecheck && npm test` in
`apps/desktop/`, and `xcodebuild test -scheme CodyncKit-Package` on an iOS simulator
from `apps/ios/Kit/`. Desktop UI checks use the `CODYNC_DEBUG_OPEN` and
`CODYNC_REMOTE_DEBUG` switches described in the
[desktop app doc](../architecture/desktop-app.md#development). Host tests cover TUI
draft recovery, lane-specific streaming, computer settings and complete
connection-request API flows. These checks do not authenticate real third-party accounts.

### Desktop app connections footer

The expanded desktop roster ends in one floating row: the account avatar and, with one
computer, a “Extensions” capsule with example service logos. A masked, layered blur softens
roster content behind it; bottom scroll padding keeps the last bot reachable. The
compact rail exposes the same action by tooltip and accessible name (the selected bot's
computer, or this one). Extensions belongs to one computer and has no computer switcher:
with several computers the capsule goes away and each computer heading carries its own
Extensions button (`square.grid.2x2`, always shown, while that computer is reachable).
The first time the roster shows several computers, a tip card at its top says Extensions
moved to each computer (`ExtensionsMovedTip.tsx`); dismissing it, or opening Extensions from
a heading, hides it for good on that device. Logos represent examples, not connected status.

This is a desktop sidebar treatment across macOS, Linux and Windows. iOS keeps its
native navigation and per-computer Extensions entries (each computer's card in Computer management, and
each computer heading in the Bots list with several computers); the TUI
keeps its keyboard Extensions action and connector tabs. Neither has this persistent
desktop footer or supports the same layered backdrop material; no capabilities or
connection permissions change.

### iOS jump to latest

A single display-linked motion owns the collection view offset during a requested
jump. Reserve that ownership before updating following, snapshots or keyboard
insets; those updates must not pin the list while the animation runs. Retarget
the current motion as self-sizing rows change the end, without offscreen sizing
jumps or starting overlapping insertion animations. Dragging and scroll-to-top
cancel the jump. Reduce Motion pins directly, and passive following still holds
the newest message at the bottom. Original scroll-edge blur remains unchanged.

This repair targets UIKit's estimated row sizing and collection layout. Desktop
uses DOM scroll geometry and the browser's smooth scrolling rather than the iOS
pre-sizing loop; the terminal jumps by line index without animation. Their current
follow/jump actions remain unchanged.

### Desktop Grok Bot visual reference

Per the requested desktop reference, use the system sans font with 13-point,
medium-weight roster names (semibold for unread), 12-point previews and chat text
at the selected text size, without the former extra two-point offset. The default
sidebar is 266 points; existing custom widths and font preferences are respected.
Reference surfaces: dark canvas #070707, sidebar #111111, capsule #181818, composer
#2f2f2f. Search/new buttons, the title pill, Extensions and composer use subtle
one-point outlines. **These reference-matched desktop controls are an explicit
exception to the filled-controls-without-borders rule.** Retain visible keyboard
focus, computer filters, connection/error states, and original Bot artwork.

The footer uses bundled Gmail, Google Calendar and Google Drive product icons
from Google's gstatic branding assets as examples of available integrations,
never as a connected-status indicator. The iOS native controls and terminal
layout remain platform-specific; this desktop styling applies to the common
Electron implementation on macOS, Linux and Windows.

### Apps and credential settings

Desktop Apps keeps Credentials in the same modal navigation stack, with a back
button and a persistent top-level entry. Settings content is constrained to 620px,
with flat sections and compact, explicitly labeled credential fields. Input edges
follow the desktop control-outline exception; there are no nested filled cards.
The existing credential APIs, secret masking, and actions are unchanged.

iOS keeps its native sheet presentation and touch-sized rows, while sharing field
labels and the saved-credentials empty state. Terminal connection forms already
show persistent labels; the TUI has no credential-manager page or modal-card layout,
so this desktop presentation change does not affect its keyboard workflow.
Desktop styling is shared across macOS, Linux and Windows.

Desktop modal sheets, confirmation dialogs and menus (dropdown and right-click
menus, the account and action panels) and the message composer use one subtle 1px
inner outline (`--outline`), including Apps, Credentials, and Settings. The outline
follows the existing corner radius and overlays the content without changing layout
or intercepting input. This is an explicit exception to the filled-surface border
rule. iOS keeps the system sheet's, system menu's and composer glass's own edge,
Android its `design` module glass edge; terminal dialogs retain their existing
character borders. None of them receives an extra desktop-style outline.

Desktop sheets and dialogs sit a step above the page (`--sheet`, raised in dark
mode, over a deeper scrim); inside them grouped rows and fields are quiet tints of
the text color, and a sheet's `ModalHeader` leads with an icon tile (a symbol, or
the item's logo for an agent) beside the title and its line. Forms fit their content
instead of a fixed height. A confirmation dialog is left-aligned with its answers in
a row on the right, Cancel first. These are desktop treatments: iOS keeps the system
sheet and `.codyncDialog`, and the terminal keeps its character dialogs.

The desktop sidebar computer filter shares the toolbar's subtle outline and 32px
height, with a capsule enclosing both the computer glyph and its chevron. This
applies in expanded and compact sidebars. iOS retains native toolbar treatment;
terminal computer selection has no corresponding icon button.

Dialog scrims cover the entire presentation surface, including the iPhone status-bar and home-indicator safe areas, throughout opening and closing. The shared iPhone overlay fades a full-screen canvas without scaling its backdrop; root in-place dialogs also extend the canvas beyond container safe areas. Desktop dialogs keep their viewport-sized backdrop fixed and scale only the card. Terminal dialogs already dim the complete terminal buffer.

## Appearance preference

Desktop General → Appearance and phone settings → Appearance offer **System**,
**Light** and **Dark**, defaulting to System. Save the choice on the device,
independently of accounts and hosts, and apply it immediately and on relaunch.
Desktop uses Electron's native theme source so native chrome, CSS and canvas
artwork agree across all windows. iPhone sets the window's interface style so
already-presented sheets update and System clears the override;
Android supplies the selected theme and matching system-bar contrast. Remote
screen viewing and explicitly dark previews keep their existing dark surfaces.

The terminal's `Ctrl+K` actions offer the same three choices, saved in the local
`tui-appearance` file. System retains the terminal's foreground/background and
uses `COLORFGBG` for accent contrast. `CODYNC_THEME` supplies the initial choice
when none is saved; `NO_COLOR` continues to disable color output.

Roster section headings and the Bots label move whole groups within a computer: drag on desktop, touch and hold then drag on phones. Section order is per computer and per device. Pinned bots remain above all groups. Terminal Alt+Up/Down moves the selected bot's group for that run.
