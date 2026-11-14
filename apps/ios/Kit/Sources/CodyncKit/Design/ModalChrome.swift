import SwiftUI

// The modal chrome shared by the app and its extensions: icon buttons, a modal's title row, and closing it.
// The app presents modals with `.codyncSheet` (CodyncUI); a share extension's sheet is the system's own.

// MARK: - Icon button

/// Plain icon button that lights up on hover and press. Icon-only, labelled for VoiceOver.
public struct IconButton: View {
    let title: String
    let systemImage: String
    let action: () -> Void

    public init(_ title: String, systemImage: String, action: @escaping () -> Void) {
        self.title = title
        self.systemImage = systemImage
        self.action = action
    }

    public var body: some View {
        Button(L10n.tr(title), systemImage: systemImage, action: action)
            .labelStyle(.iconOnly)
            .buttonStyle(IconButtonStyle())
            .help(L10n.tr(title))
    }
}

public struct IconButtonStyle: ButtonStyle {
    private let size: CGFloat = 36

    public init() {}

    public func makeBody(configuration: Configuration) -> some View {
        IconButtonBody(configuration: configuration, size: size)
    }

    private struct IconButtonBody: View {
        let configuration: Configuration
        let size: CGFloat
        @State private var hovering = false
        @Environment(\.isEnabled) private var isEnabled

        var body: some View {
            configuration.label
                .font(.system(size: size / 2, weight: .medium))
                .foregroundStyle(hovering ? Palette.text : Palette.secondary)
                .frame(width: size, height: size)
                .background(
                    Palette.text.opacity(configuration.isPressed ? 0.14 : hovering ? 0.08 : 0),
                    in: RoundedRectangle(cornerRadius: size * 0.28, style: .continuous)
                )
                .opacity(isEnabled ? 1 : 0.35)
                .scaleEffect(configuration.isPressed ? Motion.pressScale : 1)
                .animation(Motion.press, value: configuration.isPressed)
                .animation(Motion.hover, value: hovering)
                .contentShape(Rectangle())
                .onHover { hovering = $0 }
        }
    }
}

// MARK: - Modal header

/// A modal's title row: title, optional trailing actions, and a close button.
public struct ModalHeader<Title: View, Trailing: View>: View {
    let title: Title
    let trailing: Trailing
    @Environment(\.dismissModal) private var dismiss

    /// A header whose title is a view (a bot pair, say), set in the header's font.
    public init(@ViewBuilder title: () -> Title, @ViewBuilder trailing: () -> Trailing) {
        self.title = title()
        self.trailing = trailing()
    }

    public var body: some View {
        HStack(spacing: 8) {
            title
                .font(.body.weight(.semibold))
                .foregroundStyle(Palette.text)
                .lineLimit(1)
            Spacer(minLength: 8)
            trailing
            IconButton("Close", systemImage: "xmark") { dismiss() }
                .keyboardShortcut(.cancelAction)
        }
        .padding(.leading, 20)
        .padding(.trailing, 12)
        .frame(height: 56)
    }
}

public extension ModalHeader where Title == Text {
    init(_ title: String, @ViewBuilder trailing: () -> Trailing) {
        self.init(title: { Text(L10n.tr(title)) }, trailing: trailing)
    }
}

public extension ModalHeader where Title == Text, Trailing == EmptyView {
    init(_ title: String) { self.init(title) { EmptyView() } }
}

public extension ModalHeader where Trailing == EmptyView {
    init(@ViewBuilder title: () -> Title) { self.init(title: title) { EmptyView() } }
}

/// Closes the modal the view is in (the replacement for `\.dismiss` inside `.codyncSheet`).
public struct DismissModalAction: Sendable {
    let action: @MainActor @Sendable () -> Void

    public init(_ action: @escaping @MainActor @Sendable () -> Void) { self.action = action }

    @MainActor public func callAsFunction() { action() }
}

public extension EnvironmentValues {
    @Entry var dismissModal = DismissModalAction {}
}
