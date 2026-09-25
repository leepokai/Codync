import AuthenticationServices
import ClerkKit
import CodyncKit
import Foundation
import Observation

/// Clerk owns OAuth, session restoration and Keychain persistence. An account session
/// never grants access to a computer by itself: each one still approves this device (spec §4.2).
@MainActor
@Observable
final class AccountSession {
    private let clerk: Clerk?
    private(set) var isBusy = false
    var errorMessage: String?

    var showSwitcher = false

    struct Account: Identifiable {
        let id: String
        let sessionID: String
        let email: String
        let avatarURL: URL?
    }

    var userID: String? { clerk?.session?.status == .active ? clerk?.user?.id : nil }
    var accounts: [Account] {
        var seen = Set<String>()
        return (clerk?.auth.sessions ?? []).compactMap { session in
            guard session.status == .active, session.expireAt > .now,
                  let user = session.user, seen.insert(user.id).inserted else { return nil }
            return Account(id: user.id, sessionID: session.id,
                           email: user.primaryEmailAddress?.emailAddress ?? "Codync account",
                           avatarURL: URL(string: user.imageUrl).flatMap { $0.scheme == "https" ? $0 : nil })
        }
    }
    var supportsMultipleAccounts: Bool { clerk?.environment?.authConfig.singleSessionMode == false }

    var isConfigured: Bool { clerk != nil }
    /// The Codync cloud (accounts and the encrypted relay); nil in builds without one.
    let cloudURL: URL?
    var isSignedIn: Bool { userID != nil }
    var email: String? { clerk?.user?.primaryEmailAddress?.emailAddress }
    var avatarURL: URL? {
        guard let image = clerk?.user?.imageUrl, let url = URL(string: image), url.scheme == "https" else { return nil }
        return url
    }

    init() {
        let env = ProcessInfo.processInfo.environment
        let config = Bundle.main.url(forResource: "AccountConfig", withExtension: "plist")
            .flatMap { try? Data(contentsOf: $0) }
            .flatMap { try? PropertyListDecoder().decode(Configuration.self, from: $0) }
        cloudURL = (env["CODYNC_CLOUD_URL"] ?? config?.cloudURL)
            .flatMap { URL(string: $0.trimmingCharacters(in: .whitespacesAndNewlines)) }
            .flatMap { Pairing.isCloudURL($0) ? $0 : nil }
        let key = (env["CODYNC_CLERK_PUBLISHABLE_KEY"] ?? config?.clerkPublishableKey ?? "").trimmingCharacters(in: .whitespacesAndNewlines)
        guard key.hasPrefix("pk_test_") || key.hasPrefix("pk_live_") else {
            clerk = nil
            return
        }
        let scheme = Bundle.main.bundleIdentifier ?? "com.pokai.Codync"
        clerk = Clerk.configure(publishableKey: key, options: .init(
            telemetryEnabled: false,
            redirectConfig: .init(redirectUrl: "\(scheme)://callback", callbackUrlScheme: scheme)
        ))
    }

    /// The signed-in session's JWT for the Codync cloud (Clerk caches it for a minute).
    func sessionToken() async throws -> String {
        guard let clerk, isSignedIn, let token = try await clerk.auth.getToken() else { throw SessionError.signedOut }
        return token
    }

    /// The Codync cloud for `userID`'s context; nil when signed out or the build has no cloud.
    /// Requests stop working once another account becomes active, so one account's calls never carry another's token.
    func cloudClient(for userID: String?, identity: DeviceIdentity?) -> CloudClient? {
        guard let userID, let cloudURL, isConfigured else { return nil }
        return CloudClient(baseURL: cloudURL, identity: identity) { [weak self] in
            guard let self, await self.userID == userID else { throw SessionError.signedOut }
            return try await self.sessionToken()
        }
    }

    enum SessionError: LocalizedError {
        case signedOut
        var errorDescription: String? { "Sign in again to reach your account." }
    }

    func signIn() async {
        guard !isBusy else { return }
        guard let clerk else {
            errorMessage = "Sign-in setup isn't finished yet. You can continue using local pairing."
            return
        }
        isBusy = true
        errorMessage = nil
        defer { isBusy = false }
        do {
            // Start Google directly; Clerk transfers new users into sign-up.
            let result = try await clerk.auth.signInWithOAuth(provider: .google, prefersEphemeralWebBrowserSession: isSignedIn)
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
        guard !isBusy, let clerk, let sessionID = clerk.session?.id else { return }
        isBusy = true
        errorMessage = nil
        defer { isBusy = false }
        do {
            try await clerk.auth.signOut(sessionId: sessionID)
        } catch {
            errorMessage = "Couldn't sign out. Check your connection and try again."
        }
    }

    func switchAccount(_ account: Account) async {
        guard !isBusy, let clerk, account.id != userID else { return }
        isBusy = true
        errorMessage = nil
        defer { isBusy = false }
        do {
            try await clerk.auth.setActive(sessionId: account.sessionID)
        } catch {
            errorMessage = "Couldn't switch accounts. Check your connection or sign in again."
        }
    }

    func handle(_ url: URL) async {
        guard let clerk else { return }
        do { try await clerk.handle(url) }
        catch { errorMessage = "Couldn't finish signing in. Please try again." }
    }

    /// `AccountConfig.plist` (`CODYNC_CLERK_PUBLISHABLE_KEY` / `CODYNC_CLOUD_URL` override it).
    private struct Configuration: Decodable {
        let clerkPublishableKey: String?
        let cloudURL: String?
    }
}
