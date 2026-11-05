import CodyncKit
import StoreKit
import SwiftUI

extension View {
    func reviewInvitation() -> some View {
        modifier(ReviewInvitationModifier())
    }
}

/// Asks StoreKit once the prompt that made the invitation due has landed in its chat.
private struct ReviewInvitationModifier: ViewModifier {
    @Environment(\.requestReview) private var requestReview
    private let invitation = ReviewInvitation.shared

    func body(content: Content) -> some View {
        content.task(id: invitation.opportunity) {
            guard let token = invitation.opportunity else { return }
            // Every exit drops it, so a rebuilt root view (account switch, Start over) never asks without a new prompt.
            defer { invitation.skip(token) }
            do { try await Task.sleep(for: .seconds(1)) } catch { return }
            // The live state: an environment scenePhase here would still be the one this task began with.
            if UIApplication.shared.applicationState == .active, invitation.request(token) { requestReview() }
        }
    }
}
