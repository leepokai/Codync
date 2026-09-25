#if os(iOS)
import CodyncKit
import SwiftUI
import UIKit

/// The computer's screen, full screen on the phone. Opened from the roster (to use
/// the computer) or from a bot's live chip (to watch it work, then take over).
public struct ScreenView: View {
    @Environment(BotStore.self) private var model
    @Environment(\.dismiss) private var dismiss
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    /// The bot being watched, when opened from its chat.
    private let watching: String?
    /// Animated close from the presenting layer (`.codyncOverlay`'s `close`); falls back to `dismiss`.
    private let close: (() -> Void)?

    @State private var session: ScreenSession?
    @State private var display: ScreenDisplay?
    @State private var interactive = false
    @State private var mode: TouchMode = .trackpad
    @State private var keyboard = false
    @State private var armed: [String] = []
    @State private var zoomed = false
    @State private var resetToken = 0
    @State private var tookOver = false

    public init(watching: String? = nil, close: (() -> Void)? = nil) {
        self.watching = watching
        self.close = close
    }

    private var screen: ScreenState? { model.screen }
    private var agent: Bot? { screen?.agentBot.flatMap { model.bots[$0] } }

    public var body: some View {
        ZStack {
            Color.black.ignoresSafeArea()
            if let session, let display, screen?.available == true {
                ScreenCanvasView(session: session, display: display, mode: mode, interactive: interactive,
                                 keyboard: keyboard, armed: $armed, zoomed: $zoomed, resetToken: resetToken)
                    .ignoresSafeArea(.container)
                    .ignoresSafeArea(.keyboard)
                status(session)
            } else {
                unavailable
            }
        }
        .overlay(alignment: .top) { topBar }
        .safeAreaInset(edge: .bottom) {
            if keyboard && interactive {
                ModifierBar(armed: $armed) { session?.send(["type": "key", "key": $0, "modifiers": armed]); armed = [] }
                    .transition(.move(edge: .bottom).combined(with: .opacity))
            }
        }
        .statusBarHidden()
        .persistentSystemOverlays(.hidden)
        .preferredColorScheme(.dark)
        .onAppear(perform: start)
        .onDisappear(perform: stop)
        .onChange(of: screen?.available) { _, available in
            if available == true, session == nil { start() }
        }
    }

    // MARK: lifecycle

    private func start() {
        OrientationLock.set(.allButUpsideDown, prefer: .landscapeRight)
        guard session == nil, let client = model.client, let screen, screen.available, let d = display ?? screen.mainDisplay else { return }
        display = d
        let s = ScreenSession(client: client, display: d)
        session = s
        Task { await s.start() }
        // Watching a bot: look first. Otherwise take the screen (bots may still look).
        setInteractive(watching == nil)
    }

    private func stop() {
        session?.close()
        session = nil
        if tookOver { model.screenTakeover(false) }
        tookOver = false
        OrientationLock.set(.portrait, prefer: .portrait)
    }

    private func setInteractive(_ on: Bool) {
        interactive = on
        if on != tookOver {
            tookOver = on
            model.screenTakeover(on)
        }
        if !on { keyboard = false }
    }

    // MARK: chrome

    private var topBar: some View {
        HStack(spacing: 8) {
            OverlayButton("Close", "xmark") { if let close { close() } else { dismiss() } }
            if let agent, !interactive {
                Label("\(agent.name) is using the computer", systemImage: "cursorarrow.motionlines")
                    .font(.caption.weight(.medium))
                    .padding(.horizontal, 10)
                    .padding(.vertical, 6)
                    .background(.ultraThinMaterial, in: Capsule())
                    .transition(.opacity)
            }
            Spacer()
            if session != nil {
                if interactive {
                    OverlayButton(keyboard ? "Hide keyboard" : "Keyboard", keyboard ? "keyboard.chevron.compact.down" : "keyboard") { animate { keyboard.toggle() } }
                    OverlayButton(mode == .trackpad ? "Touch mode: trackpad" : "Touch mode: direct", mode == .trackpad ? "rectangle.and.hand.point.up.left" : "hand.point.up.left") {
                        animate { mode = mode == .trackpad ? .direct : .trackpad }
                    }
                    clipboardMenu
                }
                if zoomed {
                    OverlayButton("Fit screen", "arrow.down.right.and.arrow.up.left") { animate { resetToken += 1 } }
                }
                if let displays = screen?.displays, displays.count > 1 {
                    DropdownMenu {
                        displays.map { d in
                            MenuItem(d.name, icon: "display", selected: d.id == display?.id) {
                                display = d
                                session?.switchDisplay(d)
                                resetToken += 1
                            }
                        }
                    } label: { IconLabel("Displays", "display.2") }
                }
                OverlayButton(interactive ? "Hand back to bots" : "Take over", interactive ? "hand.raised.slash" : "hand.raised") {
                    animate { setInteractive(!interactive) }
                }
            }
        }
        .animation(Motion.reduced(Motion.fade, reduceMotion), value: zoomed)
        .padding(.horizontal, 12)
        .padding(.top, 6)
    }

    private func animate(_ change: () -> Void) {
        withAnimation(Motion.reduced(Motion.layout, reduceMotion), change)
    }

    private var clipboardMenu: some View {
        // MenuItem has no disabled state: unavailable actions are left out instead.
        DropdownMenu {
            var items: [MenuItem] = []
            if UIPasteboard.general.hasStrings {
                items.append(MenuItem("Send my clipboard", icon: "arrow.up.doc.on.clipboard") {
                    if let text = UIPasteboard.general.string { session?.pushClipboard(text) }
                })
            }
            if session?.remoteClipboard != nil {
                items.append(MenuItem("Copy from computer", icon: "arrow.down.doc.on.clipboard") {
                    if let text = session?.takeRemoteClipboard() { UIPasteboard.general.string = text }
                })
            }
            return items
        } label: {
            IconLabel("Clipboard", session?.remoteClipboard == nil ? "doc.on.clipboard" : "doc.on.clipboard.fill")
        }
    }

    @ViewBuilder private func status(_ session: ScreenSession) -> some View {
        switch session.phase {
        case .connecting:
            Overlay(icon: nil, text: "Connecting to the screen…")
        case .reconnecting:
            Overlay(icon: nil, text: "Reconnecting…")
        case let .failed(message):
            Overlay(icon: "exclamationmark.triangle", text: message) {
                Task { await session.start() }
            }
        case .live, .closed:
            if let error = session.lastError {
                Text(error)
                    .font(.caption)
                    .padding(8)
                    .background(.ultraThinMaterial, in: RoundedRectangle(cornerRadius: 8))
                    .frame(maxHeight: .infinity, alignment: .bottom)
                    .padding(.bottom, 24)
            }
        }
    }

    @ViewBuilder private var unavailable: some View {
        let s = screen
        let (icon, text): (String, String) =
            if model.connection != .online { ("wifi.slash", "This computer is offline.") }
            else if s == nil { ("arrow.down.circle", "Update Codync on your computer to use its screen from here.") }
            else if s?.enabled != true { ("lock.display", "Remote screen is off. Turn it on in Codync's menu on the computer.") }
            else if s?.connected != true { ("display.trianglebadge.exclamationmark", "Codync Screen isn't running on the computer. Turn Remote screen off and on again in Codync's menu there.") }
            else { ("record.circle", "Allow Codync Screen to record the screen: System Settings → Privacy & Security → Screen & System Audio Recording, on the computer.") }
        Overlay(icon: icon, text: text)
    }
}

private struct Overlay: View {
    let icon: String?
    let text: String
    var retry: (() -> Void)?

    var body: some View {
        VStack(spacing: 12) {
            if let icon {
                Image(systemName: icon).font(.largeTitle).foregroundStyle(.secondary)
            } else {
                Spinner(size: 22)
            }
            Text(text).font(.callout).multilineTextAlignment(.center).foregroundStyle(.secondary)
            if let retry {
                Button("Try again", systemImage: "arrow.clockwise", action: retry)
                    .labelStyle(.iconOnly)
                    .buttonStyle(.secondary)
            }
        }
        .padding(32)
        .frame(maxWidth: 420)
    }
}

private struct OverlayButton: View {
    let label: String
    let icon: String
    let action: () -> Void

    init(_ label: String, _ icon: String, action: @escaping () -> Void) {
        self.label = label
        self.icon = icon
        self.action = action
    }

    var body: some View {
        Button(action: action) { IconLabel(label, icon) }
            .buttonStyle(PressScale())
    }
}

private struct IconLabel: View {
    let label: String
    let icon: String

    init(_ label: String, _ icon: String) {
        self.label = label
        self.icon = icon
    }

    var body: some View {
        Image(systemName: icon)
            .font(.system(size: 15, weight: .semibold))
            .frame(width: 36, height: 36)
            .background(.ultraThinMaterial, in: Circle())
            .foregroundStyle(.white)
            .accessibilityLabel(label)
    }
}

/// Keys the iOS keyboard lacks. Modifiers arm for the next key or click.
private struct ModifierBar: View {
    @Binding var armed: [String]
    let press: (String) -> Void

    private static let modifiers = [("cmd", "command"), ("option", "option"), ("ctrl", "control"), ("shift", "shift")]
    private static let keys = [("escape", "escape"), ("tab", "arrow.right.to.line"), ("left", "arrow.left"), ("up", "arrow.up"), ("down", "arrow.down"), ("right", "arrow.right")]

    var body: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: 6) {
                ForEach(Self.modifiers, id: \.0) { name, icon in
                    let on = armed.contains(name)
                    Button {
                        withAnimation(Motion.hover) {
                            if on { armed.removeAll { $0 == name } } else { armed.append(name) }
                        }
                    } label: {
                        Image(systemName: icon).frame(width: 40, height: 34)
                            .background(on ? Color.white : Color.white.opacity(0.12), in: RoundedRectangle(cornerRadius: 8))
                            .foregroundStyle(on ? Color.black : Color.white)
                    }
                    .buttonStyle(PressScale())
                    .accessibilityLabel(name)
                    .accessibilityAddTraits(on ? .isSelected : [])
                }
                Rectangle().fill(.white.opacity(0.3)).frame(width: 0.5, height: 24)
                ForEach(Self.keys, id: \.0) { name, icon in
                    Button { press(name) } label: {
                        Image(systemName: icon).frame(width: 40, height: 34)
                            .background(Color.white.opacity(0.12), in: RoundedRectangle(cornerRadius: 8))
                            .foregroundStyle(.white)
                    }
                    .buttonStyle(PressScale())
                    .accessibilityLabel(name)
                }
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 6)
        }
        .background(.black.opacity(0.85))
    }
}

/// iPhone is portrait-only except while the screen viewer is open. The app delegate
/// answers `supportedInterfaceOrientationsFor` with `OrientationLock.mask`.
@MainActor
public enum OrientationLock {
    public static var mask: UIInterfaceOrientationMask = UIDevice.current.userInterfaceIdiom == .pad ? .all : .portrait

    static func set(_ newMask: UIInterfaceOrientationMask, prefer: UIInterfaceOrientationMask) {
        guard UIDevice.current.userInterfaceIdiom == .phone else { return }
        mask = newMask
        // Once the full-screen cover is up: every controller in the chain re-reads the mask, then rotate.
        Task { @MainActor in
            try? await Task.sleep(for: .milliseconds(100))
            for case let scene as UIWindowScene in UIApplication.shared.connectedScenes {
                var vc = scene.keyWindow?.rootViewController
                while let current = vc {
                    current.setNeedsUpdateOfSupportedInterfaceOrientations()
                    vc = current.presentedViewController
                }
                try? await Task.sleep(for: .milliseconds(150))
                // Best effort: turning the phone always works once the mask allows it.
                scene.requestGeometryUpdate(.iOS(interfaceOrientations: prefer))
            }
        }
    }
}
#endif
