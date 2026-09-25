import CodyncKit
import SwiftTerm
import SwiftUI
#if canImport(UIKit)
import UIKit
#else
import AppKit
#endif

/// Installing or signing in to an agent, live: the command runs in a terminal
/// on the computer and this screen is that terminal (links open here).
struct SetupTerminalView: View {
    let backend: Backend
    let step: SetupStep
    /// A terminal sign-in method the agent offered; nil runs Codync's own command.
    var method: AuthMethod?
    @Environment(BotStore.self) private var model
    @Environment(\.dismiss) private var dismiss
    @Environment(\.openURL) private var openURL
    @State private var session = TermSession()

    var body: some View {
        VStack(spacing: 0) {
            TerminalSurface(session: session) { openURL($0) }
                .padding(.horizontal, 10)
                .padding(.top, 6)
            footer
        }
        .background(Palette.background)
        .navigationTitle(step == .install ? "Install \(backend.name)" : method?.name ?? "Sign in to \(backend.name)")
        .inlineNavigationTitle()
        .toolbar {
            ToolbarItem(placement: .cancellationAction) {
                Button("Close", systemImage: "xmark") { dismiss() }.labelStyle(.iconOnly)
            }
        }
        .task {
            guard let client = model.client else { return }
            await session.run(client, backend: backend.id, step: step, method: method?.id)
            await model.refreshBackends()
        }
        .onDisappear { session.close() }
    }

    private var footer: some View {
        HStack(spacing: 8) {
            if let error = session.error {
                Text(error).foregroundStyle(Palette.danger)
            } else if let code = session.exitCode {
                Image(systemName: code == 0 ? "checkmark.circle.fill" : "exclamationmark.circle.fill")
                    .foregroundStyle(code == 0 ? Palette.added : Palette.warning)
                Text(code == 0 ? "Finished." : "Ended with an error (\(code)).")
                    .foregroundStyle(Palette.secondary)
            } else {
                ThinkingOrb(size: 14, color: Palette.secondary)
                Text("Running on \(model.hostName). Links open on this device.")
                    .foregroundStyle(Palette.secondary)
            }
            Spacer(minLength: 0)
            if session.exitCode != nil {
                Button("Done") { dismiss() }.buttonStyle(.borderedProminent)
            }
        }
        .font(.footnote)
        .lineLimit(2)
        .padding(.horizontal, 16)
        .padding(.vertical, 10)
    }
}

/// One setup terminal on the host: keystrokes go up in order, output comes down.
@MainActor @Observable
final class TermSession {
    private(set) var exitCode: Int?
    private(set) var error: String?
    @ObservationIgnored weak var view: TerminalView?
    @ObservationIgnored private var client: HostClient?
    @ObservationIgnored private var term: String?
    @ObservationIgnored private var size = (cols: 80, rows: 24)
    @ObservationIgnored private var sentSize = (cols: 0, rows: 0)
    @ObservationIgnored private let keys: AsyncStream<Data>.Continuation
    @ObservationIgnored private let keyStream: AsyncStream<Data>
    @ObservationIgnored private var typing: Task<Void, Never>?

    init() {
        (keyStream, keys) = AsyncStream.makeStream()
    }

    func run(_ client: HostClient, backend: String, step: SetupStep, method: String?) async {
        self.client = client
        let term: String
        do {
            term = try await client.agentSetup(backend: backend, step: step, method: method, cols: size.cols, rows: size.rows)
        } catch {
            self.error = error.localizedDescription
            return
        }
        self.term = term
        sentSize = size
        syncSize()
        let keyStream = keyStream
        // One sender, so keystrokes can't overtake each other.
        typing = Task {
            for await bytes in keyStream {
                try? await client.termInput(term, bytes)
            }
        }
        // Reattach after a dropped connection; the host replays the whole scrollback.
        var attempt = 0
        while exitCode == nil, !Task.isCancelled {
            do {
                view?.getTerminal().resetToInitialState()
                for try await event in client.termOutput(term) {
                    attempt = 0
                    switch event {
                    case .output(let data): view?.feed(byteArray: ArraySlice(data))
                    case .exit(let code): exitCode = code
                    }
                }
            } catch HostError.http(404, let message) {
                error = message
                return
            } catch {
                attempt += 1
                if attempt > 5 {
                    self.error = error.localizedDescription
                    return
                }
            }
            if exitCode == nil { try? await Task.sleep(for: .seconds(1)) }
        }
    }

    func type(_ bytes: ArraySlice<UInt8>) {
        guard exitCode == nil else { return }
        keys.yield(Data(bytes))
    }

    func resized(cols: Int, rows: Int) {
        size = (cols, rows)
        syncSize()
    }

    private func syncSize() {
        guard let client, let term, size != sentSize else { return }
        sentSize = size
        let size = size
        Task { try? await client.termResize(term, cols: size.cols, rows: size.rows) }
    }

    /// Leaving the screen ends whatever is still running.
    func close() {
        keys.finish()
        typing?.cancel()
        guard exitCode == nil, let client, let term else { return }
        Task { try? await client.termClose(term) }
    }
}

// MARK: - SwiftTerm bridge

private final class TerminalBridge: NSObject, @preconcurrency TerminalViewDelegate {
    let session: TermSession
    let open: (URL) -> Void

    init(session: TermSession, open: @escaping (URL) -> Void) {
        self.session = session
        self.open = open
    }

    @MainActor func make() -> TerminalView {
        let view = TerminalView(frame: CGRect(x: 0, y: 0, width: 400, height: 300))
        view.terminalDelegate = self
        #if canImport(UIKit)
        view.font = .monospacedSystemFont(ofSize: 12, weight: .regular)
        view.nativeBackgroundColor = UIColor(Palette.background)
        view.nativeForegroundColor = UIColor(Palette.text)
        view.keyboardDismissMode = .interactive
        #else
        view.font = .monospacedSystemFont(ofSize: 12, weight: .regular)
        view.nativeBackgroundColor = NSColor(Palette.background)
        view.nativeForegroundColor = NSColor(Palette.text)
        #endif
        session.view = view
        return view
    }

    @MainActor func focus(_ view: TerminalView) {
        #if canImport(UIKit)
        _ = view.becomeFirstResponder()
        #else
        view.window?.makeFirstResponder(view)
        #endif
    }

    @MainActor func send(source: TerminalView, data: ArraySlice<UInt8>) { session.type(data) }
    @MainActor func sizeChanged(source: TerminalView, newCols: Int, newRows: Int) {
        session.resized(cols: newCols, rows: newRows)
    }
    @MainActor func requestOpenLink(source: TerminalView, link: String, params: [String: String]) {
        if let url = URL(string: link), url.scheme?.hasPrefix("http") == true { open(url) }
    }
    func setTerminalTitle(source: TerminalView, title: String) {}
    func hostCurrentDirectoryUpdate(source: TerminalView, directory: String?) {}
    func scrolled(source: TerminalView, position: Double) {}
    func rangeChanged(source: TerminalView, startY: Int, endY: Int) {}
    func clipboardCopy(source: TerminalView, content: Data) {
        Pasteboard.copy(String(data: content, encoding: .utf8))
    }
}

#if canImport(UIKit)
private struct TerminalSurface: UIViewRepresentable {
    let session: TermSession
    let open: (URL) -> Void

    func makeCoordinator() -> TerminalBridge { TerminalBridge(session: session, open: open) }

    func makeUIView(context: Context) -> TerminalView {
        let view = context.coordinator.make()
        Task { @MainActor in context.coordinator.focus(view) }
        return view
    }

    func updateUIView(_ view: TerminalView, context: Context) {}
}
#else
private struct TerminalSurface: NSViewRepresentable {
    let session: TermSession
    let open: (URL) -> Void

    func makeCoordinator() -> TerminalBridge { TerminalBridge(session: session, open: open) }

    func makeNSView(context: Context) -> TerminalView {
        let view = context.coordinator.make()
        Task { @MainActor in context.coordinator.focus(view) }
        return view
    }

    func updateNSView(_ view: TerminalView, context: Context) {}
}
#endif
