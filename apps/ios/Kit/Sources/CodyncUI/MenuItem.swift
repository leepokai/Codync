import CodyncKit
import SwiftUI

/// One row in a Codync menu: an action, a choice when `selected` is set, or a submenu.
public struct MenuItem {
    public var title: String
    public var icon: String?
    /// A picture in the icon's place (an agent's logo).
    public var image: Image?
    public var selected: Bool?
    public var destructive: Bool
    public var divider: Bool
    /// Shows `title` as it is, never translated: a name the user gave (a section's).
    public var verbatim: Bool
    /// False for a line that only says something: it shows dimmed and can't be picked.
    public var enabled: Bool
    /// The rows of the submenu this row opens ("Move to section ▸") instead of running `action`;
    /// one level deep: a row inside it can't open another.
    public var submenu: [MenuItem] = []
    public var action: () -> Void

    public init(_ title: String, icon: String? = nil, image: Image? = nil, selected: Bool? = nil, destructive: Bool = false,
                divider: Bool = false, verbatim: Bool = false, enabled: Bool = true, action: @escaping () -> Void) {
        self.title = title
        self.icon = icon
        self.image = image
        self.selected = selected
        self.destructive = destructive
        self.divider = divider
        self.verbatim = verbatim
        self.enabled = enabled
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

/// Rows are equal when they show the same thing; the action isn't part of it, since closures always differ.
extension MenuItem: Equatable {
    public static func == (lhs: MenuItem, rhs: MenuItem) -> Bool {
        lhs.shownTitle() == rhs.shownTitle() && lhs.icon == rhs.icon && lhs.image == rhs.image && lhs.selected == rhs.selected
            && lhs.destructive == rhs.destructive && lhs.divider == rhs.divider && lhs.enabled == rhs.enabled
            && lhs.submenu == rhs.submenu
    }
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
    @State private var latest = LatestMenuRows()

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
            MenuRows(rows: latest.holding(items()), latest: latest).equatable()
        }
    }
}

/// The rows of a menu as of its last update, so a row that didn't update still runs its newest action.
@MainActor final class LatestMenuRows {
    private var rows: [MenuItem] = []

    func holding(_ rows: [MenuItem]) -> [MenuItem] {
        self.rows = rows
        return rows
    }

    /// A press can outrun an update that moved the rows: a place that now shows something else runs what was pressed,
    /// never what took its place.
    func run(_ pressed: MenuItem, row: Int, inSubmenu: Int? = nil) {
        guard rows.indices.contains(row) else { return }
        let siblings = inSubmenu == nil ? rows : rows[row].submenu
        let index = inSubmenu ?? row
        guard siblings.indices.contains(index) else { return }
        (siblings[index] == pressed ? siblings[index] : pressed).action()
    }
}

/// A menu's rows. SwiftUI rebuilds an open system menu when its content updates, and a press during that rebuild is lost,
/// so they update only when what they show changed: not because the screen behind them did.
private struct MenuRows: View {
    let rows: [MenuItem]
    let latest: LatestMenuRows

    var body: some View {
        ForEach(rows.indices, id: \.self) { index in
            let item = rows[index]
            if item.divider { Divider() }
            systemMenuRow(item, row: index, latest: latest)
        }
    }
}

extension MenuRows: @MainActor Equatable {
    static func == (lhs: Self, rhs: Self) -> Bool { lhs.rows == rhs.rows }
}

/// A `MenuItem` inside a system `Menu` or `contextMenu`; a submenu opens its rows in a system
/// `Menu`, one level deep (an opaque row type can't contain itself).
@MainActor @ViewBuilder private func systemMenuRow(_ item: MenuItem, row: Int, latest: LatestMenuRows) -> some View {
    if item.submenu.isEmpty {
        systemMenuAction(item) { latest.run(item, row: row) }
    } else {
        Menu {
            ForEach(item.submenu.indices, id: \.self) { index in
                let inner = item.submenu[index]
                let _ = assert(inner.submenu.isEmpty, "A submenu is one level deep: this row would be a dead button")
                if inner.divider { Divider() }
                systemMenuAction(inner) { latest.run(inner, row: row, inSubmenu: index) }
            }
        } label: { menuLabel(item) }
    }
}

/// A `MenuItem` that runs `action`: choices as checkmarked toggles.
@ViewBuilder private func systemMenuAction(_ item: MenuItem, action: @escaping @MainActor () -> Void) -> some View {
    let role: ButtonRole? = item.destructive ? .destructive : nil
    if let selected = item.selected {
        // SwiftUI only calls the binding's setter from the menu, on the main thread.
        Toggle(isOn: Binding(get: { selected }, set: { _ in MainActor.assumeIsolated { action() } })) { menuLabel(item) }
    } else {
        Button(role: role, action: action) { menuLabel(item) }
            .disabled(!item.enabled)
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
    @State private var latest = LatestMenuRows()

    public init(items: @escaping () -> [MenuItem], @ViewBuilder label: () -> Label) {
        self.items = items
        self.label = label()
    }

    public var body: some View {
        // The system menu: it anchors correctly inside sheets and scroll views, where an overlay can't.
        Menu {
            MenuRows(rows: latest.holding(items()), latest: latest).equatable()
        } label: { label }
            .buttonStyle(.plain)
            .menuIndicator(.hidden)
    }
}
