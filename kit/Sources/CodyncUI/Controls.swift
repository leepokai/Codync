import CodyncKit
import SwiftUI
#if os(macOS)
import AppKit
#endif

// Codync's own controls. Never use the stock ones (Menu, Picker, .switch toggles,
// Form/List styling, confirmationDialog/alert, ProgressView, DisclosureGroup,
// .bordered buttons, contextMenu/swipeActions): build from these instead.
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
                        .fill(configuration.isOn ? Palette.switchOn : Palette.bubbleUser)
                        .frame(width: 34, height: 20)
                        .overlay(alignment: configuration.isOn ? .trailing : .leading) {
                            Circle().fill(.white).padding(2)
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
            .font(InterfaceMetrics.body.weight(.medium))
            .foregroundStyle(foreground)
            .padding(.horizontal, InterfaceMetrics.value(mac: 12, mobile: 18))
            .padding(.vertical, InterfaceMetrics.value(mac: 6, mobile: 11))
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

/// The floating panel of menu rows (used by every menu, popover or overlay).
public struct MenuPanel: View {
    let items: [MenuItem]
    let dismiss: () -> Void

    public init(items: [MenuItem], dismiss: @escaping () -> Void) {
        self.items = items
        self.dismiss = dismiss
    }

    public var body: some View {
        VStack(alignment: .leading, spacing: 2) {
            ForEach(items) { item in
                if item.divider {
                    Rectangle().fill(Palette.text.opacity(0.1)).frame(height: 0.5)
                        .padding(.horizontal, 10).padding(.vertical, 4)
                }
                MenuRow(item: item) {
                    dismiss()
                    item.action()
                }
            }
        }
        .padding(6)
        .frame(minWidth: InterfaceMetrics.value(mac: 180, mobile: 220))
        .fixedSize()
    }
}

private struct MenuRow: View {
    let item: MenuItem
    let action: () -> Void
    @State private var hovering = false

    var body: some View {
        Button(action: action) {
            HStack(spacing: 10) {
                if let icon = item.icon {
                    Image(systemName: icon).frame(width: 18)
                }
                Text(item.title).lineLimit(1)
                Spacer(minLength: 16)
                if let selected = item.selected {
                    Image(systemName: "checkmark").font(.caption.weight(.semibold)).opacity(selected ? 1 : 0)
                }
            }
            .font(InterfaceMetrics.body)
            .foregroundStyle(item.destructive ? Palette.danger : Palette.text)
            .padding(.horizontal, 10)
            .padding(.vertical, InterfaceMetrics.value(mac: 6, mobile: 11))
            .background(hovering ? Palette.text.opacity(0.07) : .clear, in: RoundedRectangle(cornerRadius: 8, style: .continuous))
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .onHover { h in withAnimation(Motion.hover) { hovering = h } }
        .accessibilityAddTraits(item.selected == true ? .isSelected : [])
    }
}

public extension View {
    /// Shows a Codync menu under (or above) this view while `isPresented` is true.
    func codyncMenu(isPresented: Binding<Bool>, items: @escaping () -> [MenuItem]) -> some View {
        modifier(AnchoredMenu(isPresented: isPresented, point: nil, items: items))
    }

    /// Long-press (iPhone) or right-click (Mac) opens a Codync menu: the replacement for `contextMenu`.
    func contextActions(_ items: @escaping () -> [MenuItem]) -> some View {
        modifier(ContextActions(items: items))
    }
}

private struct ContextActions: ViewModifier {
    let items: () -> [MenuItem]
    @State private var open = false
    @State private var point: CGPoint?

    func body(content: Content) -> some View {
        content
            #if os(macOS)
            .overlay { SecondaryClickCapture { point = $0; open = true } }
            #else
            .onLongPressGesture(minimumDuration: 0.35) { point = nil; open = true }
            #endif
            .modifier(AnchoredMenu(isPresented: $open, point: point, items: items))
            .accessibilityActions {
                ForEach(items()) { item in
                    Button(item.title, action: item.action)
                }
            }
    }
}

/// Presents `MenuPanel` next to the view (or at `point` inside it, for right-clicks).
private struct AnchoredMenu: ViewModifier {
    @Binding var isPresented: Bool
    let point: CGPoint?
    let items: () -> [MenuItem]
    @State private var frame: CGRect = .zero

    func body(content: Content) -> some View {
        content
            .onGeometryChange(for: CGRect.self) { $0.frame(in: .global) } action: { frame = $0 }
            .codyncOverlay(isPresented: $isPresented) { close in
                let anchor = point.map { CGRect(x: frame.minX + $0.x, y: frame.minY + $0.y, width: 0, height: 0) } ?? frame
                AnchoredPanel(anchor: anchor, close: close) {
                    MenuPanel(items: items(), dismiss: close)
                }
            }
    }
}

/// Places a floating panel beside `anchor` (global coordinates), flipping to stay on screen;
/// a tap anywhere else closes it.
struct AnchoredPanel<Panel: View>: View {
    let anchor: CGRect
    let close: () -> Void
    @ViewBuilder let panel: () -> Panel

    var body: some View {
        GeometryReader { geo in
            let space = geo.frame(in: .global)
            let a = anchor.offsetBy(dx: -space.minX, dy: -space.minY)
            let below = a.maxY < space.height * 0.62
            let leading = a.midX < space.width * 0.6
            ZStack(alignment: Alignment(horizontal: leading ? .leading : .trailing, vertical: below ? .top : .bottom)) {
                Color.clear.contentShape(Rectangle()).onTapGesture(perform: close)
                panel()
                    .background(Palette.bubbleAgent, in: RoundedRectangle(cornerRadius: 14, style: .continuous))
                    .shadow(color: .black.opacity(0.25), radius: 20, y: 8)
                    .offset(x: leading ? max(8, a.minX) : -max(8, space.width - a.maxX),
                            y: below ? a.maxY + 6 : -(space.height - a.minY + 6))
            }
        }
        .ignoresSafeArea()
    }
}

/// A button that opens a Codync menu.
public struct DropdownMenu<Label: View>: View {
    let items: () -> [MenuItem]
    let label: Label
    @State private var open = false

    public init(items: @escaping () -> [MenuItem], @ViewBuilder label: () -> Label) {
        self.items = items
        self.label = label()
    }

    public var body: some View {
        Button { open.toggle() } label: { label }
            .buttonStyle(.plain)
            .codyncMenu(isPresented: $open, items: items)
    }
}

/// Picks one value: shows the current choice in a pill with a chevron, opens a Codync menu.
public struct ChoicePicker<ID: Hashable>: View {
    @Binding var selection: ID
    let options: [(id: ID, label: String)]
    var fill: Color

    public init(selection: Binding<ID>, options: [(id: ID, label: String)], fill: Color = Palette.bubbleUser) {
        _selection = selection
        self.options = options
        self.fill = fill
    }

    public var body: some View {
        DropdownMenu {
            options.map { option in MenuItem(option.label, selected: option.id == selection) { selection = option.id } }
        } label: {
            HStack(spacing: 6) {
                Text(options.first { $0.id == selection }?.label ?? "Choose").lineLimit(1)
                Image(systemName: "chevron.down").font(.caption2.weight(.semibold)).foregroundStyle(Palette.secondary)
            }
            .font(InterfaceMetrics.body)
            .foregroundStyle(Palette.text)
            .pill(fill: fill)
        }
        .fixedSize()
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
                        .font(InterfaceMetrics.secondary.weight(.medium))
                        .foregroundStyle(on ? Palette.text : Palette.secondary)
                        .lineLimit(1)
                        .frame(maxWidth: .infinity)
                        .padding(.vertical, InterfaceMetrics.value(mac: 5, mobile: 8))
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
        VStack(alignment: .leading, spacing: InterfaceMetrics.value(mac: 10, mobile: 14)) {
            ForEach(options, id: \.id) { option in
                Button { selection = option.id } label: {
                    HStack(spacing: 10) {
                        VStack(alignment: .leading, spacing: 2) {
                            Text(option.label).foregroundStyle(Palette.text)
                            if let detail = option.detail, !detail.isEmpty {
                                Text(detail).font(InterfaceMetrics.secondary).foregroundStyle(Palette.secondary)
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
            VStack(alignment: .leading, spacing: InterfaceMetrics.value(mac: 14, mobile: 22)) {
                content
            }
            .font(InterfaceMetrics.body)
            .padding(InterfaceMetrics.value(mac: 14, mobile: 20))
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        .scrollDismissesKeyboard(.interactively)
        .background(Palette.background)
    }
}

/// A titled card of rows: the replacement for `Section`.
public struct CardSection<Content: View>: View {
    let title: String?
    let footer: String?
    let content: Content

    public init(_ title: String? = nil, footer: String? = nil, @ViewBuilder content: () -> Content) {
        self.title = title
        self.footer = footer
        self.content = content()
    }

    public var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            if let title {
                Text(title).font(InterfaceMetrics.secondary).foregroundStyle(Palette.secondary).padding(.leading, 4)
            }
            VStack(alignment: .leading, spacing: InterfaceMetrics.value(mac: 12, mobile: 16)) {
                content
            }
            .padding(.horizontal, InterfaceMetrics.value(mac: 12, mobile: 16))
            .padding(.vertical, InterfaceMetrics.value(mac: 12, mobile: 16))
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(Palette.bubbleAgent, in: RoundedRectangle(cornerRadius: 18, style: .continuous))
            if let footer {
                Text(footer).font(.caption).foregroundStyle(Palette.tertiary).padding(.horizontal, 4)
            }
        }
    }
}

/// Label on the left, value on the right: the replacement for `LabeledContent`.
public struct ValueRow<Value: View>: View {
    let label: String
    let value: Value

    public init(_ label: String, @ViewBuilder value: () -> Value) {
        self.label = label
        self.value = value()
    }

    public init(_ label: String, value: String) where Value == Text {
        self.label = label
        self.value = Text(value)
    }

    public var body: some View {
        HStack {
            Text(label).foregroundStyle(Palette.text)
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
        .font(InterfaceMetrics.body)
        .padding(.horizontal, InterfaceMetrics.value(mac: 10, mobile: 14))
        .padding(.vertical, InterfaceMetrics.value(mac: 7, mobile: 10))
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

// MARK: - Dialogs

/// A button in a Codync dialog.
public struct DialogAction: Identifiable {
    public let id = UUID()
    public var title: String
    public var destructive: Bool
    public var action: () -> Void

    public init(_ title: String, destructive: Bool = false, action: @escaping () -> Void) {
        self.title = title
        self.destructive = destructive
        self.action = action
    }
}

public extension View {
    /// A centered card over a dimmed screen with the actions and Cancel:
    /// the replacement for `confirmationDialog` and `alert`.
    func codyncDialog(_ title: String, isPresented: Binding<Bool>, message: String? = nil,
                      cancel: String? = "Cancel", actions: @escaping () -> [DialogAction]) -> some View {
        modifier(CodyncDialog(title: title, message: message, cancel: cancel, isPresented: isPresented, actions: actions))
    }
}

private struct CodyncDialog: ViewModifier {
    let title: String
    let message: String?
    let cancel: String?
    @Binding var isPresented: Bool
    let actions: () -> [DialogAction]

    func body(content: Content) -> some View {
        content.codyncOverlay(isPresented: $isPresented) { close in
            DialogCard(title: title, message: message, cancel: cancel, actions: actions(), dismiss: close)
        }
    }
}

private struct DialogCard: View {
    let title: String
    let message: String?
    let cancel: String?
    let actions: [DialogAction]
    let dismiss: () -> Void

    var body: some View {
        ZStack {
            Color.black.opacity(0.35).ignoresSafeArea()
                .onTapGesture { if cancel != nil { dismiss() } }
            VStack(spacing: 14) {
                VStack(spacing: 6) {
                    Text(title).font(.headline).foregroundStyle(Palette.text)
                    if let message, !message.isEmpty {
                        Text(message).font(InterfaceMetrics.secondary).foregroundStyle(Palette.secondary)
                    }
                }
                .multilineTextAlignment(.center)
                VStack(spacing: 8) {
                    ForEach(actions) { action in
                        Button {
                            dismiss()
                            action.action()
                        } label: {
                            Text(action.title).frame(maxWidth: .infinity)
                        }
                        .buttonStyle(DialogButtonStyle(destructive: action.destructive, prominent: true))
                    }
                    if let cancel {
                        Button(action: dismiss) { Text(cancel).frame(maxWidth: .infinity) }
                            .buttonStyle(DialogButtonStyle(destructive: false, prominent: false))
                            .keyboardShortcut(.cancelAction)
                    }
                }
            }
            .padding(18)
            .frame(maxWidth: 320)
            .background(Palette.bubbleAgent, in: RoundedRectangle(cornerRadius: 22, style: .continuous))
            .shadow(color: .black.opacity(0.25), radius: 24, y: 10)
            .padding(24)
        }
        .accessibilityAddTraits(.isModal)
    }
}

private struct DialogButtonStyle: ButtonStyle {
    let destructive: Bool
    let prominent: Bool

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(InterfaceMetrics.body.weight(.medium))
            .foregroundStyle(destructive ? Color.white : (prominent ? Palette.onAccent : Palette.text))
            .padding(.vertical, InterfaceMetrics.value(mac: 8, mobile: 12))
            .background(destructive ? Palette.danger : (prominent ? Palette.accentFill : Palette.bubbleUser), in: Capsule())
            .opacity(configuration.isPressed ? 0.85 : 1)
            .scaleEffect(configuration.isPressed ? Motion.pressScale : 1)
            .animation(Motion.press, value: configuration.isPressed)
            .contentShape(Capsule())
    }
}

// MARK: - Shared bits

extension View {
    /// The value pill: a filled rounded box, no border.
    func pill(fill: Color = Palette.background) -> some View {
        padding(.horizontal, InterfaceMetrics.value(mac: 9, mobile: 12))
            .padding(.vertical, InterfaceMetrics.value(mac: 5, mobile: 7))
            .background(fill, in: RoundedRectangle(cornerRadius: 10, style: .continuous))
    }
}

#if os(macOS)
/// Intercepts only secondary clicks; primary clicks, scrolling and dragging pass through.
public struct SecondaryClickCapture: NSViewRepresentable {
    let onClick: (CGPoint) -> Void

    public init(onClick: @escaping (CGPoint) -> Void) { self.onClick = onClick }

    public func makeNSView(context: Context) -> CaptureView { CaptureView(onClick: onClick) }
    public func updateNSView(_ view: CaptureView, context: Context) { view.onClick = onClick }

    public final class CaptureView: NSView {
        var onClick: (CGPoint) -> Void
        override public var isFlipped: Bool { true }

        init(onClick: @escaping (CGPoint) -> Void) {
            self.onClick = onClick
            super.init(frame: .zero)
        }
        required init?(coder: NSCoder) { fatalError("init(coder:) has not been implemented") }

        override public func hitTest(_ point: NSPoint) -> NSView? {
            guard let event = NSApp.currentEvent,
                  event.type == .rightMouseDown || (event.type == .leftMouseDown && event.modifierFlags.contains(.control))
            else { return nil }
            return super.hitTest(point)
        }
        override public func rightMouseDown(with event: NSEvent) { onClick(convert(event.locationInWindow, from: nil)) }
        override public func mouseDown(with event: NSEvent) { onClick(convert(event.locationInWindow, from: nil)) }
    }
}
#endif
