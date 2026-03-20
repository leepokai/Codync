import SwiftUI
import Combine
import CodyncShared

// MARK: - Panel State

@MainActor
final class CodyncPanelState: ObservableObject {
    @Published var isExpanded = false
    @Published var isPinned = false
    @Published var headerSize: CGSize = .zero
    @Published var contentHeight: CGFloat = 0
    var collapsedRect: CGRect = .zero
    var panelRect: CGRect = .zero      // fixed size — used for hitTest
    var contentRect: CGRect = .zero    // dynamic — used for click-outside dismissal

    private let expandedWidth: CGFloat = 380
    private let expandedHeight: CGFloat = 500
    private var screenRef: NSScreen?

    func updateGeometry(for screen: NSScreen) {
        screenRef = screen
        headerSize = screen.notchSize
        recomputeRects()
    }

    func recomputeRects() {
        guard let screen = screenRef else { return }
        let centerX = screen.frame.origin.x + screen.frame.width / 2

        collapsedRect = CGRect(
            x: centerX - headerSize.width / 2,
            y: screen.frame.maxY - headerSize.height,
            width: headerSize.width,
            height: headerSize.height
        )

        // hitTest rect — always full size so clicks reach SwiftUI
        panelRect = CGRect(
            x: centerX - expandedWidth / 2,
            y: screen.frame.maxY - expandedHeight,
            width: expandedWidth,
            height: expandedHeight
        )

        // content rect — dynamic, for click-outside dismissal
        let h = contentHeight > 0 ? min(contentHeight, expandedHeight) : expandedHeight
        contentRect = CGRect(
            x: centerX - expandedWidth / 2,
            y: screen.frame.maxY - h,
            width: expandedWidth,
            height: h
        )
    }
}

// MARK: - Menu Bar Controller

@MainActor
final class MenuBarController: NSObject {
    private var statusItem: NSStatusItem!
    private let stateManager: SessionStateManager
    private var cancellables = Set<AnyCancellable>()

    private var panel: CodyncPanel?
    private var hitTestView: CodyncHitTestView?
    private let panelState = CodyncPanelState()
    private var mouseDownMonitor: EventMonitor?

    init(stateManager: SessionStateManager) {
        self.stateManager = stateManager
        super.init()
        if NSScreen.builtInOrMain.hasNotch {
            setupPanel()
        } else {
            setupStatusItem()
            setupPanel()
            observeSessionCount()
        }
        setupClickOutsideMonitor()
        observeScreenChanges()
        observePanelState()
    }

    // MARK: - Status Item

    private func setupStatusItem() {
        statusItem = NSStatusBar.system.statusItem(withLength: NSStatusItem.variableLength)
        if let button = statusItem.button {
            button.image = NSImage(systemSymbolName: "waveform.path", accessibilityDescription: "Codync")
            button.action = #selector(togglePanel)
            button.target = self
        }
    }

    private func observeSessionCount() {
        stateManager.$sessions
            .receive(on: RunLoop.main)
            .sink { [weak self] sessions in
                let activeCount = sessions.filter { $0.status.isActive }.count
                self?.updateBadge(count: activeCount)
            }
            .store(in: &cancellables)
    }

    private func updateBadge(count: Int) {
        guard let button = statusItem.button else { return }
        button.title = count > 0 ? " \(count)" : ""
    }

    // MARK: - Panel Setup

    private func setupPanel() {
        let screen = NSScreen.builtInOrMain
        panelState.updateGeometry(for: screen)

        let panel = CodyncPanel(frame: fullScreenTopFrame(for: screen))

        let contentView = CodyncPanelContentView(
            panelState: panelState,
            stateManager: stateManager
        )
        let hostingView = NSHostingView(rootView: contentView)

        let hitTestView = CodyncHitTestView()
        hitTestView.collapsedRect = panelState.collapsedRect
        hitTestView.panelRect = panelState.panelRect
        hitTestView.addSubview(hostingView)
        hostingView.translatesAutoresizingMaskIntoConstraints = false
        NSLayoutConstraint.activate([
            hostingView.topAnchor.constraint(equalTo: hitTestView.topAnchor),
            hostingView.bottomAnchor.constraint(equalTo: hitTestView.bottomAnchor),
            hostingView.leadingAnchor.constraint(equalTo: hitTestView.leadingAnchor),
            hostingView.trailingAnchor.constraint(equalTo: hitTestView.trailingAnchor),
        ])

        panel.contentView = hitTestView
        panel.orderFrontRegardless()

        self.panel = panel
        self.hitTestView = hitTestView
    }

    private func fullScreenTopFrame(for screen: NSScreen) -> NSRect {
        let height: CGFloat = 500
        return NSRect(
            x: screen.frame.origin.x,
            y: screen.frame.maxY - height,
            width: screen.frame.width,
            height: height
        )
    }

    @objc private func togglePanel() {
        panelState.isExpanded.toggle()
    }

    // MARK: - State Sync

    private func observePanelState() {
        panelState.$isExpanded
            .sink { [weak self] expanded in
                self?.hitTestView?.isExpanded = expanded
            }
            .store(in: &cancellables)

        panelState.$contentHeight
            .sink { [weak self] _ in
                guard let self else { return }
                self.panelState.recomputeRects()
                // hitTestView.panelRect stays fixed — only contentRect is dynamic
            }
            .store(in: &cancellables)

        NotificationCenter.default.addObserver(
            forName: .codePulseShouldCollapse,
            object: nil,
            queue: .main
        ) { [weak self] _ in
            Task { @MainActor in
                self?.panelState.isExpanded = false
            }
        }
    }

    // MARK: - Click Outside to Dismiss

    private func setupClickOutsideMonitor() {
        mouseDownMonitor = EventMonitor(mask: .leftMouseDown) { [weak self] _ in
            Task { @MainActor in
                self?.handleGlobalClick()
            }
        }
        mouseDownMonitor?.start()
    }

    private func handleGlobalClick() {
        guard panelState.isExpanded, !panelState.isPinned else { return }
        panelState.isExpanded = false
    }

    // MARK: - Screen Changes

    private func observeScreenChanges() {
        NotificationCenter.default.addObserver(
            self,
            selector: #selector(screenDidChange),
            name: NSApplication.didChangeScreenParametersNotification,
            object: nil
        )
    }

    @objc private func screenDidChange() {
        MainActor.assumeIsolated {
            let screen = NSScreen.builtInOrMain
            panelState.updateGeometry(for: screen)
            hitTestView?.collapsedRect = panelState.collapsedRect
            hitTestView?.panelRect = panelState.panelRect  // fixed full-size rect
            panel?.setFrame(fullScreenTopFrame(for: screen), display: true)
        }
    }
}

// MARK: - Hit Test View

/// Passes through clicks outside the active rect (collapsed header vs expanded panel).
private final class CodyncHitTestView: NSView {
    var isExpanded = false
    var collapsedRect: CGRect = .zero
    var panelRect: CGRect = .zero

    override func hitTest(_ point: NSPoint) -> NSView? {
        guard let window else { return nil }
        let screenPoint = window.convertPoint(toScreen: convert(point, to: nil))
        let activeRect = isExpanded ? panelRect : collapsedRect
        guard activeRect.contains(screenPoint) else { return nil }
        return super.hitTest(point)
    }
}

// MARK: - Panel Content View

/// The SwiftUI root view for the Codync panel.
/// Collapsed: invisible black shape matching the screen header area.
/// Expanded: shape extends downward with spring animation, revealing SessionListView.
private struct CodyncPanelContentView: View {
    @ObservedObject var panelState: CodyncPanelState
    @ObservedObject var stateManager: SessionStateManager
    @AppStorage("codync_darkMode") private var isDarkMode = true

    private var headerSize: CGSize { panelState.headerSize }
    private var isExpanded: Bool { panelState.isExpanded }

    private var panelAnimation: Animation {
        isExpanded
            ? .spring(response: 0.42, dampingFraction: 0.78)
            : .spring(response: 0.35, dampingFraction: 1.0)
    }

    private var topCornerRadius: CGFloat {
        isExpanded ? 19 : 6
    }

    private var bottomCornerRadius: CGFloat {
        isExpanded ? 24 : 14
    }

    var body: some View {
        VStack(spacing: 0) {
            // Header — matches screen notch height, tappable to expand
            ZStack {
                Color.clear
                HStack {
                    Spacer()
                    Image(systemName: "sparkle")
                        .font(.system(size: 16, weight: .medium))
                        .foregroundStyle(.white.opacity(0.9))
                        .padding(.trailing, 8)
                }
            }
            .frame(height: headerSize.height)
            .contentShape(Rectangle())
            .onTapGesture {
                panelState.isExpanded.toggle()
            }

            // Session list appears when expanded
            if isExpanded {
                SessionListView(stateManager: stateManager, panelState: panelState)
                    .environment(\.theme, CodyncTheme(isDark: isDarkMode, isPanel: true))
                    .preferredColorScheme(isDarkMode ? .dark : .light)
                    .transition(
                        .asymmetric(
                            insertion: .opacity
                                .combined(with: .scale(scale: 0.88, anchor: .top))
                                .animation(.smooth(duration: 0.3).delay(0.06)),
                            removal: .opacity
                                .animation(.easeOut(duration: 0.12))
                        )
                    )
            }
        }
        .frame(width: isExpanded ? 320 : headerSize.width - 12)
        .padding(.horizontal, isExpanded ? 19 : 6)
        .padding(.bottom, isExpanded ? 12 : 0)
        .background(Color.black)
        .clipShape(CodyncPanelShape(
            topCornerRadius: topCornerRadius,
            bottomCornerRadius: bottomCornerRadius
        ))
        .shadow(color: isExpanded ? .black.opacity(0.6) : .clear, radius: 8)
        .background(
            GeometryReader { geo in
                Color.clear.preference(key: ContentHeightKey.self, value: geo.size.height)
            }
        )
        .onPreferenceChange(ContentHeightKey.self) { height in
            if height > 0 { panelState.contentHeight = height }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
        .animation(panelAnimation, value: isExpanded)
    }
}

private struct ContentHeightKey: PreferenceKey {
    static let defaultValue: CGFloat = 0
    static func reduce(value: inout CGFloat, nextValue: () -> CGFloat) {
        value = max(value, nextValue())
    }
}
