# UI audit — 2026-09-25

Checked against the `apple-design` skill (emilkowalski/skills), plus the iOS/macOS HIG and GNOME HIG. Paths use the new layout. Line numbers are from the commit that did the restructure.

## Shared (kit/)

**High**
- `kit/Sources/CodyncKit/Design/Theme.swift:49`: `Palette.tertiary` is too low-contrast. Light `0x9B9B9B` on white is about 2.7:1; dark `0x6E6E6E` is about 3.9:1. It's used for timestamps, "Sending…" and step numbers. Fix: light `0x767676`, dark `0x8C8C8C`.
- `Theme.swift:57`: `Palette.warning` `0xF0A030` as text on white is about 2.1:1, and it's used for "Needs your approval". Fix: `Color(light: 0xB25E00, dark: 0xF0A030)`. Keep the bright amber for fills only.
- `Theme.swift:28-36`: `Color(light:dark:)` ignores Increase Contrast. Fix: branch on `accessibilityContrast == .high` (iOS) and `.accessibilityHighContrastAqua` (macOS) for border, secondary and tertiary, and add a 1pt stroke on bubbles.
- There are no haptics anywhere. Fix: add `.sensoryFeedback` at these points only:
  - send: `.impact(weight: .light)`
  - allow / deny: `.success` / `.warning`
  - QR scanned: `.success`
  - stop: `.impact(weight: .medium)`
  - save error: `.error`
- `kit/Sources/CodyncUI/Thread/ThreadView.swift:55-58`: every new message scrolls to the bottom, even while the user is reading older messages. Fix: track "at bottom" with `onScrollGeometryChange` and auto-scroll only when true; otherwise show a "↓ New message" pill.

**Medium**
- Some hit targets are under 44pt:
  - composer send/stop, 34pt: `ThreadView.swift:197,208`
  - resend/delete on a failed message: `Thread/ChatRows.swift:38-39`
  - Retry: `Bots/BotRow.swift:96`
  - Marketplace "Add": `Marketplace/MarketplaceView.swift:399-405`
  - color swatches: `Bots/BotEditorView.swift:401`

  Fix: keep the visuals and add `.frame(minWidth: 44, minHeight: 44).contentShape(Rectangle())`.
- Some controls use `onTapGesture` instead of `Button`, so they have no pressed state, no VoiceOver button trait and no keyboard focus:
  - thread header pill: `ThreadView.swift:150`
  - computer row: `apps/ios/Views/SettingsView.swift:21`
  - avatar shape/color pickers: `BotEditorView.swift:389,404`
  - macOS menu bot row: `apps/macos/App/CodyncMacApp.swift:142`

  Fix: make each a `Button` with `.buttonStyle(PressScale())`.
- `ThreadView.swift:170`: the `ellipsis.circle` menu has no accessibility label. Fix: `Label("More", systemImage: "ellipsis").labelStyle(.iconOnly)`.
- `BotEditorView.swift:197`: the save error shows at the bottom of a long form, far from the Save button. Save is also disabled with no explanation when no folder is set (`:347`). Fix: show the error at the top or in an `.alert`, and add an inline hint under "Project folder".
- `BotEditorView.swift:206`: on macOS the folder picker is the custom remote `FolderPicker`. Fix: use `.fileImporter(allowedContentTypes: [.folder])` there.
- `MarketplaceView.swift:269,280`: Remove connector/skill has no confirmation, and `try?` swallows errors. This throws away the secrets the user entered. Fix: add a `confirmationDialog` like `Bots/Dialogs.swift`, and surface errors through `lastError`.
- `apps/ios/Views/PairingView.swift:27` and `MarketplaceView.swift:174`: `.system(size: 34/30)` doesn't scale with Dynamic Type. Fix: `.largeTitle.weight(.semibold)` / `.title.bold()`.

**Low**
- `MarketplaceView.swift:503`: `PressScale` uses `dampingFraction: 0.7`, so a plain press bounces. A press carries no momentum. Fix: `.spring(response: 0.2, dampingFraction: 1)`, and reuse it on `MarketRow` and the permission option rows.
- `MarketplaceView.swift:435-437`: the skeleton's `repeatForever` pulse ignores Reduce Motion. Fix: use a static 0.6 opacity when Reduce Motion is on.
- New rows, `WorkingIndicator` and the permission card changing to its outcome all appear abruptly. Fix: `.transition(.opacity.combined(with: .move(edge: .bottom)))` and `.animation(.snappy, value:)`.
- `Thread/PermissionCard.swift:30`: pending is shown only by a 7pt amber dot, which relies on color alone. Fix: `Label("Waiting for you", systemImage: "hand.raised.fill")`.
- `ThreadView.swift:149`: the toolbar header pill uses `.glass`, which on iOS 26 puts glass on glass. Fix: skip `.glass` in toolbar content on iOS 26+.
- `BotEditorView.swift:147`: the model field has a fixed 130pt width and truncates at accessibility sizes. Fix: `minWidth: 100, maxWidth: 200`.
- `MarketplaceView.swift:71`: "Couldn't load connectors." has no Retry button.
- `MarketplaceView.swift:461`: favicons come from `google.com/s2/favicons`, which sends each connector's domain to Google. That conflicts with the "no cloud" promise. Fix: bundle the logos or fetch them through the host.

## iOS (apps/ios, apps/widgets)

- **Medium** `apps/ios/Views/BotListView.swift:71`: the title is hidden, and the only sign of the current computer is a one-letter monogram. This fails "Where am I?" when several computers are paired. Fix: show the host name in the principal toolbar slot.
- **Medium** `apps/ios/Views/SettingsView.swift:87`: the sheet has no `navigationTitle`.
- **Medium** `apps/ios/Views/PairingView.swift:92-98`: the scanner sheet has no Cancel button. On unsupported devices the scan button is silently disabled (`:53`). Fix: add a ✕ and an explanation.

## macOS (apps/macos)

**High**
- `App/CodyncMacApp.swift:22`: this is an `LSUIElement` app, so the chat window isn't in the Dock or ⌘Tab, and there are no menu commands. Fix: switch the activation policy to `.regular` while the chat window is open, and add `.commands` for New Bot (⌘N), Stop (⌘.) and Inspector (⌥⌘I).
- `CodyncMacApp.swift:215`: Stop only appears on hover. Fix: add `accessibilityAction(named: "Stop")` and a context menu.
- `CodyncMacApp.swift:47`: the solid `Palette.background` covers the menu material. Fix: remove it and let the `.window` style's material show.
- `CodyncMacApp.swift:184`: "Uninstall host service" runs with no confirmation. Fix: add a destructive `confirmationDialog`.

**Medium**
- `App/HostController.swift:90`: `restart()` gives no feedback. Fix: set `state = .starting` first.
- `HostController.swift:115-117`: `catch {}` swallows the launch-at-login error, and the checkbox silently flips back. Fix: show the error.
- `Views/ChatWindow.swift:90`: the empty-state "New Bot" button is a hand-drawn capsule with no press state or focus ring. Fix: `.borderedProminent` with `.controlSize(.large)`.
- `ChatWindow.swift:23`: the opaque background likely hides the sidebar vibrancy. Fix: put the background on the detail column only.
- `ChatWindow.swift:66`: the New Message button has no `.help`. Fix: add `.help`, and move ⌘N into `.commands`.

**Low**
- `ThreadView.swift:76`: add a ⌥⌘I shortcut for the inspector.
- `ChatWindow.swift:48-55`: the context menu has no icons, no Stop item and no ⌫ delete (`onDeleteCommand`).
- `CodyncMacApp.swift:160`: the provider label has a fixed 130pt width. Fix: use a `Grid` for the columns.
- `CodyncMacApp.swift:40`: the pairing panel swaps in without animation and the height jumps. Fix: `withAnimation(.snappy)` with `.transition(.opacity)`.

## Linux (apps/linux)

**High**
- `src/ui.rs:509-511, 660-673`: every SSE event rebuilds the whole list and thread. Selection, focus and expanders reset, and the avatar animations restart. Fix: keep widgets per id and update them in place, and keep each avatar's start time per bot.
- `src/avatar.rs:190-195`: the tick redraws forever and ignores `gtk-enable-animations`. Fix: when animations are off, draw one static frame and skip the tick.
- `src/dialogs.rs:360,786`, `src/rows.rs:209`: error toasts go to the main window's overlay, so they appear behind the dialog. Fix: give each dialog its own `ToastOverlay`, or show inline `error` styling. Keep Save insensitive until the form is valid.
- `src/rows.rs:201-203`: only the clicked permission button is disabled, so a second answer is possible. Fix: disable the whole FlowBox.

**Medium**
- `ui.rs:251`, `dialogs.rs:568,577,599`, `ui.rs:847`: `|_| {}` swallows failures. Fix: show a toast on `Err`.
- `ui.rs:61-63`: the "Reconnecting…" banner is only in the sidebar, so it's hidden when the split view collapses. Fix: move it to the content `ToolbarView`.
- `src/main.rs:17-29`, `avatar.rs:184`: hardcoded colors ignore dark mode and high contrast. Fix: use `@warning_color` / `@error_bg_color`.
- `main.rs:18-19` and others: the custom `accent-fill` class replaces `suggested-action`, so the disabled send button looks enabled. Fix: use `suggested-action`, or add `:disabled` and `:active` rules.
- `dialogs.rs:527-608`: the bot menu is a hand-built Popover. Fix: use a `gio::Menu` with actions and `PopoverMenu`, and put Delete in its own section.
- The app has no accelerators. Fix: add Ctrl+N, Ctrl+, , Esc (stop), Ctrl+Shift+F and Ctrl+Q, plus a shortcuts dialog and an `AdwAboutDialog`.
- `ui.rs:120-126`: the composer has no placeholder and no accessible label. Swift shows "Message {name}".
- Icon-only buttons have only `tooltip_text`. Fix: also set `accessible::Property::Label`.
- `avatar.rs:171-197`, `dialogs.rs:90-143`: the pickers don't show which item is selected, and their tooltips are raw ids. Fix: use grouped `ToggleButton`s with readable names.
- Parity with Swift:
  - there's no Marketplace
  - "New bot" and "Edit Bot" use different capitalization
  - the delete dialog's wording differs from Swift

**Low**
- `ui.rs:694-705`: time separators use relative `ago()` and go stale. Fix: show absolute `%H:%M`.
- `ui.rs:164`: the empty-state icon `user-available-symbolic` is a presence icon. Fix: use the app icon.
- `rows.rs:116`, `ui.rs:557`, `dialogs.rs:412-414,487`: emoji stand in for symbolic icons.
- `ui.rs:760-770`: auto-scroll uses two `set_value` calls 60 ms apart, which hops visibly.
- `dialogs.rs:696-704`: Re-scan closes and reopens Settings, which flickers and shows no progress.
- `data/com.pokai.Codync.desktop`: `Keywords` and `SingleMainWindow` are missing, and there's no metainfo file.

## Already good

- Reduce Motion pauses `CharacterAvatar` and `ThinkingOrb`.
- Icon-only Swift buttons use `Button(_, systemImage:)` with `.iconOnly` or `.help`.
- There's one confirmation, for delete, and its copy is reassuring.
- The composer sits in `safeAreaInset` and content scrolls under it; the trace sheet uses detents.
- The Mac uses `NavigationSplitView` + `.inspector`, Return to send, and ⌘1…9 quick picks.
- Linux uses the correct libadwaita skeleton (`NavigationSplitView`, `ToolbarView`, a breakpoint, `AlertDialog` destructive) and sends messages optimistically.
