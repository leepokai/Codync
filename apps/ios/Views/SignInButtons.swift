import CodyncKit
import CodyncUI
import SwiftUI

/// Equal access to every account provider at every sign-in entry point.
struct SignInButtons: View {
    @Environment(AccountSession.self) private var account

    var body: some View {
        VStack(spacing: 10) {
            Button { Task { await account.signIn(provider: .apple) } } label: {
                ZStack {
                    HStack(spacing: 10) {
                        Image(systemName: "apple.logo").font(.system(size: 20))
                        Text(L10n.tr("Continue with Apple"))
                    }
                    .opacity(account.isBusy ? 0 : 1)
                    if account.isBusy { Spinner(size: 18) }
                }
                .font(.headline)
                .frame(maxWidth: .infinity, minHeight: 50)
            }
            .buttonStyle(.primary)
            .disabled(account.isBusy)
            .animation(Motion.fade, value: account.isBusy)
            GoogleSignInButton()
            WeChatSignInButton()
        }
    }
}

/// WeChat's mark plus the one line it needs.
private struct WeChatSignInButton: View {
    @Environment(AccountSession.self) private var account

    var body: some View {
        Button { Task { await account.signIn(provider: .wechat) } } label: {
            ZStack {
                HStack(spacing: 10) {
                    Image("wechat").resizable().frame(width: 20, height: 20)
                    Text(L10n.tr("Continue with WeChat"))
                }
                .opacity(account.isBusy ? 0 : 1)
                if account.isBusy { Spinner(size: 18) }
            }
            .font(.headline)
            .frame(maxWidth: .infinity, minHeight: 50)
        }
        .buttonStyle(.secondary)
        .disabled(account.isBusy)
        .animation(Motion.fade, value: account.isBusy)
    }
}
