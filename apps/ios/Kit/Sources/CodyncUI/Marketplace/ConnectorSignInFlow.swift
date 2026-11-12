import AuthenticationServices
import CodyncKit
import SwiftUI

/// Signs in to a remote connector (host/src/market/oauth.rs). From the phone, the system sign-in sheet
/// catches the return (`codync://oauth?…`) and hands the code to the host; on the computer itself
/// the browser returns to the host directly and we wait for the connector to flip to signed in.
/// A sign-in with a code (GitHub) shows the code first (`codePrompt`), then waits the same way.
/// A view keeps one as a property and presents its code with `.signInCodeDialog(signIn.codePrompt)`.
@MainActor
struct ConnectorSignInFlow: DynamicProperty {
    @Environment(BotStore.self) private var model
    @Environment(\.webAuthenticationSession) private var webAuthenticationSession
    @Environment(\.openURL) private var openURL
    @State var codePrompt = SignInCodePrompt()

    /// `false` when the user cancels, so the caller stops there too.
    @discardableResult
    func signIn(_ id: String) async throws -> Bool {
        let plan = try await model.connectorSignIn(id)
        guard let url = URL(string: plan.url) else { return false }
        if let code = plan.code {
            guard await codePrompt.ask(code) else { return false }
            Pasteboard.copy(code)
        }
        guard plan.callback == "app" else {
            openURL(url)
            try await model.waitForSignIn(id)
            return true
        }
        let back: URL
        do {
            back = try await webAuthenticationSession.authenticate(using: url, callbackURLScheme: AppEnvironment.current.urlScheme, preferredBrowserSession: .shared)
        } catch let error as ASWebAuthenticationSessionError where error.code == .canceledLogin {
            return false
        }
        let items = URLComponents(url: back, resolvingAgainstBaseURL: false)?.queryItems ?? []
        func value(_ name: String) -> String? { items.first { $0.name == name }?.value }
        try await model.finishConnectorSignIn(
            state: value("state") ?? "",
            code: value("code"),
            error: value("error_description") ?? value("error"),
            iss: value("iss")
        )
        return true
    }
}
