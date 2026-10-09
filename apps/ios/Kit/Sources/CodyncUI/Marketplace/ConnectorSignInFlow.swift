import AuthenticationServices
import CodyncKit
import SwiftUI

/// Signs in to a remote connector (host/src/market/oauth.rs). From the phone, the system sign-in sheet
/// catches the return (`codync://oauth?…`) and hands the code to the host; on the computer itself
/// the browser returns to the host directly and we wait for the connector to flip to signed in.
@MainActor
struct ConnectorSignInFlow {
    let model: BotStore
    let webAuthenticationSession: WebAuthenticationSession
    let openURL: OpenURLAction

    /// Returns quietly when the user cancels.
    func signIn(_ id: String) async throws {
        let plan = try await model.connectorSignIn(id)
        guard let url = URL(string: plan.url) else { return }
        guard plan.callback == "app" else {
            openURL(url)
            try await model.waitForSignIn(id)
            return
        }
        let back: URL
        do {
            back = try await webAuthenticationSession.authenticate(using: url, callbackURLScheme: AppEnvironment.current.urlScheme, preferredBrowserSession: .shared)
        } catch let error as ASWebAuthenticationSessionError where error.code == .canceledLogin {
            return
        }
        let items = URLComponents(url: back, resolvingAgainstBaseURL: false)?.queryItems ?? []
        func value(_ name: String) -> String? { items.first { $0.name == name }?.value }
        try await model.finishConnectorSignIn(
            state: value("state") ?? "",
            code: value("code"),
            error: value("error_description") ?? value("error")
        )
    }
}
