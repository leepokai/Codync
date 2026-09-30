# UI conventions

## iPhone navigation

The bot list and conversation use the system `NavigationStack` toolbar. `ToolbarItem` owns the account button, centered computer status/filter menu, and new-chat button. Computer management is inside the centered menu. The conversation copies Grok Bot: the title pill is a native `Menu` (details and the bot's actions), the only trailing button is the computer (remote screen; it pulses while this bot operates the screen), and the call button sits in the composer as a filled waveform capsule while the box is empty. Menus opened from toolbar buttons (New, the computer filter, the title) are native `Menu`s, because the bar hosts its items outside SwiftUI's layout and `.codyncMenu` can't anchor to them; the conversation uses the system back button. iOS supplies Liquid Glass, control sizing, grouping and interaction feedback.

The conversation hides the navigation bar background and the top scroll-edge effect, keeping the native glass buttons over the chat. Root error dialogs use an in-place, full-screen scrim so presenting them does not change the underlying glass controls' appearance.

Do not wrap those toolbar controls in `IconButtonStyle`, hand-sized rounded backgrounds or another glass effect. The account avatar is toolbar content, with the system supplying the enclosing surface. This is the explicit exception to the custom-chrome rule below.

Connecting and reconnecting are background work. `BotStore` gives initial offline reports 1 second to recover and holds drops from online (including relay "computer offline" reports) for 5 seconds. "No access" shows immediately. Automatic read acknowledgements never open an error dialog and visible conversations are acknowledged again after reconnecting. The iOS app disconnects only in `.background`, not `.inactive`. Connection status lives only in headers and their menus. The roster header shows a small summary (`1 connected`, `1/2 connected`, `2 offline`, or `Connecting…`). Its computer menu supports multiple selections, Show only, All computers, reconnect, and management. At least one available computer stays selected; if saved exclusions would hide every computer, the list falls back to all. Filters persist on the device. Bots sort pinned first, then by recency. On iPhone, when more than one computer is shown, each computer gets a light heading (badge, name, connection, and "No bots yet" when empty) above its own bots; with one computer the list stays flat. Long-press a heading and drag it onto another computer's section to reorder computers (VoiceOver: Move up / Move down); the order is saved on the device (`AccountStore.move`). The Mac shows a flat roster. There are no connection banners. The compact Mac rail uses a computer icon for the same menu. Conversation headers show a small connection label, with the computer name available in the header menu or help text. Pull-to-refresh remains available on iPhone. Computer setup and access requests live under Manage computers.

Sources: `apps/ios/Views/BotListView.swift`, `AccountSwitcherView.swift`, `kit/Sources/CodyncUI/Thread/ThreadView.swift`, `Bots/ComputerFilterHeader.swift`, `Store/ComputerSelection.swift`, `Bots/BotRow.swift`.

## macOS menu bar

Release update controls live in the native Settings → Updates menu. Sparkle's
standard update/install windows are a native system-integration exception; keep
signature errors, progress and relaunch in its supported user driver.

Use `MenuBarExtra` with `.menuBarExtraStyle(.menu)`. Its commands, submenus, checkmarked toggles, usage-icon picker, separators and keyboard navigation use system styling. Do not add custom cards, hover backgrounds, icon buttons or a window-style menu panel here.

The menu retains host installation/restart, bot conversation/stop actions, approval review, usage limits, remote-screen permissions, launch-at-login and quit. Pairing opens a separate titled window because its QR needs a persistent scanning surface. The chat window retains its own UI conventions.

Usage limits appear directly in the top-level menu, grouped by provider with native section headers. Show each window's percentage, reset time when available, and the provider's last update; do not hide them in submenus.

Source: `apps/macos/App/CodyncMacApp.swift`.

## Shared application controls

Other custom screens use `Chrome.swift` and `Controls.swift`:

| Need | Component |
|---|---|
| Modal or full-window overlay | `.codyncSheet`, `.codyncOverlay` |
| Header outside the native bot navigation flow | `ModalHeader`, `ScreenHeader` |
| Icon action | `IconButton` |
| Tab selection | `TabBar` |
| Menu / confirmation | `.codyncMenu` (macOS), `DropdownMenu` / `.contextActions` (system menus on iOS), `.codyncDialog` |
| Toggle | `ToggleStyle.codync` |
| Form-like content | `CardForm`, `CardSection` |

Avoid adding stock `Menu`, `Picker`, `Form`/`List` styling, switch toggles, alerts, `ProgressView`, `TabView` or system sheets to these flows. System presentation APIs inside the shared chrome implementation are implementation details, not permission to bypass the components in feature screens. Native WidgetKit/ActivityKit containers and OS authentication/permission flows remain system integrations.

Anything with a background fill gets no extra drawn border. Use `Palette`, `InterfaceMetrics` and `Motion`; visibility changes animate and honor Reduce Motion. Icon-only actions have an accessibility label and desktop help where applicable. Reserve text for actions an icon cannot clearly express.

## Motion and status

`CharacterAvatar` identifies a bot. `ThinkingOrb` conveys working, searching, listening or connecting and needs adjacent text or an accessible outer label. In-app animation pauses off-screen, when inactive, or with Reduce Motion. Widgets/activities use static frames.

Provider identity and activity presentation are shared in `CodyncKit/Design`; use those components rather than re-creating mappings per app. See [mobile widgets](mobile-widgets.md) for current rendering and previews.

The [2026-09-25 audit](../archive/ui-audit-2026-09-25.md) is historical. Its old line numbers and recommendations are not the current UI policy or a list of confirmed open bugs.

Model discovery keeps loading and refresh in one fixed-size slot beside the Model picker. Refresh retains the current catalog while loading, and loading does not add a separate row to the settings card. The Model label keeps its intrinsic width; the model picker takes the remaining width and truncates long names, with the full name available in its tooltip and menu.

## macOS conversation details

The details inspector uses a compact device summary instead of an empty screen preview. Keep the computer name, remote-screen state, and iPhone hint together; show the hint only when screen capture is ready. Use 16-point horizontal insets, 28-point section gaps, and 12-point corner radii on flat surfaces, without borders.

Routines use a short empty state with one setup action and an icon action for asking in chat. Populated lists move the add action into the section header. Agent metadata uses Runtime and Workspace labels for personal bots, or Project folder for explicitly configured projects; project paths remain selectable and wrap. Personal workspace paths stay out of the default details UI. Keep status and section headings readable in both appearances through `Palette`.

On iPhone, the routine editor uses a single column of trigger choices so titles and descriptions can wrap at larger text sizes. Keep the execution environment note in the scrollable form and the full-width save action in a fixed footer on the sheet background. When required fields are empty, the footer explains why saving is unavailable.

New bots use automatically allocated personal workspaces. Settings offer Personal or an optional project folder; see [bot workspaces](../features/bot-workspaces.md).

Shared `.codyncMenu` lists scroll vertically when their content exceeds 420 points or the available space beside the trigger. Menus open toward the side with more vertical room; short lists keep their content height. Menu width is capped at 320 points and constrained to the window, and long option titles remain available through help text. This applies to Agent, Model, and other shared choice menus on macOS. On iOS every menu is the system one: `DropdownMenu` and `ChoicePicker` present the native `Menu` (choices as checkmarked toggles) and `.contextActions` the native `contextMenu` (reactions as a palette row), because an overlay anchored by global frame lands in the wrong place inside `.codyncSheet`, scroll views and the composer (the + menu opened mid-screen on iPhone, 2026-09-30). `.codyncMenu` is macOS-only and does not compile on iOS. Each computer has its own marketplace; its header names the computer and, with several online, switches between them. A new bot's editor on iPhone has a Computer dropdown listing every computer in the current account, including computers hidden by the roster filter and offline computers. It remains a dropdown with one computer. Offline choices stay selectable and are labeled; creation waits until the selected computer is online. Switching computers resets the folder, model and connectors that belonged to the old computer.

## Reading conversations

A visible conversation registers its own read scope while its scene is active. New chat-visible entries (including final updates to an existing message ID) and later roster unread updates acknowledge that scope through the host. Opening the view, returning to the foreground, and reconnecting also acknowledge it. Never gate a receipt on the cached unread count: entry and roster events can arrive separately. Replies acknowledge only their own thread; leaving a view or backgrounding the scene unregisters it. The host remains authoritative and broadcasts the resulting unread state to all clients.

The iPhone conversation navigation title includes a compact second line with the connection label and the current connection route icon: cloud for Cloudflare, Wi-Fi for direct Wi-Fi/Tailscale. It follows the live route, not the preferred route setting; offline and reconnecting states replace the route icon. VoiceOver also reads the connection description.


## Native client parity

UI changes apply to SwiftUI (iOS/macOS), Linux GTK and the terminal client together.
Use each platform's controls while preserving the same actions, information and states.
The October 2026 parity changes bring GTK and TUI up to the existing SwiftUI behavior;
SwiftUI already supplies these actions and needs no duplicate implementation.

- Chat and threads show the latest non-final agent segment only while that lane is
  working and connected. Earlier narration, tools, empty text and `(pass)` stay out
  of chat. Completed replies remain visible.
- GTK's **Load earlier messages** requests older history without evicting loaded pages.
  Failed messages retain their attachments and nonce for **Resend**, and can be deleted.
  Reactions work in chats and threads. Attachments save to Downloads without replacing
  an existing file.
- TUI keeps text and attachments in the composer until sending succeeds. A failed
  send can be retried with Enter; it reuses the nonce. A late response must not erase
  a draft edited while the request was running.
- GTK bot menus include **Memory** and **Routines**. Memory supports forgetting one or
  all facts. Routines support creation, editing, schedule previews, pause/resume, test
  runs, deletion, history and webhook credentials. The host validates schedules;
  editing can preserve existing event, interval and multiple triggers.
- GTK **Marketplace** includes agents, connectors and skills. Agent setup supports
  install/login terminals, browser authentication and masked credential fields.
  VTE's terminal sends ordered input, receives output, resizes with its view and closes
  the host terminal when dismissed. Ctrl+click opens HTTP(S) links.
- TUI **C** opens a pending connection request in the current lane; selecting a request
  and pressing Enter opens that request. Credentials, connector installation/sign-in
  and hosted apps use the secure setup APIs. Ctrl+X cancels the request; Escape closes
  the editor without cancelling. Secrets are masked and never sent as chat text.
- TUI bot settings include **Use computer**, matching SwiftUI and GTK. The separate
  remote-screen viewer and voice-call UI remain phone-specific capabilities.

### Verification

Run Rust format, Clippy and tests for `host/` and `apps/linux/`. Linux needs
`libgtk-4-dev`, `libadwaita-1-dev` and `libvte-2.91-gtk4-dev` (VTE 0.70+).

The native GTK test runs real widgets and HTTP/SSE calls against an isolated fixture:

```sh
cd apps/linux
GTK_A11Y=none CODYNC_UI_ARTIFACTS=/tmp/codync-ui \
  xvfb-run -a dbus-run-session -- python3 tests/ui_fixture.py
```

Install `xvfb`, `dbus-x11` and `imagemagick` for that command. It exercises history,
streaming, retry/discard, reactions, attachment downloads, memory, routines, skill
installation, masked agent credentials and terminal cleanup. Downloads and tokens stay
inside its temporary directory. CI runs it on both Linux architectures and uploads
screenshots as `native-ui-linux-*` artifacts. Host tests cover TUI draft recovery,
lane-specific streaming, computer settings and complete connection-request API flows.
These fixtures do not authenticate real third-party accounts.
