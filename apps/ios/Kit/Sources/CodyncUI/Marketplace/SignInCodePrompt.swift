import SwiftUI

/// The code a sign-in with a code (GitHub) asks the user to paste, shown before its page opens.
/// `ConnectorSignInFlow` awaits it; the view that started the sign-in presents it with `signInCodeDialog`.
@MainActor @Observable
final class SignInCodePrompt {
    /// The code on screen.
    private(set) var code: String?
    @ObservationIgnored private var reply: CheckedContinuation<Bool, Never>?
    /// Counts the asks, so a late "no" can't answer a newer one.
    @ObservationIgnored private var asks = 0

    /// `true` once the user wants the code copied and the page opened.
    func ask(_ code: String) async -> Bool {
        settle(false)
        asks += 1
        self.code = code
        return await withCheckedContinuation { reply = $0 }
    }

    func open() {
        settle(true)
    }

    /// Cancel, or the dialog fading out after `open`. Answers on the next turn, after the action ran.
    func dismissed() {
        code = nil
        let asked = asks
        Task { if self.asks == asked { self.settle(false) } }
    }

    private func settle(_ open: Bool) {
        reply?.resume(returning: open)
        reply = nil
    }
}

extension View {
    /// Shows `prompt`'s code over this view; a view that goes away answers no.
    func signInCodeDialog(_ prompt: SignInCodePrompt) -> some View {
        codyncDialog(
            prompt.code ?? "",
            isPresented: Binding(get: { prompt.code != nil }, set: { if !$0 { prompt.dismissed() } }),
            message: "Paste this code on the sign-in page that opens next."
        ) {
            [DialogAction("Copy and open") { prompt.open() }]
        }
        .onDisappear { prompt.dismissed() }
    }
}
