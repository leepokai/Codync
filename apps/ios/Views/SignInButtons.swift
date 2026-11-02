import CodyncKit
import CodyncUI
import SwiftUI

/// Equal access to every account provider at every sign-in entry point.
struct SignInButtons: View {
    var body: some View {
        VStack(spacing: 10) {
            ProviderButton(provider: .apple, title: "Continue with Apple") {
                Image(systemName: "apple.logo").font(.system(size: 20))
            }
            .buttonStyle(.primary)
            ProviderButton(provider: .google, title: "Continue with Google") {
                Image("google").resizable().frame(width: 20, height: 20)
            }
            .buttonStyle(.secondary)
            ProviderButton(provider: .github, title: "Continue with GitHub") {
                Image("github").resizable().frame(width: 20, height: 20)
            }
            .buttonStyle(.secondary)
        }
    }
}

/// One provider's full-width button: its mark and line, a spinner while a sign-in runs (styled by the caller).
private struct ProviderButton<Mark: View>: View {
    @Environment(AccountSession.self) private var account
    let provider: AnalyticsEvent.SignInProvider
    let title: String
    @ViewBuilder let mark: Mark

    var body: some View {
        Button { Task { await account.signIn(provider: provider) } } label: {
            ZStack {
                HStack(spacing: 10) {
                    mark
                    Text(L10n.tr(title))
                }
                .opacity(account.isBusy ? 0 : 1)
                if account.isBusy { Spinner(size: 18) }
            }
            .font(.headline)
            .frame(maxWidth: .infinity, minHeight: 50)
        }
        .disabled(account.isBusy)
        .animation(Motion.fade, value: account.isBusy)
    }
}
