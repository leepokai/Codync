import CodyncKit
import Foundation
import Observation

/// One share: what came in, the message, the chat it goes to (the last one shared to, to begin with), and sending it.
@Observable @MainActor final class ShareModel {
    enum Phase: Equatable {
        case loading, ready
        /// Bytes of the files uploaded so far, of `total`.
        case sending(sent: Int, total: Int)
        /// An interface phrase.
        case failed(String)
    }

    var message = ""
    var selected: ShareRecipient?
    private(set) var files: [IncomingFile] = []
    private(set) var phase = Phase.loading
    let targets: ShareTargets
    let hasComputers: Bool

    private let context: SharedStore.Context
    /// The text and links as they came in (`IncomingShare.pasted`).
    private var shared = ""
    /// One for every attempt: a retry after a lost reply never runs twice (the computer and the relay skip a nonce they have).
    private let nonce = UUID().uuidString
    private let folder = FileManager.default.temporaryDirectory.appending(path: "share-\(UUID().uuidString)")
    private let end: (_ sent: Bool) -> Void
    private var task: Task<Void, Never>?

    init(items: [NSExtensionItem], end: @escaping (_ sent: Bool) -> Void) {
        let context = SharedStore.activeContext
        self.context = context
        self.end = end
        targets = ShareTargets(computers: context.computers, order: context.computerOrder, bots: context.bots,
                               recent: context.shareRecipients)
        hasComputers = !context.computers.isEmpty
        selected = targets.recent.first?.recipient
        task = Task { await load(items) }
    }

    var isSending: Bool {
        if case .sending = phase { true } else { false }
    }

    private var outgoing: [OutgoingFile] { files.compactMap(\.outgoing) }
    private var text: String { message.trimmingCharacters(in: .whitespacesAndNewlines) }

    var canSend: Bool {
        selected != nil && phase != .loading && !isSending && (!text.isEmpty || !outgoing.isEmpty)
    }

    func send() {
        guard canSend, let recipient = selected else { return }
        let (text, files) = (text, outgoing)
        let total = files.reduce(0) { $0 + $1.data.count }
        let pasted = IncomingShare.pasted(shared, in: text)
        let recent = targets.recent.contains { $0.recipient == recipient }
        phase = .sending(sent: 0, total: total)
        task = Task {
            var event = AnalyticsEvent.ShareSent(.sent, files: files.count, text: !text.isEmpty, recent: recent)
            do throws(ShareDelivery.Failure) {
                let outcome = try await ShareDelivery.deliver(text, files: files, pasted: pasted, nonce: nonce, to: recipient,
                                                              in: context) { sent in
                    Task { @MainActor [weak self] in
                        guard let self, case .sending = self.phase else { return }
                        self.phase = .sending(sent: sent, total: total)
                    }
                }
                guard !Task.isCancelled else { return }
                context.shareRecipients = recipient.remembered(in: context.shareRecipients)
                if outcome == .queued { event.outcome = .queued }
                Analytics.queueShare(event)
                finish(sent: true)
            } catch {
                guard !Task.isCancelled else { return }
                event.outcome = .failed
                event.error = error.kind
                Analytics.queueShare(event)
                phase = .failed(error.message)
            }
        }
    }

    /// Closing the sheet: stops a send on its way; files already uploaded stay unsent, as in the composer.
    func cancel() {
        task?.cancel()
        finish(sent: false)
    }

    private func load(_ items: [NSExtensionItem]) async {
        let loaded = await SharedItems.load(items, into: folder)
        shared = loaded.text
        message = loaded.text
        files = loaded.files
        phase = .ready
        // A file left behind unread is reported once, as Android reports one it couldn't copy in.
        if loaded.files.contains(where: \.isUnreadable) {
            Analytics.queueShare(.init(.failed, error: .read, files: loaded.files.count, text: !loaded.text.isEmpty, recent: false))
        }
    }

    private func finish(sent: Bool) {
        try? FileManager.default.removeItem(at: folder)
        end(sent)
    }
}
