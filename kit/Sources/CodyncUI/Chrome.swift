import CodyncKit
import SwiftUI
#if os(iOS)
import UIKit
#endif

// Codync's own screen chrome: modals, headers, icon buttons and the iPhone tab bar.
// Never use `.sheet`, `.popover`, `.toolbar`/`ToolbarItem`, navigation bars or
// `TabView`: present with `.codyncSheet`, title with `ModalHeader` / `ScreenHeader`.
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
        self.size = size ?? InterfaceMetrics.value(mac: 28, mobile: 36)
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
public struct ModalHeader<Trailing: View>: View {
    let title: String
    let trailing: Trailing
    @Environment(\.dismissModal) private var dismiss

    public init(_ title: String, @ViewBuilder trailing: () -> Trailing) {
        self.title = title
        self.trailing = trailing()
    }

    public var body: some View {
        HStack(spacing: 8) {
            Text(title)
                .font(InterfaceMetrics.body.weight(.semibold))
                .foregroundStyle(Palette.text)
                .lineLimit(1)
            Spacer(minLength: 8)
            trailing
            IconButton("Close", systemImage: "xmark") { dismiss() }
                .keyboardShortcut(.cancelAction)
        }
        .padding(.leading, InterfaceMetrics.value(mac: 16, mobile: 20))
        .padding(.trailing, InterfaceMetrics.value(mac: 10, mobile: 12))
        .frame(height: InterfaceMetrics.value(mac: 48, mobile: 56))
    }
}

public extension ModalHeader where Trailing == EmptyView {
    init(_ title: String) { self.init(title) { EmptyView() } }
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
        .padding(.horizontal, InterfaceMetrics.value(mac: 12, mobile: 12))
        .frame(height: InterfaceMetrics.value(mac: 44, mobile: 52))
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
    /// Presents a Codync modal: a centered card on the Mac, a bottom sheet on iPhone.
    /// Content titles itself with `ModalHeader` and closes with `\.dismissModal`.
    func codyncSheet<Sheet: View>(isPresented: Binding<Bool>, @ViewBuilder content: @escaping () -> Sheet) -> some View {
        modifier(CodyncSheet(isPresented: isPresented, sheet: content))
    }

    func codyncSheet<Item: Identifiable, Sheet: View>(item: Binding<Item?>, @ViewBuilder content: @escaping (Item) -> Sheet) -> some View {
        let shown = Binding(get: { item.wrappedValue != nil }, set: { if !$0 { item.wrappedValue = nil } })
        return modifier(CodyncSheet(isPresented: shown, sheet: { item.wrappedValue.map(content) }))
    }

    /// Presents a full-window layer the content draws itself (menus, dialogs), fading in and out.
    /// `close` animates it away.
    func codyncOverlay<Layer: View>(isPresented: Binding<Bool>, @ViewBuilder content: @escaping (_ close: @escaping () -> Void) -> Layer) -> some View {
        modifier(CodyncOverlay(isPresented: isPresented, layer: content))
    }

    /// Mac: the window root that hosts every `.codyncSheet` below it, so modals cover the
    /// whole window rather than the column that opened them.
    @ViewBuilder func modalHost() -> some View {
        #if os(macOS)
        modifier(ModalHostModifier())
        #else
        self
        #endif
    }
}

#if os(iOS)
private struct CodyncSheet<Sheet: View>: ViewModifier {
    @Binding var isPresented: Bool
    let sheet: () -> Sheet

    func body(content: Content) -> some View {
        content
            .fullScreenCover(isPresented: $isPresented) {
                BottomSheet(close: { isPresented = false }, content: sheet)
                    .presentationBackground(.clear)
            }
            // Our own slide replaces the system one.
            .transaction(value: isPresented) { $0.disablesAnimations = true }
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
        layer(animateClose)
            .opacity(shown ? 1 : 0)
            .scaleEffect(shown ? 1 : 0.98)
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

/// The iPhone modal: slides up over a dim, drags down (from the grabber) to close.
private struct BottomSheet<Content: View>: View {
    let close: () -> Void
    @ViewBuilder let content: () -> Content
    @State private var shown = false
    @State private var drag: CGFloat = 0
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        GeometryReader { geo in
            let height = geo.size.height + geo.safeAreaInsets.bottom
            ZStack(alignment: .bottom) {
                Color.black.opacity(shown ? 0.4 : 0)
                    .ignoresSafeArea()
                    .onTapGesture(perform: animateClose)
                    .accessibilityHidden(true)
                VStack(spacing: 0) {
                    Capsule().fill(Palette.tertiary.opacity(0.6)).frame(width: 36, height: 5)
                        .frame(maxWidth: .infinity, minHeight: 22)
                        .contentShape(Rectangle())
                        .gesture(dragToClose)
                        .accessibilityHidden(true)
                    content()
                        .environment(\.dismissModal, DismissModalAction { animateClose() })
                        .frame(maxWidth: .infinity, maxHeight: .infinity)
                }
                .background(Palette.background)
                .clipShape(UnevenRoundedRectangle(topLeadingRadius: 28, topTrailingRadius: 28, style: .continuous))
                .padding(.top, 10)
                .offset(y: shown ? drag : height)
                .ignoresSafeArea(.container, edges: .bottom)
            }
        }
        .onAppear { withAnimation(Motion.reduced(Motion.layout, reduceMotion)) { shown = true } }
        .accessibilityAction(.escape, animateClose)
    }

    private var dragToClose: some Gesture {
        DragGesture()
            .onChanged { drag = max(0, $0.translation.height) }
            .onEnded { value in
                if value.translation.height > 120 || value.predictedEndTranslation.height > 320 {
                    animateClose()
                } else {
                    withAnimation(Motion.layout) { drag = 0 }
                }
            }
    }

    private func animateClose() {
        withAnimation(Motion.reduced(Motion.layout, reduceMotion)) {
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
#else
@MainActor @Observable
final class ModalHost {
    enum Chrome { case card, bare }

    struct Entry: Identifiable {
        let id: UUID
        var chrome: Chrome
        var content: AnyView
        var shown = false
    }

    var entries: [Entry] = []

    func present(_ id: UUID, chrome: Chrome, _ content: AnyView) {
        if let index = entries.firstIndex(where: { $0.id == id }) {
            entries[index].content = content
            if !entries[index].shown { withAnimation(Motion.layout) { entries[index].shown = true } }
            return
        }
        entries.append(Entry(id: id, chrome: chrome, content: content))
        withAnimation(Motion.layout) {
            if let index = entries.firstIndex(where: { $0.id == id }) { entries[index].shown = true }
        }
    }

    func close(_ id: UUID) {
        guard let index = entries.firstIndex(where: { $0.id == id }), entries[index].shown else { return }
        withAnimation(Motion.layout) {
            entries[index].shown = false
        } completion: {
            self.entries.removeAll { $0.id == id && !$0.shown }
        }
    }
}

extension EnvironmentValues {
    @Entry var modalHost: ModalHost?
}

private struct ModalHostModifier: ViewModifier {
    @State private var host = ModalHost()

    func body(content: Content) -> some View {
        content
            .environment(\.modalHost, host)
            .overlay {
                ZStack {
                    ForEach(host.entries) { entry in
                        switch entry.chrome {
                        case .card: ModalCard(shown: entry.shown) { entry.content }
                        case .bare: entry.content.opacity(entry.shown ? 1 : 0).scaleEffect(entry.shown ? 1 : 0.98)
                        }
                    }
                }
            }
    }
}

/// The Mac modal: a centered card over a dim; the content sizes it.
private struct ModalCard<Content: View>: View {
    let shown: Bool
    @ViewBuilder let content: () -> Content

    var body: some View {
        ZStack {
            Color.black.opacity(shown ? 0.4 : 0)
                .ignoresSafeArea()
                .contentShape(Rectangle())
                .onTapGesture {}
                .accessibilityHidden(true)
            content()
                .background(Palette.background)
                .clipShape(RoundedRectangle(cornerRadius: 20, style: .continuous))
                .shadow(color: .black.opacity(0.3), radius: 30, y: 12)
                .scaleEffect(shown ? 1 : 0.96)
                .opacity(shown ? 1 : 0)
                .padding(24)
        }
        .accessibilityAddTraits(.isModal)
    }
}

/// Hands a presentation to the window's `ModalHost`, carrying the stores the content reads.
private struct HostedPresentation<Layer: View>: ViewModifier {
    @Binding var isPresented: Bool
    let chrome: ModalHost.Chrome
    let layer: (_ close: @escaping () -> Void) -> Layer
    @Environment(\.modalHost) private var host
    @Environment(BotStore.self) private var store: BotStore?
    @Environment(AccountStore.self) private var accounts: AccountStore?
    @State private var id = UUID()

    func body(content: Content) -> some View {
        content
            .onChange(of: isPresented, initial: true) { _, _ in sync() }
            .onDisappear { host?.close(id) }
    }

    private func sync() {
        guard let host else { return }
        guard isPresented else { return host.close(id) }
        let binding = $isPresented
        let close = { binding.wrappedValue = false }
        // ponytail: the content is captured when it opens; state it reads through
        // stores and bindings stays live, plain values passed in don't refresh.
        host.present(id, chrome: chrome, AnyView(
            layer(close)
                .environment(store)
                .environment(accounts)
                .environment(\.modalHost, host)
                .environment(\.dismissModal, DismissModalAction { binding.wrappedValue = false })
        ))
    }
}

private struct CodyncSheet<Sheet: View>: ViewModifier {
    @Binding var isPresented: Bool
    let sheet: () -> Sheet

    func body(content: Content) -> some View {
        content.modifier(HostedPresentation(isPresented: $isPresented, chrome: .card) { _ in sheet() })
    }
}

private struct CodyncOverlay<Layer: View>: ViewModifier {
    @Binding var isPresented: Bool
    let layer: (_ close: @escaping () -> Void) -> Layer

    func body(content: Content) -> some View {
        content.modifier(HostedPresentation(isPresented: $isPresented, chrome: .bare, layer: layer))
    }
}

public extension View {
    func hidesSystemNavigationBar() -> some View { self }
}
#endif

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
