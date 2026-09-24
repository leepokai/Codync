import CodyncKit
import CodyncUI
import CoreImage.CIFilterBuiltins
import SwiftUI

@main
struct CodyncMacApp: App {
    @State private var host = HostController()

    var body: some Scene {
        MenuBarExtra {
            MenuView()
                .environment(host)
                .frame(width: 340)
        } label: {
            Image(systemName: host.needsAttention ? "person.2.badge.gearshape.fill" : host.working > 0 ? "person.2.wave.2.fill" : "person.2.fill")
                .task { host.start() }
        }
        .menuBarExtraStyle(.window)

        Window("Codync", id: "chat") {
            ChatWindow()
                .environment(host)
        }
        .defaultSize(width: 1100, height: 760)
    }
}

struct MenuView: View {
    @Environment(HostController.self) private var host
    @Environment(\.openWindow) private var openWindow
    @State private var showPairing = false

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            header
            Divider().overlay(Palette.border)
            if showPairing {
                PairingPanel { showPairing = false }
            } else {
                content
            }
            Divider().overlay(Palette.border)
            footer
        }
        .background(Palette.background)
    }

    private var header: some View {
        HStack(spacing: 10) {
            CharacterAvatar(shape: "blob", color: "green", size: 26, mood: host.working > 0 ? .working : .idle)
            VStack(alignment: .leading, spacing: 1) {
                Text("Codync").font(.headline).foregroundStyle(Palette.text)
                Text(statusLine).font(.caption).foregroundStyle(Palette.secondary)
            }
            Spacer()
            if host.state == .running {
                Button {
                    openWindow(id: "chat")
                    NSApp.activate()
                } label: {
                    Image(systemName: "bubble.left.and.bubble.right")
                }
                .help("Open Codync")
                .accessibilityLabel("Open Codync")
                .controlSize(.small)
                Button {
                    showPairing.toggle()
                    if showPairing { host.loadPairing() }
                } label: {
                    Image(systemName: "qrcode")
                }
                .help("Pair iPhone")
                .accessibilityLabel("Pair iPhone")
                .buttonStyle(.borderedProminent)
                .tint(Palette.accentFill)
                .foregroundStyle(Palette.onAccent)
                .controlSize(.small)
            }
        }
        .padding(12)
    }

    private var statusLine: String {
        switch host.state {
        case .missingBinary: "Host not found"
        case .notInstalled: "Host not installed"
        case .starting: "Starting host…"
        case .running: host.working > 0 ? "\(host.working) bot\(host.working == 1 ? "" : "s") working" : "Ready · v\(host.version ?? "")"
        case .failed: "Host problem"
        }
    }

    @ViewBuilder private var content: some View {
        switch host.state {
        case .missingBinary:
            Notice(text: "This build of Codync doesn't include codync-host. Reinstall the app, or install the host with Homebrew:", code: "brew install leepokai/codync/codync-host")
        case .notInstalled:
            VStack(alignment: .leading, spacing: 10) {
                Text("Run your bots on this Mac").font(.subheadline.bold()).foregroundStyle(Palette.text)
                Text("Codync installs a small background service that runs Claude Code, Codex and other agents for your bots — even when this menu is closed.")
                    .font(.caption)
                    .foregroundStyle(Palette.secondary)
                    .fixedSize(horizontal: false, vertical: true)
                Button("Install host") { host.install() }
                    .buttonStyle(.borderedProminent)
                    .tint(Palette.accentFill)
                    .foregroundStyle(Palette.onAccent)
            }
            .padding(12)
        case .starting:
            HStack { ProgressView().controlSize(.small); Text("Connecting…").font(.caption) }.padding(12)
        case let .failed(message):
            VStack(alignment: .leading, spacing: 8) {
                Text(message).font(.caption).foregroundStyle(Palette.danger).textSelection(.enabled)
                HStack {
                    Button("Restart host") { host.restart() }
                    Button("Open log") { NSWorkspace.shared.open(host.logURL) }
                }
            }
            .padding(12)
        case .running:
            running
        }
    }

    private var running: some View {
        VStack(alignment: .leading, spacing: 0) {
            if host.bots.isEmpty {
                Text("No bots yet. Create one from the Codync app on your iPhone.")
                    .fixedSize(horizontal: false, vertical: true)
                    .font(.caption)
                    .foregroundStyle(Palette.secondary)
                    .padding(12)
            } else {
                ScrollView {
                    VStack(spacing: 2) {
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
                    .padding(6)
                }
                // ScrollView has no intrinsic height inside a MenuBarExtra window.
                .frame(height: min(CGFloat(host.bots.count) * 46 + 12, 320))
            }
            if !host.usage.providers.isEmpty {
                Divider().overlay(Palette.border)
                VStack(alignment: .leading, spacing: 8) {
                    ForEach(host.usage.providers) { p in
                        ForEach(p.windows) { w in
                            HStack(spacing: 8) {
                                Text("\(p.name) \(w.label)").font(.caption).foregroundStyle(Palette.secondary).frame(width: 130, alignment: .leading)
                                ProgressView(value: min(w.percent, 100), total: 100)
                                    .tint(w.percent >= 90 ? Palette.danger : w.percent >= 70 ? Palette.warning : Palette.accent)
                                Text("\(Int(w.percent.rounded()))%").font(.caption.monospacedDigit()).frame(width: 36, alignment: .trailing)
                            }
                        }
                    }
                }
                .padding(12)
            }
        }
    }

    private var footer: some View {
        @Bindable var host = host
        return HStack {
            Toggle("Open at login", isOn: Binding(get: { host.launchAtLogin }, set: { host.setLaunchAtLogin($0) }))
                .toggleStyle(.checkbox)
                .font(.caption)
            Spacer()
            Menu {
                Button("Restart host") { host.restart() }
                Button("Open log") { NSWorkspace.shared.open(host.logURL) }
                Divider()
                Button("Uninstall host service") { host.uninstall() }
            } label: {
                Image(systemName: "gearshape")
            }
            .menuStyle(.borderlessButton)
            .fixedSize()
            Button("Quit Codync", systemImage: "power") { NSApp.terminate(nil) }
                .labelStyle(.iconOnly)
                .help("Quit Codync")
                .font(.caption)
        }
        .padding(10)
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
                Button("Stop", systemImage: "stop.fill", action: stop)
                    .labelStyle(.iconOnly)
                    .help("Stop")
                    .controlSize(.small)
            } else {
                Text(RelativeTime.short(Date(milliseconds: bot.lastAt))).font(.caption2).foregroundStyle(Palette.tertiary)
            }
        }
        .padding(.horizontal, 8)
        .padding(.vertical, 6)
        .background(hovering ? Palette.bubbleAgent : .clear, in: RoundedRectangle(cornerRadius: 8))
        .onHover { hovering = $0 }
    }
}

private struct PairingPanel: View {
    @Environment(HostController.self) private var host
    let close: () -> Void

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
                } else if !info.urls.contains(where: { $0.contains("://100.") || $0.contains(".ts.net") }) {
                    Text("Tip: install Tailscale on this Mac and your iPhone to reach your bots from anywhere.")
                        .font(.caption)
                        .foregroundStyle(Palette.tertiary)
                        .multilineTextAlignment(.center)
                }
                Button("Copy pairing link", systemImage: "doc.on.doc") {
                    NSPasteboard.general.clearContents()
                    NSPasteboard.general.setString(info.pairingUrl, forType: .string)
                }
                .labelStyle(.iconOnly)
                .help("Copy pairing link")
                .controlSize(.small)
            } else {
                ProgressView()
            }
            Button("Close", systemImage: "xmark", action: close)
                .labelStyle(.iconOnly)
                .help("Close")
                .controlSize(.small)
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
