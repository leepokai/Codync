import CodyncKit
import SwiftUI

extension View {
    func reviewReading(botId: String, unobscured: Bool) -> some View {
        modifier(ReviewReading(botId: botId, unobscured: unobscured))
    }
}

private struct ReviewReading: ViewModifier {
    let botId: String
    let unobscured: Bool
    @Environment(BotStore.self) private var model
    @Environment(\.scenePhase) private var phase

    private struct Reading: Equatable {
        let entries: [Entry]
        let ready: Bool
    }

    func body(content: Content) -> some View {
        let reading = Reading(entries: model.chat(botId), ready: unobscured && !ReviewInvitation.shared.isInterrupted && phase == .active
                              && model.connection == .online && model.mismatch == nil && model.lastError == nil
                              && model.screenRequest == nil && model.bots[botId]?.status == "idle")
        content.task(id: reading) {
            guard reading.ready else { return }
            do { try await Task.sleep(for: .seconds(3)) } catch { return }
            guard !Task.isCancelled else { return }
            ReviewInvitation.shared.viewed(chat: "\(model.storage.id)/\(model.computer.id)/\(botId)", entries: reading.entries)
        }
    }
}
