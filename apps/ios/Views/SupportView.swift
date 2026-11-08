import CodyncKit
import CodyncUI
import SwiftUI

/// How many log lines the page starts with and adds per tap of Show older events.
private let logPage = 100

/// Settings → Support: how to get help and the app's own recent events (OpenBot Mobile's Support
/// page). Nothing in the log leaves the phone unless Save log file is used.
struct SupportView: View {
    @Environment(\.openURL) private var openURL
    @State private var log = SupportLog.shared
    @State private var showLog = false
    @State private var confirmClear = false
    @State private var exported: ExportedLog?

    var body: some View {
        Group {
            if showLog {
                SupportLogPage(back: { withAnimation(Motion.layout) { showLog = false } })
                    .transition(.move(edge: .trailing).combined(with: .opacity))
            } else {
                page
                    .transition(.move(edge: .leading).combined(with: .opacity))
            }
        }
        .background(Palette.background)
        .codyncDialog("Clear the log?", isPresented: $confirmClear,
                      message: "The saved log keeps what was there; new events start the log again.") {
            [DialogAction("Clear", destructive: true) { log.clear() }]
        }
        .codyncSheet(item: $exported) { item in
            FileExportSheet(url: item.url) { exported = nil }
        }
    }

    private var page: some View {
        VStack(spacing: 0) {
            ModalHeader("Support")
            CardForm {
                CardSection(footer: "Codync never sends the log. It records no messages or files and masks passwords and tokens.") {
                    Text(L10n.tr("The log stays on this phone. Save it and attach it when you ask for help, so the problem can be found."))
                        .font(.subheadline)
                        .foregroundStyle(Palette.secondary)
                    Button {
                        Analytics.shared.capture(.linkOpened(.discord))
                        openURL(URL(string: "https://discord.gg/KBAX3Qznxh")!)
                    } label: {
                        RowLabel("Ask on Discord", systemImage: "bubble.left.and.bubble.right")
                    }
                    .buttonStyle(.plain)
                    Button {
                        Analytics.shared.capture(.linkOpened(.issues))
                        openURL(URL(string: "https://github.com/leepokai/Codync/issues")!)
                    } label: {
                        RowLabel("Report a problem on GitHub", systemImage: "ladybug")
                    }
                    .buttonStyle(.plain)
                }
                CardSection {
                    Button {
                        withAnimation(Motion.layout) { showLog = true }
                    } label: {
                        RowLabel("Show log", systemImage: "doc.text", chevron: true)
                    }
                    .buttonStyle(.plain)
                    Button {
                        if let url = try? log.exportFile() { exported = ExportedLog(url: url) }
                    } label: {
                        RowLabel("Save log file", systemImage: "square.and.arrow.up")
                    }
                    .buttonStyle(.plain)
                    Button { confirmClear = true } label: {
                        RowLabel("Clear log", systemImage: "trash", tint: Palette.danger)
                    }
                    .buttonStyle(.plain)
                }
            }
        }
    }
}

/// The log itself, newest first with more revealed by a tap, live while the top is in view.
private struct SupportLogPage: View {
    let back: () -> Void
    @State private var log = SupportLog.shared
    @State private var shown = logPage
    @State private var atTop = true

    var body: some View {
        VStack(spacing: 0) {
            ScreenHeader {
                BackButton(action: back)
            } title: {
                Text(L10n.tr("Support log")).font(.body.weight(.semibold)).foregroundStyle(Palette.text)
            } trailing: {
                Text("\(log.events.count)").font(.caption).foregroundStyle(Palette.tertiary)
            }
            if log.events.isEmpty {
                Text(L10n.tr("No events recorded yet."))
                    .font(.subheadline)
                    .foregroundStyle(Palette.secondary)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(20)
                Spacer(minLength: 0)
            } else {
                ScrollViewReader { proxy in
                    ScrollView {
                        LazyVStack(alignment: .leading, spacing: 6) {
                            ForEach(Array(log.events.suffix(shown).reversed())) { event in
                                SupportLogRow(event: event)
                            }
                            if shown < log.events.count {
                                Button(L10n.tr("Show older events")) { shown += logPage }
                                    .buttonStyle(.secondary)
                            }
                        }
                        .padding(.horizontal, 20)
                        .padding(.vertical, 8)
                    }
                    .onScrollGeometryChange(for: Bool.self, of: { geometry in
                        geometry.contentOffset.y <= 8
                    }, action: { _, top in atTop = top })
                    .onChange(of: log.events.last?.id) { _, newest in
                        // New events at the top only while the reader is there.
                        guard let newest, atTop else { return }
                        withAnimation(Motion.layout) { proxy.scrollTo(newest, anchor: .top) }
                    }
                }
            }
        }
    }
}

/// One line: its time, its level colored, and the event.
private struct SupportLogRow: View {
    let event: SupportEvent

    private var tint: Color {
        switch event.level {
        case .error: Palette.danger
        case .warn: Palette.warning
        case .info: Palette.secondary
        }
    }

    var body: some View {
        HStack(alignment: .top, spacing: 8) {
            Text(event.clock).font(.caption2.monospaced()).foregroundStyle(Palette.tertiary)
            VStack(alignment: .leading, spacing: 2) {
                Text("\(event.source.tag) · \(event.level.tag)")
                    .font(.caption2.weight(.bold))
                    .foregroundStyle(tint)
                Text(event.message)
                    .font(.caption)
                    .foregroundStyle(Palette.text)
                    .textSelection(.enabled)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}

/// What the share sheet is presenting; a value so a dismissed sheet doesn't reopen.
private struct ExportedLog: Identifiable {
    let url: URL
    var id: URL { url }
}
