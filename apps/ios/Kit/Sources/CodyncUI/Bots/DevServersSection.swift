import CodyncKit
import SwiftUI

/// What a bot keeps running: a row is its name and ports, tapping it opens it, the rest is behind `⋯`.
struct DevServersSection: View {
    let bot: Bot
    @State private var logs: DevServer?

    var body: some View {
        CardSection("Dev servers") {
            ForEach(bot.devServers) { server in
                DevServerRow(botId: bot.id, server: server) { logs = server }
            }
        }
        .codyncSheet(item: $logs) { DevServerLogsView(botId: bot.id, server: $0).id($0.id) }
    }
}

private struct DevServerRow: View {
    let botId: String
    let server: DevServer
    let showLogs: () -> Void
    @Environment(BotStore.self) private var model
    @Environment(\.openURL) private var openURL

    var body: some View {
        HStack(spacing: 8) {
            switch server.tap {
            case let .open(address): Button { open(address) } label: { line }.buttonStyle(.plain)
            case .logs: Button(action: showLogs) { line }.buttonStyle(.plain)
            case nil: line
            }
            DropdownMenu(items: menuItems) {
                Image(systemName: "ellipsis")
                    .font(.system(size: 15, weight: .medium))
                    .foregroundStyle(Palette.secondary)
                    .frame(width: 44, height: 44)
                    .contentShape(Rectangle())
            }
            .accessibilityLabel(L10n.tr("More actions for {name}").replacingOccurrences(of: "{name}", with: server.name))
            // A 44 pt target in a row of the card's height, its glyph near the card's edge.
            .padding(.vertical, -6)
            .padding(.trailing, -12)
        }
    }

    private var line: some View {
        HStack(spacing: 8) {
            Text(server.name)
                .foregroundStyle(Palette.text)
                .lineLimit(1)
                .frame(maxWidth: .infinity, alignment: .leading)
            if server.status == .starting { Spinner(size: 11) }
            Text(L10n.tr(server.rowValue))
                .monospacedDigit()
                .foregroundStyle(server.status == .failed ? Palette.danger : Palette.secondary)
                .lineLimit(1)
                // A long name gives way, never the ports.
                .fixedSize()
        }
        .contentShape(Rectangle())
    }

    private func menuItems() -> [MenuItem] {
        server.menuRows.enumerated().map { index, row in item(row, divider: index > 0) }
    }

    private func item(_ row: DevServer.MenuRow, divider: Bool) -> MenuItem {
        switch row {
        case .onlyOnComputer: MenuItem("Only reachable on the computer", enabled: false) {}
        case .startedByShell: MenuItem("Started through the bot's shell: it stops with the bot", enabled: false) {}
        case .open: MenuItem("Open", icon: "arrow.up.right") { if let address = server.remoteAddress { open(address) } }
        case let .openAddress(address): MenuItem(DevServer.addressTitle(address), icon: "arrow.up.right", verbatim: true) { open(address) }
        case .copyAddress: MenuItem("Copy address", icon: "doc.on.doc") { copyAddress() }
        case .logs: MenuItem("Logs", icon: "doc.text", action: showLogs)
        case .restart: MenuItem("Restart", icon: "arrow.clockwise") { act(.restart) }
        case .start: MenuItem("Start", icon: "arrow.clockwise") { act(.restart) }
        case .stop: MenuItem("Stop", icon: "stop.fill", destructive: true, divider: divider) { act(.stop) }
        case .remove: MenuItem("Remove", icon: "trash", destructive: true, divider: divider) { act(.remove) }
        }
    }

    private func open(_ address: String) {
        guard let url = URL(string: address) else { return }
        Analytics.shared.capture(.devServerAction(.open))
        openURL(url)
    }

    private func copyAddress() {
        Analytics.shared.capture(.devServerAction(.copy))
        Pasteboard.copy(server.remoteAddress)
    }

    private func act(_ action: DevServer.Action) {
        model.devServerAction(action, botId: botId, id: server.id)
    }
}

/// A server's recent output, to read.
private struct DevServerLogsView: View {
    private enum Output {
        case loading, text(String), failed(String)
    }

    let botId: String
    let server: DevServer
    @Environment(BotStore.self) private var model
    @State private var output = Output.loading
    /// Moves to read the logs again.
    @State private var reads = 0

    var body: some View {
        VStack(spacing: 0) {
            ModalHeader {
                VStack(alignment: .leading, spacing: 1) {
                    Text(server.name)
                    Text(detail)
                        .font(.system(size: 11, weight: .regular, design: .monospaced))
                        .foregroundStyle(Palette.secondary)
                        .truncationMode(.middle)
                }
            } trailing: {
                IconButton("Refresh", systemImage: "arrow.clockwise") { reads += 1 }
            }
            content.frame(maxWidth: .infinity, maxHeight: .infinity)
        }
        .background(Palette.background)
        .task(id: reads) { await read() }
    }

    /// The command, and how it ended if it failed.
    private var detail: String {
        guard server.status == .failed, let code = server.exitCode else { return server.command }
        return "\(server.command) · \(L10n.tr("Exit code {code}").replacingOccurrences(of: "{code}", with: "\(code)"))"
    }

    @ViewBuilder private var content: some View {
        switch output {
        case .loading:
            Spinner()
        case let .text(text) where text.isEmpty:
            Text(L10n.tr("No output yet")).foregroundStyle(Palette.secondary)
        case let .text(text):
            ScrollView {
                Text(text)
                    .font(.system(size: 12, design: .monospaced))
                    .foregroundStyle(Palette.text)
                    .textSelection(.enabled)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(.horizontal, 20)
                    .padding(.bottom, 20)
            }
        case let .failed(error):
            VStack(spacing: 12) {
                Text(L10n.tr(error))
                    .foregroundStyle(Palette.danger)
                    .multilineTextAlignment(.center)
                    .textSelection(.enabled)
                Button(L10n.tr("Try again")) {
                    output = .loading
                    reads += 1
                }
                .buttonStyle(.secondary)
            }
            .padding(20)
        }
    }

    private func read() async {
        let result: Output
        do {
            result = .text(try await model.devServerLogs(botId: botId, id: server.id))
        } catch {
            result = .failed(error.localizedDescription)
        }
        // A newer read, or the sheet closing, took over from this one.
        if !Task.isCancelled { output = result }
    }
}
