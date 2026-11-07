import CodyncKit
import Foundation
import os

private let log = Logger(subsystem: "com.pokai.Codync", category: "BotStore")

// Read marks: reading the chats on screen, opening one, the roster's "Mark as read" and "Mark as unread". The
// host decides what a read does to a chat marked unread; here they only go out in order, and show at once.
extension BotStore {
    static let markUnreadHost = "2.42.0"

    /// The roster offers "Mark as unread" for a chat with nothing unread; otherwise, or before host 2.42.0
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
            if connection == .online { readOnScreen(botId) }
        } else {
            readingViews[token] = nil
        }
    }

    func acknowledgeVisibleConversations(_ botId: String, entry: Entry? = nil) {
        guard connection == .online else { return }
        guard readingViews.values.contains(botId), entry?.isChat != false else { return }
        readOnScreen(botId)
    }

    /// The chat on screen is read: the host leaves one marked unread alone, and ignores it when nothing there is unread.
    func readOnScreen(_ botId: String) {
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
    /// dialog: it's read as opened the next time it's on screen with a link (`readOnScreen`).
    func openConversation(_ botId: String) {
        readAll(botId) { error in
            log.debug("opened chat stays unread for now: \(error.localizedDescription)")
            self.unsentOpens.insert(botId)
        }
    }

    /// The roster's "Mark as unread".
    public func markUnread(_ botId: String) {
        guard bots[botId] != nil else { return }
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
        let before = beforeReads[botId].flatMap { $0.rev == bot.rev ? $0 : nil } ?? bot
        beforeReads[botId] = before
        bots[botId]?.unread = 0
        bots[botId]?.keptUnread = false
        inOrder(botId) {
            do {
                try await self.withLink(replay: true) { try await $0.markRead(botId) }
            } catch {
                if self.bots[botId]?.rev == before.rev {
                    self.bots[botId]?.unread = before.unread
                    self.bots[botId]?.keptUnread = before.keptUnread
                }
                failed(error)
            }
        }
    }

    /// Runs `send` after the chat's last read mark, retries included, so the host gets them in the order they were made.
    private func inOrder(_ botId: String, _ send: @escaping @MainActor () async -> Void) {
        unsentOpens.remove(botId)
        inReadOrder(botId) {
            // An open before this mark may have failed while it waited: this one is newer.
            self.unsentOpens.remove(botId)
            await send()
        }
    }

    /// Runs `op` after the chat's read marks on their way, retries included; the next mark waits for it. A message
    /// sent from here goes through it too: sending reads the chat on the host, so it must not overtake a mark.
    @discardableResult
    func inReadOrder<T: Sendable>(_ botId: String, _ op: @escaping @MainActor () async throws -> T) -> Task<T, any Error> {
        let before = sendingReadMarks[botId]
        let task = Task {
            await before?.value
            return try await op()
        }
        let settled = Task { _ = await task.result }
        sendingReadMarks[botId] = settled
        Task {
            await settled.value
            guard sendingReadMarks[botId] == settled else { return }
            sendingReadMarks[botId] = nil
            beforeReads[botId] = nil
        }
        return task
    }
}
