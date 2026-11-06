import CodyncKit
import CodyncUI
import StoreKit
import SwiftUI

extension View {
    func reviewInvitation(blocked: Bool) -> some View {
        modifier(ReviewInvitationModifier(blocked: blocked))
    }
}

/// Only the returned-to roster can present StoreKit; navigation/overlays cancel its one-shot delay.
private struct ReviewInvitationModifier: ViewModifier {
    let blocked: Bool
    @Environment(AppStore.self) private var app
    @Environment(AccountStore.self) private var accounts
    @Environment(\.scenePhase) private var phase
    @Environment(\.requestReview) private var requestReview
    private let invitation = ReviewInvitation.shared

    private var unavailable: Bool {
        blocked || invitation.isInterrupted || phase != .active || app.tab != .bots || accounts.selection != nil
            || app.showComputers || app.showUsage || app.marketplace != nil || app.account.showSwitcher
            || accounts.lastError != nil || accounts.computers.isEmpty
            || accounts.computers.contains { computer in
                guard let store = accounts.store(for: computer.id) else { return true }
                return store.connection != .online || store.mismatch != nil || store.lastError != nil
                    || store.screenRequest != nil || store.bots.values.contains { $0.isWorking || $0.status == "error" }
            }
    }

    func body(content: Content) -> some View {
        content
            .task(id: invitation.opportunity) {
                guard let token = invitation.opportunity else { return }
                guard !unavailable else { invitation.cancel(.unavailable); return }
                do { try await Task.sleep(for: .seconds(2)) } catch { return }
                guard !Task.isCancelled, !unavailable, presentationIsClear else {
                    invitation.cancel(.unavailable)
                    return
                }
                if invitation.request(token) { requestReview() }
            }
            .onChange(of: unavailable) { _, blocked in if blocked { invitation.cancel(.unavailable) } }
            .onDisappear { invitation.cancel(.interaction) }
            .simultaneousGesture(DragGesture(minimumDistance: 0).onChanged { _ in invitation.cancel(.interaction) })
    }

    private var presentationIsClear: Bool {
        guard let scene = UIApplication.shared.connectedScenes.compactMap({ $0 as? UIWindowScene })
            .first(where: { $0.activationState == .foregroundActive }),
              let window = scene.windows.first(where: \.isKeyWindow),
              let root = window.rootViewController else { return false }
        return root.presentedViewController == nil
    }
}
