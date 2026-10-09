import CodyncKit
import SwiftUI
import UIKit

// Codync's own controls. Never use the stock ones (Picker, .switch toggles,
// Form/List styling, confirmationDialog/alert, ProgressView, DisclosureGroup,
// .bordered buttons, swipeActions): build from these instead.
// Menus are the one exception: `DropdownMenu`/`ChoicePicker` open the system `Menu` and
// `.contextActions` the system `contextMenu`. A hand-built overlay anchored by global
// frame lands in the wrong place (sheets, scroll views, the composer).
// Anything with a background fill gets no border line.

// MARK: - Switch

/// On/off switch: label on the left, a flat capsule track on the right.
public struct CodyncSwitch: ToggleStyle {
    public init() {}

    public func makeBody(configuration: Configuration) -> some View {
        SwitchBody(configuration: configuration)
    }

    private struct SwitchBody: View {
        let configuration: Configuration
        @Environment(\.accessibilityReduceMotion) private var reduceMotion
        @Environment(\.isEnabled) private var isEnabled

        var body: some View {
            Button {
                withAnimation(Motion.reduced(Motion.hover, reduceMotion)) { configuration.isOn.toggle() }
            } label: {
                HStack(spacing: 12) {
                    configuration.label
                    Spacer(minLength: 0)
                    Capsule()
                        .fill(configuration.isOn ? Palette.accentFill : Palette.bubbleUser)
                        .frame(width: 34, height: 20)
                        .overlay(alignment: configuration.isOn ? .trailing : .leading) {
                            // Black and white: the knob takes the opposite ink of an on track.
                            Circle().fill(configuration.isOn ? Palette.onAccent : .white).padding(2)
                        }
                }
                .contentShape(Rectangle())
                .opacity(isEnabled ? 1 : 0.4)
            }
            .buttonStyle(.plain)
            .accessibilityRepresentation { Toggle(isOn: configuration.$isOn) { configuration.label } }
        }
    }
}

public extension ToggleStyle where Self == CodyncSwitch {
    static var codync: CodyncSwitch { CodyncSwitch() }
}

// MARK: - Buttons

/// The filled call-to-action: accent capsule with press feedback.
public struct PrimaryButtonStyle: ButtonStyle {
    public init() {}

    public func makeBody(configuration: Configuration) -> some View {
        StyledButton(configuration: configuration, fill: Palette.accentFill, foreground: Palette.onAccent)
    }
}

/// A quieter filled button next to a primary one.
public struct SecondaryButtonStyle: ButtonStyle {
    public init() {}

    public func makeBody(configuration: Configuration) -> some View {
        StyledButton(configuration: configuration, fill: Palette.bubbleUser, foreground: Palette.text)
    }
}

private struct StyledButton: View {
    let configuration: ButtonStyleConfiguration
    let fill: Color
    let foreground: Color
    @Environment(\.isEnabled) private var isEnabled

    var body: some View {
        configuration.label
            .font(.body.weight(.medium))
            .foregroundStyle(foreground)
            .padding(.horizontal, 18)
            .padding(.vertical, 11)
            .background(fill, in: Capsule())
            .opacity(isEnabled ? (configuration.isPressed ? 0.85 : 1) : 0.4)
            .scaleEffect(configuration.isPressed ? Motion.pressScale : 1)
            .animation(Motion.press, value: configuration.isPressed)
            .contentShape(Capsule())
    }
}

public extension ButtonStyle where Self == PrimaryButtonStyle {
    static var primary: PrimaryButtonStyle { PrimaryButtonStyle() }
}

public extension ButtonStyle where Self == SecondaryButtonStyle {
    static var secondary: SecondaryButtonStyle { SecondaryButtonStyle() }
}

// MARK: - Spinner

/// Loading indicator: a thin arc that turns.
public struct Spinner: View {
    var size: CGFloat
    @State private var turning = false
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    public init(size: CGFloat = 14) { self.size = size }

    public var body: some View {
        Circle()
            .trim(from: 0, to: 0.7)
            .stroke(Palette.secondary, style: StrokeStyle(lineWidth: max(1.5, size / 9), lineCap: .round))
            .frame(width: size, height: size)
            .rotationEffect(.degrees(turning ? 360 : 0))
            .animation(reduceMotion ? nil : .linear(duration: 0.8).repeatForever(autoreverses: false), value: turning)
            .onAppear { turning = true }
            .accessibilityLabel("Loading")
    }
}

// MARK: - Menus

/// One row in a Codync menu: an action, or a choice when `selected` is set.
public struct MenuItem: Identifiable {
    public let id = UUID()
    public var title: String
    public var icon: String?
    public var selected: Bool?
    public var destructive: Bool
    public var divider: Bool
    public var action: () -> Void

    public init(_ title: String, icon: String? = nil, selected: Bool? = nil, destructive: Bool = false,
                divider: Bool = false, action: @escaping () -> Void) {
        self.title = title
        self.icon = icon
        self.selected = selected
        self.destructive = destructive
        self.divider = divider
        self.action = action
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

/// A `MenuItem` inside a system `Menu` or `contextMenu`: choices as checkmarked toggles.
@ViewBuilder private func systemMenuRow(_ item: MenuItem) -> some View {
    let role: ButtonRole? = item.destructive ? .destructive : nil
    if let selected = item.selected {
        Toggle(isOn: Binding(get: { selected }, set: { _ in item.action() })) {
            if let icon = item.icon { SwiftUI.Label(item.title, systemImage: icon) } else { Text(item.title) }
        }
    } else if let icon = item.icon {
        Button(role: role, action: item.action) { SwiftUI.Label(item.title, systemImage: icon) }
    } else {
        Button(item.title, role: role, action: item.action)
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

/// Picks one value: shows the current choice in a pill with a chevron, opens the system menu.
public struct ChoicePicker<ID: Hashable>: View {
    @Binding var selection: ID
    let options: [(id: ID, label: String)]
    var fill: Color
    var fitsAvailableWidth: Bool

    public init(selection: Binding<ID>, options: [(id: ID, label: String)], fill: Color = .clear, fitsAvailableWidth: Bool = false) {
        _selection = selection
        self.options = options
        self.fill = fill
        self.fitsAvailableWidth = fitsAvailableWidth
    }

    public var body: some View {
        DropdownMenu {
            options.map { option in MenuItem(option.label, selected: option.id == selection) { selection = option.id } }
        } label: {
            HStack(spacing: 6) {
                Text(options.first { $0.id == selection }?.label ?? "Choose").lineLimit(1)
                Image(systemName: "chevron.down").font(.caption2.weight(.semibold)).foregroundStyle(Palette.secondary)
            }
            .font(.body)
            .foregroundStyle(Palette.text)
            .pill(fill: fill)
            .overlay {
                if fill == .clear { RoundedRectangle(cornerRadius: 10, style: .continuous).strokeBorder(Palette.border) }
            }
        }
        .fixedSize(horizontal: !fitsAvailableWidth, vertical: true)
        .help(options.first { $0.id == selection }?.label ?? "Choose")
    }
}

/// Picks one of a few values side by side: the replacement for `.segmented`.
public struct SegmentedChoice<ID: Hashable>: View {
    @Binding var selection: ID
    let options: [(id: ID, label: String)]
    @Namespace private var thumb
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    public init(selection: Binding<ID>, options: [(id: ID, label: String)]) {
        _selection = selection
        self.options = options
    }

    public var body: some View {
        HStack(spacing: 2) {
            ForEach(options, id: \.id) { option in
                let on = option.id == selection
                Button {
                    withAnimation(Motion.reduced(Motion.morph, reduceMotion)) { selection = option.id }
                } label: {
                    Text(option.label)
                        .font(.subheadline.weight(.medium))
                        .foregroundStyle(on ? Palette.text : Palette.secondary)
                        .lineLimit(1)
                        .frame(maxWidth: .infinity)
                        .padding(.vertical, 8)
                        .background {
                            if on {
                                Capsule().fill(Palette.background).matchedGeometryEffect(id: "thumb", in: thumb)
                            }
                        }
                        .contentShape(Capsule())
                }
                .buttonStyle(.plain)
                .accessibilityAddTraits(on ? .isSelected : [])
            }
        }
        .padding(3)
        .background(Palette.bubbleUser, in: Capsule())
    }
}

/// A list of choices, one per row with a checkmark: the replacement for `.inline` pickers.
public struct ChoiceList<ID: Hashable>: View {
    @Binding var selection: ID
    let options: [(id: ID, label: String, detail: String?)]

    public init(selection: Binding<ID>, options: [(id: ID, label: String, detail: String?)]) {
        _selection = selection
        self.options = options
    }

    public var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            ForEach(options, id: \.id) { option in
                Button { selection = option.id } label: {
                    HStack(spacing: 10) {
                        VStack(alignment: .leading, spacing: 2) {
                            Text(option.label).foregroundStyle(Palette.text)
                            if let detail = option.detail, !detail.isEmpty {
                                Text(detail).font(.subheadline).foregroundStyle(Palette.secondary)
                            }
                        }
                        Spacer(minLength: 8)
                        Image(systemName: "checkmark")
                            .font(.caption.weight(.semibold))
                            .foregroundStyle(Palette.text)
                            .opacity(option.id == selection ? 1 : 0)
                    }
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .accessibilityAddTraits(option.id == selection ? .isSelected : [])
            }
        }
    }
}

// MARK: - Forms

/// A scrolling page of cards: the replacement for `Form` / grouped `List`.
public struct CardForm<Content: View>: View {
    let content: Content

    public init(@ViewBuilder content: () -> Content) { self.content = content() }

    public var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 32) {
                content
            }
            .font(.body)
            .padding(20)
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        .scrollDismissesKeyboard(.interactively)
        .background(Palette.background)
    }
}

/// A titled group of rows: the replacement for `Section`. Modeled on ChatGPT's desktop settings:
/// a small bold heading over a filled, rounded group whose rows are split by inset hairlines
/// (references in `docs/design/reference/`).
public struct CardSection<Content: View, Accessory: View>: View {
    let title: String?
    let footer: String?
    let content: Content
    /// Trailing controls on the heading's line (add, refresh, clear).
    let accessory: Accessory

    public init(_ title: String? = nil, footer: String? = nil, @ViewBuilder content: () -> Content,
                @ViewBuilder accessory: () -> Accessory) {
        self.title = title
        self.footer = footer
        self.content = content()
        self.accessory = accessory()
    }

    public var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            if let title {
                HStack(spacing: 8) {
                    Text(title)
                        .font(.system(size: 15, weight: .semibold))
                        .foregroundStyle(Palette.text)
                        .accessibilityAddTraits(.isHeader)
                    Spacer(minLength: 0)
                    accessory
                }
                .padding(.leading, 2)
            }
            VStack(alignment: .leading, spacing: 0) {
                _VariadicView.Tree(HairlineRows()) { content }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(Palette.surface, in: RoundedRectangle(cornerRadius: 16, style: .continuous))
            if let footer {
                Text(footer)
                    .font(.caption)
                    .foregroundStyle(Palette.secondary)
                    .lineSpacing(2)
                    .fixedSize(horizontal: false, vertical: true)
                    .padding(.horizontal, 2)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}

public extension CardSection where Accessory == EmptyView {
    init(_ title: String? = nil, footer: String? = nil, @ViewBuilder content: () -> Content) {
        self.init(title, footer: footer, content: content) { EmptyView() }
    }
}

/// A 1-pixel separator line in the border color.
public struct Hairline: View {
    @Environment(\.displayScale) private var scale

    public init() {}

    public var body: some View {
        Rectangle().fill(Palette.border).frame(height: 1 / scale)
    }
}

/// Lays a section's rows out one under another with an inset hairline between each pair.
private struct HairlineRows: _VariadicView_MultiViewRoot {
    func body(children: _VariadicView.Children) -> some View {
        let inset: CGFloat = 16
        ForEach(children) { child in
            child
                .frame(maxWidth: .infinity, minHeight: 32, alignment: .leading)
                .padding(.horizontal, inset)
                .padding(.vertical, 14)
            if child.id != children.last?.id { Hairline().padding(.horizontal, inset) }
        }
    }
}

/// Label on the left, value on the right: the replacement for `LabeledContent`.
public struct ValueRow<Value: View>: View {
    let label: String
    let detail: String?
    let value: Value

    public init(_ label: String, detail: String? = nil, @ViewBuilder value: () -> Value) {
        self.label = label
        self.detail = detail
        self.value = value()
    }

    public init(_ label: String, detail: String? = nil, value: String) where Value == Text {
        self.label = label
        self.detail = detail
        self.value = Text(value)
    }

    public var body: some View {
        HStack(spacing: 12) {
            VStack(alignment: .leading, spacing: 3) {
                Text(label).foregroundStyle(Palette.text)
                if let detail {
                    Text(detail)
                        .font(.caption)
                        .foregroundStyle(Palette.secondary)
                        .lineSpacing(2)
                        .fixedSize(horizontal: false, vertical: true)
                }
            }
            Spacer(minLength: 12)
            value.foregroundStyle(Palette.secondary)
        }
    }
}

/// A search box: the replacement for `.searchable`.
public struct SearchField: View {
    let prompt: String
    @Binding var text: String

    public init(_ prompt: String = "Search", text: Binding<String>) {
        self.prompt = prompt
        _text = text
    }

    public var body: some View {
        HStack(spacing: 8) {
            Image(systemName: "magnifyingglass").foregroundStyle(Palette.secondary)
            TextField(prompt, text: $text).textFieldStyle(.plain).plainTextInput()
            if !text.isEmpty {
                Button { text = "" } label: {
                    Image(systemName: "xmark.circle.fill").foregroundStyle(Palette.tertiary)
                }
                .buttonStyle(.plain)
                .accessibilityLabel("Clear search")
            }
        }
        .font(.body)
        .padding(.horizontal, 14)
        .padding(.vertical, 10)
        .background(Palette.bubbleAgent, in: Capsule())
    }
}

/// A row that expands to show more: the replacement for `DisclosureGroup`.
public struct Disclosure<Label: View, Content: View>: View {
    @Binding var isExpanded: Bool
    let label: Label
    let content: Content
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    public init(isExpanded: Binding<Bool>, @ViewBuilder content: () -> Content, @ViewBuilder label: () -> Label) {
        _isExpanded = isExpanded
        self.content = content()
        self.label = label()
    }

    public var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            Button {
                withAnimation(Motion.reduced(Motion.layout, reduceMotion)) { isExpanded.toggle() }
            } label: {
                HStack(spacing: 6) {
                    label
                    Spacer(minLength: 8)
                    Image(systemName: "chevron.right")
                        .font(.caption2.weight(.semibold))
                        .foregroundStyle(Palette.secondary)
                        .rotationEffect(.degrees(isExpanded ? 90 : 0))
                }
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .accessibilityValue(isExpanded ? "Expanded" : "Collapsed")
            if isExpanded { content }
        }
    }
}

// MARK: - Shared bits

extension View {
    /// The value pill: a filled rounded box, no border.
    /// A value in an outlined pill, like ChatGPT's settings dropdowns (outline or fill, never both).
    func outlinedPill() -> some View {
        pill(fill: .clear)
            .overlay(RoundedRectangle(cornerRadius: 10, style: .continuous).strokeBorder(Palette.border))
    }

    func pill(fill: Color = Palette.background) -> some View {
        padding(.horizontal, 12)
            .padding(.vertical, 7)
            .background(fill, in: RoundedRectangle(cornerRadius: 10, style: .continuous))
    }
}

