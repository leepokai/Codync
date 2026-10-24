import CodyncKit
import Foundation

extension BotStore {
    /// A call starts on this device: what it carries stays out of the chat until its line arrives.
    func beginCall(_ call: String) {
        liveCalls.insert(call)
    }

    /// A finished call's line, with the rest of its transcript; the chat then shows the call's messages behind it.
    func endCall(_ botId: String, seconds: Int, call: String, lines: [CallLine]) async {
        do {
            let line = try await withLink(replay: true) {
                try await $0.endCall(botId: botId, call: call, seconds: seconds, lines: lines)
            }
            if let line { upsert(line) }
        } catch {
            lastError = error.localizedDescription
        }
        liveCalls.remove(call)
    }
}
