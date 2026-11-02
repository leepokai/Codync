import SwiftUI

/// App-owned authentication intents used by the shared icon editor.
public struct CustomIconAccount: Sendable {
    public let signedIn: Bool
    public let loading: Bool
    public let error: String?
    let signInApple: @MainActor @Sendable () -> Void
    let signInGoogle: @MainActor @Sendable () -> Void

    public init(signedIn: Bool, loading: Bool, error: String?,
                signInApple: @escaping @MainActor @Sendable () -> Void,
                signInGoogle: @escaping @MainActor @Sendable () -> Void) {
        self.signedIn = signedIn
        self.loading = loading
        self.error = error
        self.signInApple = signInApple
        self.signInGoogle = signInGoogle
    }
}

public extension EnvironmentValues {
    @Entry var customIconAccount: CustomIconAccount?
}
