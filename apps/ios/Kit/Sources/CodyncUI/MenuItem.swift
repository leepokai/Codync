import CodyncKit
import SwiftUI

/// One row in a Codync menu: an action, a choice when `selected` is set, or a submenu.
public struct MenuItem: Identifiable {
    public let id = UUID()
    public var title: String
    public var icon: String?
    /// A picture in the icon's place (an agent's logo).
    public var image: Image?
    public var selected: Bool?
    public var destructive: Bool
    public var divider: Bool
    /// Shows `title` as it is, never translated: a name the user gave (a section's).
    public var verbatim: Bool
    /// The rows of the submenu this row opens ("Move to section ▸") instead of running `action`;
    /// one level deep: a row inside it can't open another.
    public var submenu: [MenuItem] = []
    public var action: () -> Void

    public init(_ title: String, icon: String? = nil, image: Image? = nil, selected: Bool? = nil, destructive: Bool = false,
                divider: Bool = false, verbatim: Bool = false, action: @escaping () -> Void) {
        self.title = title
        self.icon = icon
        self.image = image
        self.selected = selected
        self.destructive = destructive
        self.divider = divider
        self.verbatim = verbatim
        self.action = action
    }

    /// A row that opens a submenu of `rows`.
    public init(_ title: String, icon: String? = nil, divider: Bool = false, submenu rows: [MenuItem]) {
        self.init(title, icon: icon, divider: divider) {}
        submenu = rows
    }

    /// `title` as the row shows it: in the app's language, unless `verbatim`.
    func shownTitle(_ translate: (String) -> String = { L10n.tr($0) }) -> String { verbatim ? title : translate(title) }
}

/// A row of emoji above a message's menu (Slack's quick reactions).
public struct ReactionPick {
    public var emoji: [String]
    public var chosen: [String]
    public var toggle: (String) -> Void

    public init(emoji: [String], chosen: [String], toggle: @escaping (String) -> Void) {
        self.emoji = emoji
        self.chosen = chosen
        self.toggle = toggle
    }
}

public extension View {
    /// Long-press opens the system context menu; `reactions` puts a quick-reaction row on top.
    func contextActions(reactions: ReactionPick? = nil, _ items: @escaping () -> [MenuItem]) -> some View {
        modifier(ContextActions(items: items, reactions: reactions))
    }
}

private struct ContextActions: ViewModifier {
    let items: () -> [MenuItem]
    let reactions: ReactionPick?

    func body(content: Content) -> some View {
        content.contextMenu {
            if let reactions {
                ControlGroup {
                    ForEach(reactions.emoji, id: \.self) { emoji in
                        let chosen = reactions.chosen.contains(emoji)
                        Toggle(isOn: Binding(get: { chosen }, set: { _ in reactions.toggle(emoji) })) { Text(emoji) }
                    }
                }
                .controlGroupStyle(.palette)
            }
            ForEach(items()) { item in
                if item.divider { Divider() }
                systemMenuRow(item)
            }
        }
    }
}

/// A `MenuItem` inside a system `Menu` or `contextMenu`; a submenu opens its rows in a system
/// `Menu`, one level deep (an opaque row type can't contain itself).
@ViewBuilder private func systemMenuRow(_ item: MenuItem) -> some View {
    if item.submenu.isEmpty {
        systemMenuAction(item)
    } else {
        Menu {
            ForEach(item.submenu) { row in
                let _ = assert(row.submenu.isEmpty, "A submenu is one level deep: this row would be a dead button")
                if row.divider { Divider() }
                systemMenuAction(row)
            }
        } label: { menuLabel(item) }
    }
}

/// A `MenuItem` that runs its action: choices as checkmarked toggles.
@ViewBuilder private func systemMenuAction(_ item: MenuItem) -> some View {
    let role: ButtonRole? = item.destructive ? .destructive : nil
    if let selected = item.selected {
        // The binding's setter must be Sendable, but SwiftUI only calls it from the menu, on the
        // main thread where the action was made.
        nonisolated(unsafe) let action = item.action
        Toggle(isOn: Binding(get: { selected }, set: { _ in action() })) { menuLabel(item) }
    } else {
        Button(role: role, action: item.action) { menuLabel(item) }
    }
}

@ViewBuilder private func menuLabel(_ item: MenuItem) -> some View {
    let title = item.shownTitle()
    if let image = item.image {
        SwiftUI.Label { Text(title) } icon: { image }
    } else if let icon = item.icon {
        SwiftUI.Label(title, systemImage: icon)
    } else {
        Text(title)
    }
}

/// A button that opens the system menu.
public struct DropdownMenu<Label: View>: View {
    let items: () -> [MenuItem]
    let label: Label

    public init(items: @escaping () -> [MenuItem], @ViewBuilder label: () -> Label) {
        self.items = items
        self.label = label()
    }

    public var body: some View {
        // The system menu: it anchors correctly inside sheets and scroll views, where an overlay can't.
        Menu {
            ForEach(items()) { item in
                if item.divider { Divider() }
                systemMenuRow(item)
            }
        } label: { label }
            .buttonStyle(.plain)
            .menuIndicator(.hidden)
    }
}
