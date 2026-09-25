import AuthenticationServices
import ClerkKit
import Foundation
import Observation

/// Clerk owns OAuth, session restoration and Keychain persistence. Host pairing
/// remains separate: an account session never grants access to a local host.
@MainActor
@Observable
final class AccountSession {
    private let clerk: Clerk?
    private(set) var isBusy = false
    var errorMessage: String?

    var isConfigured: Bool { clerk != nil }
    var isSignedIn: Bool { clerk?.user != nil }
    var email: String? { clerk?.user?.primaryEmailAddress?.emailAddress }
    var avatarURL: URL? {
        guard let image = clerk?.user?.imageUrl, let url = URL(string: image), url.scheme == "https" else { return nil }
        return url
    }

    init() {
        let environmentKey = ProcessInfo.processInfo.environment["CODYNC_CLERK_PUBLISHABLE_KEY"]
        let config = Bundle.main.url(forResource: "ClerkConfig", withExtension: "plist")
            .flatMap { try? Data(contentsOf: $0) }
            .flatMap { try? PropertyListDecoder().decode(Configuration.self, from: $0) }
        let key = (environmentKey ?? config?.publishableKey ?? "").trimmingCharacters(in: .whitespacesAndNewlines)
        guard key.hasPrefix("pk_test_") || key.hasPrefix("pk_live_") else {
            clerk = nil
            return
        }
        clerk = Clerk.configure(publishableKey: key, options: .init(
            telemetryEnabled: false,
            redirectConfig: .init(redirectUrl: "com.pokai.Codync://callback", callbackUrlScheme: "com.pokai.Codync")
        ))
    }

    func signIn() async {
        guard !isBusy else { return }
        guard let clerk else {
            errorMessage = "Sign-in setup isn't finished yet. You can continue using Codync on this Mac."
            return
        }
        isBusy = true
        errorMessage = nil
        defer { isBusy = false }
        do {
            // Start Google directly; Clerk transfers new users into sign-up.
            let result = try await clerk.auth.signInWithOAuth(provider: .google)
            switch result {
            case .signIn(let signIn):
                if signIn.status != .complete {
                    errorMessage = "Google sign-in needs additional verification. Your account is not signed in yet."
                }
            case .signUp(let signUp):
                if signUp.status != .complete {
                    errorMessage = "Google sign-up needs additional account information. Your account is not signed in yet."
                }
            }
        } catch let error as ASWebAuthenticationSessionError where error.code == .canceledLogin {
            return
        } catch is CancellationError {
            return
        } catch {
            errorMessage = "Couldn't sign in. Check your connection and try again."
        }
    }

    func signOut() async {
        guard !isBusy, let clerk else { return }
        isBusy = true
        errorMessage = nil
        defer { isBusy = false }
        do {
            try await clerk.auth.signOut()
        } catch {
            errorMessage = "Couldn't sign out. Check your connection and try again."
        }
    }

    private struct Configuration: Decodable {
        let publishableKey: String
    }
}
