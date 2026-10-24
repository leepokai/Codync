import CodyncKit
import Foundation

extension BotStore {
    /// Wait for the connection before choosing normal delivery or the relay mailbox.
    private func prepareShortcut(to botId: String, allowQueue: Bool = false) async throws {
        do {
            let deadline = ContinuousClock.now + Self.actionPatience
            _ = try await ready(until: deadline)
            // Online begins with hello; the bot events can arrive in later stream chunks.
            while bots[botId] == nil && (!caughtUp || rev < catchUpRev) {
                guard mismatch == nil else { throw HostError.upgradeRequired }
                guard ContinuousClock.now < deadline else { throw HostError.unreachable }
                try await Task.sleep(for: .milliseconds(100))
                _ = try await ready(until: deadline)
            }
        } catch HostError.computerOffline where allowQueue && canQueue {
            // Acknowledged by the mailbox through the normal send path below.
        }
        try Task.checkCancellation()
        guard !retired else { throw CancellationError() }
        guard mismatch == nil else { throw HostError.upgradeRequired }
        guard let bot = bots[botId], !bot.hidden else {
            throw HostError.http(404, "This bot is no longer available. Choose another bot in the shortcut.")
        }
    }

    public func requestVoiceCall(to botId: String) async throws {
        let request = UUID()
        preparingVoiceCall = request
        voiceCallRequest = nil
        defer { if preparingVoiceCall == request { preparingVoiceCall = nil } }
        try await prepareShortcut(to: botId)
        guard preparingVoiceCall == request else { throw CancellationError() }
        Motion.animate { voiceCallRequest = botId }
    }

    /// Leaving a conversation invalidates both its queued request and any connection wait.
    func cancelVoiceCallRequest() {
        preparingVoiceCall = nil
        voiceCallRequest = nil
    }

    /// Consumed once, including when the conversation was already on screen.
    func takeVoiceCallRequest(for botId: String) -> Bool {
        guard voiceCallRequest == botId else { return false }
        voiceCallRequest = nil
        return true
    }

    /// Shortcuts finish only after the host or mailbox has accepted the message.
    public func sendAndWait(_ text: String, to botId: String) async throws -> SendProgress {
        guard !text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else {
            throw HostError.http(400, "Enter some text to send to the bot.")
        }
        try beginShortcutSend()
        defer { endShortcutSend() }
        try await prepareShortcut(to: botId, allowQueue: true)
        guard let delivery = send(text, to: botId, replyTo: nil, nonce: UUID().uuidString) else {
            throw CancellationError()
        }
        let progress = try await withTaskCancellationHandler {
            let progress = await delivery.value
            try Task.checkCancellation()
            return progress
        } onCancel: {
            delivery.cancel()
        }
        guard !retired else { throw CancellationError() }
        if case .failed = progress {
            throw HostError.http(503, "Couldn't send the message. Check the conversation and retry the failed message.")
        }
        return progress
    }
}
