import AppKit
import SwiftUI

/// A small always-on-top pill while a phone is watching, so it's never a secret;
/// its button disconnects every viewer. Black and white like the rest of Codync.
@MainActor
final class ViewingBadge {
    var onDisconnect: (() -> Void)?
    private var panel: NSPanel?
    private let model = BadgeModel()

    func update(viewers: Int) {
        guard viewers > 0 else {
            panel?.orderOut(nil)
            return
        }
        model.viewers = viewers
        let panel = panel ?? makePanel()
        self.panel = panel
        // Measure after SwiftUI has laid out the new text, or the pill gets clipped.
        panel.contentView?.layoutSubtreeIfNeeded()
        let size = panel.contentView?.fittingSize ?? NSSize(width: 220, height: 34)
        if let screen = NSScreen.main {
            let f = screen.visibleFrame
            panel.setFrame(NSRect(x: f.midX - size.width / 2, y: f.maxY - size.height - 10, width: size.width, height: size.height), display: true)
        }
        panel.orderFrontRegardless()
    }

    private func makePanel() -> NSPanel {
        let panel = NSPanel(contentRect: NSRect(x: 0, y: 0, width: 220, height: 30), styleMask: [.nonactivatingPanel, .borderless], backing: .buffered, defer: false)
        panel.level = .statusBar
        panel.collectionBehavior = [.canJoinAllSpaces, .stationary, .fullScreenAuxiliary]
        panel.isOpaque = false
        panel.backgroundColor = .clear
        panel.hasShadow = false
        let host = NSHostingView(rootView: BadgeView(model: model) { [weak self] in self?.onDisconnect?() })
        host.sizingOptions = [.intrinsicContentSize]
        host.wantsLayer = true
        host.layer?.backgroundColor = .clear
        panel.contentView = host
        return panel
    }
}

@MainActor
@Observable
private final class BadgeModel {
    var viewers = 1
}

private struct BadgeView: View {
    let model: BadgeModel
    let disconnect: () -> Void
    @State private var pulse = false
    @State private var hovering = false

    var body: some View {
        HStack(spacing: 8) {
            // A live dot, like the system's recording indicator.
            Circle()
                .fill(Color.white)
                .frame(width: 7, height: 7)
                .opacity(pulse ? 0.35 : 1)
                .animation(.easeInOut(duration: 0.9).repeatForever(autoreverses: true), value: pulse)
                .onAppear { pulse = true }
            Text(model.viewers == 1 ? "iPhone is viewing" : "\(model.viewers) viewers")
                .font(.system(size: 12, weight: .medium))
                .foregroundStyle(.white)
            Button(action: disconnect) {
                Image(systemName: "xmark")
                    .font(.system(size: 9, weight: .bold))
                    .foregroundStyle(.white.opacity(hovering ? 1 : 0.7))
                    .frame(width: 18, height: 18)
                    .background(Circle().fill(.white.opacity(hovering ? 0.22 : 0.12)))
            }
            .buttonStyle(.plain)
            .onHover { hovering = $0 }
            .help("Disconnect")
            .accessibilityLabel("Disconnect")
        }
        .padding(.leading, 12)
        .padding(.trailing, 5)
        .padding(.vertical, 5)
        .background(Capsule().fill(Color.black.opacity(0.88)))
        .overlay(Capsule().strokeBorder(.white.opacity(0.14), lineWidth: 0.5))
        .fixedSize()
        .padding(2)
    }
}
