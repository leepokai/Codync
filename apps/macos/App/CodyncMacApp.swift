import CodyncKit
import CodyncUI
import CoreImage.CIFilterBuiltins
import SwiftUI

@main
struct CodyncMacApp: App {
    @State private var host = HostController()
    @State private var account = AccountSession()
    @Environment(\.openWindow) private var openWindow

    var body: some Scene {
        MenuBarExtra {
            MenuView()
                .environment(host)
                .environment(account)
                .frame(width: 340)
        } label: {
            // The Codync mark; a dot joins it when a bot needs you.
            Image(host.needsAttention ? "MenuBarIconAlert" : "MenuBarIcon")
                .task {
                    host.start()
                    openWindow(id: "chat")
                }
        }
        .menuBarExtraStyle(.window)

        Window("Codync", id: "chat") {
            ChatWindow()
                .environment(host)
                .environment(account)
        }
        .defaultSize(width: 1100, height: 760)
        .windowStyle(.hiddenTitleBar)
    }
}

struct MenuView: View {
    @Environment(HostController.self) private var host
    @Environment(\.openWindow) private var openWindow
    @State private var showPairing = false

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            header
            VStack(spacing: 8) {
                if showPairing {
                    PairingPanel()
                        .tile()
                        // Grows out of the QR button that opened it, and goes back into it.
                        .transition(.scale(0.97, anchor: .topTrailing).combined(with: .opacity))
                } else {
                    content
                }
            }
            .padding(.horizontal, 10)
            footer
        }
        .background(Palette.background)
        .animation(Motion.layout, value: showPairing)
    }

    // MARK: header

    private var header: some View {
        HStack(spacing: 10) {
            CodyncMark()
                .frame(width: 28, height: 28)
                .foregroundStyle(Palette.text)
                .accessibilityHidden(true)
            VStack(alignment: .leading, spacing: 2) {
                Text("Codync").font(.headline).foregroundStyle(Palette.text)
                HStack(spacing: 5) {
                    if let color = status.color {
                        Circle().fill(color).frame(width: 6, height: 6)
                    }
                    Text(status.text).font(.caption).foregroundStyle(Palette.secondary)
                }
                .accessibilityElement(children: .combine)
            }
            Spacer()
            if host.state == .running {
                IconButton("Open Codync", systemImage: "bubble.left.and.bubble.right") {
                    openWindow(id: "chat")
                    NSApp.activate()
                }
                IconButton("Pair iPhone", systemImage: "qrcode", selected: showPairing) {
                    showPairing.toggle()
                    if showPairing { host.loadPairing() }
                }
            }
        }
        .padding(.horizontal, 14)
        .padding(.top, 14)
        .padding(.bottom, 12)
    }

    /// The header's state line. A dot only when something is going on: blue working, amber needs you, red trouble.
    private var status: (text: String, color: Color?) {
        switch host.state {
        case .missingBinary: ("Host not found", Palette.danger)
        case .notInstalled: ("Host not installed", nil)
        case .starting: ("Connecting…", nil)
        case .failed: ("Host problem", Palette.danger)
        case .running:
            if host.needsAttention {
                ("A bot needs you", Palette.warning)
            } else if host.working > 0 {
                ("\(host.working) bot\(host.working == 1 ? "" : "s") working", Palette.switchOn)
            } else {
                ("Connected", nil)
            }
        }
    }

    // MARK: content

    @ViewBuilder private var content: some View {
        switch host.state {
        case .missingBinary:
            Notice(text: "This build of Codync doesn't include codync-host. Reinstall the app, or install the host with Homebrew:", code: "brew install leepokai/codync/codync-host")
                .tile()
        case .notInstalled:
            VStack(alignment: .leading, spacing: 10) {
                Text("Run your bots on this Mac").font(.callout.weight(.semibold)).foregroundStyle(Palette.text)
                Text("Codync installs a small background service that runs Claude Code, Codex and other agents for your bots — even when this menu is closed.")
                    .font(.caption)
                    .foregroundStyle(Palette.secondary)
                    .fixedSize(horizontal: false, vertical: true)
                Button("Install host") { host.install() }
                    .buttonStyle(.borderedProminent)
                    .tint(Palette.accentFill)
                    .foregroundStyle(Palette.onAccent)
            }
            .tile()
        case .starting:
            HStack(spacing: 8) {
                ProgressView().controlSize(.small)
                Text("Connecting…").font(.caption).foregroundStyle(Palette.secondary)
            }
            .frame(maxWidth: .infinity)
            .tile()
        case let .failed(message):
            VStack(alignment: .leading, spacing: 10) {
                Text(message).font(.caption).foregroundStyle(Palette.danger).textSelection(.enabled)
                HStack {
                    Button("Restart host") { host.restart() }
                    Button("Open log") { NSWorkspace.shared.open(host.logURL) }
                }
                .controlSize(.small)
            }
            .tile()
        case .running:
            bots
            if host.screen != nil {
                RemoteScreenSection().tile()
            }
            if !host.usage.providers.isEmpty {
                VStack(spacing: 12) {
                    ForEach(host.usage.providers) { UsageBlock(provider: $0) }
                }
                .tile()
            }
        }
    }

    @ViewBuilder private var bots: some View {
        if host.bots.isEmpty {
            HStack(spacing: 12) {
                CharacterAvatar(shape: "cloud", color: "gray", size: 32)
                VStack(alignment: .leading, spacing: 2) {
                    Text("No bots yet").font(.callout.weight(.medium)).foregroundStyle(Palette.text)
                    Text("Create one from Codync on your iPhone.").font(.caption).foregroundStyle(Palette.secondary)
                }
                Spacer(minLength: 0)
            }
            .tile()
        } else {
            ScrollView {
                VStack(spacing: 0) {
                    ForEach(host.bots) { bot in
                        BotLine(bot: bot) { host.stop(bot) }
                            .contentShape(Rectangle())
                            .onTapGesture {
                                host.store?.selection = bot.id
                                openWindow(id: "chat")
                                NSApp.activate()
                            }
                    }
                }
                .padding(4)
            }
            // ScrollView has no intrinsic height inside a MenuBarExtra window.
            .frame(height: min(CGFloat(host.bots.count) * 46 + 8, 320))
            .tile(padding: 0)
        }
    }

    // MARK: footer

    /// Rarely used, so quiet: settings (with Open at login) and Quit.
    private var footer: some View {
        HStack(spacing: 2) {
            if let version = host.version {
                Text("v\(version)").font(.caption2.monospacedDigit()).foregroundStyle(Palette.tertiary).padding(.leading, 6)
            }
            Spacer()
            Menu {
                Toggle("Open at login", isOn: Binding(get: { host.launchAtLogin }, set: { host.setLaunchAtLogin($0) }))
                Divider()
                Button("Restart host") { host.restart() }
                Button("Open log") { NSWorkspace.shared.open(host.logURL) }
                Divider()
                Button("Uninstall host service") { host.uninstall() }
            } label: {
                Image(systemName: "gearshape")
            }
            .menuStyle(.button)
            .buttonStyle(IconButtonStyle())
            .menuIndicator(.hidden)
            .fixedSize()
            .help("Settings")
            IconButton("Quit Codync", systemImage: "power") { NSApp.terminate(nil) }
        }
        .padding(.horizontal, 10)
        .padding(.vertical, 8)
    }
}

private extension View {
    /// A Control Center–style module: one group of related things on a raised surface.
    func tile(padding: CGFloat = 12) -> some View {
        self.padding(padding)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(Palette.surface, in: RoundedRectangle(cornerRadius: 14, style: .continuous))
            .overlay(RoundedRectangle(cornerRadius: 14, style: .continuous).strokeBorder(Palette.border, lineWidth: 0.5))
    }
}

/// Remote screen: see and control this Mac from the iPhone, and let bots use it.
/// One line when all is well; it only grows to say what's missing.
private struct RemoteScreenSection: View {
    @Environment(HostController.self) private var host

    var body: some View {
        let screen = host.screen ?? ScreenState()
        VStack(alignment: .leading, spacing: 10) {
            HStack(spacing: 10) {
                Image(systemName: "display")
                    .font(.system(size: 13, weight: .semibold))
                    .foregroundStyle(screen.enabled ? .white : Palette.text)
                    .frame(width: 30, height: 30)
                    .background(screen.enabled ? Palette.switchOn : Palette.bubbleUser, in: Circle())
                    .accessibilityHidden(true)
                VStack(alignment: .leading, spacing: 1) {
                    Text("Remote screen").font(.callout.weight(.medium)).foregroundStyle(Palette.text)
                    Text(subtitle(screen))
                        .font(.caption)
                        .foregroundStyle(screen.enabled && screen.viewers > 0 ? Palette.switchOn : Palette.secondary)
                        .lineLimit(2)
                        .fixedSize(horizontal: false, vertical: true)
                }
                Spacer(minLength: 8)
                Toggle("Remote screen", isOn: Binding(get: { screen.enabled }, set: { host.setRemoteScreen($0) }))
                    .labelsHidden()
                    .toggleStyle(.switch)
                    .controlSize(.mini)
            }
            if screen.enabled {
                if host.screenAgentNeedsApproval {
                    PermissionLine(title: "Codync Screen in Login Items") { host.openLoginItemsSettings() }
                } else if screen.connected {
                    if !screen.capture {
                        PermissionLine(title: "Screen recording") {
                            open("x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture")
                        }
                    }
                    if !screen.input {
                        PermissionLine(title: "Accessibility (mouse and keyboard)") {
                            open("x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility")
                        }
                    }
                }
            }
            if let error = host.screenError {
                Text(error).font(.caption).foregroundStyle(Palette.danger).fixedSize(horizontal: false, vertical: true)
            }
        }
        .animation(Motion.layout, value: screen)
    }

    private func subtitle(_ screen: ScreenState) -> String {
        guard screen.enabled else { return "Control this Mac from your iPhone" }
        if host.screenAgentNeedsApproval { return "Needs one more step" }
        if !screen.connected { return "Starting Codync Screen…" }
        if !screen.capture || !screen.input { return "Needs permission" }
        if screen.viewers > 0 { return screen.viewers == 1 ? "Your iPhone is viewing" : "\(screen.viewers) viewers" }
        if let bot = screen.agentBot, let name = host.bots.first(where: { $0.id == bot })?.name { return "\(name) is using it" }
        let display = screen.displays.first(where: \.main)?.name ?? screen.displays.first?.name
        return display.map { "Ready · \($0)" } ?? "Ready"
    }

    private func open(_ url: String) {
        if let url = URL(string: url) { NSWorkspace.shared.open(url) }
    }
}

/// A permission that's still missing, with the way to fix it.
private struct PermissionLine: View {
    let title: String
    let fix: () -> Void

    var body: some View {
        HStack(spacing: 8) {
            Image(systemName: "exclamationmark.circle.fill")
                .foregroundStyle(Palette.warning)
                .accessibilityHidden(true)
            Text(title).font(.caption).foregroundStyle(Palette.text)
            Spacer()
            IconButton("Open System Settings", systemImage: "gearshape", action: fix)
        }
        .padding(.leading, 8)
        .padding(.vertical, 2)
        .background(Palette.warning.opacity(0.1), in: RoundedRectangle(cornerRadius: 9, style: .continuous))
        .accessibilityElement(children: .combine)
        .accessibilityValue("Not allowed")
    }
}

/// One provider's limits: its character and name, then a thin bar per limit in its color.
private struct UsageBlock: View {
    let provider: UsageProvider

    var body: some View {
        VStack(alignment: .leading, spacing: 7) {
            HStack(spacing: 6) {
                AgentIcon(registry: provider.registry, size: 15)
                Text(provider.name).font(.caption.weight(.semibold)).foregroundStyle(Palette.text)
                Spacer()
                Text("updated \(RelativeTime.short(Date(milliseconds: provider.updatedAt)))")
                    .font(.caption2)
                    .foregroundStyle(Palette.tertiary)
            }
            ForEach(provider.windows) { w in
                HStack(spacing: 8) {
                    Text(w.title).font(.caption).foregroundStyle(Palette.secondary).lineLimit(1)
                        .frame(width: 58, alignment: .leading)
                    ThinBar(percent: w.percent, tint: provider.tint)
                    Text("\(Int(w.percent.rounded()))%")
                        .font(.caption.monospacedDigit().weight(.medium))
                        .foregroundStyle(Palette.text)
                        .frame(width: 34, alignment: .trailing)
                    Text(w.resetDate.map { RelativeTime.until($0) } ?? "")
                        .font(.caption2.monospacedDigit())
                        .foregroundStyle(Palette.tertiary)
                        .frame(width: 48, alignment: .trailing)
                }
                .help(w.resetsShort() ?? w.label)
                .accessibilityElement(children: .combine)
            }
        }
    }
}

/// 4pt track in the provider's color; amber from 70%, red from 90%. Empty stays empty.
private struct ThinBar: View {
    let percent: Double
    let tint: Color

    var body: some View {
        let color = percent >= 90 ? Palette.danger : percent >= 70 ? Palette.warning : tint
        GeometryReader { geo in
            ZStack(alignment: .leading) {
                Capsule().fill(Palette.text.opacity(0.08))
                if percent > 0 {
                    Capsule().fill(color).frame(width: max(4, geo.size.width * min(1, percent / 100)))
                }
            }
        }
        .frame(height: 4)
    }
}

private struct BotLine: View {
    let bot: Bot
    let stop: () -> Void
    @State private var hovering = false

    var body: some View {
        HStack(spacing: 10) {
            AvatarWithStatus(bot: bot, size: 30)
            VStack(alignment: .leading, spacing: 1) {
                Text(bot.name).font(.callout.weight(.medium)).foregroundStyle(Palette.text)
                Text(bot.isWorking ? (bot.activity.isEmpty ? "Working…" : bot.activity) : (bot.lastMessage ?? bot.folderName))
                    .font(.caption)
                    .foregroundStyle(bot.needsInput ? Palette.warning : bot.isWorking ? Palette.accent : Palette.secondary)
                    .lineLimit(1)
            }
            Spacer()
            if bot.isWorking && hovering {
                IconButton("Stop", systemImage: "stop.fill", action: stop)
            } else {
                Text(RelativeTime.short(Date(milliseconds: bot.lastAt))).font(.caption2).foregroundStyle(Palette.tertiary)
            }
        }
        .padding(.horizontal, 8)
        .padding(.vertical, 6)
        .background(hovering ? Palette.text.opacity(0.06) : .clear, in: RoundedRectangle(cornerRadius: 10, style: .continuous))
        .animation(Motion.hover, value: hovering)
        .onHover { hovering = $0 }
    }
}

private struct PairingPanel: View {
    @Environment(HostController.self) private var host

    var body: some View {
        VStack(spacing: 12) {
            if let info = host.pairInfo {
                if let image = qr(info.pairingUrl) {
                    Image(nsImage: image)
                        .interpolation(.none)
                        .resizable()
                        .frame(width: 200, height: 200)
                        .padding(10)
                        .background(.white, in: RoundedRectangle(cornerRadius: 12))
                }
                Text("Scan with the Codync app or the iPhone Camera.")
                    .font(.caption)
                    .foregroundStyle(Palette.secondary)
                if info.urls.isEmpty {
                    Text("No network address found. Connect this Mac to Wi-Fi or Tailscale.")
                        .font(.caption)
                        .foregroundStyle(Palette.warning)
                } else if !info.urls.contains(where: Tailscale.isAddress) {
                    TailscaleTip()
                }
                IconButton("Copy pairing link", systemImage: "doc.on.doc") {
                    NSPasteboard.general.clearContents()
                    NSPasteboard.general.setString(info.pairingUrl, forType: .string)
                }
            } else {
                ProgressView()
            }
        }
        .frame(maxWidth: .infinity)
        .padding(16)
    }

    private func qr(_ string: String) -> NSImage? {
        let filter = CIFilter.qrCodeGenerator()
        filter.message = Data(string.utf8)
        filter.correctionLevel = "M"
        guard let output = filter.outputImage else { return nil }
        let rep = NSCIImageRep(ciImage: output)
        let image = NSImage(size: rep.size)
        image.addRepresentation(rep)
        return image
    }
}

/// Without Tailscale the iPhone only reaches this Mac on the same Wi-Fi.
private struct TailscaleTip: View {
    private static let app = URL(fileURLWithPath: "/Applications/Tailscale.app")
    private let installed = FileManager.default.fileExists(atPath: app.path)

    var body: some View {
        VStack(spacing: 6) {
            Text(installed
                ? "Tailscale isn't connected. Open it and sign in to reach your bots away from home."
                : "Install Tailscale on this Mac and your iPhone to reach your bots away from home.")
                .font(.caption)
                .foregroundStyle(Palette.tertiary)
                .multilineTextAlignment(.center)
            if installed {
                Button("Open Tailscale") { NSWorkspace.shared.open(Self.app) }.controlSize(.small)
            } else {
                Link("Get Tailscale", destination: Tailscale.downloadURL).font(.caption)
            }
        }
    }
}

private struct Notice: View {
    let text: String
    let code: String

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text(text).font(.caption).foregroundStyle(Palette.secondary).fixedSize(horizontal: false, vertical: true)
            Text(code)
                .font(.caption.monospaced())
                .textSelection(.enabled)
                .padding(8)
                .frame(maxWidth: .infinity, alignment: .leading)
                .background(Palette.codeBackground, in: RoundedRectangle(cornerRadius: 6))
        }
        .padding(12)
    }
}

/// The app icon's face drawn as vectors: a 9×9 dot grid lit from the upper left,
/// eyes as missing dots (same geometry as the app icon, without its black tile).
private struct CodyncMark: View {
    private static let cells = 9
    private static let eyes: Set<[Int]> = [[3, 3], [3, 5], [4, 3], [4, 5]]

    var body: some View {
        Canvas { context, size in
            let n = Double(Self.cells)
            let step = size.width / n
            for row in 0..<Self.cells {
                for col in 0..<Self.cells where !Self.eyes.contains([row, col]) {
                    let u = (Double(col) + 0.5) / n * 2 - 1
                    let v = (Double(row) + 0.5) / n * 2 - 1
                    guard hypot(u, v) <= 1.18 else { continue }
                    let shade = 0.55 + 0.45 * min(1, max(0, 0.7 - 0.3 * u - 0.4 * v))
                    let r = step * 0.36 * shade
                    let c = CGPoint(x: (Double(col) + 0.5) * step, y: (Double(row) + 0.5) * step)
                    context.fill(Path(ellipseIn: CGRect(x: c.x - r, y: c.y - r, width: 2 * r, height: 2 * r)), with: .foreground)
                }
            }
        }
    }
}
