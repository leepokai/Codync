import CodyncKit
import CodyncUI
import SwiftUI

struct AccountSwitcherButton: View {
    @Environment(AccountSession.self) private var account

    var body: some View {
        Button { account.showSwitcher = true } label: {
            HStack(spacing: 4) {
                AccountAvatar(url: account.avatarURL, size: 30)
                Image(systemName: "chevron.down")
                    .font(.system(size: 10, weight: .semibold))
                    .foregroundStyle(Palette.secondary)
            }
        }
        .accessibilityLabel("Switch account")
        .accessibilityValue(account.email ?? "Local pairing")
    }
}

struct AccountSwitcherView: View {
    @Environment(AppStore.self) private var app
    @Environment(AccountSession.self) private var account
    @State private var showSettings = false

    var body: some View {
        CardForm {
            CardSection("Accounts") {
                if account.accounts.isEmpty {
                    Label("Local pairing", systemImage: "person.crop.circle")
                        .foregroundStyle(Palette.secondary)
                }
                ForEach(account.accounts) { item in
                    Button {
                        Task { await account.switchAccount(item) }
                    } label: {
                        HStack(spacing: 12) {
                            AccountAvatar(url: item.avatarURL, size: 34)
                            Text(item.email)
                                .lineLimit(1)
                                .foregroundStyle(Palette.text)
                            Spacer(minLength: 8)
                            if item.id == account.userID {
                                Image(systemName: "checkmark")
                                    .foregroundStyle(Palette.text)
                                    .accessibilityLabel("Current account")
                            }
                        }
                        .contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
                    .disabled(account.isBusy)
                }
                if !account.isSignedIn || account.supportsMultipleAccounts {
                    Button {
                        Task { await account.signIn() }
                    } label: {
                        Label(account.isSignedIn ? "Add Google account" : "Continue with Google", systemImage: "person.crop.circle.badge.plus")
                            .foregroundStyle(Palette.text)
                            .frame(maxWidth: .infinity, alignment: .leading)
                            .contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
                    .disabled(account.isBusy || !account.isConfigured)
                }
            }

            CardSection(footer: "Computers are kept separately for each account on this iPhone. Signed in, you also see the computers added to your account.") {
                Button { showSettings = true } label: {
                    HStack {
                        Label("Computers & settings", systemImage: "desktopcomputer").foregroundStyle(Palette.text)
                        Spacer(minLength: 8)
                        Image(systemName: "chevron.right").font(.caption.weight(.semibold)).foregroundStyle(Palette.tertiary)
                    }
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
            }

            if account.isSignedIn {
                CardSection(footer: "Signing out removes this account's computers and keys from this iPhone."
                            + (account.supportsMultipleAccounts ? "" : " Sign out to use a different Google account.")) {
                    Button {
                        Task { await app.signOut() }
                    } label: {
                        Text("Sign out of this account")
                            .foregroundStyle(Palette.danger)
                            .frame(maxWidth: .infinity, alignment: .leading)
                            .contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
                    .disabled(account.isBusy)
                }
            }

            if !account.isConfigured {
                CardSection {
                    Text("Account sign-in isn't configured in this build. Pairing with a code works without it.")
                        .foregroundStyle(Palette.secondary)
                }
            }
            if let message = account.errorMessage {
                CardSection {
                    Text(message).foregroundStyle(Palette.danger)
                }
            }
        }
        .safeAreaInset(edge: .top, spacing: 0) {
            ModalHeader("Accounts") {
                if account.isBusy {
                    ThinkingOrb(state: .connecting, size: 20)
                        .accessibilityHidden(false).accessibilityLabel("Signing in")
                        .transition(.opacity)
                }
            }
            .background(Palette.background)
            .animation(Motion.fade, value: account.isBusy)
        }
        .hidesSystemNavigationBar()
        .navigationDestination(isPresented: $showSettings) { SettingsView(pushed: true) }
    }
}

private struct AccountAvatar: View {
    let url: URL?
    let size: CGFloat

    var body: some View {
        AsyncImage(url: url) { image in
            image.resizable().scaledToFill()
        } placeholder: {
            Image(systemName: "person.crop.circle.fill")
                .resizable()
                .foregroundStyle(Palette.secondary)
        }
        .frame(width: size, height: size)
        .clipShape(Circle())
        .accessibilityHidden(true)
    }
}
