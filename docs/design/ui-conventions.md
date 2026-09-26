# UI conventions

## iPhone navigation

The bot list and conversation use the system `NavigationStack` toolbar. `ToolbarItem` owns the account, computer, new-chat, call and more buttons; the conversation uses the system back button. iOS supplies Liquid Glass, control sizing, grouping and interaction feedback.

Do not wrap those toolbar controls in `IconButtonStyle`, hand-sized rounded backgrounds or another glass effect. The account avatar is toolbar content, with the system supplying the enclosing surface. This is the explicit exception to the custom-chrome rule below.

`ConnectingIndicator` is in the toolbar center and remains visible for at least 0.7 seconds after appearing. Transient connecting state is omitted from `ConnectionBanner`, so it does not insert/remove loading content above the bot rows or composer. Actionable offline/authorization warnings remain separate. This does not remove the normal pull-to-refresh interaction.

Sources: `apps/ios/Views/BotListView.swift`, `AccountSwitcherView.swift`, `kit/Sources/CodyncUI/Thread/ThreadView.swift`, `Chrome.swift`, `Bots/BotRow.swift`.

## Shared application controls

Other custom screens use `Chrome.swift` and `Controls.swift`:

| Need | Component |
|---|---|
| Modal or full-window overlay | `.codyncSheet`, `.codyncOverlay` |
| Header outside the native bot navigation flow | `ModalHeader`, `ScreenHeader` |
| Icon action | `IconButton` |
| Tab selection | `TabBar` |
| Menu / confirmation | `.codyncMenu`, `.codyncDialog` |
| Toggle | `ToggleStyle.codync` |
| Form-like content | `CardForm`, `CardSection` |

Avoid adding stock `Menu`, `Picker`, `Form`/`List` styling, switch toggles, alerts, `ProgressView`, `TabView` or system sheets to these flows. System presentation APIs inside the shared chrome implementation are implementation details, not permission to bypass the components in feature screens. Native WidgetKit/ActivityKit containers and OS authentication/permission flows remain system integrations.

Anything with a background fill gets no extra drawn border. Use `Palette`, `InterfaceMetrics` and `Motion`; visibility changes animate and honor Reduce Motion. Icon-only actions have an accessibility label and desktop help where applicable. Reserve text for actions an icon cannot clearly express.

## Motion and status

`CharacterAvatar` identifies a bot. `ThinkingOrb` conveys working, searching, listening or connecting and needs adjacent text or an accessible outer label. In-app animation pauses off-screen, when inactive, or with Reduce Motion. Widgets/activities use static frames.

Provider identity and activity presentation are shared in `CodyncKit/Design`; use those components rather than re-creating mappings per app. See [mobile widgets](mobile-widgets.md) for current rendering and previews.

The [2026-09-25 audit](../archive/ui-audit-2026-09-25.md) is historical. Its old line numbers and recommendations are not the current UI policy or a list of confirmed open bugs.
