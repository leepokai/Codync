import CodyncKit
import Foundation
import os

private let log = Logger(subsystem: "com.pokai.Codync", category: "BotStore")

// Read marks: reading the chats on screen, opening one, the roster's "Mark as read" and "Mark as unread". The
// host decides what a read does to a chat marked unread; here they only go out in order, and show at once.
extension BotStore {
    static let markUnreadHost = "2.41.0"

    /// The roster offers "Mark as unread" for a chat with nothing unread; otherwise, or before host 2.41.0
    /// (it refuses `markUnread`), "Mark as read".
    public func offersMarkUnread(_ bot: Bot) -> Bool {
        bot.unread == 0 && !bot.keptUnread
            && hostVersion.map { !AppVersion.isBelow($0.version, Self.markUnreadHost) } ?? false
    }

    /// Views register only while visible in an active scene. Multiple windows may
    /// read the same conversation without unregistering each other.
    func setReading(_ token: UUID, botId: String, active: Bool) {
        if active {
            readingViews[token] = botId
            if connection == .online { markRead(botId) }
        } else {
            readingViews[token] = nil
        }
    }

    func acknowledgeVisibleConversations(_ botId: String, entry: Entry? = nil) {
        guard connection == .online else { return }
        guard readingViews.values.contains(botId), entry?.isChat != false else { return }
        markRead(botId)
    }

    /// The chat on screen is read: the host leaves one marked unread alone, and ignores it when nothing there is unread.
    public func markRead(_ botId: String) {
        if unsentOpens.contains(botId) {
            openConversation(botId)
            return
        }
        // Entry events may arrive before the roster's unread count. The host is
        // authoritative and treats a redundant scoped acknowledgement as a no-op.
        Task {
            // Opening a cached conversation can race the foreground reconnect.
            // This background acknowledgement must never interrupt the conversation.
            if live == nil {
                try? await Task.sleep(for: Self.initialConnectionGrace)
            }
            guard !Task.isCancelled, isActive, let client = live else { return }
            do {
                try await client.markRead(botId, automatic: true)
            } catch {
                // The visible scope is acknowledged again when the link recovers.
                log.debug("read acknowledgement deferred: \(error.localizedDescription)")
            }
        }
    }

    /// The roster's "Mark as read": everything in the chat.
    public func markAllRead(_ botId: String) {
        readAll(botId) { self.lastError = $0.localizedDescription }
    }

    /// The user opened the chat, which reads it whatever is known here of it. A failure never reaches the
    /// dialog: it's read as opened the next time it's on screen with a link (`markRead`).
    func openConversation(_ botId: String) {
        readAll(botId) { error in
            log.debug("opened chat stays unread for now: \(error.localizedDescription)")
            self.unsentOpens.insert(botId)
        }
    }

    /// The roster's "Mark as unread".
    public func markUnread(_ botId: String) {
        guard bots[botId] != nil else { return }
        unsentOpens.remove(botId)
        inOrder(botId) {
            var marked: Bot?
            await self.attempt(replay: true) { marked = try await $0.markUnread(botId) }
            // A newer bot event may have beaten the answer here.
            if let marked, let bot = self.bots[botId], bot.rev < marked.rev { self.putHostBot(marked) }
        }
    }

    /// Shows the chat read at once. A refused read gives back what was there, unless a newer bot event from
    /// the host replaced it.
    private func readAll(_ botId: String, failed: @escaping @MainActor (any Error) -> Void) {
        guard let bot = bots[botId] else { return }
        unsentOpens.remove(botId)
        bots[botId]?.unread = 0
        bots[botId]?.keptUnread = false
        inOrder(botId) {
            do {
                try await self.withLink(replay: true) { try await $0.markRead(botId) }
            } catch {
                if self.bots[botId]?.rev == bot.rev {
                    self.bots[botId]?.unread = bot.unread
                    self.bots[botId]?.keptUnread = bot.keptUnread
                }
                failed(error)
            }
        }
    }

    /// Runs `send` after the chat's last read mark, retries included, so the host gets them in the order they were made.
    private func inOrder(_ botId: String, _ send: @escaping @MainActor () async -> Void) {
        let before = sendingReadMarks[botId]
        let sending = Task {
            await before?.value
            await send()
        }
        sendingReadMarks[botId] = sending
        Task {
            await sending.value
            if sendingReadMarks[botId] == sending { sendingReadMarks[botId] = nil }
        }
    }
}
