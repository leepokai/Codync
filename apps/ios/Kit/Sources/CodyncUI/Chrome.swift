import CodyncKit
import SwiftUI
import UIKit

// Codync's own screen chrome: modals, headers and icon buttons.
// Present with `.codyncSheet`, title with `ModalHeader` / `ScreenHeader`.
// The iPhone bot list and conversation use the native navigation toolbar for Liquid Glass.
// Every transition animates (Motion.layout / Motion.fade), honoring Reduce Motion.

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
        self.init(title: { Text(L10n.tr(title)) }, trailing: trailing)
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

    /// Presents a full-window layer the content draws itself (dialogs, the remote screen), fading in and out.
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
@MainActor private struct CodyncSheet<Sheet: View>: ViewModifier {
    @Binding var isPresented: Bool
    let sheet: () -> Sheet
    @State private var held = Held<() -> Sheet>()

    func body(content: Content) -> some View {
        let _ = (held.value = sheet)
        content.quietPresentation(shown: isPresented) {
            $0.sheet(isPresented: $isPresented) {
                HeldView { held.value?() }
                    .environment(\.dismissModal, DismissModalAction { isPresented = false })
                    .presentationDragIndicator(.visible)
            }
        }
    }
}

@MainActor private struct CodyncOverlay<Layer: View>: ViewModifier {
    @Binding var isPresented: Bool
    let layer: (_ close: @escaping () -> Void) -> Layer
    @State private var held = Held<(_ close: @escaping () -> Void) -> Layer>()

    func body(content: Content) -> some View {
        let _ = (held.value = layer)
        content.quietPresentation(shown: isPresented) {
            $0.fullScreenCover(isPresented: $isPresented) {
                FadeLayer(close: { isPresented = false }, layer: { close in HeldView { held.value?(close) } })
                    .presentationBackground(.clear)
            }
            .transaction(value: isPresented) { $0.disablesAnimations = true }
        }
    }
}

extension View {
    /// Applies a presentation modifier (`.sheet`, `.fullScreenCover`, `.photosPicker`) that runs again only when what it
    /// presents opens or closes. Any of them running again re-sends the content of whatever is presented, which resolves
    /// the icons of a system menu open there anew and so rebuilds it, and a press during a rebuild is lost. So they can't
    /// run with every render of the screen behind. Directly, only for what the system draws (`.photosPicker`): content of
    /// our own would freeze here, so it goes through `.codyncSheet` or `.codyncOverlay`, which keep it live with `Held`.
    func quietPresentation<Presenter: View>(shown: Bool, @ViewBuilder _ presenter: @escaping (Color) -> Presenter) -> some View {
        background { QuietPresenter(shown: shown, presenter: presenter).equatable() }
    }
}

private struct QuietPresenter<Presenter: View>: View, Equatable {
    let shown: Bool
    let presenter: (Color) -> Presenter

    nonisolated static func == (lhs: Self, rhs: Self) -> Bool { lhs.shown == rhs.shown }

    var body: some View { presenter(.clear) }
}

/// What's presented, as of the latest render of the screen behind it.
@MainActor @Observable private final class Held<Value> {
    var value: Value?
}

/// Reads `Held` in a body of its own, which is what updates when it changes.
private struct HeldView<Content: View>: View {
    @ViewBuilder let content: () -> Content

    var body: some View { content() }
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

/// A quiet "Beta" pill beside a feature still in beta (Network, Codync Cloud).
public struct BetaTag: View {
    public init() {}

    public var body: some View {
        Text(L10n.tr("Beta"))
            .font(.caption.weight(.semibold))
            .foregroundStyle(Palette.secondary)
            .padding(.horizontal, 7)
            .padding(.vertical, 2)
            .background(Palette.bubbleAgent, in: Capsule())
    }
}
