import CodyncKit
import SwiftUI
import UIKit

// Codync's own controls. Never use the stock ones (Picker, .switch toggles,
// Form/List styling, confirmationDialog/alert, ProgressView, DisclosureGroup,
// .bordered buttons, swipeActions): build from these instead (CardForm, CardSection, Hairline and Spinner
// are in CodyncKit, for the share extension).
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

// MARK: - Menus

/// Picks one value: shows the current choice in a pill with a chevron, opens the system menu.
public struct ChoicePicker<ID: Hashable>: View {
    @Binding var selection: ID
    let options: [(id: ID, label: String)]
    var fill: Color
    var fitsAvailableWidth: Bool
    /// A picture before each label, in the menu and on the pill (an agent's logo).
    var image: ((ID) -> Image?)?

    public init(selection: Binding<ID>, options: [(id: ID, label: String)], fill: Color = .clear, fitsAvailableWidth: Bool = false,
                image: ((ID) -> Image?)? = nil) {
        _selection = selection
        self.options = options
        self.fill = fill
        self.fitsAvailableWidth = fitsAvailableWidth
        self.image = image
    }

    public var body: some View {
        DropdownMenu {
            options.map { option in
                MenuItem(option.label, image: image?(option.id), selected: option.id == selection) { selection = option.id }
            }
        } label: {
            HStack(spacing: 6) {
                if let picture = image?(selection) {
                    picture.resizable().scaledToFit().frame(width: 16, height: 16)
                }
                Text(L10n.tr(options.first { $0.id == selection }?.label ?? "Choose")).lineLimit(1)
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
        .help(L10n.tr(options.first { $0.id == selection }?.label ?? "Choose"))
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
                    Text(L10n.tr(option.label))
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

public extension EnvironmentValues {
    /// How many lines a row's note shows; the details sheet keeps long ones (skill descriptions) to two.
    @Entry var noteLines: Int? = nil
}

/// Grok Bot's page tabs: plain labels, the chosen one on a light capsule.
public struct TabChoice<ID: Hashable>: View {
    @Binding var selection: ID
    let options: [(id: ID, label: String)]
    @Namespace private var pill
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    public init(selection: Binding<ID>, options: [(id: ID, label: String)]) {
        _selection = selection
        self.options = options
    }

    public var body: some View {
        // Labels too long for the width shrink a little, then scroll, rather than cut one short.
        ViewThatFits(in: .horizontal) {
            tabs(small: false)
            tabs(small: true)
            ScrollView(.horizontal) { tabs(small: false) }
                .scrollIndicators(.hidden)
        }
    }

    private func tabs(small: Bool) -> some View {
        HStack(spacing: 2) {
            ForEach(options, id: \.id) { option in
                let on = option.id == selection
                Button {
                    withAnimation(Motion.reduced(Motion.morph, reduceMotion)) { selection = option.id }
                } label: {
                    Text(L10n.tr(option.label))
                        .font(small ? .footnote : .subheadline)
                        .foregroundStyle(on ? Palette.text : Palette.secondary)
                        .lineLimit(1)
                        .padding(.horizontal, small ? 10 : 12)
                        .padding(.vertical, 6)
                        .background {
                            if on { Capsule().fill(Palette.bubbleAgent).matchedGeometryEffect(id: "pill", in: pill) }
                        }
                        .contentShape(Capsule())
                }
                .buttonStyle(.plain)
                .accessibilityAddTraits(on ? .isSelected : [])
            }
        }
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
                            Text(L10n.tr(option.label)).foregroundStyle(Palette.text)
                            if let detail = option.detail, !detail.isEmpty {
                                Text(L10n.tr(detail)).font(.subheadline).foregroundStyle(Palette.secondary)
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

/// Label on the left, value on the right: the replacement for `LabeledContent`.
public struct ValueRow<Value: View>: View {
    let label: String
    let detail: String?
    let value: Value
    @Environment(\.noteLines) private var noteLines

    public init(_ label: String, detail: String? = nil, @ViewBuilder value: () -> Value) {
        self.label = label
        self.detail = detail
        self.value = value()
    }

    public init(_ label: String, detail: String? = nil, value: String) where Value == Text {
        self.label = label
        self.detail = detail
        self.value = Text(L10n.tr(value))
    }

    public var body: some View {
        HStack(spacing: 12) {
            VStack(alignment: .leading, spacing: 3) {
                Text(L10n.tr(label)).foregroundStyle(Palette.text)
                if let detail {
                    Text(L10n.tr(detail))
                        .font(.caption)
                        .foregroundStyle(Palette.secondary)
                        .lineLimit(noteLines)
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
    /// Puts the cursor in the field as it appears.
    let autoFocus: Bool
    @FocusState private var focused: Bool

    public init(_ prompt: String = "Search", text: Binding<String>, autoFocus: Bool = false) {
        self.prompt = prompt
        _text = text
        self.autoFocus = autoFocus
    }

    public var body: some View {
        HStack(spacing: 8) {
            Image(systemName: "magnifyingglass").foregroundStyle(Palette.secondary)
            TextField(prompt, text: $text).textFieldStyle(.plain).plainTextInput()
                .focused($focused)
                .onAppear { if autoFocus { focused = true } }
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

    /// Grok Bot's list cards (routines, the computer switch): an outline on the page, no fill.
    func outlinedCard(vertical: CGFloat = 12) -> some View {
        padding(.horizontal, 14)
            .padding(.vertical, vertical)
            .frame(maxWidth: .infinity, alignment: .leading)
            .overlay(RoundedRectangle(cornerRadius: 14, style: .continuous).strokeBorder(Palette.border))
    }
}

