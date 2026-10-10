import CodyncKit
import SwiftUI
import UIKit

// Codync's own screen chrome: modals, headers, icon buttons and the iPhone tab bar.
// Present with `.codyncSheet`, title with `ModalHeader` / `ScreenHeader`.
// The iPhone bot list and conversation use the native navigation toolbar for Liquid Glass.
// Every transition animates (Motion.layout / Motion.fade), honoring Reduce Motion.

// MARK: - Icon button

/// Plain icon button that lights up on hover and press. Icon-only, labelled for VoiceOver.
public struct IconButton: View {
    let title: String
    let systemImage: String
    var selected: Bool
    let action: () -> Void

    public init(_ title: String, systemImage: String, selected: Bool = false, action: @escaping () -> Void) {
        self.title = title
        self.systemImage = systemImage
        self.selected = selected
        self.action = action
    }

    public var body: some View {
        Button(title, systemImage: systemImage, action: action)
            .labelStyle(.iconOnly)
            .buttonStyle(IconButtonStyle(selected: selected))
            .help(title)
    }
}

public struct IconButtonStyle: ButtonStyle {
    var selected: Bool
    var size: CGFloat

    public init(selected: Bool = false, size: CGFloat? = nil) {
        self.selected = selected
        self.size = size ?? 36
    }

    public func makeBody(configuration: Configuration) -> some View {
        IconButtonBody(configuration: configuration, selected: selected, size: size)
    }

    private struct IconButtonBody: View {
        let configuration: Configuration
        let selected: Bool
        let size: CGFloat
        @State private var hovering = false
        @Environment(\.isEnabled) private var isEnabled

        var body: some View {
            configuration.label
                .font(.system(size: size / 2, weight: .medium))
                .foregroundStyle(selected ? Palette.onAccent : hovering ? Palette.text : Palette.secondary)
                .frame(width: size, height: size)
                .background(
                    selected ? Palette.accentFill : Palette.text.opacity(configuration.isPressed ? 0.14 : hovering ? 0.08 : 0),
                    in: RoundedRectangle(cornerRadius: size * 0.28, style: .continuous)
                )
                .opacity(isEnabled ? 1 : 0.35)
                .scaleEffect(configuration.isPressed ? Motion.pressScale : 1)
                .animation(Motion.press, value: configuration.isPressed)
                .animation(Motion.hover, value: hovering)
                .animation(Motion.hover, value: selected)
                .contentShape(Rectangle())
                .onHover { hovering = $0 }
        }
    }
}

// MARK: - Headers

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
        self.init(title: { Text(title) }, trailing: trailing)
    }
}

public extension ModalHeader where Title == Text, Trailing == EmptyView {
    init(_ title: String) { self.init(title) { EmptyView() } }
}

public extension ModalHeader where Trailing == EmptyView {
    init(@ViewBuilder title: () -> Title) { self.init(title: title) { EmptyView() } }
}

/// A full screen's top bar (replaces the navigation bar): leading, centered title, trailing.
public struct ScreenHeader<Leading: View, Title: View, Trailing: View>: View {
    let leading: Leading
    let title: Title
    let trailing: Trailing

    public init(@ViewBuilder leading: () -> Leading, @ViewBuilder title: () -> Title, @ViewBuilder trailing: () -> Trailing) {
        self.leading = leading()
        self.title = title()
        self.trailing = trailing()
    }

    public var body: some View {
        ZStack {
            title.frame(maxWidth: .infinity)
            HStack(spacing: 6) {
                leading
                Spacer(minLength: 8)
                trailing
            }
        }
        .padding(.horizontal, 12)
        .frame(height: 52)
        .background(Palette.background)
    }
}

/// The back button for a pushed screen.
public struct BackButton: View {
    let action: () -> Void

    public init(action: @escaping () -> Void) { self.action = action }

    public var body: some View {
        IconButton("Back", systemImage: "chevron.left", action: action)
    }
}

// MARK: - Modals

/// Closes the modal the view is in (the replacement for `\.dismiss` inside `.codyncSheet`).
public struct DismissModalAction: Sendable {
    let action: @MainActor @Sendable () -> Void

    public init(_ action: @escaping @MainActor @Sendable () -> Void) { self.action = action }

    @MainActor public func callAsFunction() { action() }
}

public extension EnvironmentValues {
    @Entry var dismissModal = DismissModalAction {}
}

public extension View {
    /// Presents a Codync modal as the system bottom sheet.
    /// Content titles itself with `ModalHeader` and closes with `\.dismissModal`.
    func codyncSheet<Sheet: View>(isPresented: Binding<Bool>, @ViewBuilder content: @escaping () -> Sheet) -> some View {
        modifier(CodyncSheet(isPresented: isPresented, sheet: content))
    }

    func codyncSheet<Item: Identifiable, Sheet: View>(item: Binding<Item?>, @ViewBuilder content: @escaping (Item) -> Sheet) -> some View {
        modifier(CodyncItemSheet(item: item, sheet: content))
    }

    /// Presents a full-window layer the content draws itself (menus, dialogs), fading in and out.
    /// `close` animates it away.
    func codyncOverlay<Layer: View>(isPresented: Binding<Bool>, @ViewBuilder content: @escaping (_ close: @escaping () -> Void) -> Layer) -> some View {
        modifier(CodyncOverlay(isPresented: isPresented, layer: content))
    }

}

/// Keeps the last item on screen while the sheet slides away after the item is cleared.
private struct CodyncItemSheet<Item: Identifiable, Sheet: View>: ViewModifier {
    @Binding var item: Item?
    let sheet: (Item) -> Sheet
    @State private var last: Item?

    func body(content: Content) -> some View {
        let shown = Binding(get: { item != nil }, set: { if !$0 { item = nil } })
        content
            .modifier(CodyncSheet(isPresented: shown, sheet: { (item ?? last).map(sheet) }))
            .onChange(of: item?.id, initial: true) { if item != nil { last = item } }
    }
}

/// The system sheet: its own slide, grabber and swipe down, nothing hand-built.
private struct CodyncSheet<Sheet: View>: ViewModifier {
    @Binding var isPresented: Bool
    let sheet: () -> Sheet

    func body(content: Content) -> some View {
        content.sheet(isPresented: $isPresented) {
            sheet()
                .environment(\.dismissModal, DismissModalAction { isPresented = false })
                .presentationDragIndicator(.visible)
        }
    }
}

private struct CodyncOverlay<Layer: View>: ViewModifier {
    @Binding var isPresented: Bool
    let layer: (_ close: @escaping () -> Void) -> Layer

    func body(content: Content) -> some View {
        content
            .fullScreenCover(isPresented: $isPresented) {
                FadeLayer(close: { isPresented = false }, layer: layer)
                    .presentationBackground(.clear)
            }
            .transaction(value: isPresented) { $0.disablesAnimations = true }
    }
}

private struct FadeLayer<Layer: View>: View {
    let close: () -> Void
    let layer: (_ close: @escaping () -> Void) -> Layer
    @State private var shown = false
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        GeometryReader { geometry in
            layer(animateClose)
                .environment(\.dismissModal, DismissModalAction { animateClose() })
                // Preserve safe areas for controls (including the screen viewer's toolbar),
                // but composite the fade over the whole screen so the scrim cannot be clipped.
                .safeAreaPadding(geometry.safeAreaInsets)
                .frame(maxWidth: .infinity, maxHeight: .infinity)
                .opacity(shown ? 1 : 0)
                .ignoresSafeArea(.container)
        }
        .onAppear { withAnimation(Motion.reduced(Motion.layout, reduceMotion)) { shown = true } }
    }

    private func animateClose() {
        withAnimation(Motion.reduced(Motion.fade, reduceMotion)) {
            shown = false
        } completion: {
            var t = Transaction()
            t.disablesAnimations = true
            withTransaction(t) { close() }
        }
    }
}

/// Keeps the edge swipe back working on screens whose navigation bar we replaced.
public struct SwipeBackEnabler: UIViewControllerRepresentable {
    public init() {}

    public func makeUIViewController(context: Context) -> Controller { Controller() }
    public func updateUIViewController(_ controller: Controller, context: Context) {}

    public final class Controller: UIViewController, UIGestureRecognizerDelegate {
        override public func didMove(toParent parent: UIViewController?) {
            super.didMove(toParent: parent)
            navigationController?.interactivePopGestureRecognizer?.delegate = self
        }

        public func gestureRecognizerShouldBegin(_ gestureRecognizer: UIGestureRecognizer) -> Bool {
            (navigationController?.viewControllers.count ?? 0) > 1
        }
    }
}

public extension View {
    /// Hides the system navigation bar (we draw `ScreenHeader`) and keeps swipe-back.
    func hidesSystemNavigationBar() -> some View {
        toolbar(.hidden, for: .navigationBar)
            .background(SwipeBackEnabler().frame(width: 0, height: 0))
    }
}

// MARK: - Tab bar

/// The iPhone's bottom bar (replaces `TabView`): a floating capsule of icon tabs.
public struct TabBar<ID: Hashable>: View {
    @Binding var selection: ID
    let tabs: [(id: ID, title: String, icon: String)]
    @Namespace private var thumb
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    public init(selection: Binding<ID>, tabs: [(id: ID, title: String, icon: String)]) {
        _selection = selection
        self.tabs = tabs
    }

    public var body: some View {
        HStack(spacing: 4) {
            ForEach(tabs, id: \.id) { tab in
                let on = tab.id == selection
                Button {
                    withAnimation(Motion.reduced(Motion.morph, reduceMotion)) { selection = tab.id }
                } label: {
                    VStack(spacing: 3) {
                        Image(systemName: tab.icon).font(.system(size: 18, weight: .medium))
                        Text(tab.title).font(.caption2.weight(.medium))
                    }
                    .foregroundStyle(on ? Palette.text : Palette.secondary)
                    .frame(width: 76, height: 50)
                    .background {
                        if on {
                            Capsule().fill(Palette.text.opacity(0.08)).matchedGeometryEffect(id: "tab", in: thumb)
                        }
                    }
                    .contentShape(Capsule())
                }
                .buttonStyle(PressScale())
                .accessibilityAddTraits(on ? .isSelected : [])
            }
        }
        .padding(5)
        .glass(in: Capsule())
    }
}
