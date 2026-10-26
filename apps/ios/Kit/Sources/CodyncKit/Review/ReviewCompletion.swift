import Foundation

/// Uses host-owned turn IDs/final flags; never infers satisfaction from message content.
enum ReviewCompletion {
    static func match(entries: [Entry], sent: [String: Date], now: Date) -> [String]? {
        let chat = entries.filter { $0.isChat && $0.data.call == nil }
        guard let final = chat.last, final.kind == "agent", final.data.final == true, final.turn > 0 else { return nil }
        let turn = entries.filter { $0.turn == final.turn }
        // Non-successful completions and stops produce notices; be conservative about all notices.
        guard !turn.contains(where: { $0.kind == "notice" || $0.data.status == "failed" || $0.data.status == "cancelled" }) else { return nil }
        let nonces = turn.compactMap { entry -> String? in
            guard entry.kind == "user", entry.data.status == "sent", entry.data.call == nil,
                  entry.data.routineId == nil, entry.data.from == nil,
                  let nonce = entry.data.clientNonce, let date = sent[nonce],
                  now.timeIntervalSince(date) < 30 * ReviewHistory.day else { return nil }
            return nonce
        }
        return nonces.isEmpty ? nil : nonces
    }
}
